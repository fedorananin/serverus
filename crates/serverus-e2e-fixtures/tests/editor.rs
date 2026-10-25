use serverus_e2e_fixtures::editor::{
    rewrite_scenario_file, FAILURE_EDIT_CONTENT, LOCAL_EDIT_CONTENT, SUCCESS_EDIT_CONTENT,
};

#[test]
fn scenario_editor_rewrites_only_the_named_edit_files() {
    let directory = tempfile::tempdir().unwrap();
    let success = directory.path().join("edit-success.txt");
    let failure = directory.path().join("edit-failure.txt");
    let local = directory.path().join("local-edit.txt");
    let unrelated = directory.path().join("notes.txt");
    for path in [&success, &failure, &local, &unrelated] {
        std::fs::write(path, b"remote original\n").unwrap();
    }

    rewrite_scenario_file(&success).unwrap();
    rewrite_scenario_file(&failure).unwrap();
    rewrite_scenario_file(&local).unwrap();
    let error = rewrite_scenario_file(&unrelated).unwrap_err();

    assert_eq!(std::fs::read(&success).unwrap(), SUCCESS_EDIT_CONTENT);
    assert_eq!(std::fs::read(&failure).unwrap(), FAILURE_EDIT_CONTENT);
    assert_eq!(std::fs::read(&local).unwrap(), LOCAL_EDIT_CONTENT);
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"remote original\n");
    assert!(error.to_string().contains("unsupported scenario edit file"));
}
