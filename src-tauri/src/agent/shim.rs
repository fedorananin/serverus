//! `serverus --mcp`: the stdio MCP server an agent launches.
//!
//! It relays between the agent's stdin/stdout and the running Serverus
//! app's local socket, where the actual MCP server lives next to the
//! unlocked vault and the open sessions. MCP clients start their servers
//! with every session, so the shim answers the start-up handshake itself
//! ([`lazy`]) and reaches the app — starting it when it is not running —
//! only when the agent first calls a tool.

mod lazy;
mod lines;

pub use lazy::{serve_lazily, APP_WENT_AWAY};

/// Command-line flag that selects shim mode.
pub const FLAG: &str = "--mcp";

/// Run the relay; returns the process exit code.
pub fn run() -> i32 {
    #[cfg(unix)]
    {
        unix::run()
    }
    #[cfg(not(unix))]
    {
        eprintln!("serverus --mcp: AI agent access is not available on this platform yet.");
        1
    }
}

#[cfg(unix)]
mod unix {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use tokio::net::UnixStream;

    use crate::agent::transport::socket_path;

    const LAUNCH_WAIT: Duration = Duration::from_secs(45);

    pub fn run() -> i32 {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("serverus --mcp: {error}");
                return 1;
            }
        };
        let code = runtime.block_on(relay());
        // The blocking stdin reader cannot be interrupted; exit instead of
        // waiting for the runtime to shut down around it.
        std::process::exit(code);
    }

    async fn relay() -> i32 {
        super::serve_lazily(tokio::io::stdin(), tokio::io::stdout(), connect).await;
        0
    }

    async fn connect() -> Result<UnixStream, String> {
        let path = socket_path();
        if let Ok(stream) = UnixStream::connect(&path).await {
            return Ok(stream);
        }
        launch_app()?;
        let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;
        while tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if let Ok(stream) = UnixStream::connect(&path).await {
                return Ok(stream);
            }
        }
        Err(format!(
            "Serverus did not start serving agents within {}s. Open Serverus, check Settings → AI Agent, and try again.",
            LAUNCH_WAIT.as_secs()
        ))
    }

    /// The `.app` bundle this executable lives in, when it does.
    fn app_bundle(exe: &Path) -> Option<PathBuf> {
        exe.ancestors()
            .find(|dir| dir.extension().is_some_and(|ext| ext == "app"))
            .map(Path::to_path_buf)
    }

    fn launch_app() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let spawned = match app_bundle(&exe) {
            // Launch Services starts (or re-uses) the app like a Dock click.
            Some(bundle) if cfg!(target_os = "macos") => Command::new("open")
                .arg("-a")
                .arg(bundle)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|_| ()),
            _ => {
                use std::os::unix::process::CommandExt;
                // Its own process group, so the agent tearing down this shim
                // does not take the app with it.
                Command::new(&exe)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .process_group(0)
                    .spawn()
                    .map(|_| ())
            }
        };
        spawned.map_err(|error| format!("cannot start Serverus: {error}"))
    }
}
