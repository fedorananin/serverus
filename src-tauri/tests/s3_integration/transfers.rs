use std::sync::Arc;

use serverus_lib::session::remote_fs::{delete_recursive, RemoteFs};
use serverus_lib::transfer::{DownloadRequest, ProgressSink, TransferManager, UploadRequest};

use super::common::{assert_all_done, fs_for, settings, wait_for_drain, NullSink};
use super::server::spawn_s3;

pub(crate) async fn recursive_directory_transfers() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    std::fs::create_dir(root.path().join("files")).unwrap();
    let fs = fs_for(port, Some("files"));
    fs.probe().await.unwrap();

    let local = tempfile::tempdir().unwrap();
    let tree = local.path().join("site");
    std::fs::create_dir_all(tree.join("assets/img")).unwrap();
    std::fs::write(tree.join("index.html"), b"<html>hi</html>").unwrap();
    std::fs::write(tree.join("assets/app.js"), b"console.log(1)").unwrap();
    std::fs::write(tree.join("assets/img/logo.png"), vec![7_u8; 1024]).unwrap();

    let manager = Arc::new(TransferManager::default());
    let context_id = crate::transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(
                fs.clone(),
                "session",
                tree.to_str().unwrap(),
                "/",
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    let listing = fs.list("/site/assets").await.unwrap();
    let names: Vec<&str> = listing.iter().map(|entry| entry.name.as_str()).collect();
    assert!(names.contains(&"app.js"), "{names:?}");
    assert!(names.contains(&"img"), "{names:?}");

    let download = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            context_id,
            &sink,
            DownloadRequest::new(
                fs.clone(),
                "session",
                "/site",
                download.path().to_str().unwrap(),
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    assert_eq!(
        std::fs::read(download.path().join("site/index.html")).unwrap(),
        b"<html>hi</html>"
    );
    assert_eq!(
        std::fs::read(download.path().join("site/assets/img/logo.png")).unwrap(),
        vec![7_u8; 1024]
    );

    delete_recursive(fs.as_ref(), "/site", true).await.unwrap();
    assert!(!fs.exists("/site").await.unwrap());
}

pub(crate) async fn multipart_upload_roundtrip() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    std::fs::create_dir(root.path().join("big")).unwrap();
    let fs = fs_for(port, Some("big"));

    let payload: Vec<u8> = (0..20 * 1024 * 1024_u32)
        .map(|value| (value % 251) as u8)
        .collect();
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("blob.bin"), &payload).unwrap();

    let manager = Arc::new(TransferManager::default());
    let context_id = crate::transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(
                fs.clone(),
                "session",
                local.path().join("blob.bin").to_str().unwrap(),
                "/",
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    let entry = fs.stat("/blob.bin").await.unwrap();
    assert_eq!(entry.size, payload.len() as u64);

    let download = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            context_id,
            &sink,
            DownloadRequest::new(
                fs.clone(),
                "session",
                "/blob.bin",
                download.path().to_str().unwrap(),
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    assert_eq!(
        std::fs::read(download.path().join("blob.bin")).unwrap(),
        payload
    );
}
