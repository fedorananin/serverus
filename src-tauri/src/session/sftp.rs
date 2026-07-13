//! SFTP implementation of [`RemoteFs`] over the multiplexed SSH session
//! (SPEC §4.1: files ride the same TCP/SSH connection as terminals).

use std::io::SeekFrom;
use std::sync::Arc;

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::io::AsyncSeekExt;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{join_remote, BoxRead, BoxWrite, RemoteEntry, RemoteFs};
use crate::session::ssh::SshSession;

pub struct SftpFs {
    sftp: Arc<SftpSession>,
}

impl SftpFs {
    /// Open the SFTP subsystem on a fresh channel of the existing session.
    pub async fn open(ssh: &SshSession) -> AppResult<SftpFs> {
        let channel = {
            let handle = ssh.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|e| AppError::RemoteFs(format!("sftp channel: {e}")))?
        };
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| AppError::RemoteFs(format!("sftp subsystem: {e}")))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| AppError::RemoteFs(format!("sftp init: {e}")))?;
        // The default per-request timeout is 10 s — too tight when parallel
        // transfers saturate the channel and metadata requests (stat/mkdir)
        // queue behind bulk data. Dead links are caught by SSH keepalive
        // (30 s × 3), so a generous request timeout costs nothing.
        sftp.set_timeout(120);
        Ok(SftpFs {
            sftp: Arc::new(sftp),
        })
    }
}

fn entry_from(path: String, name: String, attrs: &FileAttributes) -> RemoteEntry {
    RemoteEntry {
        name,
        path,
        is_dir: attrs.is_dir(),
        is_symlink: attrs.is_symlink(),
        size: attrs.size.unwrap_or(0),
        mtime: attrs.mtime.map(|t| t as i64),
        permissions: attrs.permissions.map(|p| p & 0o7777),
    }
}

fn map_err(op: &str, e: russh_sftp::client::error::Error) -> AppError {
    AppError::RemoteFs(format!("{op}: {e}"))
}

#[async_trait::async_trait]
impl RemoteFs for SftpFs {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let dir = self
            .sftp
            .read_dir(path)
            .await
            .map_err(|e| map_err(path, e))?;
        let mut out = Vec::new();
        for item in dir {
            let name = item.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let full = join_remote(path, &name);
            let attrs = item.metadata();
            let mut entry = entry_from(full.clone(), name, &attrs);
            // Symlinks: resolve the target type so the UI can navigate into
            // linked directories (SPEC §5.2).
            if entry.is_symlink {
                if let Ok(target) = self.sftp.metadata(&full).await {
                    entry.is_dir = target.is_dir();
                    entry.size = target.size.unwrap_or(entry.size);
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        let attrs = self
            .sftp
            .metadata(path)
            .await
            .map_err(|e| map_err(path, e))?;
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path);
        Ok(entry_from(path.to_string(), name.to_string(), &attrs))
    }

    async fn home_dir(&self) -> AppResult<String> {
        self.sftp
            .canonicalize(".")
            .await
            .map_err(|e| map_err("home", e))
    }

    async fn mkdir(&self, path: &str) -> AppResult<()> {
        self.sftp
            .create_dir(path)
            .await
            .map_err(|e| map_err(path, e))
    }

    async fn create_file(&self, path: &str) -> AppResult<()> {
        if self.exists(path).await? {
            return Err(AppError::RemoteFs(format!("{path}: already exists")));
        }
        let file = self.sftp.create(path).await.map_err(|e| map_err(path, e))?;
        drop(file);
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
        self.sftp
            .rename(from, to)
            .await
            .map_err(|e| map_err(from, e))
    }

    async fn delete_file(&self, path: &str) -> AppResult<()> {
        self.sftp
            .remove_file(path)
            .await
            .map_err(|e| map_err(path, e))
    }

    async fn delete_dir(&self, path: &str) -> AppResult<()> {
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|e| map_err(path, e))
    }

    async fn chmod(&self, path: &str, mode: u32) -> AppResult<()> {
        // FileAttributes::default() is NOT empty (it carries uid=0/gid=0 and
        // would chown to root) — build from empty() so only the mode is sent.
        let attrs = FileAttributes {
            permissions: Some(mode),
            ..FileAttributes::empty()
        };
        self.sftp
            .set_metadata(path, attrs)
            .await
            .map_err(|e| map_err(path, e))
    }

    async fn set_mtime(&self, path: &str, mtime_unix: i64) -> AppResult<()> {
        let attrs = FileAttributes {
            mtime: Some(mtime_unix as u32),
            atime: Some(mtime_unix as u32),
            ..FileAttributes::empty()
        };
        self.sftp
            .set_metadata(path, attrs)
            .await
            .map_err(|e| map_err(path, e))
    }

    async fn open_read(&self, path: &str, offset: u64) -> AppResult<BoxRead> {
        let mut file = self
            .sftp
            .open_with_flags(path, OpenFlags::READ)
            .await
            .map_err(|e| map_err(path, e))?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .await
                .map_err(|e| AppError::RemoteFs(format!("{path}: seek: {e}")))?;
        }
        Ok(Box::new(file))
    }

    async fn open_write(&self, path: &str, offset: u64) -> AppResult<BoxWrite> {
        let flags = if offset == 0 {
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
        } else {
            OpenFlags::WRITE | OpenFlags::CREATE
        };
        let mut file = self
            .sftp
            .open_with_flags(path, flags)
            .await
            .map_err(|e| map_err(path, e))?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))
                .await
                .map_err(|e| AppError::RemoteFs(format!("{path}: seek: {e}")))?;
        }
        Ok(Box::new(file))
    }

    async fn exists(&self, path: &str) -> AppResult<bool> {
        self.sftp
            .try_exists(path)
            .await
            .map_err(|e| map_err(path, e))
    }
}
