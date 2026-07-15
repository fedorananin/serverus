use std::fs;
use std::sync::Arc;

use serverus_lib::session::remote_fs::{delete_recursive, join_remote, RemoteFs};
use serverus_lib::session::sftp::SftpFs;
use serverus_lib::transfer::{ProgressSink, TransferManager, UploadRequest};
use serverus_lib::vault::model::ConflictPolicy;

use super::common::{connect, settings, wait_for_drain, NullSink};
use crate::support::TestSshd;

pub(crate) async fn conflict_policies() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&session).await.unwrap());

    let source = tempfile::tempdir().unwrap();
    let file = source.path().join("data.txt");
    fs::write(&file, b"NEW").unwrap();

    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let base = join_remote(&scratch, "conflicts");
    fs_remote.mkdir(&base).await.unwrap();

    let remote_file = join_remote(&base, "data.txt");
    let mut writer = fs_remote.open_write(&remote_file, 0).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut writer, b"OLD")
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut writer)
        .await
        .unwrap();

    let manager = Arc::new(TransferManager::default());
    let context_id = crate::transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);

    let mut skip = settings();
    skip.conflict_policy = ConflictPolicy::Skip;
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(fs_remote.clone(), "s", file.to_str().unwrap(), &base, skip),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    let mut reader = fs_remote.open_read(&remote_file, 0).await.unwrap();
    let mut contents = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut contents)
        .await
        .unwrap();
    assert_eq!(contents, b"OLD");
    assert!(manager.clear_finished(context_id));

    let mut overwrite = settings();
    overwrite.conflict_policy = ConflictPolicy::Overwrite;
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(
                fs_remote.clone(),
                "s",
                file.to_str().unwrap(),
                &base,
                overwrite,
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    let mut reader = fs_remote.open_read(&remote_file, 0).await.unwrap();
    let mut contents = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut contents)
        .await
        .unwrap();
    assert_eq!(contents, b"NEW");

    let mut rename = settings();
    rename.conflict_policy = ConflictPolicy::Rename;
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(
                fs_remote.clone(),
                "s",
                file.to_str().unwrap(),
                &base,
                rename,
            ),
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
