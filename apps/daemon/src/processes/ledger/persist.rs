//! What the ledger keeps on disk under the data directory: the instance id that tells this data
//! directory's daemon's tags from any other's, and the processes attributed and kept, so a daemon
//! started after a restart or a crash can stop what the previous one left. What the book holds
//! is written down here ([`Persisted::from_book`]), and read back into a book at start
//! ([`Persisted::read_back`]).

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::attribute::{Attributed, Book, ENDED};
use super::table::Identity;
use crate::hostfs::replace::{replace_file, Permissions};
use crate::platform::proc::{ContainerId, ProcessId};

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
    /// The boot of the system the file was written in (`table::boot_id`): written in another,
    /// nothing it names is running, and its start times do not compare with this boot's.
    #[serde(default)]
    pub boot: Option<String>,
    #[serde(default)]
    pub attributed: Vec<PersistedProcess>,
    #[serde(default, with = "wire_identities")]
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

impl Persisted {
    /// What `book` holds that a restart needs, written in the boot `boot`.
    pub fn from_book(book: &Book, boot: Option<String>) -> Self {
        Self {
            boot,
            attributed: book
                .attributed
                .iter()
                .map(|(id, attributed)| PersistedProcess {
                    pid: id.pid.0 as i32,
                    start: id.start_micros,
                    launch: attributed.launch.clone(),
                    sid: attributed.sid.map_or(0, |sid| sid.0 as i32),
                })
                .collect(),
            kept: book.kept.iter().copied().collect(),
            kept_sessions: book
                .kept_sessions
                .iter()
                .map(|(&sid, &leader_start)| PersistedSession {
                    sid: sid.0 as i32,
                    leader_start,
                })
                .collect(),
            sessions: book
                .launches
                .values()
                .flat_map(|launch| launch.sessions.iter())
                .map(|(&sid, &leader_start)| PersistedSession {
                    sid: sid.0 as i32,
                    leader_start,
                })
                .collect(),
            kept_projects: book
                .kept_projects
                .iter()
                .map(|(id, project)| PersistedKeptProject {
                    pid: id.pid.0 as i32,
                    start: id.start_micros,
                    project: project.clone(),
                })
                .collect(),
            kept_session_projects: book
                .kept_session_projects
                .iter()
                .map(|(&sid, project)| PersistedSessionProject {
                    sid: sid.0 as i32,
                    project: project.clone(),
                })
                .collect(),
        }
    }

    /// Takes what the file holds back into `book`: what it kept as kept, with the projects they
    /// are listed under, its sessions to be proven by the first scan (see
    /// [`Book::read_back_sessions`]), and every process it attributed and did not keep as an
    /// ended launch's. Answers the processes held for an ended launch from then on.
    pub fn read_back(&self, book: &mut Book) -> HashSet<Identity> {
        book.kept.extend(self.kept.iter().copied());
        let sessions = |sessions: &[PersistedSession]| {
            sessions
                .iter()
                .map(|session| (ContainerId(session.sid as u32), session.leader_start))
                .collect::<Vec<_>>()
        };
        book.read_back_sessions(
            sessions(&self.sessions),
            sessions(&self.kept_sessions),
            self.attributed
                .iter()
                .map(PersistedProcess::identity)
                .chain(self.kept.iter().copied()),
        );
        for kept in &self.kept_projects {
            let id = Identity {
                pid: ProcessId(kept.pid as u32),
                start_micros: kept.start,
            };
            if book.kept.contains(&id) {
                book.kept_projects
                    .entry(id)
                    .or_insert_with(|| kept.project.clone());
            }
        }
        for session in &self.kept_session_projects {
            let sid = ContainerId(session.sid as u32);
            if book.kept_sessions.contains_key(&sid) {
                book.kept_session_projects
                    .entry(sid)
                    .or_insert_with(|| session.project.clone());
            }
        }
        for process in &self.attributed {
            let id = process.identity();
            if !book.kept.contains(&id) {
                book.attributed.entry(id).or_insert(Attributed {
                    launch: ENDED.to_string(),
                    sid: process.session(),
                });
            }
        }
        book.dirty = true;
        book.attributed
            .iter()
            .filter(|(_, attributed)| attributed.launch == ENDED)
            .map(|(id, _)| *id)
            .collect()
    }
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
            pid: ProcessId(self.pid as u32),
            start_micros: self.start,
        }
    }

    /// The terminal session it was in, where it was in one: written as 0 when it was in none.
    fn session(&self) -> Option<ContainerId> {
        (self.sid > 0).then_some(ContainerId(self.sid as u32))
    }
}

/// The wire form of the identities in the ledger file: `{"pid": …, "start": …}`.
mod wire_identities {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{Identity, ProcessId};

    #[derive(Serialize, Deserialize)]
    struct Wire {
        pid: i32,
        start: u64,
    }

    pub fn serialize<S: Serializer>(
        identities: &[Identity],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(identities.iter().map(|id| Wire {
            pid: id.pid.0 as i32,
            start: id.start_micros,
        }))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Identity>, D::Error> {
        Ok(Vec::<Wire>::deserialize(deserializer)?
            .into_iter()
            .map(|wire| Identity {
                pid: ProcessId(wire.pid as u32),
                start_micros: wire.start,
            })
            .collect())
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
    replace_file(
        path,
        &path.with_extension("partial"),
        bytes,
        Permissions::Private,
    )
}
