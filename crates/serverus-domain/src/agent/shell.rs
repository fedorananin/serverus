//! Typing an agent command into the user's interactive shell.
//!
//! The command is wrapped between two invisible OSC markers printed by the
//! shell itself: a start marker right before the command runs and an end
//! marker carrying `$?` right after it. Everything between them in the
//! terminal stream is the command's output; the echo of the typed line comes
//! before the start marker, so it never pollutes the result. The typed text
//! contains the escapes as `\033` / `\007` literals, which is why the echo
//! can never be mistaken for a real marker.

/// OSC number used for the markers. Unknown OSC sequences are ignored by
/// terminal emulators, so the markers never show up on screen.
pub const OSC_ID: &str = "6973";

/// Bracketed-paste delimiters (`ESC[200~` … `ESC[201~`).
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// Syntax family of the shell the command is typed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFlavor {
    /// sh, bash, zsh, dash, ksh, ash, busybox — `$?` and `{ …; }` groups.
    Posix,
    /// fish — `$status` and `begin … end`.
    Fish,
    /// csh / tcsh — `$status`, single-line commands only.
    Csh,
}

impl ShellFlavor {
    /// Classify a shell from `$SHELL` (a path or a bare name, optionally
    /// with a login-shell `-` prefix). Unknown shells are assumed POSIX.
    pub fn from_shell_path(path: &str) -> Self {
        let name = path.trim().rsplit('/').next().unwrap_or_default();
        match name.trim_start_matches('-') {
            "fish" => ShellFlavor::Fish,
            "csh" | "tcsh" => ShellFlavor::Csh,
            _ => ShellFlavor::Posix,
        }
    }
}

/// Why a command cannot be typed into the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    Empty,
    /// Raw control characters (other than newline and tab) could drive the
    /// line editor instead of becoming part of the command.
    ControlCharacters,
    /// Without bracketed paste a tab triggers completion.
    TabWithoutBracketedPaste,
    /// csh has no command grouping that survives being typed line by line.
    MultiLineUnsupported,
}

impl std::fmt::Display for WrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            WrapError::Empty => "the command is empty",
            WrapError::ControlCharacters => {
                "the command contains control characters; use send_input for keystrokes"
            }
            WrapError::TabWithoutBracketedPaste => {
                "this shell has no bracketed paste, so a tab character would trigger completion; use spaces"
            }
            WrapError::MultiLineUnsupported => {
                "multi-line commands are not supported in csh/tcsh; send one line at a time"
            }
        })
    }
}

impl std::error::Error for WrapError {}

/// The bytes the shell prints right before the command runs.
pub fn start_marker(nonce: &str) -> Vec<u8> {
    format!("\x1b]{OSC_ID};S;{nonce}\x07").into_bytes()
}

/// The bytes the shell prints after the command, up to the exit status.
/// The status digits and a terminating BEL follow.
pub fn end_marker_prefix(nonce: &str) -> Vec<u8> {
    format!("\x1b]{OSC_ID};E;{nonce};").into_bytes()
}

/// Build the keystrokes that run `command` wrapped in completion markers.
///
/// `nonce` must be short and alphanumeric — it is spliced into the typed
/// line verbatim. `bracketed_paste` reports whether the shell currently has
/// bracketed paste enabled; when it does, the whole text is pasted as one
/// unit, otherwise it is typed line by line.
pub fn wrap(
    flavor: ShellFlavor,
    nonce: &str,
    command: &str,
    bracketed_paste: bool,
) -> Result<Vec<u8>, WrapError> {
    debug_assert!(nonce.chars().all(|c| c.is_ascii_alphanumeric()));
    let command = command.replace("\r\n", "\n");
    let command = command.trim_end();
    if command.trim().is_empty() {
        return Err(WrapError::Empty);
    }
    if command
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(WrapError::ControlCharacters);
    }
    if command.contains('\t') && !bracketed_paste {
        return Err(WrapError::TabWithoutBracketedPaste);
    }

    let start = format!("printf '\\033]{OSC_ID};S;{nonce}\\007'");
    let end_format = format!("printf '\\033]{OSC_ID};E;{nonce};%s\\007'");
    // A leading space keeps the wrapper out of shell history where the shell
    // honours it (HISTCONTROL=ignorespace, zsh HIST_IGNORE_SPACE).
    // Interactive csh has no `#` comments, so only newlines force a block.
    let block = match flavor {
        ShellFlavor::Csh => command.contains('\n'),
        _ => needs_block(command),
    };
    let line = match (flavor, block) {
        (ShellFlavor::Posix, false) => format!(" {start}; {command}; {end_format} \"$?\""),
        (ShellFlavor::Posix, true) => format!(" {start}; {{\n{command}\n}}; {end_format} \"$?\""),
        (ShellFlavor::Fish, false) | (ShellFlavor::Csh, false) => {
            format!(" {start}; {command}; {end_format} $status")
        }
        (ShellFlavor::Fish, true) => {
            format!(" {start}; begin\n{command}\nend; {end_format} $status")
        }
        (ShellFlavor::Csh, true) => return Err(WrapError::MultiLineUnsupported),
    };

    let mut bytes = Vec::with_capacity(line.len() + 16);
    if bracketed_paste {
        bytes.extend_from_slice(PASTE_START);
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(PASTE_END);
    } else {
        bytes.extend_from_slice(line.replace('\n', "\r").as_bytes());
    }
    bytes.push(b'\r');
    Ok(bytes)
}

/// Whether `command` must run inside a group on lines of its own instead of
/// being spliced between `;`s: multi-line text, a comment that would swallow
/// the end marker, or a trailing operator that `; …` would turn into a
/// syntax error (`cmd &; …`) or a continuation.
fn needs_block(command: &str) -> bool {
    command.contains('\n')
        || command.contains('#')
        || command.ends_with(['&', ';', '|', '\\', '{', '('])
}
