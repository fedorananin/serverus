//! Deep folder-comparison commands (mapping-only; the walk lives in
//! `session::compare`).

use serverus_domain::fs_compare::CompareRules;

use super::prelude::*;
use crate::session::compare::{compare_subtree, CompareFilter, SubtreeStatus};

/// Compare the local tree under `local_path` with the remote tree under
/// `remote_path`. Returns as soon as a difference is found; `unknown` means
/// the tree could not be verified (too large, or unreadable locally).
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn remote_compare_subtree(
    state: State<'_, AppState>,
    session_id: String,
    local_path: String,
    remote_path: String,
    ignore_mtime: bool,
    coarse_remote_mtime: bool,
    include_hidden: bool,
    hide_local_junk: bool,
) -> ApiResult<SubtreeStatus> {
    let registry = state.compare.clone();
    let epoch = registry.epoch(&session_id);
    let cancel_key = session_id.clone();
    run_session_operation(&state.application, &session_id, move |entry, _lease| {
        let registry = registry.clone();
        async move {
            let fs = entry.remote_fs().await?;
            let rules = CompareRules {
                ignore_mtime,
                coarse_remote_mtime,
            };
            let filter = CompareFilter {
                include_hidden,
                hide_local_junk,
            };
            let cancelled = move || registry.epoch(&cancel_key) != epoch;
            compare_subtree(
                fs.as_ref(),
                &local_path,
                &remote_path,
                rules,
                filter,
                &cancelled,
            )
            .await
        }
    })
    .await
}

/// Cancel every in-flight subtree comparison for a session. Walks notice at
/// their next directory boundary.
#[tauri::command]
#[specta::specta]
pub async fn remote_compare_cancel(
    state: State<'_, AppState>,
    session_id: String,
) -> ApiResult<()> {
    state.compare.cancel(&session_id);
    Ok(())
}
