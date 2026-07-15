use std::future::Future;

use crate::error::{ApiResult, AppError};

/// Accept an async result only while the runtime generation and exact owner
/// that authorized it remain current; otherwise roll the late result back.
pub(in crate::commands) async fn validate_context_and_owner_or_rollback<
    T,
    Owner,
    Rollback,
    RollbackFuture,
>(
    application: &serverus_runtime::ApplicationHandle,
    lease: &serverus_runtime::ContextLease,
    owner_is_current: Owner,
    value: T,
    rollback: Rollback,
) -> ApiResult<T>
where
    Owner: FnOnce() -> bool,
    Rollback: FnOnce(T) -> RollbackFuture,
    RollbackFuture: Future<Output = ()>,
{
    let error = match lease.validate(application) {
        Err(error) => Some(AppError::from(error)),
        Ok(()) if !owner_is_current() => Some(AppError::SessionNotFound),
        Ok(()) => None,
    };
    if let Some(error) = error {
        rollback(value).await;
        Err(error.into())
    } else {
        Ok(value)
    }
}

pub(in crate::commands) async fn validate_context_or_rollback<T, Rollback, RollbackFuture>(
    application: &serverus_runtime::ApplicationHandle,
    lease: &serverus_runtime::ContextLease,
    value: T,
    rollback: Rollback,
) -> ApiResult<T>
where
    Rollback: FnOnce(T) -> RollbackFuture,
    RollbackFuture: Future<Output = ()>,
{
    validate_context_and_owner_or_rollback(application, lease, || true, value, rollback).await
}
