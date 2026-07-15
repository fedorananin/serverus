use std::sync::Arc;

use serverus_application::context::{
    AppEventSink, ContextCleanup, ContextEvent, RuntimeContextIdGenerator,
};
use serverus_domain::runtime_context::{
    RuntimeContext, RuntimeContextId, VaultAccess, VaultAccessEpoch, VaultKey,
};
use tokio_util::sync::CancellationToken;

use super::state::{ActiveRuntimeContext, CoordinatorState, Inner};
use super::{ContextLease, RuntimeError, VaultSwitchPermit};

#[derive(Clone)]
pub struct ApplicationHandle {
    pub(super) inner: Arc<Inner>,
}

impl ApplicationHandle {
    pub fn new(
        ids: Arc<dyn RuntimeContextIdGenerator>,
        cleanup: Arc<dyn ContextCleanup>,
        events: Arc<dyn AppEventSink>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: std::sync::Mutex::new(CoordinatorState::Empty),
                ids,
                cleanup,
                events,
            }),
        }
    }

    pub fn activate_vault(&self, vault: VaultKey) -> Result<RuntimeContextId, RuntimeError> {
        self.activate_vault_with(vault, |_| {})
    }

    /// Prepare every child admission epoch before publishing the context as
    /// active. The callback runs while the coordinator transition is locked,
    /// so another command can observe either the previous state or the fully
    /// prepared state, never a half-activated generation.
    pub fn activate_vault_with(
        &self,
        vault: VaultKey,
        prepare: impl FnOnce(RuntimeContextId),
    ) -> Result<RuntimeContextId, RuntimeError> {
        let (id, event) = {
            let mut state = self.inner.state.lock().unwrap();
            match &mut *state {
                CoordinatorState::Active(active) if active.context.vault() != &vault => {
                    return Err(RuntimeError::DifferentVaultActive);
                }
                CoordinatorState::Active(active) => {
                    let id = active.context.id();
                    let next_access_epoch = active
                        .unlocked_access_epoch
                        .next()
                        .ok_or(RuntimeError::VaultAccessEpochExhausted)?;
                    prepare(id);
                    active.unlocked_access.cancel();
                    active.unlocked_access = CancellationToken::new();
                    active.unlocked_access_epoch = next_access_epoch;
                    active.context = active.context.unlock();
                    (
                        id,
                        ContextEvent::AccessChanged {
                            context_id: id,
                            access: VaultAccess::Unlocked,
                        },
                    )
                }
                CoordinatorState::Empty => {
                    let id = self.inner.ids.next_id();
                    prepare(id);
                    *state = CoordinatorState::Active(ActiveRuntimeContext {
                        context: RuntimeContext::unlocked(id, vault.clone()),
                        cancellation: CancellationToken::new(),
                        unlocked_access: CancellationToken::new(),
                        unlocked_access_epoch: VaultAccessEpoch::initial(),
                    });
                    (
                        id,
                        ContextEvent::Activated {
                            context_id: id,
                            vault,
                        },
                    )
                }
                CoordinatorState::Switching => return Err(RuntimeError::SwitchInProgress),
            }
        };
        self.inner.events.publish(event);
        Ok(id)
    }

    pub fn lock_vault(&self) -> Result<RuntimeContextId, RuntimeError> {
        let id = {
            let mut state = self.inner.state.lock().unwrap();
            match &mut *state {
                CoordinatorState::Active(active) => {
                    let id = active.context.id();
                    active.unlocked_access.cancel();
                    active.context = active.context.lock();
                    id
                }
                CoordinatorState::Empty => return Err(RuntimeError::NoActiveContext),
                CoordinatorState::Switching => return Err(RuntimeError::SwitchInProgress),
            }
        };
        self.inner.events.publish(ContextEvent::AccessChanged {
            context_id: id,
            access: VaultAccess::Locked,
        });
        Ok(id)
    }

    /// Updates the identity used to recognize the same vault after a path move.
    /// The runtime generation and all work it owns stay unchanged.
    pub fn reidentify_vault(&self, vault: VaultKey) -> Result<RuntimeContextId, RuntimeError> {
        let id = {
            let mut state = self.inner.state.lock().unwrap();
            match &mut *state {
                CoordinatorState::Active(active) => {
                    let id = active.context.id();
                    active.context = active.context.with_vault(vault.clone());
                    id
                }
                CoordinatorState::Empty => return Err(RuntimeError::NoActiveContext),
                CoordinatorState::Switching => return Err(RuntimeError::SwitchInProgress),
            }
        };
        self.inner.events.publish(ContextEvent::VaultReidentified {
            context_id: id,
            vault,
        });
        Ok(id)
    }

    pub fn require_active(&self) -> Result<ContextLease, RuntimeError> {
        self.lease(false)
    }

    pub fn require_unlocked(&self) -> Result<ContextLease, RuntimeError> {
        self.lease(true)
    }

    pub fn begin_vault_switch(&self) -> Result<VaultSwitchPermit, RuntimeError> {
        let previous = {
            let mut state = self.inner.state.lock().unwrap();
            match std::mem::replace(&mut *state, CoordinatorState::Switching) {
                CoordinatorState::Active(context) => context,
                CoordinatorState::Empty => {
                    *state = CoordinatorState::Empty;
                    return Err(RuntimeError::NoActiveContext);
                }
                CoordinatorState::Switching => {
                    *state = CoordinatorState::Switching;
                    return Err(RuntimeError::SwitchInProgress);
                }
            }
        };
        Ok(VaultSwitchPermit::new(self.clone(), previous))
    }

    fn lease(&self, require_unlocked: bool) -> Result<ContextLease, RuntimeError> {
        let state = self.inner.state.lock().unwrap();
        match &*state {
            CoordinatorState::Active(active) => {
                if require_unlocked && active.context.access() == VaultAccess::Locked {
                    return Err(RuntimeError::VaultLocked);
                }
                Ok(ContextLease::new(
                    active.context.id(),
                    active.context.vault().clone(),
                    active.cancellation.clone(),
                    require_unlocked
                        .then(|| (active.unlocked_access_epoch, active.unlocked_access.clone())),
                ))
            }
            CoordinatorState::Empty => Err(RuntimeError::NoActiveContext),
            CoordinatorState::Switching => Err(RuntimeError::SwitchInProgress),
        }
    }
}
