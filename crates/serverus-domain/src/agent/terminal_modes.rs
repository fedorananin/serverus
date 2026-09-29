//! Tracking the terminal modes that matter for typing into a shell, plus
//! byte-level helpers for locating the completion markers in a stream.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Scan {
    #[default]
    Ground,
    Esc,
    Csi,
}

/// Follows a terminal output stream chunk by chunk (sequences may be split
/// across chunks) and remembers two private modes: the alternate screen
/// (a full-screen program is running) and bracketed paste (the line editor
/// accepts pasted text as one unit).
#[derive(Debug, Clone, Default)]
pub struct ModeTracker {
    scan: Scan,
    private: bool,
    params: String,
    alt_screen: bool,
    bracketed_paste: bool,
}

impl ModeTracker {
    pub fn alt_screen(&self) -> bool {
        self.alt_screen
    }

    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.step(byte);
        }
    }

    fn step(&mut self, byte: u8) {
        match self.scan {
            Scan::Ground => {
                if byte == 0x1b {
                    self.scan = Scan::Esc;
                }
            }
            Scan::Esc => {
                self.scan = match byte {
                    b'[' => {
                        self.private = false;
                        self.params.clear();
                        Scan::Csi
                    }
                    0x1b => Scan::Esc,
                    _ => Scan::Ground,
                }
            }
            Scan::Csi => match byte {
                b'?' if self.params.is_empty() && !self.private => self.private = true,
                b'0'..=b'9' | b';' => self.params.push(byte as char),
                0x40..=0x7e => {
                    if self.private && matches!(byte, b'h' | b'l') {
                        self.apply(byte == b'h');
                    }
                    self.scan = Scan::Ground;
                }
                0x1b => self.scan = Scan::Esc,
                // Intermediate bytes keep the sequence open; anything else
                // aborts it.
                0x20..=0x2f => {}
                _ => self.scan = Scan::Ground,
            },
        }
    }

    fn apply(&mut self, set: bool) {
        for param in self.params.split(';') {
            match param {
                "1049" | "1047" | "47" => self.alt_screen = set,
                "2004" => self.bracketed_paste = set,
                _ => {}
            }
        }
    }
}

/// Position of `needle` in `haystack` at or after `from`.
pub fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + from)
}

/// Parse the exit status that follows an end-marker prefix: decimal digits
/// (an optional leading `-`) terminated by BEL. Returns the status and the
/// number of bytes consumed including the BEL, or `None` while incomplete.
/// A malformed status (the shell printed something unexpected) yields
/// `Some((None, len))` so the caller still knows where the marker ends.
pub fn parse_exit_status(bytes: &[u8]) -> Option<(Option<i32>, usize)> {
    let end = bytes.iter().position(|&byte| byte == 0x07)?;
    let status = std::str::from_utf8(&bytes[..end])
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok());
    Some((status, end + 1))
}
