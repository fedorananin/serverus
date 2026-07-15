use std::sync::{Arc, Mutex};

use serverus_domain::runtime_context::RuntimeContextId;

use crate::error::{AppError, AppResult};

use super::state::{OperationAdmission, OperationState};

#[derive(Default)]
pub(in crate::session) struct SessionOperationRegistry {
    state: Mutex<OperationState>,
    changed: tokio::sync::Notify,
}

impl SessionOperationRegistry {
    pub(super) fn begin(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        session_id: &str,
    ) -> AppResult<OperationAdmission> {
        let mut state = self.state.lock().unwrap();
        if !state.accepting || state.active_context_id != Some(context_id) {
            return Err(AppError::WrongRuntimeContext);
        }
        let context_generation = state.context_generation;
        let session = state.sessions.entry(session_id.to_string()).or_default();
        if !session.accepting {
            return Err(AppError::SessionNotFound);
        }
        session.in_flight += 1;
        Ok(OperationAdmission {
            registry: self.clone(),
            context_id,
            context_generation,
            session_id: session_id.to_string(),
            session_generation: session.generation,
        })
    }

    pub(super) fn cancellation_error(&self, admission: &OperationAdmission) -> Option<AppError> {
        let state = self.state.lock().unwrap();
        if !state.accepting
            || state.active_context_id != Some(admission.context_id)
            || state.context_generation != admission.context_generation
        {
            return Some(AppError::WrongRuntimeContext);
        }
        match state.sessions.get(&admission.session_id) {
            Some(session)
                if session.accepting && session.generation == admission.session_generation =>
            {
                None
            }
            _ => Some(AppError::SessionNotFound),
        }
    }

    pub(in crate::session) fn activate_context(&self, context_id: RuntimeContextId) {
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

    pub(in crate::session) async fn close_context(&self, context_id: RuntimeContextId) {
        let should_wait = {
            let mut state = self.state.lock().unwrap();
            match state.active_context_id {
                Some(active) if active == context_id => {
                    state.context_generation = state.context_generation.wrapping_add(1);
                    state.active_context_id = None;
                    state.accepting = false;
                    for session in state.sessions.values_mut() {
                        session.generation = session.generation.wrapping_add(1);
                        session.accepting = false;
                    }
                    true
                }
                None if !state.accepting => true,
                _ => false,
            }
        };
        if should_wait {
            self.changed.notify_waiters();
            self.wait_all_quiescent().await;
        }
    }

    pub(in crate::session) async fn close_session(&self, session_id: &str) {
        {
            let mut state = self.state.lock().unwrap();
            let session = state.sessions.entry(session_id.to_string()).or_default();
            session.generation = session.generation.wrapping_add(1);
            session.accepting = false;
        }
        self.changed.notify_waiters();
        self.wait_session_quiescent(session_id).await;
    }

    async fn wait_all_quiescent(&self) {
        loop {
            let changed = self.changed.notified();
            let quiescent = self
                .state
                .lock()
                .unwrap()
                .sessions
                .values()
                .all(|session| session.in_flight == 0);
            if quiescent {
                return;
            }
            changed.await;
        }
    }

    async fn wait_session_quiescent(&self, session_id: &str) {
        loop {
            let changed = self.changed.notified();
            let quiescent = self
                .state
                .lock()
                .unwrap()
                .sessions
                .get(session_id)
                .is_none_or(|session| session.in_flight == 0);
            if quiescent {
                return;
            }
            changed.await;
        }
    }
}

impl OperationAdmission {
    pub(super) async fn cancelled(&self) -> AppError {
        loop {
            let changed = self.registry.changed.notified();
            if let Some(error) = self.registry.cancellation_error(self) {
                return error;
            }
            changed.await;
        }
    }
}

impl Drop for OperationAdmission {
    fn drop(&mut self) {
        let mut state = self.registry.state.lock().unwrap();
        if let Some(session) = state.sessions.get_mut(&self.session_id) {
            session.in_flight = session.in_flight.saturating_sub(1);
        }
        drop(state);
        self.registry.changed.notify_waiters();
    }
}
