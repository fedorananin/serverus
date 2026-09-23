//! Whole-subtree requests: the raw key sweep used by recursive rename and
//! ACL changes, the tree snapshot behind deep folder comparison and
//! recursive delete, and the batched `DeleteObjects` that delete runs on.

use std::collections::BTreeMap;

use aws_sdk_s3::types::{Delete, ObjectIdentifier};

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{EntryFailure, TreeSnapshot, TreeSnapshotItem};

use super::{sdk_err, Loc, S3Fs};

/// Delete files through `DeleteObjects` (at most 1000 keys per request, as
/// S3 allows), grouped per bucket. Quiet mode: the answer lists only the
/// keys that failed. An error means a request was refused as a whole —
/// e.g. by a provider without `DeleteObjects` — and the caller falls back.
pub(super) async fn delete_files(fs: &S3Fs, paths: &[String]) -> AppResult<Vec<EntryFailure>> {
    let mut by_bucket: BTreeMap<String, Vec<(String, &str)>> = BTreeMap::new();
    for path in paths {
        let (bucket, key) = fs.object(path)?;
        by_bucket.entry(bucket).or_default().push((key, path));
    }
    let mut failures = Vec::new();
    for (bucket, objects) in by_bucket {
        for chunk in objects.chunks(crate::session::remote_fs::BULK_DELETE_MAX) {
            let identifiers = chunk
                .iter()
                .map(|(key, _)| ObjectIdentifier::builder().key(key).build())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| AppError::RemoteFs(format!("{bucket}: {e}")))?;
            let delete = Delete::builder()
                .set_objects(Some(identifiers))
                .quiet(true)
                .build()
                .map_err(|e| AppError::RemoteFs(format!("{bucket}: {e}")))?;
            let out = fs
                .client
                .delete_objects()
                .bucket(&bucket)
                .delete(delete)
                .send()
                .await
                .map_err(|e| sdk_err(&bucket, e))?;
            for error in out.errors() {
                let key = error.key().unwrap_or_default();
                let path = chunk
                    .iter()
                    .find(|(candidate, _)| candidate == key)
                    .map_or(key, |(_, path)| *path);
                let reason = error.message().or(error.code()).unwrap_or("delete failed");
                failures.push(EntryFailure {
                    path: path.to_string(),
                    message: format!("{path}: {reason}"),
                });
            }
        }
    }
    Ok(failures)
}

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
    limit: usize,
) -> AppResult<Option<TreeSnapshot>> {
    let (bucket, prefix) = match fs.resolve(path)? {
        // The bucket list has no single-prefix listing; walk it.
        Loc::Root => return Ok(None),
        Loc::Bucket(b) => (b, String::new()),
        Loc::Key(b, k) => (b, format!("{k}/")),
    };
    let mut snapshot = TreeSnapshot::default();
    let mut token: Option<String> = None;
    loop {
        let mut req = fs.client.list_objects_v2().bucket(&bucket);
        if !prefix.is_empty() {
            req = req.prefix(&prefix);
        }
        if let Some(t) = token.take() {
            req = req.continuation_token(t);
        }
        let out = req.send().await.map_err(|e| sdk_err(path, e))?;
        for obj in out.contents() {
            let Some(rel) = obj.key().and_then(|k| k.strip_prefix(prefix.as_str())) else {
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
                return Ok(Some(snapshot));
            }
        }
        match out.next_continuation_token() {
            Some(t) if out.is_truncated() == Some(true) => token = Some(t.to_string()),
            _ => return Ok(Some(snapshot)),
        }
    }
}
