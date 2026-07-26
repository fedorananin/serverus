//! The extension → media type table behind [`super::guess_content_type`].
//!
//! IANA-registered types where they exist, otherwise the `x-` type providers
//! and browsers actually agree on. Nothing here is invented: an extension
//! with no defensible answer is left out so the SDK default stands.
//!
//! Extensions that name more than one format are resolved toward what people
//! actually put in a bucket, and the ones with no such answer are listed in
//! [`ABSENT_BY_DESIGN`] rather than guessed at.
//!
//! The entries are split across sibling modules purely by subject matter —
//! the lookup sees one flat sequence, so which group an extension lives in
//! never affects the answer.

mod av;
mod documents;
mod images;
mod web;

/// Every group, in the order they are searched.
///
/// Each entry maps extensions (lowercase, without the dot) to the media type
/// they upload under. Every extension appears exactly once across all groups;
/// `content_type_tests.rs` enforces that, so the order never silently decides
/// a lookup.
const GROUPS: &[&[(&[&str], &str)]] = &[web::TYPES, images::TYPES, av::TYPES, documents::TYPES];

/// Every `(extensions, media type)` entry, flattened across the groups.
pub(super) fn entries() -> impl Iterator<Item = &'static (&'static [&'static str], &'static str)> {
    GROUPS.iter().copied().flatten()
}

/// Extensions left out on purpose, with the reason. Keeping the list here
/// stops each of them from being "fixed" by the next person to notice it.
/// `content_type_tests.rs` asserts the table never grows one of these.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) const ABSENT_BY_DESIGN: &[(&str, &str)] = &[
    (
        "key",
        "Keynote presentation or a TLS private key — for a server tool the \
         second reading is likelier, and neither is worth guessing",
    ),
    (
        "env",
        "holds secrets; there is no upside to making it render inline in a \
         browser instead of downloading",
    ),
    ("pub", "an SSH public key or a Microsoft Publisher document"),
    (
        "svgz",
        "gzipped SVG needs a Content-Encoding header this module cannot set, \
         and image/svg+xml alone would render as garbage",
    ),
    (
        "bin",
        "names no format; the octet-stream default already says exactly that",
    ),
    ("dat", "names no format"),
    (
        "raw",
        "camera-vendor specific; the concrete extensions are listed",
    ),
    (
        "img",
        "a disk image, a firmware blob, or a photo, depending on who wrote it",
    ),
    ("app", "a macOS bundle directory, not a file"),
];
