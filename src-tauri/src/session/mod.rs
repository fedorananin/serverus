//! Session manager: registry of live connections and terminal channels
//! (SPEC §7.1). One SSH session multiplexes terminals, SFTP and tunnels.

pub mod ftp;
pub(crate) mod lifecycle;
pub mod remote_fs;
pub mod s3;
pub mod sftp;
pub mod ssh;
pub mod tunnel;

#[cfg(test)]
mod lifecycle_tests;

use std::collections::HashMap;
use std::ops::Deref;
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

pub(crate) use lifecycle::{LifecycleCleanup, SessionOperation};

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
    lifecycle: Arc<lifecycle::SessionLifecycle>,
    watchdog: Mutex<Option<tokio::task::JoinHandle<()>>>,
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
    write: Option<ChannelWriteHalf<russh::client::Msg>>,
    reader_task: Option<tokio::task::JoinHandle<()>>,
    cleanup: LifecycleCleanup,
}

impl Drop for TerminalEntry {
    fn drop(&mut self) {
        let Some(reader_task) = self.reader_task.take() else {
            return;
        };
        reader_task.abort();
        let wait = async move {
            let _ = reader_task.await;
        };
        if let Err(wait) = self.cleanup.try_spawn(wait) {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(wait);
            }
        }
    }
}

pub struct SessionLease {
    entry: Arc<SessionEntry>,
    operation: SessionOperation,
}

impl SessionLease {
    pub(crate) fn operation(&self) -> &SessionOperation {
        &self.operation
    }
}

impl Deref for SessionLease {
    type Target = SessionEntry;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

pub struct ClosingSession {
    entry: Arc<SessionEntry>,
}

#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Arc<SessionEntry>>>,
    terminals: tokio::sync::Mutex<HashMap<String, TerminalEntry>>,
    pub tunnels: tunnel::TunnelManager,
}

impl SessionManager {
    #[cfg(test)]
    pub(crate) fn insert_test_session(&self, session_id: &str) -> Arc<lifecycle::LifecycleGate> {
        let lifecycle = Arc::new(lifecycle::LifecycleGate::default());
        self.sessions.lock().unwrap().insert(
            session_id.to_string(),
            Arc::new(SessionEntry {
                id: session_id.to_string(),
                connection_id: "connection".into(),
                protocol: Protocol::S3,
                ssh: None,
                sftp: tokio::sync::OnceCell::new(),
                ftp: None,
                s3: None,
                tar_available: tokio::sync::OnceCell::new(),
                lifecycle: lifecycle.clone(),
                watchdog: Mutex::new(None),
            }),
        );
        lifecycle
    }

    pub fn get(&self, session_id: &str) -> AppResult<SessionLease> {
        let entry = self
            .sessions
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .ok_or(AppError::SessionNotFound)?;
        let operation = entry.lifecycle.try_begin_operation()?;
        Ok(SessionLease { entry, operation })
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
                let lifecycle = Arc::new(lifecycle::SessionLifecycle::default());
                let _ = SessionStateEvent {
                    session_id: session_id.clone(),
                    connection_id: connection_id.to_string(),
                    state: "connecting".into(),
                    message: None,
                }
                .emit(app);
                // FTP pool sized for parallel transfers + one metadata slot.
                let ftp = ftp_config.map(|c| ftp::FtpPool::new(c, max_parallel + 1));
                let s3 =
                    s3_config.map(|config| s3::S3Fs::new_in_lifecycle(config, lifecycle.cleanup()));
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
                            lifecycle,
                            watchdog: Mutex::new(None),
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
                    ssh: Some(Arc::new(SshSession::new(handle))),
                    sftp: tokio::sync::OnceCell::new(),
                    ftp: None,
                    s3: None,
                    tar_available: tokio::sync::OnceCell::new(),
                    lifecycle: Arc::new(lifecycle::SessionLifecycle::default()),
                    watchdog: Mutex::new(None),
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

    pub(crate) fn start_disconnect(&self, session_id: &str) -> Option<ClosingSession> {
        let entry = {
            let mut sessions = self.sessions.lock().unwrap();
            let entry = sessions.get(session_id)?.clone();
            if !entry.lifecycle.start_closing() {
                return None;
            }
            sessions.remove(session_id);
            entry
        };
        Some(ClosingSession { entry })
    }

    pub(crate) async fn finish_disconnect(&self, closing: ClosingSession) {
        let ClosingSession { entry } = closing;
        let watchdog = { entry.watchdog.lock().unwrap().take() };
        if let Some(watchdog) = watchdog {
            watchdog.abort();
            let _ = watchdog.await;
        }
        self.tunnels.stop_session(&entry.id).await;
        // Close terminals belonging to this session.
        let mut terminals = self.terminals.lock().await;
        let ids: Vec<String> = terminals
            .iter()
            .filter(|(_, t)| t.session_id == entry.id)
            .map(|(id, _)| id.clone())
            .collect();
        let mut drained = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(term) = terminals.remove(&id) {
                drained.push(term);
            }
        }
        drop(terminals);
        for term in drained {
            close_terminal(term).await;
        }
        let guard = entry.lifecycle.begin_close().await;
        if let Some(ssh) = &entry.ssh {
            let _ = ssh.disconnect_and_wait().await;
        }
        guard.finish().await;
    }

    pub(crate) fn install_watchdog(
        &self,
        session_id: &str,
        watchdog: tokio::task::JoinHandle<()>,
    ) -> AppResult<()> {
        let session = self.get(session_id)?;
        let mut pending = Some(watchdog);
        let registered = {
            let mut slot = session.watchdog.lock().unwrap();
            session.operation().register(|| {
                debug_assert!(slot.is_none());
                *slot = pending.take();
            })
        };
        if let Err(error) = registered {
            let watchdog = pending.take().unwrap();
            watchdog.abort();
            let wait = async move {
                let _ = watchdog.await;
            };
            if let Err(wait) = session.operation().cleanup().try_spawn(wait) {
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    runtime.spawn(wait);
                }
            }
            return Err(error);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn tunnel_start(
        &self,
        session_id: &str,
        name: &str,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> AppResult<tunnel::TunnelStatus> {
        let session = self.get(session_id)?;
        let ssh = session
            .ssh
            .clone()
            .ok_or_else(|| AppError::Other("not an SSH session".into()))?;
        tokio::select! {
            biased;
            _ = session.operation().cancelled() => Err(AppError::SessionNotFound),
            result = self.tunnels.start_guarded(
                ssh,
                session_id,
                name,
                local_port,
                remote_host,
                remote_port,
                session.operation(),
            ) => result,
        }
    }

    pub async fn tunnel_stop(&self, tunnel_id: &str) -> AppResult<()> {
        let Some(session_id) = self.tunnels.session_id(tunnel_id) else {
            return Ok(());
        };
        let session = match self.get(&session_id) {
            Ok(session) => session,
            // Session teardown owns any tunnel that is still registered.
            Err(AppError::SessionNotFound) => return Ok(()),
            Err(error) => return Err(error),
        };
        tokio::select! {
            biased;
            _ = session.operation().cancelled() => Err(AppError::SessionNotFound),
            _ = self.tunnels.stop(tunnel_id) => Ok(()),
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
        let session = self.get(session_id)?;
        let ssh = session
            .ssh
            .clone()
            .ok_or_else(|| AppError::Other("not an SSH session".into()))?;
        let setup = async {
            let channel = {
                ssh.channel_open_session()
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
            Ok::<_, AppError>(channel)
        };
        let channel = tokio::select! {
            biased;
            _ = session.operation().cancelled() => return Err(AppError::SessionNotFound),
            channel = setup => channel?,
        };

        let (mut read, write) = channel.split();
        let term_id = uuid::Uuid::new_v4().to_string();
        // Reader task with ~16 ms batching so floods don't hang the UI
        // (SPEC §5.5).
        let id_for_task = term_id.clone();
        let reader_task = tokio::spawn(async move {
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

        let mut pending = Some(TerminalEntry {
            session_id: session_id.to_string(),
            write: Some(write),
            reader_task: Some(reader_task),
            cleanup: session.operation().cleanup(),
        });
        self.register_terminal(
            session.operation(),
            term_id.clone(),
            pending.take().unwrap(),
        )
        .await?;

        Ok(term_id)
    }

    pub async fn term_write(&self, term_id: &str, data: &[u8]) -> AppResult<()> {
        let session_id = self
            .terminals
            .lock()
            .await
            .get(term_id)
            .map(|terminal| terminal.session_id.clone())
            .ok_or(AppError::SessionNotFound)?;
        let session = self.get(&session_id)?;
        let terminals = self.terminals.lock().await;
        let term = terminals.get(term_id).ok_or(AppError::SessionNotFound)?;
        tokio::select! {
            biased;
            _ = session.operation().cancelled() => Err(AppError::SessionNotFound),
            result = term.write
                .as_ref()
                .expect("registered terminals have a write half")
                .data(data) => {
                result.map_err(|e| AppError::Other(format!("terminal write: {e}")))
            }
        }
    }

    pub async fn term_resize(&self, term_id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let session_id = self
            .terminals
            .lock()
            .await
            .get(term_id)
            .map(|terminal| terminal.session_id.clone())
            .ok_or(AppError::SessionNotFound)?;
        let session = self.get(&session_id)?;
        let terminals = self.terminals.lock().await;
        let term = terminals.get(term_id).ok_or(AppError::SessionNotFound)?;
        tokio::select! {
            biased;
            _ = session.operation().cancelled() => Err(AppError::SessionNotFound),
            result = term.write
                .as_ref()
                .expect("registered terminals have a write half")
                .window_change(cols as u32, rows as u32, 0, 0) => {
                result.map_err(|e| AppError::Other(format!("terminal resize: {e}")))
            }
        }
    }

    pub async fn term_close(&self, term_id: &str) -> AppResult<()> {
        let session_id = self
            .terminals
            .lock()
            .await
            .get(term_id)
            .map(|terminal| terminal.session_id.clone());
        let Some(session_id) = session_id else {
            return Ok(());
        };
        let session = match self.get(&session_id) {
            Ok(session) => session,
            // Session teardown owns any terminal that is still registered.
            Err(AppError::SessionNotFound) => return Ok(()),
            Err(error) => return Err(error),
        };
        let completion = {
            let mut terminals = self.terminals.lock().await;
            terminals.remove(term_id).and_then(schedule_terminal_close)
        };
        if let Some(completion) = completion {
            tokio::select! {
                biased;
                _ = session.operation().cancelled() => return Err(AppError::SessionNotFound),
                _ = completion => {}
            }
        }
        Ok(())
    }

    async fn register_terminal(
        &self,
        operation: &SessionOperation,
        term_id: String,
        terminal: TerminalEntry,
    ) -> AppResult<()> {
        let mut pending = Some(terminal);
        let registered = {
            let mut terminals = self.terminals.lock().await;
            operation.register(|| {
                terminals.insert(term_id, pending.take().unwrap());
            })
        };
        if let Err(error) = registered {
            let terminal = pending.take().unwrap();
            let cleanup = terminal.cleanup.clone();
            let close = async move { close_terminal(terminal).await };
            if let Err(close) = cleanup.try_spawn(close) {
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    runtime.spawn(close);
                }
            }
            return Err(error);
        }
        Ok(())
    }
}

fn schedule_terminal_close(terminal: TerminalEntry) -> Option<tokio::sync::oneshot::Receiver<()>> {
    let cleanup = terminal.cleanup.clone();
    let (completed, completed_rx) = tokio::sync::oneshot::channel();
    let close = async move {
        close_terminal(terminal).await;
        let _ = completed.send(());
    };
    let close = match cleanup.try_spawn(close) {
        Ok(()) => return Some(completed_rx),
        Err(close) => close,
    };
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return None;
    };
    runtime.spawn(close);
    Some(completed_rx)
}

async fn close_terminal(mut terminal: TerminalEntry) {
    if let Some(write) = &terminal.write {
        let _ = tokio::time::timeout(Duration::from_secs(1), write.close()).await;
    }
    if let Some(reader_task) = terminal.reader_task.as_mut() {
        if tokio::time::timeout(Duration::from_secs(1), &mut *reader_task)
            .await
            .is_err()
        {
            reader_task.abort();
            let _ = (&mut *reader_task).await;
        }
    }
    terminal.reader_task.take();
}
