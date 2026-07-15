use std::time::Duration;

use super::{LocalDownloadTarget, TransferItem};

const REMOTE_PARTIAL_CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub(super) enum PartialTransferTarget {
    Remote(String),
    Local(LocalDownloadTarget),
}

impl TransferItem {
    pub(super) fn mark_remote_partial(&self, path: String) {
        *self.partial_target.lock().unwrap() = Some(PartialTransferTarget::Remote(path));
    }

    pub(super) fn mark_local_partial(&self, target: LocalDownloadTarget) {
        *self.partial_target.lock().unwrap() = Some(PartialTransferTarget::Local(target));
    }

    pub(super) fn clear_partial(&self) {
        self.partial_target.lock().unwrap().take();
    }

    pub(super) async fn cleanup_partial(&self) {
        let target = self.partial_target.lock().unwrap().clone();
        match target {
            Some(PartialTransferTarget::Remote(path)) => {
                let _ = tokio::time::timeout(
                    REMOTE_PARTIAL_CLEANUP_TIMEOUT,
                    self.fs.delete_file(&path),
                )
                .await;
            }
            Some(PartialTransferTarget::Local(target)) => {
                let _ = target.root.remove_file(&target.relative);
            }
            None => {}
        }
        self.clear_partial();
    }
}
