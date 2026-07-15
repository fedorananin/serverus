use std::sync::{Arc, Mutex};

use serverus_domain::runtime_context::RuntimeContextId;
use tokio::sync::Notify;

use super::admission::ProducerAdmission;
use super::state::{ActivityState, AdmissionToken};

#[derive(Default)]
pub(in crate::transfer) struct ActivityRegistry {
    pub(in crate::transfer) state: Mutex<ActivityState>,
    pub(in crate::transfer) changed: Notify,
}

impl ActivityRegistry {
    pub(in crate::transfer) fn token_is_active(
        state: &ActivityState,
        token: &AdmissionToken,
    ) -> bool {
        state.accepting
            && state.active_context_id == Some(token.context_id)
            && state.context_generation == token.context_generation
            && state
                .sessions
                .get(&token.session_id)
                .is_some_and(|session| {
                    session.accepting && session.generation == token.session_generation
                })
    }

    pub(in crate::transfer) fn begin_producer(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        session_id: &str,
    ) -> Option<ProducerAdmission> {
        let mut state = self.state.lock().unwrap();
        if !state.accepting || state.active_context_id != Some(context_id) {
            return None;
        }
        let context_generation = state.context_generation;
        let session = state.sessions.entry(session_id.to_string()).or_default();
        if !session.accepting {
            return None;
        }
        session.producers += 1;
        let token = AdmissionToken {
            context_id,
            context_generation,
            session_generation: session.generation,
            session_id: session_id.to_string(),
        };
        Some(ProducerAdmission::new(self.clone(), token))
    }

    pub(in crate::transfer) async fn wait_all_quiescent(&self) {
        loop {
            let changed = self.changed.notified();
            let is_quiescent = {
                let state = self.state.lock().unwrap();
                state.tasks.is_empty()
                    && state
                        .sessions
                        .values()
                        .all(|session| session.producers == 0)
            };
            if is_quiescent {
                return;
            }
            changed.await;
        }
    }

    pub(in crate::transfer) async fn wait_session_quiescent(&self, session_id: &str) {
        loop {
            let changed = self.changed.notified();
            let is_quiescent = {
                let state = self.state.lock().unwrap();
                let producers = state
                    .sessions
                    .get(session_id)
                    .map_or(0, |session| session.producers);
                let has_tasks = state
                    .tasks
                    .values()
                    .any(|task| task.admission.session_id == session_id);
                producers == 0 && !has_tasks
            };
            if is_quiescent {
                return;
            }
            changed.await;
        }
    }
}
