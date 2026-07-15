use super::{ApiError, AppError};
use serverus_application::context::ContextCleanupError;
use serverus_runtime::RuntimeError;

#[test]
fn ipc_error_keeps_large_optional_payloads_behind_a_pointer() {
    assert!(std::mem::size_of::<ApiError>() <= 64);
}

#[test]
fn unavailable_or_locked_context_keeps_the_existing_vault_locked_error() {
    assert!(matches!(
        AppError::from(RuntimeError::NoActiveContext),
        AppError::VaultLocked
    ));
    assert!(matches!(
        AppError::from(RuntimeError::VaultLocked),
        AppError::VaultLocked
    ));
}

#[test]
fn stale_or_different_context_has_a_typed_ipc_error() {
    for runtime_error in [
        RuntimeError::StaleContext,
        RuntimeError::DifferentVaultActive,
    ] {
        let error = AppError::from(runtime_error);
        assert!(matches!(error, AppError::WrongRuntimeContext));

        let api = ApiError::from(error);
        assert_eq!(api.code, "wrong_runtime_context");
    }
}

#[test]
fn switching_context_has_a_typed_ipc_error() {
    let api = ApiError::from(AppError::from(RuntimeError::SwitchInProgress));

    assert_eq!(api.code, "runtime_context_switching");
}

#[test]
fn cleanup_failure_does_not_expose_its_source_message() {
    let api = ApiError::from(AppError::from(RuntimeError::Cleanup(
        ContextCleanupError::new("private adapter detail"),
    )));

    assert_eq!(api.code, "runtime_cleanup_failed");
    assert_eq!(api.message, "runtime context cleanup failed");
    assert!(!api.message.contains("private adapter detail"));
}
