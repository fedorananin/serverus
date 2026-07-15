//! Finder drag preview and local recursive-copy commands.

use super::prelude::*;

/// Write the drag preview icon to a temp file once and return its path.
/// tauri-plugin-drag needs an on-disk image for the OS drag cursor.
#[tauri::command]
#[specta::specta]
pub async fn drag_preview_icon() -> ApiResult<String> {
    blocking(|| {
        let path = std::env::temp_dir().join("serverus-drag-icon.png");
        if !path.exists() {
            const ICON: &[u8] = include_bytes!("../../icons/128x128.png");
            std::fs::write(&path, ICON)?;
        }
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

/// Copy files/dirs into `dest_dir` on the local filesystem (Finder → local
/// pane drop). Skips items already inside `dest_dir` (dropped onto self).
#[tauri::command]
#[specta::specta]
pub async fn local_copy_into(paths: Vec<String>, dest_dir: String) -> ApiResult<()> {
    blocking(move || {
        let dest = local_fs::expand(&dest_dir);
        for p in paths {
            let src = std::path::PathBuf::from(&p);
            let Some(name) = src.file_name() else {
                continue;
            };
            let target = dest.join(name);
            if src.parent() == Some(dest.as_path()) || src == target {
                continue; // same directory — nothing to do
            }
            copy_recursive(&src, &target)?;
        }
        Ok(())
    })
    .await
}

fn copy_recursive(src: &std::path::Path, dest: &std::path::Path) -> AppResult<()> {
    let mut pending_permissions = Vec::new();
    copy_recursive_inner(src, dest, &mut pending_permissions)?;

    for (path, permissions, _) in &pending_permissions {
        if let Err(error) = std::fs::set_permissions(path, permissions.clone()) {
            make_partial_copy_removable(&pending_permissions);
            if let Err(cleanup_error) = remove_partial_copy(dest) {
                return Err(AppError::Other(format!(
                    "failed to apply copied permissions: {error}; failed to remove partial copy: {cleanup_error}"
                )));
            }
            return Err(error.into());
        }
    }

    Ok(())
}

type PendingPermission = (std::path::PathBuf, std::fs::Permissions, bool);

fn make_partial_copy_removable(pending_permissions: &[PendingPermission]) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for (path, _, is_directory) in pending_permissions.iter().rev() {
            let mode = if *is_directory { 0o700 } else { 0o600 };
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
        }
    }
    #[cfg(windows)]
    for (path, permissions, _) in pending_permissions.iter().rev() {
        let mut writable_permissions = permissions.clone();
        clear_windows_readonly(&mut writable_permissions);
        let _ = std::fs::set_permissions(path, writable_permissions);
    }
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn clear_windows_readonly(permissions: &mut std::fs::Permissions) {
    // Windows exposes a read-only file attribute rather than Unix mode bits,
    // so clearing it does not make the partial copy world-writable.
    permissions.set_readonly(false);
}

fn remove_partial_copy(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn copy_recursive_inner(
    src: &std::path::Path,
    dest: &std::path::Path,
    pending_permissions: &mut Vec<PendingPermission>,
) -> AppResult<()> {
    let meta = std::fs::symlink_metadata(src)?;
    let source = std::fs::canonicalize(src)?;
    let dest_parent = dest
        .parent()
        .ok_or_else(|| AppError::Other("copy destination has no parent".into()))?;
    let destination = std::fs::canonicalize(dest_parent)?.join(
        dest.file_name()
            .ok_or_else(|| AppError::Other("copy destination has no file name".into()))?,
    );

    if meta.is_dir() && destination.starts_with(&source) {
        return Err(AppError::Other(
            "cannot copy a directory inside the source directory".into(),
        ));
    }

    match std::fs::symlink_metadata(dest) {
        Ok(_) => {
            return Err(AppError::Other(format!(
                "{}: already exists",
                dest.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    if meta.is_dir() {
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;

            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = std::fs::DirBuilder::new();
        builder.create(dest).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::Other(format!("{}: already exists", dest.display()))
            } else {
                error.into()
            }
        })?;

        let copy_result = (|| -> AppResult<()> {
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                copy_recursive_inner(
                    &entry.path(),
                    &dest.join(entry.file_name()),
                    pending_permissions,
                )?;
            }
            #[cfg(unix)]
            pending_permissions.push((dest.to_path_buf(), meta.permissions(), true));
            Ok(())
        })();
        if let Err(error) = copy_result {
            if let Err(cleanup_error) = std::fs::remove_dir_all(dest) {
                return Err(AppError::Other(format!(
                    "copy failed: {error}; failed to remove partial copy: {cleanup_error}"
                )));
            }
            return Err(error);
        }
    } else {
        let mut source_file = std::fs::File::open(src)?;
        // `src` may be a symlink. Apply the permissions of the file we
        // actually opened, never the typically world-accessible link mode.
        let source_permissions = source_file.metadata()?.permissions();
        let mut destination_options = std::fs::OpenOptions::new();
        destination_options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Private files such as SSH keys must never spend the copy window
            // with broader umask-derived permissions than their source.
            destination_options.mode(0o600);
        }
        let mut destination_file = destination_options.open(dest).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::Other(format!("{}: already exists", dest.display()))
            } else {
                error.into()
            }
        })?;
        let copy_result = (|| -> std::io::Result<()> {
            std::io::copy(&mut source_file, &mut destination_file)?;
            destination_file.sync_all()
        })();
        drop(destination_file);
        if let Err(error) = copy_result {
            if let Err(cleanup_error) = std::fs::remove_file(dest) {
                return Err(AppError::Other(format!(
                    "copy failed: {error}; failed to remove partial copy: {cleanup_error}"
                )));
            }
            return Err(error.into());
        }
        pending_permissions.push((dest.to_path_buf(), source_permissions, false));
    }
    Ok(())
}

#[cfg(test)]
#[path = "local_copy_tests.rs"]
mod tests;
