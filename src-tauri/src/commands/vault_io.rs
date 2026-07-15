//! Vault import, export, activity, key, and secret commands.

use super::prelude::*;

/// Throttled user-activity ping for the auto-lock timer (SPEC §2.4).
#[tauri::command]
#[specta::specta]
pub async fn vault_touch_activity(state: State<'_, AppState>) -> ApiResult<()> {
    state.activity.touch();
    Ok(())
}

/// Export an UNENCRYPTED copy of the configuration without any secrets
/// (SPEC §8) — passwords, passphrases and inline keys are omitted.
#[tauri::command]
#[specta::specta]
pub async fn vault_export_config(state: State<'_, AppState>, path: String) -> ApiResult<()> {
    run_unlocked_vault_operation(&state.application, move |mgr| {
        let public = mgr.payload()?.to_public();
        let json = serde_json::to_vec_pretty(&public)
            .map_err(|e| AppError::Other(format!("serialize: {e}")))?;
        std::fs::write(&path, json)?;
        Ok(())
    })
    .await
}

/// Import a config file (a Serverus export or a hand-written file following
/// docs/CONFIG_FORMAT.md) into the unlocked vault. Merge semantics live in
/// `vault::import`.
#[tauri::command]
#[specta::specta]
pub async fn vault_import_config(
    state: State<'_, AppState>,
    path: String,
) -> ApiResult<ImportReport> {
    run_unlocked_vault_operation(&state.application, move |mgr| {
        let json = std::fs::read_to_string(&path)?;
        let mut connections = 0;
        let vault = mgr.with_payload(|p| {
            connections = crate::vault::import::apply(p, &json)?;
            Ok(p.to_public())
        })?;
        Ok(ImportReport { connections, vault })
    })
    .await
}

/// Read a private key file so the UI can store its text inside the vault
/// (the key then travels with vault backups). Validated in `local_fs` —
/// only PEM-looking files are returned.
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_read_file(path: String) -> ApiResult<String> {
    blocking(move || local_fs::read_private_key(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn connection_secrets(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<ConnectionSecrets> {
    run_unlocked_vault_operation(&state.application, move |mgr| {
        let conn = mgr
            .payload()?
            .connections
            .get(&id)
            .ok_or(AppError::ConnectionNotFound)?;
        Ok(ConnectionSecrets {
            password: conn.auth.password.clone(),
            key_passphrase: conn.auth.key_passphrase.clone(),
            key_inline: conn.auth.key_inline.clone(),
        })
    })
    .await
}
