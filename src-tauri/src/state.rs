//! Global application composition managed by Tauri.

mod operation;
mod s3_upload_acl;

#[cfg(all(test, feature = "scenario-tests"))]
mod quick_unlock_selection_tests;

use std::ops::Deref;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serverus_adapters::UuidRuntimeContextIdGenerator;
use serverus_application::context::{
    AppEventSink, ContextCleanup, ContextCleanupError, ContextEvent,
};
use serverus_domain::runtime_context::{RuntimeContextId, VaultKey};
use serverus_runtime::{ApplicationHandle as RuntimeApplicationHandle, RuntimeError};

use crate::app_config;
use crate::autolock::ActivityTracker;
use crate::error::{AppError, AppResult};
use crate::session::{SessionManager, SessionResourceCleanup};
use crate::transfer::TransferManager;
use crate::vault::quick_unlock::QuickUnlock;
use crate::vault::VaultManager;
use crate::watcher::EditWatcher;

/// The only application handle exposed from Tauri state.
///
/// Legacy managers stay behind this desktop composition facade while features
/// migrate to the runtime/application crates. Their crate-visible fields are a
/// transitional seam for commands that have not moved yet.
#[derive(Clone)]
pub struct DesktopApplication {
    runtime: RuntimeApplicationHandle,
    lifecycle: Arc<tokio::sync::Mutex<()>>,
    pub(crate) vault: Arc<Mutex<VaultManager>>,
    pub(crate) quick: Arc<dyn QuickUnlock>,
    pub(crate) sessions: Arc<SessionManager>,
    pub(crate) transfers: Arc<TransferManager>,
    pub(crate) edits: Arc<EditWatcher>,
    pub(crate) activity: Arc<ActivityTracker>,
}

impl DesktopApplication {
    pub(crate) fn activate_selected_vault(&self, vault_id: String) -> AppResult<RuntimeContextId> {
        let vault = VaultKey::new(vault_id)
            .map_err(|_| AppError::Other("selected vault path is empty".into()))?;
        let context_id = self
            .runtime
            .activate_vault_with(vault, |context_id| {
                // The coordinator becomes visible only after every child
                // producer epoch is ready for this exact generation.
                self.sessions.activate_context(context_id);
                self.transfers.activate_context(context_id);
                self.edits.activate_context(context_id);
            })
            .map_err(AppError::from)?;
        Ok(context_id)
    }

    pub(crate) async fn lock_lifecycle(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.lifecycle.clone().lock_owned().await
    }

    pub(crate) fn reidentify_selected_vault(
        &self,
        vault_id: String,
    ) -> AppResult<RuntimeContextId> {
        let vault = VaultKey::new(vault_id)
            .map_err(|_| AppError::Other("selected vault path is empty".into()))?;
        self.runtime.reidentify_vault(vault).map_err(Into::into)
    }

    /// Revoke secret access while preserving the active generation and its
    /// already-authenticated sessions.
    pub(crate) async fn lock_selected_vault(&self) -> AppResult<()> {
        let _lifecycle = self.lock_lifecycle().await;
        match self.runtime.lock_vault() {
            Ok(_) => self.vault.lock().unwrap().lock(),
            // The initial lock screen has no runtime context, but locking the
            // manager remains an idempotent and valid operation.
            Err(RuntimeError::NoActiveContext) => self.vault.lock().unwrap().lock(),
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
}

impl Deref for DesktopApplication {
    type Target = RuntimeApplicationHandle;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

struct DesktopContextCleanup {
    edits: Arc<EditWatcher>,
    transfers: Arc<TransferManager>,
    sessions: Arc<SessionManager>,
}

struct DesktopSessionResourceCleanup {
    edits: Arc<EditWatcher>,
    transfers: Arc<TransferManager>,
}

#[async_trait]
impl SessionResourceCleanup for DesktopSessionResourceCleanup {
    async fn clear_session(&self, session_id: &str) {
        self.edits.close_session(session_id).await;
        self.transfers.clear_session(session_id).await;
    }
}

#[async_trait]
impl ContextCleanup for DesktopContextCleanup {
    async fn retire(&self, context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
        // Ownership order matters: stop producers before dropping their
        // queues, then close the transports those workers used.
        self.sessions.close_context(context_id).await;
        self.edits.close_all().await;
        self.transfers.clear_all().await;
        for session_id in self.sessions.session_ids() {
            self.sessions.disconnect(&session_id).await;
        }
        Ok(())
    }
}

struct DesktopContextEvents;

impl AppEventSink for DesktopContextEvents {
    fn publish(&self, _event: ContextEvent) {
        // Runtime events become a desktop adapter when the corresponding IPC
        // consumers migrate; Tauri commands remain the current notification seam.
    }
}

pub struct AppState {
    pub application: DesktopApplication,
}

#[cfg(feature = "scenario-tests")]
fn desktop_quick_unlock() -> Arc<dyn QuickUnlock> {
    Arc::new(crate::vault::quick_unlock::NoQuickUnlock)
}

#[cfg(all(not(feature = "scenario-tests"), target_os = "macos"))]
fn desktop_quick_unlock() -> Arc<dyn QuickUnlock> {
    Arc::new(crate::vault::quick_unlock::MacQuickUnlock)
}

#[cfg(all(not(feature = "scenario-tests"), target_os = "windows"))]
fn desktop_quick_unlock() -> Arc<dyn QuickUnlock> {
    Arc::new(crate::vault::quick_unlock::WindowsQuickUnlock)
}

#[cfg(all(
    not(feature = "scenario-tests"),
    not(any(target_os = "macos", target_os = "windows"))
))]
fn desktop_quick_unlock() -> Arc<dyn QuickUnlock> {
    Arc::new(crate::vault::quick_unlock::NoQuickUnlock)
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::from_vault(
            VaultManager::new(app_config::vault_path()),
            desktop_quick_unlock(),
            Arc::new(ActivityTracker::default()),
        )
    }

    pub(crate) fn from_vault(
        vault: VaultManager,
        quick: Arc<dyn QuickUnlock>,
        activity: Arc<ActivityTracker>,
    ) -> Self {
        let vault = Arc::new(Mutex::new(vault));
        let transfers = Arc::new(TransferManager::default());
        let edits = Arc::new(EditWatcher::default());
        let sessions = Arc::new(SessionManager::with_resource_cleanup(Arc::new(
            DesktopSessionResourceCleanup {
                edits: edits.clone(),
                transfers: transfers.clone(),
            },
        )));
        let cleanup = Arc::new(DesktopContextCleanup {
            edits: edits.clone(),
            transfers: transfers.clone(),
            sessions: sessions.clone(),
        });
        let runtime = RuntimeApplicationHandle::new(
            Arc::new(UuidRuntimeContextIdGenerator),
            cleanup,
            Arc::new(DesktopContextEvents),
        );

        Self {
            application: DesktopApplication {
                runtime,
                lifecycle: Arc::new(tokio::sync::Mutex::new(())),
                vault,
                quick,
                sessions,
                transfers,
                edits,
                activity,
            },
        }
    }
}

impl Deref for AppState {
    type Target = DesktopApplication;

    fn deref(&self) -> &Self::Target {
        &self.application
    }
}
