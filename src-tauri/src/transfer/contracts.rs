use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    Upload,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Queued,
    Running,
    Paused,
    /// Waiting for the user's overwrite/skip/rename decision.
    Conflict,
    Done,
    Skipped,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictAction {
    Overwrite,
    Skip,
    Rename,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TransferSnapshot {
    pub id: String,
    pub session_id: String,
    pub kind: TransferKind,
    pub state: TransferState,
    pub error: Option<String>,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub accelerated: bool,
    #[specta(type = specta_typescript::Number)]
    pub done: u64,
    #[specta(type = specta_typescript::Number)]
    pub total: u64,
    #[specta(type = specta_typescript::Number)]
    pub speed_bps: u64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TransferSummary {
    pub queued: u32,
    pub running: u32,
    pub done: u32,
    pub failed: u32,
    pub total_items: u32,
}
