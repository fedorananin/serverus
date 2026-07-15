//! Transfer queue command adapters.

use serverus_domain::runtime_context::RuntimeContextId;

use super::prelude::*;

fn transfer_settings(state: &AppState) -> crate::vault::model::TransferSettings {
    state
        .vault
        .lock()
        .unwrap()
        .payload()
        .map(|p| p.settings.transfers.clone())
        .unwrap_or_default()
}

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
    local_path: String,
    remote_dir: String,
) -> ApiResult<()> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let entry = state.sessions.get(&session_id)?;
    let fs = entry.remote_fs().await?;
    let settings = transfer_settings(&state);
    let tar_ssh = entry.tar_ssh().await;
    let sink: std::sync::Arc<dyn crate::transfer::ProgressSink> = std::sync::Arc::new(app);
    state
        .transfers
        .enqueue_upload_accelerated(
            lease.context_id(),
            &sink,
            crate::transfer::UploadRequest::new(
                fs,
                &session_id,
                &local_path,
                &remote_dir,
                settings,
            ),
            tar_ssh,
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    local_dir: String,
) -> ApiResult<()> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let entry = state.sessions.get(&session_id)?;
    let fs = entry.remote_fs().await?;
    let settings = transfer_settings(&state);
    let tar_ssh = entry.tar_ssh().await;
    let sink: std::sync::Arc<dyn crate::transfer::ProgressSink> = std::sync::Arc::new(app);
    state
        .transfers
        .enqueue_download_accelerated(
            lease.context_id(),
            &sink,
            crate::transfer::DownloadRequest::new(
                fs,
                &session_id,
                &remote_path,
                &local_dir,
                settings,
            ),
            tar_ssh,
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_list(state: State<'_, AppState>) -> ApiResult<TransferListDto> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let (items, summary) = state.transfers.snapshot();
    lease.validate(&state.application).map_err(AppError::from)?;
    Ok(TransferListDto {
        runtime_context_id: lease.context_id().get().to_string(),
        items,
        summary,
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
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.pause_all(context_id)).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_resume_all(
    state: State<'_, AppState>,
    runtime_context_id: String,
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.resume_all(context_id)).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_cancel_all(
    state: State<'_, AppState>,
    runtime_context_id: String,
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.cancel_all(context_id)).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_clear_finished(
    state: State<'_, AppState>,
    runtime_context_id: String,
) -> ApiResult<()> {
    let context_id = requested_context(&state, &runtime_context_id)?;
    require_applied(state.transfers.clear_finished(context_id)).map_err(Into::into)
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
