use std::fs;

use serverus_domain::fs_compare::CompareRules;
use serverus_lib::session::compare::{compare_subtree, CompareFilter, SubtreeStatus};
use serverus_lib::session::remote_fs::RemoteFs;
use serverus_lib::session::s3::S3Fs;

use super::common::fs_for;
use super::server::spawn_s3;

async fn put(fs: &S3Fs, path: &str, bytes: &[u8]) {
    let mut writer = fs.open_write(path, 0).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut writer, bytes)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut writer)
        .await
        .unwrap();
}

/// Deep folder comparison against a real S3 server — this walk goes through
/// the `tree_snapshot` fast path (one un-delimited listing), with the
/// ignore-mtime rules the frontend always passes for S3.
pub(crate) async fn subtree_compare() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    let fs = fs_for(port, None);
    fs.probe().await.unwrap();

    fs.mkdir("/bucket").await.unwrap();
    put(&fs, "/bucket/site/top.txt", b"top").await;
    put(&fs, "/bucket/site/sub/nested.txt", b"nested").await;

    let local = tempfile::tempdir().unwrap();
    fs::create_dir_all(local.path().join("sub")).unwrap();
    fs::write(local.path().join("top.txt"), b"top").unwrap();
    fs::write(local.path().join("sub/nested.txt"), b"nested").unwrap();

    let rules = CompareRules {
        ignore_mtime: true,
        ..CompareRules::default()
    };
    let local_root = local.path().to_string_lossy().into_owned();
    let status = |remote_root: &'static str| {
        let local_root = local_root.clone();
        let fs = &fs;
        async move {
            let filter = CompareFilter::default();
            compare_subtree(
                fs.as_ref(),
                &local_root,
                remote_root,
                rules,
                filter,
                &|| false,
            )
            .await
            .unwrap()
        }
    };

    assert_eq!(status("/bucket/site").await, SubtreeStatus::Matching);

    // A nested size change is found through the snapshot.
    put(&fs, "/bucket/site/sub/nested.txt", b"nested-changed").await;
    assert_eq!(status("/bucket/site").await, SubtreeStatus::Different);
    put(&fs, "/bucket/site/sub/nested.txt", b"nested").await;
    assert_eq!(status("/bucket/site").await, SubtreeStatus::Matching);

    // A remote-only key deep in the tree is found.
    put(&fs, "/bucket/site/sub/extra.txt", b"x").await;
    assert_eq!(status("/bucket/site").await, SubtreeStatus::Different);
    fs.delete_file("/bucket/site/sub/extra.txt").await.unwrap();

    // A local-only empty directory is found (no remote placeholder).
    fs::create_dir(local.path().join("local-only")).unwrap();
    assert_eq!(status("/bucket/site").await, SubtreeStatus::Different);
}
