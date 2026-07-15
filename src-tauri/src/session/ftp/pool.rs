use std::sync::Arc;

use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::types::FileType;
use suppaftp::Mode;
use tokio::sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, Semaphore};

use crate::error::{AppError, AppResult};
use crate::vault::model::FtpTlsMode;

use super::FtpConfig;

pub(super) type FtpConn = AsyncRustlsFtpStream;

pub struct FtpPool {
    config: FtpConfig,
    idle: AsyncMutex<Vec<FtpConn>>,
    /// Bounds total simultaneous connections to the server.
    limit: Arc<Semaphore>,
}

pub(super) fn ftp_err(op: &str, error: suppaftp::FtpError) -> AppError {
    AppError::RemoteFs(format!("{op}: {error}"))
}

fn rustls_config() -> Arc<rustls::ClientConfig> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    // Pin the ring provider explicitly: aws-sdk-s3 enables rustls' aws-lc
    // feature too, and with two providers in the crate graph the plain
    // `ClientConfig::builder()` panics at runtime.
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Arc::new(
        rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("ring supports default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

impl FtpPool {
    pub fn new(config: FtpConfig, max_connections: usize) -> Arc<Self> {
        Arc::new(Self {
            config,
            idle: AsyncMutex::new(Vec::new()),
            limit: Arc::new(Semaphore::new(max_connections.max(2))),
        })
    }

    async fn connect_one(&self) -> AppResult<FtpConn> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let mut ftp = FtpConn::connect(&addr)
            .await
            .map_err(|error| AppError::Connect(format!("{addr}: {error}")))?;
        if self.config.tls == FtpTlsMode::Explicit {
            // "FTPS required": no plaintext fallback, ever (SPEC §4.3).
            let connector = tokio_rustls::TlsConnector::from(rustls_config());
            ftp = ftp
                .into_secure(AsyncRustlsConnector::from(connector), &self.config.host)
                .await
                .map_err(|error| AppError::Connect(format!("AUTH TLS: {error}")))?;
        }
        ftp.login(self.config.username.as_str(), self.config.password.as_str())
            .await
            .map_err(|error| AppError::Auth(format!("FTP login: {error}")))?;
        ftp.transfer_type(FileType::Binary)
            .await
            .map_err(|error| ftp_err("TYPE I", error))?;
        ftp.set_mode(if self.config.passive {
            Mode::Passive
        } else {
            Mode::Active
        });
        Ok(ftp)
    }

    /// Check out a connection (idle reuse or fresh dial).
    pub(super) async fn checkout(&self) -> AppResult<PooledConn> {
        let permit = self
            .limit
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Other("pool closed".into()))?;
        let existing = self.idle.lock().await.pop();
        let conn = match existing {
            Some(mut conn) => {
                // Validate the idle connection before reuse.
                if conn.noop().await.is_ok() {
                    conn
                } else {
                    self.connect_one().await?
                }
            }
            None => self.connect_one().await?,
        };
        Ok(PooledConn {
            conn: Some(conn),
            _permit: permit,
        })
    }

    pub(super) async fn give_back(&self, conn: FtpConn) {
        self.idle.lock().await.push(conn);
    }

    /// Verify credentials/reachability once at session-connect time.
    pub async fn probe(&self) -> AppResult<()> {
        let mut checked_out = self.checkout().await?;
        let conn = checked_out.conn.take().unwrap();
        self.give_back(conn).await;
        Ok(())
    }
}

/// Checked-out connection; returns to nothing on drop (dropped connections
/// are simply closed — the semaphore permit frees the slot).
pub(super) struct PooledConn {
    pub(super) conn: Option<FtpConn>,
    /// Held for its Drop: frees the pool slot when the connection dies.
    _permit: OwnedSemaphorePermit,
}
