//! Deep folder comparison: does a local directory tree match a remote one?
//!
//! The walk is protocol-agnostic — it only ever talks to [`RemoteFs`] — and
//! compares the two trees breadth-first, level by level, descending only into
//! directory pairs that exist on both sides and stopping at the first
//! difference. Backends with a cheap whole-subtree listing (S3) short-circuit
//! the remote round trips via [`RemoteFs::tree_snapshot`]; the walk itself is
//! identical either way. Per-entry match rules live in
//! `serverus_domain::fs_compare` and are shared with the flat pane comparison.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serverus_domain::fs_compare::{entries_match, CompareRules, EntryMeta};
use specta::Type;

use crate::error::{AppError, AppResult};
use crate::local_fs;
use crate::session::remote_fs::{join_remote, RemoteEntry, RemoteFs, TreeSnapshot};

/// Hard ceiling on entries examined per comparison (both sides combined).
/// Past it the honest answer is "not compared", never a guessed status.
pub const MAX_COMPARED_ENTRIES: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum SubtreeStatus {
    Matching,
    Different,
    /// Could not be verified: the tree is over [`MAX_COMPARED_ENTRIES`] or a
    /// local directory was unreadable. Deliberately not "matching".
    Unknown,
}

/// Cancellation epochs, one per session. Cancelling bumps the session's
/// epoch; a walk captured on an older epoch stops at its next directory.
/// Entries are never removed — the map is bounded by the number of session
/// ids ever compared, a handful of small strings.
#[derive(Default)]
pub struct CompareRegistry {
    epochs: std::sync::Mutex<HashMap<String, u64>>,
}

impl CompareRegistry {
    pub fn epoch(&self, session_id: &str) -> u64 {
        *self.epochs.lock().unwrap().get(session_id).unwrap_or(&0)
    }

    pub fn cancel(&self, session_id: &str) {
        *self
            .epochs
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_insert(0) += 1;
    }
}

fn meta(entry: &RemoteEntry) -> EntryMeta {
    EntryMeta {
        name: entry.name.clone(),
        is_dir: entry.is_dir,
        is_symlink: entry.is_symlink,
        size: entry.size,
        mtime: entry.mtime,
    }
}

/// Children of every directory in a snapshot, keyed by `/`-separated path
/// relative to the snapshot root (`""` = the root itself). Parent
/// directories implied by deeper paths are materialized; when a key exists
/// both as a placeholder object and an implied prefix (or, pathologically,
/// as an object *and* a prefix), the directory wins.
fn snapshot_children(snapshot: TreeSnapshot) -> HashMap<String, HashMap<String, EntryMeta>> {
    let mut dirs: HashMap<String, HashMap<String, EntryMeta>> = HashMap::new();
    dirs.entry(String::new()).or_default();
    for item in snapshot.items {
        let parts: Vec<&str> = item.rel_path.split('/').collect();
        for depth in 0..parts.len() {
            let parent = parts[..depth].join("/");
            let name = parts[depth];
            let children = dirs.entry(parent).or_default();
            let is_leaf = depth == parts.len() - 1;
            let is_dir = !is_leaf || item.is_dir;
            let entry = EntryMeta {
                name: name.to_string(),
                is_dir,
                is_symlink: false,
                size: if is_dir { 0 } else { item.size },
                mtime: if is_dir { None } else { item.mtime },
            };
            match children.get(name) {
                Some(existing) if existing.is_dir || !is_dir => {}
                _ => {
                    children.insert(name.to_string(), entry);
                }
            }
        }
    }
    dirs
}

async fn list_local(dir: String) -> Option<Vec<EntryMeta>> {
    let listed = tokio::task::spawn_blocking(move || local_fs::list(&dir))
        .await
        .ok()?
        .ok()?;
    Some(listed.iter().map(meta).collect())
}

fn join_rel(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_string()
    } else {
        format!("{rel}/{name}")
    }
}

fn join_local(dir: &str, name: &str) -> String {
    Path::new(dir).join(name).to_string_lossy().into_owned()
}

/// Which entries the walk looks at — the panes' own display filters, so the
/// comparison never disagrees with what the user sees.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompareFilter {
    /// Compare dot-entries too (the panes' "show hidden" setting).
    pub include_hidden: bool,
    /// Drop OS metadata junk (`.DS_Store`, `Thumbs.db`) from the *local*
    /// side only, mirroring the "hide local junk" panel setting. The remote
    /// side keeps such files visible so stray uploads surface as
    /// differences until they are deleted.
    pub hide_local_junk: bool,
}

fn visible(entries: Vec<EntryMeta>, include_hidden: bool) -> Vec<EntryMeta> {
    if include_hidden {
        return entries;
    }
    entries
        .into_iter()
        .filter(|entry| !entry.name.starts_with('.'))
        .collect()
}

fn visible_local(entries: Vec<EntryMeta>, filter: CompareFilter) -> Vec<EntryMeta> {
    let entries = visible(entries, filter.include_hidden);
    if !filter.hide_local_junk {
        return entries;
    }
    entries
        .into_iter()
        .filter(|entry| !serverus_domain::fs_compare::is_local_junk(&entry.name))
        .collect()
}

/// Compare the tree under `local_root` with the tree under `remote_root`.
///
/// Returns [`SubtreeStatus::Different`] as soon as any level differs, so a
/// tree that diverges early costs a single listing per side. Symlinked
/// directories are compared as symlinks and never descended into (no
/// cycles). `filter` applies the panes' display filters at every level.
pub async fn compare_subtree(
    fs: &dyn RemoteFs,
    local_root: &str,
    remote_root: &str,
    rules: CompareRules,
    filter: CompareFilter,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> AppResult<SubtreeStatus> {
    let snapshot = match fs.tree_snapshot(remote_root, MAX_COMPARED_ENTRIES).await? {
        Some(snapshot) if snapshot.truncated => return Ok(SubtreeStatus::Unknown),
        Some(snapshot) => Some(snapshot_children(snapshot)),
        None => None,
    };

    let mut visited = 0_usize;
    // (absolute local dir, `/`-relative subtree path)
    let mut pending = vec![(local_root.to_string(), String::new())];
    while let Some((local_dir, rel)) = pending.pop() {
        if cancelled() {
            return Err(AppError::Other("folder comparison cancelled".into()));
        }
        let Some(locals) = list_local(local_dir.clone()).await else {
            return Ok(SubtreeStatus::Unknown);
        };
        let locals = visible_local(locals, filter);
        let remotes = match &snapshot {
            Some(dirs) => dirs
                .get(&rel)
                .map(|children| children.values().cloned().collect())
                .unwrap_or_default(),
            None => {
                let remote_dir = if rel.is_empty() {
                    remote_root.to_string()
                } else {
                    join_remote(remote_root, &rel)
                };
                fs.list(&remote_dir).await?.iter().map(meta).collect()
            }
        };
        let remotes = visible(remotes, filter.include_hidden);

        visited += locals.len() + remotes.len();
        if visited > MAX_COMPARED_ENTRIES {
            return Ok(SubtreeStatus::Unknown);
        }

        let mut remote_by_name: HashMap<&str, &EntryMeta> = remotes
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        for local in &locals {
            let Some(remote) = remote_by_name.remove(local.name.as_str()) else {
                // A snapshot only sees objects. An empty remote "directory"
                // that exists without a placeholder object (or whose
                // placeholder the backend hides, as s3s does) is invisible
                // in it — confirm with one targeted probe before calling a
                // local directory one-sided.
                if snapshot.is_some() && local.is_dir && !local.is_symlink {
                    let child_rel = join_rel(&rel, &local.name);
                    // Quirky backends answer this probe for directory-like
                    // keys with odd errors; "can't verify" beats a guess.
                    let Ok(present) = fs.exists(&join_remote(remote_root, &child_rel)).await else {
                        return Ok(SubtreeStatus::Unknown);
                    };
                    if present {
                        // Present but empty of objects — still walk it, so
                        // local-only children inside are found.
                        pending.push((join_local(&local_dir, &local.name), child_rel));
                        continue;
                    }
                }
                return Ok(SubtreeStatus::Different);
            };
            if !entries_match(local, remote, rules) {
                return Ok(SubtreeStatus::Different);
            }
            if local.is_dir && !local.is_symlink {
                pending.push((
                    join_local(&local_dir, &local.name),
                    join_rel(&rel, &local.name),
                ));
            }
        }
        if !remote_by_name.is_empty() {
            return Ok(SubtreeStatus::Different);
        }
    }
    Ok(SubtreeStatus::Matching)
}

#[cfg(test)]
mod tests;
