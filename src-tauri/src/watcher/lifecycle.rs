use std::path::{Path, PathBuf};

use super::types::WatchedFile;
use super::EditWatcher;

impl EditWatcher {
    /// Roll back one edit that completed after its runtime context retired.
    pub async fn close_path(&self, path: &Path) {
        let watched = self.files.lock().unwrap().remove(path);
        if let Some(watched_file) = watched {
            stop_watched_file(path, watched_file).await;
        }
    }

    /// Stop watching everything belonging to a closed session.
    pub async fn close_session(&self, session_id: &str) {
        self.admissions.close_session(session_id).await;
        let watched = self.take_session_files(session_id);
        stop_watched_files(watched).await;
    }

    /// Stop every active edit watcher and remove its isolated cache directory.
    pub async fn close_all(&self) {
        self.admissions.close_all().await;
        let watched = self.files.lock().unwrap().drain().collect::<Vec<_>>();
        stop_watched_files(watched).await;
        self.clear_notifications();
    }

    fn take_session_files(&self, session_id: &str) -> Vec<(PathBuf, WatchedFile)> {
        let mut files = self.files.lock().unwrap();
        let paths = files
            .iter()
            .filter(|(_, watched)| watched.session_id == session_id)
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        paths
            .into_iter()
            .filter_map(|path| files.remove(&path).map(|watched| (path, watched)))
            .collect()
    }
}

async fn stop_watched_files(watched: Vec<(PathBuf, WatchedFile)>) {
    for (_, watched_file) in &watched {
        let _ = watched_file.shutdown.send(true);
    }
    for (path, watched_file) in watched {
        finish_watched_file(&path, watched_file).await;
    }
}

async fn stop_watched_file(path: &Path, watched_file: WatchedFile) {
    let _ = watched_file.shutdown.send(true);
    finish_watched_file(path, watched_file).await;
}

async fn finish_watched_file(path: &Path, watched_file: WatchedFile) {
    watched_file.completion.wait().await;
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}
