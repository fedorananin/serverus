//! Scan phase: learn the whole tree before touching anything.

use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use crate::error::AppResult;
use crate::session::remote_fs::{join_remote, RemoteFs, TreeSnapshot};

use super::{Checkpoint, EntryFailure, TreeProgress};

/// A non-directory entry: a file or a symlink (even one leading to a
/// directory — symlinks are never descended).
pub(super) struct PlanFile {
    pub(super) path: String,
    pub(super) is_symlink: bool,
}

/// Everything under one root, as found by the scan.
#[derive(Default)]
pub(super) struct TreePlan {
    pub(super) files: Vec<PlanFile>,
    /// Directories with their depth below the root (the root itself is 0).
    pub(super) dirs: Vec<(usize, String)>,
    /// Subdirectories that could not be listed; their contents are unknown,
    /// so neither they nor their ancestors can be removed.
    pub(super) failures: Vec<EntryFailure>,
}

impl TreePlan {
    pub(super) fn single(path: &str) -> Self {
        Self {
            files: vec![PlanFile {
                path: path.to_string(),
                is_symlink: false,
            }],
            ..Self::default()
        }
    }
}

/// `Ok(None)` = cancelled mid-scan. A root that cannot be listed is a hard
/// error — there is nothing sensible to do below it.
pub(super) async fn scan(
    fs: &dyn RemoteFs,
    root: &str,
    progress: TreeProgress<'_>,
    checkpoint: &dyn Checkpoint,
) -> AppResult<Option<TreePlan>> {
    if !checkpoint.proceed().await {
        return Ok(None);
    }
    if let Some(snapshot) = fs.tree_snapshot(root, usize::MAX).await? {
        if !snapshot.truncated {
            let plan = from_snapshot(root, snapshot);
            let total = plan.files.len() + plan.dirs.len();
            progress.total.store(total as u64, Ordering::Relaxed);
            return Ok(Some(plan));
        }
    }
    walk(fs, root, progress, checkpoint).await
}

/// Per-directory `list` walk for connection-oriented protocols.
async fn walk(
    fs: &dyn RemoteFs,
    root: &str,
    progress: TreeProgress<'_>,
    checkpoint: &dyn Checkpoint,
) -> AppResult<Option<TreePlan>> {
    let mut plan = TreePlan::default();
    let mut pending = vec![(0_usize, root.to_string())];
    while let Some((depth, dir)) = pending.pop() {
        if !checkpoint.proceed().await {
            return Ok(None);
        }
        let entries = match fs.list(&dir).await {
            Ok(entries) => entries,
            Err(error) if depth == 0 => return Err(error),
            Err(error) => {
                plan.failures.push(EntryFailure::new(&dir, &error));
                continue;
            }
        };
        plan.dirs.push((depth, dir));
        progress
            .total
            .fetch_add(1 + entries.len() as u64, Ordering::Relaxed);
        for entry in entries {
            if entry.is_dir && !entry.is_symlink {
                // Counted now as an entry of its parent; its own listing
                // adds only its children.
                progress.total.fetch_sub(1, Ordering::Relaxed);
                pending.push((depth + 1, entry.path));
            } else {
                plan.files.push(PlanFile {
                    path: entry.path,
                    is_symlink: entry.is_symlink,
                });
            }
        }
    }
    Ok(Some(plan))
}

/// Object stores list the whole subtree at once. Directories exist only as
/// optional placeholder objects, so every prefix implied by a key is one too.
fn from_snapshot(root: &str, snapshot: TreeSnapshot) -> TreePlan {
    let mut plan = TreePlan::default();
    let mut dirs = BTreeSet::new();
    for item in snapshot.items {
        let mut parent = item.rel_path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            dirs.insert(prefix.to_string());
            parent = prefix;
        }
        if item.is_dir {
            dirs.insert(item.rel_path);
        } else {
            plan.files.push(PlanFile {
                path: join_remote(root, &item.rel_path),
                is_symlink: false,
            });
        }
    }
    plan.dirs.push((0, root.to_string()));
    plan.dirs.extend(
        dirs.into_iter()
            .map(|rel| (rel.matches('/').count() + 1, join_remote(root, &rel))),
    );
    plan
}
