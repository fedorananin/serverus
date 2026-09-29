//! The agent activity journal: every tool call on a server shows up in the
//! UI of the tab it ran in, first as running, then with its outcome.

use std::time::{SystemTime, UNIX_EPOCH};

use tauri::AppHandle;

use super::ToolResult;
use crate::agent::hub::emit;
use crate::agent::types::{AgentActivityEntry, AgentActivityEvent, AgentActivityStatus};

/// Longest summary / detail kept in the journal.
const MAX_TEXT: usize = 600;

fn clip(text: &str) -> String {
    let mut chars = text.chars();
    let clipped: String = chars.by_ref().take(MAX_TEXT).collect();
    if chars.next().is_some() {
        format!("{clipped}…")
    } else {
        clipped
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// One journal entry in flight. Dropped without [`Journal::finish`] (the
/// agent cancelled the call or disconnected), it records the cancellation.
pub struct Journal<'a> {
    app: &'a AppHandle,
    event: AgentActivityEvent,
    finished: bool,
}

impl<'a> Journal<'a> {
    pub fn start(
        app: &'a AppHandle,
        tool: &str,
        summary: &str,
        session_id: Option<&str>,
        connection_id: &str,
    ) -> Self {
        let event = AgentActivityEvent {
            session_id: session_id.map(str::to_string),
            connection_id: Some(connection_id.to_string()),
            entry: AgentActivityEntry {
                id: uuid::Uuid::new_v4().to_string(),
                tool: tool.to_string(),
                summary: clip(summary),
                status: AgentActivityStatus::Running,
                detail: None,
                at_ms: now_ms(),
            },
        };
        emit(app, event.clone());
        Self {
            app,
            event,
            finished: false,
        }
    }

    /// Record the outcome and hand the result back.
    pub fn finish(mut self, result: ToolResult) -> ToolResult {
        let (status, detail) = match &result {
            Ok(_) => (AgentActivityStatus::Done, None),
            Err(error) if error.0.starts_with(DECLINED) => {
                (AgentActivityStatus::Denied, Some(clip(&error.0)))
            }
            Err(error) => (AgentActivityStatus::Failed, Some(clip(&error.0))),
        };
        self.record(status, detail);
        result
    }

    fn record(&mut self, status: AgentActivityStatus, detail: Option<String>) {
        self.finished = true;
        self.event.entry.status = status;
        self.event.entry.detail = detail;
        emit(self.app, self.event.clone());
    }
}

impl Drop for Journal<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.record(
                AgentActivityStatus::Cancelled,
                Some("The agent cancelled the call before it finished.".into()),
            );
        }
    }
}

/// Prefix of the error a declined confirmation produces.
pub const DECLINED: &str = "The user declined";
