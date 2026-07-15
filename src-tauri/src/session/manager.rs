use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::{AppError, AppResult};

use super::admission::ConnectAdmissionRegistry;
use super::operation_admission::SessionOperationRegistry;
use super::ssh::SshSession;
use super::terminal::TerminalEntry;
use super::{tunnel, SessionEntry, SessionResourceCleanup};

/// Owns the registries for live sessions and their session-scoped resources.
#[derive(Default)]
pub struct SessionManager {
    pub(super) sessions: Mutex<HashMap<String, Arc<SessionEntry>>>,
    pub(super) terminals: tokio::sync::Mutex<HashMap<String, TerminalEntry>>,
    pub tunnels: tunnel::TunnelManager,
    pub(super) connect_admissions: Arc<ConnectAdmissionRegistry>,
    pub(super) operation_admissions: Arc<SessionOperationRegistry>,
    resource_cleanup: Option<Arc<dyn SessionResourceCleanup>>,
}

impl SessionManager {
    pub(crate) fn with_resource_cleanup(cleanup: Arc<dyn SessionResourceCleanup>) -> Self {
        Self {
            resource_cleanup: Some(cleanup),
            ..Self::default()
        }
    }

    /// Return an owned snapshot of the currently registered session IDs.
    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.lock().unwrap().keys().cloned().collect()
    }

    pub fn get(&self, session_id: &str) -> AppResult<Arc<SessionEntry>> {
        self.sessions
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .ok_or(AppError::SessionNotFound)
    }

    /// A captured entry owns session-scoped registrations only while the
    /// registry still points at that exact connection instance.
    pub fn owns_entry(&self, entry: &Arc<SessionEntry>) -> bool {
        self.sessions
            .lock()
            .unwrap()
            .get(&entry.id)
            .is_some_and(|current| Arc::ptr_eq(current, entry))
    }

    pub fn ssh_of(&self, session_id: &str) -> AppResult<Arc<SshSession>> {
        self.get(session_id)?
            .ssh
            .clone()
            .ok_or_else(|| AppError::Other("not an SSH session".into()))
    }

    pub async fn disconnect(&self, session_id: &str) {
        let entry = self.retire_registered_session(session_id, None).await;
        if let Some(ssh) = entry.and_then(|entry| entry.ssh.clone()) {
            let handle = ssh.handle.lock().await;
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        }
    }

    /// Atomically claim one registered session and drain all of its children.
    /// An expected entry prevents a stale disconnect watcher from retiring a
    /// newer owner that happens to use the same identifier.
    pub(super) async fn retire_registered_session(
        &self,
        session_id: &str,
        expected: Option<&Arc<SessionEntry>>,
    ) -> Option<Arc<SessionEntry>> {
        let entry = {
            let mut sessions = self.sessions.lock().unwrap();
            let can_retire = sessions.get(session_id).is_some_and(|current| {
                expected.is_none_or(|expected| Arc::ptr_eq(current, expected))
            });
            can_retire.then(|| sessions.remove(session_id)).flatten()
        };
        let entry = entry?;

        self.operation_admissions.close_session(session_id).await;
        self.tunnels.stop_session(session_id);
        self.close_session_terminals(session_id).await;
        if let Some(cleanup) = &self.resource_cleanup {
            cleanup.clear_session(session_id).await;
        }
        Some(entry)
    }
}
