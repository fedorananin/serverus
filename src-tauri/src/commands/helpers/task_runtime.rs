use std::future::Future;

use crate::error::{ApiResult, AppError, AppResult};

/// Run a blocking closure off the async runtime and flatten errors.
pub(in crate::commands) async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> ApiResult<T> {
    match tauri::async_runtime::spawn_blocking(operation).await {
        Ok(result) => result.map_err(Into::into),
        Err(error) => Err(AppError::Other(format!("background task failed: {error}")).into()),
    }
}

/// Own a multi-step lifecycle transaction independently from its invoking
/// IPC future. Irreversible effects always reach their runtime commit.
pub(in crate::commands) async fn run_owned_operation<T: Send + 'static>(
    operation: impl Future<Output = ApiResult<T>> + Send + 'static,
) -> ApiResult<T> {
    match tauri::async_runtime::spawn(operation).await {
        Ok(result) => result,
        Err(error) => {
            Err(AppError::Other(format!("owned lifecycle operation failed: {error}")).into())
        }
    }
}
