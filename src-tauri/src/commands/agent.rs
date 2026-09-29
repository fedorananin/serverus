//! AI agent (MCP) command adapters: the UI answering agent requests and
//! taking terminals over.

use super::prelude::*;
use crate::agent::types::{AgentSetupInfo, AgentTerminalEvent, AgentUiResponse};

/// Answer a pending agent UI request. Returns false when it already
/// expired.
#[tauri::command]
#[specta::specta]
pub async fn agent_ui_respond(
    state: State<'_, AppState>,
    request_id: String,
    response: AgentUiResponse,
) -> ApiResult<bool> {
    Ok(state.agent.respond(&request_id, response))
}

/// The user takes a terminal away from the agent.
#[tauri::command]
#[specta::specta]
pub async fn agent_take_over(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    term_id: String,
) -> ApiResult<()> {
    state
        .agent
        .set_user_control(&app, &state.sessions, &term_id, true)
        .await
        .map_err(Into::into)
}

/// The user hands a terminal back to the agent.
#[tauri::command]
#[specta::specta]
pub async fn agent_hand_back(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    term_id: String,
) -> ApiResult<()> {
    state
        .agent
        .set_user_control(&app, &state.sessions, &term_id, false)
        .await
        .map_err(Into::into)
}

/// Agent state of every terminal the agent touched, for a UI that starts up.
#[tauri::command]
#[specta::specta]
pub async fn agent_terminal_states(
    state: State<'_, AppState>,
) -> ApiResult<Vec<AgentTerminalEvent>> {
    Ok(state.agent.terminal_states())
}

/// How to register Serverus with an MCP client.
#[tauri::command]
#[specta::specta]
pub async fn agent_setup_info(state: State<'_, AppState>) -> ApiResult<AgentSetupInfo> {
    Ok(state.agent.setup_info())
}
