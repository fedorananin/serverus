use crate::error::{ApiResult, AppError, AppResult};

use super::task_runtime::{blocking, run_owned_operation};

/// Run one blocking vault operation only for the exact unlock authorization
/// that admitted it.
pub(in crate::commands) async fn run_unlocked_vault_operation<T, F>(
    application: &crate::state::DesktopApplication,
    operation: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut crate::vault::VaultManager) -> AppResult<T> + Send + 'static,
{
    // Admission happens before the first await so queued work retains the
    // identity and access epoch that authorized it.
    let lease = application.require_unlocked().map_err(AppError::from)?;
    run_unlocked_vault_operation_for_lease(application, lease, operation).await
}

pub(in crate::commands) async fn run_unlocked_vault_operation_for_lease<T, F>(
    application: &crate::state::DesktopApplication,
    lease: serverus_runtime::ContextLease,
    operation: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut crate::vault::VaultManager) -> AppResult<T> + Send + 'static,
{
    let expected_vault = lease.vault().as_str().to_owned();
    let application = application.clone();
    let vault = application.vault.clone();

    run_owned_operation(async move {
        let _lifecycle = application.lock_lifecycle().await;
        lease.validate(&application).map_err(AppError::from)?;
        blocking(move || {
            let mut manager = vault.lock().unwrap();
            lease.validate(&application).map_err(AppError::from)?;
            if manager.vault_id() != expected_vault {
                return Err(AppError::WrongRuntimeContext);
            }
            operation(&mut manager)
        })
        .await
    })
    .await
}
