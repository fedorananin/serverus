//! Vault path mutation and context-switch commands.

use super::prelude::*;

/// Move the vault to a different path (§8 Vault settings). Requires an
/// unlocked vault: the file is re-encrypted and written at the new location,
/// the old file stays as a manual backup.
#[tauri::command]
#[specta::specta]
pub async fn vault_set_path(state: State<'_, AppState>, path: String) -> ApiResult<()> {
    set_vault_path_application(
        &state.application,
        local_fs::expand(&path),
        crate::app_config::save,
    )
    .await
}

async fn set_vault_path_application(
    application: &crate::state::DesktopApplication,
    path: std::path::PathBuf,
    persist: impl FnOnce(&crate::app_config::AppConfig) -> std::io::Result<()> + Send + 'static,
) -> ApiResult<()> {
    let application = application.clone();
    let vault = application.vault.clone();
    let quick = application.quick.clone();
    run_owned_operation(async move {
        let _lifecycle = application.lock_lifecycle().await;
        let vault_id = blocking(move || {
            let mut mgr = vault.lock().unwrap();
            if !mgr.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            let old_id = mgr.vault_id();
            mgr.set_path_transactional(path, |resolved| {
                // Persist the path the vault actually ended up at — set_path may
                // have appended the file name when given a folder.
                persist(&crate::app_config::AppConfig {
                    vault_path: Some(resolved.to_string_lossy().into_owned()),
                })?;
                Ok(())
            })?;
            // Quick-unlock entries are keyed by path — move them along.
            if mgr.payload()?.settings.security.touch_id && quick.is_available() {
                quick.clear(&old_id);
                let _ = quick.store_dek(&mgr.vault_id(), mgr.dek()?);
            }
            Ok(mgr.vault_id())
        })
        .await?;
        application.reidentify_selected_vault(vault_id)?;
        Ok(())
    })
    .await
}

/// Point the app at a different vault file WITHOUT unlocking anything —
/// available from the lock screen (forgot password, multiple vaults).
/// An existing file gets the unlock form, a fresh path gets the create
/// form. The current vault is locked (secrets zeroized) before switching;
/// nothing is moved or rewritten on disk.
fn switch_vault_manager(
    current: &mut crate::vault::VaultManager,
    mut target: std::path::PathBuf,
    persist: impl FnOnce(&crate::app_config::AppConfig) -> std::io::Result<()>,
) -> AppResult<()> {
    // A folder means "the vault file inside it", keeping the file name.
    if target.is_dir() {
        if let Some(name) = current.path().file_name() {
            target = target.join(name);
        }
    }

    let next = crate::vault::VaultManager::new(target);
    // Keep the selected and unlocked runtime vault intact through the only
    // fallible step. Replacing the manager is infallible after persistence.
    persist(&crate::app_config::AppConfig {
        vault_path: Some(next.vault_id()),
    })?;
    current.lock();
    *current = next;
    Ok(())
}

async fn switch_vault_application(
    application: &crate::state::DesktopApplication,
    target: std::path::PathBuf,
    persist: impl FnOnce(&crate::app_config::AppConfig) -> std::io::Result<()> + Send + 'static,
) -> ApiResult<()> {
    let application = application.clone();
    run_owned_operation(async move {
        let _lifecycle = application.lock_lifecycle().await;
        let switch_permit = match application.begin_vault_switch() {
            Ok(permit) => Some(permit),
            // Selecting the first vault on the initial lock screen has no runtime
            // generation to retire.
            Err(serverus_runtime::RuntimeError::NoActiveContext) => None,
            Err(error) => return Err(AppError::from(error).into()),
        };
        let vault = application.vault.clone();
        blocking(move || {
            let mut manager = vault.lock().unwrap();
            switch_vault_manager(&mut manager, target, persist)
        })
        .await?;
        if let Some(permit) = switch_permit {
            permit.commit().await.map_err(AppError::from)?;
        }
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_switch_path(state: State<'_, AppState>, path: String) -> ApiResult<()> {
    switch_vault_application(
        &state.application,
        local_fs::expand(&path),
        crate::app_config::save,
    )
    .await
}

#[cfg(test)]
#[path = "vault_location_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "vault_location_cancellation_tests.rs"]
mod cancellation_tests;
