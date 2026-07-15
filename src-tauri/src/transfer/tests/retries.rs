use std::time::Duration;

use serverus_domain::transfers::{
    AttemptNumber as DomainAttemptNumber, TransferStateKind as DomainTransferStateKind,
};
use tokio::sync::oneshot;

use super::super::{TransferManager, TransferState};
use super::support::{context_id, fail_recoverably, insert, item, start};

#[test]
fn manual_and_automatic_retry_claims_are_mutually_exclusive() {
    let transfer = item("session");
    start(&transfer);
    fail_recoverably(&transfer);
    let attempt = DomainAttemptNumber::try_from(2_u32).unwrap();
    let claim = transfer.retry_claim(attempt).unwrap();
    let (cancel_sender, _cancel_receiver) = oneshot::channel();
    assert!(transfer.install_pending_retry(claim, cancel_sender));

    assert!(transfer.begin_manual_retry());
    assert!(!transfer.claim_auto_retry(claim));
    assert_eq!(
        transfer.domain_state_kind(),
        DomainTransferStateKind::Queued
    );
}

#[test]
fn clear_finished_keeps_retry_backoff_observable_and_cancellable() {
    let manager = TransferManager::default();
    let context = context_id(1);
    manager.activate_context(context);
    let transfer = item("session");
    start(&transfer);
    fail_recoverably(&transfer);
    let id = transfer.id.clone();
    insert(&manager, transfer);

    assert!(manager.clear_finished(context));
    assert_eq!(manager.snapshot().0.len(), 1);
    assert_eq!(manager.snapshot().0[0].state, TransferState::Error);
    manager.cancel(&id);
    assert_eq!(manager.snapshot().0[0].state, TransferState::Cancelled);
}

#[tokio::test]
async fn clear_all_interrupts_retry_backoff_without_waiting_for_its_delay() {
    let manager = TransferManager::default();
    let transfer = item("session");
    start(&transfer);
    fail_recoverably(&transfer);
    let attempt = DomainAttemptNumber::try_from(2_u32).unwrap();
    let claim = transfer.retry_claim(attempt).unwrap();
    let (cancel_sender, cancel_receiver) = oneshot::channel();
    assert!(transfer.install_pending_retry(claim, cancel_sender));
    insert(&manager, transfer);

    manager.clear_all().await;

    tokio::time::timeout(Duration::from_millis(100), cancel_receiver)
        .await
        .expect("backoff is interrupted immediately")
        .expect("pending timer receives cancellation");
    assert!(manager.snapshot().0.is_empty());
}
