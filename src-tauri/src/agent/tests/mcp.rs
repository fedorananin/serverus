use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines};

use crate::agent::mcp::server::{serve, ToolHost, ToolOutput};
use crate::agent::mcp::tool_specs;

pub(super) struct FakeHost;

#[async_trait]
impl ToolHost for FakeHost {
    fn instructions(&self) -> String {
        "be careful".into()
    }

    fn tools(&self) -> Value {
        json!([{ "name": "echo" }])
    }

    async fn call(&self, name: String, arguments: Value) -> Option<ToolOutput> {
        match name.as_str() {
            "echo" => Some(ToolOutput {
                text: arguments["text"].as_str().unwrap_or_default().to_string(),
                is_error: false,
            }),
            "fail" => Some(ToolOutput {
                text: "nope".into(),
                is_error: true,
            }),
            "slow" => {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Some(ToolOutput {
                    text: "late".into(),
                    is_error: false,
                })
            }
            _ => None,
        }
    }
}

struct Client {
    writer: tokio::io::WriteHalf<DuplexStream>,
    lines: Lines<BufReader<tokio::io::ReadHalf<DuplexStream>>>,
}

impl Client {
    fn start() -> Self {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (server_read, server_write) = tokio::io::split(server);
        tokio::spawn(serve(server_read, server_write, Arc::new(FakeHost)));
        let (read, writer) = tokio::io::split(client);
        Self {
            writer,
            lines: BufReader::new(read).lines(),
        }
    }

    async fn send(&mut self, message: Value) {
        self.send_raw(&message.to_string()).await;
    }

    async fn send_raw(&mut self, line: &str) {
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.write_all(b"\n").await.unwrap();
    }

    async fn receive(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(5), self.lines.next_line())
            .await
            .expect("a reply in time")
            .unwrap()
            .expect("an open stream");
        serde_json::from_str(&line).unwrap()
    }

    async fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
                          "params": { "name": name, "arguments": arguments } }))
            .await;
        self.receive().await
    }
}

#[tokio::test]
async fn initialize_negotiates_the_protocol_version() {
    let mut client = Client::start();
    client
        .send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                      "params": { "protocolVersion": "2025-03-26", "capabilities": {} } }))
        .await;
    let reply = client.receive().await;
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(reply["result"]["serverInfo"]["name"], "serverus");
    assert_eq!(reply["result"]["instructions"], "be careful");
    assert!(reply["result"]["capabilities"]["tools"].is_object());

    client
        .send(json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize",
                      "params": { "protocolVersion": "1999-01-01" } }))
        .await;
    assert_eq!(
        client.receive().await["result"]["protocolVersion"],
        "2025-06-18"
    );
}

#[tokio::test]
async fn notifications_get_no_reply_and_ping_does() {
    let mut client = Client::start();
    client
        .send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await;
    client
        .send(json!({ "jsonrpc": "2.0", "id": "p", "method": "ping" }))
        .await;
    let reply = client.receive().await;
    assert_eq!(reply["id"], "p");
    assert_eq!(reply["result"], json!({}));
}

#[tokio::test]
async fn tools_are_listed_and_called() {
    let mut client = Client::start();
    client
        .send(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .await;
    assert_eq!(client.receive().await["result"]["tools"][0]["name"], "echo");

    let reply = client.call(2, "echo", json!({ "text": "hi" })).await;
    assert_eq!(reply["result"]["content"][0]["text"], "hi");
    assert_eq!(reply["result"]["isError"], false);

    let reply = client.call(3, "fail", json!({})).await;
    assert_eq!(reply["result"]["isError"], true);

    let reply = client.call(4, "missing", json!({})).await;
    assert_eq!(reply["error"]["code"], -32602);
}

#[tokio::test]
async fn malformed_input_gets_json_rpc_errors() {
    let mut client = Client::start();
    client.send_raw("{not json").await;
    assert_eq!(client.receive().await["error"]["code"], -32700);
    client
        .send(json!({ "jsonrpc": "2.0", "id": 9, "method": "resources/list" }))
        .await;
    let reply = client.receive().await;
    assert_eq!(reply["id"], 9);
    assert_eq!(reply["error"]["code"], -32601);
    client.send(json!({ "jsonrpc": "2.0", "id": 10 })).await;
    assert_eq!(client.receive().await["error"]["code"], -32600);
}

#[tokio::test]
async fn a_cancelled_call_is_never_answered() {
    let mut client = Client::start();
    client
        .send(json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                      "params": { "name": "slow" } }))
        .await;
    client
        .send(
            json!({ "jsonrpc": "2.0", "method": "notifications/cancelled",
                      "params": { "requestId": 7 } }),
        )
        .await;
    // The next reply is for the ping, not the cancelled call.
    client
        .send(json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" }))
        .await;
    assert_eq!(client.receive().await["id"], 8);
}

#[test]
fn the_tool_catalog_is_well_formed() {
    let tools = tool_specs::all();
    let tools = tools.as_array().unwrap();
    let mut names = HashSet::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(names.insert(name.to_string()), "duplicate tool {name}");
        assert!(!tool["description"].as_str().unwrap().is_empty());
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object");
        for required in schema["required"].as_array().unwrap() {
            let field = required.as_str().unwrap();
            assert!(schema["properties"].get(field).is_some(), "{name}: {field}");
        }
    }
    let expected: HashSet<String> = crate::agent::tools::NAMES
        .iter()
        .map(|n| n.to_string())
        .collect();
    assert_eq!(
        names, expected,
        "catalog and dispatcher must list the same tools"
    );
}
