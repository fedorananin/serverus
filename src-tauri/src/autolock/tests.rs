use super::{idle_timeout, poll_interval, should_lock, ActivityTracker};
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(feature = "scenario-tests"))]
#[test]
fn production_timing_keeps_minute_settings_and_ten_second_polling() {
    assert_eq!(poll_interval(), Duration::from_secs(10));
    assert_eq!(idle_timeout(1), Duration::from_secs(60));
}

#[cfg(feature = "scenario-tests")]
#[test]
fn scenario_timing_accelerates_only_the_one_minute_setting() {
    assert_eq!(poll_interval(), Duration::from_millis(200));
    assert_eq!(idle_timeout(1), Duration::from_secs(4));
    assert_eq!(idle_timeout(2), Duration::from_secs(120));
}

#[test]
fn idle_timeout_and_sleep_lock_when_nothing_holds() {
    let long = idle_timeout(15) + Duration::from_secs(1);
    assert!(should_lock(long, 15, false, false));
    assert!(should_lock(Duration::ZERO, 15, true, false));
    assert!(!should_lock(Duration::ZERO, 15, false, false));
    // 0 minutes = never lock on idle.
    assert!(!should_lock(long, 0, false, false));
}

#[test]
fn an_agent_hold_vetoes_idle_and_sleep_locks() {
    let long = idle_timeout(15) + Duration::from_secs(1);
    assert!(!should_lock(long, 15, true, true));
}

#[test]
fn holds_nest_and_release_on_drop() {
    let tracker = Arc::new(ActivityTracker::default());
    assert!(!tracker.is_held());
    let outer = tracker.hold();
    let inner = tracker.hold();
    drop(outer);
    assert!(tracker.is_held());
    drop(inner);
    assert!(!tracker.is_held());
}
