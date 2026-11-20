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

mod opener {
    use serverus_domain::open_policy::{choose_opener, Opener, Platform};

    const ALL: [Platform; 3] = [Platform::MacOs, Platform::Windows, Platform::Linux];

    #[test]
    fn documents_with_an_app_keep_it_even_with_a_fallback_set() {
        for platform in ALL {
            for fallback in [false, true] {
                assert_eq!(
                    choose_opener(platform, "report.pdf", false, fallback, || true),
                    Opener::SystemDefault
                );
            }
        }
    }

    #[test]
    fn files_no_app_opens_go_to_the_fallback_editor() {
        for platform in ALL {
            assert_eq!(
                choose_opener(platform, "data.xyz", false, true, || false),
                Opener::Fallback
            );
            assert_eq!(
                choose_opener(platform, "data.xyz", false, false, || false),
                Opener::SystemText
            );
        }
    }

    #[test]
    fn names_without_an_extension_never_ask_the_system() {
        for platform in ALL {
            for name in ["Makefile", "README", ".bashrc", ".env", "notes."] {
                let opener = choose_opener(platform, name, false, true, || {
                    panic!("{name}: the name alone decides")
                });
                assert_eq!(opener, Opener::Fallback, "{name}");
            }
        }
    }

    #[test]
    fn files_that_would_run_open_as_text_in_the_fallback_editor() {
        let cases = [
            (Platform::MacOs, "deploy.command", false),
            (Platform::MacOs, "server", true),
            (Platform::Windows, "setup.exe", false),
            (Platform::Linux, "build.sh", true),
        ];
        for (platform, name, executable) in cases {
            let ask = || panic!("{name}: would run, no need to ask");
            assert_eq!(
                choose_opener(platform, name, executable, true, ask),
                Opener::Fallback,
                "{name}"
            );
            assert_eq!(
                choose_opener(platform, name, executable, false, || true),
                Opener::SystemText,
                "{name}"
            );
        }
    }
}
