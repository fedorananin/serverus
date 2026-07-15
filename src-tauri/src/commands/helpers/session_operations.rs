use std::future::Future;

use crate::error::{ApiResult, AppResult};

/// Run an async session operation only while its context and exact session
/// registration remain current.
pub(in crate::commands) async fn run_session_operation<T, F, Fut>(
    application: &crate::state::DesktopApplication,
    session_id: &str,
    operation: F,
) -> ApiResult<T>
where
    F: FnOnce(std::sync::Arc<crate::session::SessionEntry>, serverus_runtime::ContextLease) -> Fut,
    Fut: Future<Output = AppResult<T>>,
{
    application
        .run_session_operation(session_id, operation)
        .await
        .map_err(Into::into)
}
