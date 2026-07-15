use std::sync::Arc;

use super::super::types::{TaskCompletion, TaskCompletionGuard, WatchedFile};
use super::super::EditWatcher;

#[tokio::test]
async fn close_path_rolls_back_one_late_registered_remote_edit() {
    let root = tempfile::tempdir().unwrap();
    let cache_dir = root.path().join("late-edit");
    std::fs::create_dir(&cache_dir).unwrap();
    let local_path = cache_dir.join("config.txt");
    std::fs::write(&local_path, b"plaintext").unwrap();

    let manager = EditWatcher::default();
    let watcher = notify::recommended_watcher(|_result: notify::Result<notify::Event>| {}).unwrap();
    let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let completion = Arc::new(TaskCompletion::default());
    let task_completion = completion.clone();
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let task_stopped = stopped.clone();
    tokio::spawn(async move {
        let _completion = TaskCompletionGuard(task_completion);
        shutdown_rx.changed().await.unwrap();
        task_stopped.store(*shutdown_rx.borrow(), std::sync::atomic::Ordering::SeqCst);
    });

    manager.files.lock().unwrap().insert(
        local_path.clone(),
        WatchedFile {
            session_id: "session-old-context".into(),
            _watcher: watcher,
            shutdown,
            completion,
        },
    );

    manager.close_path(&local_path).await;

    assert!(manager.files.lock().unwrap().is_empty());
    assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!cache_dir.exists());
}

#[tokio::test]
async fn close_all_stops_every_session_and_removes_cached_files() {
    let root = tempfile::tempdir().unwrap();
    let manager = EditWatcher::default();
    let stopped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut cache_dirs = Vec::new();

    for (session_id, file_name) in [("session-a", "first.txt"), ("session-b", "second.txt")] {
        let cache_dir = root.path().join(session_id);
        std::fs::create_dir(&cache_dir).unwrap();
        let local_path = cache_dir.join(file_name);
        std::fs::write(&local_path, b"plaintext").unwrap();

        let watcher =
            notify::recommended_watcher(|_result: notify::Result<notify::Event>| {}).unwrap();
        let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let completion = Arc::new(TaskCompletion::default());
        let task_completion = completion.clone();
        let task_stopped = stopped.clone();
        tokio::spawn(async move {
            let _completion = TaskCompletionGuard(task_completion);
            shutdown_rx.changed().await.unwrap();
            if *shutdown_rx.borrow() {
                task_stopped.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });

        manager.files.lock().unwrap().insert(
            local_path,
            WatchedFile {
                session_id: session_id.into(),
                _watcher: watcher,
                shutdown,
                completion,
            },
        );
        cache_dirs.push(cache_dir);
    }

    manager.close_all().await;

    assert!(manager.files.lock().unwrap().is_empty());
    assert_eq!(stopped.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert!(cache_dirs.into_iter().all(|dir| !dir.exists()));
}
