//! `transfer_status`.

use serde::Deserialize;
use serde_json::Value;

use super::super::args::parse;
use super::super::{Ctx, ToolResult};
use super::describe;

#[derive(Deserialize)]
struct StatusArgs {
    server: String,
    #[serde(default)]
    ids: Vec<String>,
}

pub async fn transfer_status(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: StatusArgs = parse(arguments)?;
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    let tab = ctx.tab(&server, false).await?;
    let transfers = &ctx.state().transfers;
    let ids = if args.ids.is_empty() {
        transfers.session_item_ids(&tab.session_id)
    } else {
        args.ids
    };
    let items = transfers.item_snapshots(&ids);
    if items.is_empty() {
        return Ok("The tab's transfer queue has no such items.".into());
    }
    Ok(describe(&items))
}
