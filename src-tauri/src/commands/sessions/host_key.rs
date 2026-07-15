use super::super::prelude::*;

pub(super) async fn accept_host_key_for_context(
    application: &crate::state::DesktopApplication,
    runtime_context_id: String,
    vault_access_epoch: String,
    host: String,
    port: u16,
    key_line: String,
) -> ApiResult<()> {
    let lease = application
        .require_unlocked()
        .map_err(AppError::from)
        .map_err(crate::error::ApiError::from)?;
    if runtime_context_id != lease.context_id().get().to_string() {
        return Err(AppError::WrongRuntimeContext.into());
    }
    let current_access_epoch = lease
        .vault_access_epoch()
        .ok_or(AppError::WrongRuntimeContext)?
        .get()
        .to_string();
    if vault_access_epoch != current_access_epoch {
        return Err(AppError::WrongRuntimeContext.into());
    }

    run_unlocked_vault_operation_for_lease(application, lease, move |manager| {
        manager.with_payload(|payload| {
            payload
                .known_hosts
                .insert(format!("{host}:{port}"), key_line);
            Ok(())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn host_key_accept(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    key_line: String,
    runtime_context_id: String,
    vault_access_epoch: String,
) -> ApiResult<()> {
    accept_host_key_for_context(
        &state.application,
        runtime_context_id,
        vault_access_epoch,
        host,
        port,
        key_line,
    )
    .await
}
