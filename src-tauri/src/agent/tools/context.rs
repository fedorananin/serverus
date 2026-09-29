//! What every tool call goes through: the vault must be unlocked and agent
//! access switched on, the named server must be shared with the agent, the
//! action must pass its access level (asking the user when needed), and the
//! work happens in a visible tab. The policy checks themselves live in
//! `authorize`.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serverus_domain::agent::access::{decide_create, AccessLevel, Decision};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use super::{ConnectionHold, ToolError};
use crate::agent::catalog::{catalog, resolve, ServerInfo};
use crate::agent::hub::AgentHub;
use crate::agent::types::{
    AgentTabInfo, AgentUiRequest, AgentUiRequestExpiredEvent, AgentUiResponse,
};
use crate::agent::unlock;
use crate::error::{AppError, AppResult};
use crate::session::remote_fs::RemoteFs;
use crate::session::SessionEntry;
use crate::state::AppState;
use crate::vault::model::AgentCreateMode;

const TABS_TIMEOUT: Duration = Duration::from_secs(10);
/// Opening a tab may include connecting and a host-key decision.
const OPEN_TAB_TIMEOUT: Duration = Duration::from_secs(180);

/// The vault-derived facts one tool call runs against.
pub struct Policy {
    pub servers: Vec<ServerInfo>,
    /// Whether the agent may add connections, and full access for all.
    pub(super) create_mode: AgentCreateMode,
    pub full_access: bool,
    /// Identifies this unlock; temporary grants die with it.
    pub(super) epoch: String,
}

impl Policy {
    /// How create_connection would go: `yes`, `with_confirmation` or `no`.
    pub fn adding_connections(&self) -> &'static str {
        match decide_create(self.create_mode.into(), self.full_access, false) {
            Decision::Allow => "yes",
            Decision::Confirm => "with_confirmation",
            Decision::Deny => "no",
        }
    }
}

/// A connected tab the agent works in.
pub struct TabRef {
    pub session_id: String,
    pub term_id: Option<String>,
    pub entry: Arc<SessionEntry>,
}

pub struct Ctx<'a> {
    pub app: &'a AppHandle,
    connection: &'a ConnectionHold,
}

impl<'a> Ctx<'a> {
    pub fn new(app: &'a AppHandle, connection: &'a ConnectionHold) -> Self {
        Self { app, connection }
    }

    pub fn state(&self) -> State<'a, AppState> {
        self.app.state::<AppState>()
    }

    pub fn hub(&self) -> Arc<AgentHub> {
        self.state().agent.clone()
    }

    pub async fn policy(&self) -> Result<Policy, ToolError> {
        unlock::wait_unlocked(self.app).await?;
        let state = self.state();
        let lease = state
            .application
            .require_unlocked()
            .map_err(AppError::from)?;
        let epoch = format!(
            "{}:{}",
            lease.context_id().get(),
            lease.vault_access_epoch().map_or(0, |epoch| epoch.get())
        );
        let vault = state.vault.lock().unwrap();
        let payload = vault.payload()?;
        if !payload.settings.agent.enabled {
            // A connected agent keeps the vault open only while it may work.
            self.connection.release();
            return Err(
                "AI agent access is turned off in Serverus. The user can enable it in Settings → AI Agent."
                    .into(),
            );
        }
        self.connection.engage(&state.activity);
        Ok(Policy {
            servers: catalog(payload),
            create_mode: payload.settings.agent.create_connections,
            full_access: payload.settings.agent.full_access,
            epoch,
        })
    }

    pub(super) async fn ui(
        &self,
        request: AgentUiRequest,
        timeout: Duration,
        timeout_message: &str,
    ) -> Result<AgentUiResponse, ToolError> {
        let app = self.app;
        self.hub()
            .bridge
            .request(
                request,
                timeout,
                timeout_message,
                |event| {
                    event
                        .emit(app)
                        .map_err(|error| AppError::Other(format!("cannot reach the UI: {error}")))
                },
                |request_id| {
                    let expired = AgentUiRequestExpiredEvent {
                        request_id: request_id.to_string(),
                    };
                    let _ = expired.emit(app);
                },
            )
            .await
            .map_err(Into::into)
    }

    pub async fn tabs(&self) -> Result<Vec<AgentTabInfo>, ToolError> {
        let timeout = format!(
            "Serverus did not answer within {}s — is its window open?",
            TABS_TIMEOUT.as_secs()
        );
        match self
            .ui(AgentUiRequest::Tabs, TABS_TIMEOUT, &timeout)
            .await?
        {
            AgentUiResponse::Tabs { tabs } => Ok(tabs),
            _ => Err("unexpected answer from the UI".into()),
        }
    }

    /// Resolve the `server` argument: a name / path / id, or `current`.
    pub async fn target(&self, policy: &Policy, spec: &str) -> Result<ServerInfo, ToolError> {
        if !spec.trim().eq_ignore_ascii_case("current") {
            return Ok(resolve(&policy.servers, spec)?.clone());
        }
        let tabs = self.tabs().await?;
        let active = tabs
            .iter()
            .find(|tab| tab.active)
            .ok_or("No tab is active in Serverus. Name a server instead (see list_servers).")?;
        let server = policy
            .servers
            .iter()
            .find(|server| server.id == active.connection_id)
            .filter(|server| server.level != AccessLevel::Off)
            .ok_or("The server in the current tab is not shared with you.")?;
        Ok(server.clone())
    }

    /// Make sure a connected tab for `server` exists (reusing the one the
    /// user has open) and return it.
    pub async fn tab(&self, server: &ServerInfo, need_terminal: bool) -> Result<TabRef, ToolError> {
        if need_terminal && !server.terminal {
            return Err(ToolError(format!(
                "`{}` has no shell (FTP, S3 or an SFTP-only SSH account). Use the file tools instead.",
                server.path()
            )));
        }
        let request = AgentUiRequest::OpenTab {
            connection_id: Some(server.id.clone()),
            need_terminal,
        };
        let timeout = format!(
            "`{}` did not connect within {}s: it may be unreachable, or a host-key decision is still waiting for the user in Serverus.",
            server.path(),
            OPEN_TAB_TIMEOUT.as_secs()
        );
        let tab = match self.ui(request, OPEN_TAB_TIMEOUT, &timeout).await? {
            AgentUiResponse::Tab { tab } => tab,
            _ => return Err("unexpected answer from the UI".into()),
        };
        let session_id = tab.session_id.ok_or("The tab has no connected session.")?;
        let state = self.state();
        let entry = state.sessions.get(&session_id)?;
        if entry.connection_id != server.id {
            return Err("The UI returned a tab of another server.".into());
        }
        if need_terminal {
            let term_id = tab
                .term_id
                .as_deref()
                .ok_or("The tab has no open terminal.")?;
            let (owner, _) = state.sessions.terminal_tap(term_id).await?;
            if owner != session_id {
                return Err("The UI returned a terminal of another tab.".into());
            }
        }
        Ok(TabRef {
            session_id,
            term_id: tab.term_id,
            entry,
        })
    }

    /// Run a remote-file operation under the session's operation admission
    /// (closing the tab or switching vaults waits for / cancels it).
    pub async fn on_fs<T, F, Fut>(&self, tab: &TabRef, operation: F) -> Result<T, ToolError>
    where
        F: FnOnce(Arc<dyn RemoteFs>) -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        self.state()
            .application
            .run_session_operation(&tab.session_id, |entry, _lease| async move {
                operation(entry.remote_fs().await?).await
            })
            .await
            .map_err(Into::into)
    }
}
