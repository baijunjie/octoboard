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
}
