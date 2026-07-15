use serverus_domain::runtime_context::RuntimeContextId;
use serverus_domain::transfers::TransferEvent as DomainTransferEvent;

use super::{domain_conflict_decision, ConflictAction, TransferManager};

impl TransferManager {
    pub fn pause(&self, id: &str) {
        if let Some(item) = self.find(id) {
            let _ = item.apply_and_dispatch(DomainTransferEvent::PauseRequested, None, None);
        }
    }

    pub fn resume(&self, id: &str) {
        if let Some(item) = self.find(id) {
            let _ = item.apply_and_dispatch(DomainTransferEvent::ResumeRequested, None, None);
        }
    }

    pub fn cancel(&self, id: &str) {
        if let Some(item) = self.find(id) {
            let _ = item.apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None);
        }
    }

    pub fn cancel_all(&self, context_id: RuntimeContextId) -> bool {
        self.apply_to_context(context_id, DomainTransferEvent::CancelRequested)
    }

    pub fn pause_all(&self, context_id: RuntimeContextId) -> bool {
        self.apply_to_context(context_id, DomainTransferEvent::PauseRequested)
    }

    pub fn resume_all(&self, context_id: RuntimeContextId) -> bool {
        self.apply_to_context(context_id, DomainTransferEvent::ResumeRequested)
    }

    fn apply_to_context(&self, context_id: RuntimeContextId, event: DomainTransferEvent) -> bool {
        self.mutate_items_for_context(context_id, |items| {
            for item in items {
                let _ = item.apply_and_dispatch(event, None, None);
            }
        })
    }

    pub fn resolve_conflict(
        &self,
        session_id: &str,
        id: &str,
        action: ConflictAction,
        apply_to_all: bool,
    ) {
        let Some(selected) = self.find(id).filter(|item| item.session_id == session_id) else {
            return;
        };
        let targets = if apply_to_all {
            selected.batch.apply_to_all(action);
            let batch_id = selected.batch.id();
            self.items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.batch.id() == batch_id)
                .cloned()
                .collect()
        } else {
            vec![selected]
        };
        for item in targets {
            let _ = item.apply_and_dispatch(
                DomainTransferEvent::ConflictResolved(domain_conflict_decision(action)),
                None,
                Some(action),
            );
        }
    }
}
