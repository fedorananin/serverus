//! Where a remembered window may reappear.
//!
//! The app can bring its window back at the size and position it had when it
//! last closed. The screens may have changed since: a second monitor was
//! unplugged, a laptop left its desk display, the resolution went down. A
//! window restored blindly would then open off-screen or larger than the
//! screen, so the saved frame is fitted to the monitors present now, and
//! dropped (default placement) when none of them shows any part of it.
//!
//! All rectangles are in the same coordinate space as the platform reports
//! monitor work areas (physical pixels in Tauri).

/// An axis-aligned rectangle: top-left corner plus size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// Area shared with `other`, 0 when they don't overlap.
    fn overlap_area(&self, other: &Rect) -> u64 {
        let left = i64::from(self.x).max(i64::from(other.x));
        let top = i64::from(self.y).max(i64::from(other.y));
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= left || bottom <= top {
            return 0;
        }
        // Both factors are positive and below 2^33, so the product fits.
        (right - left) as u64 * (bottom - top) as u64
    }

    fn right(&self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(&self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
}

/// The frame to restore the window at, given its saved outer frame and the
/// work areas (screen minus menu bar / dock / taskbar) of the monitors
/// attached now.
///
/// The window goes to the monitor showing the largest part of it, shrinks to
/// fit that monitor's work area when it is larger, and slides inside it when
/// it hangs over an edge. `None` — open at the default size and place — when
/// the saved frame is empty or no monitor shows any of it (the monitor it
/// was on is gone).
pub fn fit_to_monitors(saved: Rect, work_areas: &[Rect]) -> Option<Rect> {
    if saved.width == 0 || saved.height == 0 {
        return None;
    }
    let area = work_areas
        .iter()
        .filter(|area| area.width > 0 && area.height > 0)
        .map(|area| (saved.overlap_area(area), area))
        .filter(|(overlap, _)| *overlap > 0)
        // `max_by_key` keeps the last of equals; prefer the first monitor.
        .fold(None::<(u64, &Rect)>, |best, candidate| match best {
            Some(best) if best.0 >= candidate.0 => Some(best),
            _ => Some(candidate),
        })?
        .1;

    let width = saved.width.min(area.width);
    let height = saved.height.min(area.height);
    Some(Rect {
        x: slide_into(saved.x, width, area.x, area.width),
        y: slide_into(saved.y, height, area.y, area.height),
        width,
        height,
    })
}

/// `start` moved the least distance so that `[start, start + len)` lies
/// within `[area_start, area_start + area_len)`; `len <= area_len`.
fn slide_into(start: i32, len: u32, area_start: i32, area_len: u32) -> i32 {
    let latest = i64::from(area_start) + i64::from(area_len) - i64::from(len);
    let clamped = i64::from(start).clamp(i64::from(area_start), latest);
    // Between two valid i32 values (area_start and a point inside the area).
    clamped as i32
}
