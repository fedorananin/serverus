use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AppError, AppResult};

/// Names that could act as anything but a plain file name are refused on
/// every OS: traversal dots, separators and control bytes have no legitimate
/// reading in a remote listing entry.
fn reject_attack_component(name: &str) -> AppResult<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.bytes().any(|byte| byte < 32)
    {
        return Err(AppError::Transfer(format!(
            "{name:?}: unsafe remote file name"
        )));
    }
    Ok(())
}

fn is_windows_device_stem(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
        .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

/// Repair remote names that Win32 cannot store instead of refusing them.
/// Compiled on every OS so these platform rules stay unit-tested everywhere.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn sanitize_windows_component(name: &str) -> String {
    let mut sanitized: String = name
        .chars()
        .map(|character| match character {
            '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*' => '_',
            character => character,
        })
        .collect();
    // Win32 silently drops trailing dots and spaces. Substitute each byte so
    // distinct remote names remain distinct unless the collision is explicit.
    let trimmed = sanitized.trim_end_matches([' ', '.']).len();
    let dropped = sanitized.len() - trimmed;
    if dropped > 0 {
        sanitized.truncate(trimmed);
        sanitized.push_str(&"_".repeat(dropped));
    }
    if is_windows_device_stem(&sanitized) {
        sanitized.insert(0, '_');
    }
    sanitized
}

/// Decide the local file name for an untrusted remote listing entry.
/// Attack-shaped names are refused; names this OS cannot store are sanitized
/// on Windows and kept verbatim on macOS/Linux.
pub(crate) fn safe_local_component(name: &str) -> AppResult<String> {
    reject_attack_component(name)?;
    #[cfg(windows)]
    let local = sanitize_windows_component(name);
    #[cfg(not(windows))]
    let local = name.to_string();
    let mut components = Path::new(&local).components();
    if !matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return Err(AppError::Transfer(format!(
            "{name:?}: unsafe remote file name"
        )));
    }
    Ok(local)
}

pub(super) fn open_download_root(path: &Path) -> AppResult<Arc<cap_std::fs::Dir>> {
    cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())
        .map(Arc::new)
        .map_err(|error| AppError::Transfer(format!("{}: {error}", path.display())))
}

pub(super) fn ensure_download_directory(root: &cap_std::fs::Dir, relative: &Path) -> AppResult<()> {
    match root.create_dir(relative) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = root
                .symlink_metadata(relative)
                .map_err(|error| AppError::Transfer(format!("{}: {error}", relative.display())))?;
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(AppError::Transfer(format!(
                    "{}: local download directory is not a real directory",
                    relative.display()
                )));
            }
            Ok(())
        }
        Err(error) => Err(AppError::Transfer(format!(
            "{}: {error}",
            relative.display()
        ))),
    }
}

#[derive(Clone)]
pub(super) struct LocalDownloadTarget {
    pub(super) root: Arc<cap_std::fs::Dir>,
    pub(super) relative: PathBuf,
}

pub(super) fn local_target_exists(target: &LocalDownloadTarget) -> bool {
    target.root.symlink_metadata(&target.relative).is_ok()
}

pub(super) fn open_local_download(
    target: &LocalDownloadTarget,
    offset: u64,
) -> AppResult<tokio::fs::File> {
    match target.root.symlink_metadata(&target.relative) {
        Ok(metadata) if metadata.is_symlink() || metadata.is_dir() => {
            return Err(AppError::Transfer(format!(
                "{}: refusing to write through a local link or directory",
                target.relative.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AppError::Transfer(error.to_string())),
    }

    let mut options = cap_std::fs::OpenOptions::new();
    options.write(true);
    if offset > 0 {
        options.append(true);
    } else {
        options.create(true).truncate(true);
    }
    let file = target
        .root
        .open_with(&target.relative, &options)
        .map_err(|error| AppError::Transfer(error.to_string()))?;
    Ok(tokio::fs::File::from_std(file.into_std()))
}
