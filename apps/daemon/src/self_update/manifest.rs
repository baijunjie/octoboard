//! The release manifest the update check reads, and the release it names for this machine.
//!
//! The manifest is `{repositoryUrl}/releases/download/updater/{stable|beta}-linux.json`, written by
//! `apps/desktop/scripts/release-manifest.mjs`: the version, its notes and date, the holder floor
//! and, per platform key (`linux-x86_64`, `linux-aarch64`), the archive's address, its SHA-256 and
//! the signature of its `.sig` file. Nothing in it is trusted beyond what the archive's own checks
//! confirm (`verify`): it is plain HTTPS, so the version and floor it says are only shown to the
//! user, and the archive it points at has to carry a signature of the release key.

use serde::Deserialize;

use crate::holder::compat::Version;
use crate::protocol::DaemonUpdateChannel;

/// The repository releases are published in, from `config/app.json` (see `build.rs`).
const REPOSITORY_URL: &str = env!("OCTOBOARD_REPOSITORY_URL");

/// The website, from `config/app.json`, which serves the install script.
const WEBSITE_URL: &str = env!("OCTOBOARD_WEBSITE_URL");

/// The command that installs the latest release over an installation by hand, which the user is
/// told where the daemon cannot replace its own executable.
pub fn install_command() -> String {
    format!("curl -fsSL {WEBSITE_URL}/install.sh | sh")
}

/// The address of the manifest of `channel`.
pub fn manifest_url(channel: DaemonUpdateChannel) -> String {
    format!(
        "{REPOSITORY_URL}/releases/download/updater/{}-linux.json",
        channel.wire_name()
    )
}

/// The manifest's key for this machine's release: `linux-` and the architecture the daemon was
/// built for.
pub fn platform_key() -> String {
    format!("linux-{}", std::env::consts::ARCH)
}

/// The name of the release archive of `version` for `arch`, which the signature names too.
pub fn archive_name(version: &str, arch: &str) -> String {
    format!("octoboardd_{version}_linux_{arch}.tar.gz")
}

/// A release the manifest offers for this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub notes: Option<String>,
    pub holder_floor: Option<String>,
    /// Where the archive is, an `https` address.
    pub url: String,
    /// The content of the archive's `.sig` file, as the manifest carries it.
    pub signature: String,
    /// The archive's SHA-256, lowercase hex.
    pub sha256: String,
    /// The channel whose manifest named it.
    pub channel: DaemonUpdateChannel,
}

/// Whether the version `candidate` is strictly newer than `running`, by semantic-version
/// precedence. A version that does not read as one is never newer.
pub fn is_newer(candidate: &str, running: &str) -> bool {
    matches!(
        (Version::parse(candidate), Version::parse(running)),
        (Some(candidate), Some(running)) if candidate > running
    )
}

/// Whether `version` is made of characters a version has and a file name can carry: it is put into
/// the name of the archive and the path inside it, so nothing else (`/`, `..` and the like) may
/// come from a manifest.
fn is_plain(version: &str) -> bool {
    !version.is_empty()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
}

impl Release {
    /// Whether this release is strictly newer than the build `running`.
    pub fn is_newer_than(&self, running: &str) -> bool {
        is_newer(&self.version, running)
    }
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    holder_floor: Option<String>,
    platforms: std::collections::HashMap<String, Entry>,
}

#[derive(Deserialize)]
struct Entry {
    url: String,
    signature: String,
    sha256: String,
}

/// The release `body` (the manifest of `channel`) offers for the platform `key`. The error says what is wrong
/// with it, in English, for the check's detail.
pub fn parse(body: &str, key: &str, channel: DaemonUpdateChannel) -> Result<Release, String> {
    let manifest: Manifest =
        serde_json::from_str(body).map_err(|err| format!("the manifest is not readable: {err}"))?;
    if !is_plain(&manifest.version) || Version::parse(&manifest.version).is_none() {
        return Err(format!(
            "the manifest's version `{}` is not a version",
            manifest.version
        ));
    }
    let entry = manifest
        .platforms
        .get(key)
        .ok_or_else(|| format!("the manifest has no release for {key}"))?;
    if !entry.url.starts_with("https://") {
        return Err(format!(
            "the archive's address `{}` is not https",
            entry.url
        ));
    }
    let sha256 = entry.sha256.trim().to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("the archive's SHA-256 in the manifest is not a SHA-256".to_owned());
    }
    if entry.signature.trim().is_empty() {
        return Err("the manifest carries no signature for the archive".to_owned());
    }
    // The floor only informs the restart's confirmation; a value that is no version is left out
    // rather than failing the whole manifest.
    let holder_floor = manifest
        .holder_floor
        .filter(|floor| Version::parse(floor).is_some());
    Ok(Release {
        version: manifest.version,
        notes: manifest.notes.filter(|notes| !notes.trim().is_empty()),
        holder_floor,
        url: entry.url.clone(),
        signature: entry.signature.trim().to_owned(),
        sha256,
        channel,
    })
}
