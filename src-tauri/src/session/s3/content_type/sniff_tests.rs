use super::{sniff, HEAD_LEN};

/// A byte string padded out to the sniff window, the way the writer's buffer
/// presents a real object.
fn head(prefix: &[u8]) -> Vec<u8> {
    let mut bytes = prefix.to_vec();
    bytes.resize(HEAD_LEN, 0);
    bytes
}

#[test]
fn common_binary_formats_are_recognized() {
    assert_eq!(sniff(&head(b"\x89PNG\r\n\x1a\n")), Some("image/png"));
    assert_eq!(sniff(&head(b"\xff\xd8\xff\xe0")), Some("image/jpeg"));
    assert_eq!(sniff(&head(b"GIF89a")), Some("image/gif"));
    assert_eq!(sniff(&head(b"%PDF-1.7")), Some("application/pdf"));
    assert_eq!(sniff(&head(b"PK\x03\x04")), Some("application/zip"));
    assert_eq!(sniff(&head(b"\x1f\x8b\x08")), Some("application/gzip"));
    assert_eq!(sniff(&head(b"8BPS")), Some("image/vnd.adobe.photoshop"));
    assert_eq!(sniff(&head(b"fLaC")), Some("audio/flac"));
    assert_eq!(sniff(&head(b"ID3\x04")), Some("audio/mpeg"));
    assert_eq!(sniff(&head(b"wOF2")), Some("font/woff2"));
    assert_eq!(
        sniff(&head(b"SQLite format 3\0")),
        Some("application/vnd.sqlite3")
    );
}

#[test]
fn iso_base_media_files_are_separated_by_brand() {
    // MP4, HEIC and AVIF share the same `ftyp` header; only the brand that
    // follows tells them apart.
    assert_eq!(sniff(&head(b"\0\0\0\x18ftypisom")), Some("video/mp4"));
    assert_eq!(sniff(&head(b"\0\0\0\x18ftypavif")), Some("image/avif"));
    assert_eq!(sniff(&head(b"\0\0\0\x18ftypheic")), Some("image/heic"));
    assert_eq!(sniff(&head(b"\0\0\0\x18ftypqt  ")), Some("video/quicktime"));
    assert_eq!(sniff(&head(b"\0\0\0\x18ftypM4A ")), Some("audio/mp4"));
    assert_eq!(sniff(&head(b"\0\0\0\x18ftyp3gp4")), Some("video/3gpp"));
    // An unknown brand still plays as MP4 rather than falling back to bytes.
    assert_eq!(sniff(&head(b"\0\0\0\x18ftypzzzz")), Some("video/mp4"));
}

#[test]
fn riff_and_matroska_containers_are_separated_by_their_inner_tag() {
    assert_eq!(sniff(&head(b"RIFF\0\0\0\0WEBP")), Some("image/webp"));
    assert_eq!(sniff(&head(b"RIFF\0\0\0\0WAVE")), Some("audio/wav"));
    assert_eq!(sniff(&head(b"RIFF\0\0\0\0AVI ")), Some("video/x-msvideo"));
    // An unrecognized RIFF payload is not guessed at.
    assert_eq!(sniff(&head(b"RIFF\0\0\0\0ZZZZ")), None);

    assert_eq!(
        sniff(&head(b"\x1a\x45\xdf\xa3\x01\x00\x00\x00webm")),
        Some("video/webm")
    );
    assert_eq!(
        sniff(&head(b"\x1a\x45\xdf\xa3\x01\x00\x00\x00matroska")),
        Some("video/x-matroska")
    );
}

#[test]
fn tar_is_found_at_its_offset_inside_the_window() {
    let mut bytes = vec![0u8; HEAD_LEN];
    bytes[257..262].copy_from_slice(b"ustar");
    assert_eq!(sniff(&bytes), Some("application/x-tar"));
}

#[test]
fn text_and_markup_are_never_sniffed() {
    // Answering here would be both unreliable and unsafe: labelling an
    // unnamed upload text/html or image/svg+xml on the strength of its bytes
    // turns an inert download into a document that executes in the bucket's
    // own origin.
    assert_eq!(sniff(&head(b"<!DOCTYPE html><html><body>hi")), None);
    assert_eq!(sniff(&head(b"<html>")), None);
    assert_eq!(
        sniff(&head(b"<svg xmlns=\"http://www.w3.org/2000/svg\">")),
        None
    );
    assert_eq!(sniff(&head(b"<?xml version=\"1.0\"?>")), None);
    assert_eq!(sniff(&head(b"body { color: red }")), None);
    assert_eq!(sniff(&head(b"{\"a\": 1}")), None);
    assert_eq!(sniff(&head(b"export const a = 1;")), None);
    assert_eq!(sniff(&head(b"#!/bin/sh\necho hi")), None);
}

#[test]
fn nothing_is_claimed_for_empty_or_truncated_input() {
    assert_eq!(sniff(b""), None);
    // A prefix of a real signature must not match.
    assert_eq!(sniff(b"\x89PN"), None);
    assert_eq!(sniff(b"\0\0\0\x18ftyp"), None);
    assert_eq!(sniff(b"RIFF"), None);
    // Plain zero bytes name no format.
    assert_eq!(sniff(&vec![0u8; HEAD_LEN]), None);
}
