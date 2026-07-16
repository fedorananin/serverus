use std::time::Duration;

use serverus_domain::transfers::TransferEvent as DomainTransferEvent;
use tokio::sync::oneshot;

use super::super::{Control, TransferManager, TransferState};
use super::support::{context_id, fail_recoverably, insert, item, start};

#[test]
fn pause_before_first_control_subscription_is_retained() {
    let transfer = item("session");
    start(&transfer);
    transfer
        .apply_and_dispatch(DomainTransferEvent::PauseRequested, None, None)
        .unwrap();

    let receiver = transfer.control.subscribe();
    assert_eq!(*receiver.borrow(), Control::Pause);
}

#[test]
fn cancel_before_first_control_subscription_is_retained() {
    let transfer = item("session");
    start(&transfer);
    transfer
        .apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None)
        .unwrap();

    let receiver = transfer.control.subscribe();
    assert_eq!(*receiver.borrow(), Control::Cancel);
}

#[test]
fn manual_retry_replaces_a_sticky_cancel_before_worker_subscription() {
    let transfer = item("session");
    start(&transfer);
    transfer
        .apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None)
        .unwrap();
    transfer
        .apply_and_dispatch(DomainTransferEvent::CancellationFinished, None, None)
        .unwrap();
    assert!(transfer.begin_manual_retry());
    transfer
        .apply_and_dispatch(DomainTransferEvent::StartRequested, None, None)
        .unwrap();

    let receiver = transfer.control.subscribe();
    assert_eq!(*receiver.borrow(), Control::Run);
}

#[tokio::test]
async fn cancelling_waiting_retry_cancels_its_pending_timer() {
    let transfer = item("session");
    start(&transfer);
    fail_recoverably(&transfer);
    let attempt = serverus_domain::transfers::AttemptNumber::try_from(2_u32).unwrap();
    let claim = transfer.retry_claim(attempt).expect("retry claim exists");
    let (cancel_sender, cancel_receiver) = oneshot::channel();
    assert!(transfer.install_pending_retry(claim, cancel_sender));

    transfer
        .apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None)
        .unwrap();

    tokio::time::timeout(Duration::from_millis(100), cancel_receiver)
        .await
        .expect("retry timer is cancelled")
        .expect("retry cancellation sender remains alive");
    assert_eq!(transfer.state(), super::super::TransferState::Cancelled);
    assert_eq!(
        transfer.lifecycle.lock().unwrap().transfer.state(),
        &serverus_domain::transfers::TransferState::Cancelled
    );
}

#[test]
fn late_bulk_intent_cannot_mutate_a_new_runtime_context() {
    let manager = TransferManager::default();
    let old_context = context_id(90);
    let new_context = context_id(91);
    manager.activate_context(old_context);
    manager.activate_context(new_context);
    let transfer = item("new-session");
    start(&transfer);
    insert(&manager, transfer);

    assert!(!manager.pause_all(old_context, "new-session"));
    assert_eq!(manager.snapshot().items[0].state, TransferState::Running);

    assert!(manager.pause_all(new_context, "new-session"));
    assert_eq!(manager.snapshot().items[0].state, TransferState::Paused);
}
