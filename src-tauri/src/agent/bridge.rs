//! Request/response round trips to the UI. The backend publishes an
//! [`AgentUiRequestEvent`]; the UI answers through `agent_ui_respond` with
//! the same request id. A request that ends without an answer (timeout,
//! the MCP call cancelled) is retracted, so the UI can drop its dialog.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::oneshot;

use super::types::{AgentUiRequest, AgentUiRequestEvent, AgentUiResponse};
use crate::error::{AppError, AppResult};

#[derive(Default)]
pub struct UiBridge {
    pending: Mutex<HashMap<String, oneshot::Sender<AgentUiResponse>>>,
}

type Retract<'a> = Box<dyn FnOnce(&str) + Send + 'a>;

/// Removes the pending slot however the waiting future ends (answer,
/// timeout, or the MCP request being cancelled), retracting it from the UI
/// when it was never answered.
struct PendingSlot<'a> {
    bridge: &'a UiBridge,
    id: String,
    retract: Option<Retract<'a>>,
}

impl Drop for PendingSlot<'_> {
    fn drop(&mut self) {
        let unanswered = self.bridge.pending.lock().unwrap().remove(&self.id);
        if unanswered.is_some() {
            if let Some(retract) = self.retract.take() {
                retract(&self.id);
            }
        }
    }
}

impl UiBridge {
    /// Publish `request` through `publish` and wait up to `timeout` for the
    /// UI's answer; `retract` tells the UI when the request ends unanswered.
    /// An `Error` answer becomes an `Err`, a timeout the error
    /// `timeout_message`.
    pub async fn request(
        &self,
        request: AgentUiRequest,
        timeout: Duration,
        timeout_message: &str,
        publish: impl FnOnce(AgentUiRequestEvent) -> AppResult<()>,
        retract: impl FnOnce(&str) + Send + '_,
    ) -> AppResult<AgentUiResponse> {
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), sender);
        let _slot = PendingSlot {
            bridge: self,
            id: id.clone(),
            retract: Some(Box::new(retract)),
        };
        publish(AgentUiRequestEvent {
            request_id: id,
            request,
        })?;
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(AgentUiResponse::Error { message })) => Err(AppError::Other(message)),
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(AppError::Other("the Serverus window went away".into())),
            Err(_) => Err(AppError::Other(timeout_message.to_string())),
        }
    }

    /// Deliver the UI's answer. Returns false for an unknown or expired id.
    pub fn respond(&self, request_id: &str, response: AgentUiResponse) -> bool {
        match self.pending.lock().unwrap().remove(request_id) {
            Some(sender) => sender.send(response).is_ok(),
            None => false,
        }
    }
}
