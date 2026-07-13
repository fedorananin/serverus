//! Transfer queue (SPEC §6): parallel per-server workers, progress events,
//! pause/resume/cancel, conflict handling. Works through the RemoteFs trait —
//! it does not know whether a session is SFTP or FTP.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{oneshot, watch, Semaphore};

pub mod tar_stream;

use crate::error::{AppError, AppResult};
use crate::events::TransferProgressEvent;
use crate::session::remote_fs::{join_remote, RemoteFs};
use crate::vault::model::{ConflictPolicy, TransferSettings};

/// Where progress events go. The real app emits Tauri events; tests use a
/// channel. Keeps the queue engine testable without a running Tauri app.
pub trait ProgressSink: Send + Sync + 'static {
    fn emit(&self, event: TransferProgressEvent);
}

impl ProgressSink for tauri::AppHandle {
    fn emit(&self, event: TransferProgressEvent) {
        let _ = tauri_specta::Event::emit(&event, self);
    }
}

const CHUNK: usize = 128 * 1024;
/// Progress snapshot cap — the UI shows the head of the queue plus totals.
const SNAPSHOT_LIMIT: usize = 200;
/// Automatic retries of a failed transfer (transient network hiccups)
/// before the item stays in Error for the user to retry manually.
const AUTO_RETRIES: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    Upload,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Queued,
    Running,
    Paused,
    /// Waiting for the user's overwrite/skip/rename decision.
    Conflict,
    Done,
    Skipped,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictAction {
    Overwrite,
    Skip,
    Rename,
}

#[derive(Clone, Copy, PartialEq)]
enum Control {
    Run,
    Pause,
    Cancel,
}

pub struct TransferItem {
    pub id: String,
    pub session_id: String,
    pub kind: TransferKind,
    pub local_path: PathBuf,
    pub remote_path: String,
    pub name: String,
    total: AtomicU64,
    done: AtomicU64,
    state: Mutex<(TransferState, Option<String>)>,
    control: watch::Sender<Control>,
    resolver: Mutex<Option<oneshot::Sender<ConflictAction>>>,
    /// For speed calculation in the progress emitter.
    last_done: AtomicU64,
    speed_bps: AtomicU64,
    /// Kept for retry: the backend and settings this item runs with.
    fs: Arc<dyn RemoteFs>,
    settings: TransferSettings,
    /// Retry of a partial transfer — resume from the target's current size
    /// instead of starting over (SPEC §6.1).
    resume: std::sync::atomic::AtomicBool,
    /// Failed runs so far — drives automatic retry with backoff.
    attempts: std::sync::atomic::AtomicU32,
    /// Directory streamed through tar instead of per-file copies (SPEC §6.2).
    tar: Option<tar_stream::TarJob>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TransferSnapshot {
    pub id: String,
    pub session_id: String,
    pub kind: TransferKind,
    pub state: TransferState,
    pub error: Option<String>,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub accelerated: bool,
    #[specta(type = specta_typescript::Number)]
    pub done: u64,
    #[specta(type = specta_typescript::Number)]
    pub total: u64,
    #[specta(type = specta_typescript::Number)]
    pub speed_bps: u64,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TransferSummary {
    pub queued: u32,
    pub running: u32,
    pub done: u32,
    pub failed: u32,
    pub total_items: u32,
}

impl TransferItem {
    fn snapshot(&self) -> TransferSnapshot {
        let (state, error) = self.state.lock().unwrap().clone();
        TransferSnapshot {
            id: self.id.clone(),
            session_id: self.session_id.clone(),
            kind: self.kind,
            state,
            error,
            name: self.name.clone(),
            local_path: self.local_path.to_string_lossy().into_owned(),
            remote_path: self.remote_path.clone(),
            accelerated: self.tar.is_some(),
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            speed_bps: self.speed_bps.load(Ordering::Relaxed),
        }
    }

    fn set_state(&self, state: TransferState, error: Option<String>) {
        *self.state.lock().unwrap() = (state, error);
    }

    fn state(&self) -> TransferState {
        self.state.lock().unwrap().0.clone()
    }
}

struct ServerQueue {
    semaphore: Arc<Semaphore>,
    /// "Apply to all" conflict decision for the rest of the batch.
    policy_override: Mutex<Option<ConflictPolicy>>,
}

#[derive(Default)]
pub struct TransferManager {
    items: Mutex<Vec<Arc<TransferItem>>>,
    queues: Mutex<HashMap<String, Arc<ServerQueue>>>,
    emitter_running: std::sync::atomic::AtomicBool,
}

impl TransferManager {
    fn queue_for(&self, session_id: &str, parallel: usize) -> Arc<ServerQueue> {
        self.queues
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_insert_with(|| {
                Arc::new(ServerQueue {
                    semaphore: Arc::new(Semaphore::new(parallel.max(1))),
                    policy_override: Mutex::new(None),
                })
            })
            .clone()
    }

    fn find(&self, id: &str) -> Option<Arc<TransferItem>> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .cloned()
    }

    pub fn snapshot(&self) -> (Vec<TransferSnapshot>, TransferSummary) {
        let items = self.items.lock().unwrap();
        let mut summary = TransferSummary {
            queued: 0,
            running: 0,
            done: 0,
            failed: 0,
            total_items: items.len() as u32,
        };
        for item in items.iter() {
            match item.state() {
                TransferState::Queued => summary.queued += 1,
                TransferState::Running | TransferState::Paused | TransferState::Conflict => {
                    summary.running += 1
                }
                TransferState::Done | TransferState::Skipped => summary.done += 1,
                TransferState::Error | TransferState::Cancelled => summary.failed += 1,
            }
        }
        // Show active items first, then queued, then finished — capped.
        let mut list: Vec<&Arc<TransferItem>> = items.iter().collect();
        list.sort_by_key(|i| match i.state() {
            TransferState::Running | TransferState::Paused | TransferState::Conflict => 0,
            TransferState::Queued => 1,
            TransferState::Error => 2,
            _ => 3,
        });
        let snapshots = list
            .into_iter()
            .take(SNAPSHOT_LIMIT)
            .map(|i| i.snapshot())
            .collect();
        (snapshots, summary)
    }

    /// Drop all transfers belonging to a session (its tab was closed /
    /// disconnected). The queue and history live only for the lifetime of
    /// the connection (SPEC §6.1).
    pub fn clear_session(&self, session_id: &str) {
        let ids: Vec<String> = self
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.session_id == session_id)
            .map(|i| i.id.clone())
            .collect();
        for id in &ids {
            self.cancel(id); // stop any running/queued worker first
        }
        self.items
            .lock()
            .unwrap()
            .retain(|i| i.session_id != session_id);
        self.queues.lock().unwrap().remove(session_id);
    }

    /// Remove finished/cancelled/errored items from the list.
    pub fn clear_finished(&self) {
        self.items.lock().unwrap().retain(|i| {
            !matches!(
                i.state(),
                TransferState::Done
                    | TransferState::Skipped
                    | TransferState::Cancelled
                    | TransferState::Error
            )
        });
    }

    pub fn pause(&self, id: &str) {
        if let Some(item) = self.find(id) {
            if item.state() == TransferState::Running {
                item.set_state(TransferState::Paused, None);
                let _ = item.control.send(Control::Pause);
            }
        }
    }

    pub fn resume(&self, id: &str) {
        if let Some(item) = self.find(id) {
            if item.state() == TransferState::Paused {
                item.set_state(TransferState::Running, None);
                let _ = item.control.send(Control::Run);
            }
        }
    }

    pub fn cancel(&self, id: &str) {
        if let Some(item) = self.find(id) {
            match item.state() {
                TransferState::Queued => item.set_state(TransferState::Cancelled, None),
                TransferState::Conflict => {
                    item.set_state(TransferState::Cancelled, None);
                    // Unblock the waiting worker.
                    if let Some(tx) = item.resolver.lock().unwrap().take() {
                        let _ = tx.send(ConflictAction::Skip);
                    }
                }
                _ => {
                    let _ = item.control.send(Control::Cancel);
                }
            }
        }
    }

    pub fn cancel_all(&self) {
        let items: Vec<_> = self.items.lock().unwrap().clone();
        for item in items {
            self.cancel(&item.id);
        }
    }

    pub fn pause_all(&self) {
        let items: Vec<_> = self.items.lock().unwrap().clone();
        for item in items {
            self.pause(&item.id);
        }
    }

    pub fn resume_all(&self) {
        let items: Vec<_> = self.items.lock().unwrap().clone();
        for item in items {
            self.resume(&item.id);
        }
    }

    pub fn resolve_conflict(
        &self,
        session_id: &str,
        id: &str,
        action: ConflictAction,
        apply_to_all: bool,
    ) {
        if apply_to_all {
            if let Some(queue) = self.queues.lock().unwrap().get(session_id) {
                *queue.policy_override.lock().unwrap() = Some(match action {
                    ConflictAction::Overwrite => ConflictPolicy::Overwrite,
                    ConflictAction::Skip => ConflictPolicy::Skip,
                    ConflictAction::Rename => ConflictPolicy::Rename,
                });
            }
        }
        if let Some(item) = self.find(id) {
            if let Some(tx) = item.resolver.lock().unwrap().take() {
                let _ = tx.send(action);
            }
        }
    }

    /// Periodic progress event (~4 Hz) while transfers are active.
    fn ensure_emitter(self: &Arc<Self>, app: Arc<dyn ProgressSink>) {
        if self
            .emitter_running
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let manager = self.clone();
        tokio::spawn(async move {
            let mut idle_rounds = 0u32;
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                // Update speeds.
                {
                    let items = manager.items.lock().unwrap();
                    for item in items.iter() {
                        let done = item.done.load(Ordering::Relaxed);
                        let last = item.last_done.swap(done, Ordering::Relaxed);
                        let delta = done.saturating_sub(last);
                        // 250 ms window → bytes/s = delta * 4, smoothed.
                        let prev = item.speed_bps.load(Ordering::Relaxed);
                        let speed = if item.state() == TransferState::Running {
                            (prev / 2).saturating_add(delta * 2)
                        } else {
                            0
                        };
                        item.speed_bps.store(speed, Ordering::Relaxed);
                    }
                }
                let (items, summary) = manager.snapshot();
                let active = summary.queued + summary.running;
                app.emit(TransferProgressEvent {
                    items,
                    summary: summary.clone(),
                });
                if active == 0 {
                    idle_rounds += 1;
                    if idle_rounds > 8 {
                        manager
                            .emitter_running
                            .store(false, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                } else {
                    idle_rounds = 0;
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn add_item(
        &self,
        session_id: &str,
        kind: TransferKind,
        local_path: PathBuf,
        remote_path: String,
        total: u64,
        fs: Arc<dyn RemoteFs>,
        settings: TransferSettings,
        tar: Option<tar_stream::TarJob>,
    ) -> Arc<TransferItem> {
        let name = match kind {
            TransferKind::Upload => local_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            TransferKind::Download => remote_path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string(),
        };
        let (control, _) = watch::channel(Control::Run);
        let item = Arc::new(TransferItem {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            kind,
            local_path,
            remote_path,
            name,
            total: AtomicU64::new(total),
            done: AtomicU64::new(0),
            state: Mutex::new((TransferState::Queued, None)),
            control,
            resolver: Mutex::new(None),
            last_done: AtomicU64::new(0),
            speed_bps: AtomicU64::new(0),
            fs,
            settings,
            resume: std::sync::atomic::AtomicBool::new(false),
            attempts: std::sync::atomic::AtomicU32::new(0),
            tar,
        });
        self.items.lock().unwrap().push(item.clone());
        item
    }

    fn spawn_worker(self: &Arc<Self>, app: &Arc<dyn ProgressSink>, item: Arc<TransferItem>) {
        let queue = self.queue_for(
            &item.session_id,
            item.settings.max_parallel_per_server as usize,
        );
        let manager = self.clone();
        let app = app.clone();
        self.ensure_emitter(app.clone());
        tokio::spawn(async move {
            let _permit = queue.semaphore.clone().acquire_owned().await;
            if item.state() != TransferState::Queued {
                return; // cancelled while queued
            }
            item.set_state(TransferState::Running, None);
            let result = match &item.tar {
                Some(job) => tar_stream::run(&item, job).await,
                None => run_single(&manager, &queue, &item).await,
            };
            match result {
                Ok(final_state) => item.set_state(final_state, None),
                Err(e) => {
                    item.set_state(TransferState::Error, Some(e.to_string()));
                    manager.schedule_auto_retry(&app, item);
                }
            }
        });
    }

    /// Requeue a freshly failed item after a short backoff — transient
    /// hiccups (SFTP timeout, dropped data connection) shouldn't need a
    /// manual retry. Tar jobs are excluded: their retry path falls back to
    /// plain per-file transfers, which is the user's call.
    fn schedule_auto_retry(self: &Arc<Self>, app: &Arc<dyn ProgressSink>, item: Arc<TransferItem>) {
        let attempt = item.attempts.fetch_add(1, Ordering::Relaxed) + 1;
        if attempt > AUTO_RETRIES || item.tar.is_some() {
            return;
        }
        let manager = self.clone();
        let app = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1000 * u64::from(attempt))).await;
            // Skip when the user already cancelled, cleared or retried it.
            if manager.find(&item.id).is_none() || item.state() != TransferState::Error {
                return;
            }
            item.done.store(0, Ordering::Relaxed);
            item.resume
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = item.control.send(Control::Run);
            item.set_state(TransferState::Queued, None);
            manager.spawn_worker(&app, item);
        });
    }

    /// Retry a failed/cancelled item, resuming partial files where possible.
    /// A failed tar-stream item is re-enqueued as a plain per-file transfer
    /// (SPEC §6.2 transparent fallback).
    pub async fn retry(self: &Arc<Self>, app: &Arc<dyn ProgressSink>, id: &str) -> AppResult<()> {
        let Some(item) = self.find(id) else {
            return Ok(());
        };
        if !matches!(
            item.state(),
            TransferState::Error | TransferState::Cancelled
        ) {
            return Ok(());
        }
        if item.tar.is_some() {
            // Fall back to the plain queue.
            item.set_state(TransferState::Cancelled, None);
            let mut settings = item.settings.clone();
            settings.tar_acceleration = false;
            match item.kind {
                TransferKind::Upload => {
                    let local = item.local_path.to_string_lossy().into_owned();
                    let remote_dir = crate::session::remote_fs::parent_remote(&item.remote_path);
                    self.enqueue_upload_inner(
                        app,
                        item.fs.clone(),
                        &item.session_id,
                        &local,
                        &remote_dir,
                        settings,
                        None,
                    )
                    .await?;
                }
                TransferKind::Download => {
                    let local_dir = item
                        .local_path
                        .parent()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "/".into());
                    self.enqueue_download_inner(
                        app,
                        item.fs.clone(),
                        &item.session_id,
                        &item.remote_path,
                        &local_dir,
                        settings,
                        None,
                    )
                    .await?;
                }
            }
            return Ok(());
        }
        item.done.store(0, Ordering::Relaxed);
        item.resume
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // A manual retry re-arms the automatic ones.
        item.attempts.store(0, Ordering::Relaxed);
        let _ = item.control.send(Control::Run);
        item.set_state(TransferState::Queued, None);
        self.spawn_worker(app, item);
        Ok(())
    }

    /// Enqueue an upload of a local file or directory tree into `remote_dir`.
    /// `tar_ssh` enables tar-stream acceleration for directories (SPEC §6.2).
    pub async fn enqueue_upload(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        fs: Arc<dyn RemoteFs>,
        session_id: &str,
        local_path: &str,
        remote_dir: &str,
        settings: TransferSettings,
    ) -> AppResult<()> {
        self.enqueue_upload_inner(app, fs, session_id, local_path, remote_dir, settings, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_upload_accelerated(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        fs: Arc<dyn RemoteFs>,
        session_id: &str,
        local_path: &str,
        remote_dir: &str,
        settings: TransferSettings,
        tar_ssh: Option<Arc<crate::session::ssh::SshSession>>,
    ) -> AppResult<()> {
        self.enqueue_upload_inner(
            app, fs, session_id, local_path, remote_dir, settings, tar_ssh,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn enqueue_upload_inner(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        fs: Arc<dyn RemoteFs>,
        session_id: &str,
        local_path: &str,
        remote_dir: &str,
        settings: TransferSettings,
        tar_ssh: Option<Arc<crate::session::ssh::SshSession>>,
    ) -> AppResult<()> {
        let root = crate::local_fs::expand(local_path);
        let meta = tokio::fs::metadata(&root)
            .await
            .map_err(|e| AppError::Transfer(format!("{local_path}: {e}")))?;
        let base_name = root
            .file_name()
            .ok_or_else(|| AppError::Transfer("bad local path".into()))?
            .to_string_lossy()
            .into_owned();

        if meta.is_file() {
            let item = self.add_item(
                session_id,
                TransferKind::Upload,
                root.clone(),
                join_remote(remote_dir, &base_name),
                meta.len(),
                fs,
                settings,
                None,
            );
            self.spawn_worker(app, item);
            return Ok(());
        }

        // Accelerated path: one tar stream instead of per-file round-trips.
        if settings.tar_acceleration {
            if let Some(ssh) = tar_ssh {
                let total = local_tree_size(&root).await;
                let item = self.add_item(
                    session_id,
                    TransferKind::Upload,
                    root.clone(),
                    join_remote(remote_dir, &base_name),
                    total,
                    fs,
                    settings,
                    Some(tar_stream::TarJob { ssh }),
                );
                self.spawn_worker(app, item);
                return Ok(());
            }
        }

        // Directory: walk the local tree, create remote dirs, queue files.
        // Recursion is a worklist so deep trees can't overflow the stack.
        let remote_root = join_remote(remote_dir, &base_name);
        let _ = fs.mkdir(&remote_root).await; // may already exist
        let mut pending = vec![(root.clone(), remote_root.clone())];
        while let Some((local_dir, remote_dir)) = pending.pop() {
            let mut read_dir = tokio::fs::read_dir(&local_dir)
                .await
                .map_err(|e| AppError::Transfer(format!("{}: {e}", local_dir.display())))?;
            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(|e| AppError::Transfer(e.to_string()))?
            {
                let path = entry.path();
                let Ok(meta) = entry.metadata().await else {
                    continue;
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                let remote_child = join_remote(&remote_dir, &name);
                if meta.is_dir() {
                    let _ = fs.mkdir(&remote_child).await;
                    pending.push((path, remote_child));
                } else if meta.is_file() {
                    let item = self.add_item(
                        session_id,
                        TransferKind::Upload,
                        path,
                        remote_child,
                        meta.len(),
                        fs.clone(),
                        settings.clone(),
                        None,
                    );
                    self.spawn_worker(app, item);
                }
            }
        }
        Ok(())
    }

    /// Enqueue a download of a remote file or directory tree into `local_dir`.
    pub async fn enqueue_download(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        fs: Arc<dyn RemoteFs>,
        session_id: &str,
        remote_path: &str,
        local_dir: &str,
        settings: TransferSettings,
    ) -> AppResult<()> {
        self.enqueue_download_inner(app, fs, session_id, remote_path, local_dir, settings, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_download_accelerated(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        fs: Arc<dyn RemoteFs>,
        session_id: &str,
        remote_path: &str,
        local_dir: &str,
        settings: TransferSettings,
        tar_ssh: Option<Arc<crate::session::ssh::SshSession>>,
    ) -> AppResult<()> {
        self.enqueue_download_inner(
            app,
            fs,
            session_id,
            remote_path,
            local_dir,
            settings,
            tar_ssh,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn enqueue_download_inner(
        self: &Arc<Self>,
        app: &Arc<dyn ProgressSink>,
        fs: Arc<dyn RemoteFs>,
        session_id: &str,
        remote_path: &str,
        local_dir: &str,
        settings: TransferSettings,
        tar_ssh: Option<Arc<crate::session::ssh::SshSession>>,
    ) -> AppResult<()> {
        let entry = fs.stat(remote_path).await?;
        let local_base = crate::local_fs::expand(local_dir);

        if !entry.is_dir {
            let item = self.add_item(
                session_id,
                TransferKind::Download,
                local_base.join(&entry.name),
                remote_path.to_string(),
                entry.size,
                fs,
                settings,
                None,
            );
            self.spawn_worker(app, item);
            return Ok(());
        }

        // Accelerated path: single remote tar stream.
        if settings.tar_acceleration {
            if let Some(ssh) = tar_ssh {
                let total = remote_tree_size(fs.as_ref(), remote_path).await?;
                let item = self.add_item(
                    session_id,
                    TransferKind::Download,
                    local_base.join(&entry.name),
                    remote_path.to_string(),
                    total,
                    fs,
                    settings,
                    Some(tar_stream::TarJob { ssh }),
                );
                self.spawn_worker(app, item);
                return Ok(());
            }
        }

        // Directory: recursive remote listing — THE case that must always
        // work (SPEC §4.3), shared by SFTP and FTP through RemoteFs.
        let local_root = local_base.join(&entry.name);
        tokio::fs::create_dir_all(&local_root)
            .await
            .map_err(|e| AppError::Transfer(e.to_string()))?;
        let mut pending = vec![(remote_path.to_string(), local_root)];
        while let Some((remote_dir, local_dir)) = pending.pop() {
            for child in fs.list(&remote_dir).await? {
                let local_child = local_dir.join(&child.name);
                if child.is_dir {
                    tokio::fs::create_dir_all(&local_child)
                        .await
                        .map_err(|e| AppError::Transfer(e.to_string()))?;
                    pending.push((child.path, local_child));
                } else {
                    let item = self.add_item(
                        session_id,
                        TransferKind::Download,
                        local_child,
                        child.path,
                        child.size,
                        fs.clone(),
                        settings.clone(),
                        None,
                    );
                    self.spawn_worker(app, item);
                }
            }
        }
        Ok(())
    }
}

/// Pick a free "name (N).ext" style variant for conflict-rename.
fn renamed_variant(name: &str, attempt: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem} ({attempt}).{ext}"),
        _ => format!("{name} ({attempt})"),
    }
}

async fn run_single(
    manager: &Arc<TransferManager>,
    queue: &Arc<ServerQueue>,
    item: &Arc<TransferItem>,
) -> AppResult<TransferState> {
    let fs = item.fs.as_ref();
    let settings = &item.settings;
    let resuming = item
        .resume
        .swap(false, std::sync::atomic::Ordering::Relaxed);

    // --- Resume offset for retried partial transfers (SPEC §6.1) ---
    let mut offset: u64 = 0;
    if resuming {
        let total = item.total.load(Ordering::Relaxed);
        offset = match item.kind {
            TransferKind::Download => tokio::fs::metadata(&item.local_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0),
            // Backends without write-resume (S3) restart the upload instead.
            TransferKind::Upload if !fs.supports_write_resume() => 0,
            TransferKind::Upload => match fs.stat(&item.remote_path).await {
                Ok(entry) => entry.size,
                Err(_) => 0,
            },
        };
        if offset >= total {
            offset = 0; // target complete or bigger than expected — start over
        }
        item.done.store(offset, Ordering::Relaxed);
    }

    // --- Conflict detection (SPEC §6.1) — skipped when resuming ---
    let target_exists = !resuming
        && match item.kind {
            TransferKind::Upload => fs.exists(&item.remote_path).await?,
            TransferKind::Download => item.local_path.exists(),
        };
    let mut remote_path = item.remote_path.clone();
    let mut local_path = item.local_path.clone();

    if target_exists {
        let policy = queue
            .policy_override
            .lock()
            .unwrap()
            .unwrap_or(settings.conflict_policy);
        let action = match policy {
            ConflictPolicy::Overwrite => ConflictAction::Overwrite,
            ConflictPolicy::Skip => ConflictAction::Skip,
            ConflictPolicy::Rename => ConflictAction::Rename,
            ConflictPolicy::Ask => {
                let (tx, rx) = oneshot::channel();
                *item.resolver.lock().unwrap() = Some(tx);
                item.set_state(TransferState::Conflict, None);
                let action = rx.await.unwrap_or(ConflictAction::Skip);
                if item.state() == TransferState::Cancelled {
                    return Ok(TransferState::Cancelled);
                }
                item.set_state(TransferState::Running, None);
                action
            }
        };
        match action {
            ConflictAction::Overwrite => {}
            ConflictAction::Skip => return Ok(TransferState::Skipped),
            ConflictAction::Rename => {
                for attempt in 1u32.. {
                    match item.kind {
                        TransferKind::Upload => {
                            let dir = crate::session::remote_fs::parent_remote(&remote_path);
                            let candidate =
                                join_remote(&dir, &renamed_variant(&item.name, attempt));
                            if !fs.exists(&candidate).await? {
                                remote_path = candidate;
                                break;
                            }
                        }
                        TransferKind::Download => {
                            let candidate =
                                local_path.with_file_name(renamed_variant(&item.name, attempt));
                            if !candidate.exists() {
                                local_path = candidate;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Byte copy with pause/cancel ---
    let mut ctrl = item.control.subscribe();
    let mtime: Option<i64>;

    match item.kind {
        TransferKind::Upload => {
            let src_meta = tokio::fs::metadata(&local_path)
                .await
                .map_err(|e| AppError::Transfer(e.to_string()))?;
            mtime = src_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            let mut src = tokio::fs::File::open(&local_path)
                .await
                .map_err(|e| AppError::Transfer(e.to_string()))?;
            if offset > 0 {
                use tokio::io::AsyncSeekExt;
                src.seek(std::io::SeekFrom::Start(offset))
                    .await
                    .map_err(|e| AppError::Transfer(e.to_string()))?;
            }
            let mut dst = fs.open_write(&remote_path, offset).await?;
            if copy_loop(item, &mut ctrl, &mut src, &mut dst).await? {
                let _ = fs.delete_file(&remote_path).await; // remove partial
                return Ok(TransferState::Cancelled);
            }
            dst.shutdown()
                .await
                .map_err(|e| AppError::Transfer(format!("finalize: {e}")))?;
            if settings.preserve_mtime {
                if let Some(t) = mtime {
                    let _ = fs.set_mtime(&remote_path, t).await;
                }
            }
        }
        TransferKind::Download => {
            let entry = fs.stat(&remote_path).await?;
            mtime = entry.mtime;
            let mut src = fs.open_read(&remote_path, offset).await?;
            let mut dst = if offset > 0 {
                tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(&local_path)
                    .await
                    .map_err(|e| AppError::Transfer(e.to_string()))?
            } else {
                tokio::fs::File::create(&local_path)
                    .await
                    .map_err(|e| AppError::Transfer(e.to_string()))?
            };
            if copy_loop(item, &mut ctrl, &mut src, &mut dst).await? {
                drop(dst);
                let _ = tokio::fs::remove_file(&local_path).await;
                return Ok(TransferState::Cancelled);
            }
            dst.flush()
                .await
                .map_err(|e| AppError::Transfer(format!("finalize: {e}")))?;
            if settings.preserve_mtime {
                if let Some(t) = mtime {
                    let _ = filetime::set_file_mtime(
                        &local_path,
                        filetime::FileTime::from_unix_time(t, 0),
                    );
                }
            }
        }
    }

    let _ = manager; // reserved for retry bookkeeping
    Ok(TransferState::Done)
}

/// Copy bytes src→dst honouring pause/cancel. Returns true when cancelled.
async fn copy_loop(
    item: &TransferItem,
    ctrl: &mut watch::Receiver<Control>,
    src: &mut (impl tokio::io::AsyncRead + Unpin + ?Sized),
    dst: &mut (impl tokio::io::AsyncWrite + Unpin + ?Sized),
) -> AppResult<bool> {
    let mut buf = vec![0u8; CHUNK];
    loop {
        let current = *ctrl.borrow();
        match current {
            Control::Cancel => return Ok(true),
            Control::Pause => {
                if ctrl.changed().await.is_err() {
                    return Ok(true);
                }
                continue;
            }
            Control::Run => {}
        }
        let n = src
            .read(&mut buf)
            .await
            .map_err(|e| AppError::Transfer(format!("read: {e}")))?;
        if n == 0 {
            return Ok(false);
        }
        dst.write_all(&buf[..n])
            .await
            .map_err(|e| AppError::Transfer(format!("write: {e}")))?;
        item.done.fetch_add(n as u64, Ordering::Relaxed);
    }
}

/// Sum of file sizes in a local tree (progress denominator for tar upload).
async fn local_tree_size(root: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(mut read_dir) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if meta.is_dir() {
                pending.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

/// Sum of file sizes in a remote tree (progress denominator for tar download).
async fn remote_tree_size(fs: &dyn RemoteFs, root: &str) -> AppResult<u64> {
    let mut total = 0u64;
    let mut pending = vec![root.to_string()];
    while let Some(dir) = pending.pop() {
        for entry in fs.list(&dir).await? {
            if entry.is_dir && !entry.is_symlink {
                pending.push(entry.path);
            } else {
                total += entry.size;
            }
        }
    }
    Ok(total)
}
