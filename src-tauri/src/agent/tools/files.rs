//! Direct remote file operations: listing, reading, mkdir, rename.

use serde::Deserialize;
use serde_json::{json, Value};
use serverus_domain::agent::access::OperationClass;
use tokio::io::AsyncReadExt;

use super::args::{parse, remote_path};
use super::context::TabRef;
use super::journal::Journal;
use super::{Ctx, ToolError, ToolResult};
use crate::agent::hub::emit;
use crate::agent::types::AgentFsChangedEvent;
use crate::error::AppError;
use crate::session::remote_fs::RemoteEntry;

/// Largest `read_file` chunk.
const READ_LIMIT: u64 = 4 * 1024 * 1024;

/// Tell panes showing these paths' parents to relist.
pub(super) fn changed(ctx: &Ctx<'_>, tab: &TabRef, paths: &[&str]) {
    emit(
        ctx.app,
        AgentFsChangedEvent {
            session_id: tab.session_id.clone(),
            paths: paths.iter().map(|path| path.to_string()).collect(),
        },
    );
}

fn entry_json(entry: &RemoteEntry) -> Value {
    let kind = if entry.is_symlink {
        "symlink"
    } else if entry.is_dir {
        "dir"
    } else {
        "file"
    };
    let mut value = json!({ "name": entry.name, "type": kind });
    if !entry.is_dir {
        value["size"] = json!(entry.size);
    }
    if let Some(mtime) = entry.mtime {
        value["modified"] = json!(chrono::DateTime::from_timestamp(mtime, 0)
            .map(|time| time.to_rfc3339())
            .unwrap_or_default());
    }
    if let Some(mode) = entry.permissions {
        value["mode"] = json!(format!("{:o}", mode & 0o7777));
    }
    value
}

#[derive(Deserialize)]
struct ListArgs {
    server: String,
    #[serde(default)]
    path: Option<String>,
}

pub async fn list_directory(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: ListArgs = parse(arguments)?;
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    let tab = ctx.tab(&server, false).await?;
    let summary = args
        .path
        .clone()
        .unwrap_or_else(|| "(login directory)".into());
    let journal = Journal::start(
        ctx.app,
        "list_directory",
        &summary,
        Some(&tab.session_id),
        &server.id,
    );
    let requested = args.path.as_deref().map(remote_path).transpose();
    let result = async {
        let requested = requested?;
        let (path, mut entries) = ctx
            .on_fs(&tab, |fs| async move {
                let path = match requested {
                    Some(path) => path,
                    None => fs.home_dir().await?,
                };
                let entries = fs.list(&path).await?;
                Ok((path, entries))
            })
            .await?;
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        let listing: Vec<Value> = entries.iter().map(entry_json).collect();
        Ok(
            serde_json::to_string_pretty(&json!({ "path": path, "entries": listing }))
                .unwrap_or_default(),
        )
    }
    .await;
    journal.finish(result)
}

#[derive(Deserialize)]
struct ReadArgs {
    server: String,
    path: String,
    #[serde(default)]
    offset: u64,
    #[serde(default = "default_read")]
    max_bytes: u64,
}

fn default_read() -> u64 {
    256 * 1024
}

pub async fn read_file(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: ReadArgs = parse(arguments)?;
    let path = remote_path(&args.path)?;
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "read_file",
        &path,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        let limit = args.max_bytes.clamp(1, READ_LIMIT);
        let offset = args.offset;
        let target = path.clone();
        let (meta, bytes) = ctx
            .on_fs(&tab, |fs| async move {
                let meta = fs.stat(&target).await?;
                if meta.is_dir {
                    return Err(AppError::Other(format!(
                        "`{target}` is a directory; use list_directory."
                    )));
                }
                let mut bytes = Vec::new();
                // Size 0 may just be unknown (Linux `/proc`, `/sys`): read.
                if offset < meta.size || meta.size == 0 {
                    fs.open_read(&target, offset)
                        .await?
                        .take(limit)
                        .read_to_end(&mut bytes)
                        .await
                        .map_err(|error| {
                            AppError::RemoteFs(format!("reading `{target}`: {error}"))
                        })?;
                }
                Ok((meta, bytes))
            })
            .await?;
        if bytes[..bytes.len().min(8192)].contains(&0) {
            return Err(ToolError(format!(
                "`{path}` looks binary ({} bytes); use download to copy it to this computer.",
                meta.size
            )));
        }
        let end = args.offset + bytes.len() as u64;
        // A zero size with content read means the server did not know it.
        let size_known = meta.size > 0 || bytes.is_empty();
        let total = if size_known {
            format!("{} bytes total", meta.size)
        } else {
            "size unknown".to_string()
        };
        let more = if size_known && end < meta.size {
            format!(" — more follows; continue with offset={end}")
        } else if !size_known && bytes.len() as u64 == limit {
            format!(" — there may be more; continue with offset={end}")
        } else {
            String::new()
        };
        Ok(format!(
            "{path}: {total}, showing bytes {}–{end}{more}\n{}",
            args.offset,
            String::from_utf8_lossy(&bytes)
        ))
    }
    .await;
    journal.finish(result)
}

#[derive(Deserialize)]
struct MkdirArgs {
    server: String,
    path: String,
    #[serde(default)]
    parents: bool,
}

pub async fn make_directory(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: MkdirArgs = parse(arguments)?;
    let path = remote_path(&args.path)?;
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Write)?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "make_directory",
        &path,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Write,
            "Create a directory",
            &path,
        )
        .await?;
        let (target, parents) = (path.clone(), args.parents);
        ctx.on_fs(&tab, |fs| async move {
            if !parents {
                return fs.mkdir(&target).await;
            }
            // A relative path stays relative (to the login directory).
            let mut current = if target.starts_with('/') {
                "/".to_string()
            } else {
                String::new()
            };
            for part in target.split('/').filter(|part| !part.is_empty()) {
                if !current.is_empty() && !current.ends_with('/') {
                    current.push('/');
                }
                current.push_str(part);
                if !fs.exists(&current).await? {
                    fs.mkdir(&current).await?;
                }
            }
            Ok(())
        })
        .await?;
        changed(ctx, &tab, &[&path]);
        Ok(format!("Created {path}"))
    }
    .await;
    journal.finish(result)
}

#[derive(Deserialize)]
struct RenameArgs {
    server: String,
    from: String,
    to: String,
}

pub async fn rename(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: RenameArgs = parse(arguments)?;
    let (from, to) = (remote_path(&args.from)?, remote_path(&args.to)?);
    let summary = format!("{from} → {to}");
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Write)?;
    let tab = ctx.tab(&server, false).await?;
    let journal = Journal::start(
        ctx.app,
        "rename",
        &summary,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Write,
            "Rename or move",
            &summary,
        )
        .await?;
        let (source, target) = (from.clone(), to.clone());
        ctx.on_fs(&tab, |fs| async move {
            if fs.exists(&target).await? {
                return Err(AppError::Other(format!(
                    "`{target}` already exists; delete it first or pick another name."
                )));
            }
            fs.rename(&source, &target).await
        })
        .await?;
        changed(ctx, &tab, &[&from, &to]);
        Ok(format!("Renamed {summary}"))
    }
    .await;
    journal.finish(result)
}
