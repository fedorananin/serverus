use super::super::prelude::*;
use crate::session::TerminalStreamEvent;
use tauri::ipc::Channel;

#[tauri::command]
#[specta::specta]
pub async fn term_open(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
    output: Channel<TerminalStreamEvent>,
) -> ApiResult<String> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let entry = state.sessions.get(&session_id)?;
    let term_id = state
        .sessions
        .term_open(entry.clone(), cols, rows, output)
        .await
        .map_err(crate::error::ApiError::from)?;
    let sessions = state.sessions.clone();
    validate_context_and_owner_or_rollback(
        &state.application,
        &lease,
        || state.sessions.owns_entry(&entry),
        term_id,
        move |id| async move {
            sessions.term_close(&id).await;
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn term_write(
    state: State<'_, AppState>,
    term_id: String,
    data: String,
) -> ApiResult<()> {
    state
        .sessions
        .term_write(&term_id, data.as_bytes())
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn term_resize(
    state: State<'_, AppState>,
    term_id: String,
    cols: u16,
    rows: u16,
) -> ApiResult<()> {
    state
        .sessions
        .term_resize(&term_id, cols, rows)
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn term_close(state: State<'_, AppState>, term_id: String) -> ApiResult<()> {
    state.sessions.term_close(&term_id).await;
    Ok(())
}
