use super::*;

#[test]
fn sidebar_width_defaults_when_absent() {
    // Vaults written before the sidebar was resizable have no field.
    let panels: PanelSettings = serde_json::from_str(
        r#"{"show_hidden":false,"size_format":"kib","default_local_dir":null}"#,
    )
    .unwrap();
    assert_eq!(panels.sidebar_width, SIDEBAR_WIDTH_DEFAULT);
}

#[test]
fn appearance_defaults_to_system_for_older_vaults() {
    let mut value = serde_json::to_value(Settings::default()).unwrap();
    value.as_object_mut().unwrap().remove("appearance");

    let settings: Settings = serde_json::from_value(value).unwrap();

    assert_eq!(settings.appearance.theme, ThemePreference::System);
}

#[test]
fn folders_from_older_vaults_are_expanded() {
    // Vaults written before folders remembered their disclosure state.
    let node: TreeNode =
        serde_json::from_str(r#"{"type":"folder","id":"f1","name":"Prod"}"#).unwrap();
    assert!(matches!(
        node,
        TreeNode::Folder {
            collapsed: false,
            ..
        }
    ));
}

#[test]
fn clamp_forces_sidebar_width_into_range() {
    let mut settings = Settings::default();

    settings.panels.sidebar_width = 5000;
    settings.clamp();
    assert_eq!(settings.panels.sidebar_width, SIDEBAR_WIDTH_MAX);

    settings.panels.sidebar_width = 0;
    settings.clamp();
    assert_eq!(settings.panels.sidebar_width, SIDEBAR_WIDTH_MIN);

    settings.panels.sidebar_width = 300;
    settings.clamp();
    assert_eq!(settings.panels.sidebar_width, 300);
}
