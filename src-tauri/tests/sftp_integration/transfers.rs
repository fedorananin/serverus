use std::fs;
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::remote_fs::{delete_recursive, join_remote, RemoteFs};
use serverus_lib::session::sftp::SftpFs;
use serverus_lib::transfer::{DownloadRequest, ProgressSink, TransferManager, UploadRequest};

use super::common::{assert_all_done, connect, settings, wait_for_drain, NullSink};
use crate::support::TestSshd;

pub(crate) async fn recursive_roundtrip_through_queue() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&session).await.unwrap());

    let source_root = tempfile::tempdir().unwrap();
    let tree = source_root.path().join("tree");
    fs::create_dir_all(tree.join("sub/deeper")).unwrap();
    fs::create_dir_all(tree.join("empty-dir")).unwrap();
    fs::write(tree.join("root.txt"), b"root file").unwrap();
    fs::write(tree.join("sub/mid.txt"), vec![0_u8; 100_000]).unwrap();
    fs::write(
        tree.join("sub/deeper/leaf.bin"),
        (0..=255_u8).cycle().take(300_000).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(tree.join("sub/empty.txt"), b"").unwrap();

    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let remote_base = join_remote(&scratch, "queue-roundtrip");
    fs_remote.mkdir(&remote_base).await.unwrap();

    let manager = Arc::new(TransferManager::default());
    let context_id = crate::transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(
                fs_remote.clone(),
                "session-1",
                tree.to_str().unwrap(),
                &remote_base,
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    assert!(manager.clear_finished(context_id));

    let remote_tree = join_remote(&remote_base, "tree");
    let listing = fs_remote.list(&remote_tree).await.unwrap();
    let names: Vec<_> = listing.iter().map(|entry| entry.name.clone()).collect();
    assert!(names.contains(&"empty-dir".to_string()), "{names:?}");

    let destination_root = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            context_id,
            &sink,
            DownloadRequest::new(
                fs_remote.clone(),
                "session-1",
                &remote_tree,
                destination_root.path().to_str().unwrap(),
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    for relative_path in [
        "root.txt",
        "sub/mid.txt",
        "sub/deeper/leaf.bin",
        "sub/empty.txt",
    ] {
        let original = fs::read(tree.join(relative_path)).unwrap();
        let copied = fs::read(destination_root.path().join("tree").join(relative_path)).unwrap();
        assert_eq!(original, copied, "content mismatch for {relative_path}");
    }
    assert!(destination_root.path().join("tree/empty-dir").is_dir());

    let source_mtime = fs::metadata(tree.join("root.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let destination_mtime = fs::metadata(destination_root.path().join("tree/root.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let drift = source_mtime
        .duration_since(destination_mtime)
        .unwrap_or_else(|error| error.duration());
    assert!(drift < Duration::from_secs(2), "mtime drift {drift:?}");

    delete_recursive(fs_remote.as_ref(), &remote_base, true)
        .await
        .unwrap();
    assert!(!fs_remote.exists(&remote_base).await.unwrap());
}
