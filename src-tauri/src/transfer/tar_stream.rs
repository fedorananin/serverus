//! tar-stream acceleration for SSH directory transfers (SPEC §6.2).
//!
//! Thousands of small files over SFTP crawl because of per-file round-trips;
//! piping one tar stream through the SSH session runs at line speed. The
//! remote side needs a `tar` binary (detected once per session); anything
//! else falls back to the plain per-file queue.

use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll};

use russh::ChannelMsg;
use tokio::io::{AsyncRead, AsyncWriteExt, DuplexStream, ReadBuf};
use tokio_util::io::SyncIoBridge;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::parent_remote;
use crate::session::ssh::SshSession;

use super::{Control, TransferItem, TransferKind, TransferState};

pub struct TarJob {
    pub ssh: Arc<SshSession>,
}

/// POSIX single-quote escaping.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub async fn run(item: &Arc<TransferItem>, job: &TarJob) -> AppResult<TransferState> {
    match item.kind {
        TransferKind::Download => download(item, job).await,
        TransferKind::Upload => upload(item, job).await,
    }
}

/// `tar -cf - -C <remote parent> <name>` → unpack locally.
async fn download(item: &Arc<TransferItem>, job: &TarJob) -> AppResult<TransferState> {
    let remote_parent = parent_remote(&item.remote_path);
    let name = item
        .remote_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let local_parent = item
        .local_path
        .parent()
        .ok_or_else(|| AppError::Transfer("bad local path".into()))?
        .to_path_buf();
    tokio::fs::create_dir_all(&local_parent)
        .await
        .map_err(|e| AppError::Transfer(e.to_string()))?;

    let channel = {
        let handle = job.ssh.handle.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|e| AppError::Transfer(format!("tar channel: {e}")))?
    };
    channel
        .exec(
            true,
            format!("tar -cf - -C {} {}", shq(&remote_parent), shq(&name)),
        )
        .await
        .map_err(|e| AppError::Transfer(format!("tar exec: {e}")))?;

    let (mut pipe_w, pipe_r) = tokio::io::duplex(512 * 1024);
    // Bridge must be created on the runtime, then moved to the blocking pool.
    let bridge = SyncIoBridge::new(pipe_r);
    let unpack = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut archive = tar::Archive::new(bridge);
        archive.set_preserve_mtime(true);
        archive.unpack(&local_parent)
    });

    let mut ctrl = item.control.subscribe();
    let (mut read, write) = channel.split();
    let mut stderr: Vec<u8> = Vec::new();
    let mut exit_status: Option<u32> = None;
    let mut cancelled = false;

    while let Some(msg) = read.wait().await {
        if *ctrl.borrow_and_update() == Control::Cancel {
            cancelled = true;
            break;
        }
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
    let pack = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut builder = tar::Builder::new(bridge);
        builder.follow_symlinks(false);
        builder.append_dir_all(&base_name, &local_root)?;
        let mut inner = builder.into_inner()?;
        std::io::Write::flush(&mut inner)?;
        Ok(())
    });

    let reader = CountingReader {
        inner: pipe_r,
        item: item.clone(),
        ctrl: item.control.subscribe(),
    };
    let (mut read, write) = channel.split();

    // Stream the archive; a cancel surfaces as a read error.
    let send = write.data(reader).await;
    let cancelled = matches!(*item.control.subscribe().borrow(), Control::Cancel);
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
    send.map_err(|e| AppError::Transfer(format!("tar stream: {e}")))?;
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

/// Progress + cancellation adapter around the archive pipe.
struct CountingReader {
    inner: DuplexStream,
    item: Arc<TransferItem>,
    ctrl: tokio::sync::watch::Receiver<Control>,
}

impl AsyncRead for CountingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if *self.ctrl.borrow() == Control::Cancel {
            return Poll::Ready(Err(std::io::Error::other("cancelled")));
        }
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let n = buf.filled().len() - before;
            self.item.done.fetch_add(n as u64, Ordering::Relaxed);
        }
        result
    }
}
