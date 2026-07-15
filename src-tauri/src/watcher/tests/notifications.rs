use crate::events::RemoteEditUploadedEvent;

use super::super::{notifications, EditWatcher};

#[test]
fn remote_edit_notifications_are_bounded_and_drained_once() {
    let watcher = EditWatcher::default();
    for index in 0..40 {
        notifications::record(
            &watcher.notifications,
            RemoteEditUploadedEvent {
                name: format!("edit-{index}.txt"),
                remote_path: format!("/edit-{index}.txt"),
                error: None,
            },
        );
    }

    let pending = watcher.take_notifications();
    assert_eq!(pending.len(), 32);
    assert_eq!(pending.first().unwrap().name, "edit-8.txt");
    assert!(watcher.take_notifications().is_empty());
}
