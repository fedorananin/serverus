use std::future::pending;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AppError, AppResult};

use super::super::{
    open_download_root, ActivityRegistry, Control, LocalDownloadTarget, TransferManager,
};
use super::support::{context_id, insert, item, start, TestFs};

#[test]
fn activating_the_same_runtime_context_keeps_existing_admissions_valid() {
    let manager = TransferManager::default();
    let context = context_id(70);
    manager.activate_context(context);
    let admission = manager
        .activity
        .begin_producer(context, "session")
        .expect("context admits producer");
    let token = admission.token();

    manager.activate_context(context);

    let state = manager.activity.state.lock().unwrap();
    assert!(ActivityRegistry::token_is_active(&state, &token));
}

#[test]
fn stale_admission_cannot_register_after_a_new_context_is_activated() {
    let manager = TransferManager::default();
    let old_context = context_id(71);
    manager.activate_context(old_context);
    let admission = manager
        .activity
        .begin_producer(old_context, "session")
        .expect("old context admits producer");
    let token = admission.token();

    manager.activate_context(context_id(72));

    assert!(manager.activity.reserve_task(&token).is_none());
}

#[tokio::test]
async fn stale_command_cannot_enter_the_new_context_admission_epoch() {
    let manager = TransferManager::default();
    let old_context = context_id(76);
    manager.activate_context(old_context);
    manager.activate_context(context_id(77));
    let operation_started = Arc::new(AtomicBool::new(false));
    let started = operation_started.clone();

    let result = manager
        .run_admitted(old_context, "session", |_| async move {
            started.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

    assert!(matches!(result, Err(AppError::WrongRuntimeContext)));
    assert!(!operation_started.load(Ordering::SeqCst));
}

#[tokio::test]
async fn clear_all_drains_every_registry_before_signalling_active_work() {
    let manager = TransferManager::default();
    let transfer = item("session");
    start(&transfer);
    let receiver = transfer.control.subscribe();
    insert(&manager, transfer);

    manager.clear_all().await;

    assert!(manager.snapshot().items.is_empty());
    assert_eq!(*receiver.borrow(), Control::Cancel);
    assert!(manager.queues.lock().unwrap().is_empty());
    assert!(manager.activity.state.lock().unwrap().tasks.is_empty());
}

#[tokio::test]
async fn clear_all_aborts_tracked_tasks_and_releases_their_old_fs() {
    let manager = Arc::new(TransferManager::default());
    let context = context_id(73);
    manager.activate_context(context);
    let admission = manager
        .activity
        .begin_producer(context, "session")
        .expect("context admits producer");
    let token = admission.token();
    let task_guard = manager
        .activity
        .reserve_task(&token)
        .expect("active admission reserves task");
    let task_id = task_guard.id();
    let fs = Arc::new(TestFs::default());
    let weak_fs = Arc::downgrade(&fs);
    let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
    let task_fs = fs.clone();
    let handle = tokio::spawn(async move {
        let _guard = task_guard;
        let _fs = task_fs;
        let _ = entered_sender.send(());
        pending::<()>().await;
    });
    manager
        .activity
        .attach_abort(task_id, handle.abort_handle());
    drop(admission);
    drop(fs);
    entered_receiver.await.unwrap();

    manager.clear_all().await;

    assert!(weak_fs.upgrade().is_none());
    assert!(manager.activity.state.lock().unwrap().tasks.is_empty());
}

#[tokio::test]
async fn clear_all_allows_worker_to_remove_partial_target_before_abort_fallback() {
    let manager = Arc::new(TransferManager::default());
    let context = context_id(78);
    manager.activate_context(context);
    let admission = manager
        .activity
        .begin_producer(context, "session")
        .expect("context admits producer");
    let task_guard = manager
        .activity
        .reserve_task(&admission.token())
        .expect("active admission reserves task");
    let task_id = task_guard.id();
    let transfer = item("session");
    start(&transfer);
    let directory = tempfile::tempdir().unwrap();
    let partial_target = directory.path().join("partial-upload");
    std::fs::write(&partial_target, b"partial").unwrap();
    transfer.mark_local_partial(LocalDownloadTarget {
        root: open_download_root(directory.path()).unwrap(),
        relative: "partial-upload".into(),
    });
    insert(&manager, transfer);
    let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        let _guard = task_guard;
        let _ = entered_sender.send(());
        pending::<()>().await;
    });
    manager
        .activity
        .attach_abort(task_id, handle.abort_handle());
    drop(admission);
    entered_receiver.await.unwrap();

    manager.clear_all().await;

    assert!(!partial_target.exists(), "partial target survived cleanup");
}

#[tokio::test]
async fn clear_all_bounds_best_effort_remote_partial_cleanup() {
    let manager = TransferManager::default();
    let transfer = super::support::item_with_fs("session", Arc::new(TestFs::hanging_delete()));
    transfer.mark_remote_partial("/partial-upload".into());
    insert(&manager, transfer);

    let result = tokio::time::timeout(Duration::from_secs(2), manager.clear_all()).await;

    assert!(result.is_ok(), "remote cleanup blocked context retirement");
}

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn clear_all_cancels_a_blocked_producer_and_waits_for_quiescence() {
    let manager = Arc::new(TransferManager::default());
    let context = context_id(74);
    manager.activate_context(context);
    let dropped = Arc::new(AtomicBool::new(false));
    let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
    let task_manager = manager.clone();
    let task_dropped = dropped.clone();
    let producer = tokio::spawn(async move {
        task_manager
            .run_admitted(context, "session", |_| async move {
                let _drop_flag = DropFlag(task_dropped);
                let _ = entered_sender.send(());
                pending::<AppResult<()>>().await
            })
            .await
    });
    entered_receiver.await.unwrap();

    manager.clear_all().await;

    assert!(matches!(
        producer.await.unwrap(),
        Err(AppError::WrongRuntimeContext)
    ));
    assert!(dropped.load(Ordering::SeqCst));
    assert!(manager.activity.state.lock().unwrap().tasks.is_empty());
}

#[tokio::test]
async fn clear_session_cancels_only_that_sessions_blocked_producers() {
    let manager = Arc::new(TransferManager::default());
    let context = context_id(75);
    manager.activate_context(context);
    let (entered_sender, mut entered_receiver) = tokio::sync::mpsc::channel(2);

    let first_manager = manager.clone();
    let first_sender = entered_sender.clone();
    let first = tokio::spawn(async move {
        first_manager
            .run_admitted(context, "first", |_| async move {
                first_sender.send("first").await.unwrap();
                pending::<AppResult<()>>().await
            })
            .await
    });
    let second_manager = manager.clone();
    let second = tokio::spawn(async move {
        second_manager
            .run_admitted(context, "second", |_| async move {
                entered_sender.send("second").await.unwrap();
                pending::<AppResult<()>>().await
            })
            .await
    });
    entered_receiver.recv().await.unwrap();
    entered_receiver.recv().await.unwrap();

    manager.clear_session("first").await;

    assert!(matches!(
        first.await.unwrap(),
        Err(AppError::WrongRuntimeContext)
    ));
    assert!(!second.is_finished());
    manager.clear_all().await;
    let second_result = tokio::time::timeout(Duration::from_millis(100), second)
        .await
        .expect("second producer is released")
        .unwrap();
    assert!(matches!(second_result, Err(AppError::WrongRuntimeContext)));
}
