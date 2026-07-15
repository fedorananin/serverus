use serverus_domain::transfers::{
    FailureKind, RetryBudget, Transfer, TransferEvent, TransferEventKind, TransferStateKind,
};

use super::support::{running_transfer, transfer_id};

#[test]
fn terminal_states_reject_further_events() {
    let completed = running_transfer(RetryBudget::new(0))
        .transition(TransferEvent::AttemptSucceeded)
        .expect("a running attempt can succeed")
        .next;
    let error = completed
        .transition(TransferEvent::StartRequested)
        .expect_err("completed transfers are terminal");

    assert!(completed.state().is_terminal());
    assert_eq!(error.from(), TransferStateKind::Completed);
    assert_eq!(error.event(), TransferEventKind::StartRequested);
}

#[test]
fn cancelled_and_failed_states_are_terminal() {
    let cancelled = Transfer::queued(transfer_id(), RetryBudget::new(0))
        .transition(TransferEvent::CancelRequested)
        .expect("queued transfers can be cancelled")
        .next;
    let failed = running_transfer(RetryBudget::new(0))
        .transition(TransferEvent::PermanentFailure(FailureKind::RemoteIo))
        .expect("running transfers can fail permanently")
        .next;

    assert!(cancelled.state().is_terminal());
    assert!(failed.state().is_terminal());
}
