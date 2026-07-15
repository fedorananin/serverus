use serverus_domain::transfers::{
    AttemptNumber as DomainAttemptNumber, CompletionOutcome, ConflictDecision,
    ConflictKind as DomainConflictKind, FailureKind as DomainFailureKind, RetryBudget,
    TransferEffect as DomainTransferEffect, TransferEvent as DomainTransferEvent,
    TransferId as DomainTransferId, TransferState as DomainTransferState,
    TransferStateKind as DomainTransferStateKind,
};

use super::super::lifecycle::{domain_conflict_decision, domain_state_to_ipc};
use super::super::{ConflictAction, TransferLifecycle, TransferState};
use super::support::{item, start};

fn domain_id() -> DomainTransferId {
    DomainTransferId::try_from(41_u128).unwrap()
}

#[test]
fn every_domain_state_maps_to_the_existing_ipc_contract() {
    let attempt = DomainAttemptNumber::first();
    let cases = [
        (DomainTransferState::Queued, TransferState::Queued),
        (
            DomainTransferState::Running { attempt },
            TransferState::Running,
        ),
        (
            DomainTransferState::Paused { attempt },
            TransferState::Paused,
        ),
        (
            DomainTransferState::WaitingForConflict {
                attempt,
                conflict: DomainConflictKind::DestinationExists,
            },
            TransferState::Conflict,
        ),
        (
            DomainTransferState::WaitingForRetry {
                attempt,
                last_failure: DomainFailureKind::RemoteIo,
            },
            TransferState::Error,
        ),
        (
            DomainTransferState::Cancelling { attempt },
            TransferState::Running,
        ),
        (
            DomainTransferState::Completed {
                outcome: CompletionOutcome::Transferred,
            },
            TransferState::Done,
        ),
        (
            DomainTransferState::Completed {
                outcome: CompletionOutcome::Skipped,
            },
            TransferState::Skipped,
        ),
        (DomainTransferState::Cancelled, TransferState::Cancelled),
        (
            DomainTransferState::Failed {
                failure: DomainFailureKind::RemoteIo,
            },
            TransferState::Error,
        ),
    ];

    for (domain, ipc) in cases {
        assert_eq!(domain_state_to_ipc(&domain), ipc);
    }
}

#[test]
fn every_ipc_conflict_action_maps_to_the_same_domain_decision() {
    assert_eq!(
        domain_conflict_decision(ConflictAction::Overwrite),
        ConflictDecision::Overwrite
    );
    assert_eq!(
        domain_conflict_decision(ConflictAction::Skip),
        ConflictDecision::Skip
    );
    assert_eq!(
        domain_conflict_decision(ConflictAction::Rename),
        ConflictDecision::Rename
    );
}

#[test]
fn lifecycle_control_transitions_emit_domain_effects() {
    let mut lifecycle = TransferLifecycle::new(domain_id(), RetryBudget::new(2));
    let attempt = DomainAttemptNumber::first();

    assert_eq!(
        lifecycle
            .apply(DomainTransferEvent::StartRequested, None)
            .unwrap(),
        vec![DomainTransferEffect::StartAttempt { attempt }]
    );
    assert_eq!(
        lifecycle
            .apply(DomainTransferEvent::PauseRequested, None)
            .unwrap(),
        vec![DomainTransferEffect::PauseAttempt { attempt }]
    );
    assert_eq!(
        lifecycle
            .apply(DomainTransferEvent::ResumeRequested, None)
            .unwrap(),
        vec![DomainTransferEffect::ResumeAttempt { attempt }]
    );
}

#[test]
fn lifecycle_retry_clears_the_transient_error_when_delay_elapses() {
    let mut lifecycle = TransferLifecycle::new(domain_id(), RetryBudget::new(2));
    lifecycle
        .apply(DomainTransferEvent::StartRequested, None)
        .unwrap();
    lifecycle
        .apply(
            DomainTransferEvent::RecoverableFailure(DomainFailureKind::NetworkInterrupted),
            Some("timeout".into()),
        )
        .unwrap();
    assert_eq!(lifecycle.error.as_deref(), Some("timeout"));

    lifecycle
        .apply(DomainTransferEvent::RetryDelayElapsed, None)
        .unwrap();
    assert_eq!(lifecycle.error, None);
}

#[test]
fn lifecycle_conflict_resolution_preserves_skipped_ipc_state() {
    let transfer = item("session");
    start(&transfer);
    transfer
        .apply_and_dispatch(
            DomainTransferEvent::ConflictDetected(DomainConflictKind::DestinationExists),
            None,
            None,
        )
        .unwrap();
    transfer
        .apply_and_dispatch(
            DomainTransferEvent::ConflictResolved(ConflictDecision::Skip),
            None,
            None,
        )
        .unwrap();

    assert_eq!(transfer.state(), TransferState::Skipped);
}

#[test]
fn lifecycle_active_cancel_waits_for_worker_acknowledgement() {
    let transfer = item("session");
    start(&transfer);
    transfer
        .apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None)
        .unwrap();
    assert_eq!(
        transfer.domain_state_kind(),
        DomainTransferStateKind::Cancelling
    );

    transfer
        .apply_and_dispatch(DomainTransferEvent::CancellationFinished, None, None)
        .unwrap();
    assert_eq!(
        transfer.domain_state_kind(),
        DomainTransferStateKind::Cancelled
    );
}

#[test]
fn manual_retry_starts_a_new_queued_run_with_the_same_transfer_id() {
    let mut lifecycle = TransferLifecycle::new(domain_id(), RetryBudget::new(2));
    lifecycle
        .apply(DomainTransferEvent::StartRequested, None)
        .unwrap();
    lifecycle
        .apply(
            DomainTransferEvent::PermanentFailure(DomainFailureKind::RemoteIo),
            Some("failed".into()),
        )
        .unwrap();

    lifecycle.restart_for_manual_retry();
    assert_eq!(lifecycle.transfer.id(), domain_id());
    assert_eq!(lifecycle.transfer.state(), &DomainTransferState::Queued);
    assert_eq!(lifecycle.error, None);
}
