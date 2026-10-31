use std::sync::atomic::Ordering;

use crate::error::AppResult;
use crate::session::remote_fs::{join_remote, RemoteEntry};

use super::pool::{ftp_err, FtpConn};
use super::FtpPool;

impl FtpPool {
    /// List a directory *including dot-files*. A bare `LIST` hides them on
    /// stock vsftpd, Pure-FTPd and ProFTPD, which silently dropped
    /// `.htaccess` / `.env` from recursive transfers and deletes. `LIST -a`
    /// is what those servers (and FileZilla's "force hidden files") take.
    /// MLSD is deliberately not used: some servers report a symlinked
    /// directory there as a plain directory, and the recursive walks would
    /// then descend into — and delete inside — the link's target.
    ///
    /// A server that rejects the option, or reads `-a <dir>` as a path and
    /// lists nothing, gets a bare `LIST` from then on.
    pub(super) async fn read_dir(
        &self,
        conn: &mut FtpConn,
        path: &str,
    ) -> AppResult<Vec<RemoteEntry>> {
        if self.list_all.load(Ordering::Relaxed) {
            match conn.list(Some(&format!("-a {path}"))).await {
                Ok(lines) => {
                    let entries = parse_lines(path, &lines);
                    if !entries.is_empty() {
                        return Ok(entries);
                    }
                    // Empty: a genuinely empty folder, or `-a` misread.
                    let plain = list_plain(conn, path).await?;
                    if !plain.is_empty() {
                        self.list_all.store(false, Ordering::Relaxed);
                    }
                    return Ok(plain);
                }
                Err(suppaftp::FtpError::UnexpectedResponse(response))
                    if response.status.code() >= 500 =>
                {
                    let plain = list_plain(conn, path).await?;
                    self.list_all.store(false, Ordering::Relaxed);
                    return Ok(plain);
                }
                Err(error) => return Err(ftp_err(path, error)),
            }
        }
        list_plain(conn, path).await
    }
}

async fn list_plain(conn: &mut FtpConn, path: &str) -> AppResult<Vec<RemoteEntry>> {
    let lines = conn
        .list(Some(path))
        .await
        .map_err(|error| ftp_err(path, error))?;
    Ok(parse_lines(path, &lines))
}

pub(super) fn parse_lines(dir: &str, lines: &[String]) -> Vec<RemoteEntry> {
    lines
        .iter()
        .filter_map(|line| parse_entry(dir, line))
        .collect()
}

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
