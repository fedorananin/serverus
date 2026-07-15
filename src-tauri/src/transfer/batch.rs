use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::ConflictAction;

pub(super) struct TransferBatch {
    id: Uuid,
    policy_override: Mutex<Option<ConflictAction>>,
}

impl TransferBatch {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            id: Uuid::new_v4(),
            policy_override: Mutex::new(None),
        })
    }

    pub(super) fn id(&self) -> Uuid {
        self.id
    }

    pub(super) fn policy_override(&self) -> Option<ConflictAction> {
        *self.policy_override.lock().unwrap()
    }

    pub(super) fn apply_to_all(&self, action: ConflictAction) {
        *self.policy_override.lock().unwrap() = Some(action);
    }
}
