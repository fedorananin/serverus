use async_trait::async_trait;
use serverus_domain::runtime_context::{RuntimeContextId, VaultAccess, VaultKey};

pub trait RuntimeContextIdGenerator: Send + Sync + 'static {
    fn next_id(&self) -> RuntimeContextId;
}

#[async_trait]
pub trait ContextCleanup: Send + Sync + 'static {
    async fn retire(&self, context_id: RuntimeContextId) -> Result<(), ContextCleanupError>;
}

pub trait AppEventSink: Send + Sync + 'static {
    fn publish(&self, event: ContextEvent);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextEvent {
    Activated {
        context_id: RuntimeContextId,
        vault: VaultKey,
    },
    AccessChanged {
        context_id: RuntimeContextId,
        access: VaultAccess,
    },
    VaultReidentified {
        context_id: RuntimeContextId,
        vault: VaultKey,
    },
    Retired {
        context_id: RuntimeContextId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("runtime context cleanup failed: {message}")]
pub struct ContextCleanupError {
    message: String,
}

impl ContextCleanupError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
