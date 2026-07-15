//! Remote-edit context tests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serverus_application::context::{
    AppEventSink, ContextCleanup, ContextCleanupError, ContextEvent, RuntimeContextIdGenerator,
};
use serverus_domain::runtime_context::{RuntimeContextId, VaultKey};
use serverus_runtime::ApplicationHandle;

use super::{validate_context_and_owner_or_rollback, validate_context_or_rollback};

struct FixedId;

impl RuntimeContextIdGenerator for FixedId {
    fn next_id(&self) -> RuntimeContextId {
        RuntimeContextId::try_from(73_u128).unwrap()
    }
}

struct NoopCleanup;

#[async_trait]
impl ContextCleanup for NoopCleanup {
    async fn retire(&self, _context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
        Ok(())
    }
}

struct NoopEvents;

impl AppEventSink for NoopEvents {
    fn publish(&self, _event: ContextEvent) {}
}

#[tokio::test]
async fn a_result_that_finishes_during_switch_is_rolled_back() {
    let application = ApplicationHandle::new(
        Arc::new(FixedId),
        Arc::new(NoopCleanup),
        Arc::new(NoopEvents),
    );
    application
        .activate_vault(VaultKey::new("old-vault").unwrap())
        .unwrap();
    let lease = application.require_active().unwrap();
    let switch = application.begin_vault_switch().unwrap();
    let rolled_back = Arc::new(AtomicBool::new(false));
    let rollback_flag = rolled_back.clone();

    let error = validate_context_or_rollback(
        &application,
        &lease,
        "late-edit",
        move |_value| async move {
            rollback_flag.store(true, Ordering::SeqCst);
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error.code, "wrong_runtime_context");
    assert!(rolled_back.load(Ordering::SeqCst));
    drop(switch);
}

#[tokio::test]
async fn a_late_registration_is_rolled_back_after_its_session_closes() {
    let application = ApplicationHandle::new(
        Arc::new(FixedId),
        Arc::new(NoopCleanup),
        Arc::new(NoopEvents),
    );
    application
        .activate_vault(VaultKey::new("vault").unwrap())
        .unwrap();
    let lease = application.require_active().unwrap();
    let owner_is_current = Arc::new(AtomicBool::new(true));
    let rolled_back = Arc::new(AtomicBool::new(false));
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());

    let task = {
        let application = application.clone();
        let owner_is_current = owner_is_current.clone();
        let rolled_back = rolled_back.clone();
        let started = started.clone();
        let release = release.clone();
        tokio::spawn(async move {
            started.notify_one();
            release.notified().await;
            validate_context_and_owner_or_rollback(
                &application,
                &lease,
                || owner_is_current.load(Ordering::SeqCst),
                "late-tunnel",
                move |_value| async move {
                    rolled_back.store(true, Ordering::SeqCst);
                },
            )
            .await
        })
    };

    started.notified().await;
    owner_is_current.store(false, Ordering::SeqCst);
    release.notify_one();
    let error = task.await.unwrap().unwrap_err();

    assert_eq!(error.code, "session_not_found");
    assert!(rolled_back.load(Ordering::SeqCst));
}
