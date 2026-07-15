//! S3 ACL command adapters.

use super::prelude::*;

fn s3_of(
    entry: &crate::session::SessionEntry,
) -> AppResult<std::sync::Arc<crate::session::s3::S3Fs>> {
    entry
        .s3
        .clone()
        .ok_or_else(|| AppError::Other("not an S3 session".into()))
}

/// Public/private status for a batch of objects — fetched in the background
/// after a listing; failures come back as `unknown`, never as an error.
#[tauri::command]
#[specta::specta]
pub async fn s3_acl_status(
    state: State<'_, AppState>,
    session_id: String,
    paths: Vec<String>,
) -> ApiResult<Vec<S3AclEntry>> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move { Ok(s3_of(&entry)?.acl_status_batch(paths).await) },
    )
    .await
}

/// Make objects public or private; directories apply recursively to every
/// object under the prefix. Returns the number of objects changed.
#[tauri::command]
#[specta::specta]
pub async fn s3_set_acl(
    state: State<'_, AppState>,
    session_id: String,
    targets: Vec<S3AclTarget>,
    make_public: bool,
) -> ApiResult<u32> {
    run_session_operation(
        &state.application,
        &session_id,
        move |entry, _lease| async move { s3_of(&entry)?.set_acl(targets, make_public).await },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn s3_set_upload_acl(
    state: State<'_, AppState>,
    session_id: String,
    mode: S3UploadAcl,
    persist: bool,
) -> ApiResult<Option<PublicVault>> {
    state
        .application
        .set_s3_upload_acl(session_id, mode, persist)
        .await
        .map_err(Into::into)
}
