//! SSH tunnel command adapters.

use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn start_tunnel_for_context(
    application: &serverus_runtime::ApplicationHandle,
    lease: &serverus_runtime::ContextLease,
    sessions: &std::sync::Arc<crate::session::SessionManager>,
    entry: &std::sync::Arc<crate::session::SessionEntry>,
    name: &str,
    local_port: u16,
    remote_host: &str,
    remote_port: u16,
) -> ApiResult<TunnelStatus> {
    lease.validate(application).map_err(AppError::from)?;
    if !sessions.owns_entry(entry) {
        return Err(AppError::SessionNotFound.into());
    }
    let ssh = entry
        .ssh
        .clone()
        .ok_or_else(|| AppError::Other("not an SSH session".into()))?;
    let status = sessions
        .tunnels
        .start(ssh, &entry.id, name, local_port, remote_host, remote_port)
        .await
        .map_err(crate::error::ApiError::from)?;
    let rollback_sessions = sessions.clone();
    validate_context_and_owner_or_rollback(
        application,
        lease,
        || sessions.owns_entry(entry),
        status,
        move |late| async move {
            rollback_sessions.tunnels.stop(&late.id);
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_start(
    state: State<'_, AppState>,
    session_id: String,
    name: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
) -> ApiResult<TunnelStatus> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let entry = state.sessions.get(&session_id)?;
    start_tunnel_for_context(
        &state.application,
        &lease,
        &state.sessions,
        &entry,
        &name,
        local_port,
        &remote_host,
        remote_port,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_stop(state: State<'_, AppState>, tunnel_id: String) -> ApiResult<()> {
    state.sessions.tunnels.stop(&tunnel_id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_list(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> ApiResult<Vec<TunnelStatus>> {
    Ok(state.sessions.tunnels.list(session_id.as_deref()))
}
