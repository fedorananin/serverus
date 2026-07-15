use std::sync::Arc;

pub(super) struct WatchedFile {
    pub(super) session_id: String,
    /// Platform alias: FSEvents on macOS, ReadDirectoryChanges on Windows,
    /// inotify on Linux — matches `notify::recommended_watcher`.
    pub(super) _watcher: notify::RecommendedWatcher,
    pub(super) shutdown: tokio::sync::watch::Sender<bool>,
    pub(super) completion: Arc<TaskCompletion>,
}

#[derive(Default)]
pub(super) struct TaskCompletion {
    done: std::sync::atomic::AtomicBool,
    notify: tokio::sync::Notify,
}

impl TaskCompletion {
    pub(super) async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            if self.done.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

pub(super) struct TaskCompletionGuard(pub(super) Arc<TaskCompletion>);

impl Drop for TaskCompletionGuard {
    fn drop(&mut self) {
        self.0
            .done
            .store(true, std::sync::atomic::Ordering::Release);
        self.0.notify.notify_waiters();
    }
}
