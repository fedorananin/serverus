//! Who drives each terminal: the user can take a terminal away from the
//! agent and hand it back, and at most one agent command runs per terminal.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serverus_domain::agent::shell::ShellFlavor;
use tokio::sync::{oneshot, watch};

use super::exec::{CommandOutcome, StartedCommand};
use super::types::AgentTerminalEvent;

/// An agent command still being followed.
struct Running {
    started: StartedCommand,
    done: watch::Receiver<Option<CommandOutcome>>,
    /// Dropping or firing this stops the follower (the command itself keeps
    /// running in the shell — it is the user's terminal).
    _abandon: oneshot::Sender<()>,
}

#[derive(Default)]
struct TermControl {
    session_id: String,
    user_control: bool,
    running: Option<Running>,
    /// Held from "is the terminal free?" to "the command is registered", so
    /// two concurrent calls cannot both type into the same shell.
    typing: Arc<tokio::sync::Mutex<()>>,
    /// Shell syntax the agent said this terminal speaks (it may differ from
    /// the login shell the session probed).
    shell: Option<ShellFlavor>,
}

/// Handles the follower task needs.
pub struct RunningHandles {
    pub done: watch::Sender<Option<CommandOutcome>>,
    pub abandoned: oneshot::Receiver<()>,
}

/// A snapshot of the command an agent is running in a terminal.
pub struct RunningCommand {
    pub started: StartedCommand,
    pub done: watch::Receiver<Option<CommandOutcome>>,
}

#[derive(Default)]
pub struct ControlRegistry {
    terms: Mutex<HashMap<String, TermControl>>,
}

fn event(term_id: &str, control: &TermControl) -> AgentTerminalEvent {
    AgentTerminalEvent {
        term_id: term_id.to_string(),
        session_id: control.session_id.clone(),
        running: control
            .running
            .as_ref()
            .map(|running| running.started.command.clone()),
        user_control: control.user_control,
    }
}

impl ControlRegistry {
    pub fn user_control(&self, term_id: &str) -> bool {
        self.terms
            .lock()
            .unwrap()
            .get(term_id)
            .is_some_and(|control| control.user_control)
    }

    pub fn set_user_control(
        &self,
        term_id: &str,
        session_id: &str,
        on: bool,
    ) -> AgentTerminalEvent {
        let mut terms = self.terms.lock().unwrap();
        let control = terms.entry(term_id.to_string()).or_default();
        control.session_id = session_id.to_string();
        control.user_control = on;
        event(term_id, control)
    }

    /// The lock that serialises typing agent commands into `term_id`.
    pub fn typing_lock(&self, term_id: &str, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut terms = self.terms.lock().unwrap();
        let control = terms.entry(term_id.to_string()).or_default();
        control.session_id = session_id.to_string();
        control.typing.clone()
    }

    pub fn shell(&self, term_id: &str) -> Option<ShellFlavor> {
        self.terms.lock().unwrap().get(term_id)?.shell
    }

    pub fn set_shell(&self, term_id: &str, session_id: &str, flavor: ShellFlavor) {
        let mut terms = self.terms.lock().unwrap();
        let control = terms.entry(term_id.to_string()).or_default();
        control.session_id = session_id.to_string();
        control.shell = Some(flavor);
    }

    pub fn running(&self, term_id: &str) -> Option<RunningCommand> {
        let terms = self.terms.lock().unwrap();
        let running = terms.get(term_id)?.running.as_ref()?;
        Some(RunningCommand {
            started: running.started.clone(),
            done: running.done.clone(),
        })
    }

    /// Register a freshly typed command. A previous one still being followed
    /// is abandoned (the caller checked that the agent may replace it).
    pub fn begin(
        &self,
        term_id: &str,
        session_id: &str,
        started: StartedCommand,
    ) -> (RunningHandles, AgentTerminalEvent) {
        let (done_sender, done_receiver) = watch::channel(None);
        let (abandon_sender, abandon_receiver) = oneshot::channel();
        let mut terms = self.terms.lock().unwrap();
        let control = terms.entry(term_id.to_string()).or_default();
        control.session_id = session_id.to_string();
        control.running = Some(Running {
            started,
            done: done_receiver,
            _abandon: abandon_sender,
        });
        (
            RunningHandles {
                done: done_sender,
                abandoned: abandon_receiver,
            },
            event(term_id, control),
        )
    }

    /// The command identified by `nonce` ended (or stopped being followed).
    /// Returns the updated state unless a newer command replaced it.
    pub fn finish(&self, term_id: &str, nonce: &str) -> Option<AgentTerminalEvent> {
        let mut terms = self.terms.lock().unwrap();
        let control = terms.get_mut(term_id)?;
        let current = control.running.as_ref()?;
        if current.started.nonce != nonce {
            return None;
        }
        control.running = None;
        Some(event(term_id, control))
    }

    /// Stop following the agent's command in `term_id`, if any.
    pub fn abandon(&self, term_id: &str) -> Option<AgentTerminalEvent> {
        let mut terms = self.terms.lock().unwrap();
        let control = terms.get_mut(term_id)?;
        control.running.take()?;
        Some(event(term_id, control))
    }

    /// Forget a terminal that closed.
    pub fn forget(&self, term_id: &str) {
        self.terms.lock().unwrap().remove(term_id);
    }

    /// Forget every terminal of a closed session (their followers stop).
    pub fn forget_session(&self, session_id: &str) {
        self.terms
            .lock()
            .unwrap()
            .retain(|_, control| control.session_id != session_id);
    }

    /// Every terminal with agent state, for a UI that (re)starts.
    pub fn snapshot(&self) -> Vec<AgentTerminalEvent> {
        self.terms
            .lock()
            .unwrap()
            .iter()
            .map(|(term_id, control)| event(term_id, control))
            .collect()
    }
}
