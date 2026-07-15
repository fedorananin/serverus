use std::path::PathBuf;
use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;
use serverus_domain::transfers::{
    FailureKind as DomainFailureKind, TransferEvent as DomainTransferEvent,
};

use crate::error::AppResult;
use crate::session::remote_fs::{RemoteEntry, RemoteFs};
use crate::session::ssh::SshSession;

use super::tar_stream;
use super::{
    ensure_download_directory, open_download_root, remote_tree_size, safe_local_component,
    AdmissionToken, DownloadRequest, LocalDownloadTarget, ProgressSink, TransferBatch,
    TransferKind, TransferManager,
};

impl TransferManager {
    /// Enqueue a remote file or directory tree into `local_dir`.
    pub async fn enqueue_download(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        app: &Arc<dyn ProgressSink>,
        request: DownloadRequest<'_>,
    ) -> AppResult<()> {
        let session_id = request.session_id;
        let batch = TransferBatch::new();
        self.run_admitted(context_id, session_id, |admission| {
            self.enqueue_download_inner(admission, app, request, None, batch)
        })
        .await
    }

    pub async fn enqueue_download_accelerated(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        app: &Arc<dyn ProgressSink>,
        request: DownloadRequest<'_>,
        tar_ssh: Option<Arc<SshSession>>,
    ) -> AppResult<()> {
        let session_id = request.session_id;
        let batch = TransferBatch::new();
        self.run_admitted(context_id, session_id, |admission| {
            self.enqueue_download_inner(admission, app, request, tar_ssh, batch)
        })
        .await
    }

    /// Keep an unsafe or colliding child visible as one failed queue item
    /// without aborting the rest of its recursive-download batch.
    #[allow(clippy::too_many_arguments)]
    fn add_failed_download(
        self: &Arc<Self>,
        admission: AdmissionToken,
        batch: Arc<TransferBatch>,
        app: &Arc<dyn ProgressSink>,
        session_id: &str,
        child: &RemoteEntry,
        local_dir: PathBuf,
        fs: &Arc<dyn RemoteFs>,
        settings: &crate::vault::model::TransferSettings,
        reason: &str,
    ) -> bool {
        let Some(item) = self.add_item(
            admission,
            batch,
            session_id,
            TransferKind::Download,
            local_dir,
            child.path.clone(),
            child.size,
            fs.clone(),
            settings.clone(),
            None,
            None,
        ) else {
            return false;
        };
        item.apply_and_dispatch(DomainTransferEvent::StartRequested, None, None)
            .expect("a newly queued failed-download placeholder can start");
        item.apply_and_dispatch(
            DomainTransferEvent::PermanentFailure(DomainFailureKind::LocalIo),
            Some(reason.to_string()),
            None,
        )
        .expect("a started failed-download placeholder can fail permanently");
        self.ensure_emitter(app.clone());
        true
    }

    pub(super) async fn enqueue_download_inner(
        self: &Arc<Self>,
        admission: AdmissionToken,
        app: &Arc<dyn ProgressSink>,
        request: DownloadRequest<'_>,
        tar_ssh: Option<Arc<SshSession>>,
        batch: Arc<TransferBatch>,
    ) -> AppResult<()> {
        let DownloadRequest {
            fs,
            session_id,
            remote_path,
            local_dir,
            settings,
        } = request;
        let entry = fs.stat(remote_path).await?;
        let local_base = crate::local_fs::expand(local_dir);
        let local_name = safe_local_component(&entry.name)?;
        let destination_root = open_download_root(&local_base)?;
        let top_relative = PathBuf::from(&local_name);

        if !entry.is_dir {
            let local_target = LocalDownloadTarget {
                root: destination_root,
                relative: top_relative.clone(),
            };
            let Some(item) = self.add_item(
                admission,
                batch,
                session_id,
                TransferKind::Download,
                local_base.join(&top_relative),
                remote_path.to_string(),
                entry.size,
                fs,
                settings,
                Some(local_target),
                None,
            ) else {
                return Ok(());
            };
            self.spawn_worker(app, item);
            return Ok(());
        }

        if settings.tar_acceleration {
            if let Some(ssh) = tar_ssh {
                let total = remote_tree_size(fs.as_ref(), remote_path).await?;
                let Some(item) = self.add_item(
                    admission,
                    batch,
                    session_id,
                    TransferKind::Download,
                    local_base.join(&local_name),
                    remote_path.to_string(),
                    total,
                    fs,
                    settings,
                    None,
                    Some(tar_stream::TarJob { ssh }),
                ) else {
                    return Ok(());
                };
                self.spawn_worker(app, item);
                return Ok(());
            }
        }

        ensure_download_directory(&destination_root, &top_relative)?;
        let mut pending = vec![(remote_path.to_string(), top_relative)];
        while let Some((remote_dir, local_relative)) = pending.pop() {
            let mut used_names = std::collections::HashSet::new();
            for child in fs.list(&remote_dir).await? {
                let local_name = match safe_local_component(&child.name) {
                    Ok(name) => name,
                    Err(error) => {
                        if !self.add_failed_download(
                            admission.clone(),
                            batch.clone(),
                            app,
                            session_id,
                            &child,
                            local_base.join(&local_relative),
                            &fs,
                            &settings,
                            &error.to_string(),
                        ) {
                            return Ok(());
                        }
                        continue;
                    }
                };
                if !used_names.insert(local_name.clone()) {
                    let reason =
                        format!("{local_name}: another entry already maps to this local name");
                    if !self.add_failed_download(
                        admission.clone(),
                        batch.clone(),
                        app,
                        session_id,
                        &child,
                        local_base.join(&local_relative),
                        &fs,
                        &settings,
                        &reason,
                    ) {
                        return Ok(());
                    }
                    continue;
                }
                let local_child = local_relative.join(&local_name);
                if child.is_dir && !child.is_symlink {
                    ensure_download_directory(&destination_root, &local_child)?;
                    pending.push((child.path, local_child));
                } else {
                    let local_target = LocalDownloadTarget {
                        root: destination_root.clone(),
                        relative: local_child.clone(),
                    };
                    let Some(item) = self.add_item(
                        admission.clone(),
                        batch.clone(),
                        session_id,
                        TransferKind::Download,
                        local_base.join(&local_child),
                        child.path,
                        child.size,
                        fs.clone(),
                        settings.clone(),
                        Some(local_target),
                        None,
                    ) else {
                        return Ok(());
                    };
                    self.spawn_worker(app, item);
                }
            }
        }
        Ok(())
    }
}
