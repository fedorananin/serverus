use std::future::Future;
use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;

use crate::error::{AppError, AppResult};

use super::registry::OpenAdmissionRegistry;

impl OpenAdmissionRegistry {
    pub(in crate::watcher) async fn run<T, F, Fut>(
        self: &Arc<Self>,
        expected_context_id: RuntimeContextId,
        session_id: &str,
        operation: F,
    ) -> AppResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        let admission = self.begin(expected_context_id, session_id)?;
        tokio::select! {
            biased;
            _ = admission.cancelled() => Err(AppError::SessionNotFound),
            result = operation() => result,
        }
    }
}
