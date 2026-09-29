//! Tools that run through the tab's transfer queue, so the user sees them
//! like any other transfer: write_file, upload, download, delete, chmod.

mod status;
mod transfers;
mod tree;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::{Ctx, ToolError, ToolResult};
use crate::error::AppResult;
use crate::transfer::{
    is_settled, Enqueued, ProgressSink, TransferManager, TransferSnapshot, TransferState,
};

pub use status::transfer_status;
pub use transfers::{download, upload, write_file};
pub use tree::{chmod, delete};

/// Longest staged content waits for its upload to settle (a paused item
/// may sit in the queue indefinitely).
const STAGING_LIMIT: Duration = Duration::from_secs(12 * 60 * 60);

pub(super) fn sink(ctx: &Ctx<'_>) -> Arc<dyn ProgressSink> {
    Arc::new(ctx.app.clone())
}

/// A private temporary directory for content the agent sends inline.
/// Removed when dropped: after its upload settled, or right away when the
/// call fails or is cancelled before anything was queued.
pub(super) struct StagingDir(PathBuf);

impl StagingDir {
    pub fn create() -> Result<Self, ToolError> {
        let dir = std::env::temp_dir().join(format!("serverus-agent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir)
            .map_err(|error| ToolError(format!("staging directory: {error}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        Ok(Self(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Enqueue, then follow exactly the items the call added (its batch, which
/// a recursive transfer keeps growing): wait up to `wait` for them and
/// report. They keep going after that; `staging` lives until they settle,
/// even if this call is cancelled meanwhile.
pub(super) async fn run_queued<F, Fut>(
    ctx: &Ctx<'_>,
    wait: u64,
    staging: Option<StagingDir>,
    enqueue: F,
) -> ToolResult
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = AppResult<Enqueued>>,
{
    let transfers = ctx.state().transfers.clone();
    let Enqueued { batch, result } = enqueue().await?;
    if transfers.batch_item_ids(&batch).is_empty() {
        return Err(match result {
            Err(error) => error.into(),
            Ok(()) => "Nothing was queued.".into(),
        });
    }
    if let Some(staging) = staging {
        keep_until_settled(transfers.clone(), batch.clone(), staging);
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait.min(3600));
    let mut items = batch_items(&transfers, &batch);
    while !all_settled(&items) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(250)).await;
        items = batch_items(&transfers, &batch);
    }
    let mut report = describe(&items);
    if let Err(error) = result {
        report.push_str(&format!("\nSome paths could not be queued: {error}"));
    }
    Ok(report)
}

fn batch_items(transfers: &TransferManager, batch: &str) -> Vec<TransferSnapshot> {
    transfers.item_snapshots(&transfers.batch_item_ids(batch))
}

fn all_settled(items: &[TransferSnapshot]) -> bool {
    items.iter().all(|item| is_settled(&item.state))
}

fn keep_until_settled(transfers: Arc<TransferManager>, batch: String, staging: StagingDir) {
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + STAGING_LIMIT;
        while tokio::time::Instant::now() < deadline {
            // Gone from the queue (cleared, tab closed) counts as settled.
            if all_settled(&batch_items(&transfers, &batch)) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        drop(staging);
    });
}

fn state_label(state: &TransferState) -> &'static str {
    match state {
        TransferState::Queued => "queued",
        TransferState::Running => "running",
        TransferState::Paused => "paused",
        TransferState::Conflict => "waiting for a conflict decision",
        TransferState::Done => "done",
        TransferState::Skipped => "skipped (already exists)",
        TransferState::Cancelled => "cancelled",
        TransferState::Error => "failed",
    }
}

/// A per-item report of queue items.
pub(super) fn describe(items: &[TransferSnapshot]) -> String {
    let settled = items.iter().filter(|item| is_settled(&item.state)).count();
    let failed = items
        .iter()
        .filter(|item| matches!(item.state, TransferState::Error | TransferState::Cancelled))
        .count();
    let mut report = format!(
        "{} item(s): {} finished ({} failed or cancelled), {} still going.",
        items.len(),
        settled,
        failed,
        items.len() - settled
    );
    for item in items {
        let kind = format!("{:?}", item.kind).to_lowercase();
        let target = if item.local_path.is_empty() {
            item.remote_path.clone()
        } else {
            format!("{} ↔ {}", item.local_path, item.remote_path)
        };
        report.push_str(&format!(
            "\n- [{}] {kind} {target} ({}/{}) id={}",
            state_label(&item.state),
            item.done,
            item.total,
            item.id
        ));
        if let Some(error) = &item.error {
            report.push_str(&format!(" — {error}"));
        }
    }
    report
}
