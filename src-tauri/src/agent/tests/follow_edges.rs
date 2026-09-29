//! Edge cases of following a command: prompts that change, input that is
//! not an interrupt, wrappers the shell rejects, prompts that are not an
//! idle shell.

use std::sync::Arc;
use std::time::Duration;

use serverus_domain::agent::shell::ShellFlavor;

use super::follow::{idle_tap, start_marker, Typed};
use crate::agent::exec::{self, follow, CommandEnd, CommandOutcome, StartedCommand, TerminalState};
use crate::session::terminal_tap::TerminalTap;

fn spawn_follow(
    tap: &Arc<TerminalTap>,
    started: &StartedCommand,
) -> tokio::task::JoinHandle<CommandOutcome> {
    let (tap, started) = (tap.clone(), started.clone());
    tokio::spawn(async move { follow(&tap, started).await })
}

#[tokio::test]
async fn an_interrupt_is_recognised_when_the_prompt_changed() {
    // Prompts often show the last exit status or the time.
    let tap = idle_tap();
    let started = exec::start(
        &tap,
        &Typed::default(),
        ShellFlavor::Posix,
        "sleep 100",
        "user@host:~$".into(),
    )
    .await
    .unwrap();
    let follower = spawn_follow(&tap, &started);
    tap.push(b"sleep 100\r\n");
    tap.push(&start_marker(&started.nonce));
    tap.note_input(b"\x03");
    tap.push(b"^C\r\n[130] user@host:~$ ");
    let outcome = tokio::time::timeout(Duration::from_secs(5), follower)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome.end, CommandEnd::Interrupted);
    assert_eq!(outcome.output, "^C");
}

#[tokio::test]
async fn ordinary_input_and_a_prompt_like_line_are_not_an_interrupt() {
    // The user answers a question; the program prints something ending in
    // a prompt character and pauses. It is still running.
    let tap = idle_tap();
    let started = exec::start(
        &tap,
        &Typed::default(),
        ShellFlavor::Posix,
        "apt upgrade",
        "user@host:~$".into(),
    )
    .await
    .unwrap();
    tap.push(&start_marker(&started.nonce));
    tap.push(b"Continue? [Y/n] ");
    tap.note_input(b"y\r");
    tap.push(b"y\r\nUnpacking 45%");
    let outcome = tokio::time::timeout(Duration::from_millis(1500), follow(&tap, started)).await;
    assert!(outcome.is_err(), "no interrupt key was typed");
}

#[tokio::test]
async fn a_rejected_wrapper_is_reported_as_not_started() {
    // fish refuses `$?` at parse time: nothing runs, the prompt comes back.
    let tap = idle_tap();
    let typed = Typed::default();
    let started = exec::start(
        &tap,
        &typed,
        ShellFlavor::Posix,
        "ls",
        "user@host:~$".into(),
    )
    .await
    .unwrap();
    let follower = spawn_follow(&tap, &started);
    let echo = String::from_utf8(typed.0.lock().unwrap().clone()).unwrap();
    tap.push(echo.trim_end_matches('\r').as_bytes());
    tap.push(b"\r\nfish: $? is not the exit status. In fish, please use $status.\r\nuser@host ~> ");
    let outcome = tokio::time::timeout(Duration::from_secs(5), follower)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome.end, CommandEnd::NotStarted);
    assert_eq!(
        outcome.output,
        "fish: $? is not the exit status. In fish, please use $status."
    );
}

#[tokio::test]
async fn a_slow_echo_is_not_mistaken_for_a_rejection() {
    // Nothing came back yet: the prompt on screen is the old one.
    let tap = idle_tap();
    let started = exec::start(
        &tap,
        &Typed::default(),
        ShellFlavor::Posix,
        "ls",
        "user@host:~$".into(),
    )
    .await
    .unwrap();
    let outcome = tokio::time::timeout(Duration::from_millis(1500), follow(&tap, started)).await;
    assert!(outcome.is_err(), "the typed line was not even echoed yet");
}

#[test]
fn a_right_prompt_does_not_hide_an_idle_shell() {
    let tap = Arc::new(TerminalTap::default());
    tap.push(b"user@host ~ %\x1b[40C\x1b[32m12:04:55\x1b[0m\r\x1b[14C");
    assert_eq!(
        exec::terminal_state(&tap),
        TerminalState::Idle {
            prompt: "user@host ~ %".into()
        }
    );
}

#[test]
fn a_repl_prompt_is_not_an_idle_shell() {
    let tap = Arc::new(TerminalTap::default());
    tap.push(b"$ python3\r\nPython 3.12\r\n>>> ");
    assert!(matches!(
        exec::terminal_state(&tap),
        TerminalState::Busy { .. }
    ));
}
