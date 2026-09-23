//! Recursive operations over a remote tree — delete and recursive chmod —
//! with observable progress, pause/cancel checkpoints, bounded parallelism
//! and per-entry failure collection: one bad entry never aborts the rest.
//!
//! Two phases. The scan learns the whole tree first, so progress has a real
//! total; it costs exactly the `list` calls a one-pass walk would make (or a
//! single [`RemoteFs::tree_snapshot`] sweep on S3). The apply phase then
//! handles files first and directories deepest-first, which is the order a
//! delete needs and keeps a restrictive chmod from locking itself out.
//! Everything goes through [`RemoteFs`], so SFTP, FTP and S3 share it.

mod apply;
mod scan;
#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::error::{AppError, AppResult};

use super::RemoteFs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeAction {
    Delete,
    /// Recursive chmod. The root counts as a directory. Symlinks are never
    /// touched — SETSTAT follows them out of the tree — just like `chmod -R`.
    Chmod {
        mode: u32,
        files: bool,
        dirs: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeOutcome {
    Completed,
    Cancelled,
}

/// Live counters the caller renders. `total` grows while `scanning` is set
/// and is exact afterwards; `done` counts processed entries, failed ones
/// included, so a finished run always reads `done == total`.
#[derive(Clone, Copy)]
pub struct TreeProgress<'a> {
    pub total: &'a AtomicU64,
    pub done: &'a AtomicU64,
    pub scanning: &'a AtomicBool,
}

/// Pause/cancel hook, polled between remote requests.
#[async_trait::async_trait]
pub trait Checkpoint: Send + Sync {
    /// Wait while paused; `false` means cancelled — stop now.
    async fn proceed(&self) -> bool;
}

/// A checkpoint that never pauses or cancels.
pub struct Unattended;

#[async_trait::async_trait]
impl Checkpoint for Unattended {
    async fn proceed(&self) -> bool {
        true
    }
}

/// One entry an operation could not process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryFailure {
    pub path: String,
    pub message: String,
}

impl EntryFailure {
    pub(crate) fn new(path: &str, error: &AppError) -> Self {
        let message = match error {
            // The protocol adapters already prefix their messages with the path.
            AppError::RemoteFs(message) => message.clone(),
            other => format!("{path}: {other}"),
        };
        Self {
            path: path.to_string(),
            message,
        }
    }
}

/// Run `action` on `root` (a directory tree when `is_dir`, otherwise one
/// entry). Per-entry failures are collected and reported together as one
/// error once everything else has been processed.
pub async fn run_tree_action(
    fs: &dyn RemoteFs,
    root: &str,
    is_dir: bool,
    action: TreeAction,
    progress: TreeProgress<'_>,
    checkpoint: &dyn Checkpoint,
) -> AppResult<TreeOutcome> {
    if action == TreeAction::Delete && root.trim_matches('/').is_empty() {
        return Err(AppError::RemoteFs("refusing to delete '/'".into()));
    }
    progress.done.store(0, Ordering::Relaxed);
    progress.total.store(0, Ordering::Relaxed);
    progress.scanning.store(true, Ordering::Relaxed);
    let scanned = if is_dir {
        scan::scan(fs, root, progress, checkpoint).await
    } else {
        Ok(Some(scan::TreePlan::single(root)))
    };
    progress.scanning.store(false, Ordering::Relaxed);
    let Some(plan) = scanned? else {
        return Ok(TreeOutcome::Cancelled);
    };
    apply::apply(fs, root, plan, action, progress, checkpoint).await
}

/// Summarise collected failures into the one error the caller shows.
fn failure_error(action: TreeAction, failures: &[EntryFailure], total: u64) -> AppError {
    const SHOWN: usize = 3;
    let verb = match action {
        TreeAction::Delete => "deleted",
        TreeAction::Chmod { .. } => "changed",
    };
    let mut details = failures
        .iter()
        .take(SHOWN)
        .map(|failure| failure.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    if failures.len() > SHOWN {
        details.push_str(&format!("; …and {} more", failures.len() - SHOWN));
    }
    AppError::RemoteFs(format!(
        "{} of {total} entries could not be {verb} — {details}",
        failures.len()
    ))
}
