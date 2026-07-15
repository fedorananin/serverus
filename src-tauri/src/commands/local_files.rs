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
