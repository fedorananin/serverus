use serverus_domain::transfers::{
    CompletionOutcome, FailureKind, RetryBudget, TerminalOutcome, TransferEffect, TransferEvent,
    TransferState,
};

use super::support::running_transfer;

#[test]
fn a_successful_attempt_completes_the_transfer() {
    let transition = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::AttemptSucceeded)
        .expect("a running attempt can succeed");

    assert_eq!(
        transition.next.state(),
        &TransferState::Completed {
            outcome: CompletionOutcome::Transferred,
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::PublishTerminal {
            outcome: TerminalOutcome::Completed(CompletionOutcome::Transferred),
        }]
    );
}

#[test]
fn a_permanent_failure_fails_without_scheduling_a_retry() {
    let transfer = running_transfer(RetryBudget::new(2));
    let transition = transfer
        .transition(TransferEvent::PermanentFailure(FailureKind::Integrity))
        .expect("a running attempt can fail permanently");

    assert_eq!(transition.next.retries_used(), 0);
    assert_eq!(
        transition.next.state(),
        &TransferState::Failed {
            failure: FailureKind::Integrity,
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::PublishTerminal {
            outcome: TerminalOutcome::Failed(FailureKind::Integrity),
        }]
    );
}
