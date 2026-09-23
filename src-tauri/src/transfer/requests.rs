use std::sync::Arc;

use crate::session::remote_fs::RemoteFs;
use crate::vault::model::TransferSettings;

pub struct UploadRequest<'a> {
    pub(super) fs: Arc<dyn RemoteFs>,
    pub(super) session_id: &'a str,
    pub(super) local_path: &'a str,
    pub(super) remote_dir: &'a str,
    pub(super) settings: TransferSettings,
    pub(super) skip_junk: bool,
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
            skip_junk: false,
        }
    }

    /// Leave OS metadata junk (`.DS_Store`, `Thumbs.db`) out of a directory
    /// transfer — the "hide local junk" panel setting. An explicitly chosen
    /// top-level path is always transferred.
    pub fn skipping_junk(mut self, skip: bool) -> Self {
        self.skip_junk = skip;
        self
    }
}

pub struct DownloadRequest<'a> {
    pub(super) fs: Arc<dyn RemoteFs>,
    pub(super) session_id: &'a str,
    pub(super) remote_path: &'a str,
    pub(super) local_dir: &'a str,
    pub(super) settings: TransferSettings,
    pub(super) skip_junk: bool,
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
            skip_junk: false,
        }
    }

    /// Leave OS metadata junk (`.DS_Store`, `Thumbs.db`) out of a directory
    /// transfer — the "hide local junk" panel setting. An explicitly chosen
    /// top-level path is always transferred.
    pub fn skipping_junk(mut self, skip: bool) -> Self {
        self.skip_junk = skip;
        self
    }
}
