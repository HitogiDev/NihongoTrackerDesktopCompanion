fn main() {
    // Tauri embeds its app manifest (Common Controls v6) only into the app
    // binary. Test binaries then load comctl32 v5, which has no
    // `TaskDialogIndirect`, and Windows refuses to start them
    // (STATUS_ENTRYPOINT_NOT_FOUND) as soon as that import is linked in. So
    // the same manifest is embedded by the linker into every binary instead.
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let attributes = if msvc {
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        tauri_build::Attributes::new()
    };
    if msvc {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
