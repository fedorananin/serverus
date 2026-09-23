//! In-memory `RemoteFs` for tree-operation tests: a path → kind map with
//! failure injection, optional bulk delete / tree snapshot, and a probe for
//! how many requests were in flight at once.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{
    join_remote, parent_remote, BoxRead, BoxWrite, EntryFailure, RemoteEntry, RemoteFs,
    TreeSnapshot, TreeSnapshotItem,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    File,
    Dir,
    /// A symlink that leads to a directory (listed with `is_dir = true`).
    DirLink,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Bulk {
    #[default]
    Unsupported,
    Supported,
    /// The provider rejects the request as a whole.
    Refused,
}

#[derive(Default)]
pub(super) struct MemFs {
    pub(super) nodes: Mutex<BTreeMap<String, Kind>>,
    pub(super) modes: Mutex<BTreeMap<String, u32>>,
    pub(super) deny: HashSet<String>,
    pub(super) bulk: Bulk,
    pub(super) bulk_calls: AtomicUsize,
    pub(super) snapshot: bool,
    pub(super) parallel: usize,
    in_flight: AtomicUsize,
    pub(super) max_in_flight: AtomicUsize,
}

impl MemFs {
    pub(super) fn with(entries: &[(&str, Kind)]) -> Self {
        let fs = Self {
            parallel: 1,
            ..Self::default()
        };
        fs.nodes
            .lock()
            .unwrap()
            .extend(entries.iter().map(|(path, kind)| (path.to_string(), *kind)));
        fs
    }

    pub(super) fn paths(&self) -> Vec<String> {
        self.nodes.lock().unwrap().keys().cloned().collect()
    }

    fn children(&self, dir: &str) -> Vec<(String, Kind)> {
        self.nodes
            .lock()
            .unwrap()
            .iter()
            .filter(|(path, _)| parent_remote(path) == dir && path.as_str() != dir)
            .map(|(path, kind)| (path.clone(), *kind))
            .collect()
    }

    async fn request<T>(
        &self,
        path: &str,
        action: impl FnOnce(&Self) -> AppResult<T>,
    ) -> AppResult<T> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(now, Ordering::SeqCst);
        tokio::task::yield_now().await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        if self.deny.contains(path) {
            return Err(AppError::RemoteFs(format!("{path}: permission denied")));
        }
        action(self)
    }

    fn remove(&self, path: &str, dir: bool) -> AppResult<()> {
        if dir && !self.children(path).is_empty() {
            return Err(AppError::RemoteFs(format!("{path}: directory not empty")));
        }
        let mut nodes = self.nodes.lock().unwrap();
        match nodes.get(path) {
            Some(Kind::Dir) if !dir => Err(AppError::RemoteFs(format!("{path}: is a directory"))),
            Some(_) => {
                nodes.remove(path);
                Ok(())
            }
            // Like S3, a missing directory placeholder is fine.
            None if dir => Ok(()),
            None => Err(AppError::RemoteFs(format!("{path}: not found"))),
        }
    }
}

fn unsupported<T>() -> AppResult<T> {
    Err(AppError::RemoteFs("not supported by MemFs".into()))
}

#[async_trait::async_trait]
impl RemoteFs for MemFs {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        self.request(path, |fs| {
            Ok(fs
                .children(path)
                .into_iter()
                .map(|(child, kind)| RemoteEntry {
                    name: child.rsplit('/').next().unwrap_or_default().to_string(),
                    path: child,
                    is_dir: kind != Kind::File,
                    is_symlink: kind == Kind::DirLink,
                    size: 0,
                    mtime: None,
                    permissions: None,
                })
                .collect())
        })
        .await
    }
    async fn stat(&self, _path: &str) -> AppResult<RemoteEntry> {
        unsupported()
    }
    async fn home_dir(&self) -> AppResult<String> {
        Ok("/".into())
    }
    async fn mkdir(&self, _path: &str) -> AppResult<()> {
        unsupported()
    }
    async fn create_file(&self, _path: &str) -> AppResult<()> {
        unsupported()
    }
    async fn rename(&self, _from: &str, _to: &str) -> AppResult<()> {
        unsupported()
    }
    async fn delete_file(&self, path: &str) -> AppResult<()> {
        self.request(path, |fs| fs.remove(path, false)).await
    }
    async fn delete_dir(&self, path: &str) -> AppResult<()> {
        self.request(path, |fs| fs.remove(path, true)).await
    }
    async fn chmod(&self, path: &str, mode: u32) -> AppResult<()> {
        self.request(path, |fs| {
            fs.modes.lock().unwrap().insert(path.to_string(), mode);
            Ok(())
        })
        .await
    }
    async fn set_mtime(&self, _path: &str, _mtime_unix: i64) -> AppResult<()> {
        unsupported()
    }
    async fn open_read(&self, _path: &str, _offset: u64) -> AppResult<BoxRead> {
        unsupported()
    }
    async fn open_write(&self, _path: &str, _offset: u64) -> AppResult<BoxWrite> {
        unsupported()
    }
    async fn exists(&self, path: &str) -> AppResult<bool> {
        Ok(self.nodes.lock().unwrap().contains_key(path))
    }
    async fn tree_snapshot(&self, path: &str, _limit: usize) -> AppResult<Option<TreeSnapshot>> {
        if !self.snapshot {
            return Ok(None);
        }
        // Object-store style: only files, their parent prefixes are implied.
        let prefix = join_remote(path, "");
        let items = self
            .nodes
            .lock()
            .unwrap()
            .iter()
            .filter(|(node, kind)| **kind == Kind::File && node.starts_with(&prefix))
            .map(|(node, _)| TreeSnapshotItem {
                rel_path: node[prefix.len()..].to_string(),
                is_dir: false,
                size: 0,
                mtime: None,
            })
            .collect();
        Ok(Some(TreeSnapshot {
            items,
            truncated: false,
        }))
    }
    fn parallel_ops(&self) -> usize {
        self.parallel
    }
    async fn delete_files_bulk(&self, paths: &[String]) -> AppResult<Option<Vec<EntryFailure>>> {
        match self.bulk {
            Bulk::Unsupported => return Ok(None),
            Bulk::Refused => return Err(AppError::RemoteFs("NotImplemented".into())),
            Bulk::Supported => {}
        }
        self.bulk_calls.fetch_add(1, Ordering::SeqCst);
        let mut failures = Vec::new();
        for path in paths {
            let result = if self.deny.contains(path) {
                Err(AppError::RemoteFs(format!("{path}: AccessDenied")))
            } else {
                self.remove(path, false)
            };
            if let Err(error) = result {
                failures.push(EntryFailure::new(path, &error));
            }
        }
        Ok(Some(failures))
    }
}
