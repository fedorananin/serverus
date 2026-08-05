use serverus_domain::fs_compare::{entries_match, CompareRules, EntryMeta};

const DAY: i64 = 24 * 60 * 60;

fn file(size: u64, mtime: Option<i64>) -> EntryMeta {
    EntryMeta {
        name: "f".into(),
        is_dir: false,
        is_symlink: false,
        size,
        mtime,
    }
}

fn dir() -> EntryMeta {
    EntryMeta {
        name: "d".into(),
        is_dir: true,
        is_symlink: false,
        size: 0,
        mtime: None,
    }
}

#[test]
fn kind_mismatch_differs() {
    assert!(!entries_match(
        &dir(),
        &file(0, None),
        CompareRules::default()
    ));
    let symlink = EntryMeta {
        is_symlink: true,
        ..file(1, None)
    };
    assert!(!entries_match(
        &file(1, None),
        &symlink,
        CompareRules::default()
    ));
}

#[test]
fn directories_match_on_metadata() {
    assert!(entries_match(&dir(), &dir(), CompareRules::default()));
}

#[test]
fn files_compare_size_then_mtime() {
    let rules = CompareRules::default();
    assert!(!entries_match(&file(1, None), &file(2, None), rules));
    assert!(entries_match(
        &file(1, Some(100)),
        &file(1, Some(100)),
        rules
    ));
    assert!(!entries_match(
        &file(1, Some(100)),
        &file(1, Some(101)),
        rules
    ));
    assert!(entries_match(&file(1, None), &file(1, Some(101)), rules));
}

#[test]
fn ignore_mtime_skips_time() {
    let rules = CompareRules {
        ignore_mtime: true,
        ..CompareRules::default()
    };
    assert!(entries_match(
        &file(1, Some(100)),
        &file(1, Some(999)),
        rules
    ));
}

#[test]
fn coarse_mtime_uses_listing_precision() {
    let rules = CompareRules {
        coarse_remote_mtime: true,
        ..CompareRules::default()
    };
    // Minute precision for a stamp that is not midnight-exact.
    assert!(entries_match(
        &file(1, Some(3661)),
        &file(1, Some(3679)),
        rules
    ));
    assert!(!entries_match(
        &file(1, Some(3661)),
        &file(1, Some(3721)),
        rules
    ));
    // Date-only precision for a midnight-exact remote stamp.
    assert!(entries_match(
        &file(1, Some(DAY + 12 * 3600)),
        &file(1, Some(DAY)),
        rules
    ));
    assert!(!entries_match(
        &file(1, Some(DAY - 1)),
        &file(1, Some(DAY)),
        rules
    ));
}
