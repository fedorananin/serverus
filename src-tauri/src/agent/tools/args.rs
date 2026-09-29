//! Tool-argument parsing shared by the tools.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use super::ToolError;
use crate::vault::model::ConflictPolicy;

pub fn parse<T: DeserializeOwned>(arguments: Value) -> Result<T, ToolError> {
    serde_json::from_value(arguments)
        .map_err(|error| ToolError(format!("Invalid arguments: {error}")))
}

/// What to do when a transfer target already exists.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IfExists {
    #[default]
    Overwrite,
    Skip,
    Rename,
}

impl IfExists {
    pub fn policy(self) -> ConflictPolicy {
        match self {
            IfExists::Overwrite => ConflictPolicy::Overwrite,
            IfExists::Skip => ConflictPolicy::Skip,
            IfExists::Rename => ConflictPolicy::Rename,
        }
    }
}

/// A remote path argument: non-empty, trimmed.
pub fn remote_path(path: &str) -> Result<String, ToolError> {
    let path = path.trim();
    if path.is_empty() {
        return Err("A remote path is empty.".into());
    }
    Ok(path.to_string())
}

/// A local path argument: `~` expanded, must be absolute.
pub fn local_path(path: &str) -> Result<PathBuf, ToolError> {
    let expanded = crate::local_fs::expand(path.trim());
    if !expanded.is_absolute() {
        return Err(ToolError(format!(
            "`{path}` is not an absolute local path."
        )));
    }
    Ok(expanded)
}

pub fn default_wait() -> u64 {
    600
}

pub fn default_tree_wait() -> u64 {
    300
}
