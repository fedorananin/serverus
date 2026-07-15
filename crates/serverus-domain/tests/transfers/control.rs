use serverus_domain::transfers::{
    AttemptNumber, RetryBudget, Transfer, TransferEffect, TransferEvent, TransferState,
};

use super::support::{running_transfer, transfer_id};

#[test]
fn starting_a_queued_transfer_starts_the_first_attempt() {
    let transfer = Transfer::queued(transfer_id(), RetryBudget::new(2));
    let transition = transfer
        .transition(TransferEvent::StartRequested)
        .expect("queued transfers can start");

    assert_eq!(
        transition.next.state(),
        &TransferState::Running {
            attempt: AttemptNumber::first(),
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::StartAttempt {
            attempt: AttemptNumber::first(),
        }]
    );
    assert_eq!(transfer.state(), &TransferState::Queued);
}

#[test]
fn pausing_a_running_transfer_preserves_the_attempt() {
    let transition = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::PauseRequested)
        .expect("running transfers can pause");

    assert_eq!(
        transition.next.state(),
        &TransferState::Paused {
            attempt: AttemptNumber::first(),
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::PauseAttempt {
            attempt: AttemptNumber::first(),
        }]
    );
}

#[test]
fn resuming_a_paused_transfer_preserves_the_attempt() {
    let paused = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::PauseRequested)
        .expect("running transfers can pause")
        .next;
    let transition = paused
        .transition(TransferEvent::ResumeRequested)
        .expect("paused transfers can resume");

    assert_eq!(
        transition.next.state(),
        &TransferState::Running {
            attempt: AttemptNumber::first(),
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::ResumeAttempt {
            attempt: AttemptNumber::first(),
        }]
    );
}
