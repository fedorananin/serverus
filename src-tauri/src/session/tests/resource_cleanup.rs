use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::super::{SessionManager, SessionResourceCleanup};
use super::entry;

struct RecordingCleanup {
    sessions: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl SessionResourceCleanup for RecordingCleanup {
    async fn clear_session(&self, session_id: &str) {
        self.sessions.lock().unwrap().push(session_id.to_string());
    }
}

fn manager_with_cleanup() -> (SessionManager, Arc<Mutex<Vec<String>>>) {
    let cleared = Arc::new(Mutex::new(Vec::new()));
    let manager = SessionManager::with_resource_cleanup(Arc::new(RecordingCleanup {
        sessions: cleared.clone(),
    }));
    (manager, cleared)
}

#[tokio::test]
async fn retiring_a_session_clears_every_session_scoped_resource() {
    let (manager, cleared) = manager_with_cleanup();
    let session = entry("session-a");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(session.id.clone(), session);

    let retired = manager.retire_registered_session("session-a", None).await;

    assert!(retired.is_some());
    assert!(manager.session_ids().is_empty());
    assert_eq!(*cleared.lock().unwrap(), ["session-a"]);
}

#[tokio::test]
async fn a_stale_watcher_cannot_retire_a_replacement_session() {
    let (manager, cleared) = manager_with_cleanup();
    let stale = entry("session-a");
    let current = entry("session-a");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(current.id.clone(), current.clone());

    let retired = manager
        .retire_registered_session("session-a", Some(&stale))
        .await;

    assert!(retired.is_none());
    assert!(manager.owns_entry(&current));
    assert!(cleared.lock().unwrap().is_empty());
}
