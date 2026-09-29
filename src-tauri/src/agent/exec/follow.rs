//! Following a typed command through the terminal stream until it ends.

use std::time::Duration;

use serverus_domain::agent::shell;
use serverus_domain::agent::terminal_modes::{find, parse_exit_status};
use serverus_domain::agent::terminal_text;
use tokio::time::Instant;

use super::{last_line, recent, StartedCommand};
use crate::session::terminal_tap::{TapView, TerminalTap};

/// How long the stream must stay quiet before a command whose prompt came
/// back without an end marker (interrupted, or never run) is declared over.
const SETTLE: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEnd {
    /// The end marker arrived; the status is `None` if it was unreadable.
    Exited(Option<i32>),
    /// Someone pressed Ctrl-C (or similar) and the shell dropped the rest
    /// of the line, end marker included.
    Interrupted,
    /// The shell rejected the typed line before running any of it (no
    /// start marker, and it is back at a prompt) — typically because the
    /// shell in the terminal is not the one the wrapper was written for.
    NotStarted,
    /// The terminal closed before the command finished.
    TerminalClosed,
}

#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub end: CommandEnd,
    /// The command's output, rendered as text.
    pub output: String,
    /// The start of the output fell out of the terminal's retention window.
    pub head_dropped: bool,
}

/// Search state for one command's markers.
struct Follower {
    started: StartedCommand,
    start_marker: Vec<u8>,
    end_prefix: Vec<u8>,
    /// Absolute offset right after the start marker, once seen.
    output_from: Option<u64>,
    /// Where the next start / end marker search begins.
    start_scan: u64,
    end_scan: u64,
}

impl Follower {
    fn new(started: StartedCommand) -> Self {
        Self {
            start_marker: shell::start_marker(&started.nonce),
            end_prefix: shell::end_marker_prefix(&started.nonce),
            output_from: None,
            start_scan: started.offset,
            end_scan: started.offset,
            started,
        }
    }

    fn check(&mut self, view: &TapView<'_>, settled: bool) -> Option<CommandOutcome> {
        if self.output_from.is_none() {
            let from = view.index_of(self.start_scan);
            match find(view.bytes, &self.start_marker, from) {
                Some(at) => {
                    let after = view.start + (at + self.start_marker.len()) as u64;
                    self.output_from = Some(after);
                    self.end_scan = self.end_scan.max(after);
                }
                None => self.start_scan = rescan_point(view, self.start_marker.len()),
            }
        }

        let from = view.index_of(self.end_scan);
        if let Some(at) = find(view.bytes, &self.end_prefix, from) {
            let status_bytes = &view.bytes[at + self.end_prefix.len()..];
            // An incomplete status waits for the next chunk.
            let (status, _) = parse_exit_status(status_bytes)?;
            let end_offset = view.start + at as u64;
            return Some(self.outcome(view, CommandEnd::Exited(status), end_offset));
        }
        self.end_scan = rescan_point(view, self.end_prefix.len());

        if view.closed {
            return Some(self.outcome(view, CommandEnd::TerminalClosed, view.end()));
        }
        if !settled || view.alt_screen {
            return None;
        }
        // An interrupt kills the whole typed line, so no end marker comes.
        // Recognise it by cause and effect: an interrupt key was typed after
        // the command started, the shell is back at a prompt, and the
        // stream went quiet. (The new prompt may differ from the old one —
        // many show the last exit status or the time.)
        if view.interrupts > self.started.interrupts && self.back_at_prompt(view) {
            return Some(self.outcome(view, CommandEnd::Interrupted, view.end()));
        }
        // No start marker, yet the typed line was submitted (a newline went
        // by) and a prompt came back: the shell refused the line.
        if self.output_from.is_none() && self.submitted(view) && self.back_at_prompt(view) {
            return Some(self.outcome(view, CommandEnd::NotStarted, view.end()));
        }
        None
    }

    fn back_at_prompt(&self, view: &TapView<'_>) -> bool {
        terminal_text::looks_like_prompt(&recent(view).cursor_line)
    }

    fn submitted(&self, view: &TapView<'_>) -> bool {
        view.bytes[view.index_of(self.started.offset)..].contains(&b'\n')
    }

    fn output(&self, view: &TapView<'_>, until: u64, end: Option<&CommandEnd>) -> (String, bool) {
        let from = self.output_from.unwrap_or(self.started.offset);
        let head_dropped = from < view.start;
        let (bytes, alt) = view.from_offset(from);
        let len = until.saturating_sub(from.max(view.start)) as usize;
        let mut text = terminal_text::render(&bytes[..len.min(bytes.len())], alt);
        if matches!(end, Some(CommandEnd::NotStarted)) {
            // Without a start marker the text begins with the echo of the
            // typed wrapper; what matters is what the shell said after it.
            let nonce_line = text
                .split('\n')
                .enumerate()
                .filter(|(_, line)| line.contains(&self.started.nonce))
                .map(|(index, _)| index)
                .last();
            if let Some(index) = nonce_line {
                text = text
                    .split('\n')
                    .skip(index + 1)
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        // Without an end marker the tail is the prompt the shell came back
        // to; it is not output.
        let prompt_follows = matches!(end, Some(CommandEnd::Interrupted | CommandEnd::NotStarted));
        if prompt_follows && terminal_text::looks_like_prompt(&last_line(&text)) {
            text.truncate(text.rfind('\n').unwrap_or(0));
        }
        (text, head_dropped)
    }

    fn outcome(&self, view: &TapView<'_>, end: CommandEnd, until: u64) -> CommandOutcome {
        let (output, head_dropped) = self.output(view, until, Some(&end));
        CommandOutcome {
            end,
            output,
            head_dropped,
        }
    }
}

/// Resume a marker search where a marker split across chunks could start.
fn rescan_point(view: &TapView<'_>, marker_len: usize) -> u64 {
    view.end().saturating_sub(marker_len as u64)
}

/// Wait until the command ends. Drop the future to stop following.
pub async fn follow(tap: &TerminalTap, started: StartedCommand) -> CommandOutcome {
    let mut follower = Follower::new(started);
    let mut last_change = Instant::now();
    loop {
        let version = tap.version();
        let settled = last_change.elapsed() >= SETTLE;
        if let Some(outcome) = tap.with_view(|view| follower.check(view, settled)) {
            return outcome;
        }
        if tap.wait_change(version, SETTLE).await {
            last_change = Instant::now();
        }
    }
}

/// The output a still-running command has produced so far.
pub fn partial_output(tap: &TerminalTap, started: &StartedCommand) -> (String, bool) {
    let mut follower = Follower::new(started.clone());
    tap.with_view(|view| {
        follower.check(view, false);
        follower.output(view, view.end(), None)
    })
}
