use serverus_e2e_fixtures::workspace::FixtureWorkspace;

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
