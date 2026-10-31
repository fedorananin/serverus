//! Vaults without a master password: the empty password over
//! [`format::PASSWORDLESS_KDF`], same file format and write path.

use super::persist::bak_path;
use super::*;
use crate::vault::model::{AuthConfig, AuthMethod, Connection, Protocol};

fn test_kdf() -> KdfParams {
    KdfParams {
        m_cost_kib: 8 * 1024,
        t_cost: 1,
        p_cost: 1,
    }
}

fn temp_vault() -> (tempfile::TempDir, VaultManager) {
    let dir = tempfile::tempdir().unwrap();
    let mgr = VaultManager::new(dir.path().join("test.serverus"));
    (dir, mgr)
}

fn add_connection(mgr: &mut VaultManager) {
    mgr.with_payload(|p| {
        p.connections.insert(
            "c1".into(),
            Connection {
                name: "test".into(),
                badge: None,
                protocol: Protocol::Ssh,
                host: "example.com".into(),
                port: 22,
                auth: AuthConfig {
                    method: AuthMethod::Password,
                    username: "root".into(),
                    password: Some("hunter2-secret".into()),
                    key_path: None,
                    key_inline: None,
                    key_passphrase: None,
                },
                jump_host: None,
                ftp: None,
                s3: None,
                remote_dir: None,
                local_dir: None,
                tunnels: vec![],
                disable_terminal: false,
                notes: String::new(),
                agent_access: None,
            },
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn empty_password_creates_a_passwordless_vault_that_is_still_sealed() {
    let (_dir, mut mgr) = temp_vault();
    // Whatever the caller asks for, the empty password gets the marker KDF.
    mgr.create("", test_kdf()).unwrap();
    add_connection(&mut mgr);
    assert!(mgr.is_passwordless());

    let bytes = fs::read(mgr.path()).unwrap();
    assert!(!bytes
        .windows(b"hunter2-secret".len())
        .any(|w| w == b"hunter2-secret"));

    mgr.lock();
    assert!(mgr.is_passwordless(), "detected from the file while locked");
    mgr.unlock_with_password("").unwrap();
    assert!(mgr.payload().unwrap().connections.contains_key("c1"));
}

#[test]
fn a_vault_with_a_password_is_not_passwordless() {
    let (_dir, mut mgr) = temp_vault();
    assert!(!mgr.is_passwordless(), "no file yet");
    mgr.create("master", test_kdf()).unwrap();
    assert!(!mgr.is_passwordless());
    mgr.lock();
    assert!(!mgr.is_passwordless());
    assert!(matches!(
        mgr.unlock_with_password(""),
        Err(AppError::InvalidPassword)
    ));
}

#[test]
fn removing_the_password_keeps_the_data() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("old", test_kdf()).unwrap();
    add_connection(&mut mgr);

    mgr.change_password("old", "", test_kdf()).unwrap();
    assert!(mgr.is_passwordless());

    mgr.lock();
    assert!(mgr.unlock_with_password("old").is_err());
    mgr.unlock_with_password("").unwrap();
    assert!(mgr.payload().unwrap().connections.contains_key("c1"));
}

#[test]
fn setting_a_password_replaces_the_key_a_copy_of_the_open_file_held() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("", test_kdf()).unwrap();
    add_connection(&mut mgr);
    let old_dek = *mgr.dek().unwrap();

    mgr.change_password("", "new", test_kdf()).unwrap();
    assert!(!mgr.is_passwordless());
    assert_ne!(*mgr.dek().unwrap(), old_dek);

    // Neither the primary nor the backup opens without the password, or
    // with the key the passwordless file had.
    for path in [mgr.path().to_path_buf(), bak_path(mgr.path())] {
        let mut copy = VaultManager::new(path);
        assert!(copy.unlock_with_password("").is_err());
        assert!(copy.unlock_with_dek(&old_dek).is_err());
    }

    mgr.lock();
    mgr.unlock_with_password("new").unwrap();
    assert!(mgr.payload().unwrap().connections.contains_key("c1"));
}

#[test]
fn a_real_password_never_gets_the_passwordless_kdf() {
    assert_eq!(kdf_for("", test_kdf()), format::PASSWORDLESS_KDF);
    assert_eq!(
        kdf_for("master", format::PASSWORDLESS_KDF),
        KdfParams::default()
    );
    assert_eq!(kdf_for("master", test_kdf()), test_kdf());
}

#[test]
fn the_vaults_own_dek_removes_the_password_and_any_other_is_refused() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("master", test_kdf()).unwrap();
    add_connection(&mut mgr);
    let dek = mgr.dek().unwrap().to_vec();

    let mut wrong = dek.clone();
    wrong[0] ^= 1;
    assert!(matches!(
        mgr.remove_password_with_dek(&wrong),
        Err(AppError::InvalidPassword)
    ));
    assert!(!mgr.is_passwordless());

    mgr.remove_password_with_dek(&dek).unwrap();
    assert!(mgr.is_passwordless());
    mgr.lock();
    mgr.unlock_with_password("").unwrap();
    assert!(mgr.payload().unwrap().connections.contains_key("c1"));
}
