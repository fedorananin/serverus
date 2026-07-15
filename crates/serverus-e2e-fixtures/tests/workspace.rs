use serverus_e2e_fixtures::workspace::FixtureWorkspace;
use std::time::{Duration, SystemTime};

#[test]
fn workspace_seeds_local_and_remote_nested_trees() {
    let workspace = FixtureWorkspace::create().unwrap();
    let paths = workspace.paths();

    assert!(paths.workspace_root.is_absolute());
    assert_eq!(
        std::fs::read_to_string(paths.local_source.join("site/index.html")).unwrap(),
        "<main>Serverus E2E</main>\n"
    );
    assert_eq!(
        std::fs::read_to_string(paths.local_source.join("conflicts/overwrite.txt")).unwrap(),
        "local overwrite\n"
    );
    assert_eq!(
        std::fs::read_to_string(paths.ftp_root.join("conflicts/overwrite.txt")).unwrap(),
        "remote original\n"
    );
    assert_eq!(
        std::fs::read_to_string(paths.ftp_root.join("edit-success.txt")).unwrap(),
        "remote success original\n"
    );
    assert_eq!(
        std::fs::read_to_string(paths.ftp_root.join("edit-failure.txt")).unwrap(),
        "remote failure original\n"
    );
    assert_eq!(
        std::fs::metadata(paths.ftp_root.join("conflicts/resume.bin"))
            .unwrap()
            .len(),
        524_288
    );
    for root in [&paths.ftp_root, &paths.s3_root, &paths.ssh_root] {
        assert_eq!(
            std::fs::read_to_string(root.join("serverus-e2e/site/nested/readme.txt")).unwrap(),
            "nested fixture\n"
        );
    }
    assert_eq!(
        std::fs::metadata(paths.ssh_root.join("serverus-e2e/site/cleanup-slow.bin"))
            .unwrap()
            .len(),
        4 * 1024 * 1024
    );
    assert!(!paths
        .ftp_root
        .join("serverus-e2e/site/cleanup-slow.bin")
        .exists());
    assert!(!paths
        .s3_root
        .join("serverus-e2e/site/cleanup-slow.bin")
        .exists());
    assert!(paths.app_config_dir.is_dir());
    assert!(paths.vault_dir.is_dir());
    assert!(paths.local_download.is_dir());
}

#[test]
fn workspace_seeds_a_deterministic_directory_comparison_matrix() {
    let workspace = FixtureWorkspace::create().unwrap();
    let paths = workspace.paths();
    let local = paths.local_source.join("directory-comparison");
    let remote = paths.ftp_root.join("directory-comparison");

    assert_eq!(
        std::fs::read_to_string(local.join("identical.txt")).unwrap(),
        std::fs::read_to_string(remote.join("identical.txt")).unwrap()
    );
    assert_ne!(
        std::fs::metadata(local.join("different-size.txt"))
            .unwrap()
            .len(),
        std::fs::metadata(remote.join("different-size.txt"))
            .unwrap()
            .len()
    );
    assert!(local.join("type-changed").is_file());
    assert!(remote.join("type-changed").is_dir());
    assert!(local.join("shared-folder").is_dir());
    assert!(remote.join("shared-folder").is_dir());
    assert!(local.join("only-local.txt").is_file());
    assert!(!remote.join("only-local.txt").exists());
    assert!(!local.join("only-remote.txt").exists());
    assert!(remote.join("only-remote.txt").is_file());

    let identical_local = std::fs::metadata(local.join("identical.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let identical_remote = std::fs::metadata(remote.join("identical.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(identical_local, identical_remote);
    assert!(
        SystemTime::now()
            .duration_since(identical_local)
            .unwrap_or_default()
            < Duration::from_secs(120),
        "the matching timestamp must stay in the current FTP LIST minute"
    );
    let dated_local = std::fs::metadata(local.join("different-date.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let dated_remote = std::fs::metadata(remote.join("different-date.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        std::fs::read(local.join("different-date.txt")).unwrap(),
        std::fs::read(remote.join("different-date.txt")).unwrap()
    );
    assert_ne!(dated_local, dated_remote);
}
