fn main() {
    tauri_build::build();

    // Windows: tauri-build embeds an app manifest (Common Controls v6) into
    // the main binary only. Test executables link the same crates but get no
    // manifest, load comctl32 v5 and die at startup with
    // STATUS_ENTRYPOINT_NOT_FOUND. Embed a minimal manifest into them too —
    // `-tests` scope, so the app binary keeps the tauri-build one.
    // https://github.com/tauri-apps/tauri/pull/4383#issuecomment-1212221864
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-test-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
