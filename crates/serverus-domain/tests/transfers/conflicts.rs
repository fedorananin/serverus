use serverus_domain::transfers::{
    AttemptNumber, CompletionOutcome, ConflictDecision, ConflictKind, RetryBudget, TerminalOutcome,
    TransferEffect, TransferEvent, TransferState,
};

use super::support::running_transfer;

fn waiting_for_conflict() -> serverus_domain::transfers::Transfer {
    running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::ConflictDetected(
            ConflictKind::DestinationExists,
        ))
        .expect("running transfers can report conflicts")
        .next
}

#[test]
fn detecting_a_conflict_requests_a_decision() {
    let transition = running_transfer(RetryBudget::new(2))
        .transition(TransferEvent::ConflictDetected(
            ConflictKind::DestinationExists,
        ))
        .expect("running transfers can report conflicts");

    assert_eq!(
        transition.next.state(),
        &TransferState::WaitingForConflict {
            attempt: AttemptNumber::first(),
            conflict: ConflictKind::DestinationExists,
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::RequestConflictDecision {
            attempt: AttemptNumber::first(),
            conflict: ConflictKind::DestinationExists,
        }]
    );
}

#[test]
fn resolving_a_conflict_with_overwrite_resumes_the_attempt() {
    let transition = waiting_for_conflict()
        .transition(TransferEvent::ConflictResolved(ConflictDecision::Overwrite))
        .expect("a pending conflict can be resolved");

    assert_eq!(
        transition.next.state(),
        &TransferState::Running {
            attempt: AttemptNumber::first(),
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::ApplyConflictDecision {
            attempt: AttemptNumber::first(),
            decision: ConflictDecision::Overwrite,
        }]
    );
}

#[test]
fn resolving_a_conflict_with_rename_resumes_the_attempt_with_a_distinct_effect() {
    let transition = waiting_for_conflict()
        .transition(TransferEvent::ConflictResolved(ConflictDecision::Rename))
        .expect("a pending conflict can be renamed");

    assert_eq!(
        transition.next.state(),
        &TransferState::Running {
            attempt: AttemptNumber::first(),
        }
    );
    assert_eq!(
        transition.effects,
        vec![TransferEffect::ApplyConflictDecision {
            attempt: AttemptNumber::first(),
            decision: ConflictDecision::Rename,
        }]
    );
}

#[test]
fn resolving_a_conflict_with_skip_completes_without_copying() {
    let transition = waiting_for_conflict()
        .transition(TransferEvent::ConflictResolved(ConflictDecision::Skip))
        .expect("a pending conflict can be skipped");

    assert_eq!(
        transition.next.state(),
        &TransferState::Completed {
            outcome: CompletionOutcome::Skipped,
        }
    );
    assert_eq!(
        transition.effects,
        vec![
            TransferEffect::ApplyConflictDecision {
                attempt: AttemptNumber::first(),
                decision: ConflictDecision::Skip,
            },
            TransferEffect::PublishTerminal {
                outcome: TerminalOutcome::Completed(CompletionOutcome::Skipped),
            },
        ]
    );
}
