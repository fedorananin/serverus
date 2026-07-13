//! Global app state managed by Tauri.

use std::sync::{Arc, Mutex};

use crate::app_config;
use crate::autolock::ActivityTracker;
use crate::session::SessionManager;
use crate::transfer::TransferManager;
use crate::vault::quick_unlock::QuickUnlock;
use crate::vault::VaultManager;
use crate::watcher::EditWatcher;

pub struct AppState {
    /// Std mutex: all vault work happens inside `spawn_blocking`, never
    /// held across an await point.
    pub vault: Arc<Mutex<VaultManager>>,
    pub quick: Arc<dyn QuickUnlock>,
    pub sessions: Arc<SessionManager>,
    pub transfers: Arc<TransferManager>,
    pub edits: Arc<EditWatcher>,
    pub activity: Arc<ActivityTracker>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        let quick: Arc<dyn QuickUnlock> = Arc::new(crate::vault::quick_unlock::MacQuickUnlock);
        #[cfg(not(target_os = "macos"))]
        let quick: Arc<dyn QuickUnlock> = Arc::new(crate::vault::quick_unlock::NoQuickUnlock);

        AppState {
            vault: Arc::new(Mutex::new(VaultManager::new(app_config::vault_path()))),
            quick,
            sessions: Arc::new(SessionManager::default()),
            transfers: Arc::new(TransferManager::default()),
            edits: Arc::new(EditWatcher::default()),
            activity: Arc::new(ActivityTracker::default()),
        }
    }
}
