//! Vault lifecycle and quick-unlock commands.

use super::prelude::*;
use crate::vault::quick_unlock::QuickUnlock;
use crate::vault::VaultManager;

/// Keep the quick-unlock copy of the DEK in step with the vault: stored
/// behind biometrics while enabled — which also heals a keychain entry
/// invalidated by a fingerprint-set change (SPEC §2.3) — and gone while the
/// vault has no master password, since there is nothing to unlock then.
/// Best-effort: quick unlock failing must never block vault use.
pub(super) fn sync_quick_unlock(mgr: &VaultManager, quick: &dyn QuickUnlock) -> AppResult<()> {
    if mgr.is_passwordless() {
        quick.clear(&mgr.vault_id());
    } else if mgr.payload()?.settings.security.touch_id && quick.is_available() {
        let _ = quick.store_dek(&mgr.vault_id(), mgr.dek()?);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn vault_get_info(state: State<'_, AppState>) -> ApiResult<VaultInfo> {
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    blocking(move || {
        let mgr = vault.lock().unwrap();
        let biometry = quick.is_available();
        let passwordless = mgr.exists() && mgr.is_passwordless();
        Ok(VaultInfo {
            path: mgr.path().to_string_lossy().into_owned(),
            exists: mgr.exists(),
            unlocked: mgr.is_unlocked(),
            biometry_available: biometry,
            quick_unlock_ready: biometry && !passwordless && quick.has_dek(&mgr.vault_id()),
            quick_unlock_method: quick.method_name().to_string(),
            passwordless,
            scenario_build: cfg!(feature = "scenario-tests"),
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_create(state: State<'_, AppState>, password: String) -> ApiResult<PublicVault> {
    let password = Zeroizing::new(password);
    let application = state.application.clone();
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    let activity = state.activity.clone();
    run_owned_operation(async move {
        let _lifecycle = application.lock_lifecycle().await;
        let (public, vault_id) = blocking(move || {
            let mut mgr = vault.lock().unwrap();
            // An empty password creates a vault without a master password.
            mgr.create(&password, KdfParams::default())?;
            sync_quick_unlock(&mgr, quick.as_ref())?;
            let public = mgr.payload()?.to_public();
            activity.touch();
            Ok((public, mgr.vault_id()))
        })
        .await?;
        application.activate_selected_vault(vault_id)?;
        Ok(public)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_unlock_password(
    state: State<'_, AppState>,
    password: String,
) -> ApiResult<PublicVault> {
    let password = Zeroizing::new(password);
    let application = state.application.clone();
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    let activity = state.activity.clone();
    run_owned_operation(async move {
        let _lifecycle = application.lock_lifecycle().await;
        let (public, vault_id) = blocking(move || {
            let mut mgr = vault.lock().unwrap();
            mgr.unlock_with_password(&password)?;
            sync_quick_unlock(&mgr, quick.as_ref())?;
            let public = mgr.payload()?.to_public();
            activity.touch();
            Ok((public, mgr.vault_id()))
        })
        .await?;
        application.activate_selected_vault(vault_id)?;
        Ok(public)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_unlock_quick(state: State<'_, AppState>) -> ApiResult<PublicVault> {
    let application = state.application.clone();
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    let activity = state.activity.clone();
    run_owned_operation(async move {
        let _lifecycle = application.lock_lifecycle().await;
        let (public, vault_id) = blocking(move || {
            // Prompt outside the vault lock: the Touch ID dialog can sit there
            // for a while and must not block other vault reads.
            let vault_id = vault.lock().unwrap().vault_id();
            let dek = quick.retrieve_dek(&vault_id)?;
            let mut mgr = vault.lock().unwrap();
            mgr.unlock_with_dek(&dek)?;
            let public = mgr.payload()?.to_public();
            activity.touch();
            Ok((public, mgr.vault_id()))
        })
        .await?;
        application.activate_selected_vault(vault_id)?;
        Ok(public)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_lock(app: tauri::AppHandle, state: State<'_, AppState>) -> ApiResult<()> {
    state.application.lock_selected_vault().await?;
    let _ = VaultLockedEvent.emit(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn vault_change_password(
    state: State<'_, AppState>,
    current_password: String,
    new_password: String,
) -> ApiResult<()> {
    let current = Zeroizing::new(current_password);
    let new = Zeroizing::new(new_password);
    let quick = state.quick.clone();
    run_unlocked_vault_operation(&state.application, move |mgr| {
        // An empty `new` removes the master password; setting one on a vault
        // without it replaces the DEK, so the keychain copy must follow.
        mgr.change_password(&current, &new, KdfParams::default())?;
        sync_quick_unlock(mgr, quick.as_ref())
    })
    .await
}

/// Remove the master password after a Touch ID / Windows Hello check in
/// place of the typed password (see `VaultManager::remove_password_with_dek`).
#[tauri::command]
#[specta::specta]
pub async fn vault_remove_password_quick(state: State<'_, AppState>) -> ApiResult<()> {
    let vault_id = state.vault.lock().unwrap().vault_id();
    let quick = state.quick.clone();
    // Prompt outside the vault lock, as quick unlock does.
    let dek = blocking(move || quick.retrieve_dek(&vault_id)).await?;
    let quick = state.quick.clone();
    run_unlocked_vault_operation(&state.application, move |mgr| {
        mgr.remove_password_with_dek(&dek)?;
        sync_quick_unlock(mgr, quick.as_ref())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_set_touch_id(state: State<'_, AppState>, enabled: bool) -> ApiResult<()> {
    let quick = state.quick.clone();
    run_unlocked_vault_operation(&state.application, move |mgr| {
        mgr.with_payload(|p| {
            p.settings.security.touch_id = enabled;
            Ok(())
        })?;
        let id = mgr.vault_id();
        if enabled {
            if quick.is_available() && !mgr.is_passwordless() {
                quick.store_dek(&id, mgr.dek()?)?;
            }
        } else {
            quick.clear(&id);
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
#[path = "vault_access_tests.rs"]
mod tests;
