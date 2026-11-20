use serverus_domain::window_placement::{fit_to_monitors, Rect};

fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// A 2560×1600 laptop work area below a 50 px menu bar.
const LAPTOP: Rect = Rect {
    x: 0,
    y: 50,
    width: 2560,
    height: 1550,
};

/// A 3840×2160 display to the right of the laptop.
const EXTERNAL: Rect = Rect {
    x: 2560,
    y: 0,
    width: 3840,
    height: 2160,
};

#[test]
fn a_window_that_still_fits_comes_back_unchanged() {
    let saved = rect(200, 120, 1600, 1000);
    assert_eq!(fit_to_monitors(saved, &[LAPTOP]), Some(saved));
    assert_eq!(fit_to_monitors(saved, &[EXTERNAL, LAPTOP]), Some(saved));
}

#[test]
fn a_window_on_an_unplugged_monitor_gets_the_default_placement() {
    let on_external = rect(3000, 200, 1600, 1000);
    assert_eq!(fit_to_monitors(on_external, &[LAPTOP]), None);
    assert_eq!(fit_to_monitors(on_external, &[]), None);
}

#[test]
fn a_window_larger_than_the_screen_shrinks_to_the_work_area() {
    let huge = rect(0, 0, 3600, 2100);
    assert_eq!(fit_to_monitors(huge, &[LAPTOP]), Some(LAPTOP));
}

#[test]
fn a_window_hanging_over_an_edge_slides_back_inside() {
    // Mostly on the laptop, its right part beyond the (now gone) external.
    let saved = rect(2000, 1200, 1600, 1000);
    assert_eq!(
        fit_to_monitors(saved, &[LAPTOP]),
        Some(rect(960, 600, 1600, 1000))
    );
    // Dragged up under the menu bar and left past the edge.
    assert_eq!(
        fit_to_monitors(rect(-300, 0, 1600, 1000), &[LAPTOP]),
        Some(rect(0, 50, 1600, 1000))
    );
}

#[test]
fn a_window_spanning_two_monitors_goes_to_the_one_showing_more_of_it() {
    // 1000 px of its width on the external display, 600 on the laptop.
    let saved = rect(1960, 300, 1600, 1000);
    assert_eq!(
        fit_to_monitors(saved, &[LAPTOP, EXTERNAL]),
        Some(rect(2560, 300, 1600, 1000))
    );
}

#[test]
fn equal_overlap_prefers_the_first_monitor() {
    let saved = rect(1760, 300, 1600, 1000);
    let left = rect(0, 0, 2560, 2160);
    let right = rect(2560, 0, 2560, 2160);
    assert_eq!(
        fit_to_monitors(saved, &[left, right]),
        Some(rect(960, 300, 1600, 1000))
    );
}

#[test]
fn monitors_left_or_above_the_primary_have_negative_coordinates() {
    let above = rect(-640, -1440, 2560, 1440);
    let saved = rect(-500, -1300, 1600, 1000);
    assert_eq!(fit_to_monitors(saved, &[LAPTOP, above]), Some(saved));
}

#[test]
fn degenerate_frames_and_monitors_are_ignored() {
    assert_eq!(fit_to_monitors(rect(10, 10, 0, 600), &[LAPTOP]), None);
    assert_eq!(
        fit_to_monitors(rect(10, 60, 800, 600), &[rect(0, 0, 0, 0)]),
        None
    );
}

#[test]
fn extreme_coordinates_do_not_overflow() {
    let saved = rect(i32::MAX - 10, i32::MAX - 10, u32::MAX, u32::MAX);
    assert_eq!(fit_to_monitors(saved, &[LAPTOP]), None);
    let everywhere = rect(i32::MIN, i32::MIN, u32::MAX, u32::MAX);
    assert_eq!(fit_to_monitors(everywhere, &[LAPTOP]), Some(LAPTOP));
}
