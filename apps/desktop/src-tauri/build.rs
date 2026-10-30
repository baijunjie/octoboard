use std::fs;

fn read_json(path: &str) -> serde_json::Value {
    println!("cargo:rerun-if-changed={path}");
    serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}")))
        .unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn main() {
    // Exposes the app name and the repository address from the repo-root `config/app.json` as
    // `OCTOBOARD_APP_NAME` and `OCTOBOARD_REPOSITORY_URL`, and fails the build when a copy of it
    // that Tauri or Cargo force to live elsewhere has drifted: the `productName` in
    // `tauri.conf.json` and the workspace `repository` and `license` in the root `Cargo.toml`.
    // It also fails it when the application's version has two values: `tauri.conf.json` reads it
    // from `apps/desktop/package.json`, and the root `Cargo.toml` repeats it as the workspace
    // version both crates take (the daemon reports it in the configuration export and to the agent
    // CLIs it talks to).
    let config = read_json("../../../config/app.json");
    let name = config["name"]
        .as_str()
        .expect("config/app.json `name` is a string");
    let repository_url = config["repositoryUrl"]
        .as_str()
        .expect("config/app.json `repositoryUrl` is a string");

    let license = config["license"]
        .as_str()
        .expect("config/app.json `license` is a string");

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

    assert_eq!(
        std::env::var("CARGO_PKG_LICENSE").unwrap_or_default(),
        license,
        "the workspace `license` in the root Cargo.toml must equal config/app.json `license`"
    );

    assert_eq!(
        tauri_conf["version"].as_str(),
        Some("../package.json"),
        "tauri.conf.json `version` must be \"../package.json\", which holds the application's version"
    );
    let version = read_json("../package.json")["version"]
        .as_str()
        .expect("apps/desktop/package.json `version` is a string")
        .to_owned();
    assert_eq!(
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
        version,
        "the workspace `version` in the root Cargo.toml must equal apps/desktop/package.json `version`"
    );
    let daemon_manifest = "../../daemon/Cargo.toml";
    println!("cargo:rerun-if-changed={daemon_manifest}");
    assert!(
        fs::read_to_string(daemon_manifest)
            .unwrap_or_else(|e| panic!("read {daemon_manifest}: {e}"))
            .lines()
            .any(|line| line.trim() == "version.workspace = true"),
        "apps/daemon/Cargo.toml must take the workspace version (`version.workspace = true`)"
    );

    println!("cargo:rustc-env=OCTOBOARD_APP_NAME={name}");
    // Where the update manifests are published (`src/updater.rs`).
    println!("cargo:rustc-env=OCTOBOARD_REPOSITORY_URL={repository_url}");
    tauri_build::build()
}
