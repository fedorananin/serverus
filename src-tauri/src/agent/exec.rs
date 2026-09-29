//! Running agent commands in the user's own, visible shell.
//!
//! The command is typed into the terminal like the user would type it,
//! wrapped in completion markers (`serverus_domain::agent::shell`), and
//! followed through the terminal's output tap until the end marker carries
//! its exit status. Nothing here knows about Tauri or MCP, so integration
//! tests drive it against a real sshd.

mod follow;

use async_trait::async_trait;
use serverus_domain::agent::shell::{self, ShellFlavor};
use serverus_domain::agent::terminal_text::{self, Rendered};

use crate::error::{AppError, AppResult};
use crate::session::terminal_tap::{TapView, TerminalTap};

pub use follow::{follow, partial_output, CommandEnd, CommandOutcome};

/// Trailing output rendered to judge the terminal's state.
const STATE_WINDOW: u64 = 8 * 1024;

/// Where typed bytes go: the SSH channel of the terminal.
#[async_trait]
pub trait TerminalInput: Send + Sync {
    async fn send(&self, bytes: &[u8]) -> AppResult<()>;
}

/// What the terminal looks like right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalState {
    /// The last line looks like a shell prompt.
    Idle {
        prompt: String,
    },
    /// A full-screen program (vim, less, top…) owns the screen.
    FullScreen,
    /// A running program, a half-typed line, a password prompt…
    Busy {
        last_line: String,
    },
    Closed,
}

impl TerminalState {
    pub fn describe(&self) -> String {
        match self {
            TerminalState::Idle { prompt } => format!("idle at the prompt `{prompt}`"),
            TerminalState::FullScreen => {
                "a full-screen program (an editor, pager or monitor) is open".into()
            }
            TerminalState::Busy { last_line } if last_line.is_empty() => {
                "busy (a program is running)".into()
            }
            TerminalState::Busy { last_line } => {
                format!("busy — the last line is `{last_line}`, not a shell prompt")
            }
            TerminalState::Closed => "closed (the shell exited)".into(),
        }
    }
}

pub fn terminal_state(tap: &TerminalTap) -> TerminalState {
    // Rendering happens on a copy: the tap's lock also gates the reader
    // that feeds the user's terminal.
    state_of(&tap.snapshot().view())
}

pub(super) fn state_of(view: &TapView<'_>) -> TerminalState {
    if view.closed {
        return TerminalState::Closed;
    }
    if view.alt_screen {
        return TerminalState::FullScreen;
    }
    let recent = recent(view);
    if terminal_text::looks_like_prompt(&recent.cursor_line) {
        TerminalState::Idle {
            prompt: recent.cursor_line,
        }
    } else {
        TerminalState::Busy {
            last_line: last_line(&recent.text),
        }
    }
}

/// The rendered tail of the stream, starting at a line boundary so a cut
/// escape sequence or multi-byte character cannot garble it.
pub(super) fn recent(view: &TapView<'_>) -> Rendered {
    let window_start = view.index_of(view.end().saturating_sub(STATE_WINDOW));
    let cut = if window_start == 0 {
        0
    } else {
        // The first line boundary in the window, else the last one before
        // it (one very long line), else everything retained.
        view.bytes[window_start..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|index| window_start + index + 1)
            .or_else(|| {
                view.bytes[..window_start]
                    .iter()
                    .rposition(|&byte| byte == b'\n')
                    .map(|index| index + 1)
            })
            .unwrap_or(0)
    };
    let (bytes, alt) = view.from_offset(view.start + cut as u64);
    terminal_text::render_with_cursor(bytes, alt)
}

pub(super) fn last_line(text: &str) -> String {
    text.rsplit('\n')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

/// A command typed into a terminal, with what it takes to follow it.
#[derive(Debug, Clone)]
pub struct StartedCommand {
    pub command: String,
    pub nonce: String,
    /// Stream offset right before the command was typed.
    pub offset: u64,
    /// The prompt line the command was typed at.
    pub prompt: String,
    /// The tap's interrupt-key counter at typing time.
    pub interrupts: u64,
    /// The shell syntax the command was wrapped for.
    pub flavor: ShellFlavor,
}

/// Type `command` into the terminal. The caller has already checked that
/// the terminal is idle at `prompt`.
pub async fn start(
    tap: &TerminalTap,
    input: &dyn TerminalInput,
    flavor: ShellFlavor,
    command: &str,
    prompt: String,
) -> AppResult<StartedCommand> {
    let nonce = uuid::Uuid::new_v4().simple().to_string()[..10].to_string();
    let (offset, interrupts, bracketed_paste) =
        tap.with_view(|view| (view.end(), view.interrupts, view.bracketed_paste));
    let bytes = shell::wrap(flavor, &nonce, command, bracketed_paste)
        .map_err(|error| AppError::Other(error.to_string()))?;
    input.send(&bytes).await?;
    Ok(StartedCommand {
        command: command.to_string(),
        nonce,
        offset,
        prompt,
        interrupts,
        flavor,
    })
}
