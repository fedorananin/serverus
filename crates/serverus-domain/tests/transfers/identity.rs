use serverus_domain::transfers::{AttemptNumber, RetryBudget, Transfer, TransferId, TransferState};

use super::support::transfer_id;

#[test]
fn transfer_id_rejects_zero() {
    assert!(TransferId::try_from(0_u128).is_err());
}

#[test]
fn transfer_id_round_trips_its_non_zero_value() {
    let id = TransferId::try_from(42_u128).expect("test ID is non-zero");
    assert_eq!(id.get(), 42);
}

#[test]
fn attempt_number_rejects_zero() {
    assert!(AttemptNumber::try_from(0_u32).is_err());
}

#[test]
fn a_new_transfer_is_queued() {
    let transfer = Transfer::queued(transfer_id(), RetryBudget::new(2));

    assert_eq!(transfer.id(), transfer_id());
    assert_eq!(transfer.retry_budget(), RetryBudget::new(2));
    assert_eq!(transfer.retries_used(), 0);
    assert_eq!(transfer.state(), &TransferState::Queued);
}
