use std::path::{Path, PathBuf};

use serverus_domain::open_policy::{choose_opener, Opener, Platform};

use crate::error::{AppError, AppResult};
use crate::vault::model::EditorSettings;

mod association;

#[cfg(target_os = "macos")]
const PLATFORM: Platform = Platform::MacOs;
#[cfg(target_os = "windows")]
const PLATFORM: Platform = Platform::Windows;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const PLATFORM: Platform = Platform::Linux;

/// What actually gets started for a file.
#[derive(Debug, PartialEq, Eq)]
enum Launch<'a> {
    /// A named application (the custom editor or the fallback editor).
    App(&'a str),
    /// The system opener; `as_text` forces its text editor.
    System { as_text: bool },
}

/// Open `path` in the configured editor, or with the system default app.
///
/// The system opener runs programs, scripts and shortcuts the way a double
/// click does; whatever it would execute opens as plain text instead (see
/// [`serverus_domain::open_policy`]). Files without an extension or without
/// an app for their type go to the fallback editor from Settings → Editor,
/// or the system text editor. Folders are refused outright, because opening
/// an `.app` bundle launches it.
pub(crate) fn open_in_editor(path: &Path, editor: &EditorSettings) -> AppResult<()> {
    let target = resolve_file(path)?;
    let custom = if editor.use_system_default {
        None
    } else {
        app_name(&editor.custom_app)
    };
    let launch = match custom {
        Some(app) => Launch::App(app),
        None => {
            let fallback = app_name(&editor.fallback_app);
            match opener_for(&target, fallback.is_some()) {
                Opener::SystemDefault => Launch::System { as_text: false },
                Opener::SystemText => Launch::System { as_text: true },
                // Only chosen when a fallback is set.
                Opener::Fallback => fallback.map_or(Launch::System { as_text: true }, Launch::App),
            }
        }
    };
    start(path, launch)
}

fn app_name(name: &Option<String>) -> Option<&str> {
    name.as_deref().map(str::trim).filter(|app| !app.is_empty())
}

#[cfg(target_os = "macos")]
fn start(path: &Path, launch: Launch<'_>) -> AppResult<()> {
    #[cfg(feature = "scenario-tests")]
    if let Launch::App(app) = launch {
        std::process::Command::new(app)
            .arg(path)
            .spawn()
            .map_err(|error| AppError::Other(format!("open editor: {error}")))?;
        return Ok(());
    }

    let mut command = std::process::Command::new("open");
    match launch {
        Launch::App(app) => {
            command.arg("-a").arg(app);
        }
        Launch::System { as_text: true } => {
            command.arg("-t");
        }
        Launch::System { as_text: false } => {}
    }
    let status = command
        .arg(path)
        .status()
        .map_err(|error| AppError::Other(format!("open editor: {error}")))?;
    if !status.success() {
        return Err(AppError::Other("editor failed to open the file".into()));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn start(path: &Path, launch: Launch<'_>) -> AppResult<()> {
    let mut command = match launch {
        Launch::App(app) => std::process::Command::new(app),
        #[cfg(target_os = "windows")]
        Launch::System { as_text: true } => std::process::Command::new("notepad.exe"),
        #[cfg(target_os = "windows")]
        Launch::System { as_text: false } => std::process::Command::new("explorer"),
        // Linux has no opener that forces a text editor.
        #[cfg(not(target_os = "windows"))]
        Launch::System { as_text: true } => {
            return Err(AppError::Other(
                "the system can't safely open this file (it would run, or no app is \
                 set for it); choose an editor for such files in Settings → Editor"
                    .into(),
            ))
        }
        #[cfg(not(target_os = "windows"))]
        Launch::System { as_text: false } => std::process::Command::new("xdg-open"),
    };
    command
        .arg(path)
        .spawn()
        .map_err(|error| AppError::Other(format!("open editor: {error}")))?;
    Ok(())
}

/// The file the opener would actually get: links resolved (the opener
/// follows them, so the target's name and mode are what count), folders
/// refused.
fn resolve_file(path: &Path) -> AppResult<(PathBuf, std::fs::Metadata)> {
    let target = std::fs::canonicalize(path)
        .map_err(|error| AppError::Other(format!("open {}: {error}", path.display())))?;
    let meta = std::fs::metadata(&target)
        .map_err(|error| AppError::Other(format!("open {}: {error}", path.display())))?;
    if meta.is_dir() {
        return Err(AppError::Other(
            "folders cannot be opened in an editor".into(),
        ));
    }
    Ok((target, meta))
}

/// Who opens the resolved file while the system default app is on.
pub(super) fn opener_for(
    (target, meta): &(PathBuf, std::fs::Metadata),
    fallback_configured: bool,
) -> Opener {
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = {
        let _ = meta;
        false
    };
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    choose_opener(PLATFORM, &name, executable, fallback_configured, || {
        association::has_default_app(target)
    })
}

/// [`opener_for`] from a path, for tests.
#[cfg(test)]
pub(super) fn path_opener(path: &Path, fallback_configured: bool) -> AppResult<Opener> {
    Ok(opener_for(&resolve_file(path)?, fallback_configured))
}
