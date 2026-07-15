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

fn sample_connection() -> Connection {
    Connection {
        name: "test".into(),
        badge: None,
        protocol: Protocol::Ssh,
        host: "example.com".into(),
        port: 22,
        auth: AuthConfig {
            method: AuthMethod::Password,
            username: "root".into(),
            password: Some("secret".into()),
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
    }
}

#[test]
fn create_lock_unlock_roundtrip() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("master", test_kdf()).unwrap();
    mgr.with_payload(|p| {
        p.connections.insert("c1".into(), sample_connection());
        Ok(())
    })
    .unwrap();

    mgr.lock();
    assert!(!mgr.is_unlocked());
    assert!(matches!(mgr.payload(), Err(AppError::VaultLocked)));

    mgr.unlock_with_password("master").unwrap();
    let payload = mgr.payload().unwrap();
    assert_eq!(
        payload.connections["c1"].auth.password.as_deref(),
        Some("secret")
    );
}

#[test]
fn wrong_password_fails() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("master", test_kdf()).unwrap();
    mgr.lock();
    assert!(matches!(
        mgr.unlock_with_password("nope"),
        Err(AppError::InvalidPassword)
    ));
}

#[test]
fn change_password_keeps_data() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("old", test_kdf()).unwrap();
    mgr.with_payload(|p| {
        p.connections.insert("c1".into(), sample_connection());
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        mgr.change_password("wrong", "new"),
        Err(AppError::InvalidPassword)
    ));
    mgr.change_password("old", "new").unwrap();
    mgr.lock();
    assert!(mgr.unlock_with_password("old").is_err());
    mgr.unlock_with_password("new").unwrap();
    assert!(mgr.payload().unwrap().connections.contains_key("c1"));
}

#[test]
fn change_password_rekeys_primary_and_backup() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("old", test_kdf()).unwrap();
    mgr.with_payload(|payload| {
        payload.connections.insert("c1".into(), sample_connection());
        Ok(())
    })
    .unwrap();

    mgr.change_password("old", "new").unwrap();

    for path in [mgr.path().to_path_buf(), bak_path(mgr.path())] {
        let mut copy = VaultManager::new(path);
        assert!(matches!(
            copy.unlock_with_password("old"),
            Err(AppError::InvalidPassword)
        ));
        copy.unlock_with_password("new").unwrap();
        assert!(copy.payload().unwrap().connections.contains_key("c1"));
    }
}

#[test]
fn change_password_failure_keeps_the_old_header_and_primary_usable() {
    let (dir, mut mgr) = temp_vault();
    mgr.create("old", test_kdf()).unwrap();
    mgr.with_payload(|payload| {
        payload.connections.insert("c1".into(), sample_connection());
        Ok(())
    })
    .unwrap();
    let primary_path = mgr.path().to_path_buf();
    let primary_before = fs::read(&primary_path).unwrap();

    // The backup replacement can succeed, but the primary replacement
    // cannot replace a directory. This exercises the partial-commit edge.
    let blocked_path = dir.path().join("blocked.serverus");
    fs::create_dir(&blocked_path).unwrap();
    mgr.path = blocked_path.clone();

    assert!(mgr.change_password("old", "new").is_err());
    assert!(mgr.is_unlocked());
    assert_eq!(fs::read(&primary_path).unwrap(), primary_before);
    assert!(format::unwrap_dek(mgr.header.as_ref().unwrap(), "old").is_ok());
    assert!(format::unwrap_dek(mgr.header.as_ref().unwrap(), "new").is_err());

    let mut backup = VaultManager::new(bak_path(&blocked_path));
    backup.unlock_with_password("new").unwrap();
    assert!(backup.payload().unwrap().connections.contains_key("c1"));

    mgr.path = primary_path;
    mgr.lock();
    mgr.unlock_with_password("old").unwrap();
    assert!(mgr.payload().unwrap().connections.contains_key("c1"));
}

#[test]
fn with_payload_rolls_back_when_mutation_fails() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("pw", test_kdf()).unwrap();

    let result: AppResult<()> = mgr.with_payload(|p| {
        p.connections.insert("c1".into(), sample_connection());
        Err(AppError::Other("mutation failed".into()))
    });

    assert!(result.is_err());
    assert!(!mgr.payload().unwrap().connections.contains_key("c1"));
}

#[test]
fn with_payload_rolls_back_when_save_fails() {
    let (dir, mut mgr) = temp_vault();
    mgr.create("pw", test_kdf()).unwrap();
    let original_path = mgr.path().to_path_buf();

    let non_directory = dir.path().join("not-a-directory");
    fs::write(&non_directory, b"block parent directory creation").unwrap();
    mgr.path = non_directory.join("test.serverus");

    let result = mgr.with_payload(|p| {
        p.connections.insert("c1".into(), sample_connection());
        Ok(())
    });

    assert!(result.is_err());
    assert!(!mgr.payload().unwrap().connections.contains_key("c1"));

    mgr.path = original_path;
    mgr.lock();
    mgr.unlock_with_password("pw").unwrap();
    assert!(!mgr.payload().unwrap().connections.contains_key("c1"));
}

#[test]
fn bak_file_holds_previous_version() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("pw", test_kdf()).unwrap();
    let v1 = fs::read(mgr.path()).unwrap();

    mgr.with_payload(|p| {
        p.connections.insert("c1".into(), sample_connection());
        Ok(())
    })
    .unwrap();

    let bak = bak_path(mgr.path());
    assert!(bak.is_file());
    assert_eq!(fs::read(&bak).unwrap(), v1);

    // The .bak decrypts fine with the same password.
    let mut bak_mgr = VaultManager::new(bak);
    bak_mgr.unlock_with_password("pw").unwrap();
    assert!(bak_mgr.payload().unwrap().connections.is_empty());
}

#[test]
fn corrupted_file_reports_corruption() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("pw", test_kdf()).unwrap();
    mgr.lock();

    let mut bytes = fs::read(mgr.path()).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(mgr.path(), &bytes).unwrap();

    assert!(matches!(
        mgr.unlock_with_password("pw"),
        Err(AppError::Corrupted(_))
    ));
}

#[test]
fn unlock_with_dek_roundtrip() {
    let (_dir, mut mgr) = temp_vault();
    mgr.create("pw", test_kdf()).unwrap();
    let dek = *mgr.dek().unwrap();
    mgr.lock();

    mgr.unlock_with_dek(&dek).unwrap();
    assert!(mgr.is_unlocked());

    mgr.lock();
    let wrong = [0u8; 32];
    assert!(mgr.unlock_with_dek(&wrong).is_err());
}

#[test]
fn set_path_handles_folders_conflicts_and_missing_parents() {
    let (dir, mut mgr) = temp_vault();
    mgr.create("pw", test_kdf()).unwrap();

    // A directory target keeps the current file name.
    let sub = dir.path().join("backups");
    fs::create_dir(&sub).unwrap();
    mgr.set_path(sub.clone()).unwrap();
    assert_eq!(mgr.path(), sub.join("test.serverus"));
    assert!(sub.join("test.serverus").is_file());

    // An occupied target is a clear error and the vault stays put.
    let occupied = dir.path().join("other.serverus");
    fs::write(&occupied, b"x").unwrap();
    let err = mgr.set_path(occupied).unwrap_err();
    assert!(err.to_string().contains("already exists"));
    assert_eq!(mgr.path(), sub.join("test.serverus"));

    // Missing parent folders are created for a full file path.
    let deep = dir.path().join("a").join("b").join("moved.serverus");
    mgr.set_path(deep.clone()).unwrap();
    assert!(deep.is_file());

    // Trailing slash = "into this folder", even if it doesn't exist yet.
    let slashed = format!("{}/", dir.path().join("slashdir").display());
    mgr.set_path(PathBuf::from(slashed)).unwrap();
    assert!(dir.path().join("slashdir").join("moved.serverus").is_file());
}

#[test]
fn set_path_rolls_runtime_pointer_back_when_config_persist_fails() {
    let (dir, mut mgr) = temp_vault();
    mgr.create("pw", test_kdf()).unwrap();
    let original = mgr.path().to_path_buf();
    let target = dir.path().join("new.serverus");

    let result = mgr.set_path_transactional(target.clone(), |_| {
        Err(AppError::Other("config write failed".into()))
    });

    assert!(result.is_err());
    assert_eq!(mgr.path(), original);
    assert!(target.is_file(), "the staged copy remains recoverable");
}
