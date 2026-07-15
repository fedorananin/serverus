use std::path::PathBuf;

use serverus_e2e_fixtures::manifest::{FixtureManifest, SshManifest};
use serverus_e2e_fixtures::workspace::FixturePaths;

fn paths() -> FixturePaths {
    FixturePaths {
        workspace_root: PathBuf::from("/tmp/serverus-e2e"),
        app_config_dir: PathBuf::from("/tmp/serverus-e2e/app-config"),
        vault_dir: PathBuf::from("/tmp/serverus-e2e/vaults"),
        local_source: PathBuf::from("/tmp/serverus-e2e/local-source"),
        local_download: PathBuf::from("/tmp/serverus-e2e/local-download"),
        ftp_root: PathBuf::from("/tmp/serverus-e2e/ftp-root"),
        s3_root: PathBuf::from("/tmp/serverus-e2e/s3-root"),
        ssh_root: PathBuf::from("/tmp/serverus-e2e/ssh-root"),
    }
}

#[test]
fn manifest_serializes_fixture_paths_and_dynamic_ports() {
    let manifest = FixtureManifest::new(
        paths(),
        21_001,
        21_002,
        SshManifest::available(21_003, "alice", "/tmp/client_ed25519"),
        "/tmp/serverus-e2e-editor",
    );

    let value = serde_json::to_value(manifest).unwrap();

    assert_eq!(
        value["paths"]["local_source"],
        "/tmp/serverus-e2e/local-source"
    );
    assert_eq!(value["ftp"]["host"], "127.0.0.1");
    assert_eq!(value["ftp"]["port"], 21_001);
    assert_eq!(value["s3"]["endpoint"], "http://127.0.0.1:21002");
    assert_eq!(value["ssh"]["available"], true);
    assert_eq!(value["ssh"]["username"], "alice");
    assert_eq!(value["ssh"]["key_path"], "/tmp/client_ed25519");
    assert_eq!(value["editor"]["executable"], "/tmp/serverus-e2e-editor");
}

#[test]
fn unavailable_ssh_is_explicit_and_manifest_contains_no_secrets() {
    let json = serde_json::to_string(&FixtureManifest::new(
        paths(),
        21_001,
        21_002,
        SshManifest::unavailable(),
        "/tmp/serverus-e2e-editor",
    ))
    .unwrap();

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["ssh"]["available"], false);
    for forbidden in ["password", "secret", "access_key", "secret_key"] {
        assert!(
            !json.to_ascii_lowercase().contains(forbidden),
            "{forbidden}"
        );
    }
}
