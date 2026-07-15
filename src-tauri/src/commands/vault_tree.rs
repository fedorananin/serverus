//! Connection, folder, tree, and settings commands.

use super::prelude::*;

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
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    let quick = state.quick.clone();
    run_unlocked_vault_operation(&state.application, move |mgr| {
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
    run_unlocked_vault_operation(&state.application, move |mgr| {
        mgr.with_payload(|p| {
            p.known_hosts.remove(&host);
            Ok(p.to_public())
        })
    })
    .await
}
