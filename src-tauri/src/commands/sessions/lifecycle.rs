use super::super::prelude::*;

#[tauri::command]
#[specta::specta]
pub async fn session_disconnect(state: State<'_, AppState>, session_id: String) -> ApiResult<()> {
    state.sessions.disconnect(&session_id).await;
    Ok(())
}
