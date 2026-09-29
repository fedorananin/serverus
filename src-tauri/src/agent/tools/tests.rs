use super::args::{local_path, parse, remote_path, IfExists};
use super::create::CreateArgs;
use super::keys::key_bytes;
use super::ConnectionHold;
use crate::autolock::ActivityTracker;
use crate::vault::model::{AuthMethod, ConflictPolicy, FtpTlsMode, Protocol};
use serde_json::json;
use std::sync::Arc;

#[test]
fn named_keys_map_to_terminal_bytes() {
    assert_eq!(key_bytes("enter").unwrap(), b"\r");
    assert_eq!(key_bytes("ctrl-c").unwrap(), vec![3]);
    assert_eq!(key_bytes("CTRL-Z").unwrap(), vec![26]);
    assert_eq!(key_bytes("up").unwrap(), b"\x1b[A");
    assert_eq!(key_bytes("ctrl-\\").unwrap(), vec![0x1c]);
    assert!(key_bytes("ctrl-1").is_none());
    assert!(key_bytes("hyper").is_none());
}

#[test]
fn path_arguments_are_validated() {
    assert_eq!(remote_path("  /var/www ").unwrap(), "/var/www");
    assert!(remote_path("  ").is_err());
    assert!(local_path("relative/file").is_err());
    assert!(local_path("~/file").unwrap().is_absolute());
}

#[test]
fn if_exists_maps_to_a_conflict_policy() {
    assert_eq!(IfExists::default().policy(), ConflictPolicy::Overwrite);
    assert_eq!(IfExists::Skip.policy(), ConflictPolicy::Skip);
    assert_eq!(IfExists::Rename.policy(), ConflictPolicy::Rename);
}

fn create_args(value: serde_json::Value) -> CreateArgs {
    parse(value).unwrap()
}

#[test]
fn a_created_connection_keeps_the_secrets_it_was_given() {
    let connection = create_args(json!({
        "host": " 203.0.113.7 ", "username": "root", "password": "pw",
        "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----", "key_passphrase": "kp",
    }))
    .into_connection(None);
    assert_eq!(connection.host, "203.0.113.7");
    assert_eq!(connection.name, "203.0.113.7");
    assert_eq!(connection.port, 22);
    assert_eq!(connection.auth.method, AuthMethod::Key);
    assert_eq!(connection.auth.password.as_deref(), Some("pw"));
    assert!(connection.auth.key_inline.is_some());
    assert_eq!(connection.auth.key_passphrase.as_deref(), Some("kp"));
    assert_eq!(connection.agent_access, None);
}

#[test]
fn auth_and_protocol_defaults() {
    let password = create_args(json!({ "host": "h", "password": "pw" })).into_connection(None);
    assert_eq!(password.auth.method, AuthMethod::Password);
    let agent = create_args(json!({ "host": "h" })).into_connection(None);
    assert_eq!(agent.auth.method, AuthMethod::Agent);
    let ftp = create_args(json!({ "host": "h", "protocol": "ftp", "ftp_tls": true }))
        .into_connection(None);
    assert_eq!((ftp.protocol, ftp.port), (Protocol::Ftp, 21));
    assert_eq!(ftp.ftp.unwrap().tls, FtpTlsMode::Explicit);
    let s3 = create_args(json!({ "host": "h", "protocol": "s3", "s3_bucket": "b" }))
        .into_connection(Some("bastion".into()));
    assert_eq!(s3.port, 443);
    assert_eq!(s3.s3.unwrap().bucket.as_deref(), Some("b"));
    assert_eq!(s3.auth.method, AuthMethod::Password);
    assert_eq!(s3.jump_host.as_deref(), Some("bastion"));
}

#[test]
fn the_confirmation_names_secrets_without_showing_them() {
    let args = create_args(json!({ "host": "h", "username": "u", "password": "hunter2" }));
    let detail = args.describe("Clients");
    assert!(detail.contains("password: provided"), "{detail}");
    assert!(!detail.contains("hunter2"));
}

#[test]
fn a_connection_keeps_the_vault_open_until_it_ends() {
    let tracker = Arc::new(ActivityTracker::default());
    let connection = ConnectionHold::default();
    connection.engage(&tracker);
    connection.engage(&tracker);
    assert!(tracker.is_held());
    // Agent access switched off mid-connection.
    connection.release();
    assert!(!tracker.is_held());
    connection.engage(&tracker);
    // The agent disconnects: its toolbox (and the hold) goes away.
    drop(connection);
    assert!(!tracker.is_held());
}
