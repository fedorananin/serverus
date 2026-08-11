use std::fs;
use std::path::Path;

use serverus_domain::fs_compare::CompareRules;
use serverus_lib::session::compare::{compare_subtree, CompareFilter, SubtreeStatus};

use serverus_lib::session::remote_fs::RemoteFs;

use super::{pool_for, spawn_ftp};

/// The connect-time FEAT probe drives `preserves_mtime`: libunftp does not
/// advertise MFMT, so a probed pool must report that uploads cannot keep
/// their mtime (the comparison UI then falls back to size-only matching).
/// Before the probe the pool stays optimistic — best-effort MFMT, exactly
/// the pre-probe behavior.
#[tokio::test]
async fn ftp_feat_probe_drives_preserves_mtime() {
    let server_root = tempfile::tempdir().unwrap();
    let port = spawn_ftp(server_root.path()).await;
    let pool = pool_for(port);

    assert!(pool.preserves_mtime());
    pool.probe().await.unwrap();
    assert!(!pool.preserves_mtime());
}

/// Deep folder comparison over FTP: the same walker the panes use, with the
/// coarse-mtime rules the frontend passes for FTP listings.
#[tokio::test]
async fn ftp_subtree_compare() {
    let server_root = tempfile::tempdir().unwrap();
    let port = spawn_ftp(server_root.path()).await;
    let pool = pool_for(port);

    fn write_tree(root: &Path) {
        fs::create_dir_all(root.join("sub/inner")).unwrap();
        fs::write(root.join("top.txt"), b"top").unwrap();
        fs::write(root.join("sub/nested.txt"), b"nested").unwrap();
        fs::write(root.join("sub/inner/deep.txt"), b"deep").unwrap();
    }
    let local = tempfile::tempdir().unwrap();
    write_tree(local.path());
    let remote = server_root.path().join("cmp");
    write_tree(&remote);

    // This test pins the walk over a real FTP server, not the timestamp
    // rules: how a LIST date round-trips through libunftp and suppaftp
    // depends on the machine's timezone, so asserting on mtimes here would
    // flake. The minute/date precision rules are unit-tested in
    // `serverus_domain::fs_compare`.
    let rules = CompareRules {
        ignore_mtime: true,
        ..CompareRules::default()
    };
    let local_root = local.path().to_string_lossy().into_owned();
    let status = |hidden: bool| {
        let local_root = local_root.clone();
        let pool = pool.clone();
        async move {
            let filter = CompareFilter {
                include_hidden: hidden,
                ..CompareFilter::default()
            };
            compare_subtree(pool.as_ref(), &local_root, "/cmp", rules, filter, &|| false)
                .await
                .unwrap()
        }
    };

    assert_eq!(status(false).await, SubtreeStatus::Matching);

    // A nested size change two levels down is found.
    fs::write(remote.join("sub/inner/deep.txt"), b"deep-changed").unwrap();
    assert_eq!(status(false).await, SubtreeStatus::Different);

    // A nested remote-only file, then a local-only directory, are found.
    fs::write(remote.join("sub/inner/deep.txt"), b"deep").unwrap();
    assert_eq!(status(false).await, SubtreeStatus::Matching);
    fs::write(remote.join("sub/extra.txt"), b"x").unwrap();
    assert_eq!(status(false).await, SubtreeStatus::Different);
    fs::remove_file(remote.join("sub/extra.txt")).unwrap();
    fs::create_dir(local.path().join("sub/local-only")).unwrap();
    assert_eq!(status(false).await, SubtreeStatus::Different);
}
