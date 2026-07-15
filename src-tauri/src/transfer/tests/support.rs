use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serverus_domain::runtime_context::RuntimeContextId;
use serverus_domain::transfers::{
    FailureKind as DomainFailureKind, RetryBudget as DomainRetryBudget,
    TransferEvent as DomainTransferEvent, TransferId as DomainTransferId,
};

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{BoxRead, BoxWrite, RemoteEntry, RemoteFs};
use crate::vault::model::{ConflictPolicy, TransferSettings};

use super::super::lifecycle::{new_control_channel, PendingRetry};
use super::super::{
    AdmissionToken, LocalDownloadTarget, TransferBatch, TransferItem, TransferKind,
    TransferLifecycle, TransferManager,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn context_id(value: u128) -> RuntimeContextId {
    RuntimeContextId::try_from(value).expect("test runtime context ID is non-zero")
}

pub(super) fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 1,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false,
        tar_acceleration: false,
    }
}

#[derive(Default)]
pub(super) struct TestFs {
    hang_delete: bool,
}

impl TestFs {
    pub(super) fn hanging_delete() -> Self {
        Self { hang_delete: true }
    }
}

#[async_trait::async_trait]
impl RemoteFs for TestFs {
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

    async fn delete_file(&self, _path: &str) -> AppResult<()> {
        if self.hang_delete {
            std::future::pending::<()>().await;
        }
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

    async fn open_read(&self, _path: &str, _offset: u64) -> AppResult<BoxRead> {
        Ok(Box::new(tokio::io::empty()))
    }

    async fn open_write(&self, _path: &str, _offset: u64) -> AppResult<BoxWrite> {
        Ok(Box::new(tokio::io::sink()))
    }

    async fn exists(&self, _path: &str) -> AppResult<bool> {
        Ok(false)
    }
}

pub(super) fn admission(session_id: &str) -> AdmissionToken {
    AdmissionToken {
        context_id: context_id(1),
        context_generation: 0,
        session_generation: 0,
        session_id: session_id.to_string(),
    }
}

pub(super) fn item_with_fs(session_id: &str, fs: Arc<dyn RemoteFs>) -> Arc<TransferItem> {
    item_with_batch_and_fs(session_id, TransferBatch::new(), fs)
}

pub(super) fn item_in_batch(session_id: &str, batch: Arc<TransferBatch>) -> Arc<TransferItem> {
    item_with_batch_and_fs(session_id, batch, Arc::new(TestFs::default()))
}

fn item_with_batch_and_fs(
    session_id: &str,
    batch: Arc<TransferBatch>,
    fs: Arc<dyn RemoteFs>,
) -> Arc<TransferItem> {
    let numeric_id = u128::from(NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let domain_id = DomainTransferId::try_from(numeric_id).expect("test transfer ID is non-zero");
    Arc::new(TransferItem {
        id: numeric_id.to_string(),
        session_id: session_id.to_string(),
        batch,
        kind: TransferKind::Upload,
        local_path: PathBuf::from("fixture.txt"),
        remote_path: "/fixture.txt".into(),
        name: "fixture.txt".into(),
        admission: admission(session_id),
        total: AtomicU64::new(10),
        done: AtomicU64::new(0),
        lifecycle: Mutex::new(TransferLifecycle::new(domain_id, DomainRetryBudget::new(2))),
        control: new_control_channel(),
        resolver: Mutex::new(None),
        pending_retry: Mutex::new(None::<PendingRetry>),
        last_done: AtomicU64::new(0),
        speed_bps: AtomicU64::new(0),
        fs,
        settings: settings(),
        resume: AtomicBool::new(false),
        tar: None,
        local_target: None::<LocalDownloadTarget>,
        partial_target: Mutex::new(None),
    })
}

pub(super) fn item(session_id: &str) -> Arc<TransferItem> {
    item_with_fs(session_id, Arc::new(TestFs::default()))
}

pub(super) fn start(item: &TransferItem) {
    item.apply_and_dispatch(DomainTransferEvent::StartRequested, None, None)
        .expect("queued test transfer starts");
}

pub(super) fn fail_recoverably(item: &TransferItem) {
    item.apply_and_dispatch(
        DomainTransferEvent::RecoverableFailure(DomainFailureKind::NetworkInterrupted),
        Some("timeout".into()),
        None,
    )
    .expect("running test transfer fails recoverably");
}

pub(super) fn insert(manager: &TransferManager, item: Arc<TransferItem>) {
    manager.items.lock().unwrap().push(item);
}
