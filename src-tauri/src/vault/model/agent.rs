use serde::{Deserialize, Serialize};
use specta::Type;

/// Persisted AI-agent access level of a connection or folder. `None` in the
/// owning field means "inherit from the enclosing folder" (and, at the top,
/// `Off`). The policy itself lives in `serverus_domain::agent::access`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentAccessLevel {
    Off,
    ReadOnly,
    Ask,
    Full,
}

impl From<AgentAccessLevel> for serverus_domain::agent::access::AccessLevel {
    fn from(level: AgentAccessLevel) -> Self {
        use serverus_domain::agent::access::AccessLevel;
        match level {
            AgentAccessLevel::Off => AccessLevel::Off,
            AgentAccessLevel::ReadOnly => AccessLevel::ReadOnly,
            AgentAccessLevel::Ask => AccessLevel::Ask,
            AgentAccessLevel::Full => AccessLevel::Full,
        }
    }
}

impl From<serverus_domain::agent::access::AccessLevel> for AgentAccessLevel {
    fn from(level: serverus_domain::agent::access::AccessLevel) -> Self {
        use serverus_domain::agent::access::AccessLevel;
        match level {
            AccessLevel::Off => AgentAccessLevel::Off,
            AccessLevel::ReadOnly => AgentAccessLevel::ReadOnly,
            AccessLevel::Ask => AgentAccessLevel::Ask,
            AccessLevel::Full => AgentAccessLevel::Full,
        }
    }
}

/// Whether an agent may add connections to the vault.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentCreateMode {
    #[default]
    Off,
    /// Every new connection needs the user's confirmation.
    Ask,
    Allowed,
}

impl From<AgentCreateMode> for serverus_domain::agent::access::CreateMode {
    fn from(mode: AgentCreateMode) -> Self {
        use serverus_domain::agent::access::CreateMode;
        match mode {
            AgentCreateMode::Off => CreateMode::Off,
            AgentCreateMode::Ask => CreateMode::Ask,
            AgentCreateMode::Allowed => CreateMode::Allowed,
        }
    }
}

/// Vault-wide AI agent (MCP) settings. Everything is off by default and for
/// vaults written before the feature existed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct AgentSettings {
    /// Serve agents at all. While off, every agent request is refused.
    #[serde(default)]
    pub enabled: bool,
    /// Treat every connection as `Full`, regardless of its own level: no
    /// confirmations, no hidden servers.
    #[serde(default)]
    pub full_access: bool,
    /// Whether the agent may add connections (with whatever secrets the
    /// user asked it to store). Full access implies `Allowed`.
    #[serde(default)]
    pub create_connections: AgentCreateMode,
}
