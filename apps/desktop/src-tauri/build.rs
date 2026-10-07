use std::fs;

fn read_json(path: &str) -> serde_json::Value {
    println!("cargo:rerun-if-changed={path}");
    serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}")))
        .unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn main() {
    // Exposes the app name from the repo-root `config/app.json` as `OCTOBOARD_APP_NAME`, and fails
    // the build when a copy of it that Tauri or Cargo force to live elsewhere has drifted: the
    // `productName` in `tauri.conf.json` and the workspace `repository` in the root `Cargo.toml`.
    let config = read_json("../../../config/app.json");
    let name = config["name"]
        .as_str()
        .expect("config/app.json `name` is a string");
    let repository_url = config["repositoryUrl"]
        .as_str()
        .expect("config/app.json `repositoryUrl` is a string");

    let tauri_conf = read_json("tauri.conf.json");
    let product_name = tauri_conf["productName"].as_str();
    assert_eq!(
        product_name,
        Some(name),
        "tauri.conf.json `productName` must equal config/app.json `name` ({name:?})"
    );
    let repository = std::env::var("CARGO_PKG_REPOSITORY").unwrap_or_default();
    assert_eq!(
        repository, repository_url,
        "the workspace `repository` in the root Cargo.toml must equal config/app.json `repositoryUrl`"
    );

    println!("cargo:rustc-env=OCTOBOARD_APP_NAME={name}");
    tauri_build::build()
}
