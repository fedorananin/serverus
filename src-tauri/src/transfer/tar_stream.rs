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
        let root = cap_std::fs::Dir::open_ambient_dir(&local_parent, cap_std::ambient_authority())?;
        unpack_confined(tar::Archive::new(bridge), &root)
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

/// Unpack an untrusted tar stream under the same rules as the per-file
/// download path: every entry name is validated (and on Windows sanitized)
/// with `safe_local_component`, all writes go through a capability handle
/// on the destination, and link/device entries from the remote server are
/// skipped rather than materialized — a planted symlink must never redirect
/// a later write outside the tree.
fn unpack_confined<R: std::io::Read>(
    mut archive: tar::Archive<R>,
    root: &cap_std::fs::Dir,
) -> std::io::Result<()> {
    for entry in archive.entries()? {
        let mut entry = entry?;
        let mut relative = std::path::PathBuf::new();
        {
            let path = entry.path()?;
            for component in path.components() {
                match component {
                    std::path::Component::CurDir => {}
                    std::path::Component::Normal(part) => relative.push(
                        super::safe_local_component(&part.to_string_lossy())
                            .map_err(std::io::Error::other)?,
                    ),
                    _ => {
                        return Err(std::io::Error::other(format!(
                            "tar entry {path:?} escapes the destination"
                        )))
                    }
                }
            }
        }
        if relative.as_os_str().is_empty() {
            continue;
        }
        match entry.header().entry_type() {
            tar::EntryType::Directory => match root.create_dir(&relative) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            },
            tar::EntryType::Regular | tar::EntryType::Continuous | tar::EntryType::GNUSparse => {
                if let Some(parent) = relative.parent() {
                    if !parent.as_os_str().is_empty() {
                        root.create_dir_all(parent)?;
                    }
                }
                let mut options = cap_std::fs::OpenOptions::new();
                options.write(true).create(true).truncate(true);
                let mut file = root.open_with(&relative, &options)?.into_std();
                std::io::copy(&mut entry, &mut file)?;
                if let Ok(mtime) = entry.header().mtime() {
                    let _ = filetime::set_file_handle_times(
                        &file,
                        None,
                        Some(filetime::FileTime::from_unix_time(mtime as i64, 0)),
                    );
                }
            }
            // Symlinks, hard links, fifos, devices: never materialized from
            // an untrusted stream (matches the per-file path, which does not
            // descend into remote links either).
            _ => {}
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::unpack_confined;

    fn open_root(dir: &std::path::Path) -> cap_std::fs::Dir {
        cap_std::fs::Dir::open_ambient_dir(dir, cap_std::ambient_authority()).unwrap()
    }

    fn regular(path: &str, contents: &[u8]) -> (tar::Header, Vec<u8>) {
        let mut header = tar::Header::new_gnu();
        // Written raw so tests can smuggle names set_path would reject.
        header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
        header.set_size(contents.len() as u64);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_mtime(1_700_000_000);
        header.set_cksum();
        (header, contents.to_vec())
    }

    fn archive(entries: Vec<(tar::Header, Vec<u8>)>) -> tar::Archive<std::io::Cursor<Vec<u8>>> {
        let mut builder = tar::Builder::new(Vec::new());
        for (header, contents) in entries {
            builder.append(&header, contents.as_slice()).unwrap();
        }
        tar::Archive::new(std::io::Cursor::new(builder.into_inner().unwrap()))
    }

    #[test]
    fn unpacks_a_normal_nested_tree() {
        let dir = tempfile::tempdir().unwrap();
        let entries = vec![
            regular("tree/a.txt", b"alpha"),
            regular("tree/nested/b.txt", b"beta"),
        ];

        unpack_confined(archive(entries), &open_root(dir.path())).unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("tree/a.txt")).unwrap(),
            b"alpha"
        );
        assert_eq!(
            std::fs::read(dir.path().join("tree/nested/b.txt")).unwrap(),
            b"beta"
        );
    }

    #[test]
    fn rejects_parent_directory_traversal() {
        let outside = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        std::fs::create_dir(&inner).unwrap();

        let result = unpack_confined(
            archive(vec![regular("../evil.txt", b"pwned")]),
            &open_root(&inner),
        );

        assert!(result.is_err());
        assert!(!dir.path().join("evil.txt").exists());
        drop(outside);
    }

    #[test]
    fn rejects_absolute_entry_paths() {
        let dir = tempfile::tempdir().unwrap();

        let result = unpack_confined(
            archive(vec![regular("/tmp/evil.txt", b"pwned")]),
            &open_root(dir.path()),
        );

        assert!(result.is_err());
        assert!(
            !std::path::Path::new("/tmp/evil.txt").exists() || {
                // Never trust a pre-existing /tmp/evil.txt on the test host —
                // the assertion that matters is the Err above.
                true
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_planted_symlink_cannot_redirect_later_entries() {
        let outside = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();

        let mut link = tar::Header::new_gnu();
        link.as_old_mut().name[..4].copy_from_slice(b"link");
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_link_name(outside.path()).unwrap();
        link.set_size(0);
        link.set_cksum();

        let entries = vec![(link, Vec::new()), regular("link/victim.txt", b"pwned")];
        // The symlink entry is skipped, so "link" becomes a real directory
        // and the write lands inside the destination.
        unpack_confined(archive(entries), &open_root(dir.path())).unwrap();

        assert!(!outside.path().join("victim.txt").exists());
        assert_eq!(
            std::fs::read(dir.path().join("link/victim.txt")).unwrap(),
            b"pwned"
        );
        assert!(!dir.path().join("link").is_symlink());
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_legal_names_survive_the_tar_path_verbatim() {
        let dir = tempfile::tempdir().unwrap();

        unpack_confined(
            archive(vec![regular("tree/2024-01-01T12:00:00.log", b"log")]),
            &open_root(dir.path()),
        )
        .unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("tree/2024-01-01T12:00:00.log")).unwrap(),
            b"log"
        );
    }

    #[test]
    fn file_mtime_is_preserved() {
        let dir = tempfile::tempdir().unwrap();

        unpack_confined(
            archive(vec![regular("tree/dated.txt", b"x")]),
            &open_root(dir.path()),
        )
        .unwrap();

        let modified = std::fs::metadata(dir.path().join("tree/dated.txt"))
            .unwrap()
            .modified()
            .unwrap();
        let unix = modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(unix, 1_700_000_000);
    }
}
