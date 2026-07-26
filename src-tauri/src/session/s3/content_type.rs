//! `Content-Type` for uploaded objects.
//!
//! The AWS SDK labels every request body `application/octet-stream` unless a
//! type is set explicitly — it never infers one from the key. Browsers refuse
//! a stylesheet or an ES module served under that type, so uploading a site's
//! assets through the panel used to break the site silently: the bytes arrive
//! intact and nothing loads. Every upload path therefore declares a type.
//!
//! Two sources answer, in this order:
//!
//! 1. [`guess_content_type`] — the object's own name, via [`table`]. This is
//!    authoritative: the name is what the publisher intended and what a CDN
//!    contract is written against, and the text formats that break sites have
//!    no byte signature to find anyway.
//! 2. [`sniff`] — magic bytes, consulted only when the name resolves to
//!    nothing. It recognizes unambiguous binary formats and deliberately
//!    never answers with a document or script type; see that module for why.
//!
//! Whatever both decline keeps the `application/octet-stream` default, which
//! is already the right answer for an unrecognized binary.

mod sniff;
mod table;

pub(super) use sniff::{sniff, HEAD_LEN};

/// The media type `key` should be uploaded under from its name alone, or
/// `None` when the extension is unknown or absent.
///
/// `key` may be either an object key or a panel path — only the last segment
/// is inspected.
pub(super) fn guess_content_type(key: &str) -> Option<&'static str> {
    let name = key.rsplit('/').next()?;
    let (stem, extension) = name.rsplit_once('.')?;
    // `.htaccess` is a dotfile whose whole name follows the dot, not an
    // extension — and a bare trailing dot names no type either.
    if stem.is_empty() || extension.is_empty() {
        return None;
    }
    let extension = extension.to_ascii_lowercase();
    table::entries()
        .find(|(extensions, _)| extensions.contains(&extension.as_str()))
        .map(|(_, media_type)| *media_type)
}

#[cfg(test)]
#[path = "content_type_tests.rs"]
mod tests;
