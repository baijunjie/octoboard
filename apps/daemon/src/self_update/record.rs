//! What the update leaves in the data directory.
//!
//! - `update.lock`, held while an update downloads and replaces the executable, by the daemon or
//!   by `octoboardd update`, so that the two never run together.
//! - `update-installed.json`, written after every replacement, so that a daemon learns what
//!   `octoboardd update` installed under it.
//! - `daemon.version`, the version of the daemon that last started, so that a daemon that finds an
//!   older one there knows it replaced it.
//! - `updates/`, where an archive is downloaded to.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::holder::compat::Version;
use crate::hostfs::replace::{replace_beside, Permissions};

/// The most a record file may hold.
const RECORD_LIMIT: usize = 16 * 1024;

/// The update's files in the data directory `dir`.
#[derive(Debug, Clone)]
pub struct Files {
    dir: PathBuf,
}

/// What an update that replaced the executable left behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    /// The version installed.
    pub version: String,
    /// Its holder floor, when the manifest named one.
    pub holder_floor: Option<String>,
    /// The version that was replaced.
    pub from_version: String,
    /// The executable that was replaced.
    pub path: String,
}

impl Files {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.to_path_buf(),
        }
    }

    pub fn lock(&self) -> PathBuf {
        self.dir.join("update.lock")
    }

    pub fn updates_dir(&self) -> PathBuf {
        self.dir.join("updates")
    }

    fn installed(&self) -> PathBuf {
        self.dir.join("update-installed.json")
    }

    fn daemon_version(&self) -> PathBuf {
        self.dir.join("daemon.version")
    }

    /// Removes the files in `updates/`, which are those of a download that did not end.
    pub fn clear_updates(&self) {
        let Ok(entries) = std::fs::read_dir(self.updates_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }

    /// [`Self::clear_updates`] for a daemon that starts: a stop during a download leaves its file
    /// behind, the download's thread not having ended before the process did. Only with the update
    /// lock, which another process that updates holds while its download is under way. Whether it
    /// was taken and the folder cleared.
    pub fn sweep_updates(&self) -> bool {
        let Ok(_lock) = crate::platform::fs::try_lock_file(&self.lock()) else {
            return false;
        };
        self.clear_updates();
        true
    }

    /// Records a replacement.
    pub fn write_installed(&self, record: &Installed) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
        replace_beside(&self.installed(), &bytes, Permissions::KeptOrDefault)
    }

    /// Forgets the replacement recorded, as one that no longer describes the file: a rollback put
    /// the previous build back. Nothing recorded is no error.
    pub fn forget_installed(&self) -> std::io::Result<()> {
        crate::hostfs::replace::remove_replaced(&self.installed())
    }

    /// The last replacement recorded; `None` when there is none or it cannot be read.
    pub fn read_installed(&self) -> Option<Installed> {
        let bytes = crate::hostfs::read_regular(&self.installed(), RECORD_LIMIT).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// The version the daemon that started last ran as; `None` when none is recorded or it does
    /// not read as a version.
    pub fn read_daemon_version(&self) -> Option<String> {
        let bytes = crate::hostfs::read_regular(&self.daemon_version(), RECORD_LIMIT).ok()?;
        let text = String::from_utf8(bytes).ok()?.trim().to_owned();
        Version::parse(&text).map(|_| text)
    }

    /// Records the version the daemon that starts now runs as.
    pub fn write_daemon_version(&self, version: &str) -> std::io::Result<()> {
        replace_beside(
            &self.daemon_version(),
            format!("{version}\n").as_bytes(),
            Permissions::KeptOrDefault,
        )
    }

    /// The version the daemon replaced when it started as `running`: the version recorded by the
    /// daemon before it, when that is older. Records `running` for the next start.
    pub fn started_as(&self, running: &str) -> Option<String> {
        let before = self.read_daemon_version();
        if let Err(err) =
            std::fs::create_dir_all(&self.dir).and_then(|()| self.write_daemon_version(running))
        {
            tracing::warn!(%err, "recording the daemon's version failed");
        }
        let (before, running) = (before?, Version::parse(running)?);
        (Version::parse(&before)? < running).then_some(before)
    }
}
