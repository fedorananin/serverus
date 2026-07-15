use serde::Deserialize;
use specta::Type;

use super::{
    AuthConfig, AuthMethod, Badge, Connection, FtpOptions, Protocol, S3Options, TunnelConfig,
};

/// Input for creating/updating a connection from the UI. Secret fields are
/// "tri-state": `None` = keep the stored value, `Some("")` = clear,
/// `Some(v)` = replace.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct ConnectionInput {
    pub name: String,
    pub badge: Option<Badge>,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub auth_method: AuthMethod,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub key_inline: Option<String>,
    pub key_passphrase: Option<String>,
    pub jump_host: Option<String>,
    pub ftp: Option<FtpOptions>,
    pub s3: Option<S3Options>,
    pub remote_dir: Option<String>,
    pub local_dir: Option<String>,
    pub tunnels: Vec<TunnelConfig>,
    #[serde(default)]
    pub disable_terminal: bool,
    pub notes: String,
}

fn merge_secret(new: Option<String>, old: Option<String>) -> Option<String> {
    match new {
        None => old,
        Some(secret) if secret.is_empty() => None,
        Some(secret) => Some(secret),
    }
}

impl ConnectionInput {
    /// Merge this input over an existing connection (or a blank one).
    pub fn into_connection(self, existing: Option<&Connection>) -> Connection {
        let old = existing.map(|connection| connection.auth.clone());
        Connection {
            name: self.name,
            badge: self.badge,
            protocol: self.protocol,
            host: self.host,
            port: self.port,
            auth: AuthConfig {
                method: self.auth_method,
                username: self.username,
                password: merge_secret(
                    self.password,
                    old.as_ref().and_then(|auth| auth.password.clone()),
                ),
                key_path: self.key_path,
                key_inline: merge_secret(
                    self.key_inline,
                    old.as_ref().and_then(|auth| auth.key_inline.clone()),
                ),
                key_passphrase: merge_secret(
                    self.key_passphrase,
                    old.as_ref().and_then(|auth| auth.key_passphrase.clone()),
                ),
            },
            jump_host: self.jump_host,
            ftp: self.ftp,
            s3: self.s3,
            remote_dir: self.remote_dir,
            local_dir: self.local_dir,
            tunnels: self.tunnels,
            disable_terminal: self.disable_terminal,
            notes: self.notes,
        }
    }
}
