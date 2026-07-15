use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{join_remote, parent_remote, RemoteFs};

const REMOTE_TEMP_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Upload an edit to a unique sibling and promote it only after the staging
/// upload has been finalized. Cancellation is observed before promotion; once
/// replacement starts it runs to completion so rollback cannot be interrupted.
pub(super) async fn upload_back_controlled(
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
    let mut source = match source {
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
            let mut destination = fs_remote
                .open_write_replacement(&remote_temp, remote_path)
                .await?;
            tokio::io::copy(&mut source, &mut destination)
                .await
                .map_err(|error| AppError::Transfer(format!("auto-upload: {error}")))?;
            destination
                .shutdown()
                .await
                .map_err(|error| AppError::Transfer(format!("auto-upload finalize: {error}")))?;
            drop(destination);
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
