use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serverus_domain::transfers::{
    RetryBudget as DomainRetryBudget, TransferId as DomainTransferId,
};

use crate::events::TransferProgressEvent;
use crate::session::remote_fs::RemoteFs;
use crate::vault::model::TransferSettings;

use super::lifecycle::{new_control_channel, PendingRetry};
use super::tar_stream;
use super::{
    ActivityRegistry, AdmissionToken, LocalDownloadTarget, ProgressSink, TransferItem,
    TransferKind, TransferLifecycle, TransferManager, TransferState, AUTO_RETRIES,
};

impl TransferManager {
    /// Emit progress at roughly 4 Hz while transfers remain active.
    pub(super) fn ensure_emitter(self: &Arc<Self>, app: Arc<dyn ProgressSink>) {
        if self.emitter_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let manager = self.clone();
        tokio::spawn(async move {
            let mut idle_rounds = 0_u32;
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                {
                    let items = manager.items.lock().unwrap();
                    for item in items.iter() {
                        let done = item.done.load(Ordering::Relaxed);
                        let last = item.last_done.swap(done, Ordering::Relaxed);
                        let delta = done.saturating_sub(last);
                        let previous = item.speed_bps.load(Ordering::Relaxed);
                        let speed = if item.state() == TransferState::Running {
                            (previous / 2).saturating_add(delta * 2)
                        } else {
                            0
                        };
                        item.speed_bps.store(speed, Ordering::Relaxed);
                    }
                }
                let active = if let Some((context_id, snapshot)) = manager.progress_snapshot() {
                    let active = snapshot.summary.queued + snapshot.summary.running;
                    app.emit(TransferProgressEvent {
                        runtime_context_id: context_id.get().to_string(),
                        items: snapshot.items,
                        summary: snapshot.summary,
                        session_summaries: snapshot.session_summaries,
                    });
                    active
                } else {
                    0
                };
                if active == 0 {
                    idle_rounds += 1;
                    if idle_rounds > 8 {
                        if manager.release_or_reclaim_emitter() {
                            idle_rounds = 0;
                        } else {
                            break;
                        }
                    }
                } else {
                    idle_rounds = 0;
                }
            }
        });
    }

    /// Release emitter ownership, then re-check for work that may have been
    /// admitted while producers still observed the old owner as running.
    pub(super) fn release_or_reclaim_emitter(&self) -> bool {
        self.emitter_running.store(false, Ordering::SeqCst);
        let active = self
            .progress_snapshot()
            .is_some_and(|(_, snapshot)| snapshot.summary.queued + snapshot.summary.running > 0);
        active && !self.emitter_running.swap(true, Ordering::SeqCst)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn add_item(
        &self,
        admission: AdmissionToken,
        batch: Arc<super::TransferBatch>,
        session_id: &str,
        kind: TransferKind,
        local_path: PathBuf,
        remote_path: String,
        total: u64,
        fs: Arc<dyn RemoteFs>,
        settings: TransferSettings,
        local_target: Option<LocalDownloadTarget>,
        tar: Option<tar_stream::TarJob>,
    ) -> Option<Arc<TransferItem>> {
        debug_assert_eq!(admission.session_id, session_id);
        let name = match kind {
            TransferKind::Upload => local_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            TransferKind::Download => remote_path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string(),
        };
        let item_id = uuid::Uuid::new_v4();
        let domain_id = DomainTransferId::try_from(item_id.as_u128())
            .expect("a UUID v4 transfer ID is non-zero");
        let item = Arc::new(TransferItem {
            id: item_id.to_string(),
            session_id: session_id.to_string(),
            batch,
            kind,
            local_path,
            remote_path,
            name,
            admission: admission.clone(),
            total: AtomicU64::new(total),
            done: AtomicU64::new(0),
            lifecycle: Mutex::new(TransferLifecycle::new(
                domain_id,
                DomainRetryBudget::new(AUTO_RETRIES),
            )),
            control: new_control_channel(),
            resolver: Mutex::new(None),
            pending_retry: Mutex::new(None::<PendingRetry>),
            last_done: AtomicU64::new(0),
            speed_bps: AtomicU64::new(0),
            fs,
            settings,
            resume: AtomicBool::new(false),
            tar,
            local_target,
            partial_target: Mutex::new(None),
        });
        let mut items = self.items.lock().unwrap();
        let activity = self.activity.state.lock().unwrap();
        if !ActivityRegistry::token_is_active(&activity, &admission) {
            return None;
        }
        items.push(item.clone());
        Some(item)
    }
}
