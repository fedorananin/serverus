//! Auto-lock (SPEC §2.4): lock the vault after a period of inactivity and
//! when the Mac slept. Open network sessions are left alone — only the DEK
//! and decrypted payload are wiped.
//!
//! Sleep detection compares the monotonic clock (pauses during sleep on
//! macOS) with the wall clock; a divergence means the machine was asleep.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use tauri::Manager;
use tauri_specta::Event;

use crate::events::VaultLockedEvent;
use crate::state::AppState;

pub struct ActivityTracker {
    pub last_activity: Mutex<Instant>,
}

impl Default for ActivityTracker {
    fn default() -> Self {
        ActivityTracker {
            last_activity: Mutex::new(Instant::now()),
        }
    }
}

impl ActivityTracker {
    pub fn touch(&self) {
        *self.last_activity.lock().unwrap() = Instant::now();
    }
}

pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut prev_wall = SystemTime::now();
        let mut prev_mono = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let state = app.state::<AppState>();

            let (unlocked, timeout_min, lock_on_sleep) = {
                let mgr = state.vault.lock().unwrap();
                match mgr.payload() {
                    Ok(p) => (
                        true,
                        p.settings.security.auto_lock_minutes,
                        p.settings.security.lock_on_sleep,
                    ),
                    Err(_) => (false, 0, false),
                }
            };

            // Clock divergence → the machine slept in between.
            let wall_delta = SystemTime::now()
                .duration_since(prev_wall)
                .unwrap_or_default();
            let mono_delta = prev_mono.elapsed();
            let slept = wall_delta > mono_delta + Duration::from_secs(60);
            prev_wall = SystemTime::now();
            prev_mono = Instant::now();

            if !unlocked {
                continue;
            }

            let idle = state.activity.last_activity.lock().unwrap().elapsed();
            let idle_timeout =
                timeout_min > 0 && idle >= Duration::from_secs(timeout_min as u64 * 60);
            if idle_timeout || (slept && lock_on_sleep) {
                state.vault.lock().unwrap().lock();
                let _ = VaultLockedEvent.emit(&app);
            }
        }
    });
}
