//! Transfer queue command adapters.

use serverus_domain::runtime_context::RuntimeContextId;

use std::sync::Arc;

use super::prelude::*;

fn requested_context(state: &AppState, runtime_context_id: &str) -> AppResult<RuntimeContextId> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    if runtime_context_id != lease.context_id().get().to_string() {
        return Err(AppError::WrongRuntimeContext);
    }
    Ok(lease.context_id())
}

fn require_applied(applied: bool) -> AppResult<()> {
    if applied {
        Ok(())
    } else {
        Err(AppError::WrongRuntimeContext)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_upload(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    local_paths: Vec<String>,
    remote_dir: String,
) -> ApiResult<()> {
    state
        .application
        .enqueue_uploads(Arc::new(app), &session_id, &local_paths, &remote_dir, None)
        .await
        .and_then(|enqueued| enqueued.result)
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_paths: Vec<String>,
    local_dir: String,
) -> ApiResult<()> {
    state
        .application
        .enqueue_downloads(Arc::new(app), &session_id, &remote_paths, &local_dir, None)
        .await
        .and_then(|enqueued| enqueued.result)
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_list(state: State<'_, AppState>) -> ApiResult<TransferListDto> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let snapshot = state.transfers.snapshot();
    lease.validate(&state.application).map_err(AppError::from)?;
    Ok(TransferListDto {
        runtime_context_id: lease.context_id().get().to_string(),
        items: snapshot.items,
        summary: snapshot.summary,
        session_summaries: snapshot.session_summaries,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_pause(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    state.transfers.pause(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_resume(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    state.transfers.resume(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_cancel(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    state.transfers.cancel(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_pause_all(
    state: State<'_, AppState>,
    runtime_context_id: String,
    session_id: String,
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.pause_all(context_id, &session_id)).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_resume_all(
    state: State<'_, AppState>,
    runtime_context_id: String,
    session_id: String,
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.resume_all(context_id, &session_id)).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_cancel_all(
    state: State<'_, AppState>,
    runtime_context_id: String,
    session_id: String,
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.cancel_all(context_id, &session_id)).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_clear_finished(
    state: State<'_, AppState>,
    runtime_context_id: String,
    session_id: String,
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.clear_finished(context_id, &session_id)).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_resolve(
    state: State<'_, AppState>,
    session_id: String,
    id: String,
    action: ConflictAction,
    apply_to_all: bool,
) -> ApiResult<()> {
    state
        .transfers
        .resolve_conflict(&session_id, &id, action, apply_to_all);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_retry(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let sink: std::sync::Arc<dyn crate::transfer::ProgressSink> = std::sync::Arc::new(app);
    state
        .transfers
        .retry(lease.context_id(), &sink, &id)
        .await
        .map_err(Into::into)
}
