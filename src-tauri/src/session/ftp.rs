//! FTP/FTPS implementation of [`RemoteFs`] (SPEC §4.3).
//!
//! FTP allows one transfer per control connection, so a pool of connections
//! backs parallel transfers. Metadata operations check a connection out and
//! return it; transfer streams own their connection until finalized.
//!
//! Recursive directory operations are implemented *above* this module through
//! the RemoteFs trait — they must always work (the founding pain point).

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::future::BoxFuture;
use futures::FutureExt;
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::types::FileType;
use suppaftp::Mode;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, Semaphore};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{
    join_remote, parent_remote, BoxRead, BoxWrite, RemoteEntry, RemoteFs,
};
use crate::vault::model::{Connection, FtpTlsMode};

type FtpConn = AsyncRustlsFtpStream;

pub struct FtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Zeroizing<String>,
    pub tls: FtpTlsMode,
    pub passive: bool,
}

impl FtpConfig {
    pub fn from_connection(conn: &Connection) -> AppResult<Self> {
        let ftp = conn.ftp.clone().unwrap_or_default();
        Ok(FtpConfig {
            host: conn.host.clone(),
            port: conn.port,
            username: conn.auth.username.clone(),
            password: Zeroizing::new(conn.auth.password.clone().unwrap_or_default()),
            tls: ftp.tls,
            passive: ftp.passive,
        })
    }
}

pub struct FtpPool {
    config: FtpConfig,
    idle: AsyncMutex<Vec<FtpConn>>,
    /// Bounds total simultaneous connections to the server.
    limit: Arc<Semaphore>,
}

fn ftp_err(op: &str, e: suppaftp::FtpError) -> AppError {
    AppError::RemoteFs(format!("{op}: {e}"))
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
    pub fn new(config: FtpConfig, max_connections: usize) -> Arc<FtpPool> {
        Arc::new(FtpPool {
            config,
            idle: AsyncMutex::new(Vec::new()),
            limit: Arc::new(Semaphore::new(max_connections.max(2))),
        })
    }

    async fn connect_one(&self) -> AppResult<FtpConn> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let mut ftp = FtpConn::connect(&addr)
            .await
            .map_err(|e| AppError::Connect(format!("{addr}: {e}")))?;
        if self.config.tls == FtpTlsMode::Explicit {
            // "FTPS required": no plaintext fallback, ever (SPEC §4.3).
            let connector = tokio_rustls::TlsConnector::from(rustls_config());
            ftp = ftp
                .into_secure(AsyncRustlsConnector::from(connector), &self.config.host)
                .await
                .map_err(|e| AppError::Connect(format!("AUTH TLS: {e}")))?;
        }
        ftp.login(self.config.username.as_str(), self.config.password.as_str())
            .await
            .map_err(|e| AppError::Auth(format!("FTP login: {e}")))?;
        ftp.transfer_type(FileType::Binary)
            .await
            .map_err(|e| ftp_err("TYPE I", e))?;
        ftp.set_mode(if self.config.passive {
            Mode::Passive
        } else {
            Mode::Active
        });
        Ok(ftp)
    }

    /// Check out a connection (idle reuse or fresh dial).
    async fn checkout(&self) -> AppResult<PooledConn> {
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

    async fn give_back(&self, conn: FtpConn) {
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
struct PooledConn {
    conn: Option<FtpConn>,
    /// Held for its Drop: frees the pool slot when the connection dies.
    _permit: OwnedSemaphorePermit,
}

/// LIST output entry → RemoteEntry.
fn parse_entry(dir: &str, line: &str) -> Option<RemoteEntry> {
    let file = suppaftp::list::File::try_from(line).ok()?;
    let name = file.name().to_string();
    if name == "." || name == ".." {
        return None;
    }
    let mtime = file
        .modified()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64);
    // POSIX pex bits when the server sends a unix-style listing.
    let mode = {
        let mut m = 0u32;
        let pex = [
            (file.can_read(suppaftp::list::PosixPexQuery::Owner), 0o400),
            (file.can_write(suppaftp::list::PosixPexQuery::Owner), 0o200),
            (
                file.can_execute(suppaftp::list::PosixPexQuery::Owner),
                0o100,
            ),
            (file.can_read(suppaftp::list::PosixPexQuery::Group), 0o040),
            (file.can_write(suppaftp::list::PosixPexQuery::Group), 0o020),
            (
                file.can_execute(suppaftp::list::PosixPexQuery::Group),
                0o010,
            ),
            (file.can_read(suppaftp::list::PosixPexQuery::Others), 0o004),
            (file.can_write(suppaftp::list::PosixPexQuery::Others), 0o002),
            (
                file.can_execute(suppaftp::list::PosixPexQuery::Others),
                0o001,
            ),
        ];
        for (has, bit) in pex {
            if has {
                m |= bit;
            }
        }
        m
    };
    Some(RemoteEntry {
        path: join_remote(dir, &name),
        is_dir: file.is_directory(),
        is_symlink: file.is_symlink(),
        size: file.size() as u64,
        mtime,
        permissions: Some(mode),
        name,
    })
}

#[async_trait::async_trait]
impl RemoteFs for FtpPool {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let lines = conn.list(Some(path)).await.map_err(|e| ftp_err(path, e))?;
        let entries = lines
            .iter()
            .filter_map(|line| parse_entry(path, line))
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
            .find(|e| e.name == name)
            .ok_or_else(|| AppError::RemoteFs(format!("{path}: not found")))
    }

    async fn home_dir(&self) -> AppResult<String> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let home = conn.pwd().await.map_err(|e| ftp_err("PWD", e))?;
        let conn = pooled.conn.take().unwrap();
        self.give_back(conn).await;
        Ok(home)
    }

    async fn mkdir(&self, path: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn.mkdir(path).await.map_err(|e| ftp_err(path, e));
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
            .map_err(|e| AppError::RemoteFs(format!("{path}: {e}")))?;
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn.rename(from, to).await.map_err(|e| ftp_err(from, e));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn delete_file(&self, path: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn.rm(path).await.map_err(|e| ftp_err(path, e));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn delete_dir(&self, path: &str) -> AppResult<()> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        let result = conn.rmdir(path).await.map_err(|e| ftp_err(path, e));
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
            .map_err(|e| ftp_err("SITE CHMOD", e));
        if result.is_ok() {
            let conn = pooled.conn.take().unwrap();
            self.give_back(conn).await;
        }
        result
    }

    async fn set_mtime(&self, path: &str, mtime_unix: i64) -> AppResult<()> {
        // MFMT is a common extension; best-effort (SPEC §6.1 mtime option).
        let Some(dt) = chrono::DateTime::from_timestamp(mtime_unix, 0) else {
            return Ok(());
        };
        let stamp = dt.format("%Y%m%d%H%M%S").to_string();
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
                .map_err(|e| ftp_err("REST", e))?;
        }
        let stream = conn
            .retr_as_stream(path)
            .await
            .map_err(|e| ftp_err(path, e))?;
        Ok(Box::new(FtpReader {
            inner: Some((pooled, stream)),
        }))
    }

    async fn open_write(&self, path: &str, offset: u64) -> AppResult<BoxWrite> {
        let mut pooled = self.checkout().await?;
        let conn = pooled.conn.as_mut().unwrap();
        if offset > 0 {
            conn.resume_transfer(offset as usize)
                .await
                .map_err(|e| ftp_err("REST", e))?;
        }
        let stream = conn
            .put_with_stream(path)
            .await
            .map_err(|e| ftp_err(path, e))?;
        Ok(Box::new(FtpWriter {
            state: WriterState::Writing(Box::new((pooled, stream))),
        }))
    }

    async fn exists(&self, path: &str) -> AppResult<bool> {
        match self.stat(path).await {
            Ok(_) => Ok(true),
            Err(AppError::RemoteFs(msg)) if msg.contains("not found") => Ok(false),
            Err(e) => Err(e),
        }
    }
}

type DataStream = suppaftp::tokio::AsyncDataStream<suppaftp::tokio::AsyncRustlsStream>;

/// Read stream owning its pooled connection. On EOF or drop the transfer is
/// finalized in a background task; a mid-stream drop closes the connection
/// (server aborts the transfer, the pool slot frees via the permit).
struct FtpReader {
    inner: Option<(PooledConn, DataStream)>,
}

impl AsyncRead for FtpReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let Some((_, stream)) = self.inner.as_mut() else {
            return Poll::Ready(Ok(()));
        };
        let before = buf.filled().len();
        match Pin::new(stream).poll_read(cx, buf) {
            Poll::Ready(Ok(())) if buf.filled().len() == before => {
                // EOF: acknowledge the 226 reply off-path.
                if let Some((mut pooled, stream)) = self.inner.take() {
                    tokio::spawn(async move {
                        if let Some(conn) = pooled.conn.as_mut() {
                            let _ = conn.finalize_retr_stream(stream).await;
                        }
                        drop(pooled); // connection not reused after a transfer
                    });
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

enum WriterState {
    Writing(Box<(PooledConn, DataStream)>),
    Finalizing(BoxFuture<'static, std::io::Result<()>>),
    Done,
}

/// Write stream owning its pooled connection; `shutdown()` finalizes the
/// transfer (waits for the server's 226).
struct FtpWriter {
    state: WriterState,
}

impl AsyncWrite for FtpWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut self.state {
            WriterState::Writing(inner) => Pin::new(&mut inner.1).poll_write(cx, buf),
            _ => Poll::Ready(Err(std::io::Error::other("write after shutdown"))),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut self.state {
            WriterState::Writing(inner) => Pin::new(&mut inner.1).poll_flush(cx),
            _ => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.state {
                WriterState::Writing(_) => {
                    let WriterState::Writing(inner) =
                        std::mem::replace(&mut self.state, WriterState::Done)
                    else {
                        unreachable!()
                    };
                    let (mut pooled, stream) = *inner;
                    self.state = WriterState::Finalizing(
                        async move {
                            let conn = pooled
                                .conn
                                .as_mut()
                                .ok_or_else(|| std::io::Error::other("connection gone"))?;
                            conn.finalize_put_stream(stream)
                                .await
                                .map_err(std::io::Error::other)?;
                            drop(pooled);
                            Ok(())
                        }
                        .boxed(),
                    );
                }
                WriterState::Finalizing(future) => {
                    let result = futures::ready!(future.as_mut().poll(cx));
                    self.state = WriterState::Done;
                    return Poll::Ready(result);
                }
                WriterState::Done => return Poll::Ready(Ok(())),
            }
        }
    }
}
