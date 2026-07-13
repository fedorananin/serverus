//! Session manager: registry of live connections and terminal channels
//! (SPEC §7.1). One SSH session multiplexes terminals, SFTP and tunnels.

pub mod ftp;
pub mod remote_fs;
pub mod s3;
pub mod sftp;
pub mod ssh;
pub mod tunnel;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use russh::{ChannelMsg, ChannelWriteHalf};
use tauri::AppHandle;
use tauri_specta::Event;

use crate::error::{AppError, AppResult};
use crate::events::{SessionStateEvent, TerminalDataEvent, TerminalExitEvent};
use crate::vault::model::Protocol;
use crate::vault::VaultManager;
use ssh::{ConnectOutcome, Hop, HostKeyIssue, SshSession};

pub struct SessionEntry {
    pub id: String,
    pub connection_id: String,
    pub protocol: Protocol,
    /// Present for SSH sessions (terminals, SFTP and tunnels hang off it).
    pub ssh: Option<Arc<SshSession>>,
    /// Lazily opened SFTP subsystem over the same SSH session.
    sftp: tokio::sync::OnceCell<Arc<sftp::SftpFs>>,
    /// Connection pool for FTP sessions.
    pub ftp: Option<Arc<ftp::FtpPool>>,
    /// S3 client for object-storage sessions (SPEC §4.4).
    pub s3: Option<Arc<s3::S3Fs>>,
    /// Whether the remote side has `tar` (probed once, SPEC §6.2).
    tar_available: tokio::sync::OnceCell<bool>,
}

impl SessionEntry {
    /// SSH handle + tar availability for accelerated dir transfers.
    pub async fn tar_ssh(&self) -> Option<Arc<ssh::SshSession>> {
        let ssh = self.ssh.clone()?;
        let available = self
            .tar_available
            .get_or_init(|| {
                let ssh = ssh.clone();
                async move {
                    ssh.exec_check("command -v tar >/dev/null 2>&1")
                        .await
                        .unwrap_or(false)
                }
            })
            .await;
        if *available {
            Some(ssh)
        } else {
            None
        }
    }

    /// The protocol-agnostic file backend for this session (SPEC §7.1).
    pub async fn remote_fs(&self) -> AppResult<Arc<dyn remote_fs::RemoteFs>> {
        match self.protocol {
            Protocol::Ssh => {
                let ssh = self
                    .ssh
                    .clone()
                    .ok_or_else(|| AppError::Other("missing ssh handle".into()))?;
                let fs = self
                    .sftp
                    .get_or_try_init(|| async move { sftp::SftpFs::open(&ssh).await.map(Arc::new) })
                    .await?;
                Ok(fs.clone())
            }
            Protocol::Ftp => self
                .ftp
                .clone()
                .map(|pool| pool as Arc<dyn remote_fs::RemoteFs>)
                .ok_or_else(|| AppError::Other("missing ftp pool".into())),
            Protocol::S3 => self
                .s3
                .clone()
                .map(|fs| fs as Arc<dyn remote_fs::RemoteFs>)
                .ok_or_else(|| AppError::Other("missing s3 client".into())),
        }
    }
}

struct TerminalEntry {
    session_id: String,
    write: ChannelWriteHalf<russh::client::Msg>,
}

#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Arc<SessionEntry>>>,
    terminals: tokio::sync::Mutex<HashMap<String, TerminalEntry>>,
    pub tunnels: tunnel::TunnelManager,
}

impl SessionManager {
    pub fn get(&self, session_id: &str) -> AppResult<Arc<SessionEntry>> {
        self.sessions
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .ok_or(AppError::SessionNotFound)
    }

    pub fn ssh_of(&self, session_id: &str) -> AppResult<Arc<SshSession>> {
        self.get(session_id)?
            .ssh
            .clone()
            .ok_or_else(|| AppError::Other("not an SSH session".into()))
    }

    /// Build the jump chain (target last) from vault data, with cycle guard.
    fn build_chain(vault: &VaultManager, connection_id: &str) -> AppResult<Vec<Hop>> {
        let payload = vault.payload()?;
        let mut chain: Vec<Hop> = Vec::new();
        let mut cursor = Some(connection_id.to_string());
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = cursor {
            if !seen.insert(id.clone()) {
                return Err(AppError::Connect("jump host cycle detected".into()));
            }
            if chain.len() >= 6 {
                return Err(AppError::Connect("jump chain too long".into()));
            }
            let conn = payload
                .connections
                .get(&id)
                .ok_or(AppError::ConnectionNotFound)?;
            if conn.protocol != Protocol::Ssh {
                return Err(AppError::Connect(
                    "jump hosts and terminals require SSH connections".into(),
                ));
            }
            let known = payload
                .known_hosts
                .get(&format!("{}:{}", conn.host, conn.port))
                .cloned();
            chain.push(Hop::from_connection(conn, known));
            cursor = conn.jump_host.clone();
        }
        chain.reverse(); // bastion first, target last
        Ok(chain)
    }

    /// Connect a session for `connection_id`, dispatching on protocol.
    pub async fn connect(
        self: &Arc<Self>,
        app: &AppHandle,
        vault: &Arc<Mutex<VaultManager>>,
        connection_id: &str,
    ) -> AppResult<Result<Arc<SessionEntry>, Box<HostKeyIssue>>> {
        let (protocol, ftp_config, s3_config, max_parallel) = {
            let mgr = vault.lock().unwrap();
            let payload = mgr.payload()?;
            let conn = payload
                .connections
                .get(connection_id)
                .ok_or(AppError::ConnectionNotFound)?;
            let ftp_config = if conn.protocol == Protocol::Ftp {
                Some(ftp::FtpConfig::from_connection(conn)?)
            } else {
                None
            };
            let s3_config = if conn.protocol == Protocol::S3 {
                Some(s3::S3Config::from_connection(conn)?)
            } else {
                None
            };
            (
                conn.protocol,
                ftp_config,
                s3_config,
                payload.settings.transfers.max_parallel_per_server as usize,
            )
        };
        match protocol {
            Protocol::Ssh => self.connect_ssh(app, vault, connection_id).await,
            // FTP and S3 share the probe-then-register shape.
            Protocol::Ftp | Protocol::S3 => {
                let session_id = uuid::Uuid::new_v4().to_string();
                let _ = SessionStateEvent {
                    session_id: session_id.clone(),
                    connection_id: connection_id.to_string(),
                    state: "connecting".into(),
                    message: None,
                }
                .emit(app);
                // FTP pool sized for parallel transfers + one metadata slot.
                let ftp = ftp_config.map(|c| ftp::FtpPool::new(c, max_parallel + 1));
                let s3 = s3_config.map(s3::S3Fs::new);
                let probe = match (&ftp, &s3) {
                    (Some(pool), _) => pool.probe().await,
                    (_, Some(fs)) => fs.probe().await,
                    _ => unreachable!(),
                };
                match probe {
                    Ok(()) => {
                        let entry = Arc::new(SessionEntry {
                            id: session_id.clone(),
                            connection_id: connection_id.to_string(),
                            protocol,
                            ssh: None,
                            sftp: tokio::sync::OnceCell::new(),
                            ftp,
                            s3,
                            tar_available: tokio::sync::OnceCell::new(),
                        });
                        self.sessions
                            .lock()
                            .unwrap()
                            .insert(session_id.clone(), entry.clone());
                        let _ = SessionStateEvent {
                            session_id,
                            connection_id: connection_id.to_string(),
                            state: "connected".into(),
                            message: None,
                        }
                        .emit(app);
                        Ok(Ok(entry))
                    }
                    Err(e) => {
                        let _ = SessionStateEvent {
                            session_id,
                            connection_id: connection_id.to_string(),
                            state: "error".into(),
                            message: Some(e.to_string()),
                        }
                        .emit(app);
                        Err(e)
                    }
                }
            }
        }
    }

    /// Connect an SSH session for `connection_id`. On an unknown/changed host
    /// key the caller receives the prompt payload instead of a session.
    async fn connect_ssh(
        self: &Arc<Self>,
        app: &AppHandle,
        vault: &Arc<Mutex<VaultManager>>,
        connection_id: &str,
    ) -> AppResult<Result<Arc<SessionEntry>, Box<HostKeyIssue>>> {
        let chain = {
            let mgr = vault.lock().unwrap();
            Self::build_chain(&mgr, connection_id)?
        };

        let session_id = uuid::Uuid::new_v4().to_string();
        let _ = SessionStateEvent {
            session_id: session_id.clone(),
            connection_id: connection_id.to_string(),
            state: "connecting".into(),
            message: None,
        }
        .emit(app);

        // Stage messages ("Connecting…", "Authenticating…") stream to the UI
        // as `connecting` events so a slow connect doesn't look frozen.
        let progress = {
            let app = app.clone();
            let session_id = session_id.clone();
            let connection_id = connection_id.to_string();
            move |message: String| {
                let _ = SessionStateEvent {
                    session_id: session_id.clone(),
                    connection_id: connection_id.clone(),
                    state: "connecting".into(),
                    message: Some(message),
                }
                .emit(&app);
            }
        };
        match ssh::connect_chain_with_progress(&chain, &progress).await {
            Ok(ConnectOutcome::Connected(handle)) => {
                let entry = Arc::new(SessionEntry {
                    id: session_id.clone(),
                    connection_id: connection_id.to_string(),
                    protocol: Protocol::Ssh,
                    ssh: Some(Arc::new(SshSession {
                        handle: tokio::sync::Mutex::new(handle),
                    })),
                    sftp: tokio::sync::OnceCell::new(),
                    ftp: None,
                    s3: None,
                    tar_available: tokio::sync::OnceCell::new(),
                });
                self.sessions
                    .lock()
                    .unwrap()
                    .insert(session_id.clone(), entry.clone());
                let _ = SessionStateEvent {
                    session_id: session_id.clone(),
                    connection_id: connection_id.to_string(),
                    state: "connected".into(),
                    message: None,
                }
                .emit(app);
                // Disconnect watchdog: keep-alives are handled by russh; when
                // the connection dies the UI learns it and reconnects
                // (SPEC §4.1 auto-reconnect).
                {
                    let manager = self.clone();
                    let ssh = entry.ssh.clone().unwrap();
                    let app = app.clone();
                    let connection_id = connection_id.to_string();
                    tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            let closed = ssh.handle.lock().await.is_closed();
                            if closed {
                                if manager
                                    .sessions
                                    .lock()
                                    .unwrap()
                                    .remove(&session_id)
                                    .is_some()
                                {
                                    manager.tunnels.stop_session(&session_id);
                                    let _ = SessionStateEvent {
                                        session_id: session_id.clone(),
                                        connection_id,
                                        state: "disconnected".into(),
                                        message: None,
                                    }
                                    .emit(&app);
                                }
                                break;
                            }
                        }
                    });
                }
                Ok(Ok(entry))
            }
            Ok(ConnectOutcome::HostKeyPrompt(issue)) => Ok(Err(issue)),
            Err(e) => {
                let _ = SessionStateEvent {
                    session_id,
                    connection_id: connection_id.to_string(),
                    state: "error".into(),
                    message: Some(e.to_string()),
                }
                .emit(app);
                Err(e)
            }
        }
    }

    pub async fn disconnect(&self, session_id: &str) {
        self.tunnels.stop_session(session_id);
        let entry = self.sessions.lock().unwrap().remove(session_id);
        // Close terminals belonging to this session.
        let mut terminals = self.terminals.lock().await;
        let ids: Vec<String> = terminals
            .iter()
            .filter(|(_, t)| t.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(term) = terminals.remove(&id) {
                let _ = term.write.close().await;
            }
        }
        drop(terminals);
        if let Some(entry) = entry {
            if let Some(ssh) = &entry.ssh {
                let handle = ssh.handle.lock().await;
                let _ = handle
                    .disconnect(russh::Disconnect::ByApplication, "", "en")
                    .await;
            }
        }
    }

    // -- Terminals (SPEC §5.5) --

    pub async fn term_open(
        &self,
        app: AppHandle,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> AppResult<String> {
        let ssh = self.ssh_of(session_id)?;
        let channel = {
            let handle = ssh.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|e| AppError::Connect(format!("terminal channel: {e}")))?
        };
        channel
            .request_pty(true, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
            .await
            .map_err(|e| AppError::Connect(format!("pty: {e}")))?;
        channel
            .request_shell(true)
            .await
            .map_err(|e| AppError::Connect(format!("shell: {e}")))?;

        let (mut read, write) = channel.split();
        let term_id = uuid::Uuid::new_v4().to_string();
        self.terminals.lock().await.insert(
            term_id.clone(),
            TerminalEntry {
                session_id: session_id.to_string(),
                write,
            },
        );

        // Reader task with ~16 ms batching so floods don't hang the UI
        // (SPEC §5.5).
        let id_for_task = term_id.clone();
        tokio::spawn(async move {
            let b64 = base64::engine::general_purpose::STANDARD;
            let mut buf: Vec<u8> = Vec::new();
            let mut ticker = tokio::time::interval(Duration::from_millis(16));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let flush = |buf: &mut Vec<u8>| {
                if !buf.is_empty() {
                    let _ = TerminalDataEvent {
                        term_id: id_for_task.clone(),
                        data: b64.encode(&buf),
                    }
                    .emit(&app);
                    buf.clear();
                }
            };
            loop {
                tokio::select! {
                    msg = read.wait() => match msg {
                        Some(ChannelMsg::Data { data }) => {
                            buf.extend_from_slice(&data);
                            if buf.len() > 256 * 1024 {
                                flush(&mut buf);
                            }
                        }
                        Some(ChannelMsg::ExtendedData { data, .. }) => {
                            buf.extend_from_slice(&data);
                        }
                        Some(ChannelMsg::ExitStatus { .. })
                        | Some(ChannelMsg::Eof)
                        | Some(ChannelMsg::Close)
                        | None => {
                            flush(&mut buf);
                            let _ = TerminalExitEvent {
                                term_id: id_for_task.clone(),
                            }
                            .emit(&app);
                            break;
                        }
                        Some(_) => {}
                    },
                    _ = ticker.tick() => flush(&mut buf),
                }
            }
        });

        Ok(term_id)
    }

    pub async fn term_write(&self, term_id: &str, data: &[u8]) -> AppResult<()> {
        let terminals = self.terminals.lock().await;
        let term = terminals.get(term_id).ok_or(AppError::SessionNotFound)?;
        term.write
            .data(data)
            .await
            .map_err(|e| AppError::Other(format!("terminal write: {e}")))
    }

    pub async fn term_resize(&self, term_id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let terminals = self.terminals.lock().await;
        let term = terminals.get(term_id).ok_or(AppError::SessionNotFound)?;
        term.write
            .window_change(cols as u32, rows as u32, 0, 0)
            .await
            .map_err(|e| AppError::Other(format!("terminal resize: {e}")))
    }

    pub async fn term_close(&self, term_id: &str) {
        if let Some(term) = self.terminals.lock().await.remove(term_id) {
            let _ = term.write.close().await;
        }
    }
}
