use std::sync::Arc;
use std::time::Duration;

use serverus_domain::runtime_context::RuntimeContextId;

use super::super::cache::PendingCacheDir;
use super::super::EditWatcher;
use super::support::{RecordingFs, UploadFailure};
use crate::error::AppError;
use crate::session::remote_fs::RemoteFs;

#[tokio::test]
async fn close_session_cancels_a_blocked_open_and_releases_its_resources() {
    let root = tempfile::tempdir().unwrap();
    let cache_dir = root.path().join("blocked-open");
    let manager = Arc::new(EditWatcher::default());
    let context_id = RuntimeContextId::try_from(50_u128).unwrap();
    manager.activate_context(context_id);
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let registered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let filesystem: Arc<dyn RemoteFs> = Arc::new(RecordingFs::new(UploadFailure::None, false));
    let weak_filesystem = Arc::downgrade(&filesystem);

    let open_manager = manager.clone();
    let open_cache_dir = cache_dir.clone();
    let open_started = started.clone();
    let open_release = release.clone();
    let open_registered = registered.clone();
    let opening = tokio::spawn(async move {
        open_manager
            .admissions
            .run(context_id, "closing-session", || async move {
                std::fs::create_dir(&open_cache_dir).unwrap();
                let _pending_cache = PendingCacheDir::new(open_cache_dir.clone());
                std::fs::write(open_cache_dir.join("secret.txt"), b"plaintext").unwrap();
                let _filesystem = filesystem;
                open_started.notify_one();
                open_release.notified().await;
                open_registered.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .await
    });
    started.notified().await;

    tokio::time::timeout(
        Duration::from_secs(1),
        manager.close_session("closing-session"),
    )
    .await
    .expect("session cleanup cancels and awaits its blocked open");

    assert!(matches!(
        opening.await.expect("open task joined"),
        Err(AppError::SessionNotFound)
    ));
    release.notify_waiters();
    assert!(!registered.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!cache_dir.exists(), "partial plaintext cache leaked");
    assert!(
        weak_filesystem.upgrade().is_none(),
        "blocked open retained RemoteFs"
    );
    assert!(matches!(
        manager
            .admissions
            .run(context_id, "closing-session", || async { Ok(()) })
            .await,
        Err(AppError::SessionNotFound)
    ));
}

#[tokio::test]
async fn close_all_cancels_every_open_until_a_new_context_is_activated() {
    let manager = Arc::new(EditWatcher::default());
    let first_context = RuntimeContextId::try_from(51_u128).unwrap();
    manager.activate_context(first_context);

    let same_context_started = Arc::new(tokio::sync::Notify::new());
    let same_context_release = Arc::new(tokio::sync::Notify::new());
    let opening = tokio::spawn({
        let manager = manager.clone();
        let started = same_context_started.clone();
        let release = same_context_release.clone();
        async move {
            manager
                .admissions
                .run(first_context, "same-context", || async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(())
                })
                .await
        }
    });
    same_context_started.notified().await;
    manager.activate_context(first_context);
    same_context_release.notify_one();
    opening
        .await
        .expect("same-context open task joined")
        .expect("same-context activation keeps its admission valid");

    let root = tempfile::tempdir().unwrap();
    let release = Arc::new(tokio::sync::Notify::new());
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut openings = Vec::new();
    let mut cache_dirs = Vec::new();
    let mut weak_filesystems = Vec::new();
    for session_id in ["session-a", "session-b"] {
        let cache_dir = root.path().join(session_id);
        let filesystem: Arc<dyn RemoteFs> = Arc::new(RecordingFs::new(UploadFailure::None, false));
        weak_filesystems.push(Arc::downgrade(&filesystem));
        cache_dirs.push(cache_dir.clone());
        let manager = manager.clone();
        let release = release.clone();
        let started = started_tx.clone();
        openings.push(tokio::spawn(async move {
            manager
                .admissions
                .run(first_context, session_id, || async move {
                    std::fs::create_dir(&cache_dir).unwrap();
                    let _pending_cache = PendingCacheDir::new(cache_dir.clone());
                    std::fs::write(cache_dir.join("secret.txt"), b"plaintext").unwrap();
                    let _filesystem = filesystem;
                    started.send(()).unwrap();
                    release.notified().await;
                    Ok(())
                })
                .await
        }));
    }
    drop(started_tx);
    started_rx.recv().await.unwrap();
    started_rx.recv().await.unwrap();

    tokio::time::timeout(Duration::from_secs(1), manager.close_all())
        .await
        .expect("global cleanup cancels and awaits every blocked open");

    for opening in openings {
        assert!(matches!(
            opening.await.expect("open task joined"),
            Err(AppError::SessionNotFound)
        ));
    }
    release.notify_waiters();
    assert!(cache_dirs.iter().all(|path| !path.exists()));
    assert!(weak_filesystems
        .iter()
        .all(|filesystem| filesystem.upgrade().is_none()));
    assert!(matches!(
        manager
            .admissions
            .run(first_context, "new-session", || async { Ok(()) })
            .await,
        Err(AppError::WrongRuntimeContext)
    ));

    let new_context = RuntimeContextId::try_from(52_u128).unwrap();
    manager.activate_context(new_context);
    manager
        .admissions
        .run(new_context, "new-session", || async { Ok(()) })
        .await
        .expect("a new runtime context reopens remote-edit admission");
}

#[tokio::test]
async fn stale_context_is_rejected_before_remote_edit_side_effects() {
    let manager = EditWatcher::default();
    let old_context = RuntimeContextId::try_from(61_u128).unwrap();
    let new_context = RuntimeContextId::try_from(62_u128).unwrap();
    manager.activate_context(old_context);

    let root = tempfile::tempdir().unwrap();
    let cache_dir = root.path().join("stale-open");
    let operation_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = operation_started.clone();
    let filesystem: Arc<dyn RemoteFs> = Arc::new(RecordingFs::new(UploadFailure::None, false));
    let weak_filesystem = Arc::downgrade(&filesystem);
    let operation = move || async move {
        started.store(true, std::sync::atomic::Ordering::SeqCst);
        std::fs::create_dir(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("secret.txt"), b"plaintext").unwrap();
        let _filesystem = filesystem;
        Ok(())
    };

    manager.activate_context(new_context);
    let error = manager
        .admissions
        .run(old_context, "reused-session", operation)
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::WrongRuntimeContext));
    assert!(!operation_started.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!root.path().join("stale-open").exists());
    assert!(
        weak_filesystem.upgrade().is_none(),
        "stale open retained RemoteFs"
    );
}
