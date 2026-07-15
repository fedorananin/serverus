use std::sync::atomic::{AtomicU64, Ordering};

use serverus_domain::runtime_context::RuntimeContextId;
use serverus_lib::transfer::TransferManager;

static NEXT_CONTEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn activate(manager: &TransferManager) -> RuntimeContextId {
    let value = u128::from(NEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed));
    let context_id = RuntimeContextId::try_from(value).expect("test context ID is non-zero");
    manager.activate_context(context_id);
    context_id
}
