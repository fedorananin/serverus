use std::sync::atomic::Ordering;
#[cfg(feature = "scenario-tests")]
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::RemoteFs;

use super::{Control, TransferItem, CHUNK};

pub(super) async fn copy_loop(
    item: &TransferItem,
    control: &mut watch::Receiver<Control>,
    source: &mut (impl tokio::io::AsyncRead + Unpin + ?Sized),
    destination: &mut (impl tokio::io::AsyncWrite + Unpin + ?Sized),
) -> AppResult<bool> {
    let mut buffer = vec![0_u8; CHUNK];
    loop {
        let current = *control.borrow();
        match current {
            Control::Cancel => return Ok(true),
            Control::Pause => {
                if control.changed().await.is_err() {
                    return Ok(true);
                }
                continue;
            }
            Control::Run => {}
        }
        let count = source
            .read(&mut buffer)
            .await
            .map_err(|error| AppError::Transfer(format!("read: {error}")))?;
        if count == 0 {
            return Ok(false);
        }
        destination
            .write_all(&buffer[..count])
            .await
            .map_err(|error| AppError::Transfer(format!("write: {error}")))?;
        item.done.fetch_add(count as u64, Ordering::Relaxed);
        #[cfg(feature = "scenario-tests")]
        if let Some(delay) = scenario_chunk_delay(&item.name) {
            tokio::time::sleep(delay).await;
        }
    }
}

#[cfg(feature = "scenario-tests")]
pub(super) fn scenario_chunk_delay(name: &str) -> Option<Duration> {
    (name == "cleanup-slow.bin").then_some(Duration::from_millis(200))
}

pub(super) async fn local_tree_size(root: &std::path::Path) -> u64 {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&directory).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    total
}

pub(super) async fn remote_tree_size(fs: &dyn RemoteFs, root: &str) -> AppResult<u64> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_string()];
    while let Some(directory) = pending.pop() {
        for entry in fs.list(&directory).await? {
            if entry.is_dir && !entry.is_symlink {
                pending.push(entry.path);
            } else {
                total += entry.size;
            }
        }
    }
    Ok(total)
}
