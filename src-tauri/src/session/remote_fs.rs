//! Protocol-agnostic remote file operations (SPEC §7.1). The UI and the
//! transfer queue only ever see this trait — SFTP and FTP implement it.

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RemoteEntry {
    pub name: String,
    /// Absolute remote path.
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
    /// Unix seconds.
    #[specta(type = Option<specta_typescript::Number>)]
    pub mtime: Option<i64>,
    /// Unix permission bits (lower 12 bits meaningful), when known.
    pub permissions: Option<u32>,
}

pub type BoxRead = Box<dyn AsyncRead + Send + Unpin>;
pub type BoxWrite = Box<dyn AsyncWrite + Send + Unpin>;

#[async_trait::async_trait]
pub trait RemoteFs: Send + Sync {
    /// Directory listing. Symlinks are resolved enough to know whether they
    /// lead to a directory (SPEC §5.2).
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>>;
    async fn stat(&self, path: &str) -> AppResult<RemoteEntry>;
    /// Resolve the login/home directory used as the initial path.
    async fn home_dir(&self) -> AppResult<String>;
    async fn mkdir(&self, path: &str) -> AppResult<()>;
    /// Create an empty file (fails if it exists).
    async fn create_file(&self, path: &str) -> AppResult<()>;
    async fn rename(&self, from: &str, to: &str) -> AppResult<()>;
    async fn delete_file(&self, path: &str) -> AppResult<()>;
    /// Remove an *empty* directory; recursion lives in the transfer module.
    async fn delete_dir(&self, path: &str) -> AppResult<()>;
    async fn chmod(&self, path: &str, mode: u32) -> AppResult<()>;
    async fn set_mtime(&self, path: &str, mtime_unix: i64) -> AppResult<()>;
    /// Open for reading, positioned at `offset` (resume support).
    async fn open_read(&self, path: &str, offset: u64) -> AppResult<BoxRead>;
    /// Open for writing. `offset == 0` truncates/creates; `offset > 0`
    /// appends starting at that position (resume support).
    async fn open_write(&self, path: &str, offset: u64) -> AppResult<BoxWrite>;
    /// Whether a path exists (used for conflict detection).
    async fn exists(&self, path: &str) -> AppResult<bool>;
    /// Whether `open_write` honours a non-zero offset (partial-upload
    /// resume). S3 cannot — a retried upload restarts the file.
    fn supports_write_resume(&self) -> bool {
        true
    }
}

/// Recursively delete a remote file or directory tree. Shared by SFTP and
/// FTP — recursion happens here, through the trait (SPEC §4.3).
pub async fn delete_recursive(fs: &dyn RemoteFs, path: &str, is_dir: bool) -> AppResult<()> {
    if !is_dir {
        return fs.delete_file(path).await;
    }
    // Iterative DFS: delete files on discovery, directories child-first.
    let mut stack = vec![path.to_string()];
    let mut dirs_in_discovery_order = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs.list(&dir).await? {
            if entry.is_dir && !entry.is_symlink {
                stack.push(entry.path);
            } else {
                fs.delete_file(&entry.path).await?;
            }
        }
        dirs_in_discovery_order.push(dir);
    }
    for dir in dirs_in_discovery_order.iter().rev() {
        fs.delete_dir(dir).await?;
    }
    Ok(())
}

/// Join a remote path and a child name with `/` semantics.
pub fn join_remote(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Parent of a remote path (`/a/b` → `/a`; `/a` → `/`).
pub fn parent_remote(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(idx) => trimmed[..idx].to_string(),
    }
}
