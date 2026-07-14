use std::sync::{Arc, Mutex};

use tokio::sync::{watch, Notify};

use crate::error::{AppError, AppResult};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Open,
    Closing,
    Closed,
}

struct LifecycleState {
    phase: Phase,
    operations: usize,
    cleanups: usize,
}

pub(crate) struct LifecycleGate {
    state: Mutex<LifecycleState>,
    changed: Notify,
    cancel: watch::Sender<bool>,
}

impl Default for LifecycleGate {
    fn default() -> Self {
        let (cancel, _) = watch::channel(false);
        Self {
            state: Mutex::new(LifecycleState {
                phase: Phase::Open,
                operations: 0,
                cleanups: 0,
            }),
            changed: Notify::new(),
            cancel,
        }
    }
}

impl LifecycleGate {
    pub(crate) fn try_begin_operation(self: &Arc<Self>) -> AppResult<SessionOperation> {
        let mut state = self.state.lock().unwrap();
        if state.phase != Phase::Open {
            return Err(AppError::SessionNotFound);
        }
        state.operations += 1;
        Ok(SessionOperation {
            lifecycle: self.clone(),
            cancel: self.cancel.subscribe(),
        })
    }

    pub(crate) fn start_closing(&self) -> bool {
        let changed = {
            let mut state = self.state.lock().unwrap();
            if state.phase != Phase::Open {
                false
            } else {
                state.phase = Phase::Closing;
                true
            }
        };
        if changed {
            let _ = self.cancel.send(true);
            self.changed.notify_waiters();
        }
        changed
    }

    pub(crate) async fn begin_close(self: &Arc<Self>) -> SessionCloseGuard {
        self.start_closing();
        loop {
            let notified = self.changed.notified();
            let drained = {
                let state = self.state.lock().unwrap();
                state.operations == 0 && state.cleanups == 0
            };
            if drained {
                break;
            }
            notified.await;
        }
        SessionCloseGuard {
            lifecycle: self.clone(),
            finished: false,
        }
    }

    pub(crate) fn cleanup(self: &Arc<Self>) -> LifecycleCleanup {
        LifecycleCleanup {
            lifecycle: self.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) async fn wait_closed(&self) {
        loop {
            let notified = self.changed.notified();
            if self.state.lock().unwrap().phase == Phase::Closed {
                return;
            }
            notified.await;
        }
    }
}

pub(crate) struct SessionOperation {
    lifecycle: Arc<LifecycleGate>,
    cancel: watch::Receiver<bool>,
}

impl SessionOperation {
    pub(crate) async fn cancelled(&self) {
        let mut cancel = self.cancel.clone();
        if !*cancel.borrow() {
            let _ = cancel.changed().await;
        }
    }

    pub(crate) fn cleanup(&self) -> LifecycleCleanup {
        self.lifecycle.cleanup()
    }

    pub(crate) fn register<T>(&self, register: impl FnOnce() -> T) -> AppResult<T> {
        let state = self.lifecycle.state.lock().unwrap();
        if state.phase != Phase::Open {
            return Err(AppError::SessionNotFound);
        }
        let value = register();
        drop(state);
        Ok(value)
    }
}

impl Drop for SessionOperation {
    fn drop(&mut self) {
        let mut state = self.lifecycle.state.lock().unwrap();
        state.operations = state.operations.saturating_sub(1);
        let drained = state.operations == 0;
        drop(state);
        if drained {
            self.lifecycle.changed.notify_waiters();
        }
    }
}

pub(crate) struct SessionCloseGuard {
    lifecycle: Arc<LifecycleGate>,
    finished: bool,
}

impl SessionCloseGuard {
    pub(crate) async fn finish(mut self) {
        loop {
            let notified = self.lifecycle.changed.notified();
            {
                let mut state = self.lifecycle.state.lock().unwrap();
                if state.operations == 0 && state.cleanups == 0 {
                    state.phase = Phase::Closed;
                    self.finished = true;
                    self.lifecycle.changed.notify_waiters();
                    return;
                }
            }
            notified.await;
        }
    }
}

impl Drop for SessionCloseGuard {
    fn drop(&mut self) {
        // An interrupted owner leaves the gate closed to new operations. The
        // owned teardown task is responsible for reaching `finish`.
        let _ = self.finished;
    }
}

#[derive(Clone)]
pub(crate) struct LifecycleCleanup {
    lifecycle: Arc<LifecycleGate>,
}

impl LifecycleCleanup {
    pub(crate) fn try_spawn<F>(&self, future: F) -> Result<(), F>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(runtime) => runtime,
            Err(_) => return Err(future),
        };
        {
            let mut state = self.lifecycle.state.lock().unwrap();
            if state.phase == Phase::Closed {
                return Err(future);
            }
            state.cleanups += 1;
        }
        let lifecycle = self.lifecycle.clone();
        let registration = CleanupRegistration {
            lifecycle: lifecycle.clone(),
        };
        runtime.spawn(async move {
            let _registration = registration;
            future.await;
        });
        Ok(())
    }
}

struct CleanupRegistration {
    lifecycle: Arc<LifecycleGate>,
}

impl Drop for CleanupRegistration {
    fn drop(&mut self) {
        let mut state = self.lifecycle.state.lock().unwrap();
        state.cleanups = state.cleanups.saturating_sub(1);
        drop(state);
        self.lifecycle.changed.notify_waiters();
    }
}

pub(super) type SessionLifecycle = LifecycleGate;
