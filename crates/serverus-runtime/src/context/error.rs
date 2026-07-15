use serverus_application::context::ContextCleanupError;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RuntimeError {
    #[error("no runtime context is active")]
    NoActiveContext,
    #[error("the selected vault is locked")]
    VaultLocked,
    #[error("a different vault is already active")]
    DifferentVaultActive,
    #[error("the runtime context is stale")]
    StaleContext,
    #[error("a vault switch is in progress")]
    SwitchInProgress,
    #[error("the vault access epoch is exhausted")]
    VaultAccessEpochExhausted,
    #[error(transparent)]
    Cleanup(#[from] ContextCleanupError),
}
