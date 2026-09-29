//! `read_terminal` and `send_input`.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use serverus_domain::agent::access::OperationClass;
use serverus_domain::agent::terminal_text::{render, tail_lines, truncate_middle};

use super::super::args::parse;
use super::super::journal::Journal;
use super::super::keys::key_bytes;
use super::super::{Ctx, ToolError, ToolResult};
use super::follow::{self, Waited};
use super::{describe_outcome, ABANDONED, USER_HAS_CONTROL};
use crate::agent::exec;
use crate::session::terminal_tap::TerminalTap;

/// Longest terminal excerpt handed back to the model.
const MAX_SCREEN_CHARS: usize = 60_000;

#[derive(Deserialize)]
struct ReadArgs {
    server: String,
    #[serde(default = "default_lines")]
    lines: usize,
    #[serde(default)]
    wait_seconds: u64,
}

fn default_lines() -> usize {
    100
}

pub async fn read_terminal(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: ReadArgs = parse(arguments)?;
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    let tab = ctx.tab(&server, true).await?;
    let term_id = tab.term_id.clone().ok_or("The tab has no terminal.")?;
    let journal = Journal::start(
        ctx.app,
        "read_terminal",
        "Read the terminal",
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        let hub = ctx.hub();
        let (_, tap) = ctx.state().sessions.terminal_tap(&term_id).await?;
        let mut report = String::new();
        if args.wait_seconds > 0 {
            if let Some(running) = hub.control.running(&term_id) {
                let timeout = Duration::from_secs(args.wait_seconds.min(3600));
                match follow::wait(running.done, timeout).await {
                    Waited::Finished(outcome) => report.push_str(&format!(
                        "Your command finished — {}\n\n",
                        describe_outcome(&outcome, running.started.flavor)
                    )),
                    Waited::Abandoned => report.push_str(&format!("{ABANDONED}\n\n")),
                    Waited::StillRunning => {}
                }
            }
        }
        report.push_str(&status_line(ctx, &term_id, &tap));
        report.push_str(&format!(
            "\n--- terminal, last {} lines ---\n",
            args.lines.clamp(1, 2000)
        ));
        report.push_str(&screen(&tap, args.lines.clamp(1, 2000)));
        Ok(report)
    }
    .await;
    journal.finish(result)
}

fn status_line(ctx: &Ctx<'_>, term_id: &str, tap: &TerminalTap) -> String {
    let hub = ctx.hub();
    let mut parts = Vec::new();
    if hub.control.user_control(term_id) {
        parts.push("the user has taken control of this terminal".to_string());
    }
    if let Some(running) = hub.control.running(term_id) {
        parts.push(format!(
            "your command `{}` is still running",
            running.started.command
        ));
    }
    parts.push(format!(
        "the terminal is {}",
        exec::terminal_state(tap).describe()
    ));
    format!("State: {}.", parts.join("; "))
}

/// The terminal's retained output as text, last `lines` lines. Rendered
/// from a copy, so the terminal's reader is not held up meanwhile.
fn screen(tap: &TerminalTap, lines: usize) -> String {
    let snapshot = tap.snapshot();
    let view = snapshot.view();
    let (bytes, alt) = view.from_offset(view.start);
    let text = render(bytes, alt);
    truncate_middle(tail_lines(&text, lines), MAX_SCREEN_CHARS).0
}

#[derive(Deserialize)]
struct SendArgs {
    server: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    keys: Vec<String>,
}

pub async fn send_input(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: SendArgs = parse(arguments)?;
    let mut bytes = args.text.clone().unwrap_or_default().into_bytes();
    for key in &args.keys {
        bytes.extend(key_bytes(key).ok_or_else(|| ToolError(format!("Unknown key `{key}`.")))?);
    }
    if bytes.is_empty() {
        return Err("Nothing to send: give text and/or keys.".into());
    }
    let described = describe_input(args.text.as_deref(), &args.keys);

    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Exec)?;
    let tab = ctx.tab(&server, true).await?;
    let term_id = tab.term_id.clone().ok_or("The tab has no terminal.")?;
    let journal = Journal::start(
        ctx.app,
        "send_input",
        &described,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Exec,
            "Type into the terminal",
            &described,
        )
        .await?;
        if ctx.hub().control.user_control(&term_id) {
            return Err(USER_HAS_CONTROL.into());
        }
        let state = ctx.state();
        let (_, tap) = state.sessions.terminal_tap(&term_id).await?;
        tap.note_input(&bytes);
        state.sessions.term_write_agent(&term_id, &bytes).await?;
        settle(&tap).await;
        Ok(format!(
            "Sent. {}\n--- terminal, last 30 lines ---\n{}",
            status_line(ctx, &term_id, &tap),
            screen(&tap, 30)
        ))
    }
    .await;
    journal.finish(result)
}

/// Give the remote side a moment to react: wait for output, then for it to
/// pause briefly — bounded, since a streaming program never pauses.
async fn settle(tap: &TerminalTap) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    let version = tap.version();
    if tap.wait_change(version, Duration::from_millis(1500)).await {
        while tokio::time::Instant::now() < deadline {
            let version = tap.version();
            if !tap.wait_change(version, Duration::from_millis(300)).await {
                break;
            }
        }
    }
}

fn describe_input(text: Option<&str>, keys: &[String]) -> String {
    let mut parts = Vec::new();
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        parts.push(format!("{text:?}"));
    }
    parts.extend(keys.iter().map(|key| format!("[{key}]")));
    parts.join(" ")
}
