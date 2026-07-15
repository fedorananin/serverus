//! Vault location command tests.

use super::switch_vault_application;
use crate::autolock::ActivityTracker;
use crate::state::AppState;
use crate::vault::format::KdfParams;
use crate::vault::quick_unlock::NoQuickUnlock;
use crate::vault::VaultManager;
use serverus_runtime::RuntimeError;
use std::sync::Arc;

pub(super) fn unlocked_state(path: std::path::PathBuf) -> AppState {
    let mut manager = VaultManager::new(path);
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
    let state = AppState::from_vault(
        manager,
        Arc::new(NoQuickUnlock),
        Arc::new(ActivityTracker::default()),
    );
    let vault_id = state.vault.lock().unwrap().vault_id();
    state.application.activate_selected_vault(vault_id).unwrap();
    state
}

#[test]
fn config_failure_preserves_the_selected_unlocked_vault_and_context() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original.serverus");
    let target = directory.path().join("other.serverus");
    let state = unlocked_state(original.clone());
    let generation = state.application.require_unlocked().unwrap().context_id();

    let result = tauri::async_runtime::block_on(switch_vault_application(
        &state.application,
        target,
        |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "simulated config failure",
            ))
        },
    ));

    assert!(result.is_err());
    let manager = state.vault.lock().unwrap();
    assert_eq!(manager.path(), original);
    assert!(manager.is_unlocked());
    assert!(manager.payload().is_ok());
    drop(manager);
    assert_eq!(
        state.application.require_unlocked().unwrap().context_id(),
        generation
    );
}

#[test]
fn successful_switch_retires_the_previous_runtime_context() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("other.serverus");
    let state = unlocked_state(directory.path().join("original.serverus"));

    tauri::async_runtime::block_on(switch_vault_application(
        &state.application,
        target.clone(),
        |_| Ok(()),
    ))
    .unwrap();

    assert_eq!(state.vault.lock().unwrap().path(), target);
    assert_eq!(
        state.application.require_active(),
        Err(RuntimeError::NoActiveContext)
    );
}

#[test]
fn initial_lock_screen_can_switch_without_an_active_runtime_context() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("selected.serverus");
    let state = AppState::from_vault(
        VaultManager::new(directory.path().join("default.serverus")),
        Arc::new(NoQuickUnlock),
        Arc::new(ActivityTracker::default()),
    );

    tauri::async_runtime::block_on(switch_vault_application(
        &state.application,
        target.clone(),
        |_| Ok(()),
    ))
    .unwrap();

    assert_eq!(state.vault.lock().unwrap().path(), target);
    assert_eq!(
        state.application.require_active(),
        Err(RuntimeError::NoActiveContext)
    );
}
