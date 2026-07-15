use std::sync::atomic::Ordering;

use super::super::TransferManager;
use super::support::{context_id, insert, item};

#[test]
fn stopping_emitter_reclaims_work_missed_during_the_release_window() {
    let manager = TransferManager::default();
    manager.activate_context(context_id(1));
    manager.emitter_running.store(true, Ordering::SeqCst);

    // This is the race state: the producer has queued work, but observed the
    // previous emitter owner as running and therefore did not start another.
    insert(&manager, item("session"));

    assert!(manager.release_or_reclaim_emitter());
    assert!(manager.emitter_running.load(Ordering::SeqCst));
}

#[test]
fn idle_emitter_releases_ownership_when_no_work_arrived() {
    let manager = TransferManager::default();
    manager.activate_context(context_id(1));
    manager.emitter_running.store(true, Ordering::SeqCst);

    assert!(!manager.release_or_reclaim_emitter());
    assert!(!manager.emitter_running.load(Ordering::SeqCst));
}
