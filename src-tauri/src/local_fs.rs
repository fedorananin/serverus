//! Local filesystem operations for the left panel (SPEC §5.2). Same entry
//! shape as remote listings so the two panes share UI components.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::RemoteEntry;

fn entry_for(path: &Path, meta: &fs::Metadata, resolved: Option<&fs::Metadata>) -> RemoteEntry {
    use std::os::unix::fs::PermissionsExt;
    let target = resolved.unwrap_or(meta);
    RemoteEntry {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        path: path.to_string_lossy().into_owned(),
        is_dir: target.is_dir(),
        is_symlink: meta.is_symlink(),
        size: if target.is_dir() { 0 } else { target.len() },
        mtime: target
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
        permissions: Some(target.permissions().mode() & 0o7777),
    }
}

pub fn list(dir: &str) -> AppResult<Vec<RemoteEntry>> {
    let mut out = Vec::new();
    for item in fs::read_dir(expand(dir))? {
        let item = item?;
        let path = item.path();
        let Ok(meta) = item.metadata() else { continue };
        // For symlinks resolve the target so directories navigate correctly.
        let resolved = if meta.is_symlink() {
            fs::metadata(&path).ok()
        } else {
            None
        };
        out.push(entry_for(&path, &meta, resolved.as_ref()));
    }
    Ok(out)
}

pub fn home() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .to_string_lossy()
        .into_owned()
}

/// Expand `~` and normalize to an absolute path where possible.
pub fn expand(path: &str) -> PathBuf {
    let expanded = if path == "~" {
        home()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("{}/{}", home(), rest)
    } else {
        path.to_string()
    };
    PathBuf::from(expanded)
}

pub fn mkdir(path: &str) -> AppResult<()> {
    fs::create_dir(expand(path)).map_err(Into::into)
}

pub fn create_file(path: &str) -> AppResult<()> {
    let p = expand(path);
    if p.exists() {
        return Err(AppError::Other(format!("{path}: already exists")));
    }
    fs::File::create(p)?;
    Ok(())
}

pub fn rename(from: &str, to: &str) -> AppResult<()> {
    fs::rename(expand(from), expand(to)).map_err(Into::into)
}

/// Delete a file or a directory tree (the UI confirms beforehand).
pub fn delete(path: &str) -> AppResult<()> {
    let p = expand(path);
    let meta = fs::symlink_metadata(&p)?;
    if meta.is_dir() && !meta.is_symlink() {
        fs::remove_dir_all(p)?;
    } else {
        fs::remove_file(p)?;
    }
    Ok(())
}

pub fn chmod(path: &str, mode: u32) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(expand(path), fs::Permissions::from_mode(mode)).map_err(Into::into)
}
