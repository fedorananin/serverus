//! Whether handing a file to the OS opener ("system default editor") would
//! run it instead of showing it.
//!
//! `open` (macOS), `explorer` (Windows) and `xdg-open` (Linux) launch
//! programs, scripts, installers and shortcuts just like a double click in
//! the file manager. Serverus only ever opens a file to look at or edit it,
//! and files in its panes often came straight off a server with no
//! quarantine flag, so anything the opener would execute is opened as plain
//! text instead.
//!
//! The lists name what the platform's default handlers *run*. Scripts whose
//! default handler is an editor (`.sh`, `.py` on macOS) are left alone so
//! they keep opening in the user's IDE.

/// The desktop platform whose opener semantics apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

/// How a file may be handed to the system opener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAs {
    /// The default application for the file's type.
    Default,
    /// A text editor: the default handler would execute the file.
    Text,
}

/// Extensions macOS launches: apps, Terminal scripts and settings (which can
/// carry a command), Automator bundles, installers, Java archives, and
/// location files that can point at a local program.
const MACOS: &[&str] = &[
    "app",
    "command",
    "tool",
    "terminal",
    "action",
    "caction",
    "definition",
    "workflow",
    "pkg",
    "mpkg",
    "jar",
    "fileloc",
    "inetloc",
    "webloc",
    "url",
];

/// Extensions Windows Explorer runs: executables, installers, Windows Script
/// Host scripts, HTML applications, control panel items, registry merges,
/// shortcuts and app deployment manifests.
const WINDOWS: &[&str] = &[
    "exe",
    "com",
    "scr",
    "pif",
    "cpl",
    "msc",
    "bat",
    "cmd",
    "ps1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    "msi",
    "msp",
    "mst",
    "msix",
    "msixbundle",
    "appx",
    "appxbundle",
    "appinstaller",
    "application",
    "appref-ms",
    "gadget",
    "reg",
    "inf",
    "lnk",
    "url",
    "scf",
    "settingcontent-ms",
    "jar",
];

/// Extensions Linux desktops launch: desktop entries, self-contained
/// binaries and installers, Java archives.
const LINUX: &[&str] = &["desktop", "appimage", "run", "jar"];

/// How `name` may be opened on `platform`. `unix_executable` is whether any
/// execute bit is set on the file (always false on Windows).
///
/// macOS runs an executable in Terminal only when its name has no extension
/// (a dotfile counts as having none); a known extension decides the handler
/// regardless of the mode bits. Linux openers sniff content when the name
/// says nothing reliable, so any executable file there opens as text.
pub fn open_as(platform: Platform, name: &str, unix_executable: bool) -> OpenAs {
    let extension = extension(name);
    let listed = extension.as_deref().is_some_and(|extension| {
        let list = match platform {
            Platform::MacOs => MACOS,
            Platform::Windows => WINDOWS,
            Platform::Linux => LINUX,
        };
        list.contains(&extension)
    });
    let runs_by_mode = match platform {
        Platform::MacOs => unix_executable && extension.is_none(),
        Platform::Windows => false,
        Platform::Linux => unix_executable,
    };
    if listed || runs_by_mode {
        OpenAs::Text
    } else {
        OpenAs::Default
    }
}

/// The lowercase extension of `name`, or `None` for a bare name, a dotfile
/// (`.bashrc`) or a trailing dot. Windows ignores trailing dots and spaces in
/// file names, so they are trimmed first: `setup.exe.` is still an `.exe`.
fn extension(name: &str) -> Option<String> {
    let name = name.trim_end_matches(['.', ' ']);
    let (stem, extension) = name.rsplit_once('.')?;
    if stem.is_empty() || extension.is_empty() {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}
