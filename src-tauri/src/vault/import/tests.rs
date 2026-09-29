use super::*;
use crate::vault::model::AgentAccessLevel;

fn payload_with_conn(id: &str, password: Option<&str>) -> VaultPayload {
    let mut p = VaultPayload::default();
    let json = format!(r#"{{"connections":{{"{id}":{{"protocol":"ssh","host":"old.example"}}}}}}"#);
    apply(&mut p, &json).unwrap();
    if let Some(pw) = password {
        p.connections.get_mut(id).unwrap().auth.password = Some(pw.into());
    }
    p
}

#[test]
fn imports_minimal_handwritten_config() {
    let mut p = VaultPayload::default();
    let n = apply(
        &mut p,
        r#"{
          "tree": [
            { "type": "folder", "name": "Work",
              "children": [ { "type": "connection", "id": "web" } ] }
          ],
          "connections": {
            "web": { "protocol": "ssh", "host": "web.example.com",
                     "auth": { "username": "root", "password": "pw" } },
            "cdn": { "protocol": "s3", "host": "fra1.digitaloceanspaces.com" }
          }
        }"#,
    )
    .unwrap();
    assert_eq!(n, 2);
    let web = &p.connections["web"];
    assert_eq!(web.name, "web.example.com"); // defaults to host
    assert_eq!(web.port, 22);
    assert!(matches!(web.auth.method, AuthMethod::Password));
    assert_eq!(web.auth.password.as_deref(), Some("pw"));
    assert_eq!(p.connections["cdn"].port, 443);
    // "cdn" was not in the tree — appended at root; invariants hold.
    assert_eq!(p.tree.len(), 2);
    let tree_copy = p.tree.clone();
    tree::validate_tree(&p, &tree_copy).unwrap();
}

#[test]
fn reimport_is_idempotent_and_keeps_secrets() {
    let mut p = payload_with_conn("c1", Some("secret"));
    let json = r#"{
      "tree": [ { "type": "folder", "id": "f1", "name": "Prod",
                  "children": [ { "type": "connection", "id": "c1" } ] } ],
      "connections": { "c1": { "protocol": "ssh", "host": "new.example" } }
    }"#;
    apply(&mut p, json).unwrap();
    apply(&mut p, json).unwrap();
    assert_eq!(p.tree.len(), 1); // one folder, no duplicates
    assert_eq!(p.connections["c1"].host, "new.example");
    // The file has no password → the stored secret survives.
    assert_eq!(p.connections["c1"].auth.password.as_deref(), Some("secret"));
}

#[test]
fn prunes_unknown_refs_and_detaches_missing_jump_hosts() {
    let mut p = VaultPayload::default();
    let n = apply(
        &mut p,
        r#"{
          "tree": [ { "type": "connection", "id": "ghost" },
                    { "type": "connection", "id": "c1" } ],
          "connections": {
            "c1": { "protocol": "ssh", "host": "h", "jump_host": "nope" }
          }
        }"#,
    )
    .unwrap();
    assert_eq!(n, 1);
    assert_eq!(p.tree.len(), 1); // ghost ref dropped
    assert!(p.connections["c1"].jump_host.is_none());
}

#[test]
fn rejects_garbage_and_leaves_vault_untouched() {
    let mut p = payload_with_conn("keep", None);
    let before = p.tree.len();
    assert!(apply(&mut p, "not json").is_err());
    assert!(apply(&mut p, "{}").is_err()); // nothing to import
                                           // Same connection referenced twice → second ref is dropped, not fatal.
    apply(
        &mut p,
        r#"{"tree":[{"type":"connection","id":"keep"},{"type":"connection","id":"keep"}]}"#,
    )
    .unwrap();
    assert_eq!(p.tree.len(), before);
}

#[test]
fn known_hosts_existing_wins_and_settings_replace() {
    let mut p = VaultPayload::default();
    p.known_hosts
        .insert("h:22".into(), "ssh-ed25519 verified".into());
    let mut settings = Settings::default();
    settings.terminal.font_size = 15;
    let json = format!(
        r#"{{"known_hosts":{{"h:22":"ssh-ed25519 evil","x:22":"ssh-rsa new"}},
            "settings":{}}}"#,
        serde_json::to_string(&settings).unwrap()
    );
    apply(&mut p, &json).unwrap();
    assert_eq!(p.known_hosts["h:22"], "ssh-ed25519 verified");
    assert_eq!(p.known_hosts["x:22"], "ssh-rsa new");
    assert_eq!(p.settings.terminal.font_size, 15);
}

#[test]
fn import_never_grants_agent_access() {
    let mut p = VaultPayload::default();
    p.settings.agent.enabled = false;
    apply(
        &mut p,
        r#"{
          "tree": [ { "type": "folder", "id": "f1", "name": "Prod", "agent_access": "full",
                      "children": [ { "type": "connection", "id": "c1" } ] } ],
          "connections": { "c1": { "protocol": "ssh", "host": "h", "agent_access": "full" } },
          "settings": {
            "security": { "auto_lock_minutes": 5, "lock_on_sleep": true, "touch_id": false },
            "transfers": { "max_parallel_per_server": 2, "conflict_policy": "ask",
                           "preserve_mtime": true, "tar_acceleration": true },
            "editor": { "use_system_default": true, "custom_app": null },
            "terminal": { "font_family": "Menlo", "font_size": 12, "scrollback": 1000 },
            "panels": { "show_hidden": false, "size_format": "kib", "default_local_dir": null },
            "agent": { "enabled": true, "full_access": true }
          }
        }"#,
    )
    .unwrap();
    assert_eq!(p.connections["c1"].agent_access, None);
    assert!(matches!(
        &p.tree[0],
        TreeNode::Folder {
            agent_access: None,
            ..
        }
    ));
    // Other settings are imported; the agent switches are not.
    assert_eq!(p.settings.security.auto_lock_minutes, 5);
    assert!(!p.settings.agent.enabled);
    assert!(!p.settings.agent.full_access);
}

#[test]
fn reimport_keeps_the_agent_access_the_user_gave() {
    let json = r#"{
      "tree": [ { "type": "folder", "id": "f1", "name": "Prod",
                  "children": [ { "type": "connection", "id": "c1" } ] } ],
      "connections": { "c1": { "protocol": "ssh", "host": "h" } }
    }"#;
    let mut p = VaultPayload::default();
    apply(&mut p, json).unwrap();
    p.connections.get_mut("c1").unwrap().agent_access = Some(AgentAccessLevel::Ask);
    if let TreeNode::Folder { agent_access, .. } = &mut p.tree[0] {
        *agent_access = Some(AgentAccessLevel::ReadOnly);
    }

    apply(&mut p, json).unwrap();

    assert_eq!(
        p.connections["c1"].agent_access,
        Some(AgentAccessLevel::Ask)
    );
    assert!(matches!(
        &p.tree[0],
        TreeNode::Folder {
            agent_access: Some(AgentAccessLevel::ReadOnly),
            ..
        }
    ));
}
