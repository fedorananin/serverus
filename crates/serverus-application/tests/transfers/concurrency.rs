use super::support::*;

#[tokio::test]
async fn concurrent_commands_dispatch_effects_in_committed_revision_order() {
    let repository =
        FakeTransferRepository::with_transfer(running_transfer(), TransferRevision::new(40));
    let dispatcher =
        GatedEffectDispatcher::new(TransferRevision::new(41), repository.operations.clone());
    let handler = Arc::new(TransferCommandHandler::new(
        repository.clone(),
        dispatcher.clone(),
    ));
    let pause_handler = handler.clone();
    let pause = tokio::spawn(async move {
        pause_handler
            .handle(ApplyTransferEvent::new(
                transfer_id(),
                TransferEvent::PauseRequested,
            ))
            .await
    });
    dispatcher.wait_until_blocked().await;

    let mut resume = Box::pin(handler.handle(ApplyTransferEvent::new(
        transfer_id(),
        TransferEvent::ResumeRequested,
    )));
    let first_resume_poll = poll_once(resume.as_mut());
    dispatcher.release();

    let paused = pause
        .await
        .expect("pause task joins")
        .expect("pause succeeds");
    let resumed = match first_resume_poll {
        Poll::Ready(result) => result,
        Poll::Pending => resume.await,
    }
    .expect("resume succeeds");

    assert_eq!(paused.revision(), TransferRevision::new(41));
    assert_eq!(resumed.revision(), TransferRevision::new(42));
    assert_eq!(
        *repository.operations.lock().expect("operation lock"),
        vec![
            RecordedOperation::Persist {
                revision: TransferRevision::new(41),
                state: TransferStateKind::Paused,
            },
            RecordedOperation::Dispatch(TransferEffect::PauseAttempt {
                attempt: AttemptNumber::first(),
            }),
            RecordedOperation::Persist {
                revision: TransferRevision::new(42),
                state: TransferStateKind::Running,
            },
            RecordedOperation::Dispatch(TransferEffect::ResumeAttempt {
                attempt: AttemptNumber::first(),
            }),
        ]
    );
}

#[tokio::test]
async fn a_failed_batch_is_retried_before_a_later_revision_is_committed() {
    let repository =
        FakeTransferRepository::with_transfer(running_transfer(), TransferRevision::new(50));
    let dispatcher = FailingThenRecordingDispatcher::new(1, repository.operations.clone());
    let handler = TransferCommandHandler::new(repository.clone(), dispatcher);

    let first_error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::PauseRequested,
        ))
        .await
        .expect_err("first dispatch fails after commit");
    assert!(matches!(
        first_error,
        TransferCommandError::EffectDispatchFailed { ref batch }
            if batch.revision() == TransferRevision::new(51)
    ));

    let resumed = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::ResumeRequested,
        ))
        .await
        .expect("pending pause dispatch is retried before resume");

    assert_eq!(resumed.revision(), TransferRevision::new(52));
    assert_eq!(
        *repository.operations.lock().expect("operation lock"),
        vec![
            RecordedOperation::Persist {
                revision: TransferRevision::new(51),
                state: TransferStateKind::Paused,
            },
            RecordedOperation::Dispatch(TransferEffect::PauseAttempt {
                attempt: AttemptNumber::first(),
            }),
            RecordedOperation::Persist {
                revision: TransferRevision::new(52),
                state: TransferStateKind::Running,
            },
            RecordedOperation::Dispatch(TransferEffect::ResumeAttempt {
                attempt: AttemptNumber::first(),
            }),
        ]
    );
}

#[tokio::test]
async fn a_failed_pending_retry_blocks_the_later_revision() {
    let repository =
        FakeTransferRepository::with_transfer(running_transfer(), TransferRevision::new(60));
    let dispatcher = FailingThenRecordingDispatcher::new(2, repository.operations.clone());
    let handler = TransferCommandHandler::new(repository.clone(), dispatcher);

    let first_error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::PauseRequested,
        ))
        .await
        .expect_err("first dispatch fails after commit");
    let retry_error = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::ResumeRequested,
        ))
        .await
        .expect_err("failed pending dispatch blocks resume");

    let expected_batch = match first_error {
        TransferCommandError::EffectDispatchFailed { batch } => batch,
        other => panic!("unexpected first error: {other:?}"),
    };
    assert_eq!(
        retry_error,
        TransferCommandError::EffectDispatchFailed {
            batch: expected_batch,
        }
    );
    let stored = repository.stored().expect("paused revision remains stored");
    assert_eq!(stored.revision(), TransferRevision::new(61));
    assert_eq!(stored.transfer().state().kind(), TransferStateKind::Paused);
    assert_eq!(
        *repository.operations.lock().expect("operation lock"),
        vec![RecordedOperation::Persist {
            revision: TransferRevision::new(61),
            state: TransferStateKind::Paused,
        }]
    );
}
