//! Queueing uploads, downloads and remote tree operations for a session —
//! shared by the file-panel commands and the AI agent.

use std::sync::Arc;

use super::DesktopApplication;
use crate::error::{AppError, AppResult};
use crate::session::remote_fs::TreeAction;
use crate::transfer::{DownloadRequest, Enqueued, ProgressSink, TreeRequest, UploadRequest};
use crate::vault::model::{ConflictPolicy, TransferSettings};

/// One remote entry for a recursive delete / chmod. `is_dir` means a real
/// directory: a symlink to one is removed as a link, never descended.
pub(crate) struct TreeTargetSpec {
    pub path: String,
    pub is_dir: bool,
}

impl DesktopApplication {
    /// The vault's transfer settings, with the conflict policy optionally
    /// overridden (the agent decides up front instead of prompting).
    pub(crate) fn transfer_settings(&self, conflict: Option<ConflictPolicy>) -> TransferSettings {
        let mut settings = self
            .vault
            .lock()
            .unwrap()
            .payload()
            .map(|p| p.settings.transfers.clone())
            .unwrap_or_default();
        if let Some(policy) = conflict {
            settings.conflict_policy = policy;
        }
        settings
    }

    /// The "hide local junk" panel setting: directory transfers leave
    /// `.DS_Store` / `Thumbs.db` behind in both directions.
    fn skip_junk(&self) -> bool {
        self.vault
            .lock()
            .unwrap()
            .payload()
            .map(|p| p.settings.panels.hide_local_junk)
            .unwrap_or(true)
    }

    // Each returns what the enqueue did (`Enqueued::result` carries per-path
    // failures), or an error when nothing could be attempted at all.

    /// Queue local paths for upload into `remote_dir` as one batch.
    pub(crate) async fn enqueue_uploads(
        &self,
        sink: Arc<dyn ProgressSink>,
        session_id: &str,
        local_paths: &[String],
        remote_dir: &str,
        conflict: Option<ConflictPolicy>,
    ) -> AppResult<Enqueued> {
        let lease = self.require_active().map_err(AppError::from)?;
        let entry = self.sessions.get(session_id)?;
        let fs = entry.remote_fs().await?;
        let settings = self.transfer_settings(conflict);
        let skip_junk = self.skip_junk();
        let tar_ssh = entry.tar_ssh().await;
        let requests = local_paths
            .iter()
            .map(|local_path| {
                UploadRequest::new(
                    fs.clone(),
                    session_id,
                    local_path,
                    remote_dir,
                    settings.clone(),
                )
                .skipping_junk(skip_junk)
            })
            .collect();
        Ok(self
            .transfers
            .enqueue_uploads_tracked(lease.context_id(), &sink, requests, tar_ssh)
            .await)
    }

    /// Queue remote paths for download into `local_dir` as one batch.
    pub(crate) async fn enqueue_downloads(
        &self,
        sink: Arc<dyn ProgressSink>,
        session_id: &str,
        remote_paths: &[String],
        local_dir: &str,
        conflict: Option<ConflictPolicy>,
    ) -> AppResult<Enqueued> {
        let lease = self.require_active().map_err(AppError::from)?;
        let entry = self.sessions.get(session_id)?;
        let fs = entry.remote_fs().await?;
        let settings = self.transfer_settings(conflict);
        let skip_junk = self.skip_junk();
        let tar_ssh = entry.tar_ssh().await;
        let requests = remote_paths
            .iter()
            .map(|remote_path| {
                DownloadRequest::new(
                    fs.clone(),
                    session_id,
                    remote_path,
                    local_dir,
                    settings.clone(),
                )
                .skipping_junk(skip_junk)
            })
            .collect();
        Ok(self
            .transfers
            .enqueue_downloads_tracked(lease.context_id(), &sink, requests, tar_ssh)
            .await)
    }

    /// Queue one recursive delete / chmod item per target.
    pub(crate) async fn enqueue_tree_ops(
        &self,
        sink: Arc<dyn ProgressSink>,
        session_id: &str,
        targets: &[TreeTargetSpec],
        action: TreeAction,
    ) -> AppResult<Enqueued> {
        let lease = self.require_active().map_err(AppError::from)?;
        let entry = self.sessions.get(session_id)?;
        let fs = entry.remote_fs().await?;
        let settings = self.transfer_settings(None);
        let shell = if settings.tar_acceleration && action == TreeAction::Delete {
            entry.rm_ssh().await
        } else {
            None
        };
        let requests = targets
            .iter()
            .map(|target| TreeRequest {
                fs: fs.clone(),
                session_id,
                path: &target.path,
                is_dir: target.is_dir,
                action,
                settings: settings.clone(),
                shell: shell.clone(),
            })
            .collect();
        Ok(self
            .transfers
            .enqueue_tree_ops_tracked(lease.context_id(), &sink, requests)
            .await)
    }
}
