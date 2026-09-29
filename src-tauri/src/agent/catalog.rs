//! The agent's view of the vault: every connection with its folder path and
//! effective access level, plus name resolution. Never a secret — and never
//! the free-form notes either, where people tend to jot down passwords.

use serde::Serialize;
use serverus_domain::agent::access::{effective_level, AccessLevel};

use crate::vault::model::{AgentAccessLevel, Protocol, TreeNode, VaultPayload};

/// One connection as an agent may see it.
#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub id: String,
    pub name: String,
    /// Enclosing folder names, outermost first.
    pub folders: Vec<String>,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip)]
    pub level: AccessLevel,
    /// Whether the connection has an interactive shell.
    pub terminal: bool,
}

impl ServerInfo {
    /// `Folder/Sub/Name`.
    pub fn path(&self) -> String {
        let mut parts = self.folders.clone();
        parts.push(self.name.clone());
        parts.join("/")
    }

    pub fn access_label(&self) -> &'static str {
        access_label(self.level)
    }
}

/// How tools name an access level to the agent.
pub fn access_label(level: AccessLevel) -> &'static str {
    match level {
        AccessLevel::Off => "off",
        AccessLevel::ReadOnly => "read_only",
        AccessLevel::Ask => "ask",
        AccessLevel::Full => "full",
    }
}

/// Every connection in tree order with its effective level.
pub fn catalog(payload: &VaultPayload) -> Vec<ServerInfo> {
    let full_for_all = payload.settings.agent.full_access;
    let mut out = Vec::new();
    let mut folders = Vec::new();
    walk(payload, &payload.tree, &mut folders, full_for_all, &mut out);
    out
}

type FolderFrame = (String, Option<AgentAccessLevel>);

fn walk(
    payload: &VaultPayload,
    nodes: &[TreeNode],
    folders: &mut Vec<FolderFrame>,
    full_for_all: bool,
    out: &mut Vec<ServerInfo>,
) {
    for node in nodes {
        match node {
            TreeNode::Folder {
                name,
                children,
                agent_access,
                ..
            } => {
                folders.push((name.clone(), *agent_access));
                walk(payload, children, folders, full_for_all, out);
                folders.pop();
            }
            TreeNode::Connection { id } => {
                let Some(connection) = payload.connections.get(id) else {
                    continue;
                };
                let ancestors = folders.iter().rev().map(|(_, level)| level.map(Into::into));
                out.push(ServerInfo {
                    id: id.clone(),
                    name: connection.name.clone(),
                    folders: folders.iter().map(|(name, _)| name.clone()).collect(),
                    protocol: connection.protocol,
                    host: connection.host.clone(),
                    port: connection.port,
                    username: connection.auth.username.clone(),
                    level: effective_level(
                        connection.agent_access.map(Into::into),
                        ancestors,
                        full_for_all,
                    ),
                    terminal: connection.protocol == Protocol::Ssh && !connection.disable_terminal,
                });
            }
        }
    }
}

/// Find the one visible server `spec` names: its id, its `Folder/Name`
/// path, or its name (exact first, then case-insensitive).
pub fn resolve<'a>(servers: &'a [ServerInfo], spec: &str) -> Result<&'a ServerInfo, String> {
    let spec = spec.trim();
    let visible: Vec<&ServerInfo> = servers
        .iter()
        .filter(|server| server.level != AccessLevel::Off)
        .collect();
    let rules: [&dyn Fn(&ServerInfo) -> bool; 4] = [
        &|server| server.id == spec,
        &|server| server.path() == spec,
        &|server| server.name == spec,
        &|server| {
            server.name.eq_ignore_ascii_case(spec) || server.path().eq_ignore_ascii_case(spec)
        },
    ];
    for rule in rules {
        let matches: Vec<&ServerInfo> = visible.iter().copied().filter(|s| rule(s)).collect();
        match matches.as_slice() {
            [one] => return Ok(one),
            [] => continue,
            many => {
                let names: Vec<String> = many
                    .iter()
                    .map(|server| format!("{} (id {})", server.path(), server.id))
                    .collect();
                return Err(format!(
                    "`{spec}` matches several servers: {}. Use the folder path or the id.",
                    names.join(", ")
                ));
            }
        }
    }
    Err(format!(
        "No server named `{spec}` is available to the agent. Call list_servers to see the ones that are."
    ))
}
