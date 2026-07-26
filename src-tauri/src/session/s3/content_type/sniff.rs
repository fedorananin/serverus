//! Magic-byte detection, used only where the name says nothing.
//!
//! Sniffing is a fallback, never an override. An extension that resolves is
//! authoritative because it is what the publisher intended and what a CDN
//! contract is written against — and because the formats that actually broke
//! sites (CSS, JS, JSON) are plain UTF-8 with no signature to find. Reading
//! bytes would answer `text/plain` for a stylesheet, which a browser rejects
//! just as firmly as `application/octet-stream`. Container formats make it
//! worse: `.docx`, `.xlsx`, `.epub`, `.jar` and `.apk` are all ZIP archives,
//! so content alone would collapse them into `application/zip`.
//!
//! What sniffing is good for is the file whose name says nothing at all —
//! `photo` that is really a JPEG, `dump` that is really a SQLite database.
//!
//! **Nothing here ever resolves to HTML, SVG, XML or any script type**, and
//! that omission is deliberate rather than an oversight. Labelling an
//! unnamed upload `text/html` or `image/svg+xml` on the strength of its bytes
//! would turn an inert download into a document that executes inside the
//! bucket's own origin. The signatures below are unambiguous binary formats
//! only; anything textual keeps the octet-stream default.

/// Bytes the writer needs buffered for a full attempt. The tar signature at
/// offset 257 is the deepest probe.
pub(in crate::session::s3) const HEAD_LEN: usize = 512;

/// `(offset, magic, media type)`, longest and most specific first.
const SIGNATURES: &[(usize, &[u8], &str)] = &[
    // Images.
    (0, b"\x89PNG\r\n\x1a\n", "image/png"),
    (0, b"\xff\xd8\xff", "image/jpeg"),
    (0, b"GIF87a", "image/gif"),
    (0, b"GIF89a", "image/gif"),
    (0, b"\0\0\0\x0cJXL \r\n\x87\n", "image/jxl"),
    (0, b"8BPS", "image/vnd.adobe.photoshop"),
    (0, b"II*\0", "image/tiff"),
    (0, b"MM\0*", "image/tiff"),
    (0, b"\0\0\x01\0", "image/x-icon"),
    // Documents.
    (0, b"%PDF-", "application/pdf"),
    (0, b"{\\rtf", "application/rtf"),
    // Archives and packages. A bare ZIP signature stays `application/zip`:
    // telling an `.docx` from an `.epub` needs the archive's directory, and
    // guessing between them would be worse than the honest container type.
    (0, b"PK\x03\x04", "application/zip"),
    (0, b"\x1f\x8b", "application/gzip"),
    (0, b"BZh", "application/x-bzip2"),
    (0, b"\xfd7zXZ\0", "application/x-xz"),
    (0, b"\x28\xb5\x2f\xfd", "application/zstd"),
    (0, b"\x04\x22\x4d\x18", "application/x-lz4"),
    (0, b"7z\xbc\xaf\x27\x1c", "application/x-7z-compressed"),
    (0, b"Rar!\x1a\x07", "application/vnd.rar"),
    (0, b"MSCF", "application/vnd.ms-cab-compressed"),
    (0, b"\xed\xab\xee\xdb", "application/x-rpm"),
    (
        0,
        b"!<arch>\ndebian",
        "application/vnd.debian.binary-package",
    ),
    (257, b"ustar", "application/x-tar"),
    // Executables. Only the unmistakable one; ELF and Mach-O have no media
    // type worth more than the octet-stream default.
    (
        0,
        b"MZ\x90\0",
        "application/vnd.microsoft.portable-executable",
    ),
    // Audio.
    (0, b"ID3", "audio/mpeg"),
    (0, b"fLaC", "audio/flac"),
    (0, b"OggS", "audio/ogg"),
    (0, b"MThd", "audio/midi"),
    (0, b".snd", "audio/basic"),
    // Fonts.
    (0, b"wOFF", "font/woff"),
    (0, b"wOF2", "font/woff2"),
    (0, b"OTTO", "font/otf"),
    (0, b"ttcf", "font/collection"),
    (0, b"\0\x01\0\0\0", "font/ttf"),
    // Databases.
    (0, b"SQLite format 3\0", "application/vnd.sqlite3"),
];

/// The media type `head` unambiguously identifies, or `None` when the bytes
/// say nothing certain.
///
/// `head` is the beginning of the object; anything shorter than a signature
/// simply fails to match.
pub(in crate::session::s3) fn sniff(head: &[u8]) -> Option<&'static str> {
    // The shape-based probes come first: their formats share a prefix across
    // many media types, so a flat signature list cannot separate them.
    iso_base_media(head)
        .or_else(|| matroska(head))
        .or_else(|| riff(head))
        .or_else(|| {
            SIGNATURES
                .iter()
                .find(|(offset, magic, _)| matches_at(head, *offset, magic))
                .map(|(_, _, media_type)| *media_type)
        })
}

fn matches_at(head: &[u8], offset: usize, magic: &[u8]) -> bool {
    head.len() >= offset + magic.len() && &head[offset..offset + magic.len()] == magic
}

/// ISO base media files (`ftyp` at offset 4) cover MP4, MOV, HEIC, AVIF and
/// 3GP alike — only the brand that follows separates them.
fn iso_base_media(head: &[u8]) -> Option<&'static str> {
    if !matches_at(head, 4, b"ftyp") || head.len() < 12 {
        return None;
    }
    Some(match &head[8..12] {
        b"avif" | b"avis" => "image/avif",
        b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"mif1" | b"msf1" => "image/heic",
        b"qt  " => "video/quicktime",
        b"M4A " | b"M4B " => "audio/mp4",
        brand if brand.starts_with(b"3g") => "video/3gpp",
        // isom, iso2, mp41, mp42, avc1, dash, M4V … all play as MP4.
        _ => "video/mp4",
    })
}

/// WebM is Matroska with a different DocType, and the DocType string sits in
/// the first EBML header well inside the sniffed window.
fn matroska(head: &[u8]) -> Option<&'static str> {
    if !matches_at(head, 0, b"\x1a\x45\xdf\xa3") {
        return None;
    }
    let header = &head[..head.len().min(64)];
    Some(if header.windows(4).any(|w| w == b"webm".as_slice()) {
        "video/webm"
    } else {
        "video/x-matroska"
    })
}

/// RIFF containers hold WAV, AVI and WebP behind the same four bytes.
fn riff(head: &[u8]) -> Option<&'static str> {
    if !matches_at(head, 0, b"RIFF") || head.len() < 12 {
        return None;
    }
    match &head[8..12] {
        b"WEBP" => Some("image/webp"),
        b"WAVE" => Some("audio/wav"),
        b"AVI " => Some("video/x-msvideo"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "sniff_tests.rs"]
mod tests;
