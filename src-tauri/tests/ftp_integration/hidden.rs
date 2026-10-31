//! Dot-files in FTP listings and recursive transfers. Stock vsftpd,
//! Pure-FTPd and ProFTPD hide them from a bare `LIST`; the pool asks with
//! `LIST -a`, which libunftp must take too (it strips the options).

use std::fs;
use std::sync::Arc;

use serverus_lib::session::remote_fs::RemoteFs;
use serverus_lib::transfer::{DownloadRequest, ProgressSink, TransferManager};

use super::{
    assert_all_done, pool_for, settings, spawn_ftp, transfer_context, wait_for_drain, NullSink,
};

#[tokio::test]
async fn ftp_dot_files_are_listed_and_downloaded() {
    let server_root = tempfile::tempdir().unwrap();
    let site = server_root.path().join("site");
    fs::create_dir_all(site.join(".well-known")).unwrap();
    fs::write(site.join(".htaccess"), b"deny from all").unwrap();
    fs::write(site.join(".well-known/security.txt"), b"contact").unwrap();
    fs::write(site.join("index.html"), b"<html></html>").unwrap();
    let port = spawn_ftp(server_root.path()).await;
    let pool: Arc<dyn RemoteFs> = pool_for(port);

    let mut names: Vec<String> = pool
        .list("/site")
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    assert_eq!(names, [".htaccess", ".well-known", "index.html"]);
    assert_eq!(pool.stat("/site/.htaccess").await.unwrap().size, 13);
    assert!(pool.list("/missing").await.is_err());

    let manager = Arc::new(TransferManager::default());
    let context_id = transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    let dst = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            context_id,
            &sink,
            DownloadRequest::new(
                pool.clone(),
                "ftp-1",
                "/site",
                dst.path().to_str().unwrap(),
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    assert!(dst.path().join("site/.htaccess").is_file());
    assert!(dst.path().join("site/.well-known/security.txt").is_file());
}
