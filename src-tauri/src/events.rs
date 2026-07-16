//! Events streamed from the backend to the frontend (SPEC §7.1).

use serde::{Deserialize, Serialize};
use specta::Type;

/// The vault was locked (auto-lock, sleep, or explicit).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct VaultLockedEvent;

/// Session lifecycle: `connecting` → `connected` → `disconnected` / `error`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct SessionStateEvent {
    pub session_id: String,
    pub connection_id: String,
    pub state: String,
    pub message: Option<String>,
}

/// Periodic transfer queue snapshot (~4 Hz while transfers are active).
#[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
pub struct TransferProgressEvent {
    pub runtime_context_id: String,
    pub items: Vec<crate::transfer::TransferSnapshot>,
    pub summary: crate::transfer::TransferSummary,
    /// Exact per-session counts — the transfer panel is rendered per tab.
    pub session_summaries: std::collections::HashMap<String, crate::transfer::TransferSummary>,
}

/// A remote-edited file was saved and uploaded back ("Uploaded ✓" toast,
/// SPEC §5.3). `error` is set when the auto-upload failed.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct RemoteEditUploadedEvent {
    pub name: String,
    pub remote_path: String,
    pub error: Option<String>,
}
