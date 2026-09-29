use crate::error::ApiResult;

/// Run one blocking vault operation only for the exact unlock authorization
/// that admitted it (see `DesktopApplication::run_unlocked_vault_operation`).
pub(in crate::commands) async fn run_unlocked_vault_operation<T, F>(
    application: &crate::state::DesktopApplication,
    operation: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut crate::vault::VaultManager) -> crate::error::AppResult<T> + Send + 'static,
{
    application
        .run_unlocked_vault_operation(operation)
        .await
        .map_err(Into::into)
}

pub(in crate::commands) async fn run_unlocked_vault_operation_for_lease<T, F>(
    application: &crate::state::DesktopApplication,
    lease: serverus_runtime::ContextLease,
    operation: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut crate::vault::VaultManager) -> crate::error::AppResult<T> + Send + 'static,
{
    application
        .run_unlocked_vault_operation_for_lease(lease, operation)
        .await
        .map_err(Into::into)
}
