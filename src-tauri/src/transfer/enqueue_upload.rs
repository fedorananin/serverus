use std::sync::Arc;

use serverus_domain::runtime_context::RuntimeContextId;

use super::tar_stream;
use super::{
    local_tree_size, AdmissionToken, ProgressSink, TransferBatch, TransferKind, TransferManager,
    UploadRequest,
};
use crate::error::{AppError, AppResult};
use crate::session::remote_fs::join_remote;
use crate::session::ssh::SshSession;

impl TransferManager {
    /// Enqueue a local file or directory tree into `remote_dir`.
    pub async fn enqueue_upload(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        app: &Arc<dyn ProgressSink>,
        request: UploadRequest<'_>,
    ) -> AppResult<()> {
        let session_id = request.session_id;
        let batch = TransferBatch::new();
        self.run_admitted(context_id, session_id, |admission| {
            self.enqueue_upload_inner(admission, app, request, None, batch)
        })
        .await
    }

    pub async fn enqueue_upload_accelerated(
        self: &Arc<Self>,
        context_id: RuntimeContextId,
        app: &Arc<dyn ProgressSink>,
        request: UploadRequest<'_>,
        tar_ssh: Option<Arc<SshSession>>,
    ) -> AppResult<()> {
        let session_id = request.session_id;
        let batch = TransferBatch::new();
        self.run_admitted(context_id, session_id, |admission| {
            self.enqueue_upload_inner(admission, app, request, tar_ssh, batch)
        })
        .await
    }

    pub(super) async fn enqueue_upload_inner(
        self: &Arc<Self>,
        admission: AdmissionToken,
        app: &Arc<dyn ProgressSink>,
        request: UploadRequest<'_>,
        tar_ssh: Option<Arc<SshSession>>,
        batch: Arc<TransferBatch>,
    ) -> AppResult<()> {
        let UploadRequest {
            fs,
            session_id,
            local_path,
            remote_dir,
            settings,
        } = request;
        let root = crate::local_fs::expand(local_path);
        let metadata = tokio::fs::metadata(&root)
            .await
            .map_err(|error| AppError::Transfer(format!("{local_path}: {error}")))?;
        let base_name = root
            .file_name()
            .ok_or_else(|| AppError::Transfer("bad local path".into()))?
            .to_string_lossy()
            .into_owned();

        if metadata.is_file() {
            let Some(item) = self.add_item(
                admission,
                batch,
                session_id,
                TransferKind::Upload,
                root.clone(),
                join_remote(remote_dir, &base_name),
                metadata.len(),
                fs,
                settings,
                None,
                None,
            ) else {
                return Ok(());
            };
            self.spawn_worker(app, item);
            return Ok(());
        }

        if settings.tar_acceleration {
            if let Some(ssh) = tar_ssh {
                let total = local_tree_size(&root).await;
                let Some(item) = self.add_item(
                    admission,
                    batch,
                    session_id,
                    TransferKind::Upload,
                    root.clone(),
                    join_remote(remote_dir, &base_name),
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

        let remote_root = join_remote(remote_dir, &base_name);
        let _ = fs.mkdir(&remote_root).await;
        let mut pending = vec![(root.clone(), remote_root)];
        while let Some((local_dir, remote_dir)) = pending.pop() {
            let mut read_dir = tokio::fs::read_dir(&local_dir)
                .await
                .map_err(|error| AppError::Transfer(format!("{}: {error}", local_dir.display())))?;
            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(|error| AppError::Transfer(error.to_string()))?
            {
                let path = entry.path();
                let Ok(metadata) = entry.metadata().await else {
                    continue;
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                let remote_child = join_remote(&remote_dir, &name);
                if metadata.is_dir() {
                    let _ = fs.mkdir(&remote_child).await;
                    pending.push((path, remote_child));
                } else if metadata.is_file() {
                    let Some(item) = self.add_item(
                        admission.clone(),
                        batch.clone(),
                        session_id,
                        TransferKind::Upload,
                        path,
                        remote_child,
                        metadata.len(),
                        fs.clone(),
                        settings.clone(),
                        None,
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
