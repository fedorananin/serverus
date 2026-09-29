use serverus_domain::agent::access::AccessLevel;

use crate::agent::catalog::{catalog, resolve};
use crate::vault::model::{
    AgentAccessLevel, AuthConfig, AuthMethod, Connection, Protocol, TreeNode, VaultPayload,
};

fn connection(name: &str, protocol: Protocol, level: Option<AgentAccessLevel>) -> Connection {
    Connection {
        name: name.into(),
        badge: None,
        protocol,
        host: format!("{}.example", name.to_lowercase()),
        port: 22,
        auth: AuthConfig {
            method: AuthMethod::Password,
            username: "deploy".into(),
            password: Some("secret".into()),
            key_path: None,
            key_inline: None,
            key_passphrase: None,
        },
        jump_host: None,
        ftp: None,
        s3: None,
        remote_dir: None,
        local_dir: None,
        tunnels: vec![],
        disable_terminal: false,
        notes: "root password: hunter2".into(),
        agent_access: level,
    }
}

fn folder(id: &str, level: Option<AgentAccessLevel>, children: Vec<TreeNode>) -> TreeNode {
    TreeNode::Folder {
        id: id.into(),
        name: id.into(),
        badge: None,
        children,
        collapsed: false,
        agent_access: level,
    }
}

fn node(id: &str) -> TreeNode {
    TreeNode::Connection { id: id.into() }
}

/// Prod (full) / Inner (inherit) / web, db (own: read_only); Staging
/// (inherit → off) / web; loose ftp with its own ask.
fn payload() -> VaultPayload {
    let mut payload = VaultPayload::default();
    for (id, name, protocol, level) in [
        ("p-web", "web", Protocol::Ssh, None),
        (
            "p-db",
            "db",
            Protocol::Ssh,
            Some(AgentAccessLevel::ReadOnly),
        ),
        ("s-web", "web", Protocol::Ssh, None),
        ("files", "Files", Protocol::Ftp, Some(AgentAccessLevel::Ask)),
    ] {
        payload
            .connections
            .insert(id.into(), connection(name, protocol, level));
    }
    payload.tree = vec![
        folder(
            "Prod",
            Some(AgentAccessLevel::Full),
            vec![folder("Inner", None, vec![node("p-web"), node("p-db")])],
        ),
        folder("Staging", None, vec![node("s-web")]),
        node("files"),
    ];
    payload
}

#[test]
fn levels_inherit_from_the_nearest_folder() {
    let servers = catalog(&payload());
    let level = |id: &str| servers.iter().find(|s| s.id == id).unwrap().level;
    assert_eq!(level("p-web"), AccessLevel::Full);
    assert_eq!(level("p-db"), AccessLevel::ReadOnly);
    assert_eq!(level("s-web"), AccessLevel::Off);
    assert_eq!(level("files"), AccessLevel::Ask);
}

#[test]
fn full_access_for_all_overrides_every_level() {
    let mut payload = payload();
    payload.settings.agent.full_access = true;
    assert!(catalog(&payload)
        .iter()
        .all(|server| server.level == AccessLevel::Full));
}

#[test]
fn paths_and_shell_capability() {
    let servers = catalog(&payload());
    let web = servers.iter().find(|s| s.id == "p-web").unwrap();
    assert_eq!(web.path(), "Prod/Inner/web");
    assert!(web.terminal);
    let files = servers.iter().find(|s| s.id == "files").unwrap();
    assert_eq!(files.path(), "Files");
    assert!(!files.terminal);
}

#[test]
fn nothing_secret_reaches_the_agent() {
    let text = serde_json::to_string(&catalog(&payload())).unwrap();
    assert!(!text.contains("secret"));
    assert!(!text.contains("hunter2"));
}

#[test]
fn resolve_by_id_path_and_name() {
    let servers = catalog(&payload());
    assert_eq!(resolve(&servers, "p-db").unwrap().id, "p-db");
    assert_eq!(resolve(&servers, "Prod/Inner/web").unwrap().id, "p-web");
    assert_eq!(resolve(&servers, "db").unwrap().id, "p-db");
    assert_eq!(resolve(&servers, "files").unwrap().id, "files");
    assert_eq!(resolve(&servers, " prod/inner/WEB ").unwrap().id, "p-web");
}

#[test]
fn servers_with_access_off_do_not_exist_for_the_agent() {
    let servers = catalog(&payload());
    // "web" is unique among visible servers: the Staging one is off.
    assert_eq!(resolve(&servers, "web").unwrap().id, "p-web");
    assert!(resolve(&servers, "s-web").is_err());
    assert!(resolve(&servers, "Staging/web").is_err());
}

#[test]
fn ambiguous_names_are_refused_with_candidates() {
    let mut payload = payload();
    payload.settings.agent.full_access = true;
    let servers = catalog(&payload);
    let error = resolve(&servers, "web").unwrap_err();
    assert!(error.contains("Prod/Inner/web"), "{error}");
    assert!(error.contains("Staging/web"), "{error}");
}
