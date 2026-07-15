use serverus_domain::runtime_context::{RuntimeContextId, VaultAccessEpoch, VaultKey};
use tokio_util::sync::CancellationToken;

use super::{ApplicationHandle, RuntimeError};

#[derive(Clone, Debug)]
pub struct ContextLease {
    context_id: RuntimeContextId,
    vault: VaultKey,
    context_cancellation: CancellationToken,
    vault_access_epoch: Option<VaultAccessEpoch>,
    unlocked_access: Option<CancellationToken>,
}

impl ContextLease {
    pub(super) fn new(
        context_id: RuntimeContextId,
        vault: VaultKey,
        context_cancellation: CancellationToken,
        unlocked_access: Option<(VaultAccessEpoch, CancellationToken)>,
    ) -> Self {
        let (vault_access_epoch, unlocked_access) = match unlocked_access {
            Some((epoch, cancellation)) => (Some(epoch), Some(cancellation)),
            None => (None, None),
        };
        Self {
            context_id,
            vault,
            context_cancellation,
            vault_access_epoch,
            unlocked_access,
        }
    }

    pub const fn context_id(&self) -> RuntimeContextId {
        self.context_id
    }

    pub fn vault(&self) -> &VaultKey {
        &self.vault
    }

    pub const fn vault_access_epoch(&self) -> Option<VaultAccessEpoch> {
        self.vault_access_epoch
    }

    pub fn is_cancelled(&self) -> bool {
        self.context_cancellation.is_cancelled()
            || self
                .unlocked_access
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
    }

    pub async fn cancelled(&self) {
        if let Some(access) = &self.unlocked_access {
            tokio::select! {
                _ = self.context_cancellation.cancelled() => {}
                _ = access.cancelled() => {}
            }
        } else {
            self.context_cancellation.cancelled().await;
        }
    }

    pub fn validate(&self, handle: &ApplicationHandle) -> Result<(), RuntimeError> {
        if self.is_cancelled() {
            return Err(RuntimeError::StaleContext);
        }
        match handle.require_active() {
            Ok(current) if current.context_id == self.context_id => Ok(()),
            _ => Err(RuntimeError::StaleContext),
        }
    }
}

impl PartialEq for ContextLease {
    fn eq(&self, other: &Self) -> bool {
        self.context_id == other.context_id
    }
}

impl Eq for ContextLease {}
