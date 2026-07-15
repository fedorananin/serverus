use super::support::*;

#[tokio::test]
async fn an_invalid_transition_is_typed_and_has_no_effects() {
    let repository =
        FakeTransferRepository::with_transfer(queued_transfer(), TransferRevision::new(3));
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    let handler = TransferCommandHandler::new(repository.clone(), dispatcher.clone());

    let error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::PauseRequested,
        ))
        .await
        .expect_err("queued transfer cannot pause");

    match error {
        TransferCommandError::InvalidTransition {
            transfer_id: failed_id,
            reason,
        } => {
            assert_eq!(failed_id, transfer_id());
            assert_eq!(reason.from(), TransferStateKind::Queued);
        }
        other => panic!("unexpected application error: {other:?}"),
    }
    assert_eq!(
        repository.stored().expect("stored transfer").revision(),
        TransferRevision::new(3)
    );
    assert!(dispatcher.batches().is_empty());
}

#[tokio::test]
async fn a_missing_transfer_is_typed_and_has_no_effects() {
    let repository = FakeTransferRepository::empty();
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    let handler = TransferCommandHandler::new(repository, dispatcher.clone());

    let error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::StartRequested,
        ))
        .await
        .expect_err("missing transfer is rejected");

    assert_eq!(
        error,
        TransferCommandError::NotFound {
            transfer_id: transfer_id(),
        }
    );
    assert!(dispatcher.batches().is_empty());
}

#[tokio::test]
async fn concurrent_revision_rejection_prevents_effect_dispatch() {
    let repository =
        FakeTransferRepository::with_transfer(queued_transfer(), TransferRevision::new(5));
    repository.fail_save_with(TransferSaveError::ConcurrentRevision {
        actual_revision: Some(TransferRevision::new(6)),
    });
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    let handler = TransferCommandHandler::new(repository, dispatcher.clone());

    let error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::StartRequested,
        ))
        .await
        .expect_err("compare-and-swap rejection is surfaced");

    assert_eq!(
        error,
        TransferCommandError::ConcurrentRevision {
            transfer_id: transfer_id(),
            expected_revision: TransferRevision::new(5),
            actual_revision: Some(TransferRevision::new(6)),
        }
    );
    assert!(dispatcher.batches().is_empty());
}

#[tokio::test]
async fn repository_failure_is_a_typed_application_error() {
    let repository = FakeTransferRepository::empty();
    repository.fail_load();
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    let handler = TransferCommandHandler::new(repository, dispatcher);

    let error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::StartRequested,
        ))
        .await
        .expect_err("repository failure is surfaced");

    assert_eq!(
        error,
        TransferCommandError::RepositoryUnavailable {
            transfer_id: transfer_id(),
        }
    );
}

#[tokio::test]
async fn dispatch_failure_returns_the_committed_effect_batch() {
    let repository =
        FakeTransferRepository::with_transfer(queued_transfer(), TransferRevision::new(20));
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    dispatcher.fail();
    let handler = TransferCommandHandler::new(repository.clone(), dispatcher);

    let error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::StartRequested,
        ))
        .await
        .expect_err("dispatch failure is surfaced");

    match error {
        TransferCommandError::EffectDispatchFailed { batch } => {
            assert_eq!(batch.transfer_id(), transfer_id());
            assert_eq!(batch.revision(), TransferRevision::new(21));
            assert_eq!(
                batch.effects(),
                &[TransferEffect::StartAttempt {
                    attempt: AttemptNumber::first(),
                }]
            );
        }
        other => panic!("unexpected application error: {other:?}"),
    }
    assert_eq!(
        repository.stored().expect("committed transfer").revision(),
        TransferRevision::new(21)
    );
}
