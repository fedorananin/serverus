use std::sync::{Arc, Mutex};

use serverus_domain::runtime_context::RuntimeContextId;

use crate::error::{AppError, AppResult};

use super::state::{ConnectAdmission, ConnectAdmissionState};

#[derive(Default)]
pub(in crate::session) struct ConnectAdmissionRegistry {
    pub(super) state: Mutex<ConnectAdmissionState>,
    pub(super) changed: tokio::sync::Notify,
}

impl ConnectAdmissionRegistry {
    pub(super) fn begin(
        self: &Arc<Self>,
        expected_context_id: RuntimeContextId,
    ) -> AppResult<ConnectAdmission> {
        let mut state = self.state.lock().unwrap();
        if !state.accepting || state.active_context_id != Some(expected_context_id) {
            return Err(AppError::WrongRuntimeContext);
        }
        state.in_flight += 1;
        Ok(ConnectAdmission {
            registry: self.clone(),
            generation: state.generation,
        })
    }

    pub(super) fn is_active(&self, generation: u64) -> bool {
        let state = self.state.lock().unwrap();
        state.accepting && state.generation == generation
    }

    pub(super) async fn wait_quiescent(&self) {
        loop {
            let changed = self.changed.notified();
            if self.state.lock().unwrap().in_flight == 0 {
                return;
            }
            changed.await;
        }
    }
}

impl ConnectAdmission {
    pub(super) async fn cancelled(&self) {
        loop {
            let changed = self.registry.changed.notified();
            if !self.registry.is_active(self.generation) {
                return;
            }
            changed.await;
        }
    }
}

impl Drop for ConnectAdmission {
    fn drop(&mut self) {
        let mut state = self.registry.state.lock().unwrap();
        state.in_flight = state.in_flight.saturating_sub(1);
        drop(state);
        self.registry.changed.notify_waiters();
    }
}
