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
