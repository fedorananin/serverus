use std::sync::Arc;

use crate::session::remote_fs::RemoteFs;
use crate::vault::model::TransferSettings;

pub struct UploadRequest<'a> {
    pub(super) fs: Arc<dyn RemoteFs>,
    pub(super) session_id: &'a str,
    pub(super) local_path: &'a str,
    pub(super) remote_dir: &'a str,
    pub(super) settings: TransferSettings,
}

impl<'a> UploadRequest<'a> {
    pub fn new(
        fs: Arc<dyn RemoteFs>,
        session_id: &'a str,
        local_path: &'a str,
        remote_dir: &'a str,
        settings: TransferSettings,
    ) -> Self {
        Self {
            fs,
            session_id,
            local_path,
            remote_dir,
            settings,
        }
    }
}

pub struct DownloadRequest<'a> {
    pub(super) fs: Arc<dyn RemoteFs>,
    pub(super) session_id: &'a str,
    pub(super) remote_path: &'a str,
    pub(super) local_dir: &'a str,
    pub(super) settings: TransferSettings,
}

impl<'a> DownloadRequest<'a> {
    pub fn new(
        fs: Arc<dyn RemoteFs>,
        session_id: &'a str,
        remote_path: &'a str,
        local_dir: &'a str,
        settings: TransferSettings,
    ) -> Self {
        Self {
            fs,
            session_id,
            remote_path,
            local_dir,
            settings,
        }
    }
}
