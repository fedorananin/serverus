//! One MCP connection: newline-delimited JSON-RPC over a byte stream (the
//! stdio transport, relayed through the local socket by the shim).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::protocol::{
    decode, error_response, initialize_result, result_response, tool_result, Incoming,
    INVALID_PARAMS, METHOD_NOT_FOUND, PARSE_ERROR,
};

/// Upper bound for one message (a `write_file` carries the whole content).
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;

pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

/// What the connection serves.
#[async_trait]
pub trait ToolHost: Send + Sync + 'static {
    fn instructions(&self) -> String;
    /// The `tools` array of a `tools/list` result.
    fn tools(&self) -> Value;
    /// Run a tool. `None` means there is no tool by that name.
    async fn call(&self, name: String, arguments: Value) -> Option<ToolOutput>;
}

type InFlight = Arc<Mutex<HashMap<String, AbortHandle>>>;

/// Serve one connection until the peer disconnects. Tool calls run
/// concurrently; a `notifications/cancelled` aborts the named call.
pub async fn serve<R, W>(reader: R, writer: W, host: Arc<dyn ToolHost>)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (sender, mut outbox) = mpsc::unbounded_channel::<Value>();
    let writer_task = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(message) = outbox.recv().await {
            let mut line = serde_json::to_vec(&message).unwrap_or_default();
            line.push(b'\n');
            if writer.write_all(&line).await.is_err() || writer.flush().await.is_err() {
                break;
            }
        }
    });

    let in_flight: InFlight = Arc::default();
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut reader)
            .take(MAX_MESSAGE_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .await;
        match read {
            Ok(0) | Err(_) => break,
            Ok(_) if line.len() > MAX_MESSAGE_BYTES => {
                let _ = sender.send(error_response(
                    Value::Null,
                    PARSE_ERROR,
                    "message too large",
                ));
                break;
            }
            Ok(_) => {}
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                for message in batch {
                    dispatch(message, &host, &sender, &in_flight);
                }
            }
            Ok(message) => dispatch(message, &host, &sender, &in_flight),
            Err(_) => {
                let _ = sender.send(error_response(Value::Null, PARSE_ERROR, "invalid JSON"));
            }
        }
    }

    for (_, call) in in_flight.lock().unwrap().drain() {
        call.abort();
    }
    drop(sender);
    let _ = writer_task.await;
}

fn dispatch(
    message: Value,
    host: &Arc<dyn ToolHost>,
    sender: &mpsc::UnboundedSender<Value>,
    in_flight: &InFlight,
) {
    let (id, method, params) = match decode(message) {
        Err(response) => {
            let _ = sender.send(response);
            return;
        }
        Ok(Incoming::Response) => return,
        Ok(Incoming::Notification { method, params }) => {
            if method == "notifications/cancelled" {
                if let Some(request) = params.get("requestId") {
                    if let Some(call) = in_flight.lock().unwrap().remove(&request.to_string()) {
                        call.abort();
                    }
                }
            }
            return;
        }
        Ok(Incoming::Request { id, method, params }) => (id, method, params),
    };

    let reply = match method.as_str() {
        "initialize" => result_response(id, initialize_result(&params, &host.instructions())),
        "ping" => result_response(id, json!({})),
        "tools/list" => result_response(id, json!({ "tools": host.tools() })),
        "tools/call" => {
            spawn_call(id, params, host.clone(), sender.clone(), in_flight.clone());
            return;
        }
        _ => error_response(
            id,
            METHOD_NOT_FOUND,
            &format!("method `{method}` not found"),
        ),
    };
    let _ = sender.send(reply);
}

fn spawn_call(
    id: Value,
    params: Value,
    host: Arc<dyn ToolHost>,
    sender: mpsc::UnboundedSender<Value>,
    in_flight: InFlight,
) {
    let Some(name) = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        let _ = sender.send(error_response(
            id,
            INVALID_PARAMS,
            "tools/call needs a tool name",
        ));
        return;
    };
    let arguments = match params.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(arguments) => arguments.clone(),
    };
    let key = id.to_string();
    // Hold the registry while spawning so the task cannot finish (and
    // remove itself) before it is registered.
    let mut calls = in_flight.lock().unwrap();
    let registry = in_flight.clone();
    let task_key = key.clone();
    let task = tokio::spawn(async move {
        let reply = match host.call(name.clone(), arguments).await {
            Some(output) => result_response(id, tool_result(&output.text, output.is_error)),
            None => error_response(id, INVALID_PARAMS, &format!("unknown tool `{name}`")),
        };
        registry.lock().unwrap().remove(&task_key);
        let _ = sender.send(reply);
    });
    calls.insert(key, task.abort_handle());
}
