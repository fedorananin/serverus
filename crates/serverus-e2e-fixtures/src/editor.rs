use std::path::Path;

use anyhow::{bail, Context, Result};

pub const SUCCESS_EDIT_CONTENT: &[u8] = b"edited successfully by scenario editor\n";
pub const FAILURE_EDIT_CONTENT: &[u8] = b"replacement that must never reach remote\n";

/// Behave like a deterministic external editor while still receiving the
/// actual cache path from the desktop application.
pub fn rewrite_scenario_file(path: &Path) -> Result<()> {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        bail!("unsupported scenario edit file");
    };
    let content = match name {
        "edit-success.txt" => SUCCESS_EDIT_CONTENT,
        "edit-failure.txt" => FAILURE_EDIT_CONTENT,
        _ => bail!("unsupported scenario edit file: {name}"),
    };
    std::fs::write(path, content)
        .with_context(|| format!("rewrite scenario edit file {}", path.display()))?;
    Ok(())
}
