//! The MCP tools, backed by the desktop application.

mod args;
mod authorize;
mod context;
mod create;
mod files;
mod journal;
mod keys;
mod queue;
mod servers;
mod terminal;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;
use tauri::AppHandle;

use super::mcp::server::{ToolHost, ToolOutput};
use super::mcp::{tool_specs, INSTRUCTIONS};
use crate::autolock::{ActivityHold, ActivityTracker};
use crate::error::AppError;

pub(crate) use context::Ctx;

/// A failed tool call; the message goes to the model verbatim.
#[derive(Debug)]
pub struct ToolError(pub String);

impl From<AppError> for ToolError {
    fn from(error: AppError) -> Self {
        ToolError(error.to_string())
    }
}

impl From<String> for ToolError {
    fn from(message: String) -> Self {
        ToolError(message)
    }
}

impl From<&str> for ToolError {
    fn from(message: &str) -> Self {
        ToolError(message.to_string())
    }
}

pub type ToolResult = Result<String, ToolError>;

/// Every tool [`Toolbox`] dispatches (kept in step with the catalog).
pub(crate) const NAMES: [&str; 15] = [
    "list_servers",
    "create_connection",
    "run_command",
    "read_terminal",
    "send_input",
    "list_directory",
    "read_file",
    "write_file",
    "upload",
    "download",
    "transfer_status",
    "make_directory",
    "rename",
    "delete",
    "chmod",
];

/// Keeps the vault unlocked for as long as one agent stays connected —
/// from its first request that agent access admitted until it disconnects
/// (or a request finds agent access switched off). Neither the idle
/// timeout nor sleep locks the vault meanwhile; an explicit lock still does.
#[derive(Default)]
pub(crate) struct ConnectionHold(Mutex<Option<ActivityHold>>);

impl ConnectionHold {
    pub fn engage(&self, activity: &Arc<ActivityTracker>) {
        let mut hold = self.0.lock().unwrap();
        if hold.is_none() {
            *hold = Some(activity.hold());
        }
    }

    pub fn release(&self) {
        self.0.lock().unwrap().take();
    }
}

/// Serves the tool catalog against the running app — one per agent
/// connection, so dropping it (the agent disconnected) releases its hold.
pub struct Toolbox {
    app: AppHandle,
    hold: ConnectionHold,
}

impl Toolbox {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            hold: ConnectionHold::default(),
        }
    }
}

#[async_trait]
impl ToolHost for Toolbox {
    fn instructions(&self) -> String {
        INSTRUCTIONS.to_string()
    }

    fn tools(&self) -> Value {
        tool_specs::all()
    }

    async fn call(&self, name: String, arguments: Value) -> Option<ToolOutput> {
        if !NAMES.contains(&name.as_str()) {
            return None;
        }
        let ctx = Ctx::new(&self.app, &self.hold);
        let result = match name.as_str() {
            "list_servers" => servers::list_servers(&ctx, arguments).await,
            "create_connection" => create::create_connection(&ctx, arguments).await,
            "run_command" => terminal::run_command(&ctx, arguments).await,
            "read_terminal" => terminal::read_terminal(&ctx, arguments).await,
            "send_input" => terminal::send_input(&ctx, arguments).await,
            "list_directory" => files::list_directory(&ctx, arguments).await,
            "read_file" => files::read_file(&ctx, arguments).await,
            "make_directory" => files::make_directory(&ctx, arguments).await,
            "rename" => files::rename(&ctx, arguments).await,
            "chmod" => queue::chmod(&ctx, arguments).await,
            "write_file" => queue::write_file(&ctx, arguments).await,
            "upload" => queue::upload(&ctx, arguments).await,
            "download" => queue::download(&ctx, arguments).await,
            "transfer_status" => queue::transfer_status(&ctx, arguments).await,
            "delete" => queue::delete(&ctx, arguments).await,
            _ => return None,
        };
        Some(match result {
            Ok(text) => ToolOutput {
                text,
                is_error: false,
            },
            Err(ToolError(text)) => ToolOutput {
                text,
                is_error: true,
            },
        })
    }
}

#[cfg(test)]
mod tests;
