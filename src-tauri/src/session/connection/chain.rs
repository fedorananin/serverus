use std::collections::HashSet;

use crate::error::{AppError, AppResult};
use crate::vault::model::Protocol;
use crate::vault::VaultManager;

use super::super::ssh::Hop;

/// Build the jump chain (target last) from vault data, with cycle guard.
pub(super) fn build_chain(vault: &VaultManager, connection_id: &str) -> AppResult<Vec<Hop>> {
    let payload = vault.payload()?;
    let mut chain = Vec::new();
    let mut cursor = Some(connection_id.to_string());
    let mut seen = HashSet::new();
    while let Some(id) = cursor {
        if !seen.insert(id.clone()) {
            return Err(AppError::Connect("jump host cycle detected".into()));
        }
        if chain.len() >= 6 {
            return Err(AppError::Connect("jump chain too long".into()));
        }
        let connection = payload
            .connections
            .get(&id)
            .ok_or(AppError::ConnectionNotFound)?;
        if connection.protocol != Protocol::Ssh {
            return Err(AppError::Connect(
                "jump hosts and terminals require SSH connections".into(),
            ));
        }
        let known_host = payload
            .known_hosts
            .get(&format!("{}:{}", connection.host, connection.port))
            .cloned();
        chain.push(Hop::from_connection(connection, known_host));
        cursor = connection.jump_host.clone();
    }
    chain.reverse();
    Ok(chain)
}
