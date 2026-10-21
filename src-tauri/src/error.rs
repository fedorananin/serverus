//! Application error types.
//!
//! `AppError` is the internal error; `ApiError` is what crosses the IPC
//! boundary. Error messages must never contain secrets (SPEC hard rule) —
//! wrap external errors with context strings, never format key material.

use serde::Serialize;
use specta::Type;

// Some variants belong to milestones still under construction (sessions,
// transfers, tunnels); the allow comes off once M2–M6 land.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("vault file not found")]
    VaultNotFound,
    #[error("vault already exists at this path")]
    VaultExists,
    #[error("invalid master password")]
    InvalidPassword,
    #[error("vault is locked")]
    VaultLocked,
    #[error("vault file is corrupted: {0}")]
    Corrupted(String),
    #[error("unsupported vault format version {0}")]
    UnsupportedVersion(u8),
    #[error("quick unlock is not available: {0}")]
    QuickUnlockUnavailable(String),
    #[error("quick unlock cancelled")]
    QuickUnlockCancelled,
    #[error("connection not found")]
    ConnectionNotFound,
    #[error("session not found")]
    SessionNotFound,
    #[error("host key verification failed: {0}")]
    HostKey(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("remote file operation failed: {0}")]
    RemoteFs(String),
    #[error("transfer failed: {0}")]
    Transfer(String),
    #[error("tunnel failed: {0}")]
    Tunnel(String),
    #[error("operation belongs to a retired or different runtime context")]
    WrongRuntimeContext,
    #[error("runtime context switch is in progress")]
    RuntimeContextSwitching,
    #[error("runtime context cleanup failed")]
    RuntimeCleanupFailed,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    fn code(&self) -> &'static str {
        match self {
            AppError::VaultNotFound => "vault_not_found",
            AppError::VaultExists => "vault_exists",
            AppError::InvalidPassword => "invalid_password",
            AppError::VaultLocked => "vault_locked",
            AppError::Corrupted(_) => "corrupted",
            AppError::UnsupportedVersion(_) => "unsupported_version",
            AppError::QuickUnlockUnavailable(_) => "quick_unlock_unavailable",
            AppError::QuickUnlockCancelled => "quick_unlock_cancelled",
            AppError::ConnectionNotFound => "connection_not_found",
            AppError::SessionNotFound => "session_not_found",
            AppError::HostKey(_) => "host_key",
            AppError::Auth(_) => "auth",
            AppError::Connect(_) => "connect",
            AppError::RemoteFs(_) => "remote_fs",
            AppError::Transfer(_) => "transfer",
            AppError::Tunnel(_) => "tunnel",
            AppError::WrongRuntimeContext => "wrong_runtime_context",
            AppError::RuntimeContextSwitching => "runtime_context_switching",
            AppError::RuntimeCleanupFailed => "runtime_cleanup_failed",
            AppError::Io(_) => "io",
            AppError::Other(_) => "other",
        }
    }
}

impl From<serverus_runtime::RuntimeError> for AppError {
    fn from(error: serverus_runtime::RuntimeError) -> Self {
        match error {
            serverus_runtime::RuntimeError::NoActiveContext
            | serverus_runtime::RuntimeError::VaultLocked => AppError::VaultLocked,
            serverus_runtime::RuntimeError::StaleContext
            | serverus_runtime::RuntimeError::DifferentVaultActive
            | serverus_runtime::RuntimeError::VaultAccessEpochExhausted => {
                AppError::WrongRuntimeContext
            }
            serverus_runtime::RuntimeError::SwitchInProgress => AppError::RuntimeContextSwitching,
            serverus_runtime::RuntimeError::Cleanup(_) => AppError::RuntimeCleanupFailed,
        }
    }
}

/// Host-key confirmation payload attached to a `host_key_prompt` error:
/// the UI shows the fingerprint dialog and reconnects on acceptance.
#[derive(Debug, Clone, Serialize, Type)]
pub struct HostKeyPrompt {
    /// Opaque generation that issued this decision. The UI must return it
    /// unchanged so a prompt from a retired Vault cannot mutate the current one.
    pub runtime_context_id: String,
    /// Exact unlock epoch that authorized the connection attempt.
    pub vault_access_epoch: String,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    pub key_line: String,
    /// A different key was stored before — show the scary red variant.
    pub changed: bool,
}

/// Serializable error crossing the IPC boundary.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub host_key: Option<Box<HostKeyPrompt>>,
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError {
            code: e.code().into(),
            message: e.to_string(),
            host_key: None,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub type AppResult<T> = Result<T, AppError>;
pub type ApiResult<T> = Result<T, ApiError>;

const LOCAL_NETWORK_HINT: &str = ". If this host is on your local network, macOS may be \
     blocking Serverus: turn it on in System Settings → Privacy & Security → Local Network, \
     then connect again";

/// macOS 15+ refuses an app's connections to its local network until the
/// user grants the Local Network privilege, and reports the refusal as a bare
/// EHOSTUNREACH — the same "No route to host" a powered-off device gives
/// (Apple TN3179). The first attempt can fail while the permission alert is
/// still on screen. Returns the guidance to append to a connect error whose
/// cause chain holds such an error, or "" otherwise.
pub fn local_network_hint(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if !cfg!(target_os = "macos") {
        return "";
    }
    let mut current = Some(error);
    while let Some(error) = current {
        if io_cause(error).is_some_and(|io| io.kind() == std::io::ErrorKind::HostUnreachable) {
            return LOCAL_NETWORK_HINT;
        }
        current = error.source();
    }
    ""
}

/// The io::Error behind `error`, including the ones russh (a transparent
/// variant) and suppaftp (no `#[source]`) keep out of the `source()` chain.
fn io_cause<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a std::io::Error> {
    if let Some(io) = error.downcast_ref::<std::io::Error>() {
        return Some(io);
    }
    match error.downcast_ref::<russh::Error>() {
        Some(russh::Error::IO(io)) => return Some(io),
        Some(_) => return None,
        None => {}
    }
    match error.downcast_ref::<suppaftp::FtpError>() {
        Some(suppaftp::FtpError::ConnectionError(io)) => Some(io),
        _ => None,
    }
}

/// `AppError::Connect` for a failed connection to `target`, with
/// [`local_network_hint`] appended when it applies.
pub fn connect_error(
    target: impl std::fmt::Display,
    error: &(dyn std::error::Error + 'static),
) -> AppError {
    AppError::Connect(format!("{target}: {error}{}", local_network_hint(error)))
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod runtime_error_mapping_tests;
