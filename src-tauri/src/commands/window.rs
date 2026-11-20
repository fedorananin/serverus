//! The "remember window size and position" switch (`crate::window_state`).

use super::prelude::*;

/// Whether the window frame is remembered across restarts.
#[tauri::command]
#[specta::specta]
pub async fn window_restore_get() -> ApiResult<bool> {
    blocking(|| Ok(crate::window_state::restore_enabled())).await
}

/// Turn remembering the window frame on or off. Takes effect at the next
/// quit (saving) and start (restoring).
#[tauri::command]
#[specta::specta]
pub async fn window_restore_set(enabled: bool) -> ApiResult<()> {
    blocking(move || {
        crate::window_state::set_restore_enabled(enabled)
            .map_err(|error| AppError::Other(format!("save window settings: {error}")))
    })
    .await
}
