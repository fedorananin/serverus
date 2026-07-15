//! Miscellaneous desktop commands.

use super::prelude::*;

/// Open an http(s) URL in the user's default browser (used by the About
/// section). Each OS opener delegates and returns immediately; the URL is
/// passed as a single argument, never through a shell.
#[tauri::command]
#[specta::specta]
pub async fn open_external(url: String) -> ApiResult<()> {
    blocking(move || {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(AppError::Other("only http(s) URLs may be opened".into()));
        }
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(target_os = "windows")]
        let program = "explorer";
        #[cfg(all(unix, not(target_os = "macos")))]
        let program = "xdg-open";
        std::process::Command::new(program)
            .arg(&url)
            .spawn()
            .map_err(|e| AppError::Other(format!("failed to open URL: {e}")))?;
        Ok(())
    })
    .await
}
