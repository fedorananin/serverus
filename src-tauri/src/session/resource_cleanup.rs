use async_trait::async_trait;

/// Desktop-owned resources that must retire with their parent session.
#[async_trait]
pub(crate) trait SessionResourceCleanup: Send + Sync {
    async fn clear_session(&self, session_id: &str);
}
