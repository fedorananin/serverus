//! Integration tests for SFTP file operations and the transfer queue (M3)
//! against a real unprivileged sshd + sftp-server.

mod support;

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::remote_fs::{delete_recursive, join_remote, RemoteFs};
use serverus_lib::session::sftp::SftpFs;
use serverus_lib::session::ssh::{connect_chain, ConnectOutcome, SshSession};
use serverus_lib::transfer::{ProgressSink, TransferManager};
use serverus_lib::vault::model::{ConflictPolicy, TransferSettings};
use support::TestSshd;

struct NullSink;
impl ProgressSink for NullSink {
    fn emit(&self, _event: serverus_lib::events::TransferProgressEvent) {}
}

async fn connect(sshd: &TestSshd) -> SshSession {
    let issue = match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        _ => panic!("expected host key prompt"),
    };
    match connect_chain(&[sshd.hop(Some(issue.key_line))])
        .await
        .unwrap()
    {
        ConnectOutcome::Connected(handle) => SshSession {
            handle: tokio::sync::Mutex::new(handle),
        },
        _ => panic!("expected connection"),
    }
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 4,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: true,
        tar_acceleration: false,
    }
}

async fn wait_for_drain(manager: &Arc<TransferManager>) {
    for _ in 0..300 {
        let (_, summary) = manager.snapshot();
        if summary.queued == 0 && summary.running == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (items, summary) = manager.snapshot();
    panic!("queue did not drain: {summary:?}\n{items:#?}");
}

fn assert_all_done(manager: &Arc<TransferManager>) {
    let (items, _) = manager.snapshot();
    for item in &items {
        assert!(
            matches!(item.state, serverus_lib::transfer::TransferState::Done),
            "item not done: {item:#?}"
        );
    }
}

#[tokio::test]
async fn sftp_basic_operations() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote = SftpFs::open(&session).await.unwrap();

    let home = fs_remote.home_dir().await.unwrap();
    assert!(home.starts_with('/'));

    // Operate inside the fixture tempdir — always writable, auto-cleaned.
    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let base = join_remote(&scratch, "sftp-ops");
    fs_remote.mkdir(&base).await.unwrap();
    assert!(fs_remote.exists(&base).await.unwrap());

    // create + rename + stat + chmod
    let file_a = join_remote(&base, "a.txt");
    let file_b = join_remote(&base, "b.txt");
    fs_remote.create_file(&file_a).await.unwrap();
    fs_remote.rename(&file_a, &file_b).await.unwrap();
    let entry = fs_remote.stat(&file_b).await.unwrap();
    assert!(!entry.is_dir);

    fs_remote.chmod(&file_b, 0o640).await.unwrap();
    let entry = fs_remote.stat(&file_b).await.unwrap();
    assert_eq!(entry.permissions.unwrap() & 0o777, 0o640);

    // listing sees exactly one file
    let listing = fs_remote.list(&base).await.unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "b.txt");

    // recursive delete of a non-empty dir
    delete_recursive(&fs_remote, &base, true).await.unwrap();
    assert!(!fs_remote.exists(&base).await.unwrap());
}

/// Upload a nested local tree, download it back, byte-compare everything.
/// This is the transfer-queue counterpart of the FTP recursion test (M4).
#[tokio::test]
async fn recursive_roundtrip_through_queue() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&session).await.unwrap());

    // Local source tree: nested dirs, empty dir, empty file, binary file.
    let src_root = tempfile::tempdir().unwrap();
    let tree = src_root.path().join("tree");
    fs::create_dir_all(tree.join("sub/deeper")).unwrap();
    fs::create_dir_all(tree.join("empty-dir")).unwrap();
    fs::write(tree.join("root.txt"), b"root file").unwrap();
    fs::write(tree.join("sub/mid.txt"), vec![0u8; 100_000]).unwrap();
    fs::write(
        tree.join("sub/deeper/leaf.bin"),
        (0..=255u8).cycle().take(300_000).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(tree.join("sub/empty.txt"), b"").unwrap();

    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let remote_base = join_remote(&scratch, "queue-roundtrip");
    fs_remote.mkdir(&remote_base).await.unwrap();

    let manager = Arc::new(TransferManager::default());
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);

    // Upload the whole tree.
    manager
        .enqueue_upload(
            &sink,
            fs_remote.clone(),
            "session-1",
            tree.to_str().unwrap(),
            &remote_base,
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    manager.clear_finished();

    // Empty dir must exist remotely even with zero files inside.
    let remote_tree = join_remote(&remote_base, "tree");
    let listing = fs_remote.list(&remote_tree).await.unwrap();
    let names: Vec<_> = listing.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains(&"empty-dir".to_string()), "{names:?}");

    // Download back into a fresh dir.
    let dst_root = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            &sink,
            fs_remote.clone(),
            "session-1",
            &remote_tree,
            dst_root.path().to_str().unwrap(),
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    // Byte-compare all four files.
    for rel in [
        "root.txt",
        "sub/mid.txt",
        "sub/deeper/leaf.bin",
        "sub/empty.txt",
    ] {
        let original = fs::read(tree.join(rel)).unwrap();
        let copied = fs::read(dst_root.path().join("tree").join(rel)).unwrap();
        assert_eq!(original, copied, "content mismatch for {rel}");
    }
    assert!(dst_root.path().join("tree/empty-dir").is_dir());

    // mtime preserved (within a generous margin).
    let src_mtime = fs::metadata(tree.join("root.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let dst_mtime = fs::metadata(dst_root.path().join("tree/root.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let drift = src_mtime
        .duration_since(dst_mtime)
        .unwrap_or_else(|e| e.duration());
    assert!(drift < Duration::from_secs(2), "mtime drift {drift:?}");

    // Recursive remote cleanup.
    delete_recursive(fs_remote.as_ref(), &remote_base, true)
        .await
        .unwrap();
    assert!(!fs_remote.exists(&remote_base).await.unwrap());
}

/// Conflict policy: skip leaves the old content, overwrite replaces it.
#[tokio::test]
async fn conflict_policies() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&session).await.unwrap());

    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("data.txt");
    fs::write(&file, b"NEW").unwrap();

    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let base = join_remote(&scratch, "conflicts");
    fs_remote.mkdir(&base).await.unwrap();

    // Pre-existing remote file.
    let mut w = fs_remote
        .open_write(&join_remote(&base, "data.txt"), 0)
        .await
        .unwrap();
    use tokio::io::AsyncWriteExt;
    w.write_all(b"OLD").await.unwrap();
    w.shutdown().await.unwrap();

    let manager = Arc::new(TransferManager::default());
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);

    // Skip: remote keeps OLD.
    let mut s = settings();
    s.conflict_policy = ConflictPolicy::Skip;
    manager
        .enqueue_upload(
            &sink,
            fs_remote.clone(),
            "s",
            file.to_str().unwrap(),
            &base,
            s,
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    let mut r = fs_remote
        .open_read(&join_remote(&base, "data.txt"), 0)
        .await
        .unwrap();
    let mut buf = Vec::new();
    use tokio::io::AsyncReadExt;
    r.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, b"OLD");
    manager.clear_finished();

    // Overwrite: remote becomes NEW.
    let mut s = settings();
    s.conflict_policy = ConflictPolicy::Overwrite;
    manager
        .enqueue_upload(
            &sink,
            fs_remote.clone(),
            "s",
            file.to_str().unwrap(),
            &base,
            s,
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    let mut r = fs_remote
        .open_read(&join_remote(&base, "data.txt"), 0)
        .await
        .unwrap();
    let mut buf = Vec::new();
    r.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, b"NEW");

    // Rename: a "data (1).txt" appears.
    let mut s = settings();
    s.conflict_policy = ConflictPolicy::Rename;
    manager
        .enqueue_upload(
            &sink,
            fs_remote.clone(),
            "s",
            file.to_str().unwrap(),
            &base,
            s,
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert!(fs_remote
        .exists(&join_remote(&base, "data (1).txt"))
        .await
        .unwrap());

    delete_recursive(fs_remote.as_ref(), &base, true)
        .await
        .unwrap();
}
