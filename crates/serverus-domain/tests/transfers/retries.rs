use serverus_domain::transfers::{
    FailureKind, RetryBudget, TerminalOutcome, TransferEffect, TransferEvent, TransferState,
};

use super::support::{attempt, running_transfer};

#[test]
fn a_recoverable_failure_with_budget_schedules_the_next_attempt() {
    let transition = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::RecoverableFailure(
            FailureKind::NetworkInterrupted,
        ))
        .expect("running transfers can fail recoverably");

    assert_eq!(transition.next.retries_used(), 1);
    assert_eq!(
        transition.next.state(),
        &TransferState::WaitingForRetry {
            attempt: attempt(2),
            last_failure: FailureKind::NetworkInterrupted,
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::ScheduleRetry {
            attempt: attempt(2),
            last_failure: FailureKind::NetworkInterrupted,
        }]
    );
}

#[test]
fn an_elapsed_retry_delay_starts_the_scheduled_attempt() {
    let waiting = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::RecoverableFailure(
            FailureKind::NetworkInterrupted,
        ))
        .expect("running transfers can fail recoverably")
        .next;
    let transition = waiting
        .transition(TransferEvent::RetryDelayElapsed)
        .expect("a scheduled retry can start");

    assert_eq!(
        transition.next.state(),
        &TransferState::Running {
            attempt: attempt(2),
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::StartAttempt {
            attempt: attempt(2),
        }]
    );
}

#[test]
fn a_recoverable_failure_after_the_retry_budget_is_terminal() {
    let retrying = running_transfer(RetryBudget::new(1))
        .transition(TransferEvent::RecoverableFailure(
            FailureKind::NetworkInterrupted,
        ))
        .expect("the first failure can consume the retry budget")
        .next
        .transition(TransferEvent::RetryDelayElapsed)
        .expect("the retry can start")
        .next;
    let transition = retrying
        .transition(TransferEvent::RecoverableFailure(FailureKind::EndpointBusy))
        .expect("an exhausted retry budget becomes a failed outcome");

    assert_eq!(transition.next.retries_used(), 1);
    assert_eq!(
        transition.next.state(),
        &TransferState::Failed {
            failure: FailureKind::EndpointBusy,
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::PublishTerminal {
            outcome: TerminalOutcome::Failed(FailureKind::EndpointBusy),
        }]
    );
}

#[test]
fn cancelling_a_scheduled_retry_cancels_its_timer() {
    let waiting = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::RecoverableFailure(
            FailureKind::NetworkInterrupted,
        ))
        .expect("running transfers can fail recoverably")
        .next;
    let transition = waiting
        .transition(TransferEvent::CancelRequested)
        .expect("scheduled retries can be cancelled");

    assert_eq!(transition.next.state(), &TransferState::Cancelled);
    assert_eq!(
        transition.effects,
        vec![
            TransferEffect::CancelRetry {
                attempt: attempt(2),
            },
            TransferEffect::PublishTerminal {
                outcome: TerminalOutcome::Cancelled,
            },
        ]
    );
}
