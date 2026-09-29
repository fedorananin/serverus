//! Process-wide agent state shared by the MCP server and the Tauri commands.

use std::collections::HashMap;
use std::sync::Mutex;

use serverus_domain::agent::shell::ShellFlavor;
use tauri::AppHandle;

use super::bridge::UiBridge;
use super::control::ControlRegistry;
use super::grants::Grants;
use super::types::{AgentSetupInfo, AgentTerminalEvent, AgentUiResponse};
use crate::error::AppResult;
use crate::session::SessionManager;

/// Whether the local socket is serving agents.
#[derive(Debug, Clone)]
pub enum ListenStatus {
    Starting,
    Listening,
    Failed(String),
    Unsupported,
}

pub struct AgentHub {
    pub(crate) bridge: UiBridge,
    pub(crate) control: ControlRegistry,
    pub(crate) grants: Grants,
    /// Login-shell syntax per session, probed once.
    shells: Mutex<HashMap<String, ShellFlavor>>,
    listen: Mutex<ListenStatus>,
}

impl Default for AgentHub {
    fn default() -> Self {
        Self {
            bridge: UiBridge::default(),
            control: ControlRegistry::default(),
            grants: Grants::default(),
            shells: Mutex::default(),
            listen: Mutex::new(if cfg!(unix) {
                ListenStatus::Starting
            } else {
                ListenStatus::Unsupported
            }),
        }
    }
}

/// Publish an agent event to the UI. Delivery failures are not actionable.
pub(crate) fn emit<E>(app: &AppHandle, event: E)
where
    E: tauri_specta::Event + serde::Serialize + Clone,
{
    let _ = event.emit(app);
}

impl AgentHub {
    pub fn respond(&self, request_id: &str, response: AgentUiResponse) -> bool {
        self.bridge.respond(request_id, response)
    }

    pub fn terminal_states(&self) -> Vec<AgentTerminalEvent> {
        self.control.snapshot()
    }

    /// The user takes a terminal away from the agent (`on`) or hands it back.
    pub async fn set_user_control(
        &self,
        app: &AppHandle,
        sessions: &SessionManager,
        term_id: &str,
        on: bool,
    ) -> AppResult<()> {
        let (session_id, _) = sessions.terminal_tap(term_id).await?;
        emit(app, self.control.set_user_control(term_id, &session_id, on));
        Ok(())
    }

    /// Drop everything kept for a session that closed.
    pub(crate) fn forget_session(&self, session_id: &str) {
        self.shells.lock().unwrap().remove(session_id);
        self.control.forget_session(session_id);
    }

    pub(crate) fn shell_of(&self, session_id: &str) -> Option<ShellFlavor> {
        self.shells.lock().unwrap().get(session_id).copied()
    }

    pub(crate) fn remember_shell(&self, session_id: &str, flavor: ShellFlavor) {
        self.shells
            .lock()
            .unwrap()
            .insert(session_id.to_string(), flavor);
    }

    pub(crate) fn set_listen_status(&self, status: ListenStatus) {
        *self.listen.lock().unwrap() = status;
    }

    pub fn setup_info(&self) -> AgentSetupInfo {
        let status = self.listen.lock().unwrap().clone();
        let command = std::env::current_exe()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "serverus".into());
        let command_warning = unstable_location(&command);
        AgentSetupInfo {
            command_warning,
            supported: !matches!(status, ListenStatus::Unsupported),
            listening: matches!(status, ListenStatus::Listening),
            problem: match status {
                ListenStatus::Failed(problem) => Some(problem),
                ListenStatus::Unsupported => {
                    Some("AI agent access is not available on this platform yet.".into())
                }
                _ => None,
            },
            claude_code: format!(
                "claude mcp add --scope user serverus -- {} --mcp",
                shell_quote(&command)
            ),
            command,
            args: vec!["--mcp".into()],
        }
    }
}

/// Why the running executable's path would not work as a registered MCP
/// command after a restart: macOS runs quarantined apps from a random
/// App Translocation copy, and an app opened straight from its disk image
/// (`/Volumes/<image>/Serverus.app`) goes away with it.
pub(crate) fn unstable_location(command: &str) -> Option<String> {
    let translocated = command.contains("/AppTranslocation/");
    let mut parts = command.split('/');
    let on_image = matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(""), Some("Volumes"), Some(_), Some(app)) if app.ends_with(".app")
    );
    (translocated || on_image).then(|| {
        "Serverus is running from a disk image or a temporary location, so this path stops working once it quits. Move Serverus to /Applications (and clear the quarantine flag as the README describes), start it from there and copy the command again.".to_string()
    })
}

/// Quote a path for a POSIX shell command line when it needs it.
fn shell_quote(text: &str) -> String {
    let plain = text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+:@".contains(c));
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}
