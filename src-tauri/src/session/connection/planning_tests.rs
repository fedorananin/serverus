//! Connection-plan authorization regressions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::autolock::ActivityTracker;
use crate::error::AppError;
use crate::state::AppState;
use crate::vault::format::KdfParams;
use crate::vault::model::Connection;
use crate::vault::quick_unlock::NoQuickUnlock;
use crate::vault::VaultManager;

use super::load_authorized_plan;

fn state_with_ftp_connection(path: std::path::PathBuf) -> AppState {
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
    let connection: Connection = serde_json::from_value(serde_json::json!({
        "name": "FTP",
        "protocol": "ftp",
        "host": "127.0.0.1",
        "port": 2121,
        "auth": {
            "method": "password",
            "username": "user",
            "password": "secret"
        },
        "ftp": { "tls": "none", "passive": true }
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
    state.application.activate_selected_vault(vault_id).unwrap();
    state
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_access_cannot_materialize_a_queued_plan_or_start_a_connector() {
    let directory = tempfile::tempdir().unwrap();
    let state = state_with_ftp_connection(directory.path().join("vault.serverus"));
    let lease = state.application.require_unlocked().unwrap();
    let vault = state.vault.clone();
    let application = state.application.clone();
    let connector_started = Arc::new(AtomicBool::new(false));
    let connector_flag = connector_started.clone();
    let (queued, queued_rx) = std::sync::mpsc::channel();

    let loading = {
        let mut manager = state.vault.lock().unwrap();
        let loading = tokio::task::spawn_blocking(move || {
            let _ = queued.send(());
            let manager = vault.lock().unwrap();
            let plan = load_authorized_plan(&manager, "connection", &lease, &application);
            if plan.is_ok() {
                connector_flag.store(true, Ordering::SeqCst);
            }
            plan
        });
        queued_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("plan load reached the vault lock");
        state.application.lock_vault().unwrap();
        manager.lock();
        manager.unlock_with_password("master-password").unwrap();
        state
            .application
            .activate_selected_vault(manager.vault_id())
            .unwrap();
        loading
    };

    let error = match loading.await.unwrap() {
        Ok(_) => panic!("a revoked plan reached the connector boundary"),
        Err(error) => error,
    };
    assert!(matches!(error, AppError::WrongRuntimeContext));
    assert!(!connector_started.load(Ordering::SeqCst));
    assert!(state.sessions.session_ids().is_empty());
}
