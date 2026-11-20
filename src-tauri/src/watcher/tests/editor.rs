#[cfg(all(feature = "scenario-tests", target_os = "macos"))]
#[test]
fn scenario_custom_editor_executes_a_unix_binary_directly() {
    use super::super::editor::open_in_editor;
    use crate::vault::model::EditorSettings;

    let local = tempfile::NamedTempFile::new().unwrap();
    let editor = EditorSettings {
        use_system_default: false,
        custom_app: Some("/usr/bin/true".into()),
        fallback_app: None,
    };

    open_in_editor(local.path(), &editor).unwrap();
}

#[test]
fn folders_are_never_handed_to_the_opener() {
    use super::super::editor::open_in_editor;
    use crate::vault::model::EditorSettings;

    // Opening a folder named like a bundle would launch it.
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Calculator.app");
    std::fs::create_dir(&bundle).unwrap();

    let error = open_in_editor(&bundle, &EditorSettings::default()).unwrap_err();
    assert!(error.to_string().contains("folders"), "{error}");
}

/// Not on Linux: a headless CI desktop may have no handler for text/plain.
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn documents_keep_their_default_application() {
    use super::super::editor::path_opener;
    use serverus_domain::open_policy::Opener;

    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "hello").unwrap();

    // Even with a fallback editor set: it is only for files nothing opens.
    assert_eq!(path_opener(&notes, true).unwrap(), Opener::SystemDefault);
}

#[test]
fn files_without_an_extension_go_to_the_fallback_editor() {
    use super::super::editor::path_opener;
    use serverus_domain::open_policy::Opener;

    let dir = tempfile::tempdir().unwrap();
    for name in ["Makefile", ".env"] {
        let file = dir.path().join(name);
        std::fs::write(&file, "x").unwrap();
        assert_eq!(
            path_opener(&file, true).unwrap(),
            Opener::Fallback,
            "{name}"
        );
        assert_eq!(
            path_opener(&file, false).unwrap(),
            Opener::SystemText,
            "{name}"
        );
    }
}

/// Real Launch Services lookup: nothing is registered for a made-up type.
#[cfg(target_os = "macos")]
#[test]
fn a_type_no_app_claims_goes_to_the_fallback_editor() {
    use super::super::editor::path_opener;
    use serverus_domain::open_policy::Opener;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("dump.serverus-unclaimed-xq7");
    std::fs::write(&file, "x").unwrap();
    assert_eq!(path_opener(&file, true).unwrap(), Opener::Fallback);
}

#[cfg(unix)]
#[test]
fn a_link_is_judged_by_its_target() {
    use super::super::editor::path_opener;
    use serverus_domain::open_policy::Opener;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("server");
    std::fs::write(&program, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let link = dir.path().join("notes.txt");
    std::os::unix::fs::symlink(&program, &link).unwrap();

    assert_eq!(path_opener(&link, false).unwrap(), Opener::SystemText);
    assert_eq!(path_opener(&link, true).unwrap(), Opener::Fallback);
}
