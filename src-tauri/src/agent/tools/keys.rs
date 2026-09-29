//! Named keys for `send_input`, as the byte sequences an xterm sends.

pub fn key_bytes(name: &str) -> Option<Vec<u8>> {
    let name = name.trim().to_ascii_lowercase();
    let fixed: &[u8] = match name.as_str() {
        "enter" | "return" => b"\r",
        "tab" => b"\t",
        "escape" | "esc" => b"\x1b",
        "backspace" => b"\x7f",
        "delete" => b"\x1b[3~",
        "space" => b" ",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "page_up" => b"\x1b[5~",
        "page_down" => b"\x1b[6~",
        "ctrl-\\" => b"\x1c",
        _ => return control_letter(&name),
    };
    Some(fixed.to_vec())
}

/// `ctrl-a` … `ctrl-z` → 0x01 … 0x1a.
fn control_letter(name: &str) -> Option<Vec<u8>> {
    let letter = name.strip_prefix("ctrl-")?;
    let mut chars = letter.chars();
    match (chars.next(), chars.next()) {
        (Some(c @ 'a'..='z'), None) => Some(vec![c as u8 - b'a' + 1]),
        _ => None,
    }
}
