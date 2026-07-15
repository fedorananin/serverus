//! S3 upload-ACL use-case tests.

use super::persist_s3_upload_acl;
use crate::autolock::ActivityTracker;
use crate::error::AppError;
use crate::state::AppState;
use crate::vault::format::KdfParams;
use crate::vault::model::{Connection, S3UploadAcl};
use crate::vault::quick_unlock::NoQuickUnlock;
use crate::vault::VaultManager;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

#[test]
fn persisted_and_live_s3_modes_share_one_serialized_section() {
    let directory = tempfile::tempdir().unwrap();
    let mut manager = VaultManager::new(directory.path().join("test.serverus"));
    manager
        .create(
            "password",
            KdfParams {
                m_cost_kib: 8 * 1024,
                t_cost: 1,
                p_cost: 1,
            },
        )
        .unwrap();
    let connection: Connection = serde_json::from_value(serde_json::json!({
        "name": "Object storage",
        "protocol": "s3",
        "host": "s3.example.com",
        "port": 443,
        "auth": {
            "method": "password",
            "username": "access-key",
            "password": "secret-key"
        }
    }))
    .unwrap();
    manager
        .with_payload(|payload| {
            payload.connections.insert("connection".into(), connection);
            Ok(())
        })
        .unwrap();

    let vault = Arc::new(Mutex::new(manager));
    let live = Arc::new(Mutex::new(S3UploadAcl::Private));
    for mode in [S3UploadAcl::PublicRead, S3UploadAcl::Ask] {
        let vault_during_apply = vault.clone();
        let live_during_apply = live.clone();
        persist_s3_upload_acl(
            vault.as_ref(),
            "connection",
            mode,
            |_| Ok(()),
            move |committed| {
                assert!(matches!(
                    vault_during_apply.try_lock(),
                    Err(TryLockError::WouldBlock)
                ));
                *live_during_apply.lock().unwrap() = committed;
            },
        )
        .unwrap();
    }

    let persisted = vault.lock().unwrap().payload().unwrap().connections["connection"]
        .s3
        .as_ref()
        .unwrap()
        .upload_acl;
    assert_eq!(persisted, S3UploadAcl::Ask);
    assert_eq!(*live.lock().unwrap(), persisted);
}

#[test]
fn stale_context_is_validated_under_vault_lock_before_s3_persistence() {
    let directory = tempfile::tempdir().unwrap();
    let mut manager = VaultManager::new(directory.path().join("test.serverus"));
    manager
        .create(
            "password",
            KdfParams {
                m_cost_kib: 8 * 1024,
                t_cost: 1,
                p_cost: 1,
            },
        )
        .unwrap();
    let connection: Connection = serde_json::from_value(serde_json::json!({
        "name": "Object storage",
        "protocol": "s3",
        "host": "s3.example.com",
        "port": 443,
        "auth": {
            "method": "password",
            "username": "access-key",
            "password": "secret-key"
        }
    }))
    .unwrap();
    manager
        .with_payload(|payload| {
            payload.connections.insert("connection".into(), connection);
            Ok(())
        })
        .unwrap();

    let vault = Arc::new(Mutex::new(manager));
    let vault_during_validation = vault.clone();
    let live_updated = Arc::new(AtomicBool::new(false));
    let live_updated_during_apply = live_updated.clone();
    let error = persist_s3_upload_acl(
        vault.as_ref(),
        "connection",
        S3UploadAcl::PublicRead,
        move |_| {
            assert!(matches!(
                vault_during_validation.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            Err(AppError::WrongRuntimeContext)
        },
        move |_| live_updated_during_apply.store(true, Ordering::SeqCst),
    )
    .unwrap_err();

    assert!(matches!(error, AppError::WrongRuntimeContext));
    let persisted = vault.lock().unwrap().payload().unwrap().connections["connection"]
        .s3
        .as_ref()
        .map(|config| config.upload_acl)
        .unwrap_or(S3UploadAcl::Private);
    assert_eq!(persisted, S3UploadAcl::Private);
    assert!(!live_updated.load(Ordering::SeqCst));
}

#[test]
fn pre_lock_access_epoch_cannot_persist_after_reunlock() {
    let directory = tempfile::tempdir().unwrap();
    let mut manager = VaultManager::new(directory.path().join("test.serverus"));
    manager
        .create(
            "password",
            KdfParams {
                m_cost_kib: 8 * 1024,
                t_cost: 1,
                p_cost: 1,
            },
        )
        .unwrap();
    let connection: Connection = serde_json::from_value(serde_json::json!({
        "name": "Object storage",
        "protocol": "s3",
        "host": "s3.example.com",
        "port": 443,
        "auth": {
            "method": "password",
            "username": "access-key",
            "password": "secret-key"
        }
    }))
    .unwrap();
    manager
        .with_payload(|payload| {
            payload.connections.insert("connection".into(), connection);
            Ok(())
        })
        .unwrap();
    let state = AppState::from_vault(
        manager,
        Arc::new(NoQuickUnlock),
        Arc::new(ActivityTracker::default()),
    );
    let vault_id = state.vault.lock().unwrap().vault_id();
    state
        .application
        .activate_selected_vault(vault_id.clone())
        .unwrap();
    let stale_access = state.application.require_unlocked().unwrap();
    state.application.lock_vault().unwrap();
    state.application.activate_selected_vault(vault_id).unwrap();

    let runtime = state.application.clone();
    let error = persist_s3_upload_acl(
        state.vault.as_ref(),
        "connection",
        S3UploadAcl::PublicRead,
        move |manager| {
            stale_access.validate(&runtime).map_err(AppError::from)?;
            if manager.vault_id() != stale_access.vault().as_str() {
                return Err(AppError::WrongRuntimeContext);
            }
            Ok(())
        },
        |_| panic!("a stale access epoch updated the live session"),
    )
    .unwrap_err();

    assert!(matches!(error, AppError::WrongRuntimeContext));
    let persisted = state.vault.lock().unwrap().payload().unwrap().connections["connection"]
        .s3
        .as_ref()
        .map(|config| config.upload_acl)
        .unwrap_or(S3UploadAcl::Private);
    assert_eq!(persisted, S3UploadAcl::Private);
}
