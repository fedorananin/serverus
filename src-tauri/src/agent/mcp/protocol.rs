//! MCP message shapes (JSON-RPC 2.0) for the subset Serverus serves:
//! lifecycle, ping, and tools.

use serde_json::{json, Value};

/// Protocol revisions this server speaks, newest first. The tool surface is
/// identical across them.
pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

/// One decoded inbound message.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// A response to a request we sent (we send none) — ignored.
    Response,
}

/// Decode one JSON-RPC message. On failure returns the error response to
/// send (with a `null` id when the id is unknown).
pub fn decode(message: Value) -> Result<Incoming, Value> {
    let Value::Object(mut object) = message else {
        return Err(error_response(
            Value::Null,
            INVALID_REQUEST,
            "expected a JSON object",
        ));
    };
    let id = object.remove("id");
    let params = object.remove("params").unwrap_or(Value::Null);
    match (object.remove("method"), id) {
        (Some(Value::String(method)), Some(id)) if !id.is_null() => {
            Ok(Incoming::Request { id, method, params })
        }
        (Some(Value::String(method)), None) => Ok(Incoming::Notification { method, params }),
        (None, Some(_)) if object.contains_key("result") || object.contains_key("error") => {
            Ok(Incoming::Response)
        }
        (_, id) => Err(error_response(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "not a JSON-RPC request",
        )),
    }
}

pub fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The `initialize` result: echo the client's revision when we speak it,
/// otherwise offer our newest.
pub fn initialize_result(params: &Value, instructions: &str) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|version| SUPPORTED_VERSIONS.contains(version))
        .unwrap_or(SUPPORTED_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "serverus", "title": "Serverus", "version": env!("CARGO_PKG_VERSION") },
        "instructions": instructions,
    })
}

/// A `tools/call` result carrying one text block.
pub fn tool_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}
