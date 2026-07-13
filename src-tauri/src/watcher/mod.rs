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
use crate::session::remote_fs::RemoteFs;
use crate::vault::model::EditorSettings;

struct WatchedFile {
    session_id: String,
    _watcher: notify::FsEventWatcher,
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

        self.files.lock().unwrap().insert(
            local_path.clone(),
            WatchedFile {
                session_id: session_id.to_string(),
                _watcher: watcher,
            },
        );

        let remote_path = remote_path.to_string();
        let display_name = name.clone();
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                // Debounce: drain the burst, then wait for quiet.
                loop {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    if rx.try_recv().is_err() {
                        break;
                    }
                    while rx.try_recv().is_ok() {}
                }
                match upload_back(fs_remote.as_ref(), &local_path, &remote_path).await {
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
        });

        Ok(())
    }

    /// Stop watching everything belonging to a closed session.
    pub fn close_session(&self, session_id: &str) {
        let mut files = self.files.lock().unwrap();
        let paths: Vec<PathBuf> = files
            .iter()
            .filter(|(_, w)| w.session_id == session_id)
            .map(|(p, _)| p.clone())
            .collect();
        for path in paths {
            files.remove(&path);
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}

async fn upload_back(
    fs_remote: &dyn RemoteFs,
    local_path: &Path,
    remote_path: &str,
) -> AppResult<()> {
    let mut src = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    let mut dst = fs_remote.open_write(remote_path, 0).await?;
    tokio::io::copy(&mut src, &mut dst)
        .await
        .map_err(|e| AppError::Transfer(format!("auto-upload: {e}")))?;
    dst.shutdown()
        .await
        .map_err(|e| AppError::Transfer(format!("auto-upload finalize: {e}")))?;
    Ok(())
}

fn open_in_editor(path: &Path, editor: &EditorSettings) -> AppResult<()> {
    let mut cmd = std::process::Command::new("open");
    if !editor.use_system_default {
        if let Some(app) = editor.custom_app.as_deref().filter(|a| !a.is_empty()) {
            cmd.arg("-a").arg(app);
        }
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
