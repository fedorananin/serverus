use serverus_domain::agent::shell::{
    end_marker_prefix, start_marker, wrap, ShellFlavor, WrapError,
};

fn typed(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

#[test]
fn flavor_from_shell_path() {
    assert_eq!(ShellFlavor::from_shell_path("/bin/zsh"), ShellFlavor::Posix);
    assert_eq!(
        ShellFlavor::from_shell_path("/usr/bin/bash\n"),
        ShellFlavor::Posix
    );
    assert_eq!(ShellFlavor::from_shell_path("-bash"), ShellFlavor::Posix);
    assert_eq!(
        ShellFlavor::from_shell_path("/opt/homebrew/bin/fish"),
        ShellFlavor::Fish
    );
    assert_eq!(ShellFlavor::from_shell_path("/bin/tcsh"), ShellFlavor::Csh);
    assert_eq!(ShellFlavor::from_shell_path(""), ShellFlavor::Posix);
}

#[test]
fn markers_are_invisible_osc_sequences() {
    assert_eq!(start_marker("ab12"), b"\x1b]6973;S;ab12\x07");
    assert_eq!(end_marker_prefix("ab12"), b"\x1b]6973;E;ab12;");
}

#[test]
fn posix_single_line_is_spliced_between_markers() {
    let line = typed(wrap(ShellFlavor::Posix, "n1", "uptime", false).unwrap());
    assert_eq!(
        line,
        " printf '\\033]6973;S;n1\\007'; uptime; printf '\\033]6973;E;n1;%s\\007' \"$?\"\r"
    );
    // The typed text never contains a real ESC: its echo cannot fake a marker.
    assert!(!line.contains('\x1b'));
}

#[test]
fn trailing_newlines_and_crlf_are_normalised() {
    let line = typed(wrap(ShellFlavor::Posix, "n1", "ls\r\n\n", false).unwrap());
    assert!(line.contains("; ls; "), "{line}");
}

#[test]
fn risky_commands_run_in_a_group() {
    for command in ["sleep 5 &", "ls # list", "echo a;", "cat <<'EOF'\nx\nEOF"] {
        let line = typed(wrap(ShellFlavor::Posix, "n1", command, false).unwrap());
        assert!(line.contains("; {\r"), "{command:?} → {line:?}");
        assert!(line.contains("\r}; printf"), "{command:?} → {line:?}");
    }
}

#[test]
fn bracketed_paste_wraps_the_whole_text_and_keeps_newlines() {
    let line = typed(wrap(ShellFlavor::Posix, "n1", "echo a\necho b", true).unwrap());
    assert!(line.starts_with("\x1b[200~ printf"));
    assert!(line.ends_with("\x1b[201~\r"));
    assert!(line.contains("{\necho a\necho b\n}"));
}

#[test]
fn fish_uses_status_and_begin_end() {
    let single = typed(wrap(ShellFlavor::Fish, "n1", "ls", false).unwrap());
    assert!(single.ends_with("%s\\007' $status\r"), "{single}");
    let block = typed(wrap(ShellFlavor::Fish, "n1", "ls\npwd", true).unwrap());
    assert!(block.contains("; begin\nls\npwd\nend; printf"), "{block}");
}

#[test]
fn csh_is_single_line_only_and_keeps_hash() {
    let line = typed(wrap(ShellFlavor::Csh, "n1", "echo a#b", false).unwrap());
    assert!(line.contains("; echo a#b; "), "{line}");
    assert_eq!(
        wrap(ShellFlavor::Csh, "n1", "ls\npwd", false),
        Err(WrapError::MultiLineUnsupported)
    );
}

#[test]
fn rejects_empty_and_control_characters() {
    assert_eq!(
        wrap(ShellFlavor::Posix, "n1", "  \n", false),
        Err(WrapError::Empty)
    );
    assert_eq!(
        wrap(ShellFlavor::Posix, "n1", "ls\x1b[A", false),
        Err(WrapError::ControlCharacters)
    );
    assert_eq!(
        wrap(ShellFlavor::Posix, "n1", "printf 'a\tb'", false),
        Err(WrapError::TabWithoutBracketedPaste)
    );
    assert!(wrap(ShellFlavor::Posix, "n1", "printf 'a\tb'", true).is_ok());
}
