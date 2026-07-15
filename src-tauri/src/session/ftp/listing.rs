use crate::session::remote_fs::{join_remote, RemoteEntry};

/// Convert one FTP LIST output line into the protocol-neutral entry contract.
pub(super) fn parse_entry(dir: &str, line: &str) -> Option<RemoteEntry> {
    let file = suppaftp::list::File::try_from(line).ok()?;
    let name = file.name().to_string();
    if name == "." || name == ".." {
        return None;
    }
    let mtime = file
        .modified()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs() as i64);
    // POSIX pex bits when the server sends a unix-style listing.
    let mode = {
        let mut mode = 0_u32;
        let permissions = [
            (file.can_read(suppaftp::list::PosixPexQuery::Owner), 0o400),
            (file.can_write(suppaftp::list::PosixPexQuery::Owner), 0o200),
            (
                file.can_execute(suppaftp::list::PosixPexQuery::Owner),
                0o100,
            ),
            (file.can_read(suppaftp::list::PosixPexQuery::Group), 0o040),
            (file.can_write(suppaftp::list::PosixPexQuery::Group), 0o020),
            (
                file.can_execute(suppaftp::list::PosixPexQuery::Group),
                0o010,
            ),
            (file.can_read(suppaftp::list::PosixPexQuery::Others), 0o004),
            (file.can_write(suppaftp::list::PosixPexQuery::Others), 0o002),
            (
                file.can_execute(suppaftp::list::PosixPexQuery::Others),
                0o001,
            ),
        ];
        for (enabled, bit) in permissions {
            if enabled {
                mode |= bit;
            }
        }
        mode
    };
    Some(RemoteEntry {
        path: join_remote(dir, &name),
        is_dir: file.is_directory(),
        is_symlink: file.is_symlink(),
        size: file.size() as u64,
        mtime,
        permissions: Some(mode),
        name,
    })
}
