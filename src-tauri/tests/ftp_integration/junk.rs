//! The "hide local junk" setting in recursive FTP transfers.

use std::fs;
use std::sync::Arc;

use serverus_lib::session::remote_fs::RemoteFs;
use serverus_lib::transfer::{DownloadRequest, ProgressSink, TransferManager, UploadRequest};

use super::{
    assert_all_done, pool_for, settings, spawn_ftp, transfer_context, wait_for_drain, NullSink,
};

/// With the "hide local junk" setting, directory transfers leave
/// `.DS_Store` / `Thumbs.db` behind in both directions.
#[tokio::test]
async fn ftp_recursive_transfers_skip_junk_when_asked() {
    let server_root = tempfile::tempdir().unwrap();
    let port = spawn_ftp(server_root.path()).await;
    let pool: Arc<dyn RemoteFs> = pool_for(port);

    let src_root = tempfile::tempdir().unwrap();
    let tree = src_root.path().join("site");
    fs::create_dir_all(tree.join("sub")).unwrap();
    fs::write(tree.join("index.html"), b"<html>hi</html>").unwrap();
    fs::write(tree.join(".DS_Store"), b"junk").unwrap();
    fs::write(tree.join("sub/Thumbs.db"), b"junk").unwrap();

    let manager = Arc::new(TransferManager::default());
    let context_id = transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);

    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(
                pool.clone(),
                "ftp-1",
                tree.to_str().unwrap(),
                "/",
                settings(),
            )
            .skipping_junk(true),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    assert!(manager.clear_finished(context_id, "ftp-1"));
    assert!(pool.exists("/site/index.html").await.unwrap());
    assert!(!pool.exists("/site/.DS_Store").await.unwrap());
    assert!(!pool.exists("/site/sub/Thumbs.db").await.unwrap());

    // Junk already on the server stays there on download.
    fs::write(server_root.path().join("site/.DS_Store"), b"junk").unwrap();
    let dst_root = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            context_id,
            &sink,
            DownloadRequest::new(
                pool.clone(),
                "ftp-1",
                "/site",
                dst_root.path().to_str().unwrap(),
                settings(),
            )
            .skipping_junk(true),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    assert!(dst_root.path().join("site/index.html").is_file());
    assert!(dst_root.path().join("site/sub").is_dir());
    assert!(!dst_root.path().join("site/.DS_Store").exists());
}
