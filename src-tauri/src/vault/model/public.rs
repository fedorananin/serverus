use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    AuthMethod, Badge, Connection, FtpOptions, Protocol, S3Options, Settings, TreeNode,
    TunnelConfig, VaultPayload,
};

/// Auth config with secrets replaced by presence flags.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PublicAuth {
    pub method: AuthMethod,
    pub username: String,
    pub key_path: Option<String>,
    pub has_password: bool,
    pub has_key_inline: bool,
    pub has_key_passphrase: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PublicConnection {
    pub id: String,
    pub name: String,
    pub badge: Option<Badge>,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub auth: PublicAuth,
    pub jump_host: Option<String>,
    pub ftp: Option<FtpOptions>,
    pub s3: Option<S3Options>,
    pub remote_dir: Option<String>,
    pub local_dir: Option<String>,
    pub tunnels: Vec<TunnelConfig>,
    pub disable_terminal: bool,
    pub notes: String,
}

/// The whole vault as the UI sees it — no secrets.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PublicVault {
    pub tree: Vec<TreeNode>,
    pub connections: HashMap<String, PublicConnection>,
    pub known_hosts: HashMap<String, String>,
    pub settings: Settings,
}

impl Connection {
    pub fn to_public(&self, id: &str) -> PublicConnection {
        PublicConnection {
            id: id.to_string(),
            name: self.name.clone(),
            badge: self.badge.clone(),
            protocol: self.protocol,
            host: self.host.clone(),
            port: self.port,
            auth: PublicAuth {
                method: self.auth.method,
                username: self.auth.username.clone(),
                key_path: self.auth.key_path.clone(),
                has_password: self.auth.password.is_some(),
                has_key_inline: self.auth.key_inline.is_some(),
                has_key_passphrase: self.auth.key_passphrase.is_some(),
            },
            jump_host: self.jump_host.clone(),
            ftp: self.ftp.clone(),
            s3: self.s3.clone(),
            remote_dir: self.remote_dir.clone(),
            local_dir: self.local_dir.clone(),
            tunnels: self.tunnels.clone(),
            disable_terminal: self.disable_terminal,
            notes: self.notes.clone(),
        }
    }
}

impl VaultPayload {
    pub fn to_public(&self) -> PublicVault {
        PublicVault {
            tree: self.tree.clone(),
            connections: self
                .connections
                .iter()
                .map(|(id, connection)| (id.clone(), connection.to_public(id)))
                .collect(),
            known_hosts: self.known_hosts.clone(),
            settings: self.settings.clone(),
        }
    }
}
