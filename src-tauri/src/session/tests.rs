use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serverus_domain::runtime_context::RuntimeContextId;

use super::{SessionEntry, SessionManager};
use crate::error::AppError;
use crate::vault::model::Protocol;

mod operations;
mod resource_cleanup;
mod terminal_stream;

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn entry(id: &str) -> Arc<SessionEntry> {
    Arc::new(SessionEntry::storage(
        id.to_string(),
        format!("connection-{id}"),
        Protocol::Ftp,
        None,
        None,
    ))
}

#[test]
fn session_ids_returns_an_owned_snapshot() {
    let manager = SessionManager::default();
    manager
        .sessions
        .lock()
        .unwrap()
        .insert("session-b".into(), entry("session-b"));
    manager
        .sessions
        .lock()
        .unwrap()
        .insert("session-a".into(), entry("session-a"));

    let mut snapshot = manager.session_ids();
    manager.sessions.lock().unwrap().clear();
    snapshot.sort();

    assert_eq!(snapshot, ["session-a", "session-b"]);
    assert!(manager.session_ids().is_empty());
}

#[test]
fn session_ownership_requires_the_same_registered_entry() {
    let manager = SessionManager::default();
    let first = entry("session-a");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(first.id.clone(), first.clone());

    assert!(manager.owns_entry(&first));

    let replacement = entry("session-a");
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(replacement.id.clone(), replacement.clone());

    assert!(!manager.owns_entry(&first));
    assert!(manager.owns_entry(&replacement));
    manager.sessions.lock().unwrap().remove(&replacement.id);
    assert!(!manager.owns_entry(&replacement));
}

#[tokio::test]
async fn closing_a_context_cancels_and_awaits_its_in_flight_connect() {
    let manager = Arc::new(SessionManager::default());
    let context_id = RuntimeContextId::try_from(81_u128).unwrap();
    manager.activate_context(context_id);
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let operation_dropped = Arc::new(AtomicBool::new(false));
    let registered = Arc::new(AtomicBool::new(false));

    let connecting = tokio::spawn({
        let manager = manager.clone();
        let started = started.clone();
        let release = release.clone();
        let operation_dropped = operation_dropped.clone();
        let registered = registered.clone();
        async move {
            manager
                .run_connect_admitted(context_id, || async move {
                    let _drop_signal = DropSignal(operation_dropped);
                    started.notify_one();
                    release.notified().await;
                    registered.store(true, Ordering::SeqCst);
                    Ok(())
                })
                .await
        }
    });
    started.notified().await;

    tokio::time::timeout(Duration::from_secs(1), manager.close_context(context_id))
        .await
        .expect("context cleanup cancels and awaits the blocked connect");

    assert!(matches!(
        connecting.await.expect("connect task joined"),
        Err(AppError::WrongRuntimeContext)
    ));
    assert!(operation_dropped.load(Ordering::SeqCst));
    assert!(!registered.load(Ordering::SeqCst));
    release.notify_waiters();
}

#[tokio::test]
async fn a_stale_context_cannot_enter_a_new_connect_epoch() {
    let manager = Arc::new(SessionManager::default());
    let old_context = RuntimeContextId::try_from(82_u128).unwrap();
    let new_context = RuntimeContextId::try_from(83_u128).unwrap();
    manager.activate_context(old_context);
    manager.close_context(old_context).await;
    manager.activate_context(new_context);

    let stale_side_effect = Arc::new(AtomicBool::new(false));
    let stale_flag = stale_side_effect.clone();
    let error = manager
        .run_connect_admitted(old_context, || async move {
            stale_flag.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::WrongRuntimeContext));
    assert!(!stale_side_effect.load(Ordering::SeqCst));

    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let connecting = tokio::spawn({
        let manager = manager.clone();
        let started = started.clone();
        let release = release.clone();
        async move {
            manager
                .run_connect_admitted(new_context, || async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(())
                })
                .await
        }
    });
    started.notified().await;
    manager.activate_context(new_context);
    release.notify_one();
    connecting
        .await
        .expect("connect task joined")
        .expect("same-context activation is idempotent");
}
