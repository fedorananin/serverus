use std::future::Future;
use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;

use crate::error::{AppError, AppResult};

use super::super::{SessionEntry, SessionManager};

impl SessionManager {
    pub async fn run_session_operation<T, F, Fut>(
        &self,
        context_id: RuntimeContextId,
        entry: &Arc<SessionEntry>,
        operation: F,
    ) -> AppResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        let admission = self.operation_admissions.begin(context_id, &entry.id)?;
        if !self.owns_entry(entry) {
            return Err(AppError::SessionNotFound);
        }
        let result = tokio::select! {
            biased;
            error = admission.cancelled() => return Err(error),
            result = operation() => result,
        };
        if let Some(error) = admission.registry.cancellation_error(&admission) {
            return Err(error);
        }
        result
    }

    /// Keep blocking session work owned by the admission registry even when
    /// its IPC caller stops awaiting. Retirement suppresses the stale result,
    /// but does not report quiescence until the blocking closure has finished.
    pub async fn run_session_blocking_operation<T, F>(
        &self,
        context_id: RuntimeContextId,
        entry: &Arc<SessionEntry>,
        operation: F,
    ) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce() -> AppResult<T> + Send + 'static,
    {
        let admission = self.operation_admissions.begin(context_id, &entry.id)?;
        if !self.owns_entry(entry) {
            return Err(AppError::SessionNotFound);
        }
        let owned = tokio::spawn(async move {
            let mut blocking = tokio::task::spawn_blocking(operation);
            let cancellation = tokio::select! {
                biased;
                error = admission.cancelled() => Some(error),
                joined = &mut blocking => {
                    let result = map_blocking_join(joined)?;
                    if let Some(error) = admission.registry.cancellation_error(&admission) {
                        return Err(error);
                    }
                    return Ok(result);
                },
            };
            // The owner was already retired. Await quiescence, but never let
            // a late value, operation error, or join error replace that fact.
            let _stale_result = blocking.await;
            Err(cancellation.expect("cancellation branch sets an error"))
        });
        owned
            .await
            .map_err(|error| AppError::Other(format!("owned session operation failed: {error}")))?
    }
}

fn map_blocking_join<T>(result: Result<AppResult<T>, tokio::task::JoinError>) -> AppResult<T> {
    result
        .map_err(|error| AppError::Other(format!("blocking session operation failed: {error}")))?
}
