use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::vault::model::EditorSettings;

pub(super) fn open_in_editor(path: &Path, editor: &EditorSettings) -> AppResult<()> {
    let custom = if editor.use_system_default {
        None
    } else {
        editor.custom_app.as_deref().filter(|app| !app.is_empty())
    };

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
            Some(app) => {
                let mut command = std::process::Command::new(app);
                command.arg(path);
                command
            }
            None => {
                #[cfg(target_os = "windows")]
                let mut command = std::process::Command::new("explorer");
                #[cfg(not(target_os = "windows"))]
                let mut command = std::process::Command::new("xdg-open");
                command.arg(path);
                command
            }
        };
        command
            .spawn()
            .map_err(|error| AppError::Other(format!("open editor: {error}")))?;
        Ok(())
    }
}
