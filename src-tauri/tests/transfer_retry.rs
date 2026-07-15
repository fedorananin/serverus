//! Automatic transfer retry (transient failures — SFTP timeouts, dropped
//! data connections): a failed item requeues itself with backoff up to two
//! times before staying in Error for a manual retry.

#[path = "support/transfer_context.rs"]
mod transfer_context;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::error::{AppError, AppResult};
use serverus_lib::session::remote_fs::{BoxRead, BoxWrite, RemoteEntry, RemoteFs};
use serverus_lib::transfer::{ProgressSink, TransferManager, TransferState, UploadRequest};
use serverus_lib::vault::model::{ConflictPolicy, TransferSettings};

struct NullSink;
impl ProgressSink for NullSink {
    fn emit(&self, _event: serverus_lib::events::TransferProgressEvent) {}
}

/// Local-directory RemoteFs whose `open_write` fails the first N times —
/// simulates a flaky server without any networking.
struct FlakyFs {
    root: PathBuf,
    failures_left: AtomicU32,
    open_write_calls: AtomicU32,
}

impl FlakyFs {
    fn new(root: PathBuf, failures: u32) -> Arc<FlakyFs> {
        Arc::new(FlakyFs {
            root,
            failures_left: AtomicU32::new(failures),
            open_write_calls: AtomicU32::new(0),
        })
    }

    fn local(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }
}

#[async_trait::async_trait]
impl RemoteFs for FlakyFs {
    async fn list(&self, _path: &str) -> AppResult<Vec<RemoteEntry>> {
        Ok(Vec::new())
    }

    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        Err(AppError::RemoteFs(format!("{path}: not found")))
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
        let _ = tokio::fs::remove_file(self.local(path)).await;
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
        self.open_write_calls.fetch_add(1, Ordering::SeqCst);
        if self
            .failures_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(AppError::RemoteFs(format!("{path}: Timeout")));
        }
        let file = tokio::fs::File::create(self.local(path))
            .await
            .map_err(|e| AppError::RemoteFs(e.to_string()))?;
        Ok(Box::new(file))
    }

    async fn exists(&self, _path: &str) -> AppResult<bool> {
        Ok(false)
    }
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 2,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false,
        tar_acceleration: false,
    }
}

async fn upload_one(
    manager: &Arc<TransferManager>,
    fs: Arc<FlakyFs>,
    src: &std::path::Path,
) -> serverus_domain::runtime_context::RuntimeContextId {
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    let context_id = transfer_context::activate(manager);
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(fs, "session", src.to_str().unwrap(), "/", settings()),
        )
        .await
        .unwrap();
    context_id
}

/// Poll until the single item reaches `state` (retries include 1 s + 2 s
/// backoff sleeps, so allow plenty of time).
async fn wait_for_state(manager: &Arc<TransferManager>, state: TransferState) {
    for _ in 0..600 {
        let (items, _) = manager.snapshot();
        if items.len() == 1 && items[0].state == state {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (items, _) = manager.snapshot();
    panic!("item never reached {state:?}: {items:#?}");
}

#[tokio::test]
async fn transient_failures_are_retried_automatically() {
    let remote = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("photo.jpg"), vec![42u8; 64 * 1024]).unwrap();

    // Two failures then success — exactly the auto-retry budget.
    let fs = FlakyFs::new(remote.path().to_path_buf(), 2);
    let manager = Arc::new(TransferManager::default());
    let _context_id = upload_one(&manager, fs.clone(), &local.path().join("photo.jpg")).await;

    wait_for_state(&manager, TransferState::Done).await;
    assert_eq!(fs.open_write_calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        std::fs::read(remote.path().join("photo.jpg")).unwrap(),
        vec![42u8; 64 * 1024]
    );
}

#[tokio::test]
async fn persistent_failures_stop_after_the_retry_budget() {
    let remote = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("photo.jpg"), b"x").unwrap();

    let fs = FlakyFs::new(remote.path().to_path_buf(), u32::MAX);
    let manager = Arc::new(TransferManager::default());
    let context_id = upload_one(&manager, fs.clone(), &local.path().join("photo.jpg")).await;

    wait_for_state(&manager, TransferState::Error).await;
    // Give any (wrongly) scheduled further retries time to fire, then make
    // sure the run count stayed at 1 initial + 2 automatic retries.
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(fs.open_write_calls.load(Ordering::SeqCst), 3);
    let (items, _) = manager.snapshot();
    assert_eq!(items[0].state, TransferState::Error);

    // A manual retry re-arms the automatic budget: 3 more runs.
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .retry(context_id, &sink, &items[0].id)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(fs.open_write_calls.load(Ordering::SeqCst), 6);
}
