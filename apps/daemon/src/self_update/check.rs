//! Asking the release manifest whether a newer daemon is published. Shared by the daemon's check
//! and `octoboardd update --check`.

use std::time::Duration;

use super::fetch::{get_text, FetchError, Fetcher};
use super::manifest::{manifest_url, parse, platform_key, Release};
use crate::protocol::DaemonUpdateChannel;

/// The most of a manifest that is read; a real one is a few hundred bytes plus the notes.
const MANIFEST_LIMIT: u64 = 1024 * 1024;

/// How long reading the manifest may take.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(30);

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    /// The channel has nothing newer than the running build: no manifest (the host answers "not
    /// found" before a channel's first release), or one that names the build's own version or an
    /// older one.
    UpToDate,
    /// A strictly newer release for this machine.
    Available(Release),
}

/// Reads the manifest of `channel` through `fetcher` and says whether it offers a release newer
/// than `running`. Only "not found" means there is nothing: any other status, a network failure
/// or a manifest that cannot be read is an error, in English, for the check's detail. Blocking.
pub fn check(
    fetcher: &dyn Fetcher,
    channel: DaemonUpdateChannel,
    running: &str,
) -> Result<Checked, String> {
    let url = manifest_url(channel);
    let body = match get_text(fetcher, &url, MANIFEST_LIMIT, MANIFEST_TIMEOUT) {
        Ok(body) => body,
        Err(FetchError::Status(404)) => return Ok(Checked::UpToDate),
        Err(err) => return Err(err.to_string()),
    };
    let release = parse(&body, &platform_key(), channel)?;
    Ok(if release.is_newer_than(running) {
        Checked::Available(release)
    } else {
        Checked::UpToDate
    })
}
