//! The agent's shared-shell command engine against a real sshd and the real
//! login shell (zsh on macOS, bash on Linux CI): commands are typed into an
//! interactive PTY shell and followed through the completion markers.
#![cfg(unix)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serverus_domain::agent::shell::ShellFlavor;
use serverus_lib::agent::exec::{
    self, follow, CommandEnd, CommandOutcome, TerminalInput, TerminalState,
};
use serverus_lib::error::{AppError, AppResult};
use serverus_lib::session::ssh::{connect_chain, ConnectOutcome};
use serverus_lib::session::terminal_tap::TerminalTap;
use support::TestSshd;

struct ChannelInput(russh::ChannelWriteHalf<russh::client::Msg>);

#[async_trait]
impl TerminalInput for ChannelInput {
    async fn send(&self, bytes: &[u8]) -> AppResult<()> {
        self.0
            .data(bytes)
            .await
            .map_err(|error| AppError::Other(error.to_string()))
    }
}

struct Shell {
    _sshd: TestSshd,
    tap: Arc<TerminalTap>,
    input: ChannelInput,
}

async fn open_shell() -> Shell {
    let sshd = TestSshd::spawn();
    let issue = match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        _ => panic!("expected a host key prompt"),
    };
    let handle = match connect_chain(&[sshd.hop(Some(issue.key_line))])
        .await
        .unwrap()
    {
        ConnectOutcome::Connected(handle) => handle,
        _ => panic!("expected a connection"),
    };
    let channel = handle.channel_open_session().await.unwrap();
    channel
        .request_pty(true, "xterm-256color", 120, 40, 0, 0, &[])
        .await
        .unwrap();
    channel.request_shell(true).await.unwrap();
    let (mut read, write) = channel.split();
    let tap = Arc::new(TerminalTap::default());
    let reader_tap = tap.clone();
    tokio::spawn(async move {
        // Keep the SSH handle alive for as long as the channel is read.
        let _handle = handle;
        while let Some(message) = read.wait().await {
            match message {
                russh::ChannelMsg::Data { data } | russh::ChannelMsg::ExtendedData { data, .. } => {
                    reader_tap.push(&data)
                }
                russh::ChannelMsg::Eof | russh::ChannelMsg::Close => break,
                _ => {}
            }
        }
        reader_tap.close();
    });
    Shell {
        _sshd: sshd,
        tap,
        input: ChannelInput(write),
    }
}

impl Shell {
    async fn wait_idle(&self) -> String {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            if let TerminalState::Idle { prompt } = exec::terminal_state(&self.tap) {
                // Let the prompt finish drawing before typing.
                tokio::time::sleep(Duration::from_millis(150)).await;
                return prompt;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the shell never became idle: {:?}",
                exec::terminal_state(&self.tap)
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn run(&self, command: &str) -> CommandOutcome {
        let prompt = self.wait_idle().await;
        let started = exec::start(&self.tap, &self.input, ShellFlavor::Posix, command, prompt)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(30), follow(&self.tap, started))
            .await
            .unwrap_or_else(|_| panic!("`{command}` did not finish; terminal: {:?}", self.screen()))
    }

    fn screen(&self) -> String {
        self.tap.with_view(|view| {
            let (bytes, alt) = view.from_offset(view.start);
            serverus_domain::agent::terminal_text::render(bytes, alt)
        })
    }
}

fn assert_exit(outcome: &CommandOutcome, code: i32, output: &str) {
    assert_eq!(
        outcome.end,
        CommandEnd::Exited(Some(code)),
        "output: {:?}",
        outcome.output
    );
    assert_eq!(outcome.output, output);
}

#[tokio::test]
async fn commands_share_the_shell_and_report_status_and_output() {
    let shell = open_shell().await;
    assert_exit(&shell.run("echo hello").await, 0, "hello");
    assert_exit(&shell.run("echo oops 1>&2; (exit 3)").await, 3, "oops");
    // State persists between commands: it is the same shell.
    assert_exit(&shell.run("cd / && export SERVERUS_T=42").await, 0, "");
    assert_exit(&shell.run("pwd; echo $SERVERUS_T").await, 0, "/\n42");
    // A `#` must not swallow the end marker. (Interactive zsh does not
    // treat it as a comment unless INTERACTIVE_COMMENTS is set, so the
    // text may be echoed.)
    let outcome = shell.run("echo kept # a comment").await;
    assert_eq!(outcome.end, CommandEnd::Exited(Some(0)));
    assert!(outcome.output.starts_with("kept"), "{:?}", outcome.output);
    // Multi-line scripts run in a group.
    assert_exit(
        &shell.run("cat <<'EOF'\nline one\nline two\nEOF").await,
        0,
        "line one\nline two",
    );
}

#[tokio::test]
async fn long_output_is_followed_to_the_end() {
    let shell = open_shell().await;
    let outcome = shell
        .run("i=0; while [ $i -lt 5000 ]; do i=$((i+1)); echo \"row $i\"; done")
        .await;
    assert_eq!(outcome.end, CommandEnd::Exited(Some(0)));
    assert!(
        outcome.output.starts_with("row 1\n"),
        "{:?}",
        &outcome.output[..40]
    );
    assert!(outcome.output.ends_with("row 5000"));
}

#[tokio::test]
async fn ctrl_c_is_recognised_as_an_interrupt() {
    let shell = open_shell().await;
    let prompt = shell.wait_idle().await;
    let started = exec::start(
        &shell.tap,
        &shell.input,
        ShellFlavor::Posix,
        "sleep 30",
        prompt,
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    shell.tap.note_input(b"\x03");
    shell.input.send(b"\x03").await.unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(10), follow(&shell.tap, started))
        .await
        .unwrap_or_else(|_| panic!("interrupt not detected; terminal: {:?}", shell.screen()));
    assert_eq!(outcome.end, CommandEnd::Interrupted);
    // The shell is usable again.
    assert_exit(&shell.run("echo again").await, 0, "again");
}

#[tokio::test]
async fn a_nested_shell_without_bracketed_paste_works_too() {
    let shell = open_shell().await;
    shell.wait_idle().await;
    // Typed by "the user": a plain POSIX shell with a `$ ` prompt and no
    // line-editor niceties.
    shell.input.send(b"PS1='nested$ ' exec sh\r").await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !shell.screen().ends_with("nested$") {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{:?}",
            shell.screen()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_exit(&shell.run("echo inner").await, 0, "inner");
    assert_exit(&shell.run("printf 'a\\nb\\n'\necho c").await, 0, "a\nb\nc");
}
