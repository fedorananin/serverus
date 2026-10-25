//! Local file-panel commands.

use super::prelude::*;

#[tauri::command]
#[specta::specta]
pub async fn local_list(path: String) -> ApiResult<Vec<RemoteEntry>> {
    blocking(move || local_fs::list(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_home() -> ApiResult<String> {
    Ok(local_fs::home())
}

#[tauri::command]
#[specta::specta]
pub async fn local_mkdir(path: String) -> ApiResult<()> {
    blocking(move || local_fs::mkdir(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_create_file(path: String) -> ApiResult<()> {
    blocking(move || local_fs::create_file(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_rename(from: String, to: String) -> ApiResult<()> {
    blocking(move || local_fs::rename(&from, &to)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_delete(path: String) -> ApiResult<()> {
    blocking(move || local_fs::delete(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_chmod(path: String, mode: u32) -> ApiResult<()> {
    blocking(move || local_fs::chmod(&path, mode)).await
}

/// Open a local file in the configured editor (double click / Edit… in the
/// local pane). Unlike remote edit there is no copy to sync back: the editor
/// works on the file itself.
#[tauri::command]
#[specta::specta]
pub async fn local_edit_open(state: State<'_, AppState>, path: String) -> ApiResult<()> {
    state.application.require_active().map_err(AppError::from)?;
    let editor = state
        .vault
        .lock()
        .unwrap()
        .payload()
        .map(|p| p.settings.editor.clone())
        .unwrap_or_default();
    blocking(move || crate::watcher::open_in_editor(&local_fs::expand(&path), &editor)).await
}
