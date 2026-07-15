//! Remote-edit command adapter.

use super::prelude::*;

/// Download a remote file into the isolated edit cache, open it in the
/// configured editor and auto-upload every save (SPEC §5.3).
#[tauri::command]
#[specta::specta]
pub async fn remote_edit_open(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
) -> ApiResult<()> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    let editor = state
        .vault
        .lock()
        .unwrap()
        .payload()
        .map(|p| p.settings.editor.clone())
        .unwrap_or_default();
    let local_path = state
        .edits
        .open(
            lease.context_id(),
            app,
            fs,
            &session_id,
            &remote_path,
            &editor,
        )
        .await?;
    let edits = state.edits.clone();
    validate_context_or_rollback(
        &state.application,
        &lease,
        local_path,
        move |path| async move {
            edits.close_path(&path).await;
        },
    )
    .await?;
    Ok(())
}

/// Drain completed remote-edit outcomes so the UI cannot miss a fast save
/// while its event listener is still starting or the window is temporarily hidden.
#[tauri::command]
#[specta::specta]
pub fn remote_edit_notifications(
    state: State<'_, AppState>,
) -> ApiResult<Vec<crate::events::RemoteEditUploadedEvent>> {
    state.application.require_active().map_err(AppError::from)?;
    Ok(state.edits.take_notifications())
}

#[cfg(test)]
#[path = "remote_edit_tests.rs"]
mod tests;
