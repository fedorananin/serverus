use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::events::RemoteEditUploadedEvent;

use super::EditWatcher;

const MAX_PENDING_NOTIFICATIONS: usize = 32;

pub(super) fn record(
    notifications: &Arc<Mutex<VecDeque<RemoteEditUploadedEvent>>>,
    event: RemoteEditUploadedEvent,
) {
    let mut notifications = notifications.lock().unwrap();
    if notifications.len() == MAX_PENDING_NOTIFICATIONS {
        notifications.pop_front();
    }
    notifications.push_back(event);
}

impl EditWatcher {
    pub fn take_notifications(&self) -> Vec<RemoteEditUploadedEvent> {
        self.notifications.lock().unwrap().drain(..).collect()
    }

    pub(super) fn clear_notifications(&self) {
        self.notifications.lock().unwrap().clear();
    }
}
