//! Pure entry-comparison rules shared by folder comparison features.
//!
//! These rules are the Rust source of truth for "do these two entries look
//! synced". The flat pane comparison in `src/lib/directory-comparison.ts`
//! implements the same rules for listings the frontend already holds — keep
//! the two in lockstep when changing anything here.

/// The metadata folder comparison is allowed to look at. Deliberately a
/// subset of a listing entry: directory size/mtime are unreliable on every
/// backend and must not participate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMeta {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    /// Unix seconds, when the backend reports one.
    pub mtime: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CompareRules {
    /// Skip modification-time comparison. Set for backends whose listed
    /// mtime is server-managed upload time (S3 `LastModified`) rather than a
    /// content stamp transfers can preserve — comparing it would mark every
    /// uploaded file "different" forever.
    pub ignore_mtime: bool,
    /// Compare mtime at the precision the remote listing actually carries.
    /// Set for FTP: LIST timestamps have no seconds for recent files, and
    /// only a date for files older than ~6 months — comparing them exactly
    /// against a second-precise local mtime flags nearly every synced file
    /// "different".
    pub coarse_remote_mtime: bool,
}

/// OS metadata junk the "hide local junk" panel setting suppresses in the
/// local pane and on the local side of folder comparison (case-insensitive:
/// Windows writes both `Thumbs.db` and `thumbs.db` in the wild). Mirrored by
/// `isLocalJunk` in `src/lib/directory-comparison.ts` — keep in lockstep.
pub fn is_local_junk(name: &str) -> bool {
    name.eq_ignore_ascii_case(".DS_Store") || name.eq_ignore_ascii_case("Thumbs.db")
}

const MINUTE: i64 = 60;
const DAY: i64 = 24 * 60 * 60;

fn mtimes_match(local: i64, remote: i64, coarse_remote: bool) -> bool {
    if !coarse_remote {
        return local == remote;
    }
    // A midnight-exact remote stamp is a date-only LIST entry; anything else
    // came with minute precision.
    let precision = if remote.rem_euclid(DAY) == 0 {
        DAY
    } else {
        MINUTE
    };
    local.div_euclid(precision) == remote.div_euclid(precision)
}

/// Whether a local/remote pair of same-named entries counts as "matching".
/// Directory pairs always match here — their contents are compared by
/// walking, never inferred from directory metadata.
pub fn entries_match(local: &EntryMeta, remote: &EntryMeta, rules: CompareRules) -> bool {
    if local.is_dir != remote.is_dir || local.is_symlink != remote.is_symlink {
        return false;
    }
    if local.is_dir {
        return true;
    }
    if local.size != remote.size {
        return false;
    }
    if rules.ignore_mtime {
        return true;
    }
    match (local.mtime, remote.mtime) {
        (Some(local), Some(remote)) => mtimes_match(local, remote, rules.coarse_remote_mtime),
        // An unknown mtime on either side is not evidence of a difference.
        _ => true,
    }
}
