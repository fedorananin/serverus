use std::sync::atomic::Ordering;
use std::sync::Arc;

use futures::StreamExt;
use serverus_lib::session::remote_fs::{RemoteFs, TreeAction};
use serverus_lib::transfer::{ProgressSink, TransferKind, TransferManager, TreeRequest};
use tokio::io::AsyncWriteExt;

use super::common::{assert_all_done, fs_for, settings, wait_for_drain, NullSink};
use super::server::{spawn_s3_with_probe, MultipartProbe};

/// A recursive delete on S3 lists the subtree in one sweep and removes the
/// objects with `DeleteObjects`, at most 1000 keys per request — instead of
/// one `list` per prefix and one `DeleteObject` per key.
pub(crate) async fn recursive_delete_uses_bulk_requests() {
    let root = tempfile::tempdir().unwrap();
    let probe = Arc::new(MultipartProbe::default());
    let port = spawn_s3_with_probe(root.path(), Some(probe.clone())).await;
    std::fs::create_dir(root.path().join("files")).unwrap();
    let fs = fs_for(port, Some("files"));
    fs.probe().await.unwrap();

    // 1005 objects across 5 prefixes, plus one object that must survive.
    let paths: Vec<String> = (0..1005)
        .map(|n| format!("/site/part{}/object-{n}.txt", n % 5))
        .chain(["/keep.txt".to_string()])
        .collect();
    let mut uploads = futures::stream::iter(paths.iter().map(|path| {
        let fs = fs.clone();
        async move {
            let mut writer = fs.open_write(path, 0).await.unwrap();
            writer.write_all(b"x").await.unwrap();
            writer.shutdown().await.unwrap();
        }
    }))
    .buffer_unordered(16);
    while uploads.next().await.is_some() {}

    let manager = Arc::new(TransferManager::default());
    let context_id = crate::transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    let fs_remote: Arc<dyn RemoteFs> = fs.clone();
    manager
        .enqueue_tree_ops(
            context_id,
            &sink,
            vec![TreeRequest {
                fs: fs_remote,
                session_id: "session",
                path: "/site",
                is_dir: true,
                action: TreeAction::Delete,
                settings: settings(),
                shell: None,
            }],
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    let item = &manager.snapshot().items[0];
    assert_eq!(item.kind, TransferKind::Delete);
    // 1005 objects + the root prefix + 5 implied prefixes.
    assert_eq!((item.done, item.total), (1011, 1011));
    assert_eq!(probe.delete_objects_calls.load(Ordering::SeqCst), 2);
    assert!(!fs.exists("/site").await.unwrap());
    assert!(fs.exists("/keep.txt").await.unwrap());
}
