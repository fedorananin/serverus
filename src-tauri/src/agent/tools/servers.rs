//! `list_servers`.

use serde::Deserialize;
use serde_json::{json, Value};
use serverus_domain::agent::access::is_visible;

use super::args::parse;
use super::{Ctx, ToolResult};

#[derive(Deserialize)]
struct ListArgs {
    #[serde(default)]
    query: Option<String>,
}

pub async fn list_servers(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: ListArgs = parse(arguments)?;
    let policy = ctx.policy().await?;
    // Tabs are best-effort: a list without them is still useful.
    let tabs = ctx.tabs().await.unwrap_or_default();
    let query = args.query.map(|query| query.trim().to_lowercase());

    let servers: Vec<Value> = policy
        .servers
        .iter()
        .filter(|server| is_visible(server.level))
        .filter(|server| {
            query.as_deref().is_none_or(|query| {
                server.path().to_lowercase().contains(query)
                    || server.host.to_lowercase().contains(query)
            })
        })
        .map(|server| {
            let open: Vec<&_> = tabs
                .iter()
                .filter(|tab| tab.connection_id == server.id)
                .collect();
            json!({
                "id": server.id,
                "name": server.name,
                "path": server.path(),
                "protocol": server.protocol,
                "host": server.host,
                "port": server.port,
                "user": server.username,
                "access": server.access_label(),
                "shell": server.terminal,
                "open_tabs": open.len(),
                "current": open.iter().any(|tab| tab.active),
            })
        })
        .collect();

    let adding = policy.adding_connections();
    if servers.is_empty() {
        let hint = if adding == "no" {
            ""
        } else {
            " You can add one with create_connection if the user gives you its details."
        };
        return Ok(if query.is_some() {
            "No shared server matches that query.".into()
        } else {
            format!("The user has not shared any server with you yet. They can set a connection's or folder's \"AI agent access\" in Serverus, or enable full access for all servers in Settings → AI Agent.{hint}")
        });
    }
    let listing = json!({ "servers": servers, "can_add_connections": adding });
    Ok(serde_json::to_string_pretty(&listing).unwrap_or_default())
}
