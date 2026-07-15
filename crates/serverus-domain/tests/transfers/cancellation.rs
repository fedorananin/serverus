use serverus_domain::transfers::{
    AttemptNumber, RetryBudget, TerminalOutcome, Transfer, TransferEffect, TransferEvent,
    TransferState,
};

use super::support::{running_transfer, transfer_id};

#[test]
fn cancelling_a_queued_transfer_is_immediately_terminal() {
    let transition = Transfer::queued(transfer_id(), RetryBudget::new(2))
        .transition(TransferEvent::CancelRequested)
        .expect("queued transfers can be cancelled");

    assert_eq!(transition.next.state(), &TransferState::Cancelled);
    assert_eq!(
        transition.effects,
        vec![TransferEffect::PublishTerminal {
            outcome: TerminalOutcome::Cancelled,
        }]
    );
}

#[test]
fn cancelling_an_active_attempt_waits_for_worker_acknowledgement() {
    let transition = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::CancelRequested)
        .expect("running transfers can be cancelled");

    assert_eq!(
        transition.next.state(),
        &TransferState::Cancelling {
            attempt: AttemptNumber::first(),
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::CancelAttempt {
            attempt: AttemptNumber::first(),
        }]
    );

    let terminal = transition
        .next
        .transition(TransferEvent::CancellationFinished)
        .expect("a stopped worker acknowledges cancellation");
    assert_eq!(terminal.next.state(), &TransferState::Cancelled);
    assert_eq!(
        terminal.effects,
        vec![TransferEffect::PublishTerminal {
            outcome: TerminalOutcome::Cancelled,
        }]
    );
}

#[test]
fn cancelling_is_idempotent_while_stopping_and_after_termination() {
    let cancelling = running_transfer(RetryBudget::new(0))
        .transition(TransferEvent::CancelRequested)
        .expect("running transfer accepts cancellation")
        .next;

    let repeated = cancelling
        .transition(TransferEvent::CancelRequested)
        .expect("repeated cancellation while stopping is a no-op");
    assert_eq!(repeated.next, cancelling);
    assert!(repeated.effects.is_empty());

    let cancelled = cancelling
        .transition(TransferEvent::CancellationFinished)
        .expect("worker acknowledges cancellation")
        .next;
    let repeated = cancelled
        .transition(TransferEvent::CancelRequested)
        .expect("repeated cancellation after termination is a no-op");
    assert_eq!(repeated.next, cancelled);
    assert!(repeated.effects.is_empty());
}
