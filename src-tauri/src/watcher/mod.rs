//! Remote edit (SPEC §5.3): download a remote file into an isolated temp
//! dir, open it in the user's editor, watch it with FSEvents and upload it
//! back on every save.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::events::RemoteEditUploadedEvent;
use crate::session::remote_fs::{join_remote, parent_remote, RemoteFs};
use crate::vault::model::EditorSettings;

struct WatchedFile {
    session_id: String,
    /// Platform alias: FSEvents on macOS, ReadDirectoryChanges on Windows,
    /// inotify on Linux — matches what `notify::recommended_watcher` returns.
    _watcher: notify::RecommendedWatcher,
    shutdown: tokio::sync::watch::Sender<bool>,
    completion: Arc<TaskCompletion>,
}

#[derive(Default)]
struct TaskCompletion {
    done: std::sync::atomic::AtomicBool,
    notify: tokio::sync::Notify,
}

impl TaskCompletion {
    async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            if self.done.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

struct TaskCompletionGuard(Arc<TaskCompletion>);

impl Drop for TaskCompletionGuard {
    fn drop(&mut self) {
        self.0
            .done
            .store(true, std::sync::atomic::Ordering::Release);
        self.0.notify.notify_waiters();
    }
}

#[derive(Default)]
pub struct EditWatcher {
    /// local temp path → watch state.
    files: Mutex<HashMap<PathBuf, WatchedFile>>,
}

fn edit_cache_dir() -> PathBuf {
    std::env::temp_dir().join("serverus-edit")
}

/// Best-effort cleanup of downloaded copies (SPEC §5.3).
pub fn cleanup_all() {
    let _ = std::fs::remove_dir_all(edit_cache_dir());
}

impl EditWatcher {
    /// Download `remote_path`, open it in the editor and auto-upload saves.
    pub async fn open(
        self: &Arc<Self>,
        app: AppHandle,
        fs_remote: Arc<dyn RemoteFs>,
        session_id: &str,
        remote_path: &str,
        editor: &EditorSettings,
    ) -> AppResult<()> {
        let name = remote_path.rsplit('/').next().unwrap_or("file").to_string();
        // Isolated per-file dir avoids name collisions between servers.
        let dir = edit_cache_dir().join(uuid::Uuid::new_v4().to_string());
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AppError::Other(format!("edit cache: {e}")))?;
        let local_path = dir.join(&name);

        // Download.
        let mut reader = fs_remote.open_read(remote_path, 0).await?;
        let mut file = tokio::fs::File::create(&local_path)
            .await
            .map_err(|e| AppError::Other(e.to_string()))?;
        tokio::io::copy(&mut reader, &mut file)
            .await
            .map_err(|e| AppError::Transfer(format!("download for edit: {e}")))?;
        file.flush().await.ok();
        drop(file);

        // Open in the editor.
        open_in_editor(&local_path, editor)?;

        // Watch and re-upload on change, debounced — editors fire several
        // events per save.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let watch_target = local_path.clone();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    if event.kind.is_modify() || event.kind.is_create() {
                        let _ = tx.send(());
                    }
                }
            })
            .map_err(|e| AppError::Other(format!("watcher: {e}")))?;
        watcher
            .watch(&watch_target, RecursiveMode::NonRecursive)
            .map_err(|e| AppError::Other(format!("watch {name}: {e}")))?;

        let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let completion = Arc::new(TaskCompletion::default());
        let task_completion = completion.clone();
        let task_local_path = local_path.clone();
        let remote_path = remote_path.to_string();
        let display_name = name.clone();
        let upload_loop = async move {
            let _completion = TaskCompletionGuard(task_completion);
            loop {
                let changed = tokio::select! {
                    changed = rx.recv() => changed.is_some(),
                    shutdown = shutdown_rx.changed() => {
                        let _ = shutdown;
                        false
                    }
                };
                if !changed {
                    break;
                }
                // Debounce: drain the burst, then wait for quiet.
                loop {
                    let keep_waiting = tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(400)) => true,
                        shutdown = shutdown_rx.changed() => {
                            let _ = shutdown;
                            false
                        }
                    };
                    if !keep_waiting {
                        return;
                    }
                    if rx.try_recv().is_err() {
                        break;
                    }
                    while rx.try_recv().is_ok() {}
                }
                let Some(result) = upload_back_controlled(
                    fs_remote.clone(),
                    &task_local_path,
                    &remote_path,
                    &mut shutdown_rx,
                )
                .await
                else {
                    break;
                };
                if *shutdown_rx.borrow() {
                    break;
                }
                match result {
                    Ok(()) => {
                        let _ = RemoteEditUploadedEvent {
                            name: display_name.clone(),
                            remote_path: remote_path.clone(),
                            error: None,
                        }
                        .emit(&app);
                    }
                    Err(e) => {
                        let _ = RemoteEditUploadedEvent {
                            name: display_name.clone(),
                            remote_path: remote_path.clone(),
                            error: Some(e.to_string()),
                        }
                        .emit(&app);
                    }
                }
            }
        };

        // Register and spawn under the same lock used by close_session. The
        // task is therefore either visible to shutdown or does not exist yet.
        let mut files = self.files.lock().unwrap();
        files.insert(
            local_path.clone(),
            WatchedFile {
                session_id: session_id.to_string(),
                _watcher: watcher,
                shutdown,
                completion,
            },
        );
        drop(tokio::spawn(upload_loop));

        Ok(())
    }

    /// Stop watching everything belonging to a closed session.
    pub async fn close_session(&self, session_id: &str) {
        let watched = {
            let mut files = self.files.lock().unwrap();
            let paths: Vec<PathBuf> = files
                .iter()
                .filter(|(_, watched)| watched.session_id == session_id)
                .map(|(path, _)| path.clone())
                .collect();
            paths
                .into_iter()
                .filter_map(|path| files.remove(&path).map(|watched| (path, watched)))
                .collect::<Vec<_>>()
        };
        for (_, watched_file) in &watched {
            let _ = watched_file.shutdown.send(true);
        }
        for (path, watched_file) in watched {
            watched_file.completion.wait().await;
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}

const REMOTE_TEMP_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

async fn wait_for_shutdown(shutdown: &mut tokio::sync::watch::Receiver<bool>) {
    if !*shutdown.borrow() {
        let _ = shutdown.changed().await;
    }
}

async fn cleanup_remote_temp(fs_remote: &dyn RemoteFs, remote_temp: &str) {
    let _ = tokio::time::timeout(
        REMOTE_TEMP_CLEANUP_TIMEOUT,
        fs_remote.delete_file(remote_temp),
    )
    .await;
}

/// Upload an edit to a unique sibling and promote it only after the staging
/// upload has been finalized. Cancellation is observed before promotion; once
/// replacement starts it runs to completion so rollback cannot be interrupted.
async fn upload_back_controlled(
    fs_remote: Arc<dyn RemoteFs>,
    local_path: &Path,
    remote_path: &str,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Option<AppResult<()>> {
    let source = tokio::select! {
        biased;
        _ = wait_for_shutdown(shutdown) => return None,
        source = tokio::fs::File::open(local_path) => source,
    };
    let mut src = match source {
        Ok(source) => source,
        Err(error) => return Some(Err(AppError::Other(error.to_string()))),
    };

    let remote_temp = join_remote(
        &parent_remote(remote_path),
        &format!(".serverus-edit-{}.tmp", uuid::Uuid::new_v4()),
    );
    let staging_result = tokio::select! {
        biased;
        _ = wait_for_shutdown(shutdown) => None,
        result = async {
            let mut dst = fs_remote
                .open_write_replacement(&remote_temp, remote_path)
                .await?;
            tokio::io::copy(&mut src, &mut dst)
                .await
                .map_err(|e| AppError::Transfer(format!("auto-upload: {e}")))?;
            dst.shutdown()
                .await
                .map_err(|e| AppError::Transfer(format!("auto-upload finalize: {e}")))?;
            drop(dst);
            Ok(())
        } => Some(result),
    };

    match staging_result {
        None => {
            cleanup_remote_temp(fs_remote.as_ref(), &remote_temp).await;
            return None;
        }
        Some(Err(error)) => {
            cleanup_remote_temp(fs_remote.as_ref(), &remote_temp).await;
            return Some(Err(error));
        }
        Some(Ok(())) => {}
    }

    tokio::select! {
        biased;
        _ = wait_for_shutdown(shutdown) => {
            cleanup_remote_temp(fs_remote.as_ref(), &remote_temp).await;
            return None;
        }
        _ = std::future::ready(()) => {}
    }

    let result = fs_remote.replace_file(&remote_temp, remote_path).await;
    if result.is_err() {
        cleanup_remote_temp(fs_remote.as_ref(), &remote_temp).await;
    }
    Some(result)
}

#[cfg(test)]
async fn upload_back(
    fs_remote: Arc<dyn RemoteFs>,
    local_path: &Path,
    remote_path: &str,
) -> AppResult<()> {
    let (shutdown_sender, mut shutdown) = tokio::sync::watch::channel(false);
    let result = upload_back_controlled(fs_remote, local_path, remote_path, &mut shutdown)
        .await
        .expect("uninterrupted test upload was cancelled");
    drop(shutdown_sender);
    result
}

fn open_in_editor(path: &Path, editor: &EditorSettings) -> AppResult<()> {
    let custom = if editor.use_system_default {
        None
    } else {
        editor.custom_app.as_deref().filter(|a| !a.is_empty())
    };

    #[cfg(target_os = "macos")]
    {
        // `open` delegates to LaunchServices and returns immediately.
        let mut cmd = std::process::Command::new("open");
        if let Some(app) = custom {
            cmd.arg("-a").arg(app);
        }
        cmd.arg(path);
        let status = cmd
            .status()
            .map_err(|e| AppError::Other(format!("open editor: {e}")))?;
        if !status.success() {
            return Err(AppError::Other("editor failed to open the file".into()));
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut cmd = match custom {
            // A specific editor (name on PATH or a full path) is launched
            // directly — spawn, don't wait: it may stay open for hours.
            Some(app) => {
                let mut c = std::process::Command::new(app);
                c.arg(path);
                c
            }
            None => {
                #[cfg(target_os = "windows")]
                let mut c = std::process::Command::new("explorer");
                #[cfg(not(target_os = "windows"))]
                let mut c = std::process::Command::new("xdg-open");
                c.arg(path);
                c
            }
        };
        cmd.spawn()
            .map_err(|e| AppError::Other(format!("open editor: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};
    use std::time::Duration;

    use tokio::io::AsyncWrite;

    use super::{upload_back, upload_back_controlled};
    use crate::error::{AppError, AppResult};
    use crate::session::remote_fs::{BoxRead, BoxWrite, RemoteEntry, RemoteFs};

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum UploadFailure {
        None,
        Write,
        Finalize,
    }

    #[derive(Default)]
    struct FsState {
        files: HashMap<String, Vec<u8>>,
        open_write_paths: Vec<String>,
        rename_calls: Vec<(String, String)>,
        delete_calls: Vec<String>,
    }

    struct RecordingFs {
        state: Arc<Mutex<FsState>>,
        upload_failure: UploadFailure,
        fail_promote: bool,
        block_open_write: bool,
        open_write_started: Arc<tokio::sync::Notify>,
    }

    impl RecordingFs {
        fn new(upload_failure: UploadFailure, fail_promote: bool) -> Self {
            let mut state = FsState::default();
            state
                .files
                .insert("/dir/config.txt".into(), b"old".to_vec());
            Self {
                state: Arc::new(Mutex::new(state)),
                upload_failure,
                fail_promote,
                block_open_write: false,
                open_write_started: Arc::new(tokio::sync::Notify::new()),
            }
        }

        fn with_blocked_upload() -> Self {
            Self {
                block_open_write: true,
                ..Self::new(UploadFailure::None, false)
            }
        }
    }

    struct RecordingWriter {
        state: Arc<Mutex<FsState>>,
        path: String,
        failure: UploadFailure,
    }

    impl AsyncWrite for RecordingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.failure == UploadFailure::Write {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "simulated interrupted upload",
                )));
            }
            self.state
                .lock()
                .unwrap()
                .files
                .entry(self.path.clone())
                .or_default()
                .extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            if self.failure == UploadFailure::Finalize {
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "simulated finalize failure",
                )))
            } else {
                Poll::Ready(Ok(()))
            }
        }
    }

    #[async_trait::async_trait]
    impl RemoteFs for RecordingFs {
        async fn list(&self, _path: &str) -> AppResult<Vec<RemoteEntry>> {
            Ok(Vec::new())
        }

        async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
            Err(AppError::RemoteFs(format!("{path}: not supported")))
        }

        async fn home_dir(&self) -> AppResult<String> {
            Ok("/".into())
        }

        async fn mkdir(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }

        async fn create_file(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }

        async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
            let mut state = self.state.lock().unwrap();
            state.rename_calls.push((from.into(), to.into()));
            if self.fail_promote && from.contains(".serverus-edit-") {
                return Err(AppError::RemoteFs("simulated rename failure".into()));
            }
            if state.files.contains_key(to) {
                return Err(AppError::RemoteFs(format!("{to}: target already exists")));
            }
            let contents = state
                .files
                .remove(from)
                .ok_or_else(|| AppError::RemoteFs(format!("{from}: not found")))?;
            state.files.insert(to.into(), contents);
            Ok(())
        }

        async fn delete_file(&self, path: &str) -> AppResult<()> {
            let mut state = self.state.lock().unwrap();
            state.delete_calls.push(path.into());
            state.files.remove(path);
            Ok(())
        }

        async fn delete_dir(&self, _path: &str) -> AppResult<()> {
            Ok(())
        }

        async fn chmod(&self, _path: &str, _mode: u32) -> AppResult<()> {
            Ok(())
        }

        async fn set_mtime(&self, _path: &str, _mtime_unix: i64) -> AppResult<()> {
            Ok(())
        }

        async fn open_read(&self, path: &str, _offset: u64) -> AppResult<BoxRead> {
            Err(AppError::RemoteFs(format!("{path}: not supported")))
        }

        async fn open_write(&self, path: &str, _offset: u64) -> AppResult<BoxWrite> {
            let path = path.to_string();
            {
                let mut state = self.state.lock().unwrap();
                state.open_write_paths.push(path.clone());
                state.files.insert(path.clone(), Vec::new());
            }
            if self.block_open_write {
                self.open_write_started.notify_one();
                std::future::pending::<()>().await;
            }
            Ok(Box::new(RecordingWriter {
                state: self.state.clone(),
                path,
                failure: self.upload_failure,
            }))
        }

        async fn exists(&self, path: &str) -> AppResult<bool> {
            Ok(self.state.lock().unwrap().files.contains_key(path))
        }
    }

    fn local_edit() -> tempfile::NamedTempFile {
        let local = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(local.path(), b"new contents").unwrap();
        local
    }

    #[tokio::test]
    async fn remote_edit_stages_a_unique_sibling_before_replacing_original() {
        let local = local_edit();
        let fs = Arc::new(RecordingFs::new(UploadFailure::None, false));

        upload_back(fs.clone(), local.path(), "/dir/config.txt")
            .await
            .unwrap();

        let state = fs.state.lock().unwrap();
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
            let fs = Arc::new(RecordingFs::new(failure, false));

            assert!(upload_back(fs.clone(), local.path(), "/dir/config.txt")
                .await
                .is_err());

            let state = fs.state.lock().unwrap();
            assert_eq!(state.files.get("/dir/config.txt").unwrap(), b"old");
            let staging = &state.open_write_paths[0];
            assert_eq!(state.delete_calls, vec![staging.clone()]);
            assert!(!state.files.contains_key(staging));
        }
    }

    #[tokio::test]
    async fn failed_promotion_rolls_back_original_and_cleans_staging() {
        let local = local_edit();
        let fs = Arc::new(RecordingFs::new(UploadFailure::None, true));

        assert!(upload_back(fs.clone(), local.path(), "/dir/config.txt")
            .await
            .is_err());

        let state = fs.state.lock().unwrap();
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
        let fs = Arc::new(RecordingFs::with_blocked_upload());
        let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let local_path = local.path().to_path_buf();
        let upload = tokio::spawn({
            let fs = fs.clone();
            async move {
                upload_back_controlled(fs, &local_path, "/dir/config.txt", &mut shutdown_rx).await
            }
        });

        tokio::time::timeout(Duration::from_secs(1), fs.open_write_started.notified())
            .await
            .expect("staging upload never started");
        shutdown.send(true).unwrap();
        assert!(upload.await.unwrap().is_none());

        let state = fs.state.lock().unwrap();
        assert_eq!(state.files.get("/dir/config.txt").unwrap(), b"old");
        assert_eq!(state.files.len(), 1, "staging object leaked");
    }
}
