use super::{idle_timeout, poll_interval};
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
