//! `delete` and `chmod`: recursive remote tree operations run as items of
//! the tab's transfer queue.

use serde::Deserialize;
use serde_json::Value;
use serverus_domain::agent::access::OperationClass;

use super::super::args::{default_tree_wait, parse, remote_path};
use super::super::files::changed;
use super::super::journal::Journal;
use super::super::{Ctx, ToolError, ToolResult};
use super::{run_queued, sink};
use crate::session::remote_fs::TreeAction;
use crate::state::TreeTargetSpec;

#[derive(Deserialize)]
struct DeleteArgs {
    server: String,
    paths: Vec<String>,
    #[serde(default = "default_tree_wait")]
    wait_seconds: u64,
}

pub async fn delete(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: DeleteArgs = parse(arguments)?;
    let paths = args
        .paths
        .iter()
        .map(|path| remote_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    if paths.is_empty() {
        return Err("Give at least one remote path.".into());
    }
    if paths
        .iter()
        .any(|path| path.trim_end_matches('/').is_empty())
    {
        return Err("Refusing to delete the root directory.".into());
    }
    let summary = paths.join(", ");
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Destructive)?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "delete",
        &summary,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Destructive,
            "Delete",
            &summary,
        )
        .await?;
        // A symlink to a directory is removed as a link, never descended —
        // so the link itself is examined (`lstat`), not what it points to.
        let targets = ctx
            .on_fs(&tab, |fs| async move {
                let mut targets = Vec::new();
                for path in paths {
                    let entry = fs.lstat(&path).await?;
                    targets.push(TreeTargetSpec {
                        is_dir: entry.is_dir && !entry.is_symlink,
                        path,
                    });
                }
                Ok(targets)
            })
            .await?;
        let application = ctx.state().application.clone();
        let (sink, session_id) = (sink(ctx), tab.session_id.clone());
        run_queued(ctx, args.wait_seconds, None, || async move {
            application
                .enqueue_tree_ops(sink, &session_id, &targets, TreeAction::Delete)
                .await
        })
        .await
    }
    .await;
    journal.finish(result)
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
enum ApplyTo {
    Files,
    Dirs,
    #[default]
    Both,
}

#[derive(Deserialize)]
struct ChmodArgs {
    server: String,
    path: String,
    mode: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    apply_to: ApplyTo,
    #[serde(default = "default_tree_wait")]
    wait_seconds: u64,
}

pub async fn chmod(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: ChmodArgs = parse(arguments)?;
    let path = remote_path(&args.path)?;
    let mode = u32::from_str_radix(args.mode.trim(), 8)
        .ok()
        .filter(|mode| *mode <= 0o7777)
        .ok_or_else(|| ToolError(format!("`{}` is not an octal mode like 755.", args.mode)))?;
    let summary = format!(
        "{path} → {mode:o}{}",
        if args.recursive { " (recursive)" } else { "" }
    );
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Destructive)?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "chmod",
        &summary,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Destructive,
            "Change permissions",
            &summary,
        )
        .await?;
        // Symlinks are never chmodded: on most servers that would change
        // whatever they point to, possibly outside the tree.
        let target = path.clone();
        let entry = ctx
            .on_fs(&tab, |fs| async move { fs.lstat(&target).await })
            .await?;
        if entry.is_symlink {
            return Err(ToolError(format!(
                "`{path}` is a symlink; permissions are changed on real files and directories only. Chmod the path it points to instead."
            )));
        }
        if !args.recursive {
            let target = path.clone();
            ctx.on_fs(&tab, |fs| async move { fs.chmod(&target, mode).await })
                .await?;
            changed(ctx, &tab, &[&path]);
            return Ok(format!("Changed {summary}"));
        }
        if !entry.is_dir {
            return Err(
                "recursive chmod needs a real directory; drop `recursive` for a file.".into(),
            );
        }
        let action = TreeAction::Chmod {
            mode,
            files: !matches!(args.apply_to, ApplyTo::Dirs),
            dirs: !matches!(args.apply_to, ApplyTo::Files),
        };
        let targets = vec![TreeTargetSpec { path, is_dir: true }];
        let application = ctx.state().application.clone();
        let (sink, session_id) = (sink(ctx), tab.session_id.clone());
        run_queued(ctx, args.wait_seconds, None, || async move {
            application
                .enqueue_tree_ops(sink, &session_id, &targets, action)
                .await
        })
        .await
    }
    .await;
    journal.finish(result)
}
