use super::*;

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
