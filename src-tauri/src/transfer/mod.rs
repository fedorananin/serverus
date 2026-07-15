//! Transfer queue (SPEC §6): parallel per-server workers, progress events,
//! pause/resume/cancel, conflict handling through the protocol-neutral RemoteFs.

mod activity;
mod batch;
mod cleanup;
mod contracts;
mod controls;
mod enqueue_download;
mod enqueue_upload;
mod item;
mod item_lifecycle;
mod lifecycle;
mod local_target;
mod manager;
mod partial;
mod progress;
mod requests;
mod single_transfer;
mod sink;
mod tree_size;
mod worker;

pub mod tar_stream;

pub use contracts::{
    ConflictAction, TransferKind, TransferSnapshot, TransferState, TransferSummary,
};
pub use manager::TransferManager;
pub use requests::{DownloadRequest, UploadRequest};
pub use sink::ProgressSink;

pub(crate) use local_target::safe_local_component;
#[cfg(test)]
pub(crate) use local_target::sanitize_windows_component;

use activity::{ActivityRegistry, AdmissionToken, ServerQueue};
use batch::TransferBatch;
pub use item::TransferItem;
use lifecycle::{domain_conflict_decision, Control, RetryClaim, TransferLifecycle, AUTO_RETRIES};
use local_target::{
    ensure_download_directory, local_target_exists, open_download_root, open_local_download,
    LocalDownloadTarget,
};
use single_transfer::run_single;
use tree_size::{copy_loop, local_tree_size, remote_tree_size};

const CHUNK: usize = 128 * 1024;
const SNAPSHOT_LIMIT: usize = 200;

#[cfg(test)]
mod tests;
