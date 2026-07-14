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
use tokio::task::{JoinHandle, JoinSet};

use crate::error::{AppError, AppResult};
use crate::session::ssh::SshSession;
use crate::session::{LifecycleCleanup, SessionOperation};

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
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    listener_task: Option<JoinHandle<()>>,
    cleanup: Option<LifecycleCleanup>,
}

impl Drop for ActiveTunnel {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.listener_task.take() {
            if let Some(cleanup) = &self.cleanup {
                let wait = async move {
                    let _ = task.await;
                };
                if let Err(wait) = cleanup.try_spawn(wait) {
                    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                        runtime.spawn(wait);
                    }
                }
            } else {
                task.abort();
            }
        }
    }
}

impl ActiveTunnel {
    async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.listener_task.as_mut() {
            let _ = task.await;
        }
        self.listener_task.take();
    }
}

struct TunnelConnectionCount(Arc<AtomicU32>);

impl Drop for TunnelConnectionCount {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn abort_and_drain(tasks: &mut JoinSet<()>) {
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
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
        self.start_inner(
            ssh,
            session_id,
            name,
            local_port,
            remote_host,
            remote_port,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_guarded(
        &self,
        ssh: Arc<SshSession>,
        session_id: &str,
        name: &str,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
        operation: &SessionOperation,
    ) -> AppResult<TunnelStatus> {
        self.start_inner(
            ssh,
            session_id,
            name,
            local_port,
            remote_host,
            remote_port,
            Some(operation),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_inner(
        &self,
        ssh: Arc<SshSession>,
        session_id: &str,
        name: &str,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
        operation: Option<&SessionOperation>,
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
        let (shutdown, mut shutdown_rx) = tokio::sync::oneshot::channel();
        let listener_task = tokio::spawn(async move {
            let mut live_connections = JoinSet::new();
            loop {
                let accepted = tokio::select! {
                    accepted = listener.accept() => Some(accepted),
                    _ = live_connections.join_next(), if !live_connections.is_empty() => None,
                    _ = &mut shutdown_rx => break,
                };
                let (socket, peer) = match accepted {
                    Some(Ok(connection)) => connection,
                    Some(Err(_)) => break,
                    None => continue,
                };
                let channel = tokio::select! {
                    channel = task_ssh.channel_open_direct_tcpip(
                            task_host.clone(),
                            remote_port as u32,
                            peer.ip().to_string(),
                            peer.port() as u32,
                        ) => channel,
                    _ = &mut shutdown_rx => break,
                };
                let Ok(channel) = channel else {
                    continue; // remote refused; keep listening
                };
                let up = task_up.clone();
                let down = task_down.clone();
                let conns = task_conns.clone();
                conns.fetch_add(1, Ordering::Relaxed);
                let connection_count = TunnelConnectionCount(conns);
                live_connections.spawn(async move {
                    let _connection_count = connection_count;
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
                    let _: std::io::Result<((), ())> = tokio::try_join!(upload, download);
                });
            }
            abort_and_drain(&mut live_connections).await;
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
            shutdown: Some(shutdown),
            listener_task: Some(listener_task),
            cleanup: operation.map(SessionOperation::cleanup),
        };
        if let Some(operation) = operation {
            self.register_tunnel(operation, tunnel).await
        } else {
            let status = status_of(&tunnel);
            self.tunnels
                .lock()
                .unwrap()
                .insert(tunnel.id.clone(), tunnel);
            Ok(status)
        }
    }

    async fn register_tunnel(
        &self,
        operation: &SessionOperation,
        tunnel: ActiveTunnel,
    ) -> AppResult<TunnelStatus> {
        let status = status_of(&tunnel);
        let mut pending = Some(tunnel);
        let registration = {
            let mut tunnels = self.tunnels.lock().unwrap();
            operation.register(|| {
                let tunnel = pending.take().unwrap();
                tunnels.insert(tunnel.id.clone(), tunnel);
            })
        };
        if let Err(error) = registration {
            let tunnel = pending.take().unwrap();
            let cleanup = operation.cleanup();
            let shutdown = async move { tunnel.shutdown().await };
            if let Err(shutdown) = cleanup.try_spawn(shutdown) {
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    runtime.spawn(shutdown);
                }
            }
            return Err(error);
        }
        Ok(status)
    }

    pub async fn stop(&self, tunnel_id: &str) {
        let completion = {
            let mut tunnels = self.tunnels.lock().unwrap();
            tunnels.remove(tunnel_id).and_then(schedule_shutdown)
        };
        if let Some(completion) = completion {
            let _ = completion.await;
        }
    }

    pub fn session_id(&self, tunnel_id: &str) -> Option<String> {
        self.tunnels
            .lock()
            .unwrap()
            .get(tunnel_id)
            .map(|tunnel| tunnel.session_id.clone())
    }

    pub async fn stop_session(&self, session_id: &str) {
        let drained = {
            let mut tunnels = self.tunnels.lock().unwrap();
            let ids: Vec<String> = tunnels
                .iter()
                .filter(|(_, tunnel)| tunnel.session_id == session_id)
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter()
                .filter_map(|id| tunnels.remove(&id))
                .collect::<Vec<_>>()
        };
        for tunnel in drained {
            tunnel.shutdown().await;
        }
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

fn schedule_shutdown(tunnel: ActiveTunnel) -> Option<tokio::sync::oneshot::Receiver<()>> {
    let cleanup = tunnel.cleanup.clone();
    let (completed, completed_rx) = tokio::sync::oneshot::channel();
    let shutdown = async move {
        tunnel.shutdown().await;
        let _ = completed.send(());
    };
    if let Some(cleanup) = cleanup {
        match cleanup.try_spawn(shutdown) {
            Ok(()) => return Some(completed_rx),
            Err(shutdown) => {
                let Ok(runtime) = tokio::runtime::Handle::try_current() else {
                    return None;
                };
                runtime.spawn(shutdown);
            }
        }
    } else {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return None;
        };
        runtime.spawn(shutdown);
    }
    Some(completed_rx)
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, AtomicU64};
    use std::sync::Arc;

    use super::{abort_and_drain, ActiveTunnel, TunnelManager};
    use crate::error::AppError;
    use crate::session::lifecycle::LifecycleGate;

    #[tokio::test]
    async fn tunnel_shutdown_waits_for_blocked_children_to_drop() {
        struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

        impl Drop for DropSignal {
            fn drop(&mut self) {
                if let Some(signal) = self.0.take() {
                    let _ = signal.send(());
                }
            }
        }

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(async move {
            let _drop_signal = DropSignal(Some(dropped_tx));
            let _ = started_tx.send(());
            std::future::pending::<()>().await;
        });
        started_rx.await.unwrap();

        abort_and_drain(&mut tasks).await;

        dropped_rx
            .await
            .expect("blocked forwarding child was dropped before shutdown returned");
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn late_tunnel_listener_is_rejected_and_joined() {
        let lifecycle = Arc::new(LifecycleGate::default());
        let operation = lifecycle.try_begin_operation().unwrap();
        let close = tokio::spawn({
            let lifecycle = lifecycle.clone();
            async move { lifecycle.begin_close().await }
        });
        operation.cancelled().await;

        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let (shutdown_seen, shutdown_seen_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = tokio::sync::oneshot::channel();
        let (stopped_tx, stopped_rx) = tokio::sync::oneshot::channel();
        let listener_task = tokio::spawn(async move {
            let _ = shutdown_rx.await;
            let _ = shutdown_seen.send(());
            let _ = release_rx.await;
            let _ = stopped_tx.send(());
        });
        let tunnel = ActiveTunnel {
            id: "tunnel".into(),
            session_id: "session".into(),
            name: "late".into(),
            local_port: 12345,
            remote_host: "example.com".into(),
            remote_port: 22,
            bytes_up: Arc::new(AtomicU64::new(0)),
            bytes_down: Arc::new(AtomicU64::new(0)),
            connections: Arc::new(AtomicU32::new(0)),
            shutdown: Some(shutdown),
            listener_task: Some(listener_task),
            cleanup: Some(operation.cleanup()),
        };
        let manager = Arc::new(TunnelManager::default());
        let (registered, registered_rx) = tokio::sync::oneshot::channel();
        let caller = tokio::spawn({
            let manager = manager.clone();
            async move {
                let result = manager.register_tunnel(&operation, tunnel).await;
                let _ = registered.send(matches!(result, Err(AppError::SessionNotFound)));
                std::future::pending::<()>().await;
            }
        });
        assert!(registered_rx.await.unwrap());
        shutdown_seen_rx.await.unwrap();
        caller.abort();
        let _ = caller.await;
        assert!(!close.is_finished());

        release.send(()).unwrap();
        stopped_rx.await.unwrap();
        assert!(manager.list(Some("session")).is_empty());
        close.await.unwrap().finish().await;
    }

    #[tokio::test]
    async fn cancelled_stop_still_joins_the_listener_before_session_close() {
        let lifecycle = Arc::new(LifecycleGate::default());
        let operation = lifecycle.try_begin_operation().unwrap();
        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let (shutdown_seen, shutdown_seen_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = tokio::sync::oneshot::channel();
        let (stopped, stopped_rx) = tokio::sync::oneshot::channel();
        let listener_task = tokio::spawn(async move {
            let _ = shutdown_rx.await;
            let _ = shutdown_seen.send(());
            let _ = release_rx.await;
            let _ = stopped.send(());
        });
        let tunnel = ActiveTunnel {
            id: "tunnel".into(),
            session_id: "session".into(),
            name: "active".into(),
            local_port: 12345,
            remote_host: "example.com".into(),
            remote_port: 22,
            bytes_up: Arc::new(AtomicU64::new(0)),
            bytes_down: Arc::new(AtomicU64::new(0)),
            connections: Arc::new(AtomicU32::new(0)),
            shutdown: Some(shutdown),
            listener_task: Some(listener_task),
            cleanup: Some(operation.cleanup()),
        };
        let manager = Arc::new(TunnelManager::default());
        manager
            .tunnels
            .lock()
            .unwrap()
            .insert(tunnel.id.clone(), tunnel);

        let caller = tokio::spawn({
            let manager = manager.clone();
            async move { manager.stop("tunnel").await }
        });
        shutdown_seen_rx.await.unwrap();
        caller.abort();
        let _ = caller.await;
        assert!(manager.list(Some("session")).is_empty());

        let close = tokio::spawn({
            let lifecycle = lifecycle.clone();
            async move { lifecycle.begin_close().await }
        });
        operation.cancelled().await;
        drop(operation);
        tokio::task::yield_now().await;
        assert!(!close.is_finished());

        release.send(()).unwrap();
        stopped_rx.await.unwrap();
        close.await.unwrap().finish().await;
    }
}
