use super::super::{safe_local_component, sanitize_windows_component};

#[test]
fn attack_shaped_names_are_refused_on_every_os() {
    for name in [
        "",
        ".",
        "..",
        "a/b",
        "../escape",
        "/absolute",
        "nul\0byte",
        "line\nbreak",
        "bell\x07",
    ] {
        assert!(
            safe_local_component(name).is_err(),
            "attack name was accepted: {name:?}"
        );
    }
}

#[cfg(not(windows))]
#[test]
fn unix_keeps_locally_legal_names_verbatim() {
    for name in [
        "2024-01-01T12:00:00.log",
        "wild*card",
        "what?.txt",
        "aux.txt",
        "CON",
        "trailing.",
        "trailing ",
        r"back\slash.txt",
        "C:drive-relative.txt",
        "..double-dot-prefix",
        "файл — 🚀.txt",
    ] {
        assert_eq!(
            safe_local_component(name).unwrap(),
            name,
            "legal Unix name was altered"
        );
    }
}

#[cfg(windows)]
#[test]
fn windows_sanitizes_instead_of_refusing() {
    assert_eq!(
        safe_local_component("2024-01-01T12:00:00.log").unwrap(),
        "2024-01-01T12_00_00.log"
    );
    assert_eq!(safe_local_component("aux.txt").unwrap(), "_aux.txt");
    assert_eq!(safe_local_component("trailing.").unwrap(), "trailing_");
}

#[test]
fn the_windows_sanitizer_repairs_every_reserved_form() {
    for (name, expected) in [
        ("2024-01-01T12:00:00.log", "2024-01-01T12_00_00.log"),
        ("wild*card?", "wild_card_"),
        (r"back\slash.txt", "back_slash.txt"),
        ("pipe|quote\"<>", "pipe_quote___"),
        ("aux.txt", "_aux.txt"),
        ("CON", "_CON"),
        ("com1.log", "_com1.log"),
        ("trailing.", "trailing_"),
        ("trailing ", "trailing_"),
        ("trail.. ", "trail___"),
        ("normal.txt", "normal.txt"),
        (".env", ".env"),
    ] {
        assert_eq!(
            sanitize_windows_component(name),
            expected,
            "sanitizing {name:?}"
        );
    }
}
