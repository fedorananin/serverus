//! The local socket MCP clients reach through the `--mcp` shim.
//!
//! Only processes of the same OS user may connect: the socket file is
//! `0600` inside the per-user config directory, and every accepted peer's
//! UID is checked against the socket owner's — the ssh-agent trust model.

use std::path::PathBuf;

use crate::app_config;

/// Where the running app listens.
pub fn socket_path() -> PathBuf {
    app_config::config_dir().join("agent.sock")
}

#[cfg(unix)]
pub use unix::{bind, remove_socket, Bound};

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use tokio::net::{UnixListener, UnixStream};

    use super::socket_path;
    use crate::agent::mcp::server::{serve, ToolHost};

    /// Set once this process owns the socket file.
    static BOUND: AtomicBool = AtomicBool::new(false);

    /// A bound, owner-only socket.
    pub struct Bound {
        listener: UnixListener,
        owner: u32,
    }

    /// Bind the socket. A live socket from another running instance is
    /// left alone (that instance serves agents); a stale file from a crash
    /// is replaced.
    pub async fn bind() -> Result<Bound, String> {
        let path = socket_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        if path.exists() {
            if UnixStream::connect(&path).await.is_ok() {
                return Err("another Serverus instance is already serving agents".into());
            }
            let _ = std::fs::remove_file(&path);
        }
        let listener = UnixListener::bind(&path)
            .map_err(|error| format!("cannot listen on {}: {error}", path.display()))?;
        BOUND.store(true, Ordering::SeqCst);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("cannot restrict {}: {error}", path.display()))?;
        let owner = std::fs::metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
            .uid();
        Ok(Bound { listener, owner })
    }

    impl Bound {
        /// Serve connections until the app exits, each against its own host
        /// from `new_host` (dropped when the connection ends).
        pub async fn serve<H>(self, new_host: impl Fn() -> Arc<H>)
        where
            H: ToolHost,
        {
            let Bound { listener, owner } = self;
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    // Out of descriptors and the like: back off, don't spin.
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                };
                let same_user = stream.peer_cred().is_ok_and(|peer| peer.uid() == owner);
                if !same_user {
                    continue;
                }
                let host: Arc<dyn ToolHost> = new_host();
                tokio::spawn(async move {
                    let (reader, writer) = stream.into_split();
                    serve(reader, writer, host).await;
                });
            }
        }
    }

    /// Remove the socket on exit — only if this process bound it (another
    /// instance may be the one serving).
    pub fn remove_socket() {
        if BOUND.load(Ordering::SeqCst) {
            let _ = std::fs::remove_file(socket_path());
        }
    }
}
