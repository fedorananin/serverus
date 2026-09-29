//! Vault mutations admitted by one exact unlock.

use super::DesktopApplication;
use crate::error::{AppError, AppResult};
use crate::vault::VaultManager;

impl DesktopApplication {
    /// Run one blocking vault operation only for the exact unlock
    /// authorization that admitted it. Admission happens before the first
    /// await so queued work retains the identity and access epoch that
    /// authorized it.
    pub(crate) async fn run_unlocked_vault_operation<T, F>(&self, operation: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut VaultManager) -> AppResult<T> + Send + 'static,
    {
        let lease = self.require_unlocked().map_err(AppError::from)?;
        self.run_unlocked_vault_operation_for_lease(lease, operation)
            .await
    }

    pub(crate) async fn run_unlocked_vault_operation_for_lease<T, F>(
        &self,
        lease: serverus_runtime::ContextLease,
        operation: F,
    ) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut VaultManager) -> AppResult<T> + Send + 'static,
    {
        let expected_vault = lease.vault().as_str().to_owned();
        let application = self.clone();
        self.run_owned_operation(async move {
            let _lifecycle = application.lock_lifecycle().await;
            lease.validate(&application).map_err(AppError::from)?;
            let vault = application.vault.clone();
            let admitted = application.clone();
            match tauri::async_runtime::spawn_blocking(move || {
                let mut manager = vault.lock().unwrap();
                lease.validate(&admitted).map_err(AppError::from)?;
                if manager.vault_id() != expected_vault {
                    return Err(AppError::WrongRuntimeContext);
                }
                operation(&mut manager)
            })
            .await
            {
                Ok(result) => result,
                Err(error) => Err(AppError::Other(format!("background task failed: {error}"))),
            }
        })
        .await
    }
}
