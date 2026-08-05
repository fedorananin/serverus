use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serverus_domain::fs_compare::CompareRules;

use super::{compare_subtree, CompareFilter, SubtreeStatus};
use crate::error::AppResult;
use crate::session::remote_fs::{
    join_remote, BoxRead, BoxWrite, RemoteEntry, RemoteFs, TreeSnapshot,
};

/// Listing-only fake: `dirs` maps absolute remote paths to their children;
/// an optional snapshot exercises the bulk path instead.
#[derive(Default)]
struct FakeFs {
    dirs: HashMap<String, Vec<RemoteEntry>>,
    snapshot: Mutex<Option<TreeSnapshot>>,
}

fn entry(name: &str, is_dir: bool, size: u64, mtime: Option<i64>) -> RemoteEntry {
    RemoteEntry {
        name: name.to_string(),
        path: String::new(),
        is_dir,
        is_symlink: false,
        size,
        mtime,
        permissions: None,
    }
}

#[async_trait::async_trait]
impl RemoteFs for FakeFs {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        Ok(self
            .dirs
            .get(path)
            .unwrap_or_else(|| panic!("unexpected list of {path}"))
            .clone())
    }
    async fn tree_snapshot(&self, _path: &str, _limit: usize) -> AppResult<Option<TreeSnapshot>> {
        Ok(self.snapshot.lock().unwrap().take())
    }
    async fn stat(&self, _path: &str) -> AppResult<RemoteEntry> {
        unimplemented!()
    }
    async fn home_dir(&self) -> AppResult<String> {
        unimplemented!()
    }
    async fn mkdir(&self, _path: &str) -> AppResult<()> {
        unimplemented!()
    }
    async fn create_file(&self, _path: &str) -> AppResult<()> {
        unimplemented!()
    }
    async fn rename(&self, _from: &str, _to: &str) -> AppResult<()> {
        unimplemented!()
    }
    async fn delete_file(&self, _path: &str) -> AppResult<()> {
        unimplemented!()
    }
    async fn delete_dir(&self, _path: &str) -> AppResult<()> {
        unimplemented!()
    }
    async fn chmod(&self, _path: &str, _mode: u32) -> AppResult<()> {
        unimplemented!()
    }
    async fn set_mtime(&self, _path: &str, _mtime_unix: i64) -> AppResult<()> {
        unimplemented!()
    }
    async fn open_read(&self, _path: &str, _offset: u64) -> AppResult<BoxRead> {
        unimplemented!()
    }
    async fn open_write(&self, _path: &str, _offset: u64) -> AppResult<BoxWrite> {
        unimplemented!()
    }
    async fn exists(&self, path: &str) -> AppResult<bool> {
        Ok(self.dirs.contains_key(path))
    }
}

const REMOTE: &str = "/remote";
const NOT_CANCELLED: &(dyn Fn() -> bool + Sync) = &|| false;

fn ignore_mtime() -> CompareRules {
    CompareRules {
        ignore_mtime: true,
        ..CompareRules::default()
    }
}

/// A local tree `a.txt (3 bytes)`, `sub/b.txt (5 bytes)`.
fn local_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"abc").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/b.txt"), b"hello").unwrap();
    dir
}

fn matching_remote() -> FakeFs {
    let mut fs = FakeFs::default();
    fs.dirs.insert(
        REMOTE.into(),
        vec![entry("a.txt", false, 3, None), entry("sub", true, 0, None)],
    );
    fs.dirs.insert(
        join_remote(REMOTE, "sub"),
        vec![entry("b.txt", false, 5, None)],
    );
    fs
}

async fn status(local: &Path, fs: &FakeFs, rules: CompareRules, hidden: bool) -> SubtreeStatus {
    let filter = CompareFilter {
        include_hidden: hidden,
        ..CompareFilter::default()
    };
    status_filtered(local, fs, rules, filter).await
}

async fn status_filtered(
    local: &Path,
    fs: &FakeFs,
    rules: CompareRules,
    filter: CompareFilter,
) -> SubtreeStatus {
    compare_subtree(
        fs,
        &local.to_string_lossy(),
        REMOTE,
        rules,
        filter,
        NOT_CANCELLED,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn matching_trees_match() {
    let local = local_tree();
    assert_eq!(
        status(local.path(), &matching_remote(), ignore_mtime(), false).await,
        SubtreeStatus::Matching
    );
}

#[tokio::test]
async fn nested_size_difference_detected() {
    let local = local_tree();
    let mut remote = matching_remote();
    remote.dirs.insert(
        join_remote(REMOTE, "sub"),
        vec![entry("b.txt", false, 6, None)],
    );
    assert_eq!(
        status(local.path(), &remote, ignore_mtime(), false).await,
        SubtreeStatus::Different
    );
}

#[tokio::test]
async fn nested_local_only_file_detected() {
    let local = local_tree();
    fs::write(local.path().join("sub/extra.txt"), b"x").unwrap();
    assert_eq!(
        status(local.path(), &matching_remote(), ignore_mtime(), false).await,
        SubtreeStatus::Different
    );
}

#[tokio::test]
async fn nested_remote_only_file_detected() {
    let local = local_tree();
    let mut remote = matching_remote();
    remote
        .dirs
        .get_mut(&join_remote(REMOTE, "sub"))
        .unwrap()
        .push(entry("extra.txt", false, 1, None));
    assert_eq!(
        status(local.path(), &remote, ignore_mtime(), false).await,
        SubtreeStatus::Different
    );
}

#[tokio::test]
async fn mtime_mismatch_detected_when_not_ignored() {
    let local = local_tree();
    let mut remote = matching_remote();
    // Local files carry a real (recent) mtime; an ancient remote stamp on
    // equal sizes must surface under the default rules.
    remote.dirs.insert(
        REMOTE.into(),
        vec![
            entry("a.txt", false, 3, Some(1)),
            entry("sub", true, 0, None),
        ],
    );
    assert_eq!(
        status(local.path(), &remote, CompareRules::default(), false).await,
        SubtreeStatus::Different
    );
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_directories_are_not_descended() {
    let local = local_tree();
    std::os::unix::fs::symlink(local.path().join("sub"), local.path().join("link")).unwrap();
    let mut remote = matching_remote();
    let mut link = entry("link", true, 0, None);
    link.is_symlink = true;
    remote.dirs.get_mut(REMOTE).unwrap().push(link);
    // No `/remote/link` listing exists in the fake — descending would panic.
    assert_eq!(
        status(local.path(), &remote, ignore_mtime(), false).await,
        SubtreeStatus::Matching
    );
}

#[tokio::test]
async fn oversized_tree_reports_unknown() {
    let local = local_tree();
    let mut remote = matching_remote();
    let huge = (0..=super::MAX_COMPARED_ENTRIES)
        .map(|i| entry(&format!("f{i}"), false, 1, None))
        .collect();
    remote.dirs.insert(REMOTE.into(), huge);
    assert_eq!(
        status(local.path(), &remote, ignore_mtime(), false).await,
        SubtreeStatus::Unknown
    );
}

#[tokio::test]
async fn unreadable_local_root_reports_unknown() {
    let missing = Path::new("/nonexistent-serverus-compare-root");
    assert_eq!(
        status(missing, &FakeFs::default(), ignore_mtime(), false).await,
        SubtreeStatus::Unknown
    );
}

#[tokio::test]
async fn cancellation_stops_the_walk() {
    let local = local_tree();
    let result = compare_subtree(
        &matching_remote(),
        &local.path().to_string_lossy(),
        REMOTE,
        ignore_mtime(),
        CompareFilter::default(),
        &|| true,
    )
    .await;
    assert!(result.is_err());
}

mod filters;
mod snapshot;
