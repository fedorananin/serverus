use serverus_domain::runtime_context::RuntimeContextId;

use super::registry::OpenAdmissionRegistry;

impl OpenAdmissionRegistry {
    pub(in crate::watcher) fn activate_context(&self, context_id: RuntimeContextId) {
        let mut state = self.state.lock().unwrap();
        if state.active_context_id == Some(context_id) && state.accepting {
            return;
        }
        state.context_generation = state.context_generation.wrapping_add(1);
        state.active_context_id = Some(context_id);
        state.accepting = true;
        state.sessions.clear();
        drop(state);
        self.changed.notify_waiters();
    }

    pub(in crate::watcher) async fn close_session(&self, session_id: &str) {
        {
            let mut state = self.state.lock().unwrap();
            let session = state.sessions.entry(session_id.to_string()).or_default();
            session.generation = session.generation.wrapping_add(1);
            session.accepting = false;
        }
        self.changed.notify_waiters();
        self.wait_session_quiescent(session_id).await;
    }

    pub(in crate::watcher) async fn close_all(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.context_generation = state.context_generation.wrapping_add(1);
            state.active_context_id = None;
            state.accepting = false;
            for session in state.sessions.values_mut() {
                session.generation = session.generation.wrapping_add(1);
                session.accepting = false;
            }
        }
        self.changed.notify_waiters();
        self.wait_all_quiescent().await;
    }
}
