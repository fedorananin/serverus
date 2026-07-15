use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

pub(super) fn edit_cache_dir() -> PathBuf {
    std::env::temp_dir().join("serverus-edit")
}

pub(super) fn validate_edit_filename(name: &str) -> AppResult<()> {
    let contains_unsafe_character = name.chars().any(|character| {
        character < ' '
            || matches!(
                character,
                '\0' | '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
            )
    });
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || contains_unsafe_character
    {
        return Err(AppError::RemoteFs(format!(
            "remote edit filename is not portable: {name:?}"
        )));
    }

    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
        || stem
            .strip_prefix("LPT")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number));
    if reserved {
        return Err(AppError::RemoteFs(format!(
            "remote edit filename is reserved on Windows: {name:?}"
        )));
    }
    Ok(())
}

pub(super) fn create_private_edit_dir(root: &Path) -> AppResult<PathBuf> {
    ensure_private_cache_root(root)?;
    for _ in 0..16 {
        let dir = root.join(uuid::Uuid::new_v4().to_string());
        let builder = private_dir_builder();
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::Other(
        "could not allocate a unique remote-edit cache directory".into(),
    ))
}

pub(super) async fn create_private_cache_file(path: &Path) -> AppResult<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options
        .open(path)
        .await
        .map_err(|error| AppError::Other(format!("edit cache file: {error}")))
}

pub(super) struct PendingCacheDir(Option<PathBuf>);

impl PendingCacheDir {
    pub(super) fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    pub(super) fn keep(&mut self) {
        self.0 = None;
    }
}

impl Drop for PendingCacheDir {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Best-effort cleanup of downloaded copies (SPEC §5.3).
pub fn cleanup_all() {
    let _ = std::fs::remove_dir_all(edit_cache_dir());
}

fn ensure_private_cache_root(root: &Path) -> AppResult<()> {
    match std::fs::symlink_metadata(root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AppError::Other(format!(
                    "edit cache path is not a directory: {}",
                    root.display()
                )));
            }
            secure_unix_cache_root(root, &metadata)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match private_dir_builder().create(root) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    ensure_private_cache_root(root)
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn secure_unix_cache_root(root: &Path, metadata: &std::fs::Metadata) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    }
    let secured = std::fs::symlink_metadata(root)?;
    if secured.file_type().is_symlink()
        || !secured.is_dir()
        || secured.permissions().mode() & 0o077 != 0
    {
        return Err(AppError::Other(format!(
            "edit cache directory is not private: {}",
            root.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_unix_cache_root(_root: &Path, _metadata: &std::fs::Metadata) -> AppResult<()> {
    Ok(())
}

#[cfg(unix)]
fn private_dir_builder() -> std::fs::DirBuilder {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder
}

#[cfg(not(unix))]
fn private_dir_builder() -> std::fs::DirBuilder {
    std::fs::DirBuilder::new()
}
