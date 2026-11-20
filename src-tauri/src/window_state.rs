//! Remembering the main window's size and position across restarts.
//!
//! Off by default: the window opens at the size and place `tauri.conf.json`
//! gives it. With "Remember window size and position" on (Settings →
//! Appearance), the frame the window had when the app quit comes back at the
//! next start — fitted to the monitors present then
//! ([`serverus_domain::window_placement`]), so an unplugged display or a
//! smaller screen never leaves the window off-screen or oversized.
//!
//! The switch and the frame live in `window.json` beside `config.json`, not
//! in the vault: the window opens before anything is unlocked. Window
//! geometry is not a secret.
//!
//! The window starts hidden (`visible: false` in `tauri.conf.json`) and is
//! shown once placed, so it never visibly jumps from the default spot.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serverus_domain::window_placement::{fit_to_monitors, Rect};
use tauri::{Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// The on-disk state. Unknown or broken files read as the default (off).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowStateFile {
    pub restore: bool,
    pub frame: Option<SavedFrame>,
}

/// The window's last normal (not minimized, not fullscreen) frame, in
/// physical pixels. `x`/`y`/`width`/`height` are the outer frame, title bar
/// included — what has to fit on a screen; `inner_*` is the content size,
/// which is what `set_size` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedFrame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub inner_width: u32,
    pub inner_height: u32,
    pub maximized: bool,
}

impl SavedFrame {
    fn outer(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }

    /// The content size that gives an outer frame of `outer`, keeping the
    /// decoration size recorded with this frame.
    fn inner_for(&self, outer: Rect) -> PhysicalSize<u32> {
        let decoration_w = self.width.saturating_sub(self.inner_width);
        let decoration_h = self.height.saturating_sub(self.inner_height);
        PhysicalSize::new(
            outer.width.saturating_sub(decoration_w),
            outer.height.saturating_sub(decoration_h),
        )
    }
}

fn state_file() -> PathBuf {
    crate::app_config::config_dir().join("window.json")
}

pub(crate) fn load_from(path: &Path) -> WindowStateFile {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Whether the window frame is remembered across restarts.
pub fn restore_enabled() -> bool {
    load_from(&state_file()).restore
}

/// Turn remembering on or off. Turning it off forgets the saved frame, so
/// turning it back on later starts from the window as it is then.
pub fn set_restore_enabled(enabled: bool) -> std::io::Result<()> {
    set_restore_enabled_at(&state_file(), enabled)
}

pub(crate) fn set_restore_enabled_at(path: &Path, enabled: bool) -> std::io::Result<()> {
    let mut state = load_from(path);
    state.restore = enabled;
    if !enabled {
        state.frame = None;
    }
    crate::app_config::write_json(path, &state)
}

/// Write `frame` as the one to restore — only while remembering is on.
pub(crate) fn save_frame_at(path: &Path, frame: Option<SavedFrame>) -> std::io::Result<()> {
    let state = load_from(path);
    if !state.restore || frame.is_none() {
        return Ok(());
    }
    crate::app_config::write_json(path, &WindowStateFile { frame, ..state })
}

/// The latest normal frame of the main window, kept in memory as it moves
/// and written out once, at exit.
#[derive(Default)]
pub struct WindowTracker {
    last: Mutex<Option<SavedFrame>>,
}

impl WindowTracker {
    /// Note the window's current frame. Minimized and fullscreen states are
    /// skipped (their frame isn't one to come back to); a maximized window
    /// keeps the normal frame from before and only flags itself maximized.
    pub fn record<R: Runtime>(&self, window: &WebviewWindow<R>) {
        if window.is_minimized().unwrap_or(true) || window.is_fullscreen().unwrap_or(true) {
            return;
        }
        let maximized = window.is_maximized().unwrap_or(false);
        let mut last = self.last.lock().unwrap_or_else(|error| error.into_inner());
        if maximized {
            if let Some(frame) = last.as_mut() {
                frame.maximized = true;
                return;
            }
        }
        let (Ok(position), Ok(outer), Ok(inner)) = (
            window.outer_position(),
            window.outer_size(),
            window.inner_size(),
        ) else {
            return;
        };
        *last = Some(SavedFrame {
            x: position.x,
            y: position.y,
            width: outer.width,
            height: outer.height,
            inner_width: inner.width,
            inner_height: inner.height,
            maximized,
        });
    }

    fn snapshot(&self) -> Option<SavedFrame> {
        *self.last.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// Put the main window where it was last time (when remembering is on and
/// a monitor still shows that spot), then show it. Called once from setup.
pub fn restore_and_show<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let state = load_from(&state_file());
    if let (true, Some(frame)) = (state.restore, state.frame) {
        place(&window, frame);
    }
    let _ = window.show();
    app.state::<WindowTracker>().record(&window);
}

fn place<R: Runtime>(window: &WebviewWindow<R>, frame: SavedFrame) {
    let work_areas: Vec<Rect> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            Rect {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
            }
        })
        .collect();
    let Some(fitted) = fit_to_monitors(frame.outer(), &work_areas) else {
        return;
    };
    let _ = window.set_size(frame.inner_for(fitted));
    let _ = window.set_position(PhysicalPosition::new(fitted.x, fitted.y));
    if frame.maximized {
        let _ = window.maximize();
    }
}

/// Track a move or resize of the main window.
pub fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if !matches!(
        event,
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
    ) || window.label() != "main"
    {
        return;
    }
    if let Some(window) = window.app_handle().get_webview_window("main") {
        window.app_handle().state::<WindowTracker>().record(&window);
    }
}

/// Persist the tracked frame (when remembering is on). Called at exit.
pub fn save_on_exit<R: Runtime>(app: &tauri::AppHandle<R>) {
    let frame = app.state::<WindowTracker>().snapshot();
    let _ = save_frame_at(&state_file(), frame);
}

#[cfg(test)]
mod tests;
