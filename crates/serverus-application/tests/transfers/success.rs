use super::support::*;

#[tokio::test]
async fn a_valid_transition_is_persisted_before_effect_dispatch() {
    let repository =
        FakeTransferRepository::with_transfer(queued_transfer(), TransferRevision::new(7));
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    let handler = TransferCommandHandler::new(repository.clone(), dispatcher.clone());

    let result = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::StartRequested,
        ))
        .await
        .expect("valid transition succeeds");

    assert_eq!(result.revision(), TransferRevision::new(8));
    assert_eq!(
        result.transfer().state(),
        &TransferState::Running {
            attempt: AttemptNumber::first(),
        }
    );
    assert_eq!(repository.stored(), Some(result.clone().into_versioned()));
    assert_eq!(
        *repository.operations.lock().expect("operation lock"),
        vec![
            RecordedOperation::Persist {
                revision: TransferRevision::new(8),
                state: TransferStateKind::Running,
            },
            RecordedOperation::Dispatch(TransferEffect::StartAttempt {
                attempt: AttemptNumber::first(),
            }),
        ]
    );
}

#[tokio::test]
async fn effect_batch_preserves_domain_effect_order() {
    let repository =
        FakeTransferRepository::with_transfer(waiting_for_conflict(), TransferRevision::new(11));
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    let handler = TransferCommandHandler::new(repository.clone(), dispatcher.clone());

    handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::ConflictResolved(ConflictDecision::Skip),
        ))
        .await
        .expect("conflict can be skipped");

    let effects = vec![
        TransferEffect::ApplyConflictDecision {
            attempt: AttemptNumber::first(),
            decision: ConflictDecision::Skip,
        },
        TransferEffect::PublishTerminal {
            outcome: TerminalOutcome::Completed(CompletionOutcome::Skipped),
        },
    ];
    assert_eq!(dispatcher.batches().len(), 1);
    assert_eq!(dispatcher.batches()[0].transfer_id(), transfer_id());
    assert_eq!(
        dispatcher.batches()[0].revision(),
        TransferRevision::new(12)
    );
    assert_eq!(dispatcher.batches()[0].effects(), effects.as_slice());
    assert_eq!(
        *repository.operations.lock().expect("operation lock"),
        vec![
            RecordedOperation::Persist {
                revision: TransferRevision::new(12),
                state: TransferStateKind::Completed,
            },
            RecordedOperation::Dispatch(effects[0]),
            RecordedOperation::Dispatch(effects[1]),
        ]
    );
}
