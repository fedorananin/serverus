use std::future::Future;

use serverus_domain::runtime_context::RuntimeContextId;

use crate::error::{AppError, AppResult};

use super::super::SessionManager;

impl SessionManager {
    pub(in crate::session) async fn run_connect_admitted<T, F, Fut>(
        &self,
        expected_context_id: RuntimeContextId,
        operation: F,
    ) -> AppResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        let admission = self.connect_admissions.begin(expected_context_id)?;
        tokio::select! {
            biased;
            _ = admission.cancelled() => Err(AppError::WrongRuntimeContext),
            result = operation() => result,
        }
    }

    /// Open a fresh session-connect epoch after the previous context retired.
    pub fn activate_context(&self, context_id: RuntimeContextId) {
        self.operation_admissions.activate_context(context_id);
        let mut state = self.connect_admissions.state.lock().unwrap();
        if state.active_context_id == Some(context_id) && state.accepting {
            return;
        }
        state.generation = state.generation.wrapping_add(1);
        state.active_context_id = Some(context_id);
        state.accepting = true;
        drop(state);
        self.connect_admissions.changed.notify_waiters();
    }

    /// Stop new connects and await every connect admitted by this context.
    pub async fn close_context(&self, context_id: RuntimeContextId) {
        self.operation_admissions.close_context(context_id).await;
        let should_wait = {
            let mut state = self.connect_admissions.state.lock().unwrap();
            match state.active_context_id {
                Some(active) if active == context_id => {
                    state.generation = state.generation.wrapping_add(1);
                    state.active_context_id = None;
                    state.accepting = false;
                    true
                }
                None if !state.accepting => true,
                _ => false,
            }
        };
        if should_wait {
            self.connect_admissions.changed.notify_waiters();
            self.connect_admissions.wait_quiescent().await;
        }
    }
}
