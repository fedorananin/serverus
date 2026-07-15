use std::sync::{Arc, Mutex};

use serverus_domain::runtime_context::RuntimeContextId;

use crate::error::{AppError, AppResult};

use super::state::{OpenAdmission, OpenAdmissionState};

#[derive(Default)]
pub(in crate::watcher) struct OpenAdmissionRegistry {
    pub(super) state: Mutex<OpenAdmissionState>,
    pub(super) changed: tokio::sync::Notify,
}

impl OpenAdmissionRegistry {
    pub(super) fn begin(
        self: &Arc<Self>,
        expected_context_id: RuntimeContextId,
        session_id: &str,
    ) -> AppResult<OpenAdmission> {
        let mut state = self.state.lock().unwrap();
        if !state.accepting || state.active_context_id != Some(expected_context_id) {
            return Err(AppError::WrongRuntimeContext);
        }
        let context_generation = state.context_generation;
        let session = state.sessions.entry(session_id.to_string()).or_default();
        if !session.accepting {
            return Err(AppError::SessionNotFound);
        }
        session.opens += 1;
        Ok(OpenAdmission {
            registry: self.clone(),
            session_id: session_id.to_string(),
            context_generation,
            generation: session.generation,
        })
    }

    pub(super) fn is_active(
        &self,
        session_id: &str,
        context_generation: u64,
        session_generation: u64,
    ) -> bool {
        let state = self.state.lock().unwrap();
        state.accepting
            && state.context_generation == context_generation
            && state.sessions.get(session_id).is_some_and(|session| {
                session.accepting && session.generation == session_generation
            })
    }

    pub(super) async fn wait_session_quiescent(&self, session_id: &str) {
        loop {
            let changed = self.changed.notified();
            let is_quiescent = self
                .state
                .lock()
                .unwrap()
                .sessions
                .get(session_id)
                .is_none_or(|session| session.opens == 0);
            if is_quiescent {
                return;
            }
            changed.await;
        }
    }

    pub(super) async fn wait_all_quiescent(&self) {
        loop {
            let changed = self.changed.notified();
            let is_quiescent = self
                .state
                .lock()
                .unwrap()
                .sessions
                .values()
                .all(|session| session.opens == 0);
            if is_quiescent {
                return;
            }
            changed.await;
        }
    }
}

impl OpenAdmission {
    pub(super) async fn cancelled(&self) {
        loop {
            let changed = self.registry.changed.notified();
            if !self
                .registry
                .is_active(&self.session_id, self.context_generation, self.generation)
            {
                return;
            }
            changed.await;
        }
    }
}

impl Drop for OpenAdmission {
    fn drop(&mut self) {
        let mut state = self.registry.state.lock().unwrap();
        if let Some(session) = state.sessions.get_mut(&self.session_id) {
            session.opens = session.opens.saturating_sub(1);
        }
        drop(state);
        self.registry.changed.notify_waiters();
    }
}
