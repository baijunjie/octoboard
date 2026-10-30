//! What the ledger keeps on disk under the data directory: the instance id that tells this data
//! directory's daemon's tags from any other's, and the processes attributed and kept, so a daemon
//! started after a restart or a crash can stop what the previous one left.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::table::Identity;

const INSTANCE_FILE: &str = "instance-id";
pub(super) const LEDGER_FILE: &str = "process-ledger.json";

/// The instance id kept in `dir`, created on first use. When it can be neither read nor written,
/// a fresh one for this run, which still keeps another daemon's processes apart but leaves the
/// processes of a previous run to nobody.
pub(super) fn instance_id(dir: &Path) -> String {
    let path = dir.join(INSTANCE_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) if uuid::Uuid::parse_str(text.trim()).is_ok() => return text.trim().to_string(),
        Ok(_) => {
            tracing::warn!(path = %path.display(), "the instance id file holds no id; writing a new one")
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "reading the instance id failed; using one for this run only");
            return uuid::Uuid::new_v4().to_string();
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(err) = write_atomically(&path, id.as_bytes()) {
        tracing::warn!(path = %path.display(), %err, "writing the instance id failed; using one for this run only");
    }
    id
}

/// The ledger file's content.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Persisted {
    #[serde(default)]
    pub attributed: Vec<PersistedProcess>,
    #[serde(default)]
    pub kept: Vec<Identity>,
    #[serde(default)]
    pub kept_sessions: Vec<PersistedSession>,
    /// The terminal sessions recorded for the launches, whose members a restart stops.
    #[serde(default)]
    pub sessions: Vec<PersistedSession>,
    /// The project each kept process is listed under, for those that have one.
    #[serde(default)]
    pub kept_projects: Vec<PersistedKeptProject>,
    /// The project each kept terminal session is listed under, for those that have one.
    #[serde(default)]
    pub kept_session_projects: Vec<PersistedSessionProject>,
}

/// A kept process with the project it is listed under.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct PersistedKeptProject {
    pub pid: i32,
    pub start: u64,
    pub project: String,
}

/// A kept terminal session with the project it is listed under.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct PersistedSessionProject {
    pub sid: i32,
    pub project: String,
}

/// A terminal session recorded for a launch or as kept, with its leader's start time when the
/// leader was running when it was recorded.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct PersistedSession {
    pub sid: i32,
    pub leader_start: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct PersistedProcess {
    pub pid: i32,
    pub start: u64,
    pub launch: String,
    pub sid: i32,
}

impl PersistedProcess {
    pub fn identity(&self) -> Identity {
        Identity {
            pid: self.pid,
            start: self.start,
        }
    }
}

/// What reading the ledger file found.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Loaded {
    /// There is no file: nothing was written down, so nothing was kept.
    Missing,
    Read(Persisted),
    /// There is a file that could not be read or parsed: what it said, the kept processes
    /// included, is unknown. It has been moved aside (see [`set_aside`]) when that was possible.
    Unreadable,
}

/// The ledger file at `path`. A file that cannot be read is moved aside, so that the next write
/// does not replace what someone may still want to look at.
pub(super) fn load(path: &Path) -> Loaded {
    let parsed = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Loaded::Missing,
        Err(err) => Err(err),
    };
    match parsed {
        Ok(persisted) => Loaded::Read(persisted),
        Err(err) => {
            let aside = set_aside(path);
            tracing::warn!(
                path = %path.display(), %err, aside = %aside.display(),
                "the process ledger file could not be read and was moved aside; what the previous run left is not stopped, since what the user kept is not known",
            );
            Loaded::Unreadable
        }
    }
}

/// Moves the file at `path` to the name beside it that says it could not be read, replacing an
/// earlier one there, and answers that name.
pub(super) fn set_aside(path: &Path) -> std::path::PathBuf {
    let aside = path.with_extension("json.unreadable");
    if let Err(err) = std::fs::rename(path, &aside) {
        tracing::warn!(path = %path.display(), %err, "moving the unreadable process ledger file aside failed");
    }
    aside
}

pub(super) fn save(path: &Path, persisted: &Persisted) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(persisted).map_err(std::io::Error::other)?;
    write_atomically(path, &bytes)
}

/// Writes `bytes` to a file beside `path` and renames it into place, so a reader never sees half
/// of it. The caller keeps two writes of one path from running at once.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("partial");
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&partial)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&partial, path));
    if written.is_err() {
        std::fs::remove_file(&partial).ok();
    }
    written
}
