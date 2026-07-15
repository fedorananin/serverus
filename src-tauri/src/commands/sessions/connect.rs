use super::super::prelude::*;
use super::super::tunnels::start_tunnel_for_context;

/// Open a session from one lifecycle-authorized, fully materialized plan.
#[tauri::command]
#[specta::specta]
pub async fn session_connect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
) -> ApiResult<SessionDto> {
    let lease = state
        .application
        .require_unlocked()
        .map_err(AppError::from)?;
    let vault_access_epoch = lease
        .vault_access_epoch()
        .ok_or(AppError::WrongRuntimeContext)?
        .get()
        .to_string();
    let sessions = state.sessions.clone();
    let plan_lease = lease.clone();
    let plan_runtime = state.application.clone();
    let plan_connection_id = connection_id.clone();
    let plan =
        run_unlocked_vault_operation_for_lease(&state.application, lease.clone(), move |manager| {
            crate::session::load_authorized_plan(
                manager,
                &plan_connection_id,
                &plan_lease,
                &plan_runtime,
            )
        })
        .await?;
    let autostart = plan.autostart_tunnels().to_vec();
    let outcome = tokio::select! {
        biased;
        _ = lease.cancelled() => Err(AppError::WrongRuntimeContext),
        outcome = sessions.connect_authorized_plan(
            lease.context_id(),
            &app,
            &connection_id,
            plan,
        ) => outcome,
    };
    if let Err(error) = lease.validate(&state.application) {
        if let Ok(Ok(entry)) = &outcome {
            sessions.disconnect(&entry.id).await;
        }
        return Err(AppError::from(error).into());
    }
    map_connect_outcome(
        &state.application,
        &sessions,
        lease,
        vault_access_epoch,
        autostart,
        outcome,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn map_connect_outcome(
    application: &crate::state::DesktopApplication,
    sessions: &std::sync::Arc<crate::session::SessionManager>,
    lease: serverus_runtime::ContextLease,
    vault_access_epoch: String,
    autostart: Vec<crate::vault::model::TunnelConfig>,
    outcome: AppResult<
        Result<
            std::sync::Arc<crate::session::SessionEntry>,
            Box<crate::session::ssh::HostKeyIssue>,
        >,
    >,
) -> ApiResult<SessionDto> {
    match outcome {
        Ok(Ok(entry)) => {
            for tunnel in autostart {
                let _ = start_tunnel_for_context(
                    application,
                    &lease,
                    sessions,
                    &entry,
                    &tunnel.name,
                    tunnel.local_port,
                    &tunnel.remote_host,
                    tunnel.remote_port,
                )
                .await;
            }
            let dto = SessionDto {
                session_id: entry.id.clone(),
                connection_id: entry.connection_id.clone(),
            };
            let owner = entry.clone();
            let rollback_sessions = sessions.clone();
            let rollback_session_id = entry.id.clone();
            validate_context_and_owner_or_rollback(
                application,
                &lease,
                || sessions.owns_entry(&owner),
                dto,
                move |_dto| async move {
                    rollback_sessions.disconnect(&rollback_session_id).await;
                },
            )
            .await
        }
        Ok(Err(issue)) => Err(crate::error::ApiError {
            code: "host_key_prompt".into(),
            message: if issue.changed {
                format!(
                    "HOST KEY CHANGED for {}:{} — possible man-in-the-middle attack",
                    issue.host, issue.port
                )
            } else {
                format!("Unknown host {}:{}", issue.host, issue.port)
            },
            host_key: Some(Box::new(crate::error::HostKeyPrompt {
                runtime_context_id: lease.context_id().get().to_string(),
                vault_access_epoch,
                host: issue.host,
                port: issue.port,
                algorithm: issue.algorithm,
                fingerprint: issue.fingerprint,
                key_line: issue.key_line,
                changed: issue.changed,
            })),
        }),
        Err(error) => Err(error.into()),
    }
}
