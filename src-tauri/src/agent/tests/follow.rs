//! The exec engine against a scripted shell: the test plays the remote side
//! by pushing what a shell would print into the terminal tap.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serverus_domain::agent::shell::ShellFlavor;

use crate::agent::exec::{self, follow, partial_output, CommandEnd, TerminalInput, TerminalState};
use crate::error::AppResult;
use crate::session::terminal_tap::TerminalTap;

#[derive(Default)]
pub(super) struct Typed(pub(super) Mutex<Vec<u8>>);

#[async_trait]
impl TerminalInput for Typed {
    async fn send(&self, bytes: &[u8]) -> AppResult<()> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }
}

pub(super) fn idle_tap() -> Arc<TerminalTap> {
    let tap = Arc::new(TerminalTap::default());
    tap.push(b"Last login: today\r\nuser@host:~$ ");
    tap
}

pub(super) fn start_marker(nonce: &str) -> Vec<u8> {
    format!("\x1b]6973;S;{nonce}\x07").into_bytes()
}

fn end_marker(nonce: &str, status: i32) -> Vec<u8> {
    format!("\x1b]6973;E;{nonce};{status}\x07").into_bytes()
}

#[test]
fn terminal_state_reads_the_prompt() {
    let tap = idle_tap();
    assert_eq!(
        exec::terminal_state(&tap),
        TerminalState::Idle {
            prompt: "user@host:~$".into()
        }
    );
    tap.push(b"make\r\n");
    assert!(matches!(
        exec::terminal_state(&tap),
        TerminalState::Busy { .. }
    ));
    tap.push(b"\x1b[?1049h\x1b[H~\r\n~");
    assert_eq!(exec::terminal_state(&tap), TerminalState::FullScreen);
    tap.close();
    assert_eq!(exec::terminal_state(&tap), TerminalState::Closed);
}

#[tokio::test]
async fn a_command_ends_with_its_status_and_clean_output() {
    let tap = idle_tap();
    let typed = Typed::default();
    let started = exec::start(
        &tap,
        &typed,
        ShellFlavor::Posix,
        "ls -1",
        "user@host:~$".into(),
    )
    .await
    .unwrap();
    let typed_line = String::from_utf8(typed.0.lock().unwrap().clone()).unwrap();
    assert!(typed_line.contains(&started.nonce));
    assert!(typed_line.ends_with('\r'));

    let follower = tokio::spawn({
        let tap = tap.clone();
        let started = started.clone();
        async move { follow(&tap, started).await }
    });
    // Echo of the typed line (it mentions the nonce, but never as a marker).
    tap.push(typed_line.replace('\r', "\r\n").as_bytes());
    tap.push(&start_marker(&started.nonce));
    tap.push(b"a.txt\r\n\x1b[01;34mbin\x1b[0m\r\n");
    // The end marker split across two chunks.
    let end = end_marker(&started.nonce, 2);
    tap.push(&end[..9]);
    tap.push(&end[9..]);
    tap.push(b"user@host:~$ ");

    let outcome = tokio::time::timeout(Duration::from_secs(5), follower)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome.end, CommandEnd::Exited(Some(2)));
    assert_eq!(outcome.output, "a.txt\nbin");
    assert!(!outcome.head_dropped);
}

#[tokio::test]
async fn bracketed_paste_is_used_when_the_shell_enabled_it() {
    let tap = Arc::new(TerminalTap::default());
    tap.push(b"\x1b[?2004huser@host:~$ ");
    let typed = Typed::default();
    exec::start(
        &tap,
        &typed,
        ShellFlavor::Posix,
        "echo hi",
        "user@host:~$".into(),
    )
    .await
    .unwrap();
    assert!(typed.0.lock().unwrap().starts_with(b"\x1b[200~"));
}

#[tokio::test]
async fn partial_output_of_a_running_command() {
    let tap = idle_tap();
    let started = exec::start(
        &tap,
        &Typed::default(),
        ShellFlavor::Posix,
        "build",
        "$".into(),
    )
    .await
    .unwrap();
    tap.push(b"echo\r\n");
    tap.push(&start_marker(&started.nonce));
    tap.push(b"step 1\r\nstep 2\r\n");
    let (output, head_dropped) = partial_output(&tap, &started);
    assert_eq!(output, "step 1\nstep 2");
    assert!(!head_dropped);
}

#[tokio::test]
async fn a_closed_terminal_ends_the_command() {
    let tap = idle_tap();
    let started = exec::start(
        &tap,
        &Typed::default(),
        ShellFlavor::Posix,
        "exit",
        "$".into(),
    )
    .await
    .unwrap();
    let follower = tokio::spawn({
        let tap = tap.clone();
        async move { follow(&tap, started).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    tap.close();
    let outcome = tokio::time::timeout(Duration::from_secs(5), follower)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome.end, CommandEnd::TerminalClosed);
}

#[tokio::test]
async fn ctrl_c_back_to_the_same_prompt_is_an_interrupt() {
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
    let follower = tokio::spawn({
        let tap = tap.clone();
        let started = started.clone();
        async move { follow(&tap, started).await }
    });
    tap.push(b"sleep 100\r\n");
    tap.push(&start_marker(&started.nonce));
    tap.push(b"working\r\n");
    // The user presses Ctrl-C; the shell drops the rest of the line.
    tap.note_input(b"\x03");
    tap.push(b"^C\r\nuser@host:~$ ");
    let outcome = tokio::time::timeout(Duration::from_secs(5), follower)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome.end, CommandEnd::Interrupted);
    assert_eq!(outcome.output, "working\n^C");
}

#[tokio::test]
async fn a_prompt_like_line_without_input_is_not_an_interrupt() {
    let tap = idle_tap();
    let started = exec::start(
        &tap,
        &Typed::default(),
        ShellFlavor::Posix,
        "cat p",
        "user@host:~$".into(),
    )
    .await
    .unwrap();
    tap.push(&start_marker(&started.nonce));
    tap.push(b"user@host:~$ ");
    let outcome = tokio::time::timeout(Duration::from_millis(1500), follow(&tap, started)).await;
    assert!(
        outcome.is_err(),
        "no input happened, so the command is still running"
    );
}
