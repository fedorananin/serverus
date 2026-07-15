//! Protocol-agnostic remote file-panel commands.

use super::prelude::*;

#[tauri::command]
#[specta::specta]
pub async fn remote_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> ApiResult<Vec<RemoteEntry>> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move { entry.remote_fs().await?.list(&path).await },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_home(state: State<'_, AppState>, session_id: String) -> ApiResult<String> {
    run_session_operation(
        &state.application,
        &session_id,
        |entry, _lease| async move { entry.remote_fs().await?.home_dir().await },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> ApiResult<()> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move { entry.remote_fs().await?.mkdir(&path).await },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_create_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> ApiResult<()> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move { entry.remote_fs().await?.create_file(&path).await },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> ApiResult<()> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move { entry.remote_fs().await?.rename(&from, &to).await },
    )
    .await
}

/// Recursive delete — works identically for SFTP and FTP (SPEC §4.3).
#[tauri::command]
#[specta::specta]
pub async fn remote_delete(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    is_dir: bool,
) -> ApiResult<()> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move {
            let fs = entry.remote_fs().await?;
            remote_fs::delete_recursive(fs.as_ref(), &path, is_dir).await
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_chmod(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: u32,
) -> ApiResult<()> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move { entry.remote_fs().await?.chmod(&path, mode).await },
    )
    .await
}
