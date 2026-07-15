//! Tiny plaintext app config: only the pointer to the vault file location.
//!
//! All real settings live *inside* the encrypted vault (SPEC §8); the vault
//! path itself can't (chicken-and-egg), so it is the single exception. It
//! contains no secrets.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(feature = "scenario-tests")]
use std::ffi::OsString;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub vault_path: Option<String>,
}

pub fn config_dir() -> PathBuf {
    #[cfg(feature = "scenario-tests")]
    {
        required_scenario_config_dir(std::env::var_os("SERVERUS_SCENARIO_CONFIG_DIR"))
    }

    #[cfg(not(feature = "scenario-tests"))]
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Serverus")
}

#[cfg(feature = "scenario-tests")]
fn required_scenario_config_dir(value: Option<OsString>) -> PathBuf {
    let value = value
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| panic!("SERVERUS_SCENARIO_CONFIG_DIR must be set and non-empty"));
    PathBuf::from(value)
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
    save_to(&config_file(), config)
}

fn save_to(path: &Path, config: &AppConfig) -> std::io::Result<()> {
    save_to_with(path, config, replace_config_file)
}

fn save_to_with(
    path: &Path,
    config: &AppConfig,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let json = serde_json::to_vec_pretty(config).expect("config serializes");
    let mut temp_name = path.file_name().unwrap_or_default().to_os_string();
    temp_name.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let temp = path.with_file_name(temp_name);

    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&json)?;
        file.sync_all()?;
        drop(file);
        replace(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn replace_config_file(temp: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(temp, target)?;
    if let Some(parent) = target.parent() {
        if let Ok(directory) = fs::File::open(parent) {
            // The rename is already committed; a directory that does not
            // support fsync must not make memory and config disagree.
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

#[cfg(windows)]
fn replace_config_file(temp: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let temp_wide: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(temp_wide.as_ptr()),
            PCWSTR(target_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

/// The vault path currently in effect.
pub fn vault_path() -> PathBuf {
    load()
        .vault_path
        .map(PathBuf::from)
        .unwrap_or_else(default_vault_path)
}

#[cfg(test)]
mod tests;
