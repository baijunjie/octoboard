//! The version a Qwen home runs. Qwen Code updates itself per home, without touching the npm
//! package: it installs a newer package under `<home>/updates/npm/<launcher key>/versions/<v>`
//! and records it in `active.json` there, and the launcher (`cli-entry.js`) runs that copy for
//! that home (0.28.3). So two accounts can run two versions of one install, and the version is
//! the account's, read here from the files as the launcher reads them, with no process started.
//!
//! The launcher key is the first 16 hex digits of the SHA-256 of the launcher's real path. The
//! copy runs only while `active.json` names a plain version, a `bootstrap` whose real path is the
//! launcher, a `baseVersion` that is the npm package's own and a `bootstrapCtimeMs` that is the
//! launcher's change time in milliseconds (an npm update or reinstall of the package changes the
//! last two), and while the copy's `package.json` names the package and that version; anything
//! else runs the npm package itself (read from 0.28.3's `cli-entry.js`).

use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::super::files::{read_regular_text, SMALL_FILE_LIMIT};
use super::super::{parse_version, AgentVersion};

/// The npm package's name, which its `package.json` and every copy's carry.
const NPM_PACKAGE: &str = "@qwen-code/qwen-code";

/// The version the Qwen home `home` runs of the install whose launcher `binary` resolves to:
/// its own update where one is active for this launcher, else the npm package's. `None` where
/// `binary` is not the package's launcher, nothing tells the version, or an update matches the
/// launcher in all but a change time that cannot be read here: then whether it runs cannot be
/// told, and the caller asks the binary.
pub(super) fn of_home(binary: &str, home: &Path) -> Option<AgentVersion> {
    use crate::platform::shell::{self, ResolvedBinary, Shim};

    let resolved = crate::hostfs::canonical(binary).ok()?;
    let launcher = match shell::shim_package(&ResolvedBinary::new(&resolved), NPM_PACKAGE) {
        Shim::Package(package) => crate::hostfs::canonical(package.join("cli-entry.js")).ok()?,
        _ => resolved,
    };
    let base = base_version(&launcher)?;
    let ctime_ms = crate::platform::fs::change_time_ms(&launcher);
    let version = match updated_version(&launcher, &base, home, ctime_ms) {
        Active::Update(version) => version,
        Active::Package => base,
        Active::Unknown => return None,
    };
    parse_version(release(&version)?)
}

/// Which copy the launcher runs for a home.
#[derive(Debug, PartialEq, Eq)]
enum Active {
    /// Its own update, of this version.
    Update(String),
    /// The npm package itself.
    Package,
    /// An update that matches the launcher in everything but its change time, which cannot be
    /// read.
    Unknown,
}

/// The npm package's version, from the `package.json` beside the launcher or the one above it,
/// as the launcher looks for it; `None` for another package's.
fn base_version(launcher: &Path) -> Option<String> {
    let dir = launcher.parent()?;
    let manifest = [dir.join("package.json"), dir.parent()?.join("package.json")]
        .into_iter()
        .find(|candidate| candidate.is_file())?;
    package_version(&manifest)
}

/// Which copy the launcher runs for `home`. `ctime_ms` is the launcher's change time, `None`
/// where it cannot be read, which leaves an update that matches in all else
/// [`Active::Unknown`]: what cannot be told is taken neither for a match nor for a mismatch.
fn updated_version(launcher: &Path, base: &str, home: &Path, ctime_ms: Option<f64>) -> Active {
    let root = launcher_root(launcher, home);
    let Some(text) = read_regular_text(&root.join("active.json"), SMALL_FILE_LIMIT).ok() else {
        return Active::Package;
    };
    let matched = || -> Option<Option<String>> {
        let active: Value = serde_json::from_str(&text).ok()?;
        let version = active.get("version")?.as_str()?;
        if !plain_version(version) {
            return None;
        }
        let bootstrap = crate::hostfs::canonical(active.get("bootstrap")?.as_str()?).ok()?;
        if bootstrap != launcher || active.get("baseVersion")?.as_str()? != base {
            return None;
        }
        let recorded = recorded_ctime_ms(&text, &active)?;
        let copy = root
            .join("versions")
            .join(version)
            .join("node_modules")
            .join("@qwen-code")
            .join("qwen-code");
        if !copy.join("cli.js").is_file() || package_version(&copy.join("package.json"))? != version
        {
            return None;
        }
        // Compared exactly, as the launcher compares its two numbers.
        match ctime_ms {
            Some(ctime_ms) if recorded == ctime_ms => Some(Some(version.to_owned())),
            Some(_) => None,
            None => Some(None),
        }
    };
    match matched() {
        Some(Some(version)) => Active::Update(version),
        Some(None) => Active::Unknown,
        None => Active::Package,
    }
}

/// `bootstrapCtimeMs` of `active.json`, whose text is `text` and whose value is `active`, read
/// from the number's own text, which the standard library parses to the nearest double, the one
/// Node wrote it from: `serde_json` without its `float_roundtrip` feature may parse it a step off.
/// `None` where it is not a number.
fn recorded_ctime_ms(text: &str, active: &Value) -> Option<f64> {
    active.get("bootstrapCtimeMs")?.as_f64()?;
    const KEY: &str = "\"bootstrapCtimeMs\"";
    let rest = text[text.find(KEY)? + KEY.len()..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let end = rest
        .find(|c: char| !matches!(c, '0'..='9' | '-' | '+' | '.' | 'e' | 'E'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// The directory of `home`'s updates of the install whose launcher is at `launcher`.
fn launcher_root(launcher: &Path, home: &Path) -> PathBuf {
    let digest = Sha256::digest(launcher.to_string_lossy().as_bytes());
    let key: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    home.join("updates").join("npm").join(key)
}

/// The version a `package.json` of the npm package names; `None` for another package's.
fn package_version(manifest: &Path) -> Option<String> {
    let value: Value =
        serde_json::from_str(&read_regular_text(manifest, SMALL_FILE_LIMIT).ok()?).ok()?;
    if value.get("name")?.as_str()? != NPM_PACKAGE {
        return None;
    }
    Some(value.get("version")?.as_str()?.to_owned())
}

/// Whether `version` is one the launcher takes: `major.minor.patch` with an optional pre-release
/// and build suffix of letters, digits, dots and hyphens.
fn plain_version(version: &str) -> bool {
    let (core, suffix) = match version.find(['-', '+']) {
        Some(at) => version.split_at(at),
        None => (version, ""),
    };
    parse_version(core).is_some()
        && suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

/// The release a version names, its pre-release or build suffix left off.
fn release(version: &str) -> Option<&str> {
    version.split(['-', '+']).next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// An npm install of the package at `version` and a Qwen home, in a scratch directory.
    struct Install {
        _scratch: ScratchDir,
        launcher: PathBuf,
        home: PathBuf,
    }

    fn npm_install(name: &str, version: &str) -> Install {
        let scratch = ScratchDir::new(name);
        let package = scratch.join("lib/node_modules/@qwen-code/qwen-code");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join("package.json"),
            format!(r#"{{"name":"@qwen-code/qwen-code","version":"{version}"}}"#),
        )
        .unwrap();
        std::fs::write(package.join("cli-entry.js"), "").unwrap();
        let launcher = crate::hostfs::canonical(package.join("cli-entry.js")).unwrap();
        let home = scratch.join("home/.qwen");
        std::fs::create_dir_all(&home).unwrap();
        Install {
            _scratch: scratch,
            launcher,
            home,
        }
    }

    /// Writes `home`'s update of `version` for the launcher, its copy whole, and `active.json`
    /// as the launcher writes it, the change time given; returns the update's directory.
    fn update(install: &Install, version: &str, base: &str, ctime_ms: f64) -> PathBuf {
        let root = launcher_root(&install.launcher, &install.home);
        let copy = root.join(format!(
            "versions/{version}/node_modules/@qwen-code/qwen-code"
        ));
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::write(
            copy.join("package.json"),
            format!(r#"{{"name":"@qwen-code/qwen-code","version":"{version}"}}"#),
        )
        .unwrap();
        std::fs::write(copy.join("cli.js"), "").unwrap();
        std::fs::write(
            root.join("active.json"),
            serde_json::json!({
                "version": version,
                "bootstrap": install.launcher.to_string_lossy(),
                "baseVersion": base,
                "bootstrapCtimeMs": ctime_ms,
            })
            .to_string(),
        )
        .unwrap();
        root
    }

    fn active(install: &Install, ctime_ms: Option<f64>) -> Active {
        updated_version(&install.launcher, "0.28.0", &install.home, ctime_ms)
    }

    fn update_of(version: &str) -> Active {
        Active::Update(version.to_string())
    }

    /// The directory of a home's updates is named by the first 16 hex digits of the SHA-256 of
    /// the launcher's real path (0.28.3: `3fe0cacc706db533` for the launcher measured).
    #[test]
    fn a_homes_updates_are_named_by_the_launchers_path() {
        let root = launcher_root(
            Path::new("/opt/lib/node_modules/@qwen-code/qwen-code/cli-entry.js"),
            Path::new("/h/.qwen"),
        );
        let digest = Sha256::digest(b"/opt/lib/node_modules/@qwen-code/qwen-code/cli-entry.js");
        let expected: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(root, Path::new("/h/.qwen/updates/npm").join(expected));
        assert_eq!(root.file_name().unwrap().len(), 16);
    }

    /// An update whose record matches the launcher is the home's version; a record that names
    /// another base version, another change time, another launcher or a copy that is not whole
    /// runs the package itself, as does a home with no update; one that matches in all but a change
    /// time that cannot be read is not told.
    #[test]
    fn only_an_update_that_matches_the_launcher_is_active() {
        let install = npm_install("qwen-version-active", "0.28.0");
        assert_eq!(active(&install, Some(1.5)), Active::Package, "no update");
        assert_eq!(
            active(&install, None),
            Active::Package,
            "no update, no time"
        );

        let root = update(&install, "0.28.3", "0.28.0", 1795100767270.9033);
        assert_eq!(
            active(&install, Some(1795100767270.9033)),
            update_of("0.28.3")
        );
        // A millisecond later: at this size an f64's step is about 0.0002, so a change in the
        // fourth decimal place could round to the same number.
        assert_eq!(
            active(&install, Some(1795100767271.9033)),
            Active::Package,
            "changed"
        );
        assert_eq!(
            active(&install, None),
            Active::Unknown,
            "change time unknown"
        );

        update(&install, "0.28.3", "0.27.9", 1795100767270.9033);
        assert_eq!(
            active(&install, Some(1795100767270.9033)),
            Active::Package,
            "base"
        );
        assert_eq!(active(&install, None), Active::Package, "base, no time");

        update(&install, "0.28.3", "0.28.0", 1795100767270.9033);
        std::fs::remove_file(root.join("versions/0.28.3/node_modules/@qwen-code/qwen-code/cli.js"))
            .unwrap();
        assert_eq!(
            active(&install, Some(1795100767270.9033)),
            Active::Package,
            "copy"
        );

        let other = npm_install("qwen-version-other", "0.28.0");
        update(&install, "0.28.3", "0.28.0", 1795100767270.9033);
        assert_eq!(
            updated_version(
                &other.launcher,
                "0.28.0",
                &install.home,
                Some(1795100767270.9033)
            ),
            Active::Package,
            "another launcher's key names another directory"
        );
    }

    /// The recorded change time is read from the number's own text, to the nearest double: written
    /// as Node writes it or with digits that name the same double, it matches the change time
    /// exactly; one step of the double off, it does not.
    #[test]
    fn the_recorded_change_time_is_compared_exactly_from_its_text() {
        let ctime = 1795100767270.9033_f64;
        for text in [
            "1795100767270.9033",
            "1795100767270.90330",
            "1.7951007672709033e12",
        ] {
            let json = format!(r#"{{"version":"0.28.3", "bootstrapCtimeMs" : {text}}}"#);
            let value: Value = serde_json::from_str(&json).unwrap();
            assert_eq!(recorded_ctime_ms(&json, &value), Some(ctime), "{text}");
        }
        let next = f64::from_bits(ctime.to_bits() + 1);
        let json = format!(r#"{{"bootstrapCtimeMs":{next}}}"#);
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(recorded_ctime_ms(&json, &value), Some(next));
        assert_ne!(next, ctime);
        let json = r#"{"bootstrapCtimeMs":"1795100767270.9033"}"#;
        let value: Value = serde_json::from_str(json).unwrap();
        assert_eq!(recorded_ctime_ms(json, &value), None, "not a number");
    }

    /// The version a home runs is its active update's, else the package's; a binary that is not
    /// the package's launcher tells none.
    #[test]
    fn a_home_runs_its_update_or_else_the_package() {
        let install = npm_install("qwen-version-home", "0.28.0");
        let binary = install.launcher.to_string_lossy().into_owned();
        assert_eq!(of_home(&binary, &install.home), Some((0, 28, 0)));

        let ctime = crate::platform::fs::change_time_ms(&install.launcher).unwrap();
        update(&install, "0.28.3", "0.28.0", ctime);
        assert_eq!(of_home(&binary, &install.home), Some((0, 28, 3)));

        let other_home = install.home.with_file_name("other");
        assert_eq!(of_home(&binary, &other_home), Some((0, 28, 0)));

        assert_eq!(of_home("/no/such/qwen", &install.home), None);
    }

    #[test]
    fn a_version_the_launcher_takes_is_plain() {
        assert!(plain_version("0.28.3"));
        assert!(plain_version("0.29.0-preview.1"));
        assert!(plain_version("1.0.0+build.5"));
        assert!(!plain_version("0.28"));
        assert!(!plain_version("../0.28.3"));
        assert!(!plain_version("0.28.3/x"));
    }
}
