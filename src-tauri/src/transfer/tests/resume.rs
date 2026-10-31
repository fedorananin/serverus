use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{BoxRead, BoxWrite, RemoteEntry, RemoteFs};
use crate::vault::model::ConflictPolicy;

use super::super::{
    open_download_root, run_single, LocalDownloadTarget, ServerQueue, TransferBatch, TransferItem,
    TransferKind, TransferManager, TransferState,
};
use super::support::{item_with_shape, settings, start, Shape};

const CONTENT: &[u8] = b"remote bytes, all of them";

/// One remote file. The first read breaks after a few bytes (a dropped
/// connection); every later one serves the rest from the requested offset.
#[derive(Default)]
struct FlakyFs {
    broke_once: AtomicBool,
    fail_open_write: bool,
    atomic: bool,
    deleted: Mutex<Vec<String>>,
}

struct BrokenRead(&'static [u8]);

impl tokio::io::AsyncRead for BrokenRead {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.0.is_empty() {
            return std::task::Poll::Ready(Err(std::io::Error::other("connection reset")));
        }
        buf.put_slice(self.0);
        self.0 = &[];
        std::task::Poll::Ready(Ok(()))
    }
}

#[async_trait::async_trait]
impl RemoteFs for FlakyFs {
    async fn list(&self, _path: &str) -> AppResult<Vec<RemoteEntry>> {
        Ok(Vec::new())
    }
    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        Ok(RemoteEntry {
            name: "foo.zip".into(),
            path: path.into(),
            is_dir: false,
            is_symlink: false,
            size: CONTENT.len() as u64,
            mtime: None,
            permissions: None,
        })
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
    async fn rename(&self, _from: &str, _to: &str) -> AppResult<()> {
        Ok(())
    }
    async fn delete_file(&self, path: &str) -> AppResult<()> {
        self.deleted.lock().unwrap().push(path.into());
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
    async fn open_read(&self, _path: &str, offset: u64) -> AppResult<BoxRead> {
        if !self.broke_once.swap(true, Ordering::Relaxed) {
            return Ok(Box::new(BrokenRead(&CONTENT[..6])));
        }
        Ok(Box::new(&CONTENT[offset as usize..]))
    }
    async fn open_write(&self, _path: &str, _offset: u64) -> AppResult<BoxWrite> {
        if self.fail_open_write {
            return Err(AppError::RemoteFs("permission denied".into()));
        }
        Ok(Box::new(tokio::io::sink()))
    }
    async fn exists(&self, _path: &str) -> AppResult<bool> {
        Ok(true)
    }
    fn writes_atomically(&self) -> bool {
        self.atomic
    }
}

fn queue() -> Arc<ServerQueue> {
    Arc::new(ServerQueue {
        semaphore: Arc::new(Semaphore::new(1)),
    })
}

async fn attempt(item: &Arc<TransferItem>) -> AppResult<TransferState> {
    let manager = Arc::new(TransferManager::default());
    run_single(&manager, &queue(), item).await
}

#[tokio::test]
async fn retry_after_rename_continues_into_the_renamed_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("foo.zip"), b"ORIGINAL").unwrap();
    let mut transfer = settings();
    transfer.conflict_policy = ConflictPolicy::Rename;
    let item = item_with_shape(
        "session",
        TransferBatch::new(),
        Arc::new(FlakyFs::default()),
        Shape {
            kind: TransferKind::Download,
            local_path: dir.path().join("foo.zip"),
            remote_path: "/foo.zip".into(),
            total: CONTENT.len() as u64,
            settings: transfer,
            local_target: Some(LocalDownloadTarget {
                root: open_download_root(dir.path()).unwrap(),
                relative: "foo.zip".into(),
            }),
        },
    );
    start(&item);

    assert!(attempt(&item).await.is_err(), "the first read breaks");
    item.resume.store(true, Ordering::Relaxed);
    assert_eq!(attempt(&item).await.unwrap(), TransferState::Done);

    assert_eq!(
        std::fs::read(dir.path().join("foo.zip")).unwrap(),
        b"ORIGINAL"
    );
    assert_eq!(
        std::fs::read(dir.path().join("foo (1).zip")).unwrap(),
        CONTENT
    );
}

async fn upload_partial(fs: FlakyFs) -> (Arc<TransferItem>, AppResult<TransferState>) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("index.html");
    std::fs::write(&source, CONTENT).unwrap();
    let item = item_with_shape(
        "session",
        TransferBatch::new(),
        Arc::new(fs),
        Shape {
            local_path: source,
            remote_path: "/index.html".into(),
            ..Shape::fixture()
        },
    );
    start(&item);
    let result = attempt(&item).await;
    (item, result)
}

#[tokio::test]
async fn a_target_that_never_opened_is_not_a_partial() {
    let fs = FlakyFs {
        fail_open_write: true,
        ..FlakyFs::default()
    };
    let (item, result) = upload_partial(fs).await;
    assert!(result.is_err());
    assert!(
        !item.has_partial(),
        "the original would be deleted on tab close"
    );
}

#[tokio::test]
async fn an_atomic_upload_never_leaves_a_partial_to_delete() {
    let fs = FlakyFs {
        atomic: true,
        ..FlakyFs::default()
    };
    let (item, result) = upload_partial(fs).await;
    assert_eq!(result.unwrap(), TransferState::Done);
    assert!(!item.has_partial());

    let fs = FlakyFs::default();
    let (item, _) = upload_partial(fs).await;
    assert!(
        item.has_partial(),
        "in-place writes still clean up after themselves"
    );
}
