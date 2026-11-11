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
    embed_ui();
}

/// Writes `embedded_ui.rs` into the build's output directory: the table of the UI build's files
/// (`src/server/ui.rs`), each `include_bytes!`'d, with the `embedded-ui` feature, and an empty one
/// without. The build is `packages/ui/dist`, which the workspace package builds first
/// (`pnpm --filter @octoboard/daemon... build`); a build with the feature and no UI build there
/// fails rather than ship a binary with no UI.
///
/// The UI build bakes `VITE_DAEMON_KEY` and `VITE_DAEMON_PORT` from its environment into the
/// bundle, which the daemon serves without the key; a build with the feature refuses while either
/// is set, since the same command builds the UI first.
fn embed_ui() {
    let out = std::path::Path::new(&std::env::var("OUT_DIR").expect("set by cargo"))
        .join("embedded_ui.rs");
    let mut table = String::from("pub(super) static FILES: &[(&str, &[u8])] = &[\n");
    if std::env::var_os("CARGO_FEATURE_EMBEDDED_UI").is_some() {
        for baked in ["VITE_DAEMON_KEY", "VITE_DAEMON_PORT"] {
            println!("cargo:rerun-if-env-changed={baked}");
            assert!(
                std::env::var_os(baked).is_none(),
                "unset {baked}: the UI build bakes it into the bundle the daemon serves without its key"
            );
        }
        let dist =
            std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
                .join("../../packages/ui/dist");
        println!("cargo:rerun-if-changed={}", dist.display());
        assert!(
            dist.join("index.html").is_file(),
            "the `embedded-ui` feature embeds the UI build, and {} has none: build it first, \
             or build through `pnpm --filter @octoboard/daemon... build`",
            dist.display()
        );
        let mut files = Vec::new();
        collect_files(&dist, &dist, &mut files);
        files.sort();
        for (name, path) in files {
            table.push_str(&format!("    ({name:?}, include_bytes!({path:?})),\n"));
        }
    }
    table.push_str("];\n");
    fs::write(&out, table).expect("write embedded_ui.rs");
}

/// Every file below `dir`, as its path relative to `root` with `/` separators and its path,
/// absolute as `root` is.
fn collect_files(root: &std::path::Path, dir: &std::path::Path, files: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).expect("read the UI build") {
        let path = entry.expect("read the UI build").path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).expect("below the build directory");
            let name = relative
                .components()
                .map(|part| part.as_os_str().to_str().expect("a UTF-8 file name"))
                .collect::<Vec<_>>()
                .join("/");
            files.push((name, path.to_str().expect("a UTF-8 path").to_string()));
        }
    }
}

/// Links `octoboardd.exe.manifest`, which declares the daemon long-path aware and asks for no
/// elevation, into the Windows MSVC binary as its manifest resource (`RT_MANIFEST`, ID 1): it is
/// written into a compiled resource file (`.res`) here and handed to the linker as an input, with
/// `/MANIFEST:NO` so the linker makes no manifest of its own. `link.exe` and `lld-link` both take a
/// `.res` input as it is; having the linker merge the file in (`/MANIFESTINPUT`) instead needs a
/// linker built with libxml2 or `mt.exe` on the `PATH`, which `cargo xwin`'s `lld-link` lacks.
/// Other targets link no manifest.
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
    let xml = fs::read(&manifest).expect("read octoboardd.exe.manifest");
    let res = std::path::Path::new(&std::env::var("OUT_DIR").expect("set by cargo"))
        .join("octoboardd-manifest.res");
    fs::write(&res, manifest_resource(&xml)).expect("write octoboardd-manifest.res");
    println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

/// A compiled resource file (the `.res` format `rc.exe` writes) holding `xml` as the manifest
/// resource: the empty entry every such file starts with, then one entry of type `RT_MANIFEST` (24)
/// and ID 1, the ID of an executable's manifest, in US English as linkers write it.
fn manifest_resource(xml: &[u8]) -> Vec<u8> {
    fn entry(out: &mut Vec<u8>, kind: u16, id: u16, memory_flags: u16, language: u16, data: &[u8]) {
        // DataSize and HeaderSize (32: these two, the type and the name, then the fixed fields).
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(32u32.to_le_bytes());
        // The type and the name, each an ordinal: 0xFFFF and the number.
        for half in [0xFFFF, kind, 0xFFFF, id] {
            out.extend(half.to_le_bytes());
        }
        // DataVersion, MemoryFlags, LanguageId, Version and Characteristics.
        out.extend(0u32.to_le_bytes());
        out.extend(memory_flags.to_le_bytes());
        out.extend(language.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    let mut out = Vec::new();
    entry(&mut out, 0, 0, 0, 0, &[]);
    // MOVEABLE | PURE | DISCARDABLE, as rc.exe marks a manifest; 0x0409 is US English.
    entry(&mut out, 24, 1, 0x1030, 0x0409, xml);
    out
}
