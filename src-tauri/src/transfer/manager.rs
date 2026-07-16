use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serverus_domain::runtime_context::RuntimeContextId;
use tokio::sync::Semaphore;

use super::{
    ActivityRegistry, AdmissionToken, ServerQueue, TransferItem, TransferQueueSnapshot,
    TransferState, TransferSummary, SNAPSHOT_LIMIT,
};
use crate::error::{AppError, AppResult};

#[derive(Default)]
pub struct TransferManager {
    pub(super) items: Mutex<Vec<Arc<TransferItem>>>,
    pub(super) queues: Mutex<HashMap<String, Arc<ServerQueue>>>,
    pub(super) emitter_running: AtomicBool,
    pub(super) activity: Arc<ActivityRegistry>,
}

impl TransferManager {
    pub(super) async fn run_admitted<F, Fut>(
        &self,
        context_id: RuntimeContextId,
        session_id: &str,
        operation: F,
    ) -> AppResult<()>
    where
        F: FnOnce(AdmissionToken) -> Fut,
        Fut: Future<Output = AppResult<()>>,
    {
        let Some(admission) = self.activity.begin_producer(context_id, session_id) else {
            return Err(AppError::WrongRuntimeContext);
        };
        let token = admission.token();
        tokio::select! {
            biased;
            _ = admission.cancelled() => Err(AppError::WrongRuntimeContext),
            result = operation(token) => result,
        }
    }

    /// Open a fresh admission generation after the previous context cleanup.
    pub fn activate_context(&self, context_id: RuntimeContextId) {
        let _items = self.items.lock().unwrap();
        let mut state = self.activity.state.lock().unwrap();
        if state.active_context_id == Some(context_id) && state.accepting {
            return;
        }
        state.context_generation = state.context_generation.wrapping_add(1);
        state.active_context_id = Some(context_id);
        state.accepting = true;
        state.sessions.clear();
        drop(state);
        self.activity.changed.notify_waiters();
    }

    pub(super) fn queue_for_admitted(
        &self,
        admission: &AdmissionToken,
        parallel: usize,
    ) -> Option<Arc<ServerQueue>> {
        let activity = self.activity.state.lock().unwrap();
        if !ActivityRegistry::token_is_active(&activity, admission) {
            return None;
        }
        Some(
            self.queues
                .lock()
                .unwrap()
                .entry(admission.session_id.clone())
                .or_insert_with(|| {
                    Arc::new(ServerQueue {
                        semaphore: Arc::new(Semaphore::new(parallel.max(1))),
                    })
                })
                .clone(),
        )
    }

    pub(super) fn find(&self, id: &str) -> Option<Arc<TransferItem>> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .find(|item| item.id == id)
            .cloned()
    }

    pub(super) fn mutate_items_for_context(
        &self,
        context_id: RuntimeContextId,
        mutation: impl FnOnce(&mut Vec<Arc<TransferItem>>),
    ) -> bool {
        let mut items = self.items.lock().unwrap();
        let activity = self.activity.state.lock().unwrap();
        if !activity.accepting || activity.active_context_id != Some(context_id) {
            return false;
        }
        mutation(&mut items);
        true
    }

    pub fn snapshot(&self) -> TransferQueueSnapshot {
        let items = self.items.lock().unwrap();
        Self::snapshot_items(&items)
    }

    pub(super) fn progress_snapshot(&self) -> Option<(RuntimeContextId, TransferQueueSnapshot)> {
        let items = self.items.lock().unwrap();
        let activity = self.activity.state.lock().unwrap();
        let context_id = activity.active_context_id.filter(|_| activity.accepting)?;
        Some((context_id, Self::snapshot_items(&items)))
    }

    fn snapshot_items(items: &[Arc<TransferItem>]) -> TransferQueueSnapshot {
        let mut summary = TransferSummary {
            queued: 0,
            running: 0,
            done: 0,
            failed: 0,
            total_items: items.len() as u32,
        };
        let mut session_summaries: HashMap<String, TransferSummary> = HashMap::new();
        for item in items.iter() {
            let session = session_summaries
                .entry(item.session_id.clone())
                .or_default();
            session.total_items += 1;
            match item.state() {
                TransferState::Queued => {
                    summary.queued += 1;
                    session.queued += 1;
                }
                TransferState::Running | TransferState::Paused | TransferState::Conflict => {
                    summary.running += 1;
                    session.running += 1;
                }
                TransferState::Done | TransferState::Skipped => {
                    summary.done += 1;
                    session.done += 1;
                }
                TransferState::Error | TransferState::Cancelled => {
                    summary.failed += 1;
                    session.failed += 1;
                }
            }
        }
        let mut list: Vec<&Arc<TransferItem>> = items.iter().collect();
        list.sort_by_key(|item| match item.state() {
            TransferState::Running | TransferState::Paused | TransferState::Conflict => 0,
            TransferState::Queued => 1,
            TransferState::Error => 2,
            _ => 3,
        });
        // The snapshot cap is per session: the panel is rendered per tab, so
        // one busy session must not evict another session's items.
        let mut taken: HashMap<&str, usize> = HashMap::new();
        let snapshots = list
            .into_iter()
            .filter(|item| {
                let count = taken.entry(item.session_id.as_str()).or_insert(0);
                *count += 1;
                *count <= SNAPSHOT_LIMIT
            })
            .map(|item| item.snapshot())
            .collect();
        TransferQueueSnapshot {
            items: snapshots,
            summary,
            session_summaries,
        }
    }
}
