use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serverus_lib::error::{AppError, AppResult};
use serverus_lib::events::TransferProgressEvent;
use serverus_lib::session::remote_fs::{BoxRead, BoxWrite, RemoteEntry, RemoteFs};
use serverus_lib::transfer::{DownloadRequest, ProgressSink, TransferManager, TransferState};
use serverus_lib::vault::model::{ConflictPolicy, TransferSettings};

use super::transfer_context;

struct NullSink;

impl ProgressSink for NullSink {
    fn emit(&self, _event: TransferProgressEvent) {}
}

pub(super) struct ListingFs {
    root: RemoteEntry,
    listings: HashMap<String, Vec<RemoteEntry>>,
}

impl ListingFs {
    pub(super) fn file(name: &str) -> Arc<Self> {
        Arc::new(Self {
            root: entry(name, "/tree", false, false),
            listings: HashMap::new(),
        })
    }

    pub(super) fn directory_with_children(names: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            root: entry("tree", "/tree", true, false),
            listings: HashMap::from([(
                "/tree".into(),
                names
                    .iter()
                    .enumerate()
                    // Keep paths unique when two entries have the same name.
                    .map(|(index, name)| {
                        entry(name, &format!("/tree/{index}-{name}"), false, false)
                    })
                    .collect(),
            )]),
        })
    }

    pub(super) fn directory_with_nested_file(directory: &str, file: &str) -> Arc<Self> {
        Arc::new(Self {
            root: entry("tree", "/tree", true, false),
            listings: HashMap::from([
                (
                    "/tree".into(),
                    vec![entry(directory, &format!("/tree/{directory}"), true, false)],
                ),
                (
                    format!("/tree/{directory}"),
                    vec![entry(
                        file,
                        &format!("/tree/{directory}/{file}"),
                        false,
                        false,
                    )],
                ),
            ]),
        })
    }
}

fn entry(name: &str, path: &str, is_dir: bool, is_symlink: bool) -> RemoteEntry {
    RemoteEntry {
        name: name.into(),
        path: path.into(),
        is_dir,
        is_symlink,
        size: 6,
        mtime: None,
        permissions: None,
    }
}

#[async_trait::async_trait]
impl RemoteFs for ListingFs {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        Ok(self.listings.get(path).cloned().unwrap_or_default())
    }

    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        if path == "/tree" {
            Ok(self.root.clone())
        } else if let Some(entry) = self
            .listings
            .values()
            .flatten()
            .find(|entry| entry.path == path)
        {
            Ok(entry.clone())
        } else {
            Err(AppError::RemoteFs(format!("{path}: not found")))
        }
    }

    async fn home_dir(&self) -> AppResult<String> {
        Ok("/".into())
    }

    async fn mkdir(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn create_file(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn rename(&self, _from: &str, _to: &str) -> AppResult<()> {
        Ok(())
    }

    async fn delete_file(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn delete_dir(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn chmod(&self, _path: &str, _mode: u32) -> AppResult<()> {
        Ok(())
    }

    async fn set_mtime(&self, _path: &str, _mtime_unix: i64) -> AppResult<()> {
        Ok(())
    }

    async fn open_read(&self, _path: &str, _offset: u64) -> AppResult<BoxRead> {
        Ok(Box::new(std::io::Cursor::new(b"attack".to_vec())))
    }

    async fn open_write(&self, path: &str, _offset: u64) -> AppResult<BoxWrite> {
        Err(AppError::RemoteFs(format!("{path}: not supported")))
    }

    async fn exists(&self, _path: &str) -> AppResult<bool> {
        Ok(false)
    }
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 1,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false,
        tar_acceleration: false,
    }
}

pub(super) async fn attempt_enqueue(
    fs: Arc<ListingFs>,
    destination: &Path,
) -> (Arc<TransferManager>, AppResult<()>) {
    let manager = Arc::new(TransferManager::default());
    let context_id = transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    let result = manager
        .enqueue_download(
            context_id,
            &sink,
            DownloadRequest::new(
                fs,
                "session",
                "/tree",
                destination.to_str().unwrap(),
                settings(),
            ),
        )
        .await;
    (manager, result)
}

pub(super) async fn enqueue(
    fs: Arc<ListingFs>,
    destination: &Path,
) -> AppResult<Arc<TransferManager>> {
    let (manager, result) = attempt_enqueue(fs, destination).await;
    result?;
    Ok(manager)
}

pub(super) async fn wait_for_transfer(manager: &TransferManager) {
    for _ in 0..100 {
        let items = manager.snapshot().items;
        if !items.is_empty()
            && items.iter().all(|item| {
                matches!(
                    item.state,
                    TransferState::Done | TransferState::Error | TransferState::Cancelled
                )
            })
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("transfer did not finish");
}
