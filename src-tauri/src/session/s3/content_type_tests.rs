use std::collections::HashSet;

use super::guess_content_type;
use super::table::{entries, ABSENT_BY_DESIGN};

#[test]
fn web_assets_get_the_types_browsers_require() {
    assert_eq!(
        guess_content_type("assets/app.css"),
        Some("text/css; charset=utf-8")
    );
    assert_eq!(
        guess_content_type("assets/app.js"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        guess_content_type("assets/app.mjs"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(guess_content_type("fonts/inter.woff2"), Some("font/woff2"));
    assert_eq!(guess_content_type("logo.svg"), Some("image/svg+xml"));
    assert_eq!(guess_content_type("app.wasm"), Some("application/wasm"));
}

#[test]
fn office_documents_get_their_full_openxml_types() {
    assert_eq!(guess_content_type("report.doc"), Some("application/msword"));
    assert_eq!(
        guess_content_type("report.docx"),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
    );
    assert_eq!(
        guess_content_type("budget.xls"),
        Some("application/vnd.ms-excel")
    );
    assert_eq!(
        guess_content_type("budget.xlsx"),
        Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
    );
    assert_eq!(
        guess_content_type("deck.pptx"),
        Some("application/vnd.openxmlformats-officedocument.presentationml.presentation")
    );
    assert_eq!(
        guess_content_type("notes.odt"),
        Some("application/vnd.oasis.opendocument.text")
    );
}

#[test]
fn media_and_image_formats_are_covered_beyond_the_web_set() {
    assert_eq!(guess_content_type("clip.avi"), Some("video/x-msvideo"));
    assert_eq!(guess_content_type("clip.wmv"), Some("video/x-ms-wmv"));
    assert_eq!(guess_content_type("clip.flv"), Some("video/x-flv"));
    assert_eq!(guess_content_type("clip.3gp"), Some("video/3gpp"));
    assert_eq!(guess_content_type("clip.mkv"), Some("video/x-matroska"));
    assert_eq!(guess_content_type("photo.heic"), Some("image/heic"));
    assert_eq!(guess_content_type("photo.jxl"), Some("image/jxl"));
    assert_eq!(
        guess_content_type("art.psd"),
        Some("image/vnd.adobe.photoshop")
    );
    assert_eq!(guess_content_type("shot.cr3"), Some("image/x-canon-cr3"));
    assert_eq!(guess_content_type("song.alac"), Some("audio/x-alac"));
    assert_eq!(guess_content_type("song.wma"), Some("audio/x-ms-wma"));
}

#[test]
fn streaming_and_subtitle_sidecars_are_typed() {
    // A playlist served as octet-stream is the classic reason HLS from a
    // bucket refuses to start.
    assert_eq!(
        guess_content_type("stream/index.m3u8"),
        Some("application/vnd.apple.mpegurl")
    );
    assert_eq!(
        guess_content_type("stream/manifest.mpd"),
        Some("application/dash+xml")
    );
    assert_eq!(
        guess_content_type("captions.vtt"),
        Some("text/vtt; charset=utf-8")
    );
}

#[test]
fn ts_resolves_to_the_transport_stream_not_typescript() {
    // HLS segments are what gets served from object storage; typing them as
    // text would break playback.
    assert_eq!(
        guess_content_type("stream/segment-001.ts"),
        Some("video/mp2t")
    );
}

#[test]
fn extension_matching_ignores_case_and_leading_path() {
    assert_eq!(
        guess_content_type("/bucket/deep/dir/STYLE.CSS"),
        Some("text/css; charset=utf-8")
    );
    assert_eq!(guess_content_type("IMG.JPEG"), Some("image/jpeg"));
}

#[test]
fn multiple_dots_use_the_last_extension() {
    assert_eq!(
        guess_content_type("app.min.css"),
        Some("text/css; charset=utf-8")
    );
    assert_eq!(guess_content_type("app.js.map"), Some("application/json"));
    assert_eq!(
        guess_content_type("backup.tar.gz"),
        Some("application/gzip")
    );
}

#[test]
fn unknown_and_missing_extensions_send_no_header() {
    assert_eq!(guess_content_type("archive.unknownext"), None);
    assert_eq!(guess_content_type("Makefile"), None);
    assert_eq!(guess_content_type("dir/"), None);
    assert_eq!(guess_content_type(""), None);
    assert_eq!(guess_content_type("trailing."), None);
}

#[test]
fn dotfiles_are_not_extensions() {
    // `.env` would otherwise be served as an "env" type; more importantly
    // `.htaccess` must not be mistaken for an `htaccess` extension.
    assert_eq!(guess_content_type(".htaccess"), None);
    assert_eq!(guess_content_type("config/.env"), None);
}

#[test]
fn a_directory_placeholder_key_has_no_type() {
    // `mkdir` writes the zero-byte `dir/` marker; it must stay untyped
    // even when the folder name looks like a file.
    assert_eq!(guess_content_type("assets/app.css/"), None);
}

#[test]
fn extensions_left_out_on_purpose_stay_out() {
    for (extension, reason) in ABSENT_BY_DESIGN {
        assert_eq!(
            guess_content_type(&format!("file.{extension}")),
            None,
            "`.{extension}` is listed as absent by design ({reason}) but the \
             table now answers for it"
        );
    }
}

#[test]
fn the_table_is_lowercase_and_free_of_duplicates() {
    let mut seen = HashSet::new();
    for (extensions, media_type) in entries() {
        assert!(
            !extensions.is_empty(),
            "media type {media_type} lists no extensions"
        );
        assert!(
            !media_type.is_empty(),
            "extensions {extensions:?} have an empty media type"
        );
        for extension in *extensions {
            assert_eq!(
                *extension,
                extension.to_ascii_lowercase(),
                "extension {extension} must be lowercase to match the lookup"
            );
            assert!(
                !extension.starts_with('.'),
                "extension {extension} must not carry a leading dot"
            );
            assert!(
                seen.insert(*extension),
                "extension {extension} is listed twice; the lookup would \
                 silently resolve it by table order"
            );
        }
    }
}
