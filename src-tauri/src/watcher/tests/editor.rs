#[cfg(all(feature = "scenario-tests", target_os = "macos"))]
#[test]
fn scenario_custom_editor_executes_a_unix_binary_directly() {
    use super::super::editor::open_in_editor;
    use crate::vault::model::EditorSettings;

    let local = tempfile::NamedTempFile::new().unwrap();
    let editor = EditorSettings {
        use_system_default: false,
        custom_app: Some("/usr/bin/true".into()),
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

#[test]
fn documents_keep_their_default_application() {
    use super::super::editor::target_opens_as;
    use serverus_domain::open_policy::OpenAs;

    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "hello").unwrap();

    assert_eq!(target_opens_as(&notes).unwrap(), OpenAs::Default);
}

#[cfg(unix)]
#[test]
fn a_link_is_judged_by_its_target() {
    use super::super::editor::target_opens_as;
    use serverus_domain::open_policy::OpenAs;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("server");
    std::fs::write(&program, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let link = dir.path().join("notes.txt");
    std::os::unix::fs::symlink(&program, &link).unwrap();

    assert_eq!(target_opens_as(&link).unwrap(), OpenAs::Text);
}
