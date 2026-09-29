//! `write_file`, `upload` and `download`.

use serde::Deserialize;
use serde_json::Value;
use serverus_domain::agent::access::OperationClass;

use super::super::args::{default_wait, local_path, parse, remote_path, IfExists};
use super::super::journal::Journal;
use super::super::{Ctx, ToolError, ToolResult};
use super::{run_queued, sink, StagingDir};
use crate::session::remote_fs::parent_remote;

#[derive(Deserialize)]
struct WriteArgs {
    server: String,
    path: String,
    content: String,
    #[serde(default)]
    if_exists: IfExists,
}

pub async fn write_file(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: WriteArgs = parse(arguments)?;
    let path = remote_path(&args.path)?;
    let name = path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or("The path must name a file.")?
        .to_string();
    let summary = format!("{path} ({} bytes)", args.content.len());
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Write)?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "write_file",
        &summary,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Write,
            "Write a file",
            &summary,
        )
        .await?;
        let staging = StagingDir::create()?;
        let local = staging.path().join(&name);
        std::fs::write(&local, args.content.as_bytes())
            .map_err(|error| ToolError(format!("staging the content: {error}")))?;
        let local = local.to_string_lossy().into_owned();
        let remote_dir = parent_remote(&path);
        let application = ctx.state().application.clone();
        let sink = sink(ctx);
        let session_id = tab.session_id.clone();
        let policy = Some(args.if_exists.policy());
        run_queued(ctx, default_wait(), Some(staging), || async move {
            application
                .enqueue_uploads(sink, &session_id, &[local], &remote_dir, policy)
                .await
        })
        .await
    }
    .await;
    journal.finish(result)
}

#[derive(Deserialize)]
struct UploadArgs {
    server: String,
    local_paths: Vec<String>,
    remote_dir: String,
    #[serde(default)]
    if_exists: IfExists,
    #[serde(default = "default_wait")]
    wait_seconds: u64,
}

pub async fn upload(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: UploadArgs = parse(arguments)?;
    let remote_dir = remote_path(&args.remote_dir)?;
    let mut locals = Vec::new();
    for path in &args.local_paths {
        let local = local_path(path)?;
        if !local.exists() {
            return Err(ToolError(format!(
                "`{}` does not exist on this computer.",
                local.display()
            )));
        }
        locals.push(local.to_string_lossy().into_owned());
    }
    if locals.is_empty() {
        return Err("Give at least one local path.".into());
    }
    let summary = format!("{} → {remote_dir}", locals.join(", "));
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Write)?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "upload",
        &summary,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Write,
            "Upload files",
            &summary,
        )
        .await?;
        let application = ctx.state().application.clone();
        let (sink, session_id, policy) =
            (sink(ctx), tab.session_id.clone(), args.if_exists.policy());
        run_queued(ctx, args.wait_seconds, None, || async move {
            application
                .enqueue_uploads(sink, &session_id, &locals, &remote_dir, Some(policy))
                .await
        })
        .await
    }
    .await;
    journal.finish(result)
}

#[derive(Deserialize)]
struct DownloadArgs {
    server: String,
    remote_paths: Vec<String>,
    local_dir: String,
    #[serde(default)]
    if_exists: IfExists,
    #[serde(default = "default_wait")]
    wait_seconds: u64,
}

pub async fn download(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: DownloadArgs = parse(arguments)?;
    let local_dir = local_path(&args.local_dir)?;
    let remotes = args
        .remote_paths
        .iter()
        .map(|path| remote_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    if remotes.is_empty() {
        return Err("Give at least one remote path.".into());
    }
    let summary = format!("{} → {}", remotes.join(", "), local_dir.display());
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Write)?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "download",
        &summary,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Write,
            "Download files",
            &summary,
        )
        .await?;
        std::fs::create_dir_all(&local_dir)
            .map_err(|error| ToolError(format!("creating {}: {error}", local_dir.display())))?;
        let local_dir = local_dir.to_string_lossy().into_owned();
        let application = ctx.state().application.clone();
        let (sink, session_id, policy) =
            (sink(ctx), tab.session_id.clone(), args.if_exists.policy());
        run_queued(ctx, args.wait_seconds, None, || async move {
            application
                .enqueue_downloads(sink, &session_id, &remotes, &local_dir, Some(policy))
                .await
        })
        .await
    }
    .await;
    journal.finish(result)
}
