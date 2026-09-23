//! Recursive delete / chmod adapters. They queue one item per selected entry
//! in the session's transfer panel and return at once; progress, cancel and
//! the outcome live there instead of in a call that blocks for minutes.

use serde::Deserialize;
use specta::Type;

use super::prelude::*;
use super::transfers::transfer_settings;
use crate::session::remote_fs::TreeAction;
use crate::transfer::{ProgressSink, TreeRequest};

/// One selected entry. `is_dir` means a real directory: a symlink to one is
/// removed as a link, never descended.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct TreeTarget {
    pub path: String,
    pub is_dir: bool,
}

/// Which entries below the selected directories a recursive chmod touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ChmodScope {
    Files,
    Dirs,
    Both,
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_delete(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    targets: Vec<TreeTarget>,
) -> ApiResult<()> {
    enqueue(app, &state, &session_id, targets, TreeAction::Delete).await
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_chmod(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    targets: Vec<TreeTarget>,
    mode: u32,
    scope: ChmodScope,
) -> ApiResult<()> {
    let action = TreeAction::Chmod {
        mode,
        files: scope != ChmodScope::Dirs,
        dirs: scope != ChmodScope::Files,
    };
    enqueue(app, &state, &session_id, targets, action).await
}

async fn enqueue(
    app: tauri::AppHandle,
    state: &AppState,
    session_id: &str,
    targets: Vec<TreeTarget>,
    action: TreeAction,
) -> ApiResult<()> {
    let lease = state.application.require_active().map_err(AppError::from)?;
    let entry = state.sessions.get(session_id)?;
    let fs = entry.remote_fs().await?;
    let settings = transfer_settings(state);
    let shell = if settings.tar_acceleration && action == TreeAction::Delete {
        entry.rm_ssh().await
    } else {
        None
    };
    let sink: std::sync::Arc<dyn ProgressSink> = std::sync::Arc::new(app);
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
    state
        .transfers
        .enqueue_tree_ops(lease.context_id(), &sink, requests)
        .await
        .map_err(Into::into)
}
