use std::collections::HashMap;
use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;

use super::registry::OpenAdmissionRegistry;

pub(super) struct SessionAdmissionState {
    pub(super) generation: u64,
    pub(super) accepting: bool,
    pub(super) opens: usize,
}

impl Default for SessionAdmissionState {
    fn default() -> Self {
        Self {
            generation: 0,
            accepting: true,
            opens: 0,
        }
    }
}

pub(super) struct OpenAdmissionState {
    pub(super) context_generation: u64,
    pub(super) active_context_id: Option<RuntimeContextId>,
    pub(super) accepting: bool,
    pub(super) sessions: HashMap<String, SessionAdmissionState>,
}

impl Default for OpenAdmissionState {
    fn default() -> Self {
        Self {
            context_generation: 0,
            active_context_id: None,
            accepting: true,
            sessions: HashMap::new(),
        }
    }
}

pub(super) struct OpenAdmission {
    pub(super) registry: Arc<OpenAdmissionRegistry>,
    pub(super) session_id: String,
    pub(super) context_generation: u64,
    pub(super) generation: u64,
}
