use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;

use super::super::ssh::{self as protocol_ssh, ConnectOutcome, Hop, HostKeyIssue, SshSession};
use super::super::{SessionEntry, SessionManager};
use super::emit_session_state;
use crate::error::AppResult;

impl SessionManager {
    /// Connect an SSH session for `connection_id`. On an unknown/changed host
    /// key the caller receives the prompt payload instead of a session.
    pub(super) async fn connect_ssh(
        self: &Arc<Self>,
        app: &AppHandle,
        connection_id: &str,
        chain: Vec<Hop>,
    ) -> AppResult<Result<Arc<SessionEntry>, Box<HostKeyIssue>>> {
        let session_id = uuid::Uuid::new_v4().to_string();
        emit_session_state(app, &session_id, connection_id, "connecting", None);

        // Stage messages stream to the UI so a slow connect does not look frozen.
        let progress = {
            let app = app.clone();
            let session_id = session_id.clone();
            let connection_id = connection_id.to_string();
            move |message: String| {
                emit_session_state(
                    &app,
                    &session_id,
                    &connection_id,
                    "connecting",
                    Some(message),
                );
            }
        };

        match protocol_ssh::connect_chain_with_progress(&chain, &progress).await {
            Ok(ConnectOutcome::Connected(handle)) => {
                let entry = Arc::new(SessionEntry::ssh(
                    session_id.clone(),
                    connection_id.to_string(),
                    SshSession {
                        handle: tokio::sync::Mutex::new(handle),
                    },
                ));
                self.sessions
                    .lock()
                    .unwrap()
                    .insert(session_id.clone(), entry.clone());
                emit_session_state(app, &session_id, connection_id, "connected", None);
                self.watch_disconnect(app, connection_id, &session_id, &entry);
                Ok(Ok(entry))
            }
            Ok(ConnectOutcome::HostKeyPrompt(issue)) => Ok(Err(issue)),
            Err(error) => {
                emit_session_state(
                    app,
                    &session_id,
                    connection_id,
                    "error",
                    Some(error.to_string()),
                );
                Err(error)
            }
        }
    }

    fn watch_disconnect(
        self: &Arc<Self>,
        app: &AppHandle,
        connection_id: &str,
        session_id: &str,
        entry: &Arc<SessionEntry>,
    ) {
        let manager = self.clone();
        let ssh = entry.ssh.clone().expect("SSH entry has a session handle");
        let expected_entry = entry.clone();
        let app = app.clone();
        let connection_id = connection_id.to_string();
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if !ssh.handle.lock().await.is_closed() {
                    continue;
                }
                if manager
                    .retire_registered_session(&session_id, Some(&expected_entry))
                    .await
                    .is_some()
                {
                    emit_session_state(&app, &session_id, &connection_id, "disconnected", None);
                }
                break;
            }
        });
    }
}
