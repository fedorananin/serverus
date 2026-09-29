//! The shim's protocol layer. It answers what an MCP client asks while it
//! starts up (`initialize`, `ping`, `tools/list`) by itself and reaches the
//! running app only for the first real tool call — so starting an agent
//! session never launches Serverus. Once connected it relays verbatim. If
//! the app goes away mid-session, the calls still waiting get an error and
//! the next tool call connects (and launches) again.

use std::collections::HashSet;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite, BufReader, WriteHalf};

use super::lines::{read_line, write_line, Line};

use crate::agent::mcp::protocol::{
    decode, error_response, initialize_result, result_response, tool_result, Incoming,
    METHOD_NOT_FOUND, PARSE_ERROR,
};
use crate::agent::mcp::{tool_specs, INSTRUCTIONS};

/// What a call still waiting for the app is told when the app goes away.
pub const APP_WENT_AWAY: &str =
    "Serverus quit (or restarted) before this call finished. Call the tool again to reconnect.";

type Output<W> = Arc<tokio::sync::Mutex<W>>;
/// Ids (as JSON text) of the requests forwarded to the app and unanswered.
type Pending = Arc<Mutex<HashSet<String>>>;

/// Serve one MCP client on `input` / `output`. `connect` reaches the app,
/// launching it if needed; it is called on the first tool call, and again
/// after the app went away.
pub async fn serve_lazily<R, W, C, Fut, S>(input: R, output: W, mut connect: C)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
    C: FnMut() -> Fut,
    Fut: Future<Output = Result<S, String>>,
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let out: Output<W> = Arc::new(tokio::sync::Mutex::new(output));
    let mut input = BufReader::new(input);
    let mut link: Option<Link<S>> = None;
    let mut line = Vec::new();
    loop {
        line.clear();
        match read_line(&mut input, &mut line).await {
            Ok(Line::Data) => {}
            Ok(Line::TooLong) => {
                write_json(
                    &out,
                    error_response(Value::Null, PARSE_ERROR, "message too large"),
                )
                .await;
                continue;
            }
            Ok(Line::Eof) | Err(_) => break,
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Some(open) = link.as_mut().filter(|link| link.is_alive()) {
            open.forward(&line, &out).await;
            continue;
        }
        link = None;
        match route(&line) {
            Route::Local(reply) => {
                if let Some(reply) = reply {
                    write_json(&out, reply).await;
                }
            }
            Route::App(id) => match connect().await {
                Ok(stream) => {
                    let mut opened = Link::open(stream, out.clone());
                    opened.forward(&line, &out).await;
                    link = Some(opened);
                }
                Err(problem) => {
                    write_json(&out, result_response(id, tool_result(&problem, true))).await
                }
            },
        }
    }
    // The client closed our stdin: dropping the link tells the app the
    // session is over.
}

/// What to do with one client message while no app is connected.
enum Route {
    /// Answer here (`None`: nothing to answer, e.g. a notification).
    Local(Option<Value>),
    /// A tool call, which needs the app; carries its id.
    App(Value),
}

fn route(line: &[u8]) -> Route {
    let message = match serde_json::from_slice::<Value>(line) {
        Ok(message) => message,
        Err(_) => {
            return Route::Local(Some(error_response(
                Value::Null,
                PARSE_ERROR,
                "invalid JSON",
            )))
        }
    };
    // A batch may hold tool calls; the app sorts it out.
    if message.is_array() {
        return Route::App(Value::Null);
    }
    match decode(message) {
        Err(response) => Route::Local(Some(response)),
        Ok(Incoming::Response | Incoming::Notification { .. }) => Route::Local(None),
        Ok(Incoming::Request { id, method, params }) => Route::Local(Some(match method.as_str() {
            "initialize" => result_response(id, initialize_result(&params, INSTRUCTIONS)),
            "ping" => result_response(id, json!({})),
            "tools/list" => result_response(id, json!({ "tools": tool_specs::all() })),
            "tools/call" => return Route::App(id),
            _ => error_response(
                id,
                METHOD_NOT_FOUND,
                &format!("method `{method}` not found"),
            ),
        })),
    }
}

/// A live connection to the app.
struct Link<S> {
    writer: WriteHalf<S>,
    alive: Arc<AtomicBool>,
    pending: Pending,
}

impl<S> Link<S>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    fn open<W>(stream: S, out: Output<W>) -> Self
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, writer) = tokio::io::split(stream);
        let alive = Arc::new(AtomicBool::new(true));
        let pending = Pending::default();
        tokio::spawn(pump(reader, out, pending.clone(), alive.clone()));
        Self {
            writer,
            alive,
            pending,
        }
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Send one client message to the app. A request that cannot reach it
    /// is answered with an error right here.
    async fn forward<W>(&mut self, line: &[u8], out: &Output<W>)
    where
        W: AsyncWrite + Unpin,
    {
        let ids = request_ids(line);
        // Registered under the lock the reader clears the set with, so a
        // request is either answered by the app or failed by the reader.
        let registered = {
            let mut pending = self.pending.lock().unwrap();
            let alive = self.is_alive();
            if alive {
                pending.extend(ids.iter().cloned());
            }
            alive
        };
        if !registered {
            fail_all(out, ids).await;
            return;
        }
        if write_line(&mut self.writer, line).await.is_err() {
            let orphans: Vec<String> = {
                let mut pending = self.pending.lock().unwrap();
                ids.into_iter().filter(|id| pending.remove(id)).collect()
            };
            fail_all(out, orphans).await;
        }
    }
}

/// Copy the app's messages to the client until the app goes away, then
/// fail what it left unanswered.
async fn pump<R, W>(app: R, out: Output<W>, pending: Pending, alive: Arc<AtomicBool>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut app = BufReader::new(app);
    let mut line = Vec::new();
    loop {
        line.clear();
        match read_line(&mut app, &mut line).await {
            Ok(Line::Data) => {}
            Ok(Line::TooLong) => continue,
            Ok(Line::Eof) | Err(_) => break,
        }
        if let Some(id) = response_id(&line) {
            pending.lock().unwrap().remove(&id);
        }
        let mut out = out.lock().await;
        if write_line(&mut *out, &line).await.is_err() {
            break;
        }
    }
    let orphans: Vec<String> = {
        let mut pending = pending.lock().unwrap();
        alive.store(false, Ordering::SeqCst);
        pending.drain().collect()
    };
    fail_all(&out, orphans).await;
}

async fn fail_all<W: AsyncWrite + Unpin>(out: &Output<W>, ids: Vec<String>) {
    for id in ids {
        let id = serde_json::from_str(&id).unwrap_or(Value::Null);
        write_json(out, result_response(id, tool_result(APP_WENT_AWAY, true))).await;
    }
}

/// Ids of the requests in a message (a batch holds several).
fn request_ids(line: &[u8]) -> Vec<String> {
    let Ok(message) = serde_json::from_slice::<Value>(line) else {
        return Vec::new();
    };
    let messages = match message {
        Value::Array(batch) => batch,
        single => vec![single],
    };
    messages
        .iter()
        .filter(|message| message.get("method").is_some())
        .filter_map(|message| message.get("id").filter(|id| !id.is_null()))
        .map(Value::to_string)
        .collect()
}

/// The id a response answers, if the line is a response.
fn response_id(line: &[u8]) -> Option<String> {
    let message = serde_json::from_slice::<Value>(line).ok()?;
    if message.get("result").is_none() && message.get("error").is_none() {
        return None;
    }
    message.get("id").map(Value::to_string)
}

async fn write_json<W: AsyncWrite + Unpin>(out: &Output<W>, message: Value) {
    let mut line = serde_json::to_vec(&message).unwrap_or_default();
    line.push(b'\n');
    let mut out = out.lock().await;
    let _ = write_line(&mut *out, &line).await;
}
