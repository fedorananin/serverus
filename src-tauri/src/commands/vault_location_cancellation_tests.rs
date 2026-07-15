//! Cancellation-atomic vault location regressions.

use std::sync::{mpsc, Arc};
use std::time::Duration;

use serverus_runtime::RuntimeError;

use super::tests::unlocked_state;
use super::{set_vault_path_application, switch_vault_application};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_switch_future_cannot_restore_work_after_selection_commits() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original.serverus");
    let target = directory.path().join("other.serverus");
    let state = Arc::new(unlocked_state(original));
    let old_lease = state.application.require_active().unwrap();
    let persist_started = Arc::new(tokio::sync::Notify::new());
    let started_for_task = persist_started.clone();
    let (release_tx, release_rx) = mpsc::channel();
    let task_state = state.clone();
    let task_target = target.clone();

    let command = tokio::spawn(async move {
        switch_vault_application(&task_state.application, task_target, move |_| {
            started_for_task.notify_one();
            release_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("test releases persisted selection");
            Ok(())
        })
        .await
    });

    persist_started.notified().await;
    command.abort();
    assert!(command.await.unwrap_err().is_cancelled());
    release_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let selected = state.vault.lock().unwrap().path() == target;
            let retired = state.application.require_active() == Err(RuntimeError::NoActiveContext);
            if selected && retired && old_lease.is_cancelled() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("owned switch must finish after its caller is dropped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_move_future_cannot_leave_runtime_at_the_previous_path() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original.serverus");
    let target = directory.path().join("moved.serverus");
    let state = Arc::new(unlocked_state(original));
    let context_id = state.application.require_active().unwrap().context_id();
    let persist_started = Arc::new(tokio::sync::Notify::new());
    let started_for_task = persist_started.clone();
    let (release_tx, release_rx) = mpsc::channel();
    let task_state = state.clone();
    let task_target = target.clone();

    let command = tokio::spawn(async move {
        set_vault_path_application(&task_state.application, task_target, move |_| {
            started_for_task.notify_one();
            release_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("test releases persisted path");
            Ok(())
        })
        .await
    });

    persist_started.notified().await;
    command.abort();
    assert!(command.await.unwrap_err().is_cancelled());
    release_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let selected = state.vault.lock().unwrap().path() == target;
            let runtime_matches = matches!(
                state
                    .application
                    .activate_selected_vault(target.to_string_lossy().into_owned()),
                Ok(current) if current == context_id
            );
            if selected && runtime_matches {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("owned move must reidentify the runtime after its caller is dropped");
}
