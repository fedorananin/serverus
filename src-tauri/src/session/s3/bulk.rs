//! Un-delimited (whole-subtree) listings: the raw key sweep used by
//! recursive delete/rename, and the tree snapshot behind deep folder
//! comparison.

use crate::error::AppResult;
use crate::session::remote_fs::{TreeSnapshot, TreeSnapshotItem};

use super::{sdk_err, S3Fs};

/// All object keys under a prefix (no delimiter — full recursive set).
pub(super) async fn list_all_keys(fs: &S3Fs, bucket: &str, prefix: &str) -> AppResult<Vec<String>> {
    let mut keys = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let mut req = fs.client.list_objects_v2().bucket(bucket);
        if !prefix.is_empty() {
            req = req.prefix(prefix);
        }
        if let Some(t) = token.take() {
            req = req.continuation_token(t);
        }
        let out = req.send().await.map_err(|e| sdk_err(prefix, e))?;
        keys.extend(
            out.contents()
                .iter()
                .filter_map(|o| o.key().map(str::to_string)),
        );
        match out.next_continuation_token() {
            Some(t) if out.is_truncated() == Some(true) => token = Some(t.to_string()),
            _ => break,
        }
    }
    Ok(keys)
}

/// The whole subtree in one un-delimited page loop — one request per 1000
/// objects instead of one `list` per directory. See
/// [`crate::session::remote_fs::RemoteFs::tree_snapshot`].
pub(super) async fn tree_snapshot(
    fs: &S3Fs,
    path: &str,
    bucket: &str,
    prefix: &str,
    limit: usize,
) -> AppResult<TreeSnapshot> {
    let mut snapshot = TreeSnapshot::default();
    let mut token: Option<String> = None;
    loop {
        let mut req = fs.client.list_objects_v2().bucket(bucket);
        if !prefix.is_empty() {
            req = req.prefix(prefix);
        }
        if let Some(t) = token.take() {
            req = req.continuation_token(t);
        }
        let out = req.send().await.map_err(|e| sdk_err(path, e))?;
        for obj in out.contents() {
            let Some(rel) = obj.key().and_then(|k| k.strip_prefix(prefix)) else {
                continue;
            };
            if rel.is_empty() {
                // The listed directory's own placeholder object.
                continue;
            }
            // A trailing slash is a directory placeholder object; plain
            // keys are files (their parent prefixes are implied).
            let (rel_path, is_dir) = match rel.strip_suffix('/') {
                Some(dir) if !dir.is_empty() => (dir.to_string(), true),
                Some(_) => continue,
                None => (rel.to_string(), false),
            };
            snapshot.items.push(TreeSnapshotItem {
                rel_path,
                is_dir,
                size: if is_dir {
                    0
                } else {
                    obj.size().unwrap_or(0).max(0) as u64
                },
                mtime: obj.last_modified().map(|d| d.secs()),
            });
            if snapshot.items.len() > limit {
                snapshot.truncated = true;
                return Ok(snapshot);
            }
        }
        match out.next_continuation_token() {
            Some(t) if out.is_truncated() == Some(true) => token = Some(t.to_string()),
            _ => return Ok(snapshot),
        }
    }
}
