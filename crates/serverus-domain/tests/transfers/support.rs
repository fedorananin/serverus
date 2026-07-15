use serverus_domain::transfers::{AttemptNumber, RetryBudget, Transfer, TransferEvent, TransferId};

pub(super) fn transfer_id() -> TransferId {
    TransferId::try_from(42_u128).expect("test ID is non-zero")
}

pub(super) fn running_transfer(retry_budget: RetryBudget) -> Transfer {
    Transfer::queued(transfer_id(), retry_budget)
        .transition(TransferEvent::StartRequested)
        .expect("queued transfers can start")
        .next
}

pub(super) fn attempt(number: u32) -> AttemptNumber {
    AttemptNumber::try_from(number).expect("test attempt number is non-zero")
}
