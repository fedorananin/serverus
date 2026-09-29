//! `create_connection`: the agent adds a connection — with the secrets the
//! user asked it to store — to a folder of the sidebar.

use serde::Deserialize;
use serde_json::Value;
use serverus_domain::agent::access::{may_use_as_jump_host, AccessLevel};

use super::args::parse;
use super::{Ctx, ToolError, ToolResult};
use crate::agent::catalog::{access_label, resolve, ServerInfo};
use crate::agent::hub::emit;
use crate::agent::placement::{add_connection, preview_level};
use crate::agent::types::AgentVaultChangedEvent;
use crate::vault::model::{
    AuthConfig, AuthMethod, Connection, FtpOptions, FtpTlsMode, Protocol, S3Options, S3UploadAcl,
};

#[derive(Deserialize)]
pub(super) struct CreateArgs {
    host: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    protocol: Option<Protocol>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    username: String,
    #[serde(default)]
    auth: Option<AuthMethod>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    key_path: Option<String>,
    #[serde(default)]
    key_passphrase: Option<String>,
    #[serde(default)]
    jump_host: Option<String>,
    #[serde(default)]
    remote_dir: Option<String>,
    #[serde(default)]
    local_dir: Option<String>,
    #[serde(default)]
    disable_terminal: bool,
    #[serde(default)]
    ftp_tls: bool,
    #[serde(default)]
    s3_region: Option<String>,
    #[serde(default)]
    s3_bucket: Option<String>,
    #[serde(default)]
    s3_path_style: bool,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    allow_duplicate: bool,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl CreateArgs {
    fn protocol(&self) -> Protocol {
        self.protocol.unwrap_or(Protocol::Ssh)
    }

    fn auth_method(&self) -> AuthMethod {
        self.auth
            .unwrap_or(if self.private_key.is_some() || self.key_path.is_some() {
                AuthMethod::Key
            } else if self.password.is_some() || self.protocol() != Protocol::Ssh {
                AuthMethod::Password
            } else {
                AuthMethod::Agent
            })
    }

    pub(super) fn into_connection(self, jump_host: Option<String>) -> Connection {
        let protocol = self.protocol();
        let method = self.auth_method();
        let host = self.host.trim().to_string();
        Connection {
            name: non_empty(self.name).unwrap_or_else(|| host.clone()),
            badge: None,
            protocol,
            port: self.port.unwrap_or(match protocol {
                Protocol::Ssh => 22,
                Protocol::Ftp => 21,
                Protocol::S3 => 443,
            }),
            host,
            auth: AuthConfig {
                method,
                username: self.username.trim().to_string(),
                password: self.password.filter(|secret| !secret.is_empty()),
                key_path: non_empty(self.key_path),
                key_inline: self.private_key.filter(|secret| !secret.trim().is_empty()),
                key_passphrase: self.key_passphrase.filter(|secret| !secret.is_empty()),
            },
            jump_host,
            ftp: (protocol == Protocol::Ftp).then_some(FtpOptions {
                tls: if self.ftp_tls {
                    FtpTlsMode::Explicit
                } else {
                    FtpTlsMode::None
                },
                passive: true,
            }),
            s3: (protocol == Protocol::S3).then(|| S3Options {
                region: non_empty(self.s3_region),
                bucket: non_empty(self.s3_bucket),
                path_style: self.s3_path_style,
                public_base_url: None,
                upload_acl: S3UploadAcl::Private,
            }),
            remote_dir: non_empty(self.remote_dir),
            local_dir: non_empty(self.local_dir),
            tunnels: Vec::new(),
            disable_terminal: protocol == Protocol::Ssh && self.disable_terminal,
            notes: self.notes.unwrap_or_default(),
            // Set below from the folder: the agent never picks its own access.
            agent_access: None,
        }
    }

    /// What the confirmation dialog shows: everything but the secrets'
    /// values.
    pub(super) fn describe(&self, folder: &str) -> String {
        let secret = |present: bool| if present { "provided" } else { "none" };
        format!(
            "{} {}@{}:{}\nfolder: {folder}\nname: {}\nauth: {:?}, password: {}, private key: {}{}",
            format!("{:?}", self.protocol()).to_uppercase(),
            self.username.trim(),
            self.host.trim(),
            self.port.map_or("default".into(), |port| port.to_string()),
            non_empty(self.name.clone()).unwrap_or_else(|| self.host.trim().to_string()),
            self.auth_method(),
            secret(self.password.is_some()),
            secret(self.private_key.is_some() || self.key_path.is_some()),
            self.jump_host
                .as_deref()
                .map(|jump| format!("\nvia jump host: {jump}"))
                .unwrap_or_default(),
        )
    }
}

/// Same protocol, host, port and user as an existing connection.
pub(super) fn duplicate_of<'a>(
    servers: &'a [ServerInfo],
    candidate: &Connection,
) -> Option<&'a ServerInfo> {
    servers.iter().find(|server| {
        server.protocol == candidate.protocol
            && server.host.eq_ignore_ascii_case(&candidate.host)
            && server.port == candidate.port
            && server.username == candidate.auth.username
    })
}

pub async fn create_connection(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: CreateArgs = parse(arguments)?;
    let host = args.host.trim();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err("Give the server's host name or IP address.".into());
    }
    let policy = ctx.policy().await?;
    let (jump_host, jump_level) = match args.jump_host.as_deref() {
        None => (None, None),
        Some(spec) => {
            let bastion = resolve(&policy.servers, spec)?;
            if bastion.protocol != Protocol::Ssh || !may_use_as_jump_host(bastion.level) {
                return Err(ToolError(format!(
                    "You may not route through `{}`: a jump host must be an SSH server where you can run commands (access \"ask\" or \"full\").",
                    bastion.path()
                )));
            }
            (Some(bastion.id.clone()), Some(bastion.level))
        }
    };
    let folder = args.folder.clone().unwrap_or_default();
    let folder_label = if folder.trim().is_empty() {
        "(top level)".to_string()
    } else {
        folder.clone()
    };
    let level = {
        let state = ctx.state();
        let vault = state.vault.lock().unwrap();
        preview_level(&vault.payload()?.tree, &folder, policy.full_access)?
    };
    let mut detail = args.describe(&folder_label);
    detail.push_str(&format!(
        "\nAI agent access: {} (from the folder)",
        access_label(level)
    ));
    let allow_duplicate = args.allow_duplicate;
    let connection = args.into_connection(jump_host);
    if let Some(existing) = duplicate_of(&policy.servers, &connection).filter(|_| !allow_duplicate)
    {
        let known = if existing.level == AccessLevel::Off {
            "a connection you cannot see".to_string()
        } else {
            format!("`{}` (id {})", existing.path(), existing.id)
        };
        return Err(ToolError(format!(
            "The user already has {known} for {}@{}:{}. Use it, or pass allow_duplicate=true to add another.",
            connection.auth.username, connection.host, connection.port
        )));
    }

    ctx.authorize_create(&policy, &detail, level, jump_level)
        .await?;

    let full_access = policy.full_access;
    let (id, path, created, vault) = ctx
        .state()
        .application
        .run_unlocked_vault_operation(move |manager| {
            manager.with_payload(|payload| {
                let added = add_connection(payload, &folder, connection, full_access, level)?;
                Ok((added.id, added.path, added.created, payload.to_public()))
            })
        })
        .await?;

    // Folders created on the way are the user's news, not the agent's: an
    // existing folder the agent cannot see stays unconfirmed to it.
    let summary = if created.is_empty() {
        format!("🤖 AI agent added the connection {path}")
    } else {
        format!(
            "🤖 AI agent added the connection {path} (new folder: {})",
            created.join(", ")
        )
    };
    emit(ctx.app, AgentVaultChangedEvent { vault, summary });
    Ok(format!(
        "Added `{path}` (id {id}) with access \"{}\". Work with it by passing server=\"{id}\" to the other tools; on the first connection the user confirms the server's host key in Serverus.",
        access_label(level)
    ))
}
