use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::vault::model::Protocol;

use super::{ftp, remote_fs, s3, sftp, ssh};

/// A live protocol session registered under one frontend-visible identifier.
pub struct SessionEntry {
    pub id: String,
    pub connection_id: String,
    pub protocol: Protocol,
    /// Present for SSH sessions (terminals, SFTP and tunnels hang off it).
    pub ssh: Option<Arc<ssh::SshSession>>,
    /// Lazily opened SFTP subsystem over the same SSH session.
    pub(super) sftp: tokio::sync::OnceCell<Arc<sftp::SftpFs>>,
    /// Connection pool for FTP sessions.
    pub ftp: Option<Arc<ftp::FtpPool>>,
    /// S3 client for object-storage sessions (SPEC §4.4).
    pub s3: Option<Arc<s3::S3Fs>>,
    /// Whether the remote side has `tar` (probed once, SPEC §6.2).
    pub(super) tar_available: tokio::sync::OnceCell<bool>,
}

impl SessionEntry {
    pub(super) fn storage(
        id: String,
        connection_id: String,
        protocol: Protocol,
        ftp: Option<Arc<ftp::FtpPool>>,
        s3: Option<Arc<s3::S3Fs>>,
    ) -> Self {
        Self {
            id,
            connection_id,
            protocol,
            ssh: None,
            sftp: tokio::sync::OnceCell::new(),
            ftp,
            s3,
            tar_available: tokio::sync::OnceCell::new(),
        }
    }

    pub(super) fn ssh(id: String, connection_id: String, session: ssh::SshSession) -> Self {
        Self {
            id,
            connection_id,
            protocol: Protocol::Ssh,
            ssh: Some(Arc::new(session)),
            sftp: tokio::sync::OnceCell::new(),
            ftp: None,
            s3: None,
            tar_available: tokio::sync::OnceCell::new(),
        }
    }

    /// SSH handle + tar availability for accelerated dir transfers.
    pub async fn tar_ssh(&self) -> Option<Arc<ssh::SshSession>> {
        let ssh = self.ssh.clone()?;
        let available = self
            .tar_available
            .get_or_init(|| {
                let ssh = ssh.clone();
                async move {
                    ssh.exec_check("command -v tar >/dev/null 2>&1")
                        .await
                        .unwrap_or(false)
                }
            })
            .await;
        if *available {
            Some(ssh)
        } else {
            None
        }
    }

    /// The protocol-agnostic file backend for this session (SPEC §7.1).
    pub async fn remote_fs(&self) -> AppResult<Arc<dyn remote_fs::RemoteFs>> {
        match self.protocol {
            Protocol::Ssh => {
                let ssh = self
                    .ssh
                    .clone()
                    .ok_or_else(|| AppError::Other("missing ssh handle".into()))?;
                let fs = self
                    .sftp
                    .get_or_try_init(|| async move { sftp::SftpFs::open(&ssh).await.map(Arc::new) })
                    .await?;
                Ok(fs.clone())
            }
            Protocol::Ftp => self
                .ftp
                .clone()
                .map(|pool| pool as Arc<dyn remote_fs::RemoteFs>)
                .ok_or_else(|| AppError::Other("missing ftp pool".into())),
            Protocol::S3 => self
                .s3
                .clone()
                .map(|fs| fs as Arc<dyn remote_fs::RemoteFs>)
                .ok_or_else(|| AppError::Other("missing s3 client".into())),
        }
    }
}
