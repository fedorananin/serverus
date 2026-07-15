use std::time::Duration;

use super::super::tree_size::scenario_chunk_delay;

#[test]
fn only_the_scenario_cleanup_fixture_is_paced() {
    assert_eq!(
        scenario_chunk_delay("cleanup-slow.bin"),
        Some(Duration::from_millis(200))
    );
    assert_eq!(scenario_chunk_delay("index.html"), None);
}
