use std::sync::Arc;

use super::registry::ActivityRegistry;
use super::state::AdmissionToken;

pub(in crate::transfer) struct ProducerAdmission {
    registry: Arc<ActivityRegistry>,
    token: AdmissionToken,
}

impl ProducerAdmission {
    pub(super) fn new(registry: Arc<ActivityRegistry>, token: AdmissionToken) -> Self {
        Self { registry, token }
    }

    pub(in crate::transfer) fn token(&self) -> AdmissionToken {
        self.token.clone()
    }

    pub(in crate::transfer) async fn cancelled(&self) {
        loop {
            let changed = self.registry.changed.notified();
            let active = {
                let state = self.registry.state.lock().unwrap();
                ActivityRegistry::token_is_active(&state, &self.token)
            };
            if !active {
                return;
            }
            changed.await;
        }
    }
}

impl Drop for ProducerAdmission {
    fn drop(&mut self) {
        let mut state = self.registry.state.lock().unwrap();
        if let Some(session) = state.sessions.get_mut(&self.token.session_id) {
            session.producers = session.producers.saturating_sub(1);
        }
        drop(state);
        self.registry.changed.notify_waiters();
    }
}
