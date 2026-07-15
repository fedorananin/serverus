use crate::error::{AppError, AppResult};
use crate::vault::model::Protocol;
use crate::vault::VaultManager;

use super::super::{ftp, s3};
use super::chain::build_chain;
use super::plan::ConnectionPlan;

pub(crate) fn load_authorized_plan(
    vault: &VaultManager,
    connection_id: &str,
    lease: &serverus_runtime::ContextLease,
    application: &serverus_runtime::ApplicationHandle,
) -> AppResult<ConnectionPlan> {
    lease.validate(application).map_err(AppError::from)?;
    if vault.vault_id() != lease.vault().as_str() {
        return Err(AppError::WrongRuntimeContext);
    }
    load_plan(vault, connection_id)
}

fn load_plan(vault: &VaultManager, connection_id: &str) -> AppResult<ConnectionPlan> {
    let payload = vault.payload()?;
    let connection = payload
        .connections
        .get(connection_id)
        .ok_or(AppError::ConnectionNotFound)?;
    match connection.protocol {
        Protocol::Ssh => Ok(ConnectionPlan::Ssh {
            chain: build_chain(vault, connection_id)?,
            autostart_tunnels: connection
                .tunnels
                .iter()
                .filter(|tunnel| tunnel.autostart)
                .cloned()
                .collect(),
        }),
        Protocol::Ftp => Ok(ConnectionPlan::Ftp {
            config: ftp::FtpConfig::from_connection(connection)?,
            max_parallel: payload.settings.transfers.max_parallel_per_server as usize,
        }),
        Protocol::S3 => Ok(ConnectionPlan::S3 {
            config: s3::S3Config::from_connection(connection)?,
        }),
    }
}

#[cfg(test)]
#[path = "planning_tests.rs"]
mod tests;
