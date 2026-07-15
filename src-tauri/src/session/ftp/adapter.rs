use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{
    parent_remote, replace_file_via_backup, BoxRead, BoxWrite, RemoteEntry, RemoteFs,
};

use super::pool::ftp_err;
use super::{listing, streams, FtpPool};

#[async_trait::async_trait]
impl RemoteFs for FtpPool {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let lines = conn
            .list(Some(path))
            .await
            .map_err(|error| ftp_err(path, error))?;
        let entries = lines
            .iter()
            .filter_map(|line| listing::parse_entry(path, line))
            .collect();
        let conn = pooled.conn.take().unwrap();
        self.give_back(conn).await;
        Ok(entries)
    }

    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        // FTP has no reliable per-entry stat: list the parent and match.
        let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        let parent = parent_remote(path);
        if name.is_empty() {
            return Ok(RemoteEntry {
                name: "/".into(),
                path: "/".into(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                mtime: None,
                permissions: None,
            });
        }
        let entries = self.list(&parent).await?;
        entries
            .into_iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| AppError::RemoteFs(format!("{path}: not found")))
    }

    async fn home_dir(&self) -> AppResult<String> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let home = conn.pwd().await.map_err(|error| ftp_err("PWD", error))?;
        let conn = pooled.conn.take().unwrap();
        self.give_back(conn).await;
        Ok(home)
    }

    async fn mkdir(&self, path: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn.mkdir(path).await.map_err(|error| ftp_err(path, error));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn create_file(&self, path: &str) -> AppResult<()> {
        if self.exists(path).await? {
            return Err(AppError::RemoteFs(format!("{path}: already exists")));
        }
        let mut writer = self.open_write(path, 0).await?;
        use tokio::io::AsyncWriteExt;
        writer
            .shutdown()
            .await
            .map_err(|error| AppError::RemoteFs(format!("{path}: {error}")))?;
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn
            .rename(from, to)
            .await
            .map_err(|error| ftp_err(from, error));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn replace_file(&self, staged: &str, target: &str) -> AppResult<()> {
        // UNIX-like FTP servers often expose modes through LIST and support
        // SITE CHMOD. Preserve it when available, but do not make remote edit
        // depend on this optional FTP extension.
        if let Ok(entry) = self.stat(target).await {
            if let Some(mode) = entry.permissions {
                let _ = self.chmod(staged, mode).await;
            }
        }
        replace_file_via_backup(self, staged, target).await
    }

    async fn delete_file(&self, path: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn.rm(path).await.map_err(|error| ftp_err(path, error));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn delete_dir(&self, path: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn.rmdir(path).await.map_err(|error| ftp_err(path, error));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn chmod(&self, path: &str, mode: u32) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn
            .site(format!("CHMOD {:o} {}", mode & 0o7777, path))
            .await
            .map(|_| ())
            .map_err(|error| ftp_err("SITE CHMOD", error));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn set_mtime(&self, path: &str, mtime_unix: i64) -> AppResult<()> {
        // MFMT is a common extension; best-effort (SPEC §6.1 mtime option).
        let Some(datetime) = chrono::DateTime::from_timestamp(mtime_unix, 0) else {
            return Ok(());
        };
        let stamp = datetime.format("%Y%m%d%H%M%S").to_string();
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let _ = conn
            .custom_command(
                format!("MFMT {stamp} {path}"),
                &[suppaftp::Status::File, suppaftp::Status::CommandOk],
            )
            .await;
        let conn = pooled.conn.take().unwrap();
        self.give_back(conn).await;
        Ok(())
    }

    async fn open_read(&self, path: &str, offset: u64) -> AppResult<BoxRead> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        if offset > 0 {
            // Resume via REST (SPEC §6.1).
            conn.resume_transfer(offset as usize)
                .await
                .map_err(|error| ftp_err("REST", error))?;
        }
        let stream = conn
            .retr_as_stream(path)
            .await
            .map_err(|error| ftp_err(path, error))?;
        Ok(streams::reader(pooled, stream))
    }

    async fn open_write(&self, path: &str, offset: u64) -> AppResult<BoxWrite> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        if offset > 0 {
            conn.resume_transfer(offset as usize)
                .await
                .map_err(|error| ftp_err("REST", error))?;
        }
        let stream = conn
            .put_with_stream(path)
            .await
            .map_err(|error| ftp_err(path, error))?;
        Ok(streams::writer(pooled, stream))
    }

    async fn exists(&self, path: &str) -> AppResult<bool> {
        match self.stat(path).await {
            Ok(_) => Ok(true),
            Err(AppError::RemoteFs(message)) if message.contains("not found") => Ok(false),
            Err(error) => Err(error),
        }
    }
}
