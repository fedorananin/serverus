//! Whether the system has an application registered for a file.
//!
//! Every lookup answers `true` when it can't tell — an unknown answer must
//! never send a document away from its app — so only a clear "nothing opens
//! this" routes a file to the fallback editor.

use std::path::Path;

/// macOS: Launch Services' choice for the file, as Finder would make it.
#[cfg(target_os = "macos")]
pub(super) fn has_default_app(path: &Path) -> bool {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    let Some(path) = path.to_str() else {
        return true;
    };
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    NSWorkspace::sharedWorkspace()
        .URLForApplicationToOpenURL(&url)
        .is_some()
}

/// Windows: the shell's association for the extension. Only an explicit
/// "no association" counts as none; `ASSOCF_INIT_IGNOREUNKNOWN` keeps the
/// "Open with…" catch-all from answering for unregistered types.
#[cfg(target_os = "windows")]
pub(super) fn has_default_app(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        AssocQueryStringW, ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_FRIENDLYAPPNAME,
    };

    /// `HRESULT_FROM_WIN32(ERROR_NO_ASSOCIATION)`.
    const NO_ASSOCIATION: i32 = 0x8007_0483_u32 as i32;

    let Some(extension) = path.extension() else {
        return false;
    };
    let mut dotted = std::ffi::OsString::from(".");
    dotted.push(extension);
    let wide: Vec<u16> = dotted.encode_wide().chain(Some(0)).collect();
    let mut length = 0u32;
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the
    // call; with no output buffer the call only writes the length.
    let result = unsafe {
        AssocQueryStringW(
            ASSOCF_INIT_IGNOREUNKNOWN,
            ASSOCSTR_FRIENDLYAPPNAME,
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            None,
            &mut length,
        )
    };
    result.0 != NO_ASSOCIATION
}

/// Linux: the desktop's default handler for the file's MIME type
/// (`xdg-mime`). No `xdg-mime`, or no type, means we can't tell.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn has_default_app(path: &Path) -> bool {
    use std::ffi::OsStr;

    fn query(args: &[&OsStr]) -> Option<String> {
        let output = std::process::Command::new("xdg-mime")
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    let filetype = query(&[
        OsStr::new("query"),
        OsStr::new("filetype"),
        path.as_os_str(),
    ]);
    let Some(mime) = filetype.filter(|mime| !mime.is_empty()) else {
        return true;
    };
    match query(&[
        OsStr::new("query"),
        OsStr::new("default"),
        OsStr::new(&mime),
    ]) {
        Some(handler) => !handler.is_empty(),
        None => true,
    }
}
