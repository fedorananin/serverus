use std::sync::atomic::Ordering;
use std::sync::Arc;

use serverus_domain::transfers::{
    ConflictKind as DomainConflictKind, TransferEvent as DomainTransferEvent,
    TransferStateKind as DomainTransferStateKind,
};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::join_remote;
use crate::vault::model::ConflictPolicy;

use super::{
    domain_conflict_decision, local_target_exists, open_local_download, ConflictAction,
    ServerQueue, TransferItem, TransferKind, TransferManager, TransferState,
};

fn renamed_variant(name: &str, attempt: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => {
            format!("{stem} ({attempt}).{extension}")
        }
        _ => format!("{name} ({attempt})"),
    }
}

pub(super) async fn run_single(
    _manager: &Arc<TransferManager>,
    _queue: &Arc<ServerQueue>,
    item: &Arc<TransferItem>,
) -> AppResult<TransferState> {
    let fs = item.fs.as_ref();
    let settings = &item.settings;
    let mut local_target = item.local_target.clone();
    // Failed-name placeholders intentionally have no writable capability.
    // A manual retry must fail explicitly instead of probing the display path.
    if item.kind == TransferKind::Download && local_target.is_none() && item.tar.is_none() {
        return Err(AppError::Transfer(
            "this entry's remote name cannot be stored locally".into(),
        ));
    }
    let resuming = item.resume.swap(false, Ordering::Relaxed);

    let mut offset = 0_u64;
    if resuming {
        let total = item.total.load(Ordering::Relaxed);
        offset = match item.kind {
            TransferKind::Download => local_target
                .as_ref()
                .and_then(|target| target.root.symlink_metadata(&target.relative).ok())
                .filter(|metadata| metadata.is_file() && !metadata.is_symlink())
                .map(|metadata| metadata.len())
                .unwrap_or(0),
            TransferKind::Upload if !fs.supports_write_resume() => 0,
            TransferKind::Upload => fs
                .stat(&item.remote_path)
                .await
                .map_or(0, |entry| entry.size),
        };
        if offset >= total {
            offset = 0;
        }
        item.done.store(offset, Ordering::Relaxed);
    }

    let target_exists = !resuming
        && match item.kind {
            TransferKind::Upload => fs.exists(&item.remote_path).await?,
            TransferKind::Download => local_target
                .as_ref()
                .map(local_target_exists)
                .unwrap_or_else(|| item.local_path.exists()),
        };
    let mut remote_path = item.remote_path.clone();
    let mut local_path = item.local_path.clone();

    if target_exists {
        let batch_action = item.batch.policy_override();
        let policy = settings.conflict_policy;
        let mut conflict_receiver = None;
        if batch_action.is_none() && policy == ConflictPolicy::Ask {
            let (sender, receiver) = oneshot::channel();
            *item.resolver.lock().unwrap() = Some(sender);
            conflict_receiver = Some(receiver);
        }
        match item.apply_and_dispatch(
            DomainTransferEvent::ConflictDetected(DomainConflictKind::DestinationExists),
            None,
            None,
        ) {
            Ok(_) => {}
            Err(_) if item.domain_state_kind() == DomainTransferStateKind::Cancelling => {
                item.resolver.lock().unwrap().take();
                return Ok(TransferState::Cancelled);
            }
            Err(error) => {
                item.resolver.lock().unwrap().take();
                return Err(AppError::Transfer(format!(
                    "transfer lifecycle conflict detection: {error}"
                )));
            }
        }

        let action = if let Some(action) = batch_action.or_else(|| item.batch.policy_override()) {
            action
        } else {
            match policy {
                ConflictPolicy::Overwrite => ConflictAction::Overwrite,
                ConflictPolicy::Skip => ConflictAction::Skip,
                ConflictPolicy::Rename => ConflictAction::Rename,
                ConflictPolicy::Ask => conflict_receiver
                    .expect("ask policy created a resolver")
                    .await
                    .unwrap_or(ConflictAction::Skip),
            }
        };

        if item.domain_state_kind() == DomainTransferStateKind::Cancelling {
            return Ok(TransferState::Cancelled);
        }
        if item.domain_state_kind() == DomainTransferStateKind::WaitingForConflict {
            item.apply_and_dispatch(
                DomainTransferEvent::ConflictResolved(domain_conflict_decision(action)),
                None,
                Some(action),
            )
            .map_err(|error| {
                AppError::Transfer(format!("transfer lifecycle conflict resolution: {error}"))
            })?;
        }
        match action {
            ConflictAction::Overwrite => {}
            ConflictAction::Skip => return Ok(TransferState::Skipped),
            ConflictAction::Rename => {
                for attempt in 1_u32.. {
                    match item.kind {
                        TransferKind::Upload => {
                            let directory = crate::session::remote_fs::parent_remote(&remote_path);
                            let candidate =
                                join_remote(&directory, &renamed_variant(&item.name, attempt));
                            if !fs.exists(&candidate).await? {
                                remote_path = candidate;
                                break;
                            }
                        }
                        TransferKind::Download => {
                            let renamed = renamed_variant(&item.name, attempt);
                            let candidate = local_path.with_file_name(&renamed);
                            let exists = if let Some(target) = &mut local_target {
                                target.relative = target.relative.with_file_name(renamed);
                                local_target_exists(target)
                            } else {
                                candidate.exists()
                            };
                            if !exists {
                                local_path = candidate;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut control = item.control.subscribe();
    let mtime;
    match item.kind {
        TransferKind::Upload => {
            let metadata = tokio::fs::metadata(&local_path)
                .await
                .map_err(|error| AppError::Transfer(error.to_string()))?;
            mtime = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64);
            let mut source = tokio::fs::File::open(&local_path)
                .await
                .map_err(|error| AppError::Transfer(error.to_string()))?;
            if offset > 0 {
                source
                    .seek(std::io::SeekFrom::Start(offset))
                    .await
                    .map_err(|error| AppError::Transfer(error.to_string()))?;
            }
            item.mark_remote_partial(remote_path.clone());
            let mut destination = fs.open_write(&remote_path, offset).await?;
            if super::copy_loop(item, &mut control, &mut source, &mut destination).await? {
                return Ok(TransferState::Cancelled);
            }
            destination
                .shutdown()
                .await
                .map_err(|error| AppError::Transfer(format!("finalize: {error}")))?;
            if settings.preserve_mtime {
                if let Some(time) = mtime {
                    let _ = fs.set_mtime(&remote_path, time).await;
                }
            }
        }
        TransferKind::Download => {
            let entry = fs.stat(&remote_path).await?;
            mtime = entry.mtime;
            let mut source = fs.open_read(&remote_path, offset).await?;
            let target = local_target.as_ref().ok_or_else(|| {
                AppError::Transfer("download is missing its local capability".into())
            })?;
            item.mark_local_partial(target.clone());
            let mut destination = open_local_download(target, offset)?;
            if super::copy_loop(item, &mut control, &mut source, &mut destination).await? {
                drop(destination);
                return Ok(TransferState::Cancelled);
            }
            destination
                .flush()
                .await
                .map_err(|error| AppError::Transfer(format!("finalize: {error}")))?;
            if settings.preserve_mtime {
                if let Some(time) = mtime {
                    let file = destination.into_std().await;
                    let _ = filetime::set_file_handle_times(
                        &file,
                        None,
                        Some(filetime::FileTime::from_unix_time(time, 0)),
                    );
                }
            }
        }
    }
    Ok(TransferState::Done)
}
