mod chain;
mod plan;
mod planning;
mod ssh;
mod storage;

use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;
use tauri::AppHandle;
use tauri_specta::Event;

pub(crate) use self::plan::ConnectionPlan;
pub(crate) use self::planning::load_authorized_plan;
use super::ssh::HostKeyIssue;
use super::{SessionEntry, SessionManager};
use crate::error::AppResult;
use crate::events::SessionStateEvent;

pub(super) fn emit_session_state(
    app: &AppHandle,
    session_id: &str,
    connection_id: &str,
    state: &str,
    message: Option<String>,
) {
    let _ = SessionStateEvent {
        session_id: session_id.to_string(),
        connection_id: connection_id.to_string(),
        state: state.to_string(),
        message,
    }
    .emit(app);
}

impl SessionManager {
    /// Enter the network connector only with a fully materialized plan that
    /// the desktop lifecycle authorized against the current unlocked vault.
    pub(crate) async fn connect_authorized_plan(
        self: &Arc<Self>,
        expected_context_id: RuntimeContextId,
        app: &AppHandle,
        connection_id: &str,
        plan: ConnectionPlan,
    ) -> AppResult<Result<Arc<SessionEntry>, Box<HostKeyIssue>>> {
        self.run_connect_admitted(expected_context_id, || async move {
            match plan {
                ConnectionPlan::Ssh { chain, .. } => {
                    self.connect_ssh(app, connection_id, chain).await
                }
                storage => self
                    .connect_storage(app, connection_id, storage)
                    .await
                    .map(Ok),
            }
        })
        .await
    }
}
