use super::{save_to_with, AppConfig};

#[cfg(feature = "scenario-tests")]
use super::required_scenario_config_dir;
#[cfg(feature = "scenario-tests")]
use std::ffi::OsString;
#[cfg(feature = "scenario-tests")]
use std::path::PathBuf;

#[cfg(feature = "scenario-tests")]
#[test]
fn scenario_config_dir_uses_the_explicit_isolated_directory() {
    let directory = OsString::from("/tmp/serverus-scenario-config");

    assert_eq!(
        required_scenario_config_dir(Some(directory)),
        PathBuf::from("/tmp/serverus-scenario-config")
    );
}

#[cfg(feature = "scenario-tests")]
#[test]
#[should_panic(expected = "SERVERUS_SCENARIO_CONFIG_DIR must be set")]
fn scenario_config_dir_fails_closed_when_the_variable_is_missing() {
    required_scenario_config_dir(None);
}

#[cfg(feature = "scenario-tests")]
#[test]
#[should_panic(expected = "SERVERUS_SCENARIO_CONFIG_DIR must be set")]
fn scenario_config_dir_fails_closed_when_the_variable_is_empty() {
    required_scenario_config_dir(Some(OsString::new()));
}

#[test]
fn failed_atomic_replace_preserves_the_previous_config() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    std::fs::write(&path, b"previous config").unwrap();
    let config = AppConfig {
        vault_path: Some("/new/vault.serverus".into()),
    };

    let result = save_to_with(&path, &config, |temp, target| {
        assert!(temp.is_file());
        assert_eq!(target, path);
        Err(std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "simulated replace failure",
        ))
    });

    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous config");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
