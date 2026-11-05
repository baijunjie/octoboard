use std::fs;

fn main() {
    // Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME`.
    let path = "../../config/app.json";
    println!("cargo:rerun-if-changed={path}");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).expect("read config/app.json"))
            .expect("parse config/app.json");
    let name = config["name"]
        .as_str()
        .expect("config/app.json `name` is a string");
    println!("cargo:rustc-env=OCTOBOARD_APP_NAME={name}");

    embed_windows_manifest();
}

/// Has the MSVC linker embed `octoboardd.exe.manifest`, which declares the daemon long-path aware,
/// in the Windows binary: the linker merges it into the manifest it makes, so no resource compiler
/// or crate is needed. Other targets link no manifest.
fn embed_windows_manifest() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }
    let manifest =
        std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
            .join("octoboardd.exe.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
