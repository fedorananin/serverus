fn main() {
    // Windows manifest handling: tauri-build normally embeds the Common
    // Controls v6 manifest through a winres resource, but that reaches only
    // the main binary (`rustc-link-arg-bins`) — test executables get no
    // manifest, load comctl32 v5 and abort at startup with
    // STATUS_ENTRYPOINT_NOT_FOUND (TaskDialogIndirect is v6-only). So the
    // winres manifest is disabled and the same manifest is embedded via
    // plain `rustc-link-arg`, which covers every link target: the app,
    // unit-test and integration-test binaries.
    // https://github.com/tauri-apps/tauri/issues/13419
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        // Linker warnings become errors: a broken manifest must fail the
        // build, not resurface as a startup crash.
        println!("cargo:rustc-link-arg=/WX");
    }
}
