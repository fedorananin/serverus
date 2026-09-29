//! AI agent access over the Model Context Protocol.
//!
//! An agent (Claude Code, Claude Desktop, …) launches `serverus --mcp`
//! ([`shim`]), which relays to the running app's local socket
//! ([`transport`]). There the MCP server ([`mcp`]) exposes tools
//! ([`tools`]) that work in the user's visible tabs: commands are typed into
//! the real terminal ([`exec`]), transfers go through the tab's queue. The
//! vault's per-connection access levels decide what an agent may see and do
//! ([`catalog`], `serverus_domain::agent::access`), with confirmations and
//! tab handling delegated to the UI ([`bridge`]).

// The tool layer is platform-neutral; only the socket transport is Unix.
// Where nothing serves it yet (Windows), its code is unused, not dead.
#![cfg_attr(not(unix), allow(dead_code))]

pub mod bridge;
pub mod catalog;
mod control;
pub mod exec;
mod grants;
pub mod hub;
pub mod mcp;
pub mod placement;
pub mod shim;
mod tools;
pub mod transport;
pub mod types;
mod unlock;

pub use hub::AgentHub;

/// Start serving agents on the local socket (no-op where unsupported).
pub fn spawn(app: tauri::AppHandle) {
    #[cfg(unix)]
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;

        let hub = app.state::<crate::state::AppState>().agent.clone();
        match transport::bind().await {
            Ok(bound) => {
                hub.set_listen_status(hub::ListenStatus::Listening);
                // A toolbox per connection: its lifetime is the agent's.
                bound
                    .serve(move || std::sync::Arc::new(tools::Toolbox::new(app.clone())))
                    .await;
            }
            Err(problem) => hub.set_listen_status(hub::ListenStatus::Failed(problem)),
        }
    });
    #[cfg(not(unix))]
    let _ = app;
}

/// Clean up on application exit.
pub fn shutdown() {
    #[cfg(unix)]
    transport::remove_socket();
}

#[cfg(test)]
mod tests;
