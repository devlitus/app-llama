use std::path::PathBuf;
use tauri_build::{Attributes, WindowsAttributes};

fn main() {
    let attributes = if target_is_msvc() {
        embed_manifest_in_every_target();
        let windows = WindowsAttributes::new_without_app_manifest();
        Attributes::new().windows_attributes(windows)
    } else {
        Attributes::new()
    };
    if let Err(error) = tauri_build::try_build(attributes) {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn target_is_msvc() -> bool {
    std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc")
}

// tauri-build solo incrusta su manifiesto en el binario, y sin Common Controls v6
// el ejecutable de los tests de la lib no arranca en Windows (STATUS_ENTRYPOINT_NOT_FOUND).
// Se incrusta el mismo manifiesto en todos los destinos con un único enlace.
fn embed_manifest_in_every_target() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("windows-app.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
