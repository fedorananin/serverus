//! SSH connection establishment: host key verification, auth (agent → key →
//! password), jump-host chains (SPEC §4.1).

use std::sync::Arc;
use std::time::Duration;

use russh::client::{AuthResult, Config, Handle, Handler};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::{load_secret_key, ssh_key};
use tokio::sync::Mutex as AsyncMutex;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::vault::model::{AuthMethod, Connection};

/// Outcome of a host-key check that stops the connection: the UI shows a
/// prompt and, on acceptance, stores the key in the vault and reconnects.
#[derive(Debug, Clone)]
pub struct HostKeyIssue {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    /// SHA-256 fingerprint, OpenSSH textual form.
    pub fingerprint: String,
    /// `algo base64` line as stored in the vault's known_hosts.
    pub key_line: String,
    /// True when a *different* key was stored before (possible MITM — the UI
    /// must show the scary red variant, SPEC §4.1).
    pub changed: bool,
}

pub enum ConnectOutcome {
    Connected(SshTransportChain),
    HostKeyPrompt(Box<HostKeyIssue>),
}

/// Every SSH transport in a jump-host chain, ordered root to target.
pub struct SshTransportChain {
    handles: Vec<Handle<ClientHandler>>,
}

impl SshTransportChain {
    fn leaf(&self) -> AppResult<&Handle<ClientHandler>> {
        self.handles
            .last()
            .ok_or_else(|| AppError::Connect("SSH transport is closed".into()))
    }

    pub async fn channel_open_session(
        &self,
    ) -> Result<russh::Channel<russh::client::Msg>, russh::Error> {
        self.leaf()
            .map_err(|_| russh::Error::SendError)?
            .channel_open_session()
            .await
    }

    pub fn is_closed(&self) -> bool {
        self.handles.last().is_none_or(Handle::is_closed)
    }

    /// Stop and join every transport from the target back to the root jump
    /// host. Awaiting the Handle futures is the protocol teardown barrier.
    pub async fn disconnect_and_wait(mut self) -> AppResult<()> {
        while let Some(mut handle) = self.handles.pop() {
            // A send failure means the transport receiver is already closed;
            // the Handle future below is still the authoritative join barrier.
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
            // Any result means the owned transport task was joined and its
            // stream was shut down. Teardown must continue through every root
            // even when the peer had already closed or a protocol error won.
            let _ = (&mut handle).await;
        }
        Ok(())
    }
}

/// russh event handler: verifies the server key against the vault-stored
/// known_hosts entry for this host:port.
pub struct ClientHandler {
    /// `algo base64` accepted earlier, if any.
    expected: Option<String>,
    /// Set by check_server_key so the caller can build a HostKeyIssue.
    seen: Arc<std::sync::Mutex<Option<ssh_key::PublicKey>>>,
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        *self.seen.lock().unwrap() = Some(server_public_key.clone());
        let line = key_line(server_public_key);
        Ok(self.expected.as_deref() == Some(line.as_str()))
    }
}

pub fn key_line(key: &ssh_key::PublicKey) -> String {
    format!(
        "{} {}",
        key.algorithm(),
        key.to_openssh()
            .map(|s| s.split_whitespace().nth(1).unwrap_or_default().to_string())
            .unwrap_or_default()
    )
}

fn client_config() -> Arc<Config> {
    Arc::new(Config {
        // SPEC §4.1: server-alive 30 s.
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        ..Config::default()
    })
}

/// One hop of an SSH chain: everything needed to dial and authenticate.
pub struct Hop {
    pub host: String,
    pub port: u16,
    pub auth: HopAuth,
    pub known_host_line: Option<String>,
}

pub struct HopAuth {
    pub username: String,
    pub method: AuthMethod,
    pub password: Option<Zeroizing<String>>,
    pub key_path: Option<String>,
    pub key_inline: Option<Zeroizing<String>>,
    pub key_passphrase: Option<Zeroizing<String>>,
}

impl Hop {
    pub fn from_connection(conn: &Connection, known_host_line: Option<String>) -> Self {
        Hop {
            host: conn.host.clone(),
            port: conn.port,
            auth: HopAuth {
                username: conn.auth.username.clone(),
                method: conn.auth.method,
                password: conn.auth.password.clone().map(Zeroizing::new),
                key_path: conn.auth.key_path.clone(),
                key_inline: conn.auth.key_inline.clone().map(Zeroizing::new),
                key_passphrase: conn.auth.key_passphrase.clone().map(Zeroizing::new),
            },
            known_host_line,
        }
    }
}

/// Connect through a chain of hops: `chain[0]` is dialed directly, each
/// following hop is reached through a direct-tcpip channel of the previous
/// one (SPEC §4.1 jump hosts). Returns the handle of the *last* hop.
pub async fn connect_chain(chain: &[Hop]) -> AppResult<ConnectOutcome> {
    connect_chain_with_progress(chain, &|_| {}).await
}

/// Like [`connect_chain`], reporting human-readable stage messages
/// ("Connecting to…", "Authenticating…") so the UI can show what a slow
/// connect is actually doing instead of a frozen screen.
pub async fn connect_chain_with_progress(
    chain: &[Hop],
    progress: &(dyn Fn(String) + Send + Sync),
) -> AppResult<ConnectOutcome> {
    assert!(!chain.is_empty());
    let mut handles: Vec<Handle<ClientHandler>> = Vec::with_capacity(chain.len());

    for hop in chain {
        progress(if handles.is_empty() {
            format!("Connecting to {}:{}…", hop.host, hop.port)
        } else {
            format!(
                "Connecting to {}:{} through the bastion…",
                hop.host, hop.port
            )
        });
        let seen = Arc::new(std::sync::Mutex::new(None));
        let handler = ClientHandler {
            expected: hop.known_host_line.clone(),
            seen: seen.clone(),
        };

        let connected = match handles.last() {
            None => {
                russh::client::connect(client_config(), (hop.host.as_str(), hop.port), handler)
                    .await
            }
            Some(bastion) => {
                let channel = bastion
                    .channel_open_direct_tcpip(hop.host.clone(), hop.port as u32, "127.0.0.1", 0)
                    .await;
                let channel = match channel {
                    Ok(channel) => channel,
                    Err(error) => {
                        let _ = SshTransportChain { handles }.disconnect_and_wait().await;
                        return Err(AppError::Connect(format!("jump channel: {error}")));
                    }
                };
                russh::client::connect_stream(client_config(), channel.into_stream(), handler).await
            }
        };

        let mut handle = match connected {
            Ok(handle) => handle,
            Err(e) => {
                // An unknown/changed host key surfaces as UnknownKey; build
                // the interactive prompt payload from the recorded key.
                let seen_key = { seen.lock().unwrap().take() };
                if let Some(seen_key) = seen_key {
                    if matches!(e, russh::Error::UnknownKey) {
                        let issue = HostKeyIssue {
                            host: hop.host.clone(),
                            port: hop.port,
                            algorithm: seen_key.algorithm().to_string(),
                            fingerprint: seen_key.fingerprint(ssh_key::HashAlg::Sha256).to_string(),
                            key_line: key_line(&seen_key),
                            changed: hop.known_host_line.is_some(),
                        };
                        let _ = SshTransportChain { handles }.disconnect_and_wait().await;
                        return Ok(ConnectOutcome::HostKeyPrompt(Box::new(issue)));
                    }
                }
                let _ = SshTransportChain { handles }.disconnect_and_wait().await;
                return Err(AppError::Connect(format!("{}:{}: {e}", hop.host, hop.port)));
            }
        };

        progress(format!(
            "Authenticating as {}@{}…",
            hop.auth.username, hop.host
        ));
        if let Err(error) = authenticate(&mut handle, &hop.auth).await {
            handles.push(handle);
            let _ = SshTransportChain { handles }.disconnect_and_wait().await;
            return Err(error);
        }
        handles.push(handle);
    }

    Ok(ConnectOutcome::Connected(SshTransportChain { handles }))
}

/// Try the configured methods in the fixed order agent → key → password
/// (SPEC §4.1), collecting failures for a useful error message.
async fn authenticate(handle: &mut Handle<ClientHandler>, auth: &HopAuth) -> AppResult<()> {
    let mut attempts: Vec<String> = Vec::new();

    if auth.method == AuthMethod::Agent {
        match try_agent(handle, &auth.username).await {
            Ok(true) => return Ok(()),
            Ok(false) => attempts.push("agent: no identity accepted".into()),
            Err(e) => attempts.push(format!("agent: {e}")),
        }
    }

    if auth.method == AuthMethod::Key {
        match try_key(handle, auth).await {
            Ok(true) => return Ok(()),
            Ok(false) => attempts.push("key: rejected by server".into()),
            Err(e) => attempts.push(format!("key: {e}")),
        }
    }

    if auth.method == AuthMethod::Password || auth.method == AuthMethod::Key {
        // Key configs may carry a fallback password; only try when present.
        if let Some(password) = &auth.password {
            let result = handle
                .authenticate_password(auth.username.clone(), password.as_str())
                .await
                .map_err(|e| AppError::Auth(format!("password auth: {e}")))?;
            if matches!(result, AuthResult::Success) {
                return Ok(());
            }
            attempts.push("password: rejected by server".into());
        } else if auth.method == AuthMethod::Password {
            attempts.push("password: not set".into());
        }
    }

    Err(AppError::Auth(attempts.join("; ")))
}

/// Try the local ssh-agent. Only where the agent lives is OS-specific:
/// `$SSH_AUTH_SOCK` (unix socket) vs the OpenSSH-for-Windows named pipe,
/// with PuTTY's Pageant as the Windows fallback.
async fn try_agent(handle: &mut Handle<ClientHandler>, username: &str) -> AppResult<bool> {
    #[cfg(unix)]
    {
        let mut agent = russh::keys::agent::client::AgentClient::connect_env()
            .await
            .map_err(|e| AppError::Auth(format!("SSH_AUTH_SOCK: {e}")))?;
        agent_auth(handle, username, &mut agent).await
    }
    #[cfg(windows)]
    {
        const OPENSSH_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";
        match russh::keys::agent::client::AgentClient::connect_named_pipe(OPENSSH_PIPE).await {
            Ok(mut agent) => agent_auth(handle, username, &mut agent).await,
            Err(pipe_err) => {
                match russh::keys::agent::client::AgentClient::connect_pageant().await {
                    Ok(mut agent) => agent_auth(handle, username, &mut agent).await,
                    Err(_) => Err(AppError::Auth(format!("ssh-agent: {pipe_err}"))),
                }
            }
        }
    }
}

/// Offer every identity the agent holds until one is accepted.
async fn agent_auth<S>(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    agent: &mut russh::keys::agent::client::AgentClient<S>,
) -> AppResult<bool>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let rsa_hash = handle
        .best_supported_rsa_hash()
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .flatten();
    for identity in identities {
        let key = identity.public_key().into_owned();
        let result = handle
            .authenticate_publickey_with(username, key, rsa_hash, agent)
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?;
        if matches!(result, AuthResult::Success) {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn try_key(handle: &mut Handle<ClientHandler>, auth: &HopAuth) -> AppResult<bool> {
    let passphrase = auth.key_passphrase.as_ref().map(|p| p.as_str());
    let key = if let Some(inline) = &auth.key_inline {
        russh::keys::decode_secret_key(inline, passphrase)
            .map_err(|e| AppError::Auth(format!("inline key: {e}")))?
    } else if let Some(path) = &auth.key_path {
        let expanded = shellexpand_home(path);
        load_secret_key(&expanded, passphrase)
            .map_err(|e| AppError::Auth(format!("{expanded}: {e}")))?
    } else {
        return Err(AppError::Auth("no key path configured".into()));
    };

    let rsa_hash = handle
        .best_supported_rsa_hash()
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
        .flatten();
    let result = handle
        .authenticate_publickey(
            auth.username.clone(),
            PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
        )
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?;
    Ok(matches!(result, AuthResult::Success))
}

/// Expand a leading `~` — key paths are stored user-friendly.
pub fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

/// A live SSH session shared by terminal channels, SFTP and tunnels.
pub struct SshSession {
    transport: AsyncMutex<Option<SshTransportChain>>,
}

impl SshSession {
    pub fn new(transport: SshTransportChain) -> Self {
        Self {
            transport: AsyncMutex::new(Some(transport)),
        }
    }

    #[cfg(test)]
    pub(crate) fn disconnected_for_test() -> Self {
        Self {
            transport: AsyncMutex::new(None),
        }
    }

    pub async fn is_closed(&self) -> bool {
        self.transport
            .lock()
            .await
            .as_ref()
            .is_none_or(SshTransportChain::is_closed)
    }

    pub async fn channel_open_session(
        &self,
    ) -> Result<russh::Channel<russh::client::Msg>, russh::Error> {
        let transport = self.transport.lock().await;
        let transport = transport.as_ref().ok_or(russh::Error::SendError)?;
        transport.channel_open_session().await
    }

    pub async fn channel_open_direct_tcpip(
        &self,
        host: String,
        port: u32,
        originator_host: String,
        originator_port: u32,
    ) -> Result<russh::Channel<russh::client::Msg>, russh::Error> {
        let transport = self.transport.lock().await;
        let leaf = transport
            .as_ref()
            .ok_or(russh::Error::SendError)?
            .leaf()
            .map_err(|_| russh::Error::SendError)?;
        leaf.channel_open_direct_tcpip(host, port, originator_host, originator_port)
            .await
    }

    pub async fn disconnect_and_wait(&self) -> AppResult<()> {
        let transport = self.transport.lock().await.take();
        match transport {
            Some(transport) => transport.disconnect_and_wait().await,
            None => Ok(()),
        }
    }

    /// Run a command and report whether it exited 0 (used e.g. for the
    /// `command -v tar` capability probe, SPEC §6.2).
    pub async fn exec_check(&self, cmd: &str) -> AppResult<bool> {
        let channel = {
            self.channel_open_session()
                .await
                .map_err(|e| AppError::Connect(format!("exec channel: {e}")))?
        };
        channel
            .exec(true, cmd)
            .await
            .map_err(|e| AppError::Connect(format!("exec: {e}")))?;
        let (mut read, write) = channel.split();
        let mut status = None;
        while let Some(msg) = read.wait().await {
            match msg {
                russh::ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                russh::ChannelMsg::Close => break,
                _ => {}
            }
        }
        let _ = write.close().await;
        Ok(status == Some(0))
    }
}
