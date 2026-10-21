use super::{connect_error, local_network_hint, ApiError, AppError};
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

#[derive(Debug, thiserror::Error)]
#[error("dispatch failure")]
struct Wrapper(#[source] std::io::Error);

#[test]
fn local_network_hint_only_for_host_unreachable_on_macos() {
    let unreachable = std::io::Error::from(std::io::ErrorKind::HostUnreachable);
    let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
    let expected = cfg!(target_os = "macos");
    assert_eq!(!local_network_hint(&unreachable).is_empty(), expected);
    assert_eq!(
        !local_network_hint(&Wrapper(unreachable)).is_empty(),
        expected
    );
    assert!(local_network_hint(&refused).is_empty());
    assert!(local_network_hint(&Wrapper(refused)).is_empty());
}

#[test]
fn connect_errors_find_the_io_error_russh_and_suppaftp_hide() {
    let unreachable = || std::io::Error::from(std::io::ErrorKind::HostUnreachable);
    let ssh = connect_error("pi.local:22", &russh::Error::IO(unreachable())).to_string();
    let ftp = suppaftp::FtpError::ConnectionError(unreachable());
    let ftp = connect_error("pi.local:21", &ftp).to_string();
    for message in [ssh, ftp] {
        assert!(message.starts_with("connection failed: pi.local:2"));
        assert_eq!(message.contains("Local Network"), cfg!(target_os = "macos"));
    }
    let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
    let refused = connect_error("pi.local:22", &russh::Error::IO(refused)).to_string();
    assert!(!refused.contains("Local Network"));
}
