use std::time::Duration;

use crate::session::terminal_tap::TerminalTap;

#[test]
fn offsets_stay_absolute_across_eviction() {
    let tap = TerminalTap::default();
    let chunk = vec![b'x'; 1024 * 1024];
    tap.push(&chunk);
    tap.push(&chunk);
    tap.push(b"tail");
    let end = tap.end_offset();
    assert_eq!(end, 2 * 1024 * 1024 + 4);
    tap.with_view(|view| {
        assert!(view.start > 0, "old output was evicted");
        assert_eq!(view.end(), end);
        let (bytes, _) = view.from_offset(end - 4);
        assert_eq!(bytes, b"tail");
        // An evicted offset clamps to what is retained.
        let (bytes, _) = view.from_offset(0);
        assert_eq!(bytes.len() as u64, end - view.start);
    });
}

#[test]
fn the_alternate_screen_state_survives_eviction() {
    let tap = TerminalTap::default();
    tap.push(b"\x1b[?1049h");
    tap.push(&vec![b'~'; 3 * 1024 * 1024]);
    tap.with_view(|view| {
        assert!(view.alt_screen);
        let (_, alt) = view.from_offset(view.start);
        assert!(
            alt,
            "the retained window starts inside the full-screen program"
        );
    });
}

#[test]
fn modes_and_interrupt_keys_are_tracked() {
    let tap = TerminalTap::default();
    tap.push(b"\x1b[?2004h$ ");
    assert!(tap.bracketed_paste());
    let version = tap.version();
    tap.note_input(b"y\r");
    assert!(tap.version() > version, "any input is a change");
    tap.with_view(|view| assert_eq!(view.interrupts, 0));
    tap.note_input(b"\x03");
    tap.note_input(b"\x1a");
    tap.with_view(|view| assert_eq!(view.interrupts, 2));
}

#[test]
fn a_snapshot_is_a_consistent_copy() {
    let tap = TerminalTap::default();
    tap.push(b"\x1b[?1049hfull screen");
    let snapshot = tap.snapshot();
    tap.push(b"\x1b[?1049l more");
    let view = snapshot.view();
    assert_eq!(view.bytes, b"\x1b[?1049hfull screen");
    assert!(view.alt_screen);
}

#[tokio::test]
async fn wait_change_wakes_on_output_and_times_out_otherwise() {
    let tap = std::sync::Arc::new(TerminalTap::default());
    let version = tap.version();
    assert!(!tap.wait_change(version, Duration::from_millis(30)).await);
    let waiter = tokio::spawn({
        let tap = tap.clone();
        async move { tap.wait_change(version, Duration::from_secs(5)).await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    tap.push(b"x");
    assert!(waiter.await.unwrap());
}
