use std::collections::HashMap;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::io::AsyncWrite;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{
    replace_file_via_backup, BoxRead, BoxWrite, RemoteEntry, RemoteFs,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UploadFailure {
    None,
    Write,
    Finalize,
}

#[derive(Default)]
pub(super) struct FsState {
    pub(super) files: HashMap<String, Vec<u8>>,
    pub(super) open_write_paths: Vec<String>,
    pub(super) rename_calls: Vec<(String, String)>,
    pub(super) delete_calls: Vec<String>,
}

pub(super) struct RecordingFs {
    pub(super) state: Arc<Mutex<FsState>>,
    upload_failure: UploadFailure,
    fail_promote: bool,
    block_open_write: bool,
    pub(super) open_write_started: Arc<tokio::sync::Notify>,
}

impl RecordingFs {
    pub(super) fn new(upload_failure: UploadFailure, fail_promote: bool) -> Self {
        let mut state = FsState::default();
        state
            .files
            .insert("/dir/config.txt".into(), b"old".to_vec());
        Self {
            state: Arc::new(Mutex::new(state)),
            upload_failure,
            fail_promote,
            block_open_write: false,
            open_write_started: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub(super) fn with_blocked_upload() -> Self {
        Self {
            block_open_write: true,
            ..Self::new(UploadFailure::None, false)
        }
    }
}

struct RecordingWriter {
    state: Arc<Mutex<FsState>>,
    path: String,
    failure: UploadFailure,
}

impl AsyncWrite for RecordingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.failure == UploadFailure::Write {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "simulated interrupted upload",
            )));
        }
        self.state
            .lock()
            .unwrap()
            .files
            .entry(self.path.clone())
            .or_default()
            .extend_from_slice(buffer);
        Poll::Ready(Ok(buffer.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.failure == UploadFailure::Finalize {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "simulated finalize failure",
            )))
        } else {
            Poll::Ready(Ok(()))
        }
    }
}

#[async_trait::async_trait]
impl RemoteFs for RecordingFs {
    async fn list(&self, _path: &str) -> AppResult<Vec<RemoteEntry>> {
        Ok(Vec::new())
    }

    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        Err(AppError::RemoteFs(format!("{path}: not supported")))
    }

    async fn home_dir(&self) -> AppResult<String> {
        Ok("/".into())
    }

    async fn mkdir(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn create_file(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        state.rename_calls.push((from.into(), to.into()));
        if self.fail_promote && from.contains(".serverus-edit-") {
            return Err(AppError::RemoteFs("simulated rename failure".into()));
        }
        if state.files.contains_key(to) {
            return Err(AppError::RemoteFs(format!("{to}: target already exists")));
        }
        let contents = state
            .files
            .remove(from)
            .ok_or_else(|| AppError::RemoteFs(format!("{from}: not found")))?;
        state.files.insert(to.into(), contents);
        Ok(())
    }

    async fn replace_file(&self, staged: &str, target: &str) -> AppResult<()> {
        replace_file_via_backup(self, staged, target).await
    }

    async fn delete_file(&self, path: &str) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        state.delete_calls.push(path.into());
        state.files.remove(path);
        Ok(())
    }

    async fn delete_dir(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn chmod(&self, _path: &str, _mode: u32) -> AppResult<()> {
        Ok(())
    }

    async fn set_mtime(&self, _path: &str, _mtime_unix: i64) -> AppResult<()> {
        Ok(())
    }

    async fn open_read(&self, path: &str, _offset: u64) -> AppResult<BoxRead> {
        Err(AppError::RemoteFs(format!("{path}: not supported")))
    }

    async fn open_write(&self, path: &str, _offset: u64) -> AppResult<BoxWrite> {
        let path = path.to_string();
        {
            let mut state = self.state.lock().unwrap();
            state.open_write_paths.push(path.clone());
            state.files.insert(path.clone(), Vec::new());
        }
        if self.block_open_write {
            self.open_write_started.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(Box::new(RecordingWriter {
            state: self.state.clone(),
            path,
            failure: self.upload_failure,
        }))
    }

    async fn exists(&self, path: &str) -> AppResult<bool> {
        Ok(self.state.lock().unwrap().files.contains_key(path))
    }
}

pub(super) fn local_edit() -> tempfile::NamedTempFile {
    let local = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(local.path(), b"new contents").unwrap();
    local
}
