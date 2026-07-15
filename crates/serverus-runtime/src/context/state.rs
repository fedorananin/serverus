use std::sync::{Arc, Mutex};

use serverus_application::context::{AppEventSink, ContextCleanup, RuntimeContextIdGenerator};
use serverus_domain::runtime_context::{RuntimeContext, VaultAccessEpoch};
use tokio_util::sync::CancellationToken;

pub(super) struct Inner {
    pub(super) state: Mutex<CoordinatorState>,
    pub(super) ids: Arc<dyn RuntimeContextIdGenerator>,
    pub(super) cleanup: Arc<dyn ContextCleanup>,
    pub(super) events: Arc<dyn AppEventSink>,
}

pub(super) enum CoordinatorState {
    Empty,
    Active(ActiveRuntimeContext),
    Switching,
}

pub(super) struct ActiveRuntimeContext {
    pub(super) context: RuntimeContext,
    pub(super) cancellation: CancellationToken,
    /// Revoked on lock and replaced for every successful unlock. Active-only
    /// leases intentionally do not carry this token, so live sessions survive.
    pub(super) unlocked_access: CancellationToken,
    pub(super) unlocked_access_epoch: VaultAccessEpoch,
}
