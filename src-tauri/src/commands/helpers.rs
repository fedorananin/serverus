//! Focused command-adapter policies shared by legacy command slices.

mod rollback;
mod session_operations;
mod task_runtime;
mod vault_operations;

pub(super) use rollback::{validate_context_and_owner_or_rollback, validate_context_or_rollback};
pub(super) use session_operations::run_session_operation;
pub(super) use task_runtime::{blocking, run_owned_operation};
pub(super) use vault_operations::{
    run_unlocked_vault_operation, run_unlocked_vault_operation_for_lease,
};
