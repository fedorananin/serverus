//! DTOs exposed by the Tauri command surface.

use serde::Serialize;
use specta::Type;

use crate::transfer::{TransferSnapshot, TransferSummary};
use crate::vault::model::PublicVault;

#[derive(Debug, Clone, Serialize, Type)]
pub struct VaultInfo {
    pub path: String,
    pub exists: bool,
    pub unlocked: bool,
    pub biometry_available: bool,
    /// A DEK for this vault is stored behind biometrics — Touch ID unlock
    /// can be offered right away.
    pub quick_unlock_ready: bool,
    /// UI label for the platform's quick-unlock mechanism
    /// ("Touch ID" / "Windows Hello").
    pub quick_unlock_method: String,
    /// True only in `scenario-tests` builds. The lock screen shows a typed
    /// vault-path field then — WebDriver cannot drive the native file
    /// pickers — and hides it from real users otherwise.
    pub scenario_build: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct SessionDto {
    pub session_id: String,
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TransferListDto {
    pub runtime_context_id: String,
    pub items: Vec<TransferSnapshot>,
    pub summary: TransferSummary,
    /// Exact per-session counts — the transfer panel is rendered per tab.
    pub session_summaries: std::collections::HashMap<String, TransferSummary>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ImportReport {
    /// Number of connections created or updated by the import.
    pub connections: u32,
    pub vault: PublicVault,
}

/// Decrypted secrets for one connection, for pre-filling the edit form.
/// Safe: the vault is already unlocked (master password / Touch ID), and the
/// values are never persisted outside the encrypted vault.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ConnectionSecrets {
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
    pub key_inline: Option<String>,
}
