use std::collections::HashMap;
use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;

use super::registry::SessionOperationRegistry;

pub(super) struct SessionOperations {
    pub(super) generation: u64,
    pub(super) accepting: bool,
    pub(super) in_flight: usize,
}

impl Default for SessionOperations {
    fn default() -> Self {
        Self {
            generation: 0,
            accepting: true,
            in_flight: 0,
        }
    }
}

#[derive(Default)]
pub(super) struct OperationState {
    pub(super) context_generation: u64,
    pub(super) active_context_id: Option<RuntimeContextId>,
    pub(super) accepting: bool,
    pub(super) sessions: HashMap<String, SessionOperations>,
}

pub(super) struct OperationAdmission {
    pub(super) registry: Arc<SessionOperationRegistry>,
    pub(super) context_id: RuntimeContextId,
    pub(super) context_generation: u64,
    pub(super) session_id: String,
    pub(super) session_generation: u64,
}
