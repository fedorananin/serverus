use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use serverus_domain::transfers::TransferStateKind as DomainTransferStateKind;
use tokio::sync::{oneshot, watch};

use crate::session::remote_fs::RemoteFs;
use crate::vault::model::TransferSettings;

use super::lifecycle::PendingRetry;
use super::partial::PartialTransferTarget;
use super::tar_stream;
use super::{
    AdmissionToken, ConflictAction, Control, LocalDownloadTarget, TransferBatch, TransferKind,
    TransferLifecycle,
};

pub struct TransferItem {
    pub id: String,
    pub session_id: String,
    pub(super) batch: Arc<TransferBatch>,
    pub kind: TransferKind,
    pub local_path: PathBuf,
    pub remote_path: String,
    pub name: String,
    pub(super) admission: AdmissionToken,
    pub(super) total: AtomicU64,
    pub(super) done: AtomicU64,
    pub(super) lifecycle: Mutex<TransferLifecycle>,
    pub(super) control: watch::Sender<Control>,
    pub(super) resolver: Mutex<Option<oneshot::Sender<ConflictAction>>>,
    pub(super) pending_retry: Mutex<Option<PendingRetry>>,
    pub(super) last_done: AtomicU64,
    pub(super) speed_bps: AtomicU64,
    pub(super) fs: Arc<dyn RemoteFs>,
    pub(super) settings: TransferSettings,
    pub(super) resume: AtomicBool,
    pub(super) tar: Option<tar_stream::TarJob>,
    pub(super) local_target: Option<LocalDownloadTarget>,
    pub(super) partial_target: Mutex<Option<PartialTransferTarget>>,
}

impl TransferItem {
    pub(super) fn domain_state_kind(&self) -> DomainTransferStateKind {
        self.lifecycle.lock().unwrap().domain_state_kind()
    }
}
