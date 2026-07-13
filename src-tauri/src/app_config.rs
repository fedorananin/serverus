//! Tiny plaintext app config: only the pointer to the vault file location.
//!
//! All real settings live *inside* the encrypted vault (SPEC §8); the vault
//! path itself can't (chicken-and-egg), so it is the single exception. It
//! contains no secrets.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub vault_path: Option<String>,
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Serverus")
}

fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn default_vault_path() -> PathBuf {
    config_dir().join("default.serverus")
}

pub fn load() -> AppConfig {
    fs::read(config_file())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(config: &AppConfig) -> std::io::Result<()> {
    fs::create_dir_all(config_dir())?;
    let json = serde_json::to_vec_pretty(config).expect("config serializes");
    fs::write(config_file(), json)
}

/// The vault path currently in effect.
pub fn vault_path() -> PathBuf {
    load()
        .vault_path
        .map(PathBuf::from)
        .unwrap_or_else(default_vault_path)
}
