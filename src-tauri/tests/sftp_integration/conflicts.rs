use std::fs;
use std::sync::Arc;

use std::time::Duration;

use serverus_lib::session::remote_fs::{delete_recursive, join_remote, RemoteFs};
use serverus_lib::session::sftp::SftpFs;
use serverus_lib::transfer::{
    ConflictAction, ProgressSink, TransferManager, TransferState, UploadRequest,
};
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
    assert!(manager.clear_finished(context_id, "s"));

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

async fn read_remote(fs_remote: &Arc<dyn RemoteFs>, path: &str) -> Vec<u8> {
    let mut reader = fs_remote.open_read(path, 0).await.unwrap();
    let mut contents = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut contents)
        .await
        .unwrap();
    contents
}

/// Regression: a multi-file selection is enqueued as ONE batch, so resolving
/// a conflict with "apply to all remaining conflicts" must cover every file
/// of the selection — not just the tree of the file that raised the dialog.
pub(crate) async fn apply_to_all_spans_selection() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote: Arc<dyn RemoteFs> = Arc::new(SftpFs::open(&session).await.unwrap());

    let source = tempfile::tempdir().unwrap();
    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let base = join_remote(&scratch, "conflicts-all");
    fs_remote.mkdir(&base).await.unwrap();

    let names = ["a.txt", "b.txt", "c.txt"];
    let mut requests_data = Vec::new();
    for name in names {
        let local = source.path().join(name);
        fs::write(&local, b"NEW").unwrap();
        let remote = join_remote(&base, name);
        let mut writer = fs_remote.open_write(&remote, 0).await.unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut writer, b"OLD")
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::shutdown(&mut writer)
            .await
            .unwrap();
        requests_data.push(local);
    }

    let manager = Arc::new(TransferManager::default());
    let context_id = crate::transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);

    let mut ask = settings();
    ask.conflict_policy = ConflictPolicy::Ask;
    let requests = requests_data
        .iter()
        .map(|local| {
            UploadRequest::new(
                fs_remote.clone(),
                "s",
                local.to_str().unwrap(),
                &base,
                ask.clone(),
            )
        })
        .collect();
    manager
        .enqueue_uploads_accelerated(context_id, &sink, requests, None)
        .await
        .unwrap();

    // Wait until at least one item raises the conflict dialog.
    let conflicted = 'wait: {
        for _ in 0..300 {
            let items = manager.snapshot().items;
            if let Some(item) = items.iter().find(|i| i.state == TransferState::Conflict) {
                break 'wait item.clone();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("no transfer reached the Conflict state");
    };

    // One resolution with apply_to_all must settle every file of the batch.
    manager.resolve_conflict("s", &conflicted.id, ConflictAction::Overwrite, true);
    wait_for_drain(&manager).await;

    let (items, summary) = {
        let s = manager.snapshot();
        (s.items, s.summary)
    };
    assert_eq!(summary.failed, 0, "unexpected failures: {items:#?}");
    assert!(
        items.iter().all(|i| i.state == TransferState::Done),
        "not all items finished: {items:#?}"
    );
    for name in names {
        assert_eq!(
            read_remote(&fs_remote, &join_remote(&base, name)).await,
            b"NEW",
            "{name} was not overwritten"
        );
    }

    delete_recursive(fs_remote.as_ref(), &base, true)
        .await
        .unwrap();
}
