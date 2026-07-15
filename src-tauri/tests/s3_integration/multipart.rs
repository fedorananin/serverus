use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::remote_fs::RemoteFs;

use super::common::{fs_for, MULTIPART_PART_SIZE};
use super::server::{spawn_s3_with_probe, MultipartProbe};

pub(crate) async fn dropping_writer_aborts_after_create_while_first_part_is_pending() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("uploads")).unwrap();
    let probe = Arc::new(MultipartProbe::default());
    probe.block_upload_part.store(true, Ordering::SeqCst);
    let port = spawn_s3_with_probe(root.path(), Some(probe.clone())).await;
    let fs = fs_for(port, Some("uploads"));

    let writer = fs.open_write("/cancelled.bin", 0).await.unwrap();
    let mut write_task = tokio::spawn(async move {
        let mut writer = writer;
        tokio::io::AsyncWriteExt::write_all(&mut writer, &vec![1; MULTIPART_PART_SIZE]).await?;
        tokio::io::AsyncWriteExt::flush(&mut writer).await
    });

    tokio::select! {
        _ = probe.upload_part_started.notified() => {}
        result = &mut write_task => {
            panic!("the write ended before UploadPart was pending: {result:?}")
        }
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!(
                "the first UploadPart request never arrived; create calls: {}",
                probe.create_calls.load(Ordering::SeqCst)
            );
        }
    }
    assert_eq!(probe.create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(probe.upload_part_calls.load(Ordering::SeqCst), 1);

    write_task.abort();
    assert!(write_task.await.unwrap_err().is_cancelled());

    tokio::time::timeout(Duration::from_secs(5), probe.abort_seen.notified())
        .await
        .expect("dropping the writer did not abort the multipart upload");
    assert_eq!(probe.abort_calls.load(Ordering::SeqCst), 1);
    assert_eq!(probe.complete_calls.load(Ordering::SeqCst), 0);
}

pub(crate) async fn completed_upload_is_not_aborted() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("uploads")).unwrap();
    let probe = Arc::new(MultipartProbe::default());
    let port = spawn_s3_with_probe(root.path(), Some(probe.clone())).await;
    let fs = fs_for(port, Some("uploads"));

    let mut writer = fs.open_write("/complete.bin", 0).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut writer, &vec![2; MULTIPART_PART_SIZE + 1])
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut writer)
        .await
        .unwrap();
    drop(writer);

    assert_eq!(probe.create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(probe.upload_part_calls.load(Ordering::SeqCst), 1);
    assert_eq!(probe.complete_calls.load(Ordering::SeqCst), 1);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), probe.abort_seen.notified())
            .await
            .is_err(),
        "a completed multipart upload was aborted"
    );
    assert_eq!(probe.abort_calls.load(Ordering::SeqCst), 0);
}

pub(crate) async fn small_writer_uses_put_object_without_multipart() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("uploads")).unwrap();
    let probe = Arc::new(MultipartProbe::default());
    let port = spawn_s3_with_probe(root.path(), Some(probe.clone())).await;
    let fs = fs_for(port, Some("uploads"));

    let mut writer = fs.open_write("/small.bin", 0).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut writer, b"small payload")
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut writer)
        .await
        .unwrap();
    drop(writer);

    assert_eq!(probe.put_object_calls.load(Ordering::SeqCst), 1);
    assert_eq!(probe.create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(probe.upload_part_calls.load(Ordering::SeqCst), 0);
    assert_eq!(probe.complete_calls.load(Ordering::SeqCst), 0);
    assert_eq!(probe.abort_calls.load(Ordering::SeqCst), 0);
}
