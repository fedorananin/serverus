//! Events streamed from the backend to the frontend (SPEC §7.1).

use serde::{Deserialize, Serialize};
use specta::Type;

/// The vault was locked (auto-lock, sleep, or explicit).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct VaultLockedEvent {
    #[specta(type = specta_typescript::Number)]
    pub context_epoch: u64,
}

/// Session lifecycle: `connecting` → `connected` → `disconnected` / `error`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct SessionStateEvent {
    #[specta(type = specta_typescript::Number)]
    pub context_epoch: u64,
    pub session_id: String,
    pub connection_id: String,
    pub state: String,
    pub message: Option<String>,
}

/// Batched terminal output (base64-encoded raw bytes, ~16 ms cadence).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct TerminalDataEvent {
    #[specta(type = specta_typescript::Number)]
    pub context_epoch: u64,
    pub term_id: String,
    pub data: String,
}

/// The remote shell ended (exit, EOF or channel close).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct TerminalExitEvent {
    #[specta(type = specta_typescript::Number)]
    pub context_epoch: u64,
    pub term_id: String,
}

/// Periodic transfer queue snapshot (~4 Hz while transfers are active).
#[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
pub struct TransferProgressEvent {
    #[specta(type = specta_typescript::Number)]
    pub context_epoch: u64,
    pub items: Vec<crate::transfer::TransferSnapshot>,
    pub summary: crate::transfer::TransferSummary,
}

/// A remote-edited file was saved and uploaded back ("Uploaded ✓" toast,
/// SPEC §5.3). `error` is set when the auto-upload failed.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RemoteEditUploadedEvent {
    #[specta(type = specta_typescript::Number)]
    pub context_epoch: u64,
    pub name: String,
    pub remote_path: String,
    pub error: Option<String>,
}
