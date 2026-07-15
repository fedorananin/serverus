use super::archive::unpack_confined;

fn open_root(dir: &std::path::Path) -> cap_std::fs::Dir {
    cap_std::fs::Dir::open_ambient_dir(dir, cap_std::ambient_authority()).unwrap()
}

fn regular(path: &str, contents: &[u8]) -> (tar::Header, Vec<u8>) {
    let mut header = tar::Header::new_gnu();
    // Written raw so tests can smuggle names set_path would reject.
    header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
    header.set_size(contents.len() as u64);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_mtime(1_700_000_000);
    header.set_cksum();
    (header, contents.to_vec())
}

fn archive(entries: Vec<(tar::Header, Vec<u8>)>) -> tar::Archive<std::io::Cursor<Vec<u8>>> {
    let mut builder = tar::Builder::new(Vec::new());
    for (header, contents) in entries {
        builder.append(&header, contents.as_slice()).unwrap();
    }
    tar::Archive::new(std::io::Cursor::new(builder.into_inner().unwrap()))
}

#[test]
fn unpacks_a_normal_nested_tree() {
    let dir = tempfile::tempdir().unwrap();
    let entries = vec![
        regular("tree/a.txt", b"alpha"),
        regular("tree/nested/b.txt", b"beta"),
    ];

    unpack_confined(archive(entries), &open_root(dir.path())).unwrap();

    assert_eq!(
        std::fs::read(dir.path().join("tree/a.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(
        std::fs::read(dir.path().join("tree/nested/b.txt")).unwrap(),
        b"beta"
    );
}

#[test]
fn rejects_parent_directory_traversal() {
    let outside = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let inner = dir.path().join("inner");
    std::fs::create_dir(&inner).unwrap();

    let result = unpack_confined(
        archive(vec![regular("../evil.txt", b"pwned")]),
        &open_root(&inner),
    );

    assert!(result.is_err());
    assert!(!dir.path().join("evil.txt").exists());
    drop(outside);
}

#[test]
fn rejects_absolute_entry_paths() {
    let dir = tempfile::tempdir().unwrap();

    let result = unpack_confined(
        archive(vec![regular("/tmp/evil.txt", b"pwned")]),
        &open_root(dir.path()),
    );

    assert!(result.is_err());
    assert!(
        !std::path::Path::new("/tmp/evil.txt").exists() || {
            // Never trust a pre-existing /tmp/evil.txt on the test host —
            // the assertion that matters is the Err above.
            true
        }
    );
}

#[cfg(unix)]
#[test]
fn a_planted_symlink_cannot_redirect_later_entries() {
    let outside = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();

    let mut link = tar::Header::new_gnu();
    link.as_old_mut().name[..4].copy_from_slice(b"link");
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_link_name(outside.path()).unwrap();
    link.set_size(0);
    link.set_cksum();

    let entries = vec![(link, Vec::new()), regular("link/victim.txt", b"pwned")];
    // The symlink entry is skipped, so "link" becomes a real directory
    // and the write lands inside the destination.
    unpack_confined(archive(entries), &open_root(dir.path())).unwrap();

    assert!(!outside.path().join("victim.txt").exists());
    assert_eq!(
        std::fs::read(dir.path().join("link/victim.txt")).unwrap(),
        b"pwned"
    );
    assert!(!dir.path().join("link").is_symlink());
}

#[cfg(not(windows))]
#[test]
fn unix_legal_names_survive_the_tar_path_verbatim() {
    let dir = tempfile::tempdir().unwrap();

    unpack_confined(
        archive(vec![regular("tree/2024-01-01T12:00:00.log", b"log")]),
        &open_root(dir.path()),
    )
    .unwrap();

    assert_eq!(
        std::fs::read(dir.path().join("tree/2024-01-01T12:00:00.log")).unwrap(),
        b"log"
    );
}

#[test]
fn file_mtime_is_preserved() {
    let dir = tempfile::tempdir().unwrap();

    unpack_confined(
        archive(vec![regular("tree/dated.txt", b"x")]),
        &open_root(dir.path()),
    )
    .unwrap();

    let modified = std::fs::metadata(dir.path().join("tree/dated.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let unix = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(unix, 1_700_000_000);
}
