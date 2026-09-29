//! IPC contracts between the agent backend and the UI: requests the backend
//! needs the UI to answer (open a tab, confirm an action), and the events
//! that keep the UI's agent indicators current.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Something only the UI can do or decide, answered via `agent_ui_respond`.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentUiRequest {
    /// Report the open session tabs.
    Tabs,
    /// Make sure a connected tab exists — the active one when
    /// `connection_id` is `None` — and report it. With `need_terminal` the
    /// reply also names the tab's active terminal.
    OpenTab {
        connection_id: Option<String>,
        need_terminal: bool,
    },
    /// Ask the user to allow one agent action.
    Confirm {
        connection_id: String,
        server: String,
        /// Short imperative summary ("Run a command").
        action: String,
        /// The exact command, paths, etc.
        detail: String,
    },
}

#[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
pub struct AgentUiRequestEvent {
    pub request_id: String,
    pub request: AgentUiRequest,
}

/// A request the UI was shown ended without an answer (timed out, or the
/// agent cancelled the call): drop its dialog.
#[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
pub struct AgentUiRequestExpiredEvent {
    pub request_id: String,
}

/// One session tab as the UI sees it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentTabInfo {
    pub tab_id: String,
    pub connection_id: String,
    pub session_id: Option<String>,
    /// `connecting` / `connected` / `error` / `disconnected`.
    pub state: String,
    pub active: bool,
    /// The tab's active terminal, when one is open.
    pub term_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentConfirmDecision {
    Deny,
    Once,
    /// Allow every action on this server for a while without asking.
    ForAWhile,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentUiResponse {
    Tabs { tabs: Vec<AgentTabInfo> },
    Tab { tab: AgentTabInfo },
    Confirm { decision: AgentConfirmDecision },
    Error { message: String },
}

/// Agent state of one terminal: whether the user holds it and which agent
/// command (if any) is running in it.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct AgentTerminalEvent {
    pub term_id: String,
    pub session_id: String,
    pub running: Option<String>,
    pub user_control: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentActivityStatus {
    Running,
    Done,
    Failed,
    Denied,
    /// The agent cancelled the call (or disconnected) before it finished.
    Cancelled,
}

/// One entry of the agent activity journal. The same `id` is sent again
/// when the action finishes.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentActivityEntry {
    pub id: String,
    pub tool: String,
    pub summary: String,
    pub status: AgentActivityStatus,
    pub detail: Option<String>,
    /// Unix milliseconds of the start.
    #[specta(type = specta_typescript::Number)]
    pub at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct AgentActivityEvent {
    /// The session the action ran in, when it has one.
    pub session_id: Option<String>,
    pub connection_id: Option<String>,
    pub entry: AgentActivityEntry,
}

/// The agent changed remote paths outside the transfer queue (mkdir,
/// rename, chmod): panes showing their parents should relist.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct AgentFsChangedEvent {
    pub session_id: String,
    pub paths: Vec<String>,
}

/// How to connect an MCP client to this Serverus.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AgentSetupInfo {
    /// Agent access is implemented on this platform.
    pub supported: bool,
    /// The socket is currently accepting agents.
    pub listening: bool,
    /// Why it is not listening, when it is not.
    pub problem: Option<String>,
    /// Executable to register as the MCP server (stdio transport).
    pub command: String,
    pub args: Vec<String>,
    /// Ready-to-paste `claude mcp add …` line.
    pub claude_code: String,
    /// Why `command` will not keep working (a temporary app location).
    pub command_warning: Option<String>,
}

/// The agent changed the vault (added a connection): the UI takes the new
/// secret-free vault and tells the user what happened.
#[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
pub struct AgentVaultChangedEvent {
    pub vault: crate::vault::model::PublicVault,
    pub summary: String,
}
