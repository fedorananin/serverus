use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serverus_application::context::{ContextCleanup, ContextCleanupError, ContextEvent};
use serverus_domain::runtime_context::{RuntimeContextId, VaultKey};
use serverus_runtime::{ApplicationHandle, RuntimeError};
use serverus_testkit::{DeterministicContextIds, RecordedContextCleanup, RecordedContextEvents};

use super::support::{
    handle, FixedId, GatedCleanup, NoopEvents, PanickingGatedCleanup, RecordedCleanup,
    RecordedEvents,
};

#[test]
fn activation_is_not_visible_until_child_admissions_are_ready() {
    let handle = Arc::new(handle());
    let activating = handle.clone();
    let (prepare_started_tx, prepare_started_rx) = std::sync::mpsc::channel();
    let (release_prepare_tx, release_prepare_rx) = std::sync::mpsc::channel();
    let activation = std::thread::spawn(move || {
        activating.activate_vault_with(VaultKey::new("primary").unwrap(), |_| {
            prepare_started_tx.send(()).unwrap();
            release_prepare_rx.recv().unwrap();
        })
    });

    prepare_started_rx.recv().unwrap();
    let observing = handle.clone();
    let (observer_started_tx, observer_started_rx) = std::sync::mpsc::channel();
    let (observed_tx, observed_rx) = std::sync::mpsc::channel();
    let observer = std::thread::spawn(move || {
        observer_started_tx.send(()).unwrap();
        observed_tx.send(observing.require_active()).unwrap();
    });

    observer_started_rx.recv().unwrap();
    assert!(matches!(
        observed_rx.recv_timeout(Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    release_prepare_tx.send(()).unwrap();
    let activated = activation.join().unwrap().unwrap();
    let observed = observed_rx.recv().unwrap().unwrap();
    observer.join().unwrap();

    assert_eq!(observed.context_id(), activated);
}

#[test]
fn an_aborted_vault_switch_blocks_new_work_then_restores_the_context() {
    let handle = handle();
    let id = handle
        .activate_vault(VaultKey::new("primary").unwrap())
        .unwrap();
    let lease = handle.require_active().unwrap();

    let pending = handle.begin_vault_switch().unwrap();
    assert_eq!(handle.require_active(), Err(RuntimeError::SwitchInProgress));

    drop(pending);
    assert_eq!(handle.require_active().unwrap().context_id(), id);
    assert_eq!(lease.validate(&handle), Ok(()));
    assert!(!lease.is_cancelled());
}

#[tokio::test]
async fn a_committed_vault_switch_retires_owned_work_and_invalidates_leases() {
    let cleanup = RecordedCleanup::default();
    let events = RecordedEvents::default();
    let handle = ApplicationHandle::new(
        Arc::new(FixedId),
        Arc::new(cleanup.clone()),
        Arc::new(events.clone()),
    );
    let id = handle
        .activate_vault(VaultKey::new("primary").unwrap())
        .unwrap();
    let lease = handle.require_active().unwrap();

    let retired = handle.begin_vault_switch().unwrap().commit().await.unwrap();

    assert_eq!(retired, id);
    assert_eq!(cleanup.snapshot(), vec![id]);
    assert_eq!(handle.require_active(), Err(RuntimeError::NoActiveContext));
    assert_eq!(lease.validate(&handle), Err(RuntimeError::StaleContext));
    assert!(lease.is_cancelled());
    assert!(events
        .snapshot()
        .contains(&ContextEvent::Retired { context_id: id }));
}

#[tokio::test]
async fn aborting_commit_during_cleanup_finalizes_retirement_without_reauthorizing_old_work() {
    let ids = DeterministicContextIds::new([11, 12]).unwrap();
    let cleanup = GatedCleanup::default();
    let events = RecordedEvents::default();
    let handle = ApplicationHandle::new(
        Arc::new(ids),
        Arc::new(cleanup.clone()),
        Arc::new(events.clone()),
    );
    let first = handle
        .activate_vault(VaultKey::new("primary").unwrap())
        .unwrap();
    let old_lease = handle.require_active().unwrap();
    let permit = handle.begin_vault_switch().unwrap();
    let commit = tokio::spawn(async move { permit.commit().await });
    cleanup.started.notified().await;
    assert!(old_lease.is_cancelled());
    assert_eq!(handle.require_active(), Err(RuntimeError::SwitchInProgress));

    commit.abort();
    assert!(commit.await.unwrap_err().is_cancelled());
    assert_eq!(handle.require_active(), Err(RuntimeError::SwitchInProgress));
    assert_eq!(old_lease.validate(&handle), Err(RuntimeError::StaleContext));
    assert_eq!(
        handle.activate_vault(VaultKey::new("secondary").unwrap()),
        Err(RuntimeError::SwitchInProgress)
    );
    assert!(!events
        .snapshot()
        .contains(&ContextEvent::Retired { context_id: first }));

    cleanup.release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if handle.require_active() == Err(RuntimeError::NoActiveContext) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("owned cleanup completes logical retirement");
    assert!(events
        .snapshot()
        .contains(&ContextEvent::Retired { context_id: first }));
    let second = handle
        .activate_vault(VaultKey::new("secondary").unwrap())
        .unwrap();
    assert_eq!(first.get(), 11);
    assert_eq!(second.get(), 12);
    assert_ne!(first, second);
    assert_eq!(handle.require_active().unwrap().context_id(), second);
}

#[tokio::test]
async fn panicking_cleanup_keeps_the_runtime_non_admitting() {
    let ids = DeterministicContextIds::new([21, 22]).unwrap();
    let cleanup = PanickingGatedCleanup::default();
    let handle = ApplicationHandle::new(
        Arc::new(ids),
        Arc::new(cleanup.clone()),
        Arc::new(NoopEvents),
    );
    handle
        .activate_vault(VaultKey::new("primary").unwrap())
        .unwrap();
    let permit = handle.begin_vault_switch().unwrap();
    let commit = tokio::spawn(async move { permit.commit().await });
    cleanup.started.notified().await;

    cleanup.release.notify_one();
    let error = commit
        .await
        .expect("outer commit task handles inner cleanup panic")
        .expect_err("cleanup panic becomes a typed runtime error");

    assert!(matches!(error, RuntimeError::Cleanup(_)));
    assert_eq!(handle.require_active(), Err(RuntimeError::SwitchInProgress));
    assert_eq!(
        handle.activate_vault(VaultKey::new("secondary").unwrap()),
        Err(RuntimeError::SwitchInProgress)
    );
}

#[tokio::test]
async fn cleanup_failure_keeps_the_runtime_non_admitting() {
    struct FailingCleanup;

    #[async_trait]
    impl ContextCleanup for FailingCleanup {
        async fn retire(&self, _context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
            Err(ContextCleanupError::new("shutdown timed out"))
        }
    }

    let events = RecordedEvents::default();
    let handle = ApplicationHandle::new(
        Arc::new(FixedId),
        Arc::new(FailingCleanup),
        Arc::new(events.clone()),
    );
    let id = handle
        .activate_vault(VaultKey::new("primary").unwrap())
        .unwrap();
    let lease = handle.require_active().unwrap();

    let error = handle
        .begin_vault_switch()
        .unwrap()
        .commit()
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "runtime context cleanup failed: shutdown timed out"
    );
    assert_eq!(handle.require_active(), Err(RuntimeError::SwitchInProgress));
    assert_eq!(lease.validate(&handle), Err(RuntimeError::StaleContext));
    assert!(!events
        .snapshot()
        .contains(&ContextEvent::Retired { context_id: id }));
    assert_eq!(
        handle.activate_vault(VaultKey::new("secondary").unwrap()),
        Err(RuntimeError::SwitchInProgress)
    );
}

#[tokio::test]
async fn a_new_vault_after_retirement_receives_a_new_generation() {
    let ids = DeterministicContextIds::new([11, 12]).unwrap();
    let cleanup = RecordedContextCleanup::default();
    let events = RecordedContextEvents::default();
    let handle = ApplicationHandle::new(Arc::new(ids), Arc::new(cleanup.clone()), Arc::new(events));

    let first = handle
        .activate_vault(VaultKey::new("primary").unwrap())
        .unwrap();
    let old_lease = handle.require_active().unwrap();
    handle.begin_vault_switch().unwrap().commit().await.unwrap();
    let second = handle
        .activate_vault(VaultKey::new("secondary").unwrap())
        .unwrap();

    assert_eq!(first.get(), 11);
    assert_eq!(second.get(), 12);
    assert_ne!(first, second);
    assert_eq!(cleanup.snapshot(), vec![first]);
    assert_eq!(old_lease.validate(&handle), Err(RuntimeError::StaleContext));
}
