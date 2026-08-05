//! Snapshot-path (bulk remote listing) walker tests.

use std::fs;

use super::{ignore_mtime, local_tree, status, FakeFs, SubtreeStatus, REMOTE};
use crate::session::remote_fs::{join_remote, TreeSnapshot, TreeSnapshotItem};

#[tokio::test]
async fn snapshot_path_matches_and_differs() {
    fn snapshot_fs(b_size: u64) -> FakeFs {
        let fs = FakeFs::default();
        *fs.snapshot.lock().unwrap() = Some(TreeSnapshot {
            items: vec![
                TreeSnapshotItem {
                    rel_path: "a.txt".into(),
                    is_dir: false,
                    size: 3,
                    mtime: None,
                },
                // `sub` is implied by its child only — no explicit marker.
                TreeSnapshotItem {
                    rel_path: "sub/b.txt".into(),
                    is_dir: false,
                    size: b_size,
                    mtime: None,
                },
            ],
            truncated: false,
        });
        fs
    }
    let local = local_tree();
    assert_eq!(
        status(local.path(), &snapshot_fs(5), ignore_mtime(), false).await,
        SubtreeStatus::Matching
    );
    assert_eq!(
        status(local.path(), &snapshot_fs(9), ignore_mtime(), false).await,
        SubtreeStatus::Different
    );
}

#[tokio::test]
async fn snapshot_probes_placeholderless_empty_directories() {
    // Backends can hide empty-prefix placeholders from un-delimited
    // listings; a probe must decide between "empty dir" and "missing dir".
    let local = local_tree();
    fs::remove_file(local.path().join("sub/b.txt")).unwrap();
    let snapshot_items = || {
        Some(TreeSnapshot {
            items: vec![TreeSnapshotItem {
                rel_path: "a.txt".into(),
                is_dir: false,
                size: 3,
                mtime: None,
            }],
            truncated: false,
        })
    };

    let mut fs = FakeFs::default();
    *fs.snapshot.lock().unwrap() = snapshot_items();
    // `sub` exists remotely (probe answers true) but has no objects.
    fs.dirs.insert(join_remote(REMOTE, "sub"), Vec::new());
    assert_eq!(
        status(local.path(), &fs, ignore_mtime(), false).await,
        SubtreeStatus::Matching
    );

    let fs = FakeFs::default();
    *fs.snapshot.lock().unwrap() = snapshot_items();
    // `sub` does not exist remotely at all.
    assert_eq!(
        status(local.path(), &fs, ignore_mtime(), false).await,
        SubtreeStatus::Different
    );
}

#[tokio::test]
async fn truncated_snapshot_reports_unknown() {
    let local = local_tree();
    let fs = FakeFs::default();
    *fs.snapshot.lock().unwrap() = Some(TreeSnapshot {
        items: vec![],
        truncated: true,
    });
    assert_eq!(
        status(local.path(), &fs, ignore_mtime(), false).await,
        SubtreeStatus::Unknown
    );
}

#[tokio::test]
async fn snapshot_directory_placeholder_still_registers_empty_dir() {
    let local = local_tree();
    fs::remove_file(local.path().join("sub/b.txt")).unwrap();
    let fs = FakeFs::default();
    *fs.snapshot.lock().unwrap() = Some(TreeSnapshot {
        items: vec![
            TreeSnapshotItem {
                rel_path: "a.txt".into(),
                is_dir: false,
                size: 3,
                mtime: None,
            },
            TreeSnapshotItem {
                rel_path: "sub".into(),
                is_dir: true,
                size: 0,
                mtime: None,
            },
        ],
        truncated: false,
    });
    assert_eq!(
        status(local.path(), &fs, ignore_mtime(), false).await,
        SubtreeStatus::Matching
    );
}
