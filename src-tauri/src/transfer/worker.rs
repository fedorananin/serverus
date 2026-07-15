use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serverus_domain::runtime_context::RuntimeContextId;
use serverus_domain::transfers::{
    AttemptNumber as DomainAttemptNumber, TransferEffect as DomainTransferEffect,
    TransferEvent as DomainTransferEvent,
};
use tokio::sync::oneshot;

use crate::error::AppResult;
use crate::session::remote_fs::parent_remote;

use super::{
    run_single, DownloadRequest, ProgressSink, TransferItem, TransferKind, TransferManager,
    TransferState, UploadRequest,
};

impl TransferManager {
    pub(super) fn spawn_worker(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        item: Arc<TransferItem>,
    ) {
        self.spawn_worker_for_event(app, item, DomainTransferEvent::StartRequested);
    }

    fn spawn_worker_for_event(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        item: Arc<TransferItem>,
        start_event: DomainTransferEvent,
    ) {
        self.spawn_worker_inner(app, item, Some(start_event));
    }

    fn spawn_started_worker(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        item: Arc<TransferItem>,
    ) {
        self.spawn_worker_inner(app, item, None);
    }

    fn spawn_worker_inner(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        item: Arc<TransferItem>,
        start_event: Option<DomainTransferEvent>,
    ) {
        let Some(queue) = self.queue_for_admitted(
            &item.admission,
            item.settings.max_parallel_per_server as usize,
        ) else {
            return;
        };
        let Some(task_guard) = self.activity.reserve_task(&item.admission) else {
            return;
        };
        let task_id = task_guard.id();
        let manager = self.clone();
        let app = app.clone();
        self.ensure_emitter(app.clone());
        let handle = tokio::spawn(async move {
            let _task_guard = task_guard;
            let _permit = queue.semaphore.clone().acquire_owned().await;
            if let Some(start_event) = start_event {
                let Ok(_) = item.apply_and_dispatch(start_event, None, None) else {
                    return;
                };
            }
            let result = match &item.tar {
                Some(job) => super::tar_stream::run(&item, job).await,
                None => run_single(&manager, &queue, &item).await,
            };
            manager.finish_worker(&app, item, result).await;
        });
        self.activity.attach_abort(task_id, handle.abort_handle());
    }

    async fn finish_worker(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        item: Arc<TransferItem>,
        result: AppResult<TransferState>,
    ) {
        for effect in item.complete_worker(result).await {
            if let DomainTransferEffect::ScheduleRetry { attempt, .. } = effect {
                self.schedule_auto_retry(app, item.clone(), attempt);
            }
        }
    }

    fn schedule_auto_retry(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        item: Arc<TransferItem>,
        attempt: DomainAttemptNumber,
    ) {
        let Some(claim) = item.retry_claim(attempt) else {
            return;
        };
        let Some(task_guard) = self.activity.reserve_task(&item.admission) else {
            return;
        };
        let task_id = task_guard.id();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        if !item.install_pending_retry(claim, cancel_tx) {
            return;
        }
        let retry_number = attempt.get().saturating_sub(1);
        let manager = self.clone();
        let app = app.clone();
        let handle = tokio::spawn(async move {
            let _task_guard = task_guard;
            tokio::select! {
                biased;
                _ = cancel_rx => return,
                _ = tokio::time::sleep(Duration::from_millis(
                    1000 * u64::from(retry_number),
                )) => {}
            }
            if !item.claim_auto_retry(claim) {
                return;
            }
            item.done.store(0, Ordering::Relaxed);
            item.resume.store(true, Ordering::Relaxed);
            manager.spawn_started_worker(&app, item);
        });
        self.activity.attach_abort(task_id, handle.abort_handle());
    }

    pub async fn retry(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        app: &Arc<dyn ProgressSink>,
        id: &str,
    ) -> AppResult<()> {
        let Some(item) = self.find(id) else {
            return Ok(());
        };
        let session_id = item.session_id.clone();
        self.run_admitted(context_id, &session_id, |admission| {
            self.retry_inner(app, item, admission)
        })
        .await
    }

    async fn retry_inner(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        item: Arc<TransferItem>,
        admission: super::AdmissionToken,
    ) -> AppResult<()> {
        if !item.begin_manual_retry() {
            return Ok(());
        }
        if item.tar.is_some() {
            let _ = item.apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None);
            let mut settings = item.settings.clone();
            settings.tar_acceleration = false;
            match item.kind {
                TransferKind::Upload => {
                    let local = item.local_path.to_string_lossy().into_owned();
                    let remote_dir = parent_remote(&item.remote_path);
                    self.enqueue_upload_inner(
                        admission,
                        app,
                        UploadRequest::new(
                            item.fs.clone(),
                            &item.session_id,
                            &local,
                            &remote_dir,
                            settings,
                        ),
                        None,
                        item.batch.clone(),
                    )
                    .await?;
                }
                TransferKind::Download => {
                    let local_dir = item
                        .local_path
                        .parent()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "/".into());
                    self.enqueue_download_inner(
                        admission,
                        app,
                        DownloadRequest::new(
                            item.fs.clone(),
                            &item.session_id,
                            &item.remote_path,
                            &local_dir,
                            settings,
                        ),
                        None,
                        item.batch.clone(),
                    )
                    .await?;
                }
            }
            return Ok(());
        }
        item.done.store(0, Ordering::Relaxed);
        item.resume.store(true, Ordering::Relaxed);
        self.spawn_worker(app, item);
        Ok(())
    }
}
