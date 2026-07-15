//! Local port forwarding over the multiplexed SSH session (SPEC §4.2):
//! `localhost:<local_port>` → SSH → `<remote_host>:<remote_port>`, with live
//! traffic counters.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use specta::Type;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::error::{AppError, AppResult};
use crate::session::ssh::SshSession;

pub struct ActiveTunnel {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    bytes_up: Arc<AtomicU64>,
    bytes_down: Arc<AtomicU64>,
    connections: Arc<AtomicU32>,
    shutdown: watch::Sender<bool>,
    listener_task: JoinHandle<()>,
}

impl Drop for ActiveTunnel {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.listener_task.abort();
    }
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct TunnelStatus {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    #[specta(type = specta_typescript::Number)]
    pub bytes_up: u64,
    #[specta(type = specta_typescript::Number)]
    pub bytes_down: u64,
    pub connections: u32,
}

#[derive(Default)]
pub struct TunnelManager {
    tunnels: Mutex<HashMap<String, ActiveTunnel>>,
}

impl TunnelManager {
    pub async fn start(
        &self,
        ssh: Arc<SshSession>,
        session_id: &str,
        name: &str,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> AppResult<TunnelStatus> {
        // One tunnel per local port.
        {
            let tunnels = self.tunnels.lock().unwrap();
            if tunnels.values().any(|t| t.local_port == local_port) {
                return Err(AppError::Tunnel(format!(
                    "local port {local_port} is already forwarded"
                )));
            }
        }
        let listener = TcpListener::bind(("127.0.0.1", local_port))
            .await
            .map_err(|e| AppError::Tunnel(format!("bind 127.0.0.1:{local_port}: {e}")))?;

        let bytes_up = Arc::new(AtomicU64::new(0));
        let bytes_down = Arc::new(AtomicU64::new(0));
        let connections = Arc::new(AtomicU32::new(0));

        let task_ssh = ssh.clone();
        let task_host = remote_host.to_string();
        let task_up = bytes_up.clone();
        let task_down = bytes_down.clone();
        let task_conns = connections.clone();
        let (shutdown, mut listener_shutdown) = watch::channel(false);
        let connection_shutdown = shutdown.clone();
        let listener_task = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    result = listener.accept() => result,
                    _ = listener_shutdown.changed() => break,
                };
                let Ok((socket, peer)) = accepted else { break };
                let channel = {
                    let handle = task_ssh.handle.lock().await;
                    handle
                        .channel_open_direct_tcpip(
                            task_host.clone(),
                            remote_port as u32,
                            peer.ip().to_string(),
                            peer.port() as u32,
                        )
                        .await
                };
                let Ok(channel) = channel else {
                    continue; // remote refused; keep listening
                };
                let up = task_up.clone();
                let down = task_down.clone();
                let conns = task_conns.clone();
                let mut shutdown = connection_shutdown.subscribe();
                conns.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    let (mut sock_r, mut sock_w) = socket.into_split();
                    let stream = channel.into_stream();
                    let (mut chan_r, mut chan_w) = tokio::io::split(stream);
                    let upload = async {
                        let mut buf = vec![0u8; 32 * 1024];
                        loop {
                            let n = sock_r.read(&mut buf).await?;
                            if n == 0 {
                                break;
                            }
                            chan_w.write_all(&buf[..n]).await?;
                            up.fetch_add(n as u64, Ordering::Relaxed);
                        }
                        chan_w.shutdown().await
                    };
                    let download = async {
                        let mut buf = vec![0u8; 32 * 1024];
                        loop {
                            let n = chan_r.read(&mut buf).await?;
                            if n == 0 {
                                break;
                            }
                            sock_w.write_all(&buf[..n]).await?;
                            down.fetch_add(n as u64, Ordering::Relaxed);
                        }
                        sock_w.shutdown().await
                    };
                    tokio::select! {
                        _ = shutdown.changed() => {}
                        _ = async { tokio::try_join!(upload, download) } => {}
                    }
                    conns.fetch_sub(1, Ordering::Relaxed);
                });
            }
        });

        let tunnel = ActiveTunnel {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            name: name.to_string(),
            local_port,
            remote_host: remote_host.to_string(),
            remote_port,
            bytes_up,
            bytes_down,
            connections,
            shutdown,
            listener_task,
        };
        let status = status_of(&tunnel);
        self.tunnels
            .lock()
            .unwrap()
            .insert(tunnel.id.clone(), tunnel);
        Ok(status)
    }

    pub fn stop(&self, tunnel_id: &str) {
        // Drop stops the listener and closes every live forwarding task.
        self.tunnels.lock().unwrap().remove(tunnel_id);
    }

    pub fn stop_session(&self, session_id: &str) {
        self.tunnels
            .lock()
            .unwrap()
            .retain(|_, t| t.session_id != session_id);
    }

    pub fn list(&self, session_id: Option<&str>) -> Vec<TunnelStatus> {
        self.tunnels
            .lock()
            .unwrap()
            .values()
            .filter(|t| session_id.is_none_or(|s| t.session_id == s))
            .map(status_of)
            .collect()
    }
}

fn status_of(t: &ActiveTunnel) -> TunnelStatus {
    TunnelStatus {
        id: t.id.clone(),
        session_id: t.session_id.clone(),
        name: t.name.clone(),
        local_port: t.local_port,
        remote_host: t.remote_host.clone(),
        remote_port: t.remote_port,
        bytes_up: t.bytes_up.load(Ordering::Relaxed),
        bytes_down: t.bytes_down.load(Ordering::Relaxed),
        connections: t.connections.load(Ordering::Relaxed),
    }
}
