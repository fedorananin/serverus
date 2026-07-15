//! Regressions for commands queued across a vault context switch.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tauri::Manager;

use super::{connection_secrets, known_host_remove};
use crate::autolock::ActivityTracker;
use crate::error::ApiResult;
use crate::state::AppState;
use crate::vault::format::KdfParams;
use crate::vault::model::{AuthMethod, ConnectionInput, Protocol};
use crate::vault::quick_unlock::NoQuickUnlock;
use crate::vault::VaultManager;

const CONNECTION_ID: &str = "shared-connection";
const PROTECTED_HOST: &str = "protected.example:22";

fn poll_once_pending<F: Future>(mut future: Pin<&mut F>) {
    let mut context = Context::from_waker(futures::task::noop_waker_ref());
    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
}

fn test_vault(path: std::path::PathBuf, password: &str) -> VaultManager {
    let mut manager = VaultManager::new(path);
    manager
        .create(
            "master-password",
            KdfParams {
                m_cost_kib: 8 * 1024,
                t_cost: 1,
                p_cost: 1,
            },
        )
        .unwrap();
    manager
        .with_payload(|payload| {
            payload.connections.insert(
                CONNECTION_ID.into(),
                ConnectionInput {
                    name: "Server".into(),
                    badge: None,
                    protocol: Protocol::Ssh,
                    host: "example.test".into(),
                    port: 22,
                    auth_method: AuthMethod::Password,
                    username: "user".into(),
                    password: Some(password.into()),
                    key_path: None,
                    key_inline: None,
                    key_passphrase: None,
                    jump_host: None,
                    ftp: None,
                    s3: None,
                    remote_dir: None,
                    local_dir: None,
                    tunnels: Vec::new(),
                    disable_terminal: false,
                    notes: String::new(),
                }
                .into_connection(None),
            );
            payload
                .known_hosts
                .insert(PROTECTED_HOST.into(), "ssh-ed25519 public-key".into());
            Ok(())
        })
        .unwrap();
    manager
}

fn test_app(vault: VaultManager) -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .manage(AppState::from_vault(
            vault,
            Arc::new(NoQuickUnlock),
            Arc::new(ActivityTracker::default()),
        ))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap()
}

fn was_rejected_as_stale<T>(result: &ApiResult<T>) -> bool {
    matches!(result, Err(error) if error.code == "wrong_runtime_context")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_read_and_mutation_cannot_reach_a_switched_unlocked_vault() {
    let directory = tempfile::tempdir().unwrap();
    let original = test_vault(directory.path().join("original.serverus"), "old-secret");
    let replacement = test_vault(directory.path().join("replacement.serverus"), "new-secret");
    let replacement_id = replacement.vault_id();
    let app = test_app(original);
    let state = app.state::<AppState>();
    state
        .application
        .activate_selected_vault(state.vault.lock().unwrap().vault_id())
        .unwrap();

    let lifecycle = state.application.lock_lifecycle().await;
    let mut read = Box::pin(connection_secrets(
        app.state::<AppState>(),
        CONNECTION_ID.into(),
    ));
    let mut mutation = Box::pin(known_host_remove(
        app.state::<AppState>(),
        PROTECTED_HOST.into(),
    ));
    let switch = {
        let mut manager = state.vault.lock().unwrap();
        poll_once_pending(read.as_mut());
        poll_once_pending(mutation.as_mut());
        let switch = state.application.begin_vault_switch().unwrap();
        *manager = replacement;
        switch
    };
    switch.commit().await.unwrap();
    state
        .application
        .activate_selected_vault(replacement_id)
        .unwrap();
    drop(lifecycle);

    let read_result = read.await;
    let mutation_result = mutation.await;
    let protected_host_remains = state
        .vault
        .lock()
        .unwrap()
        .payload()
        .unwrap()
        .known_hosts
        .contains_key(PROTECTED_HOST);

    assert!(
        was_rejected_as_stale(&read_result),
        "a stale queued read returned data from the replacement vault"
    );
    assert!(
        was_rejected_as_stale(&mutation_result),
        "a stale queued mutation was accepted by the replacement vault"
    );
    assert!(protected_host_remains, "the replacement vault was mutated");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_read_and_mutation_cannot_survive_lock_then_reunlock() {
    let directory = tempfile::tempdir().unwrap();
    let manager = test_vault(directory.path().join("vault.serverus"), "secret");
    let app = test_app(manager);
    let state = app.state::<AppState>();
    state
        .application
        .activate_selected_vault(state.vault.lock().unwrap().vault_id())
        .unwrap();

    let lifecycle = state.application.lock_lifecycle().await;
    let mut read = Box::pin(connection_secrets(
        app.state::<AppState>(),
        CONNECTION_ID.into(),
    ));
    let mut mutation = Box::pin(known_host_remove(
        app.state::<AppState>(),
        PROTECTED_HOST.into(),
    ));
    {
        let mut manager = state.vault.lock().unwrap();
        poll_once_pending(read.as_mut());
        poll_once_pending(mutation.as_mut());
        state.application.lock_vault().unwrap();
        manager.lock();
        manager.unlock_with_password("master-password").unwrap();
        state
            .application
            .activate_selected_vault(manager.vault_id())
            .unwrap();
    }
    drop(lifecycle);

    let read_result = read.await;
    let mutation_result = mutation.await;
    let protected_host_remains = state
        .vault
        .lock()
        .unwrap()
        .payload()
        .unwrap()
        .known_hosts
        .contains_key(PROTECTED_HOST);

    assert!(
        was_rejected_as_stale(&read_result),
        "a pre-lock queued read survived a new unlock authorization"
    );
    assert!(
        was_rejected_as_stale(&mutation_result),
        "a pre-lock queued mutation survived a new unlock authorization"
    );
    assert!(protected_host_remains, "the re-unlocked vault was mutated");
}
