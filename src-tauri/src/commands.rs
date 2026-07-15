//! Tauri commands: thin layer — parse input, call a module, return the result.
//! Blocking work (KDF, disk, keychain prompts) runs in `spawn_blocking`.

use serde::Serialize;
use specta::Type;
use tauri::State;
use tauri_specta::Event;
use zeroize::Zeroizing;

use crate::error::{ApiResult, AppError, AppResult};
use crate::events::VaultLockedEvent;
use crate::state::AppState;
use crate::vault::format::KdfParams;
use crate::vault::model::{Badge, ConnectionInput, PublicVault, Settings, TreeNode};
use crate::vault::tree;

/// Run a blocking closure off the async runtime and flatten errors.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> ApiResult<T> {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(result) => result.map_err(Into::into),
        Err(e) => Err(AppError::Other(format!("background task failed: {e}")).into()),
    }
}

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
pub struct VaultInfo {
    pub path: String,
    pub exists: bool,
    pub unlocked: bool,
    pub biometry_available: bool,
    /// A DEK for this vault is stored behind biometrics — Touch ID unlock
    /// can be offered right away.
    pub quick_unlock_ready: bool,
    /// UI label for the platform's quick-unlock mechanism
    /// ("Touch ID" / "Windows Hello").
    pub quick_unlock_method: String,
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

/// Open an http(s) URL in the user's default browser (used by the About
/// section). Each OS opener delegates and returns immediately; the URL is
/// passed as a single argument, never through a shell.
#[tauri::command]
#[specta::specta]
pub async fn open_external(url: String) -> ApiResult<()> {
    blocking(move || {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(AppError::Other("only http(s) URLs may be opened".into()));
        }
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(target_os = "windows")]
        let program = "explorer";
        #[cfg(all(unix, not(target_os = "macos")))]
        let program = "xdg-open";
        std::process::Command::new(program)
            .arg(&url)
            .spawn()
            .map_err(|e| AppError::Other(format!("failed to open URL: {e}")))?;
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_get_info(state: State<'_, AppState>) -> ApiResult<VaultInfo> {
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    blocking(move || {
        let mgr = vault.lock().unwrap();
        let biometry = quick.is_available();
        Ok(VaultInfo {
            path: mgr.path().to_string_lossy().into_owned(),
            exists: mgr.exists(),
            unlocked: mgr.is_unlocked(),
            biometry_available: biometry,
            quick_unlock_ready: biometry && quick.has_dek(&mgr.vault_id()),
            quick_unlock_method: quick.method_name().to_string(),
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_create(state: State<'_, AppState>, password: String) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    let activity = state.activity.clone();
    blocking(move || {
        let password = Zeroizing::new(password);
        let mut mgr = vault.lock().unwrap();
        mgr.create(&password, KdfParams::default())?;
        if mgr.payload()?.settings.security.touch_id && quick.is_available() {
            // Best-effort: quick unlock failing must never block vault use.
            let _ = quick.store_dek(&mgr.vault_id(), mgr.dek()?);
        }
        let public = mgr.payload()?.to_public();
        activity.touch();
        Ok(public)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_unlock_password(
    state: State<'_, AppState>,
    password: String,
) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    let activity = state.activity.clone();
    blocking(move || {
        let password = Zeroizing::new(password);
        let mut mgr = vault.lock().unwrap();
        mgr.unlock_with_password(&password)?;
        // Re-arm quick unlock: also heals a keychain entry invalidated by a
        // fingerprint-set change (SPEC §2.3).
        if mgr.payload()?.settings.security.touch_id && quick.is_available() {
            let _ = quick.store_dek(&mgr.vault_id(), mgr.dek()?);
        }
        let public = mgr.payload()?.to_public();
        activity.touch();
        Ok(public)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_unlock_quick(state: State<'_, AppState>) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    let activity = state.activity.clone();
    blocking(move || {
        // Prompt outside the vault lock: the Touch ID dialog can sit there
        // for a while and must not block other vault reads.
        let vault_id = vault.lock().unwrap().vault_id();
        let dek = quick.retrieve_dek(&vault_id)?;
        let mut mgr = vault.lock().unwrap();
        mgr.unlock_with_dek(&dek)?;
        let public = mgr.payload()?.to_public();
        activity.touch();
        Ok(public)
    })
    .await
}

#[cfg(test)]
mod vault_unlock_activity_tests {
    use super::{vault_create, vault_unlock_password, vault_unlock_quick};
    use crate::autolock::ActivityTracker;
    use crate::error::AppResult;
    use crate::session::SessionManager;
    use crate::state::AppState;
    use crate::transfer::TransferManager;
    use crate::vault::format::KdfParams;
    use crate::vault::quick_unlock::{NoQuickUnlock, QuickUnlock};
    use crate::vault::VaultManager;
    use crate::watcher::EditWatcher;
    use std::sync::{Arc, Mutex};
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
            .manage(AppState {
                vault: Arc::new(Mutex::new(vault)),
                quick,
                sessions: Arc::new(SessionManager::default()),
                transfers: Arc::new(TransferManager::default()),
                edits: Arc::new(EditWatcher::default()),
                activity,
            })
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
    }
}

#[tauri::command]
#[specta::specta]
pub async fn vault_lock(app: tauri::AppHandle, state: State<'_, AppState>) -> ApiResult<()> {
    let vault = state.vault.clone();
    blocking(move || {
        vault.lock().unwrap().lock();
        Ok(())
    })
    .await?;
    let _ = VaultLockedEvent.emit(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn vault_change_password(
    state: State<'_, AppState>,
    current_password: String,
    new_password: String,
) -> ApiResult<()> {
    let vault = state.vault.clone();
    blocking(move || {
        let current = Zeroizing::new(current_password);
        let new = Zeroizing::new(new_password);
        let mut mgr = vault.lock().unwrap();
        mgr.change_password(&current, &new)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_set_touch_id(state: State<'_, AppState>, enabled: bool) -> ApiResult<()> {
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            p.settings.security.touch_id = enabled;
            Ok(())
        })?;
        let id = mgr.vault_id();
        if enabled {
            if quick.is_available() {
                quick.store_dek(&id, mgr.dek()?)?;
            }
        } else {
            quick.clear(&id);
        }
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------------------
// Connections & tree (M1)
// ---------------------------------------------------------------------------

/// Create or update a connection. When creating, the tree node is appended
/// to `parent_folder` (or the root). Returns the updated public vault.
#[tauri::command]
#[specta::specta]
pub async fn connection_upsert(
    state: State<'_, AppState>,
    id: Option<String>,
    input: ConnectionInput,
    parent_folder: Option<String>,
) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            match id {
                Some(id) => {
                    let existing = p
                        .connections
                        .get(&id)
                        .ok_or(AppError::ConnectionNotFound)?
                        .clone();
                    let conn = input.into_connection(Some(&existing));
                    p.connections.insert(id, conn);
                }
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let conn = input.into_connection(None);
                    p.connections.insert(id.clone(), conn);
                    tree::insert_node(
                        &mut p.tree,
                        parent_folder.as_deref(),
                        TreeNode::Connection { id },
                    )?;
                }
            }
            Ok(p.to_public())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn connection_duplicate(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            let mut conn = p
                .connections
                .get(&id)
                .ok_or(AppError::ConnectionNotFound)?
                .clone();
            conn.name = format!("{} copy", conn.name);
            let new_id = uuid::Uuid::new_v4().to_string();
            p.connections.insert(new_id.clone(), conn);
            // Place the copy right after the original when it sits at the
            // root; otherwise append to the same folder.
            tree::insert_after(&mut p.tree, &id, TreeNode::Connection { id: new_id });
            Ok(p.to_public())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn connection_delete(state: State<'_, AppState>, id: String) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            if p.connections.remove(&id).is_none() {
                return Err(AppError::ConnectionNotFound);
            }
            tree::remove_nodes(
                &mut p.tree,
                &|n| matches!(n, TreeNode::Connection { id: cid } if cid == &id),
                false,
            );
            // Detach as jump host anywhere it was referenced.
            for conn in p.connections.values_mut() {
                if conn.jump_host.as_deref() == Some(id.as_str()) {
                    conn.jump_host = None;
                }
            }
            Ok(p.to_public())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn folder_create(
    state: State<'_, AppState>,
    name: String,
    parent_folder: Option<String>,
    badge: Option<Badge>,
) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            tree::insert_node(
                &mut p.tree,
                parent_folder.as_deref(),
                TreeNode::Folder {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    badge,
                    children: vec![],
                    collapsed: false,
                },
            )?;
            Ok(p.to_public())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn folder_update(
    state: State<'_, AppState>,
    id: String,
    name: String,
    badge: Option<Badge>,
) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            tree::update_folder(&mut p.tree, &id, name, badge)?;
            Ok(p.to_public())
        })
    })
    .await
}

/// Delete a folder; its children are lifted to the parent level.
#[tauri::command]
#[specta::specta]
pub async fn folder_delete(state: State<'_, AppState>, id: String) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            tree::remove_nodes(
                &mut p.tree,
                &|n| matches!(n, TreeNode::Folder { id: fid, .. } if fid == &id),
                true,
            );
            Ok(p.to_public())
        })
    })
    .await
}

/// Replace the whole tree (drag & drop reordering). Validated against the
/// connections map before committing.
#[tauri::command]
#[specta::specta]
pub async fn tree_update(
    state: State<'_, AppState>,
    tree: Vec<TreeNode>,
) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            tree::validate_tree(p, &tree)?;
            p.tree = tree;
            Ok(p.to_public())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn settings_update(
    state: State<'_, AppState>,
    mut settings: Settings,
) -> ApiResult<PublicVault> {
    settings.clamp();
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        let was_touch_id = mgr.payload()?.settings.security.touch_id;
        mgr.with_payload(|p| {
            p.settings = settings;
            Ok(())
        })?;
        let now_touch_id = mgr.payload()?.settings.security.touch_id;
        if was_touch_id != now_touch_id {
            let id = mgr.vault_id();
            if now_touch_id {
                if quick.is_available() {
                    let _ = quick.store_dek(&id, mgr.dek()?);
                }
            } else {
                quick.clear(&id);
            }
        }
        Ok(mgr.payload()?.to_public())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn known_host_remove(state: State<'_, AppState>, host: String) -> ApiResult<PublicVault> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            p.known_hosts.remove(&host);
            Ok(p.to_public())
        })
    })
    .await
}

/// Move the vault to a different path (§8 Vault settings). Requires an
/// unlocked vault: the file is re-encrypted and written at the new location,
/// the old file stays as a manual backup.
#[tauri::command]
#[specta::specta]
pub async fn vault_set_path(state: State<'_, AppState>, path: String) -> ApiResult<()> {
    let vault = state.vault.clone();
    let quick = state.quick.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        if !mgr.is_unlocked() {
            return Err(AppError::VaultLocked);
        }
        let old_id = mgr.vault_id();
        mgr.set_path_transactional(local_fs::expand(&path), |resolved| {
            // Persist the path the vault actually ended up at — set_path may
            // have appended the file name when given a folder.
            crate::app_config::save(&crate::app_config::AppConfig {
                vault_path: Some(resolved.to_string_lossy().into_owned()),
            })?;
            Ok(())
        })?;
        // Quick-unlock entries are keyed by path — move them along.
        if mgr.payload()?.settings.security.touch_id && quick.is_available() {
            quick.clear(&old_id);
            let _ = quick.store_dek(&mgr.vault_id(), mgr.dek()?);
        }
        Ok(())
    })
    .await
}

/// Point the app at a different vault file WITHOUT unlocking anything —
/// available from the lock screen (forgot password, multiple vaults).
/// An existing file gets the unlock form, a fresh path gets the create
/// form. The current vault is locked (secrets zeroized) before switching;
/// nothing is moved or rewritten on disk.
fn switch_vault_manager(
    current: &mut crate::vault::VaultManager,
    mut target: std::path::PathBuf,
    persist: impl FnOnce(&crate::app_config::AppConfig) -> std::io::Result<()>,
) -> AppResult<()> {
    // A folder means "the vault file inside it", keeping the file name.
    if target.is_dir() {
        if let Some(name) = current.path().file_name() {
            target = target.join(name);
        }
    }

    let next = crate::vault::VaultManager::new(target);
    // Keep the selected and unlocked runtime vault intact through the only
    // fallible step. Replacing the manager is infallible after persistence.
    persist(&crate::app_config::AppConfig {
        vault_path: Some(next.vault_id()),
    })?;
    current.lock();
    *current = next;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn vault_switch_path(state: State<'_, AppState>, path: String) -> ApiResult<()> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        switch_vault_manager(&mut mgr, local_fs::expand(&path), crate::app_config::save)
    })
    .await
}

#[cfg(test)]
mod vault_switch_path_tests {
    use super::switch_vault_manager;
    use crate::vault::format::KdfParams;
    use crate::vault::VaultManager;

    #[test]
    fn config_failure_preserves_the_selected_unlocked_vault() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.serverus");
        let target = directory.path().join("other.serverus");
        let mut manager = VaultManager::new(original.clone());
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

        let result = switch_vault_manager(&mut manager, target, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "simulated config failure",
            ))
        });

        assert!(result.is_err());
        assert_eq!(manager.path(), original);
        assert!(manager.is_unlocked());
        assert!(manager.payload().is_ok());
    }
}

// ---------------------------------------------------------------------------
// Sessions & terminals (M2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
pub struct SessionDto {
    pub session_id: String,
    pub connection_id: String,
}

/// Open an SSH session. On an unknown or changed host key this fails with
/// code `host_key_prompt` and a `host_key` payload; the UI confirms with the
/// user, calls `host_key_accept`, then retries.
#[tauri::command]
#[specta::specta]
pub async fn session_connect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
) -> ApiResult<SessionDto> {
    let sessions = state.sessions.clone();
    let vault = state.vault.clone();
    match sessions.connect(&app, &vault, &connection_id).await {
        Ok(Ok(entry)) => {
            // Autostart tunnels flagged in the connection config (SPEC §4.2).
            if let Some(ssh) = entry.ssh.clone() {
                let autostart: Vec<crate::vault::model::TunnelConfig> = vault
                    .lock()
                    .unwrap()
                    .payload()
                    .map(|p| {
                        p.connections
                            .get(&connection_id)
                            .map(|c| c.tunnels.iter().filter(|t| t.autostart).cloned().collect())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                for t in autostart {
                    let _ = sessions
                        .tunnels
                        .start(
                            ssh.clone(),
                            &entry.id,
                            &t.name,
                            t.local_port,
                            &t.remote_host,
                            t.remote_port,
                        )
                        .await;
                }
            }
            Ok(SessionDto {
                session_id: entry.id.clone(),
                connection_id: entry.connection_id.clone(),
            })
        }
        Ok(Err(issue)) => Err(crate::error::ApiError {
            code: "host_key_prompt".into(),
            message: if issue.changed {
                format!(
                    "HOST KEY CHANGED for {}:{} — possible man-in-the-middle attack",
                    issue.host, issue.port
                )
            } else {
                format!("Unknown host {}:{}", issue.host, issue.port)
            },
            host_key: Some(crate::error::HostKeyPrompt {
                host: issue.host,
                port: issue.port,
                algorithm: issue.algorithm,
                fingerprint: issue.fingerprint,
                key_line: issue.key_line,
                changed: issue.changed,
            }),
        }),
        Err(e) => Err(e.into()),
    }
}

/// Store an accepted host key in the vault (SPEC §4.1).
#[tauri::command]
#[specta::specta]
pub async fn host_key_accept(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    key_line: String,
) -> ApiResult<()> {
    let vault = state.vault.clone();
    blocking(move || {
        let mut mgr = vault.lock().unwrap();
        mgr.with_payload(|p| {
            p.known_hosts.insert(format!("{host}:{port}"), key_line);
            Ok(())
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn session_disconnect(state: State<'_, AppState>, session_id: String) -> ApiResult<()> {
    state.edits.close_session(&session_id).await;
    // Queue + history are per-connection: drop them when the tab closes.
    state.transfers.clear_session(&session_id);
    state.sessions.disconnect(&session_id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn term_open(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> ApiResult<String> {
    state
        .sessions
        .term_open(app, &session_id, cols, rows)
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn term_write(
    state: State<'_, AppState>,
    term_id: String,
    data: String,
) -> ApiResult<()> {
    state
        .sessions
        .term_write(&term_id, data.as_bytes())
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn term_resize(
    state: State<'_, AppState>,
    term_id: String,
    cols: u16,
    rows: u16,
) -> ApiResult<()> {
    state
        .sessions
        .term_resize(&term_id, cols, rows)
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn term_close(state: State<'_, AppState>, term_id: String) -> ApiResult<()> {
    state.sessions.term_close(&term_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// File panels: local + remote (M3)
// ---------------------------------------------------------------------------

use crate::local_fs;
use crate::session::remote_fs::{self, RemoteEntry};

#[tauri::command]
#[specta::specta]
pub async fn local_list(path: String) -> ApiResult<Vec<RemoteEntry>> {
    blocking(move || local_fs::list(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_home() -> ApiResult<String> {
    Ok(local_fs::home())
}

#[tauri::command]
#[specta::specta]
pub async fn local_mkdir(path: String) -> ApiResult<()> {
    blocking(move || local_fs::mkdir(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_create_file(path: String) -> ApiResult<()> {
    blocking(move || local_fs::create_file(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_rename(from: String, to: String) -> ApiResult<()> {
    blocking(move || local_fs::rename(&from, &to)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_delete(path: String) -> ApiResult<()> {
    blocking(move || local_fs::delete(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn local_chmod(path: String, mode: u32) -> ApiResult<()> {
    blocking(move || local_fs::chmod(&path, mode)).await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> ApiResult<Vec<RemoteEntry>> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    fs.list(&path).await.map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn remote_home(state: State<'_, AppState>, session_id: String) -> ApiResult<String> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    fs.home_dir().await.map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn remote_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> ApiResult<()> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    fs.mkdir(&path).await.map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn remote_create_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> ApiResult<()> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    fs.create_file(&path).await.map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn remote_rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> ApiResult<()> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    fs.rename(&from, &to).await.map_err(Into::into)
}

/// Recursive delete — works identically for SFTP and FTP (SPEC §4.3).
#[tauri::command]
#[specta::specta]
pub async fn remote_delete(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    is_dir: bool,
) -> ApiResult<()> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    remote_fs::delete_recursive(fs.as_ref(), &path, is_dir)
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn remote_chmod(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: u32,
) -> ApiResult<()> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    fs.chmod(&path, mode).await.map_err(Into::into)
}

// ---------------------------------------------------------------------------
// S3 ACLs (SPEC §4.4) — public/private handling lives outside RemoteFs
// ---------------------------------------------------------------------------

use crate::session::s3::{S3AclEntry, S3AclTarget};
use crate::vault::model::S3UploadAcl;

fn s3_of(
    state: &AppState,
    session_id: &str,
) -> AppResult<std::sync::Arc<crate::session::s3::S3Fs>> {
    state
        .sessions
        .get(session_id)?
        .s3
        .clone()
        .ok_or_else(|| AppError::Other("not an S3 session".into()))
}

/// Public/private status for a batch of objects — fetched in the background
/// after a listing; failures come back as `unknown`, never as an error.
#[tauri::command]
#[specta::specta]
pub async fn s3_acl_status(
    state: State<'_, AppState>,
    session_id: String,
    paths: Vec<String>,
) -> ApiResult<Vec<S3AclEntry>> {
    let fs = s3_of(&state, &session_id)?;
    Ok(fs.acl_status_batch(paths).await)
}

/// Make objects public or private; directories apply recursively to every
/// object under the prefix. Returns the number of objects changed.
#[tauri::command]
#[specta::specta]
pub async fn s3_set_acl(
    state: State<'_, AppState>,
    session_id: String,
    targets: Vec<S3AclTarget>,
    make_public: bool,
) -> ApiResult<u32> {
    let fs = s3_of(&state, &session_id)?;
    fs.set_acl(targets, make_public).await.map_err(Into::into)
}

/// Switch the ACL applied to subsequent uploads: the pane toggle and the
/// "ask" dialog resolve here. Persists the choice in the connection config
/// and applies it to the selected live session.
fn persist_s3_upload_acl(
    vault: &std::sync::Mutex<crate::vault::VaultManager>,
    connection_id: &str,
    mode: S3UploadAcl,
    apply_live: impl FnOnce(S3UploadAcl),
) -> AppResult<Option<PublicVault>> {
    let mut mgr = vault.lock().unwrap();
    let updated = mgr.with_payload(|payload| {
        let connection = payload
            .connections
            .get_mut(connection_id)
            .ok_or(AppError::ConnectionNotFound)?;
        connection
            .s3
            .get_or_insert_with(Default::default)
            .upload_acl = mode;
        Ok(Some(payload.to_public()))
    })?;
    // Keep the vault mutex through the live update. Concurrent persisted
    // changes therefore commit and take effect in one consistent order.
    apply_live(mode);
    Ok(updated)
}

#[tauri::command]
#[specta::specta]
pub async fn s3_set_upload_acl(
    state: State<'_, AppState>,
    session_id: String,
    mode: S3UploadAcl,
    persist: bool,
) -> ApiResult<Option<PublicVault>> {
    let fs = s3_of(&state, &session_id)?;
    if !persist {
        fs.set_upload_acl(mode);
        return Ok(None);
    }
    let connection_id = state.sessions.get(&session_id)?.connection_id.clone();
    let vault = state.vault.clone();
    blocking(move || {
        persist_s3_upload_acl(vault.as_ref(), &connection_id, mode, |committed| {
            fs.set_upload_acl(committed);
        })
    })
    .await
}

#[cfg(test)]
mod s3_upload_acl_tests {
    use super::persist_s3_upload_acl;
    use crate::vault::format::KdfParams;
    use crate::vault::model::{Connection, S3UploadAcl};
    use crate::vault::VaultManager;
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
            persist_s3_upload_acl(vault.as_ref(), "connection", mode, move |committed| {
                assert!(matches!(
                    vault_during_apply.try_lock(),
                    Err(TryLockError::WouldBlock)
                ));
                *live_during_apply.lock().unwrap() = committed;
            })
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
}

// ---------------------------------------------------------------------------
// Transfer queue (M3)
// ---------------------------------------------------------------------------

use crate::transfer::{ConflictAction, TransferSnapshot, TransferSummary};

fn transfer_settings(state: &AppState) -> crate::vault::model::TransferSettings {
    state
        .vault
        .lock()
        .unwrap()
        .payload()
        .map(|p| p.settings.transfers.clone())
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_upload(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    local_path: String,
    remote_dir: String,
) -> ApiResult<()> {
    let entry = state.sessions.get(&session_id)?;
    let fs = entry.remote_fs().await?;
    let settings = transfer_settings(&state);
    let tar_ssh = entry.tar_ssh().await;
    let sink: std::sync::Arc<dyn crate::transfer::ProgressSink> = std::sync::Arc::new(app);
    state
        .transfers
        .enqueue_upload_accelerated(
            &sink,
            fs,
            &session_id,
            &local_path,
            &remote_dir,
            settings,
            tar_ssh,
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    local_dir: String,
) -> ApiResult<()> {
    let entry = state.sessions.get(&session_id)?;
    let fs = entry.remote_fs().await?;
    let settings = transfer_settings(&state);
    let tar_ssh = entry.tar_ssh().await;
    let sink: std::sync::Arc<dyn crate::transfer::ProgressSink> = std::sync::Arc::new(app);
    state
        .transfers
        .enqueue_download_accelerated(
            &sink,
            fs,
            &session_id,
            &remote_path,
            &local_dir,
            settings,
            tar_ssh,
        )
        .await
        .map_err(Into::into)
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TransferListDto {
    pub items: Vec<TransferSnapshot>,
    pub summary: TransferSummary,
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_list(state: State<'_, AppState>) -> ApiResult<TransferListDto> {
    let (items, summary) = state.transfers.snapshot();
    Ok(TransferListDto { items, summary })
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_pause(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    state.transfers.pause(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_resume(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    state.transfers.resume(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_cancel(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    state.transfers.cancel(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_pause_all(state: State<'_, AppState>) -> ApiResult<()> {
    state.transfers.pause_all();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_resume_all(state: State<'_, AppState>) -> ApiResult<()> {
    state.transfers.resume_all();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_cancel_all(state: State<'_, AppState>) -> ApiResult<()> {
    state.transfers.cancel_all();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_clear_finished(state: State<'_, AppState>) -> ApiResult<()> {
    state.transfers.clear_finished();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_resolve(
    state: State<'_, AppState>,
    session_id: String,
    id: String,
    action: ConflictAction,
    apply_to_all: bool,
) -> ApiResult<()> {
    state
        .transfers
        .resolve_conflict(&session_id, &id, action, apply_to_all);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transfer_retry(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    let sink: std::sync::Arc<dyn crate::transfer::ProgressSink> = std::sync::Arc::new(app);
    state.transfers.retry(&sink, &id).await.map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Remote edit (M5)
// ---------------------------------------------------------------------------

/// Download a remote file into the isolated edit cache, open it in the
/// configured editor and auto-upload every save (SPEC §5.3).
#[tauri::command]
#[specta::specta]
pub async fn remote_edit_open(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
) -> ApiResult<()> {
    let fs = state.sessions.get(&session_id)?.remote_fs().await?;
    let editor = state
        .vault
        .lock()
        .unwrap()
        .payload()
        .map(|p| p.settings.editor.clone())
        .unwrap_or_default();
    state
        .edits
        .open(app, fs, &session_id, &remote_path, &editor)
        .await
        .map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Tunnels (M6)
// ---------------------------------------------------------------------------

use crate::session::tunnel::TunnelStatus;

#[tauri::command]
#[specta::specta]
pub async fn tunnel_start(
    state: State<'_, AppState>,
    session_id: String,
    name: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
) -> ApiResult<TunnelStatus> {
    let ssh = state.sessions.ssh_of(&session_id)?;
    state
        .sessions
        .tunnels
        .start(
            ssh,
            &session_id,
            &name,
            local_port,
            &remote_host,
            remote_port,
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_stop(state: State<'_, AppState>, tunnel_id: String) -> ApiResult<()> {
    state.sessions.tunnels.stop(&tunnel_id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn tunnel_list(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> ApiResult<Vec<TunnelStatus>> {
    Ok(state.sessions.tunnels.list(session_id.as_deref()))
}

// ---------------------------------------------------------------------------
// Auto-lock support & config export (M7)
// ---------------------------------------------------------------------------

/// Throttled user-activity ping for the auto-lock timer (SPEC §2.4).
#[tauri::command]
#[specta::specta]
pub async fn vault_touch_activity(state: State<'_, AppState>) -> ApiResult<()> {
    state.activity.touch();
    Ok(())
}

/// Export an UNENCRYPTED copy of the configuration without any secrets
/// (SPEC §8) — passwords, passphrases and inline keys are omitted.
#[tauri::command]
#[specta::specta]
pub async fn vault_export_config(state: State<'_, AppState>, path: String) -> ApiResult<()> {
    let vault = state.vault.clone();
    blocking(move || {
        let mgr = vault.lock().unwrap();
        let public = mgr.payload()?.to_public();
        let json = serde_json::to_vec_pretty(&public)
            .map_err(|e| AppError::Other(format!("serialize: {e}")))?;
        std::fs::write(&path, json)?;
        Ok(())
    })
    .await
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct ImportReport {
    /// Number of connections created or updated by the import.
    pub connections: u32,
    pub vault: PublicVault,
}

/// Import a config file (a Serverus export or a hand-written file following
/// docs/CONFIG_FORMAT.md) into the unlocked vault. Merge semantics live in
/// `vault::import`.
#[tauri::command]
#[specta::specta]
pub async fn vault_import_config(
    state: State<'_, AppState>,
    path: String,
) -> ApiResult<ImportReport> {
    let vault = state.vault.clone();
    blocking(move || {
        let json = std::fs::read_to_string(&path)?;
        let mut mgr = vault.lock().unwrap();
        let mut connections = 0;
        let vault = mgr.with_payload(|p| {
            connections = crate::vault::import::apply(p, &json)?;
            Ok(p.to_public())
        })?;
        Ok(ImportReport { connections, vault })
    })
    .await
}

/// Read a private key file so the UI can store its text inside the vault
/// (the key then travels with vault backups). Validated in `local_fs` —
/// only PEM-looking files are returned.
#[tauri::command]
#[specta::specta]
pub async fn ssh_key_read_file(path: String) -> ApiResult<String> {
    blocking(move || local_fs::read_private_key(&path)).await
}

/// Decrypted secrets for one connection, for pre-filling the edit form.
/// Safe: the vault is already unlocked (master password / Touch ID), and the
/// values are never persisted outside the encrypted vault.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ConnectionSecrets {
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
    pub key_inline: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn connection_secrets(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<ConnectionSecrets> {
    let vault = state.vault.clone();
    blocking(move || {
        let mgr = vault.lock().unwrap();
        let conn = mgr
            .payload()?
            .connections
            .get(&id)
            .ok_or(AppError::ConnectionNotFound)?;
        Ok(ConnectionSecrets {
            password: conn.auth.password.clone(),
            key_passphrase: conn.auth.key_passphrase.clone(),
            key_inline: conn.auth.key_inline.clone(),
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Drag & drop helpers (Finder integration)
// ---------------------------------------------------------------------------

/// Write the drag preview icon to a temp file once and return its path.
/// tauri-plugin-drag needs an on-disk image for the OS drag cursor.
#[tauri::command]
#[specta::specta]
pub async fn drag_preview_icon() -> ApiResult<String> {
    blocking(|| {
        let path = std::env::temp_dir().join("serverus-drag-icon.png");
        if !path.exists() {
            const ICON: &[u8] = include_bytes!("../icons/128x128.png");
            std::fs::write(&path, ICON)?;
        }
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

/// Copy files/dirs into `dest_dir` on the local filesystem (Finder → local
/// pane drop). Skips items already inside `dest_dir` (dropped onto self).
#[tauri::command]
#[specta::specta]
pub async fn local_copy_into(paths: Vec<String>, dest_dir: String) -> ApiResult<()> {
    blocking(move || {
        let dest = local_fs::expand(&dest_dir);
        for p in paths {
            let src = std::path::PathBuf::from(&p);
            let Some(name) = src.file_name() else {
                continue;
            };
            let target = dest.join(name);
            if src.parent() == Some(dest.as_path()) || src == target {
                continue; // same directory — nothing to do
            }
            copy_recursive(&src, &target)?;
        }
        Ok(())
    })
    .await
}

fn copy_recursive(src: &std::path::Path, dest: &std::path::Path) -> AppResult<()> {
    let mut pending_permissions = Vec::new();
    copy_recursive_inner(src, dest, &mut pending_permissions)?;

    for (path, permissions, _) in &pending_permissions {
        if let Err(error) = std::fs::set_permissions(path, permissions.clone()) {
            make_partial_copy_removable(&pending_permissions);
            if let Err(cleanup_error) = remove_partial_copy(dest) {
                return Err(AppError::Other(format!(
                    "failed to apply copied permissions: {error}; failed to remove partial copy: {cleanup_error}"
                )));
            }
            return Err(error.into());
        }
    }

    Ok(())
}

type PendingPermission = (std::path::PathBuf, std::fs::Permissions, bool);

fn make_partial_copy_removable(pending_permissions: &[PendingPermission]) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for (path, _, is_directory) in pending_permissions.iter().rev() {
            let mode = if *is_directory { 0o700 } else { 0o600 };
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
        }
    }
    #[cfg(windows)]
    for (path, permissions, _) in pending_permissions.iter().rev() {
        let mut writable_permissions = permissions.clone();
        clear_windows_readonly(&mut writable_permissions);
        let _ = std::fs::set_permissions(path, writable_permissions);
    }
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn clear_windows_readonly(permissions: &mut std::fs::Permissions) {
    // Windows exposes a read-only file attribute rather than Unix mode bits,
    // so clearing it does not make the partial copy world-writable.
    permissions.set_readonly(false);
}

fn remove_partial_copy(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn copy_recursive_inner(
    src: &std::path::Path,
    dest: &std::path::Path,
    pending_permissions: &mut Vec<PendingPermission>,
) -> AppResult<()> {
    let meta = std::fs::symlink_metadata(src)?;
    let source = std::fs::canonicalize(src)?;
    let dest_parent = dest
        .parent()
        .ok_or_else(|| AppError::Other("copy destination has no parent".into()))?;
    let destination = std::fs::canonicalize(dest_parent)?.join(
        dest.file_name()
            .ok_or_else(|| AppError::Other("copy destination has no file name".into()))?,
    );

    if meta.is_dir() && destination.starts_with(&source) {
        return Err(AppError::Other(
            "cannot copy a directory inside the source directory".into(),
        ));
    }

    match std::fs::symlink_metadata(dest) {
        Ok(_) => {
            return Err(AppError::Other(format!(
                "{}: already exists",
                dest.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    if meta.is_dir() {
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;

            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = std::fs::DirBuilder::new();
        builder.create(dest).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::Other(format!("{}: already exists", dest.display()))
            } else {
                error.into()
            }
        })?;

        let copy_result = (|| -> AppResult<()> {
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                copy_recursive_inner(
                    &entry.path(),
                    &dest.join(entry.file_name()),
                    pending_permissions,
                )?;
            }
            #[cfg(unix)]
            pending_permissions.push((dest.to_path_buf(), meta.permissions(), true));
            Ok(())
        })();
        if let Err(error) = copy_result {
            if let Err(cleanup_error) = std::fs::remove_dir_all(dest) {
                return Err(AppError::Other(format!(
                    "copy failed: {error}; failed to remove partial copy: {cleanup_error}"
                )));
            }
            return Err(error);
        }
    } else {
        let mut source_file = std::fs::File::open(src)?;
        // `src` may be a symlink. Apply the permissions of the file we
        // actually opened, never the typically world-accessible link mode.
        let source_permissions = source_file.metadata()?.permissions();
        let mut destination_options = std::fs::OpenOptions::new();
        destination_options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Private files such as SSH keys must never spend the copy window
            // with broader umask-derived permissions than their source.
            destination_options.mode(0o600);
        }
        let mut destination_file = destination_options.open(dest).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::Other(format!("{}: already exists", dest.display()))
            } else {
                error.into()
            }
        })?;
        let copy_result = (|| -> std::io::Result<()> {
            std::io::copy(&mut source_file, &mut destination_file)?;
            destination_file.sync_all()
        })();
        drop(destination_file);
        if let Err(error) = copy_result {
            if let Err(cleanup_error) = std::fs::remove_file(dest) {
                return Err(AppError::Other(format!(
                    "copy failed: {error}; failed to remove partial copy: {cleanup_error}"
                )));
            }
            return Err(error.into());
        }
        pending_permissions.push((dest.to_path_buf(), source_permissions, false));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::copy_recursive;

    #[test]
    fn local_copy_refuses_to_overwrite_an_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.txt");
        let destination = root.path().join("destination.txt");
        std::fs::write(&source, "new contents").unwrap();
        std::fs::write(&destination, "keep me").unwrap();

        let error = copy_recursive(&source, &destination).unwrap_err();

        assert!(error.to_string().contains("already exists"));
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "keep me");
    }

    #[test]
    fn local_copy_refuses_to_overwrite_an_existing_directory() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("keep.txt"), "keep me").unwrap();

        let error = copy_recursive(&source, &destination).unwrap_err();

        assert!(error.to_string().contains("already exists"));
        assert_eq!(
            std::fs::read_to_string(destination.join("keep.txt")).unwrap(),
            "keep me"
        );
    }

    #[test]
    fn local_copy_refuses_to_copy_a_directory_into_its_descendant() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("file.txt"), "contents").unwrap();
        let destination = source.join("nested");

        let error = copy_recursive(&source, &destination).unwrap_err();

        assert!(error.to_string().contains("inside the source directory"));
        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[test]
    fn local_copy_detects_a_descendant_through_a_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let alias = root.path().join("source-alias");
        std::fs::create_dir(&source).unwrap();
        symlink(&source, &alias).unwrap();
        let destination = alias.join("nested");

        let error = copy_recursive(&source, &destination).unwrap_err();

        assert!(error.to_string().contains("inside the source directory"));
        assert!(!source.join("nested").exists());
    }

    #[cfg(unix)]
    #[test]
    fn local_copy_refuses_to_replace_a_symlink() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.txt");
        let target = root.path().join("target.txt");
        let destination = root.path().join("destination.txt");
        std::fs::write(&source, "new contents").unwrap();
        std::fs::write(&target, "keep me").unwrap();
        symlink(&target, &destination).unwrap();

        let error = copy_recursive(&source, &destination).unwrap_err();

        assert!(error.to_string().contains("already exists"));
        assert_eq!(std::fs::read_to_string(target).unwrap(), "keep me");
    }

    #[test]
    fn local_copy_copies_a_directory_tree() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/file.txt"), "contents").unwrap();

        copy_recursive(&source, &destination).unwrap();

        assert_eq!(
            std::fs::read_to_string(destination.join("nested/file.txt")).unwrap(),
            "contents"
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_copy_uses_followed_file_permissions_for_a_symlink_source() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("private-key");
        let source = root.path().join("private-key-link");
        let destination = root.path().join("copy");
        std::fs::write(&target, "secret").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &source).unwrap();

        copy_recursive(&source, &destination).unwrap();

        assert_eq!(
            std::fs::metadata(destination).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_directory_copy_removes_the_partial_destination() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        symlink(source.join("missing"), source.join("dangling")).unwrap();

        assert!(copy_recursive(&source, &destination).is_err());

        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_directory_copy_removes_restricted_subdirectories() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        let restricted = source.join("a-restricted");
        std::fs::create_dir_all(&restricted).unwrap();
        std::fs::write(restricted.join("copied.txt"), "contents").unwrap();
        std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o500)).unwrap();
        symlink(source.join("missing"), source.join("z-dangling")).unwrap();

        let result = copy_recursive(&source, &destination);
        std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(result.is_err());
        assert!(!destination.exists());
    }
}
