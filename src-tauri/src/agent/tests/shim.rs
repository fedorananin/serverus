//! The `--mcp` shim against the real MCP server (with a fake tool host)
//! over in-memory pipes: what it answers alone, and when it reaches the app.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines};
use tokio::task::JoinHandle;

use super::mcp::FakeHost;
use crate::agent::mcp::server::serve;
use crate::agent::mcp::{tool_specs, INSTRUCTIONS};
use crate::agent::shim::{serve_lazily, APP_WENT_AWAY};

/// The app side: counts connections and can fail or quit on demand.
#[derive(Default)]
struct App {
    connects: AtomicUsize,
    fail_next: Mutex<bool>,
    sockets: Mutex<Vec<JoinHandle<()>>>,
}

impl App {
    fn quit(&self) {
        for socket in self.sockets.lock().unwrap().drain(..) {
            socket.abort();
        }
    }
}

struct Agent {
    writer: tokio::io::WriteHalf<DuplexStream>,
    lines: Lines<BufReader<tokio::io::ReadHalf<DuplexStream>>>,
    app: Arc<App>,
}

impl Agent {
    fn start() -> Self {
        let app = Arc::new(App::default());
        let (agent, shim) = tokio::io::duplex(64 * 1024);
        let (shim_read, shim_write) = tokio::io::split(shim);
        let connecting = app.clone();
        tokio::spawn(serve_lazily(shim_read, shim_write, move || {
            let app = connecting.clone();
            async move {
                app.connects.fetch_add(1, Ordering::SeqCst);
                if std::mem::take(&mut *app.fail_next.lock().unwrap()) {
                    return Err("Serverus did not start".to_string());
                }
                // The socket runs through a proxy, so quitting can close it
                // the way a dying process does.
                let (shim_side, mut socket) = tokio::io::duplex(64 * 1024);
                let (mut server_end, app_side) = tokio::io::duplex(64 * 1024);
                let (read, write) = tokio::io::split(app_side);
                tokio::spawn(serve(read, write, Arc::new(FakeHost)));
                let socket = tokio::spawn(async move {
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut server_end).await;
                });
                app.sockets.lock().unwrap().push(socket);
                Ok(shim_side)
            }
        }));
        let (read, writer) = tokio::io::split(agent);
        Self {
            writer,
            lines: BufReader::new(read).lines(),
            app,
        }
    }

    async fn send(&mut self, message: Value) {
        let line = format!("{message}\n");
        self.writer.write_all(line.as_bytes()).await.unwrap();
    }

    async fn receive(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(5), self.lines.next_line())
            .await
            .expect("a reply in time")
            .unwrap()
            .expect("an open stream");
        serde_json::from_str(&line).unwrap()
    }

    async fn call(&mut self, id: u64, tool: &str) -> Value {
        self.send(json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": tool, "arguments": { "text": "hi" } },
        }))
        .await;
        self.receive().await
    }

    fn connects(&self) -> usize {
        self.app.connects.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn starting_a_session_does_not_reach_the_app() {
    let mut agent = Agent::start();
    agent
        .send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18", "capabilities": {} } }))
        .await;
    let init = agent.receive().await;
    assert_eq!(init["result"]["serverInfo"]["name"], "serverus");
    assert_eq!(init["result"]["instructions"], INSTRUCTIONS);
    agent
        .send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await;
    agent
        .send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
        .await;
    assert_eq!(agent.receive().await["result"]["tools"], tool_specs::all());
    agent
        .send(json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }))
        .await;
    assert_eq!(agent.receive().await["result"], json!({}));
    agent
        .send(json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" }))
        .await;
    assert_eq!(agent.receive().await["error"]["code"], -32601);
    assert_eq!(agent.connects(), 0, "nothing launched Serverus");
}

#[tokio::test]
async fn the_first_tool_call_connects_once() {
    let mut agent = Agent::start();
    let reply = agent.call(1, "echo").await;
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["content"][0]["text"], "hi");
    let reply = agent.call(2, "echo").await;
    assert_eq!(reply["id"], 2);
    // Once connected, everything is relayed to the app.
    agent
        .send(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }))
        .await;
    assert_eq!(
        agent.receive().await["result"]["tools"],
        json!([{ "name": "echo" }])
    );
    assert_eq!(agent.connects(), 1);
}

#[tokio::test]
async fn a_failed_launch_answers_the_call_and_the_next_call_retries() {
    let mut agent = Agent::start();
    *agent.app.fail_next.lock().unwrap() = true;
    let reply = agent.call(1, "echo").await;
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["isError"], true);
    assert_eq!(
        reply["result"]["content"][0]["text"],
        "Serverus did not start"
    );
    let reply = agent.call(2, "echo").await;
    assert_eq!(reply["result"]["content"][0]["text"], "hi");
    assert_eq!(agent.connects(), 2);
}

#[tokio::test]
async fn the_app_going_away_fails_waiting_calls_and_reconnects_later() {
    let mut agent = Agent::start();
    agent.call(1, "echo").await;
    agent
        .send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "slow", "arguments": {} } }))
        .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    agent.app.quit();
    let reply = agent.receive().await;
    assert_eq!(reply["id"], 2);
    assert_eq!(reply["result"]["isError"], true);
    assert_eq!(reply["result"]["content"][0]["text"], APP_WENT_AWAY);

    let reply = agent.call(3, "echo").await;
    assert_eq!(reply["result"]["content"][0]["text"], "hi");
    assert_eq!(agent.connects(), 2);
}
