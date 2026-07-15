use serverus_domain::transfers::{
    ConflictKind as DomainConflictKind, TransferEvent as DomainTransferEvent,
};

use super::super::{ConflictAction, TransferBatch, TransferManager, TransferState};
use super::support::{insert, item_in_batch, start};

fn wait_for_decision(item: &super::super::TransferItem) {
    start(item);
    item.apply_and_dispatch(
        DomainTransferEvent::ConflictDetected(DomainConflictKind::DestinationExists),
        None,
        None,
    )
    .expect("running transfer waits for a conflict decision");
}

#[test]
fn apply_to_all_resolves_the_whole_batch_without_leaking_to_the_next_operation() {
    for (action, expected_state) in [
        (ConflictAction::Overwrite, TransferState::Running),
        (ConflictAction::Skip, TransferState::Skipped),
        (ConflictAction::Rename, TransferState::Running),
    ] {
        let manager = TransferManager::default();
        let batch = TransferBatch::new();
        let first = item_in_batch("session", batch.clone());
        let second = item_in_batch("session", batch.clone());
        let separate = item_in_batch("session", TransferBatch::new());
        for item in [&first, &second, &separate] {
            wait_for_decision(item);
            insert(&manager, item.clone());
        }

        manager.resolve_conflict("session", &first.id, action, true);

        assert_eq!(first.state(), expected_state);
        assert_eq!(second.state(), expected_state);
        assert_eq!(separate.state(), TransferState::Conflict);
        assert_eq!(batch.policy_override(), Some(action));
        assert_eq!(separate.batch.policy_override(), None);
    }
}
