//! `run_command`: typing a command into the server's visible terminal.

mod follow;
mod read;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use serverus_domain::agent::access::OperationClass;
use serverus_domain::agent::shell::ShellFlavor;
use serverus_domain::agent::terminal_text::truncate_middle;

use super::args::parse;
use super::context::TabRef;
use super::journal::Journal;
use super::{Ctx, ToolError, ToolResult};
use crate::agent::exec::{self, CommandEnd, CommandOutcome, TerminalInput, TerminalState};
use crate::agent::hub::emit;
use crate::error::AppResult;
use crate::session::SessionManager;
use follow::Waited;

pub use read::{read_terminal, send_input};

/// Longest output handed back to the model.
pub(super) const MAX_OUTPUT_CHARS: usize = 30_000;

pub(super) const USER_HAS_CONTROL: &str = "The user has taken control of this terminal in Serverus. Ask them to hand it back (the \"Hand back\" button above the terminal) and then call read_terminal to see what they did.";

/// Typed input goes straight to the terminal's SSH channel.
pub(crate) struct SessionInput {
    pub sessions: Arc<SessionManager>,
    pub term_id: String,
}

#[async_trait]
impl TerminalInput for SessionInput {
    async fn send(&self, bytes: &[u8]) -> AppResult<()> {
        self.sessions.term_write_agent(&self.term_id, bytes).await
    }
}

#[derive(Deserialize)]
struct RunArgs {
    server: String,
    command: String,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
    #[serde(default)]
    force: bool,
    /// Syntax of the shell running in the terminal, when it is not the
    /// login shell. Remembered for the terminal.
    #[serde(default)]
    shell: Option<ShellArg>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ShellArg {
    Posix,
    Fish,
    Csh,
}

impl From<ShellArg> for ShellFlavor {
    fn from(shell: ShellArg) -> Self {
        match shell {
            ShellArg::Posix => ShellFlavor::Posix,
            ShellArg::Fish => ShellFlavor::Fish,
            ShellArg::Csh => ShellFlavor::Csh,
        }
    }
}

fn flavor_name(flavor: ShellFlavor) -> &'static str {
    match flavor {
        ShellFlavor::Posix => "posix",
        ShellFlavor::Fish => "fish",
        ShellFlavor::Csh => "csh",
    }
}

fn default_timeout() -> u64 {
    60
}

pub async fn run_command(ctx: &Ctx<'_>, arguments: Value) -> ToolResult {
    let args: RunArgs = parse(arguments)?;
    let policy = ctx.policy().await?;
    let server = ctx.target(&policy, &args.server).await?;
    ctx.precheck(&server, OperationClass::Exec)?;
    let tab = ctx.tab(&server, true).await?;
    let journal = Journal::start(
        ctx.app,
        "run_command",
        &args.command,
        Some(&tab.session_id),
        &server.id,
    );
    let result = async {
        ctx.authorize(
            &policy,
            &server,
            OperationClass::Exec,
            "Run a command",
            &args.command,
        )
        .await?;
        run_in_tab(ctx, &tab, &args).await
    }
    .await;
    journal.finish(result)
}

async fn run_in_tab(ctx: &Ctx<'_>, tab: &TabRef, args: &RunArgs) -> ToolResult {
    let state = ctx.state();
    let hub = ctx.hub();
    let term_id = tab.term_id.clone().ok_or("The tab has no terminal.")?;
    // One caller at a time from "is the terminal free?" until the command
    // is registered as running: concurrent calls must not both type.
    let typing = hub.control.typing_lock(&term_id, &tab.session_id);
    let typing = typing.lock().await;
    if hub.control.user_control(&term_id) {
        return Err(USER_HAS_CONTROL.into());
    }
    let (_, tap) = state.sessions.terminal_tap(&term_id).await?;

    if let Some(previous) = hub.control.running(&term_id) {
        if !args.force {
            return Err(ToolError(format!(
                "Your previous command `{}` is still running in this terminal. Wait for it with read_terminal(wait_seconds), stop it with send_input(keys=[\"ctrl-c\"]), or pass force=true to type anyway.",
                previous.started.command
            )));
        }
        if let Some(event) = hub.control.abandon(&term_id) {
            emit(ctx.app, event);
        }
    }

    let prompt = match exec::terminal_state(&tap) {
        TerminalState::Idle { prompt } => prompt,
        TerminalState::Busy { last_line } if args.force => last_line,
        TerminalState::FullScreen if args.force => String::new(),
        other => {
            return Err(ToolError(format!(
                "The terminal is {}. Look with read_terminal; finish or interrupt what is running (send_input), or pass force=true to type anyway.",
                other.describe()
            )))
        }
    };

    if let Some(shell) = args.shell {
        hub.control
            .set_shell(&term_id, &tab.session_id, shell.into());
    }
    let flavor = match hub.control.shell(&term_id) {
        Some(flavor) => flavor,
        None => shell_flavor(ctx, tab).await,
    };
    let input = SessionInput {
        sessions: state.sessions.clone(),
        term_id: term_id.clone(),
    };
    let started = exec::start(&tap, &input, flavor, &args.command, prompt).await?;
    let done = follow::spawn(
        ctx.app,
        &term_id,
        &tab.session_id,
        tap.clone(),
        started.clone(),
    );
    drop(typing);

    let timeout = Duration::from_secs(args.timeout_seconds.clamp(1, 3600));
    match follow::wait(done, timeout).await {
        Waited::Finished(outcome) => Ok(describe_outcome(&outcome, flavor)),
        Waited::StillRunning => {
            let (output, head_dropped) = exec::partial_output(&tap, &started);
            Ok(format!(
                "The command is still running after {}s (it keeps running in the user's terminal). Wait for it with read_terminal(wait_seconds=…), or interrupt it with send_input(keys=[\"ctrl-c\"]).\nOutput so far:\n{}",
                timeout.as_secs(),
                clip_output(&output, head_dropped)
            ))
        }
        Waited::Abandoned => Ok(ABANDONED.into()),
    }
}

/// Why a followed command's outcome will never be known.
pub(super) const ABANDONED: &str = "Serverus stopped following this command before it finished: a newer command replaced it (force=true) or the terminal went away. It may still be running — look with read_terminal.";

/// Learn (once per session) whether the login shell is POSIX, fish or csh.
async fn shell_flavor(ctx: &Ctx<'_>, tab: &TabRef) -> ShellFlavor {
    let hub = ctx.hub();
    if let Some(flavor) = hub.shell_of(&tab.session_id) {
        return flavor;
    }
    let Some(ssh) = tab.entry.ssh.clone() else {
        return ShellFlavor::Posix;
    };
    let flavor = match ssh
        .exec_capture("printf '%s' \"$SHELL\"", 512, Duration::from_secs(10))
        .await
    {
        Ok(capture) => ShellFlavor::from_shell_path(&capture.output),
        Err(_) => ShellFlavor::Posix,
    };
    hub.remember_shell(&tab.session_id, flavor);
    flavor
}

pub(super) fn clip_output(output: &str, head_dropped: bool) -> String {
    let (text, _) = truncate_middle(output, MAX_OUTPUT_CHARS);
    let text = if text.trim().is_empty() {
        "(no output)".to_string()
    } else {
        text
    };
    if head_dropped {
        format!("… [earlier output is no longer retained] …\n{text}")
    } else {
        text
    }
}

pub(super) fn describe_outcome(outcome: &CommandOutcome, flavor: ShellFlavor) -> String {
    let status = match outcome.end {
        CommandEnd::Exited(Some(code)) => format!("exit status: {code}"),
        CommandEnd::Exited(None) => "finished (exit status unreadable)".into(),
        CommandEnd::Interrupted => "interrupted before it finished (Ctrl-C)".into(),
        CommandEnd::TerminalClosed => "the terminal closed before the command finished".into(),
        CommandEnd::NotStarted => format!(
            "not run: the shell rejected the line Serverus typed around your command (written for a {} shell). The shell in this terminal is probably a different one — for example the user started fish or csh from the login shell. Retry with shell=\"posix\", \"fish\" or \"csh\" (remembered for this terminal). The shell printed:",
            flavor_name(flavor)
        ),
    };
    format!(
        "{status}\n{}",
        clip_output(&outcome.output, outcome.head_dropped)
    )
}
