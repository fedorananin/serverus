//! Runtime ownership and session-admission policies for desktop use cases.

use std::future::Future;
use std::sync::Arc;

use serverus_runtime::ContextLease;

use super::DesktopApplication;
use crate::error::{AppError, AppResult};
use crate::session::SessionEntry;

impl DesktopApplication {
    pub(crate) async fn run_session_operation<T, F, Fut>(
        &self,
        session_id: &str,
        operation: F,
    ) -> AppResult<T>
    where
        F: FnOnce(Arc<SessionEntry>, ContextLease) -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        let lease = self.require_active().map_err(AppError::from)?;
        let entry = self.sessions.get(session_id)?;
        let outcome = self
            .sessions
            .run_session_operation(lease.context_id(), &entry, || {
                operation(entry.clone(), lease.clone())
            })
            .await;
        lease.validate(self).map_err(AppError::from)?;
        if !self.sessions.owns_entry(&entry) {
            return Err(AppError::SessionNotFound);
        }
        outcome
    }

    pub(crate) async fn run_session_blocking_operation<T, F>(
        &self,
        session_id: &str,
        operation: F,
    ) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(Arc<SessionEntry>, ContextLease) -> AppResult<T> + Send + 'static,
    {
        let lease = self.require_active().map_err(AppError::from)?;
        let entry = self.sessions.get(session_id)?;
        let outcome = self
            .sessions
            .run_session_blocking_operation(lease.context_id(), &entry, {
                let entry = entry.clone();
                let lease = lease.clone();
                move || operation(entry, lease)
            })
            .await;
        lease.validate(self).map_err(AppError::from)?;
        if !self.sessions.owns_entry(&entry) {
            return Err(AppError::SessionNotFound);
        }
        outcome
    }

    pub(crate) async fn run_owned_operation<T: Send + 'static>(
        &self,
        operation: impl Future<Output = AppResult<T>> + Send + 'static,
    ) -> AppResult<T> {
        match tauri::async_runtime::spawn(operation).await {
            Ok(result) => result,
            Err(error) => Err(AppError::Other(format!(
                "owned lifecycle operation failed: {error}"
            ))),
        }
    }
}
