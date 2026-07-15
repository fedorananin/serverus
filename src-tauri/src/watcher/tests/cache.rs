use super::super::cache::{
    create_private_cache_file, create_private_edit_dir, validate_edit_filename, PendingCacheDir,
};

#[test]
fn edit_cache_filename_rejects_portable_path_escapes() {
    for unsafe_name in [
        "",
        ".",
        "..",
        "../secret",
        r"..\secret",
        r"C:\secret",
        "name:stream",
        "CON",
        "aux.txt",
        "COM1.log",
        "LPT9",
        "trailing.",
        "trailing ",
        "wild*card",
        "line\nbreak",
    ] {
        assert!(
            validate_edit_filename(unsafe_name).is_err(),
            "accepted unsafe edit filename: {unsafe_name:?}"
        );
    }
    for safe_name in ["config.yml", ".env", "résumé.txt"] {
        validate_edit_filename(safe_name).unwrap();
    }
}

#[tokio::test]
async fn edit_cache_uses_private_permissions_and_exclusive_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let dir = create_private_edit_dir(&root).unwrap();
    let path = dir.join("config.txt");
    let file = create_private_cache_file(&path).await.unwrap();
    drop(file);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    std::fs::write(&path, b"keep me").unwrap();
    assert!(create_private_cache_file(&path).await.is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"keep me");
}

#[test]
fn pending_cache_cleanup_removes_partial_plaintext() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let dir = create_private_edit_dir(&root).unwrap();
    std::fs::write(dir.join("partial.txt"), b"secret").unwrap();

    drop(PendingCacheDir::new(dir.clone()));

    assert!(!dir.exists());
}

#[cfg(unix)]
#[test]
fn edit_cache_refuses_a_symlinked_root() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let root = temp.path().join("cache");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&target, &root).unwrap();

    assert!(create_private_edit_dir(&root).is_err());
    assert_eq!(
        std::fs::metadata(target).unwrap().permissions().mode() & 0o777,
        0o755
    );
}
