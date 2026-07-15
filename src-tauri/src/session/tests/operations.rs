use std::future::pending;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serverus_domain::runtime_context::RuntimeContextId;

use super::super::SessionManager;
use super::entry;
use crate::error::{AppError, AppResult};

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn context_id(value: u128) -> RuntimeContextId {
    RuntimeContextId::try_from(value).unwrap()
}

async fn wait_until_context_is_revoked(
    manager: &SessionManager,
    context: RuntimeContextId,
    session: &Arc<super::super::SessionEntry>,
) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let result = manager
                .run_session_operation(context, session, || async { Ok(()) })
                .await;
            if matches!(result, Err(AppError::WrongRuntimeContext)) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("context admission was not revoked");
}

#[tokio::test]
async fn closing_context_cancels_and_awaits_in_flight_remote_operation() {
    let manager = Arc::new(SessionManager::default());
    let context = context_id(91);
    manager.activate_context(context);
    let session = entry("session");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(session.id.clone(), session.clone());
    let started = Arc::new(tokio::sync::Notify::new());
    let operation_dropped = Arc::new(AtomicBool::new(false));

    let operation = tokio::spawn({
        let manager = manager.clone();
        let started = started.clone();
        let operation_dropped = operation_dropped.clone();
        async move {
            manager
                .run_session_operation(context, &session, || async move {
                    let _drop_signal = DropSignal(operation_dropped);
                    started.notify_one();
                    pending::<AppResult<()>>().await
                })
                .await
        }
    });
    started.notified().await;

    tokio::time::timeout(Duration::from_secs(1), manager.close_context(context))
        .await
        .expect("context cleanup cancels and awaits the remote operation");

    assert!(matches!(
        operation.await.expect("operation task joined"),
        Err(AppError::WrongRuntimeContext)
    ));
    assert!(operation_dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn disconnect_cancels_only_that_sessions_remote_operations() {
    let manager = Arc::new(SessionManager::default());
    let context = context_id(92);
    manager.activate_context(context);
    for session_id in ["first", "second"] {
        let session = entry(session_id);
        manager
            .sessions
            .lock()
            .unwrap()
            .insert(session.id.clone(), session);
    }
    let (started, mut starts) = tokio::sync::mpsc::channel(2);

    let first = tokio::spawn({
        let manager = manager.clone();
        let started = started.clone();
        let session = manager.get("first").unwrap();
        async move {
            manager
                .run_session_operation(context, &session, || async move {
                    started.send("first").await.unwrap();
                    pending::<AppResult<()>>().await
                })
                .await
        }
    });
    let second = tokio::spawn({
        let manager = manager.clone();
        let session = manager.get("second").unwrap();
        async move {
            manager
                .run_session_operation(context, &session, || async move {
                    started.send("second").await.unwrap();
                    pending::<AppResult<()>>().await
                })
                .await
        }
    });
    starts.recv().await.unwrap();
    starts.recv().await.unwrap();

    manager.disconnect("first").await;

    assert!(matches!(
        first.await.expect("first operation joined"),
        Err(AppError::SessionNotFound)
    ));
    assert!(!second.is_finished());
    manager.close_context(context).await;
    assert!(matches!(
        second.await.expect("second operation joined"),
        Err(AppError::WrongRuntimeContext)
    ));
}

#[tokio::test]
async fn stale_context_is_rejected_before_remote_operation_starts() {
    let manager = SessionManager::default();
    let stale = context_id(93);
    let active = context_id(94);
    manager.activate_context(active);
    let session = entry("session");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(session.id.clone(), session.clone());
    let started = Arc::new(AtomicBool::new(false));
    let operation_started = started.clone();

    let error = manager
        .run_session_operation(stale, &session, || async move {
            operation_started.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::WrongRuntimeContext));
    assert!(!started.load(Ordering::SeqCst));
}

#[tokio::test]
async fn unowned_session_is_rejected_before_remote_operation_starts() {
    let manager = SessionManager::default();
    let context = context_id(95);
    manager.activate_context(context);
    let session = entry("not-registered");
    let started = Arc::new(AtomicBool::new(false));
    let operation_started = started.clone();

    let error = manager
        .run_session_operation(context, &session, || async move {
            operation_started.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::SessionNotFound));
    assert!(!started.load(Ordering::SeqCst));
}

#[tokio::test]
async fn context_cleanup_awaits_blocking_operation_and_suppresses_its_stale_result() {
    let manager = Arc::new(SessionManager::default());
    let context = context_id(96);
    manager.activate_context(context);
    let session = entry("session");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(session.id.clone(), session.clone());
    let (started, started_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let side_effect_finished = Arc::new(AtomicBool::new(false));

    let operation = tokio::spawn({
        let manager = manager.clone();
        let session = session.clone();
        let side_effect_finished = side_effect_finished.clone();
        async move {
            manager
                .run_session_blocking_operation(context, &session, move || {
                    let _ = started.send(());
                    release_rx.recv().unwrap();
                    side_effect_finished.store(true, Ordering::SeqCst);
                    Err::<&'static str, _>(AppError::Other("late blocking failure".into()))
                })
                .await
        }
    });
    started_rx.await.unwrap();

    let cleanup = tokio::spawn({
        let manager = manager.clone();
        async move { manager.close_context(context).await }
    });
    wait_until_context_is_revoked(&manager, context, &session).await;
    assert!(!cleanup.is_finished());
    assert!(!operation.is_finished());

    release.send(()).unwrap();
    cleanup.await.expect("cleanup task joined");
    assert!(matches!(
        operation.await.expect("operation task joined"),
        Err(AppError::WrongRuntimeContext)
    ));
    assert!(side_effect_finished.load(Ordering::SeqCst));
}

#[tokio::test]
async fn dropped_caller_does_not_detach_blocking_operation_from_cleanup() {
    let manager = Arc::new(SessionManager::default());
    let context = context_id(97);
    manager.activate_context(context);
    let session = entry("session");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(session.id.clone(), session.clone());
    let (started, started_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let side_effect_finished = Arc::new(AtomicBool::new(false));

    let caller = tokio::spawn({
        let manager = manager.clone();
        let session = session.clone();
        let side_effect_finished = side_effect_finished.clone();
        async move {
            manager
                .run_session_blocking_operation(context, &session, move || {
                    let _ = started.send(());
                    release_rx.recv().unwrap();
                    side_effect_finished.store(true, Ordering::SeqCst);
                    Ok(())
                })
                .await
        }
    });
    started_rx.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());

    let cleanup = tokio::spawn({
        let manager = manager.clone();
        async move { manager.close_context(context).await }
    });
    wait_until_context_is_revoked(&manager, context, &session).await;
    assert!(!cleanup.is_finished());

    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), cleanup)
        .await
        .expect("cleanup remained attached to blocking work")
        .expect("cleanup task joined");
    assert!(side_effect_finished.load(Ordering::SeqCst));
}
