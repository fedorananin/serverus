use serverus_domain::open_policy::{open_as, OpenAs, Platform};

#[test]
fn documents_open_with_their_default_application() {
    for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
        for name in ["notes.txt", "photo.JPG", "index.html", "archive.tar.gz"] {
            assert_eq!(open_as(platform, name, false), OpenAs::Default, "{name}");
        }
    }
}

#[test]
fn macos_launchers_open_as_text() {
    for name in [
        "Calculator.app",
        "deploy.command",
        "fix.TOOL",
        "Evil.terminal",
        "Shortcut.fileloc",
        "Install.pkg",
        "Service.workflow",
        "tool.jar",
    ] {
        assert_eq!(
            open_as(Platform::MacOs, name, false),
            OpenAs::Text,
            "{name}"
        );
    }
}

#[test]
fn macos_scripts_keep_their_editor() {
    // Their default handler is an editor, not Terminal.
    for name in ["deploy.sh", "script.py", "app.js", "script.scpt"] {
        assert_eq!(
            open_as(Platform::MacOs, name, true),
            OpenAs::Default,
            "{name}"
        );
    }
}

#[test]
fn macos_runs_only_extensionless_executables() {
    assert_eq!(open_as(Platform::MacOs, "server", true), OpenAs::Text);
    assert_eq!(open_as(Platform::MacOs, ".bashrc", true), OpenAs::Text);
    assert_eq!(open_as(Platform::MacOs, "server.", true), OpenAs::Text);
    assert_eq!(open_as(Platform::MacOs, "server", false), OpenAs::Default);
    // A known extension decides the handler whatever the mode bits say
    // (OneDrive marks every file executable).
    assert_eq!(open_as(Platform::MacOs, "photo.jpg", true), OpenAs::Default);
}

#[test]
fn windows_runnables_open_as_text() {
    for name in [
        "setup.exe",
        "SETUP.EXE",
        "run.bat",
        "run.cmd",
        "script.vbs",
        "payload.js",
        "page.hta",
        "installer.msi",
        "tweak.reg",
        "link.lnk",
        "setup.exe.",
        "setup.exe ",
    ] {
        assert_eq!(
            open_as(Platform::Windows, name, false),
            OpenAs::Text,
            "{name}"
        );
    }
}

#[test]
fn windows_ignores_mode_bits() {
    assert_eq!(open_as(Platform::Windows, "server", true), OpenAs::Default);
}

#[test]
fn linux_executables_and_launchers_open_as_text() {
    assert_eq!(open_as(Platform::Linux, "server", true), OpenAs::Text);
    assert_eq!(open_as(Platform::Linux, "tool.bin", true), OpenAs::Text);
    assert_eq!(open_as(Platform::Linux, "app.desktop", false), OpenAs::Text);
    assert_eq!(
        open_as(Platform::Linux, "App.AppImage", false),
        OpenAs::Text
    );
    assert_eq!(
        open_as(Platform::Linux, "deploy.sh", false),
        OpenAs::Default
    );
}
