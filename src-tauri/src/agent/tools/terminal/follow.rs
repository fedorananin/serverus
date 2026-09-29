//! The background task that follows an agent command until it ends — also
//! after `run_command` returned.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use crate::agent::exec::{self, CommandEnd, CommandOutcome, StartedCommand};
use crate::agent::hub::emit;
use crate::session::terminal_tap::TerminalTap;
use crate::state::AppState;

/// Register `started` as the terminal's running agent command and follow
/// it in the background. The receiver yields the outcome once known.
pub(crate) fn spawn(
    app: &AppHandle,
    term_id: &str,
    session_id: &str,
    tap: Arc<TerminalTap>,
    started: StartedCommand,
) -> watch::Receiver<Option<CommandOutcome>> {
    let hub = app.state::<AppState>().agent.clone();
    let (handles, event) = hub.control.begin(term_id, session_id, started.clone());
    emit(app, event);
    let receiver = handles.done.subscribe();

    let app = app.clone();
    let term_id = term_id.to_string();
    tokio::spawn(async move {
        let outcome = tokio::select! {
            outcome = exec::follow(&tap, started.clone()) => Some(outcome),
            // Replaced by a newer command or forgotten: stop following.
            _ = handles.abandoned => None,
        };
        let closed = matches!(
            outcome.as_ref().map(|outcome| &outcome.end),
            Some(CommandEnd::TerminalClosed)
        );
        if let Some(outcome) = outcome {
            let _ = handles.done.send(Some(outcome));
        }
        if let Some(event) = hub.control.finish(&term_id, &started.nonce) {
            emit(&app, event);
        }
        if closed {
            hub.control.forget(&term_id);
        }
    });
    receiver
}

/// How waiting for a followed command ended.
pub(crate) enum Waited {
    Finished(CommandOutcome),
    /// The wait timed out; the command is still being followed.
    StillRunning,
    /// Nobody follows it any more (a newer command replaced it, or its
    /// terminal was forgotten) — its outcome will never be known.
    Abandoned,
}

/// Wait up to `timeout` for the outcome.
pub(crate) async fn wait(
    mut done: watch::Receiver<Option<CommandOutcome>>,
    timeout: Duration,
) -> Waited {
    match tokio::time::timeout(timeout, done.wait_for(Option::is_some)).await {
        Ok(Ok(outcome)) => match outcome.clone() {
            Some(outcome) => Waited::Finished(outcome),
            None => Waited::Abandoned,
        },
        Ok(Err(_)) => Waited::Abandoned,
        Err(_) => Waited::StillRunning,
    }
}
