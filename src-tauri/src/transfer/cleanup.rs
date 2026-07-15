use serverus_domain::runtime_context::RuntimeContextId;
use serverus_domain::transfers::{
    TransferEvent as DomainTransferEvent, TransferStateKind as DomainTransferStateKind,
};
use std::time::Duration;

use super::TransferManager;

const COOPERATIVE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);

impl TransferManager {
    /// Drop transfers belonging to a disconnected session after quiescence.
    pub async fn clear_session(&self, session_id: &str) {
        let (drained, aborts) = {
            let mut items = self.items.lock().unwrap();
            let mut activity = self.activity.state.lock().unwrap();
            let session = activity.sessions.entry(session_id.to_string()).or_default();
            session.generation = session.generation.wrapping_add(1);
            session.accepting = false;
            let aborts = activity
                .tasks
                .values()
                .filter(|task| task.admission.session_id == session_id)
                .filter_map(|task| task.abort.clone())
                .collect::<Vec<_>>();
            let mut drained = Vec::new();
            items.retain(|item| {
                if item.session_id == session_id {
                    drained.push(item.clone());
                    false
                } else {
                    true
                }
            });
            self.queues.lock().unwrap().remove(session_id);
            (drained, aborts)
        };
        self.activity.changed.notify_waiters();
        for item in &drained {
            let _ = item.apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None);
        }
        if tokio::time::timeout(
            COOPERATIVE_CLEANUP_TIMEOUT,
            self.activity.wait_session_quiescent(session_id),
        )
        .await
        .is_err()
        {
            for abort in aborts {
                abort.abort();
            }
            self.activity.wait_session_quiescent(session_id).await;
        }
        for item in drained {
            item.cleanup_partial().await;
        }
    }

    /// Invalidate and drain all queue/history state before signalling workers.
    pub async fn clear_all(&self) {
        let (drained, aborts) = {
            let mut items = self.items.lock().unwrap();
            let mut activity = self.activity.state.lock().unwrap();
            activity.context_generation = activity.context_generation.wrapping_add(1);
            activity.active_context_id = None;
            activity.accepting = false;
            for session in activity.sessions.values_mut() {
                session.generation = session.generation.wrapping_add(1);
                session.accepting = false;
            }
            let aborts = activity
                .tasks
                .values()
                .filter_map(|task| task.abort.clone())
                .collect::<Vec<_>>();
            self.queues.lock().unwrap().clear();
            (std::mem::take(&mut *items), aborts)
        };
        self.activity.changed.notify_waiters();
        for item in &drained {
            let _ = item.apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None);
        }
        if tokio::time::timeout(
            COOPERATIVE_CLEANUP_TIMEOUT,
            self.activity.wait_all_quiescent(),
        )
        .await
        .is_err()
        {
            for abort in aborts {
                abort.abort();
            }
            self.activity.wait_all_quiescent().await;
        }
        for item in drained {
            item.cleanup_partial().await;
        }
    }

    /// Remove domain-terminal transfers; retry backoff remains observable.
    pub fn clear_finished(&self, context_id: RuntimeContextId) -> bool {
        self.mutate_items_for_context(context_id, |items| {
            items.retain(|item| {
                !matches!(
                    item.domain_state_kind(),
                    DomainTransferStateKind::Completed
                        | DomainTransferStateKind::Cancelled
                        | DomainTransferStateKind::Failed
                )
            });
        })
    }
}
