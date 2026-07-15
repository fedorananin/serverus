//! Local recursive-copy tests.

use super::copy_recursive;

#[test]
fn local_copy_refuses_to_overwrite_an_existing_file() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.txt");
    let destination = root.path().join("destination.txt");
    std::fs::write(&source, "new contents").unwrap();
    std::fs::write(&destination, "keep me").unwrap();

    let error = copy_recursive(&source, &destination).unwrap_err();

    assert!(error.to_string().contains("already exists"));
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "keep me");
}

#[test]
fn local_copy_refuses_to_overwrite_an_existing_directory() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("keep.txt"), "keep me").unwrap();

    let error = copy_recursive(&source, &destination).unwrap_err();

    assert!(error.to_string().contains("already exists"));
    assert_eq!(
        std::fs::read_to_string(destination.join("keep.txt")).unwrap(),
        "keep me"
    );
}

#[test]
fn local_copy_refuses_to_copy_a_directory_into_its_descendant() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("file.txt"), "contents").unwrap();
    let destination = source.join("nested");

    let error = copy_recursive(&source, &destination).unwrap_err();

    assert!(error.to_string().contains("inside the source directory"));
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn local_copy_detects_a_descendant_through_a_symlinked_parent() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let alias = root.path().join("source-alias");
    std::fs::create_dir(&source).unwrap();
    symlink(&source, &alias).unwrap();
    let destination = alias.join("nested");

    let error = copy_recursive(&source, &destination).unwrap_err();

    assert!(error.to_string().contains("inside the source directory"));
    assert!(!source.join("nested").exists());
}

#[cfg(unix)]
#[test]
fn local_copy_refuses_to_replace_a_symlink() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.txt");
    let target = root.path().join("target.txt");
    let destination = root.path().join("destination.txt");
    std::fs::write(&source, "new contents").unwrap();
    std::fs::write(&target, "keep me").unwrap();
    symlink(&target, &destination).unwrap();

    let error = copy_recursive(&source, &destination).unwrap_err();

    assert!(error.to_string().contains("already exists"));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep me");
}

#[test]
fn local_copy_copies_a_directory_tree() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    std::fs::create_dir_all(source.join("nested")).unwrap();
    std::fs::write(source.join("nested/file.txt"), "contents").unwrap();

    copy_recursive(&source, &destination).unwrap();

    assert_eq!(
        std::fs::read_to_string(destination.join("nested/file.txt")).unwrap(),
        "contents"
    );
}

#[cfg(unix)]
#[test]
fn local_copy_uses_followed_file_permissions_for_a_symlink_source() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("private-key");
    let source = root.path().join("private-key-link");
    let destination = root.path().join("copy");
    std::fs::write(&target, "secret").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, &source).unwrap();

    copy_recursive(&source, &destination).unwrap();

    assert_eq!(
        std::fs::metadata(destination).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn failed_directory_copy_removes_the_partial_destination() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    std::fs::create_dir(&source).unwrap();
    symlink(source.join("missing"), source.join("dangling")).unwrap();

    assert!(copy_recursive(&source, &destination).is_err());

    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn failed_directory_copy_removes_restricted_subdirectories() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let restricted = source.join("a-restricted");
    std::fs::create_dir_all(&restricted).unwrap();
    std::fs::write(restricted.join("copied.txt"), "contents").unwrap();
    std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o500)).unwrap();
    symlink(source.join("missing"), source.join("z-dangling")).unwrap();

    let result = copy_recursive(&source, &destination);
    std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o700)).unwrap();

    assert!(result.is_err());
    assert!(!destination.exists());
}
