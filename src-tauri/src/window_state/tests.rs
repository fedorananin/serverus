use super::{load_from, save_frame_at, set_restore_enabled_at, SavedFrame, WindowStateFile};

fn frame() -> SavedFrame {
    SavedFrame {
        x: 120,
        y: 80,
        width: 1600,
        height: 1056,
        inner_width: 1600,
        inner_height: 1000,
        maximized: false,
    }
}

#[test]
fn a_missing_or_broken_file_means_off() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window.json");
    assert_eq!(load_from(&path), WindowStateFile::default());
    std::fs::write(&path, b"{ not json").unwrap();
    assert!(!load_from(&path).restore);
}

#[test]
fn frames_are_saved_only_while_remembering_is_on() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window.json");

    save_frame_at(&path, Some(frame())).unwrap();
    assert!(!path.exists(), "off by default: nothing is written");

    set_restore_enabled_at(&path, true).unwrap();
    save_frame_at(&path, Some(frame())).unwrap();
    assert_eq!(
        load_from(&path),
        WindowStateFile {
            restore: true,
            frame: Some(frame()),
        }
    );

    // An exit without a tracked frame keeps the one saved before.
    save_frame_at(&path, None).unwrap();
    assert_eq!(load_from(&path).frame, Some(frame()));
}

#[test]
fn turning_remembering_off_forgets_the_frame() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window.json");
    set_restore_enabled_at(&path, true).unwrap();
    save_frame_at(&path, Some(frame())).unwrap();

    set_restore_enabled_at(&path, false).unwrap();
    assert_eq!(load_from(&path), WindowStateFile::default());

    set_restore_enabled_at(&path, true).unwrap();
    assert_eq!(load_from(&path).frame, None);
}

#[test]
fn inner_size_keeps_the_recorded_decoration() {
    let fitted = serverus_domain::window_placement::Rect {
        x: 0,
        y: 50,
        width: 1200,
        height: 800,
    };
    let inner = frame().inner_for(fitted);
    assert_eq!((inner.width, inner.height), (1200, 744));
}
