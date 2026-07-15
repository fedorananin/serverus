//! Session command tests.

use std::sync::Arc;

use super::accept_host_key_for_context;
use crate::autolock::ActivityTracker;
use crate::state::AppState;
use crate::vault::format::KdfParams;
use crate::vault::quick_unlock::NoQuickUnlock;
use crate::vault::VaultManager;

struct TestState {
    state: AppState,
    _directory: tempfile::TempDir,
}

fn unlocked_state() -> TestState {
    let directory = tempfile::tempdir().unwrap();
    let mut manager = VaultManager::new(directory.path().join("host-key.serverus"));
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
    TestState {
        state,
        _directory: directory,
    }
}

#[test]
fn a_prompt_from_another_runtime_context_cannot_mutate_the_current_vault() {
    let fixture = unlocked_state();
    let state = &fixture.state;

    let error = tauri::async_runtime::block_on(accept_host_key_for_context(
        &state.application,
        "retired-context".into(),
        "retired-access".into(),
        "old.example".into(),
        22,
        "ssh-ed25519 old-key".into(),
    ))
    .unwrap_err();

    assert_eq!(error.code, "wrong_runtime_context");
    assert!(state
        .vault
        .lock()
        .unwrap()
        .payload()
        .unwrap()
        .known_hosts
        .is_empty());
}

#[test]
fn the_current_runtime_context_can_accept_its_prompt() {
    let fixture = unlocked_state();
    let state = &fixture.state;
    let context_id = state
        .application
        .require_unlocked()
        .unwrap()
        .context_id()
        .get()
        .to_string();
    let access_epoch = state
        .application
        .require_unlocked()
        .unwrap()
        .vault_access_epoch()
        .unwrap()
        .get()
        .to_string();

    tauri::async_runtime::block_on(accept_host_key_for_context(
        &state.application,
        context_id,
        access_epoch,
        "current.example".into(),
        2222,
        "ssh-ed25519 current-key".into(),
    ))
    .unwrap();

    assert_eq!(
        state
            .vault
            .lock()
            .unwrap()
            .payload()
            .unwrap()
            .known_hosts
            .get("current.example:2222")
            .map(String::as_str),
        Some("ssh-ed25519 current-key")
    );
}

#[test]
fn a_prompt_from_before_lock_cannot_mutate_the_reunlocked_vault() {
    let fixture = unlocked_state();
    let state = &fixture.state;
    let old_lease = state.application.require_unlocked().unwrap();
    let context_id = old_lease.context_id().get().to_string();
    let access_epoch = old_lease.vault_access_epoch().unwrap().get().to_string();

    state.application.lock_vault().unwrap();
    {
        let mut manager = state.vault.lock().unwrap();
        manager.lock();
        manager.unlock_with_password("password").unwrap();
        state
            .application
            .activate_selected_vault(manager.vault_id())
            .unwrap();
    }

    let error = tauri::async_runtime::block_on(accept_host_key_for_context(
        &state.application,
        context_id,
        access_epoch,
        "stale.example".into(),
        22,
        "ssh-ed25519 stale-key".into(),
    ))
    .unwrap_err();

    assert_eq!(error.code, "wrong_runtime_context");
    assert!(state
        .vault
        .lock()
        .unwrap()
        .payload()
        .unwrap()
        .known_hosts
        .is_empty());
}
