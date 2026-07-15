/// Unpack an untrusted tar stream under the same rules as the per-file
/// download path: every entry name is validated (and on Windows sanitized)
/// with `safe_local_component`, all writes go through a capability handle
/// on the destination, and link/device entries from the remote server are
/// skipped rather than materialized — a planted symlink must never redirect
/// a later write outside the tree.
pub(super) fn unpack_confined<R: std::io::Read>(
    mut archive: tar::Archive<R>,
    root: &cap_std::fs::Dir,
) -> std::io::Result<()> {
    for entry in archive.entries()? {
        let mut entry = entry?;
        let mut relative = std::path::PathBuf::new();
        {
            let path = entry.path()?;
            for component in path.components() {
                match component {
                    std::path::Component::CurDir => {}
                    std::path::Component::Normal(part) => relative.push(
                        crate::transfer::safe_local_component(&part.to_string_lossy())
                            .map_err(std::io::Error::other)?,
                    ),
                    _ => {
                        return Err(std::io::Error::other(format!(
                            "tar entry {path:?} escapes the destination"
                        )))
                    }
                }
            }
        }
        if relative.as_os_str().is_empty() {
            continue;
        }
        match entry.header().entry_type() {
            tar::EntryType::Directory => match root.create_dir(&relative) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            },
            tar::EntryType::Regular | tar::EntryType::Continuous | tar::EntryType::GNUSparse => {
                if let Some(parent) = relative.parent() {
                    if !parent.as_os_str().is_empty() {
                        root.create_dir_all(parent)?;
                    }
                }
                let mut options = cap_std::fs::OpenOptions::new();
                options.write(true).create(true).truncate(true);
                let mut file = root.open_with(&relative, &options)?.into_std();
                std::io::copy(&mut entry, &mut file)?;
                if let Ok(mtime) = entry.header().mtime() {
                    let _ = filetime::set_file_handle_times(
                        &file,
                        None,
                        Some(filetime::FileTime::from_unix_time(mtime as i64, 0)),
                    );
                }
            }
            // Symlinks, hard links, fifos, devices: never materialized from
            // an untrusted stream (matches the per-file path, which does not
            // descend into remote links either).
            _ => {}
        }
    }
    Ok(())
}
