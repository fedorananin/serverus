//! Recursive delete and recursive chmod as transfer-queue items, against a
//! real unprivileged sshd: the portable SFTP walk, the server-side `rm`
//! shortcut, per-entry failure reporting, and chmod scopes.

mod support;
#[path = "support/transfer_context.rs"]
mod transfer_context;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::remote_fs::{RemoteFs, TreeAction};
use serverus_lib::session::sftp::SftpFs;
use serverus_lib::session::ssh::{connect_chain, ConnectOutcome, SshSession};
use serverus_lib::transfer::{
    ProgressSink, TransferKind, TransferManager, TransferSnapshot, TransferState, TreeRequest,
};
use serverus_lib::vault::model::{ConflictPolicy, TransferSettings};
use support::TestSshd;

struct NullSink;
impl ProgressSink for NullSink {
    fn emit(&self, _event: serverus_lib::events::TransferProgressEvent) {}
}

async fn connect(sshd: &TestSshd) -> Arc<SshSession> {
    let issue = match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        _ => panic!("expected host key prompt"),
    };
    match connect_chain(&[sshd.hop(Some(issue.key_line))])
        .await
        .unwrap()
    {
        ConnectOutcome::Connected(handle) => Arc::new(SshSession {
            handle: tokio::sync::Mutex::new(handle),
        }),
        _ => panic!("expected connection"),
    }
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 2,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false,
        tar_acceleration: true,
    }
}

/// 4 folders × 25 files, an empty folder, and a symlink to a folder that
/// lives outside the tree. Returns the number of entries a delete handles.
fn build_tree(root: &Path, outside: &Path) -> u64 {
    for d in 0..4 {
        let dir = root.join(format!("dir{d}"));
        fs::create_dir_all(&dir).unwrap();
        for f in 0..25 {
            fs::write(dir.join(format!("f{f}.txt")), format!("{d}-{f}")).unwrap();
        }
    }
    fs::create_dir_all(root.join("dir0/empty")).unwrap();
    fs::create_dir_all(outside).unwrap();
    fs::write(outside.join("precious.txt"), b"keep me").unwrap();
    std::os::unix::fs::symlink(outside, root.join("link")).unwrap();
    // 100 files + 1 symlink + root + 4 folders + `empty`.
    100 + 1 + 1 + 4 + 1
}

async fn run(
    manager: &Arc<TransferManager>,
    fs_remote: &Arc<dyn RemoteFs>,
    path: &Path,
    action: TreeAction,
    shell: Option<Arc<SshSession>>,
) -> TransferSnapshot {
    let context_id = transfer_context::activate(manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    let path = path.to_string_lossy().into_owned();
    manager
        .enqueue_tree_ops(
            context_id,
            &sink,
            vec![TreeRequest {
                fs: fs_remote.clone(),
                session_id: "s",
                path: &path,
                is_dir: true,
                action,
                settings: settings(),
                shell,
            }],
        )
        .await
        .unwrap();
    for _ in 0..600 {
        let summary = manager.snapshot().summary;
        if summary.queued == 0 && summary.running == 0 {
            let mut items = manager.snapshot().items;
            assert_eq!(items.len(), 1, "{items:#?}");
            assert!(manager.clear_finished(context_id, "s"));
            return items.remove(0);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("queue did not drain: {:#?}", manager.snapshot().items);
}

#[tokio::test]
async fn delete_walks_the_tree_over_sftp_and_via_rm() {
    let sshd = TestSshd::spawn();
    let ssh = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&ssh).await.unwrap());
    let manager = Arc::new(TransferManager::default());

    for (label, shell) in [("sftp", None), ("rm", Some(ssh.clone()))] {
        let tree = sshd.dir.path().join(format!("tree-{label}"));
        let outside = sshd.dir.path().join(format!("outside-{label}"));
        let entries = build_tree(&tree, &outside);

        let item = run(
            &manager,
            &fs_remote,
            &tree,
            TreeAction::Delete,
            shell.clone(),
        )
        .await;
        assert_eq!(item.kind, TransferKind::Delete);
        assert!(
            matches!(item.state, TransferState::Done),
            "{label}: {item:#?}"
        );
        assert_eq!(item.accelerated, shell.is_some(), "{label}");
        assert!(!item.scanning);
        assert_eq!((item.done, item.total), (entries, entries), "{label}");
        assert!(!tree.exists(), "{label}: tree survived");
        // The symlink went, the directory it pointed at did not.
        assert_eq!(fs::read(outside.join("precious.txt")).unwrap(), b"keep me");
    }
}

#[tokio::test]
async fn a_locked_folder_fails_alone_and_is_reported() {
    let sshd = TestSshd::spawn();
    let ssh = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&ssh).await.unwrap());
    let manager = Arc::new(TransferManager::default());

    for (label, shell) in [("sftp", None), ("rm", Some(ssh.clone()))] {
        let tree = sshd.dir.path().join(format!("locked-{label}"));
        let outside = sshd.dir.path().join(format!("outside-{label}"));
        build_tree(&tree, &outside);
        // Entries inside dir2 cannot be unlinked while it is read-only.
        let locked = tree.join("dir2");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();

        let item = run(&manager, &fs_remote, &tree, TreeAction::Delete, shell).await;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            matches!(item.state, TransferState::Error),
            "{label}: {item:#?}"
        );
        let error = item.error.unwrap_or_default();
        assert!(error.contains("dir2"), "{label}: {error}");
        assert_eq!(item.done, item.total, "{label}: progress must finish");
        // Everything outside the locked folder is gone; the folder and its
        // ancestors remain, with all 25 files.
        assert_eq!(fs::read_dir(&locked).unwrap().count(), 25, "{label}");
        let left: Vec<_> = fs::read_dir(&tree)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec!["dir2"], "{label}");
    }
}

#[tokio::test]
async fn recursive_chmod_applies_its_scope() {
    let sshd = TestSshd::spawn();
    let ssh = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&ssh).await.unwrap());
    let manager = Arc::new(TransferManager::default());
    let tree = sshd.dir.path().join("www");
    let entries = build_tree(&tree, &sshd.dir.path().join("elsewhere"));
    let mode = |path: &Path| fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777;

    let dirs = TreeAction::Chmod {
        mode: 0o750,
        files: false,
        dirs: true,
    };
    let item = run(&manager, &fs_remote, &tree, dirs, Some(ssh.clone())).await;
    assert_eq!(item.kind, TransferKind::Chmod);
    assert!(matches!(item.state, TransferState::Done), "{item:#?}");
    // Chmod never takes the rm shortcut, and skips the symlink.
    assert!(!item.accelerated);
    assert_eq!(item.total, 6);
    assert_eq!(mode(&tree), 0o750);
    assert_eq!(mode(&tree.join("dir0/empty")), 0o750);
    assert_ne!(mode(&tree.join("dir1/f1.txt")), 0o750);

    let files = TreeAction::Chmod {
        mode: 0o640,
        files: true,
        dirs: false,
    };
    let item = run(&manager, &fs_remote, &tree, files, None).await;
    assert!(matches!(item.state, TransferState::Done), "{item:#?}");
    assert_eq!(item.total, entries - 6 - 1);
    assert_eq!(mode(&tree.join("dir3/f24.txt")), 0o640);
    assert_eq!(mode(&tree.join("dir3")), 0o750);
    assert_ne!(mode(&sshd.dir.path().join("elsewhere")), 0o640);
}
