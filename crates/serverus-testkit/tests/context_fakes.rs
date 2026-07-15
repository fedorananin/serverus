use serverus_application::context::{
    AppEventSink, ContextCleanup, ContextEvent, RuntimeContextIdGenerator,
};
use serverus_domain::runtime_context::{RuntimeContextId, VaultKey};
use serverus_testkit::{DeterministicContextIds, RecordedContextCleanup, RecordedContextEvents};

#[test]
fn deterministic_ids_follow_the_requested_sequence() {
    let ids = DeterministicContextIds::new([7, 9]).unwrap();

    assert_eq!(ids.next_id().get(), 7);
    assert_eq!(ids.next_id().get(), 9);
}

#[test]
#[should_panic(expected = "deterministic context ID sequence is exhausted")]
fn deterministic_ids_fail_loudly_when_exhausted() {
    let ids = DeterministicContextIds::new([7]).unwrap();

    ids.next_id();
    ids.next_id();
}

#[tokio::test]
async fn cleanup_records_retired_generations_and_can_fail_once() {
    let cleanup = RecordedContextCleanup::default();
    let id = RuntimeContextId::try_from(7).unwrap();

    cleanup.fail_next("worker shutdown timed out");
    let error = cleanup.retire(id).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "runtime context cleanup failed: worker shutdown timed out"
    );
    assert_eq!(cleanup.snapshot(), vec![id]);

    cleanup.retire(id).await.unwrap();
    assert_eq!(cleanup.snapshot(), vec![id, id]);
}

#[test]
fn event_sink_records_events_in_order() {
    let events = RecordedContextEvents::default();
    let id = RuntimeContextId::try_from(7).unwrap();
    let activated = ContextEvent::Activated {
        context_id: id,
        vault: VaultKey::new("primary").unwrap(),
    };
    let retired = ContextEvent::Retired { context_id: id };

    events.publish(activated.clone());
    events.publish(retired.clone());

    assert_eq!(events.snapshot(), vec![activated, retired]);
}
