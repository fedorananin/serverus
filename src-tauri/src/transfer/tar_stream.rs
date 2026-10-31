//! tar-stream acceleration for SSH directory transfers (SPEC §6.2).
//!
//! Thousands of small files over SFTP crawl because of per-file round-trips;
//! piping one tar stream through the SSH session runs at line speed. The
//! remote side needs a `tar` binary (detected once per session); anything
//! else falls back to the plain per-file queue.

mod archive;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use russh::ChannelMsg;
use tokio::io::AsyncWriteExt;
use tokio_util::io::SyncIoBridge;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::parent_remote;
use crate::session::ssh::SshSession;

use super::{
    ensure_download_directory, open_download_root, Control, Direction, TransferItem, TransferState,
};
use archive::{pack_tree, unpack_confined};

pub struct TarJob {
    pub ssh: Arc<SshSession>,
    /// Leave `.DS_Store` / `Thumbs.db` out of the archive (upload) or out
    /// of the unpacked tree (download).
    pub skip_junk: bool,
}

/// POSIX single-quote escaping.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub async fn run(item: &Arc<TransferItem>, job: &TarJob) -> AppResult<TransferState> {
    match item.kind.direction() {
        Some(Direction::Download) => download(item, job).await,
        Some(Direction::Upload) => upload(item, job).await,
        None => Err(AppError::Transfer("not a byte transfer".into())),
    }
}

/// `tar -cf - -C <remote dir> .` → unpack into the local folder.
///
/// Archiving the folder's *contents* keeps every entry under the folder the
/// user downloads — the stream is untrusted, so the unpacker is confined to
/// that folder, never its parent — and follows the folder itself when it is
/// a symlink, as the per-file walk does.
async fn download(item: &Arc<TransferItem>, job: &TarJob) -> AppResult<TransferState> {
    let local_parent = item
        .local_path
        .parent()
        .ok_or_else(|| AppError::Transfer("bad local path".into()))?;
    let local_name = item
        .local_path
        .file_name()
        .ok_or_else(|| AppError::Transfer("bad local path".into()))?;
    tokio::fs::create_dir_all(local_parent)
        .await
        .map_err(|e| AppError::Transfer(e.to_string()))?;
    let parent = open_download_root(local_parent)?;
    ensure_download_directory(&parent, Path::new(local_name))?;
    let root = parent
        .open_dir(local_name)
        .map_err(|e| AppError::Transfer(format!("{}: {e}", item.local_path.display())))?;

    let channel = {
        let handle = job.ssh.handle.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|e| AppError::Transfer(format!("tar channel: {e}")))?
    };
    channel
        .exec(true, format!("tar -cf - -C {} .", shq(&item.remote_path)))
        .await
        .map_err(|e| AppError::Transfer(format!("tar exec: {e}")))?;

    let (mut pipe_w, pipe_r) = tokio::io::duplex(512 * 1024);
    // Bridge must be created on the runtime, then moved to the blocking pool.
    let bridge = SyncIoBridge::new(pipe_r);
    let skip_junk = job.skip_junk;
    let unpack = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        unpack_confined(tar::Archive::new(bridge), &root, skip_junk)
    });

    let mut ctrl = item.control.subscribe();
    let (mut read, write) = channel.split();
    let mut stderr: Vec<u8> = Vec::new();
    let mut exit_status: Option<u32> = None;
    let mut cancelled = false;

    loop {
        // Cancel must land even while the remote tar is silent. There is no
        // pause here (see `TransferItem::pausable`).
        let msg = tokio::select! {
            msg = read.wait() => msg,
            _ = ctrl.wait_for(|control| *control == Control::Cancel) => {
                cancelled = true;
                break;
            }
        };
        let Some(msg) = msg else { break };
        match msg {
            ChannelMsg::Data { data } => {
                if pipe_w.write_all(&data).await.is_err() {
                    break; // unpacker died — its error surfaces below
                }
                item.done.fetch_add(data.len() as u64, Ordering::Relaxed);
            }
            ChannelMsg::ExtendedData { data, .. } => {
                if stderr.len() < 4096 {
                    stderr.extend_from_slice(&data);
                }
            }
            ChannelMsg::ExitStatus { exit_status: s } => exit_status = Some(s),
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }
    drop(pipe_w);
    let _ = write.close().await;

    if cancelled {
        let _ = unpack.await;
        return Ok(TransferState::Cancelled);
    }
    unpack
        .await
        .map_err(|e| AppError::Transfer(format!("unpack task: {e}")))?
        .map_err(|e| AppError::Transfer(format!("unpack: {e}")))?;
    if let Some(status) = exit_status {
        if status != 0 {
            return Err(AppError::Transfer(format!(
                "remote tar exited with {status}: {}",
                String::from_utf8_lossy(&stderr)
            )));
        }
    }
    // Progress ends at 100% even though tar overhead != file bytes.
    item.done
        .store(item.total.load(Ordering::Relaxed), Ordering::Relaxed);
    Ok(TransferState::Done)
}

/// Pack locally → `tar -xf - -C <remote parent>` on the server.
async fn upload(item: &Arc<TransferItem>, job: &TarJob) -> AppResult<TransferState> {
    let remote_parent = parent_remote(&item.remote_path);
    let base_name = item
        .local_path
        .file_name()
        .ok_or_else(|| AppError::Transfer("bad local path".into()))?
        .to_string_lossy()
        .into_owned();
    let local_root = item.local_path.clone();

    let channel = {
        let handle = job.ssh.handle.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|e| AppError::Transfer(format!("tar channel: {e}")))?
    };
    channel
        .exec(true, format!("tar -xf - -C {}", shq(&remote_parent)))
        .await
        .map_err(|e| AppError::Transfer(format!("tar exec: {e}")))?;

    let (pipe_w, pipe_r) = tokio::io::duplex(512 * 1024);
    let bridge = SyncIoBridge::new(pipe_w);
    let skip_junk = job.skip_junk;
    let pack = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut builder = tar::Builder::new(bridge);
        builder.follow_symlinks(false);
        pack_tree(&mut builder, &base_name, &local_root, skip_junk)?;
        let mut inner = builder.into_inner()?;
        std::io::Write::flush(&mut inner)?;
        Ok(())
    });

    let (mut read, write) = channel.split();

    // Stream the archive. Pausing simply stops feeding it: the remote tar
    // waits on its stdin and the packer on the full pipe.
    let mut pipe_r = pipe_r;
    let mut ctrl = item.control.subscribe();
    let mut sink = write.make_writer();
    let send = super::copy_loop(item, &mut ctrl, &mut pipe_r, &mut sink).await;
    drop(pipe_r); // unblocks the packer if the stream stopped early
    let _ = sink.flush().await;
    drop(sink);
    let cancelled = matches!(send, Ok(true));
    let _ = write.eof().await;

    let mut stderr: Vec<u8> = Vec::new();
    let mut exit_status: Option<u32> = None;
    while let Some(msg) = read.wait().await {
        match msg {
            ChannelMsg::ExtendedData { data, .. } => {
                if stderr.len() < 4096 {
                    stderr.extend_from_slice(&data);
                }
            }
            ChannelMsg::ExitStatus { exit_status: s } => exit_status = Some(s),
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }
    let _ = write.close().await;
    let pack_result = pack.await;

    if cancelled {
        return Ok(TransferState::Cancelled);
    }
    send?;
    pack_result
        .map_err(|e| AppError::Transfer(format!("pack task: {e}")))?
        .map_err(|e| AppError::Transfer(format!("pack: {e}")))?;
    if let Some(status) = exit_status {
        if status != 0 {
            return Err(AppError::Transfer(format!(
                "remote tar exited with {status}: {}",
                String::from_utf8_lossy(&stderr)
            )));
        }
    }
    item.done
        .store(item.total.load(Ordering::Relaxed), Ordering::Relaxed);
    Ok(TransferState::Done)
}
