//! Server-side `rm -rf` for directory deletes over SSH: one exec instead of
//! one SFTP request per entry — the same trade the tar stream makes for
//! transfers. Progress still has a real total: `find` streams the entry
//! list first (its lines are counted as they arrive), then `rm -v` reports
//! every removal, one line each.
//!
//! Cancel closes the channel, and `rm` dies of SIGPIPE on its next output
//! flush. Pause only stops reading: `rm` blocks once the SSH window fills,
//! which can be thousands of entries later — the panel therefore offers no
//! pause for these items, only cancel.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use russh::ChannelMsg;
use tokio::sync::watch;

use crate::error::{AppError, AppResult};
use crate::session::ssh::SshSession;

use super::super::{Control, TransferItem, TransferState};
use super::TreeJob;

/// POSIX single-quote escaping.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

struct Exec {
    status: Option<u32>,
    stderr: String,
    cancelled: bool,
}

/// `None` = the server could not do it and nothing was removed; the caller
/// falls back to the portable walk.
pub(super) async fn delete(
    item: &Arc<TransferItem>,
    job: &TreeJob,
    ssh: &Arc<SshSession>,
) -> Option<AppResult<TransferState>> {
    let path = item.remote_path.trim_end_matches('/');
    if path.is_empty() || !path.starts_with('/') {
        return None;
    }
    let mut control = item.control.subscribe();
    item.done.store(0, Ordering::Relaxed);
    item.total.store(0, Ordering::Relaxed);

    job.scanning.store(true, Ordering::Relaxed);
    let scan = exec_counting(
        ssh,
        &format!("find {}", shq(path)),
        &item.total,
        &mut control,
    )
    .await;
    job.scanning.store(false, Ordering::Relaxed);
    let scan = match scan {
        Ok(scan) => scan,
        Err(_) => return None,
    };
    if scan.cancelled {
        return Some(Ok(TransferState::Cancelled));
    }
    if item.total.load(Ordering::Relaxed) == 0 {
        // `find` missing or the path unreadable — let the walk say why.
        return None;
    }

    let removal = format!("rm -rfv -- {}", shq(path));
    let removal = match exec_counting(ssh, &removal, &item.done, &mut control).await {
        Ok(removal) => removal,
        Err(error) if item.done.load(Ordering::Relaxed) > 0 => return Some(Err(error)),
        Err(_) => return None,
    };
    if removal.cancelled {
        return Some(Ok(TransferState::Cancelled));
    }
    let removed = item.done.load(Ordering::Relaxed);
    match removal.status {
        Some(0) => {
            // `find` and `rm -v` count slightly differently on odd names.
            let total = item.total.load(Ordering::Relaxed).max(removed);
            item.total.store(total, Ordering::Relaxed);
            item.done.store(total, Ordering::Relaxed);
            Some(Ok(TransferState::Done))
        }
        // Nothing removed: an `rm` without `-v`, or nothing removable — the
        // walk either works or reports each entry's own reason.
        _ if removed == 0 => None,
        status => {
            item.done
                .store(item.total.load(Ordering::Relaxed), Ordering::Relaxed);
            let status = status.map_or_else(|| "no status".into(), |code| code.to_string());
            Some(Err(AppError::RemoteFs(format!(
                "{path}: remote rm exited with {status} — {}",
                first_lines(&removal.stderr)
            ))))
        }
    }
}

/// Run `command`, adding one to `counter` per stdout line.
async fn exec_counting(
    ssh: &SshSession,
    command: &str,
    counter: &AtomicU64,
    control: &mut watch::Receiver<Control>,
) -> AppResult<Exec> {
    let channel = {
        let handle = ssh.handle.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|e| AppError::RemoteFs(format!("rm channel: {e}")))?
    };
    channel
        .exec(true, command)
        .await
        .map_err(|e| AppError::RemoteFs(format!("rm exec: {e}")))?;
    let (mut read, write) = channel.split();
    let mut result = Exec {
        status: None,
        stderr: String::new(),
        cancelled: false,
    };
    loop {
        let current = *control.borrow_and_update();
        match current {
            Control::Cancel => {
                result.cancelled = true;
                break;
            }
            // Not reading while paused eventually stalls the remote process.
            Control::Pause => {
                if control.changed().await.is_err() {
                    break;
                }
                continue;
            }
            Control::Run => {}
        }
        let message = tokio::select! {
            changed = control.changed() => {
                if changed.is_err() {
                    break;
                }
                continue;
            }
            message = read.wait() => message,
        };
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } => {
                let lines = data.iter().filter(|byte| **byte == b'\n').count();
                counter.fetch_add(lines as u64, Ordering::Relaxed);
            }
            ChannelMsg::ExtendedData { data, .. } if result.stderr.len() < 4096 => {
                result.stderr.push_str(&String::from_utf8_lossy(&data));
            }
            ChannelMsg::ExitStatus { exit_status } => result.status = Some(exit_status),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    let _ = write.close().await;
    Ok(result)
}

fn first_lines(stderr: &str) -> String {
    const SHOWN: usize = 3;
    let lines: Vec<&str> = stderr.lines().filter(|line| !line.is_empty()).collect();
    let mut text = lines
        .iter()
        .take(SHOWN)
        .copied()
        .collect::<Vec<_>>()
        .join("; ");
    if lines.len() > SHOWN {
        text.push_str(&format!("; …and {} more", lines.len() - SHOWN));
    }
    if text.is_empty() {
        text.push_str("no error output");
    }
    text
}
