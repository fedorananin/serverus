use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;

use super::registry::ConnectAdmissionRegistry;

#[derive(Default)]
pub(super) struct ConnectAdmissionState {
    pub(super) generation: u64,
    pub(super) active_context_id: Option<RuntimeContextId>,
    pub(super) accepting: bool,
    pub(super) in_flight: usize,
}

pub(super) struct ConnectAdmission {
    pub(super) registry: Arc<ConnectAdmissionRegistry>,
    pub(super) generation: u64,
}
