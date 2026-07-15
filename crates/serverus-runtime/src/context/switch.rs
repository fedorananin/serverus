use serverus_application::context::{ContextCleanupError, ContextEvent};
use serverus_domain::runtime_context::RuntimeContextId;

use super::state::{ActiveRuntimeContext, CoordinatorState};
use super::{ApplicationHandle, RuntimeError};

pub struct VaultSwitchPermit {
    handle: ApplicationHandle,
    previous: Option<ActiveRuntimeContext>,
}

impl VaultSwitchPermit {
    pub(super) fn new(handle: ApplicationHandle, previous: ActiveRuntimeContext) -> Self {
        Self {
            handle,
            previous: Some(previous),
        }
    }

    pub async fn commit(mut self) -> Result<RuntimeContextId, RuntimeError> {
        let previous = self.previous.take().ok_or(RuntimeError::SwitchInProgress)?;
        let context_id = previous.context.id();
        let finalizer = RetirementFinalizer::new(self.handle.clone(), context_id);
        let cleanup = self.handle.inner.cleanup.clone();
        previous.cancellation.cancel();
        let cleanup = tokio::spawn(async move {
            let cleanup = cleanup.retire(context_id).await;
            if cleanup.is_ok() {
                finalizer.finish();
            }
            cleanup
        })
        .await
        .map_err(|_| {
            RuntimeError::Cleanup(ContextCleanupError::new(
                "context cleanup task terminated unexpectedly",
            ))
        })?;
        cleanup?;
        Ok(context_id)
    }
}

/// The owned cleanup task finalizes only a proven-complete retirement. A
/// cleanup error or panic deliberately leaves the runtime non-admitting.
struct RetirementFinalizer {
    handle: ApplicationHandle,
    context_id: RuntimeContextId,
}

impl RetirementFinalizer {
    fn new(handle: ApplicationHandle, context_id: RuntimeContextId) -> Self {
        Self { handle, context_id }
    }

    fn finish(self) {
        let retired = {
            let mut state = self.handle.inner.state.lock().unwrap();
            if matches!(*state, CoordinatorState::Switching) {
                *state = CoordinatorState::Empty;
                true
            } else {
                false
            }
        };
        if retired {
            self.handle.inner.events.publish(ContextEvent::Retired {
                context_id: self.context_id,
            });
        }
    }
}

impl Drop for VaultSwitchPermit {
    fn drop(&mut self) {
        let Some(previous) = self.previous.take() else {
            return;
        };
        let mut state = self.handle.inner.state.lock().unwrap();
        if matches!(*state, CoordinatorState::Switching) {
            *state = CoordinatorState::Active(previous);
        }
    }
}
