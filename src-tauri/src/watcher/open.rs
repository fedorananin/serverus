use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use notify::{RecursiveMode, Watcher};
use serverus_domain::runtime_context::RuntimeContextId;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::io::AsyncWriteExt;

use super::cache::{
    create_private_cache_file, create_private_edit_dir, edit_cache_dir, validate_edit_filename,
    PendingCacheDir,
};
use super::editor::open_in_editor;
use super::types::{TaskCompletion, TaskCompletionGuard, WatchedFile};
use super::upload::upload_back_controlled;
use super::EditWatcher;
use crate::error::{AppError, AppResult};
use crate::events::RemoteEditUploadedEvent;
use crate::session::remote_fs::RemoteFs;
use crate::vault::model::EditorSettings;

impl EditWatcher {
    /// Open a fresh admission generation after the previous context cleanup
    /// reached quiescence. Re-activating the same live context is idempotent.
    pub fn activate_context(&self, context_id: RuntimeContextId) {
        self.admissions.activate_context(context_id);
    }

    /// Download `remote_path`, open it in the editor and auto-upload saves.
    pub async fn open(
        self: &Arc<Self>,
        expected_context_id: RuntimeContextId,
        app: AppHandle,
        fs_remote: Arc<dyn RemoteFs>,
        session_id: &str,
        remote_path: &str,
        editor: &EditorSettings,
    ) -> AppResult<PathBuf> {
        self.admissions
            .run(expected_context_id, session_id, || {
                self.open_admitted(app, fs_remote, session_id, remote_path, editor)
            })
            .await
    }

    async fn open_admitted(
        self: &Arc<Self>,
        app: AppHandle,
        fs_remote: Arc<dyn RemoteFs>,
        session_id: &str,
        remote_path: &str,
        editor: &EditorSettings,
    ) -> AppResult<PathBuf> {
        let name = remote_path.rsplit('/').next().unwrap_or("file").to_string();
        validate_edit_filename(&name)?;
        let dir = create_private_edit_dir(&edit_cache_dir())?;
        let mut pending_cache = PendingCacheDir::new(dir.clone());
        let local_path = dir.join(&name);

        download(fs_remote.as_ref(), remote_path, &local_path).await?;
        let initial_stamp = file_stamp(&local_path).await?;
        let (watcher, changes) = watch_changes(&local_path, &name)?;
        open_in_editor(&local_path, editor)?;

        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
        let completion = Arc::new(TaskCompletion::default());
        let upload_loop = watch_and_upload(
            app,
            fs_remote,
            local_path.clone(),
            remote_path.to_string(),
            name,
            changes,
            shutdown_rx,
            completion.clone(),
            initial_stamp,
            self.notifications.clone(),
        );

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
        pending_cache.keep();
        Ok(local_path)
    }
}

async fn download(
    fs_remote: &dyn RemoteFs,
    remote_path: &str,
    local_path: &std::path::Path,
) -> AppResult<()> {
    let mut reader = fs_remote.open_read(remote_path, 0).await?;
    let mut file = create_private_cache_file(local_path).await?;
    tokio::io::copy(&mut reader, &mut file)
        .await
        .map_err(|error| AppError::Transfer(format!("download for edit: {error}")))?;
    file.flush()
        .await
        .map_err(|error| AppError::Transfer(format!("download for edit: {error}")))?;
    Ok(())
}

fn watch_changes(
    local_path: &std::path::Path,
    name: &str,
) -> AppResult<(
    notify::RecommendedWatcher,
    tokio::sync::mpsc::UnboundedReceiver<()>,
)> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            if event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove() {
                let _ = sender.send(());
            }
        }
    })
    .map_err(|error| AppError::Other(format!("watcher: {error}")))?;
    let parent = local_path
        .parent()
        .ok_or_else(|| AppError::Other(format!("edit cache file has no parent: {name}")))?;
    watcher
        .watch(parent, RecursiveMode::NonRecursive)
        .map_err(|error| AppError::Other(format!("watch {name}: {error}")))?;
    Ok((watcher, receiver))
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

async fn file_stamp(path: &std::path::Path) -> AppResult<FileStamp> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| AppError::Other(format!("inspect edited file: {error}")))?;
    Ok(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

#[allow(clippy::too_many_arguments)]
async fn watch_and_upload(
    app: AppHandle,
    fs_remote: Arc<dyn RemoteFs>,
    local_path: PathBuf,
    remote_path: String,
    display_name: String,
    mut changes: tokio::sync::mpsc::UnboundedReceiver<()>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    completion: Arc<TaskCompletion>,
    mut uploaded_stamp: FileStamp,
    notifications: Arc<std::sync::Mutex<std::collections::VecDeque<RemoteEditUploadedEvent>>>,
) {
    let _completion = TaskCompletionGuard(completion);
    while await_change(&mut changes, &mut shutdown).await {
        if !await_quiet(&mut changes, &mut shutdown).await {
            return;
        }
        let Ok(changed_stamp) = file_stamp(&local_path).await else {
            continue;
        };
        if changed_stamp == uploaded_stamp {
            continue;
        }
        let Some(result) =
            upload_back_controlled(fs_remote.clone(), &local_path, &remote_path, &mut shutdown)
                .await
        else {
            break;
        };
        if *shutdown.borrow() {
            break;
        }
        uploaded_stamp = changed_stamp;
        let error = result.err().map(|error| error.to_string());
        let event = RemoteEditUploadedEvent {
            name: display_name.clone(),
            remote_path: remote_path.clone(),
            error,
        };
        super::notifications::record(&notifications, event.clone());
        let _ = event.emit(&app);
    }
}

async fn await_change(
    changes: &mut tokio::sync::mpsc::UnboundedReceiver<()>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        changed = changes.recv() => changed.is_some(),
        _ = tokio::time::sleep(Duration::from_millis(500)) => true,
        stopped = shutdown.changed() => {
            let _ = stopped;
            false
        }
    }
}

async fn await_quiet(
    changes: &mut tokio::sync::mpsc::UnboundedReceiver<()>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    loop {
        let keep_waiting = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(400)) => true,
            stopped = shutdown.changed() => {
                let _ = stopped;
                false
            }
        };
        if !keep_waiting {
            return false;
        }
        if changes.try_recv().is_err() {
            return true;
        }
        while changes.try_recv().is_ok() {}
    }
}
