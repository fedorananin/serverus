//! Atomic vault writes (SPEC §2.2): temp file + rename, with a `.bak` copy
//! of the previous version.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AppResult;

pub(super) fn bak_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    // One overwritable .bak of the previous version.
    if path.exists() {
        let previous = fs::read(path)?;
        replace_atomic(&bak_path(path), &previous)?;
    }

    replace_atomic(path, bytes)
}

/// Rekey both recoverable copies without ever leaving a partially-written
/// vault. If interrupted between the two renames, the primary still opens
/// with the old password and the backup already opens with the new one.
pub(super) fn write_password_change_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    replace_atomic(&bak_path(path), bytes)?;
    replace_atomic(path, bytes)
}

fn replace_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&dir)?;

    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    // Persist the rename itself.
    if let Ok(d) = fs::File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
}
