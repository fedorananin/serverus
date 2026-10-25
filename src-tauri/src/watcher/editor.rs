use std::path::Path;

use serverus_domain::open_policy::{open_as, OpenAs, Platform};

use crate::error::{AppError, AppResult};
use crate::vault::model::EditorSettings;

#[cfg(target_os = "macos")]
const PLATFORM: Platform = Platform::MacOs;
#[cfg(target_os = "windows")]
const PLATFORM: Platform = Platform::Windows;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const PLATFORM: Platform = Platform::Linux;

/// Open `path` in the configured editor, or with the system default app.
///
/// The system opener runs programs, scripts and shortcuts the way a double
/// click does; whatever it would execute opens as plain text instead (see
/// [`serverus_domain::open_policy`]). Folders are refused outright, because
/// opening an `.app` bundle launches it.
pub(crate) fn open_in_editor(path: &Path, editor: &EditorSettings) -> AppResult<()> {
    let custom = if editor.use_system_default {
        None
    } else {
        editor.custom_app.as_deref().filter(|app| !app.is_empty())
    };
    let as_text = target_opens_as(path)? == OpenAs::Text;

    #[cfg(all(feature = "scenario-tests", target_os = "macos"))]
    if let Some(app) = custom {
        std::process::Command::new(app)
            .arg(path)
            .spawn()
            .map_err(|error| AppError::Other(format!("open editor: {error}")))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        if let Some(app) = custom {
            command.arg("-a").arg(app);
        } else if as_text {
            command.arg("-t");
        }
        command.arg(path);
        let status = command
            .status()
            .map_err(|error| AppError::Other(format!("open editor: {error}")))?;
        if !status.success() {
            return Err(AppError::Other("editor failed to open the file".into()));
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut command = match custom {
            Some(app) => std::process::Command::new(app),
            #[cfg(target_os = "windows")]
            None if as_text => std::process::Command::new("notepad.exe"),
            #[cfg(target_os = "windows")]
            None => std::process::Command::new("explorer"),
            // Linux has no opener that forces a text editor.
            #[cfg(not(target_os = "windows"))]
            None if as_text => {
                return Err(AppError::Other(
                    "this file would run if opened with the system default app; \
                     choose an editor in Settings → Editor to open it as text"
                        .into(),
                ))
            }
            #[cfg(not(target_os = "windows"))]
            None => std::process::Command::new("xdg-open"),
        };
        command
            .arg(path)
            .spawn()
            .map_err(|error| AppError::Other(format!("open editor: {error}")))?;
        Ok(())
    }
}

/// How the system opener may treat `path`. Links are resolved first: the
/// opener follows them, so the target's name and mode are what count.
pub(super) fn target_opens_as(path: &Path) -> AppResult<OpenAs> {
    let target = std::fs::canonicalize(path)
        .map_err(|error| AppError::Other(format!("open {}: {error}", path.display())))?;
    let meta = std::fs::metadata(&target)
        .map_err(|error| AppError::Other(format!("open {}: {error}", path.display())))?;
    if meta.is_dir() {
        return Err(AppError::Other(
            "folders cannot be opened in an editor".into(),
        ));
    }
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    Ok(open_as(PLATFORM, &name, executable))
}
