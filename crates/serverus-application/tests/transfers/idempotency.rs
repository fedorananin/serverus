use super::support::*;

#[tokio::test]
async fn an_idempotent_domain_noop_is_not_recommitted_or_dispatched() {
    let cancelled = queued_transfer()
        .transition(TransferEvent::CancelRequested)
        .expect("queued transfer can be cancelled")
        .next;
    let repository =
        FakeTransferRepository::with_transfer(cancelled.clone(), TransferRevision::new(30));
    let dispatcher = RecordingEffectDispatcher::new(repository.operations.clone());
    let handler = TransferCommandHandler::new(repository.clone(), dispatcher.clone());

    let result = handler
        .handle(ApplyTransferEvent::new(
            transfer_id(),
            TransferEvent::CancelRequested,
        ))
        .await
        .expect("repeated cancellation is idempotent");

    assert_eq!(result.revision(), TransferRevision::new(30));
    assert_eq!(result.transfer(), &cancelled);
    assert!(repository
        .operations
        .lock()
        .expect("operation lock")
        .is_empty());
    assert!(dispatcher.batches().is_empty());
}
