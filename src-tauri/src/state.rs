//! Global app state managed by Tauri.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri_specta::Event;

use crate::app_config;
use crate::autolock::ActivityTracker;
use crate::error::{AppError, AppResult};
use crate::events::SessionStateEvent;
use crate::runtime_context::RuntimeContext;
use crate::session::SessionEntry;
use crate::session::SessionManager;
use crate::transfer::TransferManager;
use crate::vault::quick_unlock::QuickUnlock;
use crate::vault::VaultManager;
use crate::watcher::EditWatcher;

pub struct AppState {
    /// Std mutex: all vault work happens inside `spawn_blocking`, never
    /// held across an await point.
    pub vault: Arc<Mutex<VaultManager>>,
    pub quick: Arc<dyn QuickUnlock>,
    pub sessions: Arc<SessionManager>,
    pub transfers: Arc<TransferManager>,
    pub edits: Arc<EditWatcher>,
    pub runtime_context: Arc<RuntimeContext>,
    pub activity: Arc<ActivityTracker>,
    disconnects: Arc<SessionDisconnectCoordinator>,
}

#[derive(Default)]
struct DisconnectCompletion {
    outcome: Mutex<Option<Result<(), String>>>,
    changed: tokio::sync::Notify,
}

#[derive(Clone)]
pub(crate) struct SessionDisconnect {
    completion: Arc<DisconnectCompletion>,
}

impl SessionDisconnect {
    fn complete(&self, outcome: Result<(), String>) {
        *self.completion.outcome.lock().unwrap() = Some(outcome);
        self.completion.changed.notify_waiters();
    }

    pub(crate) async fn wait(&self) -> AppResult<()> {
        loop {
            let changed = self.completion.changed.notified();
            if let Some(outcome) = self.completion.outcome.lock().unwrap().clone() {
                return outcome.map_err(AppError::Other);
            }
            changed.await;
        }
    }
}

async fn catch_teardown_panic(
    teardown: impl std::future::Future<Output = ()>,
) -> Result<(), String> {
    use futures::FutureExt;

    std::panic::AssertUnwindSafe(teardown)
        .catch_unwind()
        .await
        .map_err(|_| "session teardown task panicked".to_string())
}

pub(crate) struct DisconnectStart {
    pub(crate) completion: SessionDisconnect,
    pub(crate) initiated: bool,
}

struct SessionDisconnectCoordinator {
    sessions: Arc<SessionManager>,
    transfers: Arc<TransferManager>,
    edits: Arc<EditWatcher>,
    in_flight: Mutex<HashMap<String, SessionDisconnect>>,
}

fn start_remote_disconnect(
    coordinator: &Arc<SessionDisconnectCoordinator>,
    session_id: &str,
) -> Option<DisconnectStart> {
    coordinator.start(session_id)
}

async fn run_disconnect_watchdog(
    ready: tokio::sync::oneshot::Receiver<()>,
    remote_closed: impl std::future::Future<Output = ()>,
    coordinator: Arc<SessionDisconnectCoordinator>,
    session_id: String,
    notify_disconnected: impl FnOnce(),
) {
    if ready.await.is_err() {
        return;
    }
    remote_closed.await;
    if start_remote_disconnect(&coordinator, &session_id).is_some_and(|started| started.initiated) {
        notify_disconnected();
    }
}

impl SessionDisconnectCoordinator {
    fn start(self: &Arc<Self>, session_id: &str) -> Option<DisconnectStart> {
        let mut in_flight = self.in_flight.lock().unwrap();
        if let Some(completion) = in_flight.get(session_id) {
            return Some(DisconnectStart {
                completion: completion.clone(),
                initiated: false,
            });
        }

        let closing = self.sessions.start_disconnect(session_id)?;
        let completion = SessionDisconnect {
            completion: Arc::new(DisconnectCompletion::default()),
        };
        in_flight.insert(session_id.to_string(), completion.clone());
        drop(in_flight);

        let coordinator = self.clone();
        let session_id = session_id.to_string();
        let owner_completion = completion.clone();
        tokio::spawn(async move {
            let outcome = catch_teardown_panic(async {
                tokio::join!(
                    coordinator.edits.close_session(&session_id),
                    coordinator.transfers.clear_session(&session_id),
                );
                coordinator.sessions.finish_disconnect(closing).await;
            })
            .await;
            coordinator.finish(&session_id, &owner_completion, outcome);
        });

        Some(DisconnectStart {
            completion,
            initiated: true,
        })
    }

    fn finish(
        &self,
        session_id: &str,
        completion: &SessionDisconnect,
        outcome: Result<(), String>,
    ) {
        completion.complete(outcome.clone());
        if outcome.is_ok() {
            self.in_flight.lock().unwrap().remove(session_id);
        }
        // A failed teardown remains as a fail-closed tombstone. Returning the
        // same error on repeated requests is safer than pretending a removed
        // session finished releasing all of its resources.
    }

    async fn disconnect_all(self: &Arc<Self>) -> AppResult<()> {
        for session_id in self.sessions.session_ids() {
            let _ = self.start(&session_id);
        }
        // Snapshot after starting every live session so remote-close races and
        // already-running tab teardowns are included in the same barrier.
        let completions: Vec<_> = self.in_flight.lock().unwrap().values().cloned().collect();
        let results = futures::future::join_all(
            completions
                .into_iter()
                .map(|completion| async move { completion.wait().await }),
        )
        .await;
        let mut first_error = None;
        for result in results {
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        let quick: Arc<dyn QuickUnlock> = Arc::new(crate::vault::quick_unlock::MacQuickUnlock);
        #[cfg(target_os = "windows")]
        let quick: Arc<dyn QuickUnlock> = Arc::new(crate::vault::quick_unlock::WindowsQuickUnlock);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let quick: Arc<dyn QuickUnlock> = Arc::new(crate::vault::quick_unlock::NoQuickUnlock);

        let runtime_context = Arc::new(RuntimeContext::default());
        Self::with_parts(
            Arc::new(Mutex::new(VaultManager::new(app_config::vault_path()))),
            quick,
            Arc::new(SessionManager::new(runtime_context.clone())),
            Arc::new(TransferManager::new(runtime_context.clone())),
            Arc::new(EditWatcher::default()),
            Arc::new(ActivityTracker::default()),
            runtime_context,
        )
    }

    pub(crate) fn with_parts(
        vault: Arc<Mutex<VaultManager>>,
        quick: Arc<dyn QuickUnlock>,
        sessions: Arc<SessionManager>,
        transfers: Arc<TransferManager>,
        edits: Arc<EditWatcher>,
        activity: Arc<ActivityTracker>,
        runtime_context: Arc<RuntimeContext>,
    ) -> Self {
        let disconnects = Arc::new(SessionDisconnectCoordinator {
            sessions: sessions.clone(),
            transfers: transfers.clone(),
            edits: edits.clone(),
            in_flight: Mutex::new(HashMap::new()),
        });

        AppState {
            vault,
            quick,
            sessions,
            transfers,
            edits,
            runtime_context,
            activity,
            disconnects,
        }
    }

    pub(crate) fn start_session_disconnect(&self, session_id: &str) -> Option<DisconnectStart> {
        self.disconnects.start(session_id)
    }

    pub(crate) async fn disconnect_all_sessions(&self) -> AppResult<()> {
        self.disconnects.disconnect_all().await
    }

    pub(crate) async fn install_ssh_watchdog(
        &self,
        app: tauri::AppHandle,
        entry: &Arc<SessionEntry>,
    ) -> AppResult<()> {
        let Some(ssh) = entry.ssh.clone() else {
            return Ok(());
        };
        let session_id = entry.id.clone();
        let connection_id = entry.connection_id.clone();
        let context_epoch = entry.context_epoch;
        let disconnects = self.disconnects.clone();
        let (ready, ready_rx) = tokio::sync::oneshot::channel();
        let remote_closed = async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                if ssh.is_closed().await {
                    break;
                }
            }
        };
        let watchdog_session_id = session_id.clone();
        let watchdog = tokio::spawn(run_disconnect_watchdog(
            ready_rx,
            remote_closed,
            disconnects,
            watchdog_session_id,
            move || {
                let _ = SessionStateEvent {
                    context_epoch,
                    session_id,
                    connection_id,
                    state: "disconnected".into(),
                    message: None,
                }
                .emit(&app);
            },
        ));
        self.sessions.install_watchdog(&entry.id, watchdog)?;
        let _ = ready.send(());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::{
        catch_teardown_panic, run_disconnect_watchdog, start_remote_disconnect,
        DisconnectCompletion, SessionDisconnect, SessionDisconnectCoordinator,
    };
    use crate::session::SessionManager;
    use crate::transfer::TransferManager;
    use crate::watcher::EditWatcher;

    #[tokio::test]
    async fn remote_close_uses_one_owned_teardown_after_a_waiter_is_aborted() {
        struct StopSignal(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for StopSignal {
            fn drop(&mut self) {
                if let Some(signal) = self.0.take() {
                    let _ = signal.send(());
                }
            }
        }

        let sessions = Arc::new(SessionManager::default());
        let transfers = Arc::new(TransferManager::default());
        let edits = Arc::new(EditWatcher::default());
        let coordinator = Arc::new(SessionDisconnectCoordinator {
            sessions: sessions.clone(),
            transfers: transfers.clone(),
            edits: edits.clone(),
            in_flight: std::sync::Mutex::new(std::collections::HashMap::new()),
        });
        let lifecycle = sessions.insert_test_session("session");
        let operation = sessions.get("session").unwrap();
        let transfer_stopped = transfers.insert_test_task("session");
        let edit_stopped = edits.insert_test_watch("session");
        let (ready, ready_rx) = tokio::sync::oneshot::channel();
        let (remote_closed, remote_closed_rx) = tokio::sync::oneshot::channel();
        let (disconnected, disconnected_rx) = tokio::sync::oneshot::channel();
        let (watchdog_stopped, watchdog_stopped_rx) = tokio::sync::oneshot::channel();
        let watchdog_coordinator = coordinator.clone();
        let watchdog = tokio::spawn(async move {
            let _stopped = StopSignal(Some(watchdog_stopped));
            run_disconnect_watchdog(
                ready_rx,
                async move {
                    let _ = remote_closed_rx.await;
                },
                watchdog_coordinator,
                "session".into(),
                move || {
                    let _ = disconnected.send(());
                },
            )
            .await;
        });
        sessions.install_watchdog("session", watchdog).unwrap();
        ready.send(()).unwrap();
        remote_closed.send(()).unwrap();
        disconnected_rx.await.unwrap();

        let second = start_remote_disconnect(&coordinator, "session").unwrap();
        assert!(!second.initiated);
        let duplicate = start_remote_disconnect(&coordinator, "session").unwrap();
        assert!(!duplicate.initiated);
        assert!(Arc::ptr_eq(
            &duplicate.completion.completion,
            &second.completion.completion
        ));
        operation.operation().cancelled().await;
        assert!(sessions.get("session").is_err());

        let waiter = tokio::spawn({
            let completion = duplicate.completion.clone();
            async move { completion.wait().await }
        });
        tokio::task::yield_now().await;
        waiter.abort();
        let _ = waiter.await;
        assert!(second
            .completion
            .completion
            .outcome
            .lock()
            .unwrap()
            .is_none());

        drop(operation);
        tokio::time::timeout(Duration::from_secs(2), second.completion.wait())
            .await
            .expect("owned teardown stopped with its aborted waiter")
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), lifecycle.wait_closed())
            .await
            .expect("session lifecycle did not reach closed");
        tokio::time::timeout(Duration::from_secs(1), transfer_stopped)
            .await
            .expect("transfer task was not joined")
            .expect("transfer stop signal was dropped");
        tokio::time::timeout(Duration::from_secs(1), edit_stopped)
            .await
            .expect("remote edit task was not joined")
            .expect("remote edit stop signal was dropped");
        tokio::time::timeout(Duration::from_secs(1), watchdog_stopped_rx)
            .await
            .expect("watchdog task was not joined")
            .expect("watchdog stop signal was dropped");
    }

    #[tokio::test]
    async fn owner_panic_is_reported_as_a_failed_disconnect() {
        let completion = SessionDisconnect {
            completion: Arc::new(DisconnectCompletion::default()),
        };
        let outcome = catch_teardown_panic(async {
            panic!("simulated teardown panic");
        })
        .await;
        completion.complete(outcome);

        let error = completion.wait().await.unwrap_err();
        assert!(error.to_string().contains("teardown task panicked"));
    }

    #[tokio::test]
    async fn failed_disconnect_is_retained_for_repeated_requests() {
        let sessions = Arc::new(SessionManager::default());
        let coordinator = Arc::new(SessionDisconnectCoordinator {
            sessions,
            transfers: Arc::new(TransferManager::default()),
            edits: Arc::new(EditWatcher::default()),
            in_flight: std::sync::Mutex::new(std::collections::HashMap::new()),
        });
        let completion = SessionDisconnect {
            completion: Arc::new(DisconnectCompletion::default()),
        };
        coordinator
            .in_flight
            .lock()
            .unwrap()
            .insert("session".into(), completion.clone());

        coordinator.finish(
            "session",
            &completion,
            Err("simulated teardown failure".into()),
        );

        let repeated = coordinator.start("session").unwrap();
        assert!(!repeated.initiated);
        assert!(Arc::ptr_eq(
            &completion.completion,
            &repeated.completion.completion
        ));
        let error = repeated.completion.wait().await.unwrap_err();
        assert!(error.to_string().contains("simulated teardown failure"));
    }
}
