//! Integration tests for tar-stream accelerated directory transfers (M5,
//! SPEC §6.2) against a real sshd — the remote `tar` is the system binary.

mod support;
#[path = "support/transfer_context.rs"]
mod transfer_context;

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::remote_fs::{join_remote, RemoteFs};
use serverus_lib::session::sftp::SftpFs;
use serverus_lib::session::ssh::{connect_chain, ConnectOutcome, SshSession};
use serverus_lib::transfer::{
    DownloadRequest, ProgressSink, TransferManager, TransferState, UploadRequest,
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
        _ => panic!(),
    };
    match connect_chain(&[sshd.hop(Some(issue.key_line))])
        .await
        .unwrap()
    {
        ConnectOutcome::Connected(handle) => Arc::new(SshSession {
            handle: tokio::sync::Mutex::new(handle),
        }),
        _ => panic!(),
    }
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 4,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: true,
        tar_acceleration: true,
    }
}

async fn wait_for_drain(manager: &Arc<TransferManager>) {
    for _ in 0..600 {
        let summary = manager.snapshot().summary;
        if summary.queued == 0 && summary.running == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (items, summary) = {
        let s = manager.snapshot();
        (s.items, s.summary)
    };
    panic!("queue did not drain: {summary:?}\n{items:#?}");
}

#[tokio::test]
async fn tar_capability_probe() {
    let sshd = TestSshd::spawn();
    let ssh = connect(&sshd).await;
    assert!(ssh
        .exec_check("command -v tar >/dev/null 2>&1")
        .await
        .unwrap());
    assert!(!ssh
        .exec_check("command -v definitely-not-a-binary-xyz >/dev/null 2>&1")
        .await
        .unwrap());
}

/// Upload a tree through one tar stream, download it back the same way,
/// verify contents byte-for-byte, confirm the accelerated flag is set.
#[tokio::test]
async fn tar_roundtrip_many_small_files() {
    let sshd = TestSshd::spawn();
    let ssh = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&ssh).await.unwrap());

    // 120 small files across nested dirs — the SFTP-round-trip killer.
    let src_root = tempfile::tempdir().unwrap();
    let tree = src_root.path().join("many");
    for d in 0..6 {
        let dir = tree.join(format!("dir{d}"));
        fs::create_dir_all(&dir).unwrap();
        for f in 0..20 {
            fs::write(dir.join(format!("f{f}.txt")), format!("{d}-{f}")).unwrap();
        }
    }

    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let remote_base = join_remote(&scratch, "tar-dest");
    fs_remote.mkdir(&remote_base).await.unwrap();

    let manager = Arc::new(TransferManager::default());
    let context_id = transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);

    manager
        .enqueue_upload_accelerated(
            context_id,
            &sink,
            UploadRequest::new(
                fs_remote.clone(),
                "s",
                tree.to_str().unwrap(),
                &remote_base,
                settings(),
            ),
            Some(ssh.clone()),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    {
        let items = manager.snapshot().items;
        assert_eq!(items.len(), 1, "tar upload must be a single queue item");
        assert!(items[0].accelerated);
        assert!(
            matches!(items[0].state, TransferState::Done),
            "{:#?}",
            items[0]
        );
        assert!(manager.clear_finished(context_id, "s"));
    }

    // Spot-check on the remote side.
    let listing = fs_remote
        .list(&join_remote(&remote_base, "many/dir3"))
        .await
        .unwrap();
    assert_eq!(listing.len(), 20);

    // Accelerated download back.
    let dst_root = tempfile::tempdir().unwrap();
    manager
        .enqueue_download_accelerated(
            context_id,
            &sink,
            DownloadRequest::new(
                fs_remote.clone(),
                "s",
                &join_remote(&remote_base, "many"),
                dst_root.path().to_str().unwrap(),
                settings(),
            ),
            Some(ssh.clone()),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    {
        let items = manager.snapshot().items;
        assert_eq!(items.len(), 1);
        assert!(items[0].accelerated);
        assert!(
            matches!(items[0].state, TransferState::Done),
            "{:#?}",
            items[0]
        );
    }

    for d in 0..6 {
        for f in 0..20 {
            let rel = format!("dir{d}/f{f}.txt");
            let original = fs::read(tree.join(&rel)).unwrap();
            let copied = fs::read(dst_root.path().join("many").join(&rel)).unwrap();
            assert_eq!(original, copied, "mismatch at {rel}");
        }
    }
}

/// The Compare Folders feature marks a file "same" only when size and mtime
/// match exactly, so a tar-accelerated upload must land files with the very
/// second the local listing reports.
#[tokio::test]
async fn tar_upload_preserves_exact_mtime() {
    let sshd = TestSshd::spawn();
    let ssh = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&ssh).await.unwrap());

    let src_root = tempfile::tempdir().unwrap();
    let tree = src_root.path().join("stamped");
    fs::create_dir_all(&tree).unwrap();
    let file = tree.join("old.txt");
    fs::write(&file, b"payload").unwrap();
    // A timestamp firmly in the past so "extraction time" can't pass by luck.
    filetime::set_file_mtime(&file, filetime::FileTime::from_unix_time(1_600_000_000, 0)).unwrap();

    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let remote_base = join_remote(&scratch, "tar-mtime");
    fs_remote.mkdir(&remote_base).await.unwrap();

    let manager = Arc::new(TransferManager::default());
    let context_id = transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload_accelerated(
            context_id,
            &sink,
            UploadRequest::new(
                fs_remote.clone(),
                "s",
                tree.to_str().unwrap(),
                &remote_base,
                settings(),
            ),
            Some(ssh.clone()),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;

    let local_listing = serverus_lib::local_fs::list(tree.to_str().unwrap()).unwrap();
    let local_entry = local_listing.iter().find(|e| e.name == "old.txt").unwrap();
    let remote_listing = fs_remote
        .list(&join_remote(&remote_base, "stamped"))
        .await
        .unwrap();
    let remote_entry = remote_listing.iter().find(|e| e.name == "old.txt").unwrap();
    assert_eq!(local_entry.size, remote_entry.size);
    assert_eq!(
        local_entry.mtime, remote_entry.mtime,
        "tar upload must preserve the exact mtime the Compare feature checks"
    );
}
