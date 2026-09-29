//! Getting the user's attention: an agent request that arrives while the
//! vault is locked waits for the user to unlock it.

use std::time::Duration;

use tauri::{AppHandle, Manager, UserAttentionType};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// How long a request waits for the user to unlock the vault.
const UNLOCK_WAIT: Duration = Duration::from_secs(180);

/// Bring the Serverus window forward (without stealing focus) and bounce
/// its Dock icon.
pub fn request_attention(app: &AppHandle, critical: bool) {
    if let Some(window) = app.webview_windows().into_values().next() {
        let _ = window.unminimize();
        let _ = window.show();
        let kind = if critical {
            UserAttentionType::Critical
        } else {
            UserAttentionType::Informational
        };
        let _ = window.request_user_attention(Some(kind));
    }
}

fn unlocked(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .application
        .require_unlocked()
        .is_ok()
}

/// Return once the vault is unlocked, asking the user to unlock it first
/// when it is not.
pub async fn wait_unlocked(app: &AppHandle) -> AppResult<()> {
    if unlocked(app) {
        return Ok(());
    }
    request_attention(app, true);
    let deadline = tokio::time::Instant::now() + UNLOCK_WAIT;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if unlocked(app) {
            return Ok(());
        }
    }
    Err(AppError::Other(
        "Serverus is locked. Ask the user to unlock it (Touch ID or master password), then try again."
            .into(),
    ))
}
