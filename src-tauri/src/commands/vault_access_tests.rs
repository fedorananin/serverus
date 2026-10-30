//! Vault access command tests.

use super::{
    vault_change_password, vault_create, vault_get_info, vault_unlock_password, vault_unlock_quick,
};
use crate::autolock::ActivityTracker;
use crate::error::AppResult;
use crate::state::AppState;
use crate::vault::format::KdfParams;
use crate::vault::quick_unlock::{NoQuickUnlock, QuickUnlock};
use crate::vault::VaultManager;
use serverus_runtime::RuntimeError;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Manager;
use zeroize::Zeroizing;

struct StoredQuickUnlock {
    dek: Zeroizing<Vec<u8>>,
}

impl QuickUnlock for StoredQuickUnlock {
    fn is_available(&self) -> bool {
        true
    }

    fn has_dek(&self, _vault_id: &str) -> bool {
        true
    }

    fn store_dek(&self, _vault_id: &str, _dek: &[u8]) -> AppResult<()> {
        Ok(())
    }

    fn retrieve_dek(&self, _vault_id: &str) -> AppResult<Zeroizing<Vec<u8>>> {
        Ok(self.dek.clone())
    }

    fn clear(&self, _vault_id: &str) {}
}

fn test_kdf() -> KdfParams {
    KdfParams {
        m_cost_kib: 8 * 1024,
        t_cost: 1,
        p_cost: 1,
    }
}

fn expired_activity() -> Arc<ActivityTracker> {
    let activity = Arc::new(ActivityTracker::default());
    *activity.last_activity.lock().unwrap() = Instant::now() - Duration::from_secs(120);
    activity
}

fn test_app(
    vault: VaultManager,
    quick: Arc<dyn QuickUnlock>,
    activity: Arc<ActivityTracker>,
) -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .manage(AppState::from_vault(vault, quick, activity))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap()
}

fn assert_idle_interval_was_restarted(activity: &ActivityTracker, command_started: Instant) {
    let last_activity = *activity.last_activity.lock().unwrap();
    assert!(
        last_activity >= command_started,
        "expected the idle interval to start after the unlock command"
    );
}

#[test]
fn vault_create_restarts_an_expired_idle_interval() {
    let directory = tempfile::tempdir().unwrap();
    let activity = expired_activity();
    let app = test_app(
        VaultManager::new(directory.path().join("created.serverus")),
        Arc::new(NoQuickUnlock),
        activity.clone(),
    );
    let command_started = Instant::now();

    tauri::async_runtime::block_on(vault_create(app.state::<AppState>(), "password".into()))
        .unwrap();

    assert_idle_interval_was_restarted(&activity, command_started);
    assert!(app
        .state::<AppState>()
        .application
        .require_unlocked()
        .is_ok());
}

#[test]
fn password_unlock_restarts_an_expired_idle_interval() {
    let directory = tempfile::tempdir().unwrap();
    let mut vault = VaultManager::new(directory.path().join("password.serverus"));
    vault.create("password", test_kdf()).unwrap();
    vault.lock();
    let activity = expired_activity();
    let app = test_app(vault, Arc::new(NoQuickUnlock), activity.clone());
    let command_started = Instant::now();

    tauri::async_runtime::block_on(vault_unlock_password(
        app.state::<AppState>(),
        "password".into(),
    ))
    .unwrap();

    assert_idle_interval_was_restarted(&activity, command_started);
    assert!(app
        .state::<AppState>()
        .application
        .require_unlocked()
        .is_ok());
}

#[test]
fn quick_unlock_restarts_an_expired_idle_interval() {
    let directory = tempfile::tempdir().unwrap();
    let mut vault = VaultManager::new(directory.path().join("quick.serverus"));
    vault.create("password", test_kdf()).unwrap();
    let dek = Zeroizing::new(vault.dek().unwrap().to_vec());
    vault.lock();
    let activity = expired_activity();
    let app = test_app(vault, Arc::new(StoredQuickUnlock { dek }), activity.clone());
    let command_started = Instant::now();

    tauri::async_runtime::block_on(vault_unlock_quick(app.state::<AppState>())).unwrap();

    assert_idle_interval_was_restarted(&activity, command_started);
    assert!(app
        .state::<AppState>()
        .application
        .require_unlocked()
        .is_ok());
}

#[test]
fn locking_revokes_vault_access_without_retiring_the_runtime_context() {
    let directory = tempfile::tempdir().unwrap();
    let app = test_app(
        VaultManager::new(directory.path().join("locked.serverus")),
        Arc::new(NoQuickUnlock),
        Arc::new(ActivityTracker::default()),
    );
    tauri::async_runtime::block_on(vault_create(app.state::<AppState>(), "password".into()))
        .unwrap();
    let state = app.state::<AppState>();
    let generation = state.application.require_unlocked().unwrap().context_id();

    tauri::async_runtime::block_on(state.application.lock_selected_vault()).unwrap();

    assert!(!state.vault.lock().unwrap().is_unlocked());
    assert_eq!(
        state.application.require_unlocked(),
        Err(RuntimeError::VaultLocked)
    );
    assert_eq!(
        state.application.require_active().unwrap().context_id(),
        generation
    );
}

#[test]
fn a_passwordless_vault_reports_itself_and_offers_no_quick_unlock() {
    let directory = tempfile::tempdir().unwrap();
    let mut vault = VaultManager::new(directory.path().join("open.serverus"));
    vault.create("", test_kdf()).unwrap();
    let dek = Zeroizing::new(vault.dek().unwrap().to_vec());
    vault.lock();
    // A keychain entry left over from before the password was removed.
    let app = test_app(
        vault,
        Arc::new(StoredQuickUnlock { dek }),
        Arc::new(ActivityTracker::default()),
    );

    let info = tauri::async_runtime::block_on(vault_get_info(app.state::<AppState>())).unwrap();
    assert!(info.passwordless);
    assert!(!info.quick_unlock_ready);

    tauri::async_runtime::block_on(vault_unlock_password(app.state::<AppState>(), "".into()))
        .unwrap();
    assert!(app
        .state::<AppState>()
        .application
        .require_unlocked()
        .is_ok());
}

#[test]
fn removing_and_setting_the_password_flips_the_reported_mode() {
    let directory = tempfile::tempdir().unwrap();
    let mut vault = VaultManager::new(directory.path().join("flip.serverus"));
    vault.create("password", test_kdf()).unwrap();
    vault.lock();
    let app = test_app(
        vault,
        Arc::new(NoQuickUnlock),
        Arc::new(ActivityTracker::default()),
    );
    let state = || app.state::<AppState>();
    let passwordless = || {
        tauri::async_runtime::block_on(vault_get_info(state()))
            .unwrap()
            .passwordless
    };
    tauri::async_runtime::block_on(vault_unlock_password(state(), "password".into())).unwrap();
    assert!(!passwordless());

    tauri::async_runtime::block_on(vault_change_password(state(), "password".into(), "".into()))
        .unwrap();
    assert!(passwordless());

    tauri::async_runtime::block_on(vault_change_password(state(), "".into(), "again".into()))
        .unwrap();
    assert!(!passwordless());
}
