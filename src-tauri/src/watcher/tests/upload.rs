use std::sync::Arc;
use std::time::Duration;

use super::super::upload::upload_back_controlled;
use super::support::{local_edit, RecordingFs, UploadFailure};
use crate::error::AppResult;
use crate::session::remote_fs::RemoteFs;

async fn upload_back(
    filesystem: Arc<dyn RemoteFs>,
    local_path: &std::path::Path,
    remote_path: &str,
) -> AppResult<()> {
    let (_shutdown_sender, mut shutdown) = tokio::sync::watch::channel(false);
    upload_back_controlled(filesystem, local_path, remote_path, &mut shutdown)
        .await
        .expect("uninterrupted test upload was cancelled")
}

#[tokio::test]
async fn remote_edit_stages_a_unique_sibling_before_replacing_original() {
    let local = local_edit();
    let filesystem = Arc::new(RecordingFs::new(UploadFailure::None, false));

    upload_back(filesystem.clone(), local.path(), "/dir/config.txt")
        .await
        .unwrap();

    let state = filesystem.state.lock().unwrap();
    assert_eq!(state.files.get("/dir/config.txt").unwrap(), b"new contents");
    assert_eq!(state.open_write_paths.len(), 1);
    let staging = &state.open_write_paths[0];
    assert!(staging.starts_with("/dir/.serverus-edit-"), "{staging}");
    assert_eq!(state.rename_calls.len(), 2);
    assert_eq!(state.rename_calls[0].0, "/dir/config.txt");
    let backup = &state.rename_calls[0].1;
    assert!(backup.starts_with("/dir/.serverus-replace-"), "{backup}");
    assert_eq!(
        state.rename_calls[1],
        (staging.clone(), "/dir/config.txt".into())
    );
    assert_eq!(state.delete_calls, vec![backup.clone()]);
    assert!(!state.files.contains_key(staging));
    assert!(!state.files.contains_key(backup));
}

#[tokio::test]
async fn upload_and_finalize_failures_preserve_original_and_clean_staging() {
    for failure in [UploadFailure::Write, UploadFailure::Finalize] {
        let local = local_edit();
        let filesystem = Arc::new(RecordingFs::new(failure, false));

        assert!(
            upload_back(filesystem.clone(), local.path(), "/dir/config.txt")
                .await
                .is_err()
        );

        let state = filesystem.state.lock().unwrap();
        assert_eq!(state.files.get("/dir/config.txt").unwrap(), b"old");
        let staging = &state.open_write_paths[0];
        assert_eq!(state.delete_calls, vec![staging.clone()]);
        assert!(!state.files.contains_key(staging));
    }
}

#[tokio::test]
async fn failed_promotion_rolls_back_original_and_cleans_staging() {
    let local = local_edit();
    let filesystem = Arc::new(RecordingFs::new(UploadFailure::None, true));

    assert!(
        upload_back(filesystem.clone(), local.path(), "/dir/config.txt")
            .await
            .is_err()
    );

    let state = filesystem.state.lock().unwrap();
    assert_eq!(state.files.get("/dir/config.txt").unwrap(), b"old");
    let staging = &state.open_write_paths[0];
    assert!(!state.files.contains_key(staging));
    assert_eq!(state.rename_calls.len(), 3);
    let backup = &state.rename_calls[0].1;
    assert_eq!(
        state.rename_calls[2],
        (backup.clone(), "/dir/config.txt".into())
    );
    assert!(!state.files.contains_key(backup));
}

#[tokio::test]
async fn cancellation_during_staging_preserves_original_and_cleans_staging() {
    let local = local_edit();
    let filesystem = Arc::new(RecordingFs::with_blocked_upload());
    let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let local_path = local.path().to_path_buf();
    let upload = tokio::spawn({
        let filesystem = filesystem.clone();
        async move {
            upload_back_controlled(filesystem, &local_path, "/dir/config.txt", &mut shutdown_rx)
                .await
        }
    });

    tokio::time::timeout(
        Duration::from_secs(1),
        filesystem.open_write_started.notified(),
    )
    .await
    .expect("staging upload never started");
    shutdown.send(true).unwrap();
    assert!(upload.await.unwrap().is_none());

    let state = filesystem.state.lock().unwrap();
    assert_eq!(state.files.get("/dir/config.txt").unwrap(), b"old");
    assert_eq!(state.files.len(), 1, "staging object leaked");
}
