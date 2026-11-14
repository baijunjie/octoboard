//! What the daemon of the server form knows about its own update: the channel it follows, the
//! release it found, the download that runs or ran and the replacement that was made. See "Daemon
//! updates" in `apps/daemon/PROTOCOL.md`.

use serde::{Deserialize, Serialize};

use super::codes::Code;

/// Which release line the daemon's update check follows (`Settings.daemon_update_channel`).
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonUpdateChannel {
    /// Releases that are not marked as prereleases.
    #[default]
    Stable,
    /// Every release, prereleases included.
    Beta,
}

impl DaemonUpdateChannel {
    /// The name it has on the wire, which is what the serialization writes and what the release
    /// manifest's file name carries.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }

    /// The value named `text` on the wire, or `None` for a name this build does not know.
    pub fn from_wire_name(text: &str) -> Option<Self> {
        [Self::Stable, Self::Beta]
            .into_iter()
            .find(|value| value.wire_name() == text)
    }
}

/// What the daemon's update is doing. A check and a download never run together.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonUpdatePhase {
    Idle,
    /// Reading the release manifest.
    Checking,
    /// Downloading the archive, checking it and replacing the executable: `install` says how far.
    Downloading,
    /// The executable has been replaced; the update takes effect when the daemon restarts, and no
    /// more checks are made until it has.
    Installed,
}

/// A release newer than the daemon that the last check found.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonUpdateAvailable {
    pub version: String,
    /// The oldest holder version the new daemon adopts the terminals of (`handover_check`'s
    /// `holder_floor`); null when the manifest names none.
    pub holder_floor: Option<String>,
    /// The channel the manifest was read from.
    pub channel: DaemonUpdateChannel,
}

/// How much of the archive has been downloaded.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonUpdateProgress {
    /// Bytes received so far.
    pub downloaded: u64,
    /// The archive's size in bytes, when the server said it.
    pub total: Option<u64>,
}

/// The release the executable was replaced with.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonUpdateInstalled {
    /// Its version; null when the file was replaced by something else (an install script run
    /// meanwhile) and the daemon did not write it, so the version is not known.
    pub version: Option<String>,
    /// Its holder floor, null where `version` is.
    pub holder_floor: Option<String>,
}

/// How the last check ended.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonUpdateCheckOutcome {
    UpToDate,
    Available,
    Failed,
}

/// The last check that ended.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonUpdateCheck {
    pub outcome: DaemonUpdateCheckOutcome,
    /// When it ended, in milliseconds since the epoch.
    pub at: i64,
    /// Why a `failed` check failed, `daemon_update_check_failed`; null otherwise.
    pub error: Option<Code>,
    /// The English text of the failure behind `error`, trimmed and cut to a bounded length.
    pub detail: Option<String>,
}

/// Where the download and replacement of an update is.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonUpdateInstallState {
    Running,
    Succeeded,
    Failed,
}

/// The download and replacement of the update as the daemon last made one. Runtime state: never
/// stored.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonUpdateInstall {
    pub state: DaemonUpdateInstallState,
    /// When it started, in milliseconds since the epoch.
    pub started_at: i64,
    /// When it ended; null while it runs.
    pub finished_at: Option<i64>,
    /// Why a `failed` one failed: `daemon_update_download_failed`, `daemon_update_unverified` or
    /// `daemon_update_replace_failed`; null otherwise.
    pub error: Option<Code>,
    /// The English text of the failure behind `error`, trimmed and cut to a bounded length.
    pub detail: Option<String>,
}

/// Why the daemon does not update itself and the user has to.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonUpdateManualReason {
    /// The user the daemon runs as may not replace its executable: the directory it lives in is
    /// not writable by them.
    NotWritable,
}

/// An update the daemon found and cannot install itself.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonUpdateManual {
    pub reason: DaemonUpdateManualReason,
    /// The executable that would be replaced, as a path on the daemon's host.
    pub path: String,
    /// The shell command that installs the update over it by hand: the install script, told the
    /// executable's directory and the channel the daemon follows. For `not_writable`, it has to be
    /// run by a user who may write to that directory (its owner).
    pub command: String,
}

/// The daemon's own update as it stands. Runtime state: never stored, and `null` in the snapshot
/// where the daemon does not update itself (a build without the feature, the desktop
/// application's daemon, a host it has no release for).
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaemonUpdateStatus {
    pub phase: DaemonUpdatePhase,
    /// The newer release the last check found and none has replaced yet; null otherwise.
    pub available: Option<DaemonUpdateAvailable>,
    /// How far the archive has come while `phase` is `downloading`; null otherwise.
    pub progress: Option<DaemonUpdateProgress>,
    /// What replaced the executable, once `phase` is `installed`; null otherwise.
    pub installed: Option<DaemonUpdateInstalled>,
    /// The last check that ended since the daemon started; null when none has.
    pub last_check: Option<DaemonUpdateCheck>,
    /// The last download and replacement since the daemon started; null when none ran.
    pub install: Option<DaemonUpdateInstall>,
    /// Set while an update is available that the user has to install by hand.
    pub manual: Option<DaemonUpdateManual>,
    /// The version this daemon replaced when it started, as an update left it; null when it
    /// replaced none. Held until the daemon stops, so a window that opens later is told too.
    pub updated_from: Option<String>,
}

impl DaemonUpdateStatus {
    /// The status of a daemon that has checked nothing and updated nothing.
    pub fn idle() -> Self {
        Self {
            phase: DaemonUpdatePhase::Idle,
            available: None,
            progress: None,
            installed: None,
            last_check: None,
            install: None,
            manual: None,
            updated_from: None,
        }
    }
}
