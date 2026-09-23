//! Recursive delete and recursive chmod as transfer-queue items, so they
//! get the same per-session panel as transfers: live progress, pause,
//! cancel, retry and a finished entry that says the work is really over.
//! The tree walk itself is protocol-neutral — see
//! [`crate::session::remote_fs::run_tree_action`].

mod shell;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;
use tokio::sync::{watch, Mutex};

use crate::error::AppResult;
use crate::session::remote_fs::{
    run_tree_action, Checkpoint, RemoteFs, TreeAction, TreeOutcome, TreeProgress,
};
use crate::session::ssh::SshSession;
use crate::vault::model::TransferSettings;

use super::{
    Control, ProgressSink, TransferBatch, TransferItem, TransferKind, TransferManager,
    TransferState,
};

/// What a tree item does, plus its live phase flags.
pub struct TreeJob {
    pub(super) action: TreeAction,
    pub(super) is_dir: bool,
    /// SSH session for server-side `rm -rf` (directory deletes only).
    pub(super) shell: Option<Arc<SshSession>>,
    pub(super) scanning: AtomicBool,
    pub(super) accelerated: AtomicBool,
}

/// One selected entry to delete or chmod recursively.
pub struct TreeRequest<'a> {
    pub fs: Arc<dyn RemoteFs>,
    pub session_id: &'a str,
    pub path: &'a str,
    /// A real directory — never a symlink to one, which is removed as a link.
    pub is_dir: bool,
    pub action: TreeAction,
    pub settings: TransferSettings,
    /// Present when the server can run `rm` (see `SessionEntry::rm_ssh`).
    pub shell: Option<Arc<SshSession>>,
}

impl TransferManager {
    /// Queue one item per selected entry; the whole selection shares a batch.
    pub async fn enqueue_tree_ops(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        app: &Arc<dyn ProgressSink>,
        requests: Vec<TreeRequest<'_>>,
    ) -> AppResult<()> {
        let batch = TransferBatch::new();
        for request in requests {
            let session_id = request.session_id;
            let batch = batch.clone();
            self.run_admitted(context_id, session_id, |admission| async move {
                let kind = match request.action {
                    TreeAction::Delete => TransferKind::Delete,
                    TreeAction::Chmod { .. } => TransferKind::Chmod,
                };
                let shell = request
                    .shell
                    .filter(|_| request.action == TreeAction::Delete && request.is_dir);
                let job = TreeJob {
                    action: request.action,
                    is_dir: request.is_dir,
                    accelerated: AtomicBool::new(shell.is_some()),
                    shell,
                    scanning: AtomicBool::new(false),
                };
                if let Some(item) = self.add_item(
                    admission,
                    batch,
                    session_id,
                    kind,
                    PathBuf::new(),
                    request.path.to_string(),
                    0,
                    request.fs,
                    request.settings,
                    None,
                    None,
                    Some(job),
                ) {
                    self.spawn_worker(app, item);
                }
                Ok(())
            })
            .await?;
        }
        Ok(())
    }
}

/// Worker body for a tree item.
pub(super) async fn run(item: &Arc<TransferItem>, job: &TreeJob) -> AppResult<TransferState> {
    if let Some(ssh) = &job.shell {
        job.accelerated.store(true, Ordering::Relaxed);
        if let Some(result) = shell::delete(item, job, ssh).await {
            return result;
        }
        // `rm`/`find` unusable on this server: nothing was touched, so the
        // portable walk below starts from a clean slate.
        job.accelerated.store(false, Ordering::Relaxed);
    }
    let checkpoint = ItemCheckpoint(Mutex::new(item.control.subscribe()));
    let progress = TreeProgress {
        total: &item.total,
        done: &item.done,
        scanning: &job.scanning,
    };
    let outcome = run_tree_action(
        item.fs.as_ref(),
        &item.remote_path,
        job.is_dir,
        job.action,
        progress,
        &checkpoint,
    )
    .await?;
    Ok(match outcome {
        TreeOutcome::Completed => TransferState::Done,
        TreeOutcome::Cancelled => TransferState::Cancelled,
    })
}

/// Pause/cancel through the item's control channel.
struct ItemCheckpoint(Mutex<watch::Receiver<Control>>);

#[async_trait::async_trait]
impl Checkpoint for ItemCheckpoint {
    async fn proceed(&self) -> bool {
        let mut control = self.0.lock().await;
        loop {
            let current = *control.borrow_and_update();
            match current {
                Control::Run => return true,
                Control::Cancel => return false,
                Control::Pause => {
                    if control.changed().await.is_err() {
                        return false;
                    }
                }
            }
        }
    }
}
