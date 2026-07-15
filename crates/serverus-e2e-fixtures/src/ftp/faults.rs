use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use async_trait::async_trait;
use tokio::io::{AsyncRead, ReadBuf};
use unftp_core::auth::DefaultUser;
use unftp_core::storage::{Fileinfo, Metadata, Result, StorageBackend};
use unftp_sbe_fs::{Filesystem, Meta};

pub(super) const FAULT_AFTER_BYTES: u64 = 64 * 1024;
const FAILED_RETRIEVALS: usize = 3;

#[derive(Debug)]
pub(super) struct RetrievalFaults {
    attempts: AtomicUsize,
    telemetry: Mutex<PathBuf>,
}

impl RetrievalFaults {
    pub(super) fn new(telemetry: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            attempts: AtomicUsize::new(0),
            telemetry: Mutex::new(telemetry),
        })
    }

    pub(super) fn fail_after(&self, path: &str, start_pos: u64) -> io::Result<Option<u64>> {
        if Path::new(path).file_name().and_then(|name| name.to_str()) != Some("resume.bin") {
            return Ok(None);
        }
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        let telemetry = self.telemetry.lock().unwrap();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&*telemetry)?;
        writeln!(file, "{{\"start_pos\":{start_pos}}}")?;
        Ok((attempt < FAILED_RETRIEVALS).then_some(FAULT_AFTER_BYTES))
    }

    pub(super) fn reject_edit_promotion(&self, from: &str, to: &str) -> bool {
        let from_name = Path::new(from).file_name().and_then(|name| name.to_str());
        let to_name = Path::new(to).file_name().and_then(|name| name.to_str());
        from_name.is_some_and(|name| name.starts_with(".serverus-edit-"))
            && to_name == Some("edit-failure.txt")
    }
}

pub(super) struct FailAfter<R> {
    inner: tokio::io::Take<R>,
}

impl<R: AsyncRead + Unpin> FailAfter<R> {
    pub(super) fn new(inner: R, limit: u64) -> Self {
        use tokio::io::AsyncReadExt;
        Self {
            inner: inner.take(limit),
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for FailAfter<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.inner.limit() == 0 {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "fixture interrupted retrieval",
            )));
        }
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}

#[derive(Debug)]
pub(super) struct FaultyFilesystem {
    inner: Filesystem,
    faults: Arc<RetrievalFaults>,
}

impl FaultyFilesystem {
    pub(super) fn new(root: PathBuf, faults: Arc<RetrievalFaults>) -> io::Result<Self> {
        Ok(Self {
            inner: Filesystem::new(root)?,
            faults,
        })
    }
}

#[async_trait]
impl StorageBackend<DefaultUser> for FaultyFilesystem {
    type Metadata = Meta;

    fn enter(&mut self, user: &DefaultUser) -> io::Result<()> {
        self.inner.enter(user)
    }

    fn supported_features(&self) -> u32 {
        <Filesystem as StorageBackend<DefaultUser>>::supported_features(&self.inner)
    }

    async fn metadata<P: AsRef<Path> + Send + Debug>(
        &self,
        user: &DefaultUser,
        path: P,
    ) -> Result<Self::Metadata> {
        self.inner.metadata(user, path).await
    }

    async fn list<P>(&self, user: &DefaultUser, path: P) -> Result<Vec<Fileinfo<PathBuf, Meta>>>
    where
        P: AsRef<Path> + Send + Debug,
        Self::Metadata: Metadata,
    {
        self.inner.list(user, path).await
    }

    async fn get<P: AsRef<Path> + Send + Debug>(
        &self,
        user: &DefaultUser,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>> {
        let path = path.as_ref().to_path_buf();
        let failure = self.faults.fail_after(&path.to_string_lossy(), start_pos)?;
        let reader = self.inner.get(user, path, start_pos).await?;
        Ok(match failure {
            Some(limit) => Box::new(FailAfter::new(reader, limit)),
            None => reader,
        })
    }

    async fn put<P, R>(&self, user: &DefaultUser, input: R, path: P, start_pos: u64) -> Result<u64>
    where
        P: AsRef<Path> + Send + Debug,
        R: AsyncRead + Send + Sync + Unpin + 'static,
    {
        self.inner.put(user, input, path, start_pos).await
    }

    async fn del<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
        self.inner.del(user, path).await
    }

    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
        self.inner.mkd(user, path).await
    }

    async fn rename<P: AsRef<Path> + Send + Debug>(
        &self,
        user: &DefaultUser,
        from: P,
        to: P,
    ) -> Result<()> {
        if self.faults.reject_edit_promotion(
            &from.as_ref().to_string_lossy(),
            &to.as_ref().to_string_lossy(),
        ) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture rejected remote-edit staging promotion",
            )
            .into());
        }
        self.inner.rename(user, from, to).await
    }

    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
        self.inner.rmd(user, path).await
    }

    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
        self.inner.cwd(user, path).await
    }
}
