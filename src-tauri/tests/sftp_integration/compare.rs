use std::fs;
use std::fs::File;
use std::time::{Duration, SystemTime};

use serverus_domain::fs_compare::CompareRules;
use serverus_lib::session::compare::{compare_subtree, CompareFilter, SubtreeStatus};
use serverus_lib::session::sftp::SftpFs;

use super::common::connect;
use crate::support::TestSshd;

const NOT_CANCELLED: &(dyn Fn() -> bool + Sync) = &|| false;

/// Pin a file's mtime so local and remote copies agree byte-for-byte on the
/// clock too, no matter how far apart the test created them.
fn pin_mtime(path: &std::path::Path) {
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    File::options()
        .append(true)
        .open(path)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
}

fn write_tree(root: &std::path::Path) {
    fs::create_dir_all(root.join("sub/inner")).unwrap();
    fs::write(root.join("top.txt"), b"top").unwrap();
    fs::write(root.join("sub/nested.txt"), b"nested").unwrap();
    fs::write(root.join("sub/inner/deep.txt"), b"deep").unwrap();
    for file in ["top.txt", "sub/nested.txt", "sub/inner/deep.txt"] {
        pin_mtime(&root.join(file));
    }
}

/// Deep comparison over a real sshd: matching trees, then a nested change,
/// a nested extra file, and a hidden file that only counts when shown.
pub(crate) async fn subtree_compare() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote = SftpFs::open(&session).await.unwrap();

    // Both trees live on this machine — the "remote" one is just served
    // through the sshd, which is exactly what the walker sees in production.
    let local = tempfile::tempdir().unwrap();
    write_tree(local.path());
    let remote = sshd.dir.path().join("compare-remote");
    write_tree(&remote);
    let remote_root = remote.to_string_lossy().into_owned();
    let local_root = local.path().to_string_lossy().into_owned();

    let status = |rules: CompareRules, hidden: bool| {
        let local_root = local_root.clone();
        let remote_root = remote_root.clone();
        let fs_remote = &fs_remote;
        async move {
            let filter = CompareFilter {
                include_hidden: hidden,
                ..CompareFilter::default()
            };
            compare_subtree(
                fs_remote,
                &local_root,
                &remote_root,
                rules,
                filter,
                NOT_CANCELLED,
            )
            .await
            .unwrap()
        }
    };

    let rules = CompareRules::default();
    assert_eq!(status(rules, false).await, SubtreeStatus::Matching);

    // A nested size change two levels down is found.
    fs::write(remote.join("sub/inner/deep.txt"), b"deep-changed").unwrap();
    pin_mtime(&remote.join("sub/inner/deep.txt"));
    assert_eq!(status(rules, false).await, SubtreeStatus::Different);

    // Same size, different (pinned vs fresh) mtime is a difference too —
    // unless the caller ignores mtime, as the S3 pane does.
    fs::write(remote.join("sub/inner/deep.txt"), b"deep").unwrap();
    assert_eq!(status(rules, false).await, SubtreeStatus::Different);
    let ignore_mtime = CompareRules {
        ignore_mtime: true,
        ..rules
    };
    assert_eq!(status(ignore_mtime, false).await, SubtreeStatus::Matching);
    pin_mtime(&remote.join("sub/inner/deep.txt"));
    assert_eq!(status(rules, false).await, SubtreeStatus::Matching);

    // A nested remote-only file is found; hidden files follow the pane filter.
    fs::write(remote.join("sub/.hidden"), b"x").unwrap();
    assert_eq!(status(rules, false).await, SubtreeStatus::Matching);
    assert_eq!(status(rules, true).await, SubtreeStatus::Different);
    fs::rename(remote.join("sub/.hidden"), remote.join("sub/extra.txt")).unwrap();
    assert_eq!(status(rules, false).await, SubtreeStatus::Different);
}
