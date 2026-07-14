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
    #[error("vault context is closed")]
    VaultContextClosed,
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
            AppError::VaultContextClosed => "vault_context_closed",
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
            AppError::Io(_) => "io",
            AppError::Other(_) => "other",
        }
    }
}

/// Host-key confirmation payload attached to a `host_key_prompt` error:
/// the UI shows the fingerprint dialog and reconnects on acceptance.
#[derive(Debug, Clone, Serialize, Type)]
pub struct HostKeyPrompt {
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
    pub host_key: Option<HostKeyPrompt>,
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
