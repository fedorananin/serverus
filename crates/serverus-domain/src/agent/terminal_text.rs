//! Turning raw terminal output into text an agent can read.
//!
//! This is not a terminal emulator: it keeps a line model with a cursor
//! column, which is enough for shell output — carriage-return progress bars
//! collapse to their final state, line editing via backspace / erase
//! sequences resolves, colours and titles vanish. Content drawn on the
//! alternate screen (vim, less, top) is skipped entirely: it is a picture,
//! not a transcript.

/// Farthest column a cursor movement can reach. Real terminals stop at
/// their width; without a bound, one `ESC[<huge>C` from a remote program
/// would make the next character allocate a line that long.
const MAX_COLUMNS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Csi,
    /// OSC / DCS / APC / PM / SOS payload, terminated by BEL or ESC `\`.
    String,
    StringEsc,
    /// ESC ( / ) / * / + designate a character set; one more byte follows.
    Charset,
}

struct Renderer {
    lines: Vec<String>,
    line: Vec<char>,
    col: usize,
    alt: bool,
    state: State,
    params: String,
}

impl Renderer {
    fn put(&mut self, c: char) {
        if self.alt {
            return;
        }
        if self.col < self.line.len() {
            self.line[self.col] = c;
        } else {
            self.line.resize(self.col, ' ');
            self.line.push(c);
        }
        self.col += 1;
    }

    fn newline(&mut self) {
        if self.alt {
            return;
        }
        let text: String = self.line.iter().collect();
        self.lines.push(text.trim_end().to_string());
        self.line.clear();
        self.col = 0;
    }

    fn csi(&mut self, final_byte: char) {
        let params = std::mem::take(&mut self.params);
        if let Some(private) = params.strip_prefix('?') {
            if matches!(final_byte, 'h' | 'l')
                && private
                    .split(';')
                    .any(|p| matches!(p, "1049" | "1047" | "47"))
            {
                // Entering or leaving the alternate screen: nothing drawn
                // there belongs to the transcript, and the cursor returns to
                // where it was.
                self.alt = final_byte == 'h';
            }
            return;
        }
        if self.alt {
            return;
        }
        let n = params
            .split(';')
            .next()
            .and_then(|p| p.parse::<usize>().ok());
        match final_byte {
            // Erase in line: 0/empty = cursor to end, 1 = start to cursor, 2 = all.
            'K' => match n.unwrap_or(0) {
                0 => self.line.truncate(self.col),
                1 => {
                    let end = self.col.min(self.line.len());
                    self.line[..end].fill(' ');
                }
                _ => self.line.clear(),
            },
            'C' => {
                self.col = self
                    .col
                    .saturating_add(n.unwrap_or(1).max(1))
                    .min(MAX_COLUMNS)
            }
            'D' => self.col = self.col.saturating_sub(n.unwrap_or(1).max(1)),
            'G' => self.col = (n.unwrap_or(1).max(1) - 1).min(MAX_COLUMNS),
            // Delete characters, shifting the rest of the line left.
            'P' => {
                if self.col < self.line.len() {
                    let end = self
                        .col
                        .saturating_add(n.unwrap_or(1).max(1))
                        .min(self.line.len());
                    self.line.drain(self.col..end);
                }
            }
            // Erase characters in place.
            'X' => {
                let end = self
                    .col
                    .saturating_add(n.unwrap_or(1).max(1))
                    .min(self.line.len());
                if self.col < end {
                    self.line[self.col..end].fill(' ');
                }
            }
            _ => {}
        }
    }

    fn feed(&mut self, c: char) {
        match self.state {
            State::Ground => match c {
                '\x1b' => self.state = State::Esc,
                '\n' => self.newline(),
                '\r' => self.col = 0,
                '\x08' => self.col = self.col.saturating_sub(1),
                '\t' => self.put('\t'),
                c if c.is_control() => {}
                c => self.put(c),
            },
            State::Esc => {
                self.state = match c {
                    '[' => {
                        self.params.clear();
                        State::Csi
                    }
                    ']' | 'P' | 'X' | '^' | '_' => State::String,
                    '(' | ')' | '*' | '+' => State::Charset,
                    '\x1b' => State::Esc,
                    _ => State::Ground,
                }
            }
            State::Csi => match c {
                '\x40'..='\x7e' => {
                    self.state = State::Ground;
                    self.csi(c);
                }
                '\x1b' => self.state = State::Esc,
                c => self.params.push(c),
            },
            State::String => match c {
                '\x07' => self.state = State::Ground,
                '\x1b' => self.state = State::StringEsc,
                _ => {}
            },
            State::StringEsc => {
                self.state = if c == '\\' {
                    State::Ground
                } else {
                    State::String
                }
            }
            State::Charset => self.state = State::Ground,
        }
    }
}

/// Render raw terminal output as plain text.
///
/// `starts_on_alt_screen` says whether the slice begins while a full-screen
/// program has the alternate screen, so its drawing is skipped from the
/// start. Trailing whitespace is trimmed from every line; the last line is
/// kept even without a newline (it is usually the prompt).
pub fn render(bytes: &[u8], starts_on_alt_screen: bool) -> String {
    render_with_cursor(bytes, starts_on_alt_screen).text
}

/// [`render`]'s text plus the line the cursor is on, cut at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    /// The cursor's line up to the cursor, trailing blanks trimmed. For an
    /// idle shell this is exactly its prompt: a right-hand prompt (zsh
    /// `RPROMPT`, starship's right format) is drawn past the cursor and the
    /// cursor then moves back. Empty after a trailing newline and while a
    /// full-screen program runs.
    pub cursor_line: String,
}

pub fn render_with_cursor(bytes: &[u8], starts_on_alt_screen: bool) -> Rendered {
    let mut renderer = Renderer {
        lines: Vec::new(),
        line: Vec::new(),
        col: 0,
        alt: starts_on_alt_screen,
        state: State::Ground,
        params: String::new(),
    };
    for c in String::from_utf8_lossy(bytes).chars() {
        renderer.feed(c);
    }
    let cursor_line = if renderer.alt {
        String::new()
    } else {
        let upto = renderer.col.min(renderer.line.len());
        let text: String = renderer.line[..upto].iter().collect();
        text.trim_end().to_string()
    };
    if !renderer.line.is_empty() {
        renderer.newline();
    }
    Rendered {
        text: renderer.lines.join("\n"),
        cursor_line,
    }
}

/// The last `count` lines of `text`.
pub fn tail_lines(text: &str, count: usize) -> &str {
    if count == 0 {
        return "";
    }
    let mut seen = 0;
    for (index, byte) in text.bytes().enumerate().rev() {
        if byte == b'\n' {
            seen += 1;
            if seen == count {
                return &text[index + 1..];
            }
        }
    }
    text
}

/// Whether the last line looks like an idle shell prompt: it ends with one
/// of the characters prompts conventionally end with. A running program's
/// output or a half-typed command line almost never does. Prompts of
/// programs other than the shell (REPLs, database clients) and shell
/// continuation prompts are not idle shells, so typing a command line
/// there would be wrong.
pub fn looks_like_prompt(text: &str) -> bool {
    let last = text.rsplit('\n').next().unwrap_or_default().trim_end();
    last.ends_with(['$', '#', '%', '>', '❯', '➜', '»', '›', 'λ']) && !is_program_prompt(last)
}

fn is_program_prompt(line: &str) -> bool {
    let line = line.trim();
    // `>` alone or one bare word before it: a continuation prompt (PS2,
    // zsh's `dquote>` / `heredoc>`) or a program's own (node's `>`,
    // `mysql>`, `sqlite>`, `ftp>`). Shell prompts carry a host, path or
    // user around their sigil.
    if let Some(word) = line.strip_suffix('>') {
        if word
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return true;
        }
    }
    // Python's `>>>`; psql's `db=#`, `db=>` and their continuations.
    const SUFFIXES: [&str; 5] = [">>>", "=#", "=>", "-#", "->"];
    // irb / pry, MariaDB's `MariaDB [db]>`.
    const PREFIXES: [&str; 3] = ["irb(", "pry(", "MariaDB ["];
    SUFFIXES.iter().any(|suffix| line.ends_with(suffix))
        || PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// `text` cut to at most `max_chars` characters by dropping the middle: the
/// head shows how a command started, the (larger) tail how it ended.
/// Returns the text and how many characters were omitted.
pub fn truncate_middle(text: &str, max_chars: usize) -> (String, usize) {
    let total = text.chars().count();
    if total <= max_chars {
        return (text.to_string(), 0);
    }
    let head = max_chars / 3;
    let tail = max_chars - head;
    let omitted = total - head - tail;
    let head_text: String = text.chars().take(head).collect();
    let tail_text: String = text.chars().skip(total - tail).collect();
    (
        format!("{head_text}\n… [{omitted} characters omitted] …\n{tail_text}"),
        omitted,
    )
}
