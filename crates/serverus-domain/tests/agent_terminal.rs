use serverus_domain::agent::terminal_modes::{find, parse_exit_status, ModeTracker};
use serverus_domain::agent::terminal_text::{
    looks_like_prompt, render, render_with_cursor, tail_lines, truncate_middle,
};

#[test]
fn render_strips_colours_and_titles() {
    let raw = b"\x1b]0;user@host: ~\x07\x1b[1;32mgreen\x1b[0m plain\r\n";
    assert_eq!(render(raw, false), "green plain");
}

#[test]
fn render_collapses_carriage_return_progress() {
    let raw = b"downloading 10%\rdownloading 55%\rdownloading 100%\r\ndone\r\n";
    assert_eq!(render(raw, false), "downloading 100%\ndone");
}

#[test]
fn render_applies_backspace_and_erase() {
    // Typed "lsx", backspace, erase to end of line, Enter.
    assert_eq!(render(b"$ lsx\x08\x1b[K\r\n", false), "$ ls");
    // Delete-character sequence used by line editors.
    assert_eq!(render(b"abcd\x1b[3D\x1b[1P\r\n", false), "acd");
}

#[test]
fn render_keeps_the_unterminated_last_line() {
    assert_eq!(render(b"out\r\nuser@host:~$ ", false), "out\nuser@host:~$");
}

#[test]
fn render_skips_alternate_screen_content() {
    let raw = b"before\r\n\x1b[?1049hVIM SCREEN\r\n~\r\n\x1b[?1049lafter\r\n";
    assert_eq!(render(raw, false), "before\nafter");
    // A slice that starts inside a full-screen program skips until it leaves.
    assert_eq!(render(b"drawing\x1b[?1049lback\r\n", true), "back");
}

#[test]
fn render_survives_invalid_utf8() {
    assert_eq!(render(b"a\xffb\r\n", false), "a\u{fffd}b");
}

#[test]
fn huge_cursor_movements_stay_bounded() {
    // A remote program can print any parameter; a real terminal stops at
    // its width, and so must the renderer (no giant line, no overflow).
    for raw in [
        &b"a\x1b[10000000000000Cb\r\n"[..],
        b"a\x1b[18446744073709551615Cb\r\n",
        b"a\x1b[99999999999999999999999Cb\r\n",
        b"a\x1b[18446744073709551615Gb\r\n",
        b"abc\x1b[18446744073709551615P\x1b[18446744073709551615X\r\n",
    ] {
        assert!(render(raw, false).len() <= 4100);
    }
    assert_eq!(
        render(b"abc\x1b[1G\x1b[18446744073709551615P\r\n", false),
        ""
    );
}

#[test]
fn the_cursor_line_stops_at_the_cursor() {
    // zsh draws RPROMPT at the right edge, then moves the cursor back.
    let raw = b"out\r\nuser@host ~ %\x1b[30C12:04:55\r\x1b[14C";
    let rendered = render_with_cursor(raw, false);
    assert_eq!(rendered.cursor_line, "user@host ~ %");
    assert!(rendered.text.ends_with("12:04:55"));
    assert!(looks_like_prompt(&rendered.cursor_line));
    assert!(!looks_like_prompt(&rendered.text));
    // After a newline the cursor sits on an empty line.
    assert_eq!(render_with_cursor(b"done\r\n", false).cursor_line, "");
    // A full-screen program has no cursor line.
    assert_eq!(render_with_cursor(b"$ \x1b[?1049h~", false).cursor_line, "");
}

#[test]
fn prompts_of_other_programs_are_not_idle_shells() {
    for line in [
        ">",
        "> ",
        "dquote>",
        "heredoc>",
        "mysql>",
        "sqlite>",
        "sftp>",
        ">>> ",
        "postgres=#",
        "postgres=>",
        "postgres-#",
        "irb(main):001:0>",
        "MariaDB [shop]>",
    ] {
        assert!(!looks_like_prompt(line), "{line:?} is not a shell prompt");
    }
    assert!(looks_like_prompt("user@host:~>"));
    assert!(looks_like_prompt("[prod] ~/app >"));
}

#[test]
fn prompt_detection() {
    assert!(looks_like_prompt("output\nuser@host:~$"));
    assert!(looks_like_prompt("host% "));
    assert!(looks_like_prompt("[root@x ~]# "));
    assert!(looks_like_prompt("~/src ❯ "));
    assert!(!looks_like_prompt("user@host:~$ ls -"));
    assert!(!looks_like_prompt("compiling...\n"));
    assert!(!looks_like_prompt(""));
}

#[test]
fn tail_lines_counts_from_the_end() {
    assert_eq!(tail_lines("a\nb\nc", 2), "b\nc");
    assert_eq!(tail_lines("a\nb\nc", 10), "a\nb\nc");
    assert_eq!(tail_lines("a\nb\nc", 0), "");
}

#[test]
fn truncate_middle_keeps_head_and_larger_tail() {
    let text = "0123456789".repeat(10);
    let (short, omitted) = truncate_middle(&text, 30);
    assert_eq!(omitted, 70);
    assert!(short.starts_with("0123456789\n… [70 characters omitted] …\n"));
    assert!(short.ends_with("01234567890123456789"));
    assert_eq!(truncate_middle("abc", 30), ("abc".to_string(), 0));
}

#[test]
fn modes_follow_split_sequences() {
    let mut modes = ModeTracker::default();
    modes.feed(b"prompt\x1b[?20");
    assert!(!modes.bracketed_paste());
    modes.feed(b"04h$ ");
    assert!(modes.bracketed_paste());
    modes.feed(b"\x1b[?1049h");
    assert!(modes.alt_screen());
    modes.feed(b"\x1b[?1049;2004l");
    assert!(!modes.alt_screen());
    assert!(!modes.bracketed_paste());
    // Ordinary colour sequences never flip a mode.
    modes.feed(b"\x1b[1049h\x1b[0m");
    assert!(!modes.alt_screen());
}

#[test]
fn find_and_parse_exit_status() {
    let stream = b"xx\x1b]6973;E;n1;127\x07tail";
    let prefix = b"\x1b]6973;E;n1;";
    let at = find(stream, prefix, 0).unwrap();
    assert_eq!(at, 2);
    assert_eq!(find(stream, prefix, 3), None);
    let (status, used) = parse_exit_status(&stream[at + prefix.len()..]).unwrap();
    assert_eq!(status, Some(127));
    assert_eq!(used, 4);
    assert_eq!(parse_exit_status(b"12"), None);
    assert_eq!(parse_exit_status(b"oops\x07"), Some((None, 5)));
}
