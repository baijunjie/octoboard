//! The holders a quit of the desktop application left to end, kept in the file `application-quit`
//! of the data directory (`paths::application_quit_record`) until each is gone.
//!
//! The application leaves the file when it quits: `{"version":1}` for a plain quit, which names
//! no holder and so means every holder the registry lists when a daemon reads it (the record is
//! unscoped), or empty, which older applications leave and reads the same. A daemon reconciles
//! the file at every start, under its instance lock, before it adopts a holder or starts a
//! terminal ([`reconcile`]): an unscoped record is made scoped by writing back the holders the
//! registry lists now, each as the id and the process identity its registry file names
//! (`{"version":1,"targets":[{"holder_id":"…","started":"…"}]}`). From then on the record names
//! only holders that already ran, so a holder started after it is never one of them, and it stays
//! until each target is resolved ([`QuitTargets::resolved`]): confirmed ended, or found gone. A
//! start cut short, a crash, or a daemon killed meanwhile leaves the targets not yet resolved for
//! the next start, which ends them whether or not it follows the application.
//!
//! A record of a version this build does not know, or one that cannot be read, is taken as
//! unscoped, and logged as an error. Every write replaces the file whole. Stated limits: a holder
//! the previous daemon launched that had not yet written its registry file when the record was
//! scoped is not a target, and a record that cannot be rewritten stays unscoped, read again by the
//! next start.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use super::registry;
use crate::hostfs::replace::{remove_replaced, replace_beside, Permissions};

/// The version of the record this build reads and writes.
const VERSION: u64 = 1;

/// A holder to end: its registry file's id, and the `<pid>:<start>` identity of its process, which
/// tells it from another process with the pid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub holder_id: String,
    pub started: String,
}

/// What the record says, as it is read.
#[derive(Debug, PartialEq, Eq)]
enum Intent {
    /// Every holder the registry lists when it is read.
    Unscoped,
    /// These holders.
    Scoped(Vec<Target>),
}

#[derive(Deserialize)]
struct Record {
    #[serde(default)]
    targets: Option<Vec<Target>>,
}

#[derive(Serialize)]
struct Written<'a> {
    version: u64,
    targets: &'a [Target],
}

/// The holders still to be ended, and the file that keeps them. Shared by the holder client, which
/// resolves a target as its ending is confirmed, even when that is long after the start.
pub struct QuitTargets {
    path: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    /// The targets not resolved yet.
    remaining: Vec<Target>,
    /// What this daemon last wrote to the file, or found in it as a scoped record; `None` when
    /// the file says something else it could not replace (an unscoped record).
    written: Option<Vec<Target>>,
}

impl QuitTargets {
    /// The targets `targets` names, which the file `path` holds as a scoped record.
    pub fn new(path: PathBuf, targets: Vec<Target>) -> Self {
        let written = Some(targets.clone());
        Self::with_written(path, targets, written)
    }

    fn with_written(path: PathBuf, targets: Vec<Target>, written: Option<Vec<Target>>) -> Self {
        Self {
            path,
            inner: Mutex::new(Inner {
                remaining: targets,
                written,
            }),
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The targets not resolved yet.
    pub fn remaining(&self) -> Vec<Target> {
        self.inner().remaining.clone()
    }

    /// Whether the holder `holder_id`, whose registry file names the process `started`, is a
    /// target not resolved yet.
    pub fn is_target(&self, holder_id: &str, started: &str) -> bool {
        self.inner()
            .remaining
            .iter()
            .any(|target| target.holder_id == holder_id && target.started == started)
    }

    /// Notes the holder `holder_id` as ended, or found gone, and rewrites the record without it;
    /// the file is removed once no target is left. A holder that is no target is no change. The
    /// file is touched only while it still says what this daemon last wrote: the application
    /// writes a record of its own at every quit, which a rewrite must not overwrite or remove, so
    /// a record that is unscoped or differs is left as it is, and only the daemon's own list
    /// changes. A record that cannot be rewritten is logged as an error and keeps the target,
    /// which the next start finds gone and resolves again.
    pub fn resolved(&self, holder_id: &str) {
        let mut inner = self.inner();
        let before = inner.remaining.len();
        inner
            .remaining
            .retain(|target| target.holder_id != holder_id);
        if inner.remaining.len() == before {
            return;
        }
        let Some(written) = inner.written.clone() else {
            tracing::debug!(path = %self.path.display(), "the application's quit record is unscoped; it is left as it is");
            return;
        };
        match read(&self.path) {
            Some(Intent::Scoped(on_disk)) if on_disk == written => {}
            Some(_) => {
                tracing::info!(path = %self.path.display(), "the application's quit record was replaced; it is left as it is");
                return;
            }
            None => return,
        }
        if inner.remaining.is_empty() {
            remove_file(&self.path);
            inner.written = None;
        } else if let Err(err) = write(&self.path, &inner.remaining) {
            tracing::error!(path = %self.path.display(), %err, "rewriting the application's quit record failed; the holder it names is found gone at the next start");
        } else {
            inner.written = Some(inner.remaining.clone());
        }
    }
}

/// Removes the file at `path`. One that cannot be removed is logged as an error and is acted on
/// again at every start until it can be.
fn remove_file(path: &Path) {
    if let Err(err) = remove_replaced(path) {
        tracing::error!(path = %path.display(), %err, "removing the application's quit record failed; it is acted on again at every start until it is removed");
    }
}

/// Reconciles the record in `path` with the registry in `registry_dir`, at a daemon's start: an
/// unscoped record is written back scoped to the holders the registry lists, and one that names no
/// holder is removed. `None` where there is no record, or nothing to end. **Blocks.**
pub fn reconcile(path: &Path, registry_dir: &Path) -> Option<Arc<QuitTargets>> {
    let targets = match read(path)? {
        Intent::Scoped(targets) => targets,
        Intent::Unscoped => {
            let listed: Vec<Target> = registry::list(registry_dir)
                .into_iter()
                .map(|entry| Target {
                    holder_id: entry.holder_id,
                    started: entry.started,
                })
                .collect();
            if listed.is_empty() {
                remove_file(path);
                return None;
            }
            // Before any holder is adopted: what is listed now is what the quit left behind.
            let scoped = write(path, &listed);
            if let Err(err) = &scoped {
                tracing::error!(path = %path.display(), %err, "scoping the application's quit record failed; it stays unscoped, and a later start reads it again");
            }
            let written = scoped.ok().map(|()| listed.clone());
            return Some(Arc::new(QuitTargets::with_written(
                path.to_path_buf(),
                listed,
                written,
            )));
        }
    };
    if targets.is_empty() {
        remove_file(path);
        return None;
    }
    Some(Arc::new(QuitTargets::new(path.to_path_buf(), targets)))
}

/// The record in `path`, `None` where there is none. One that cannot be read counts as unscoped.
fn read(path: &Path) -> Option<Intent> {
    match std::fs::read(path) {
        Ok(bytes) => Some(parse(&bytes)),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => {
            tracing::error!(path = %path.display(), %err, "reading the application's quit record failed; every holder is ended");
            Some(Intent::Unscoped)
        }
    }
}

/// What `bytes` say. Nothing but white space is what an older application leaves; content that is
/// not a record of a version this build knows is taken as unscoped, and logged.
fn parse(bytes: &[u8]) -> Intent {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Intent::Unscoped;
    }
    let value: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(%err, "the application's quit record cannot be read; every holder is ended");
            return Intent::Unscoped;
        }
    };
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(version) if version <= VERSION => {}
        version => {
            tracing::error!(?version, "the application's quit record is of a version this build does not know; every holder is ended");
            return Intent::Unscoped;
        }
    }
    match serde_json::from_value::<Record>(value) {
        Ok(Record {
            targets: Some(targets),
        }) => Intent::Scoped(targets),
        Ok(Record { targets: None }) => Intent::Unscoped,
        Err(err) => {
            tracing::error!(%err, "the application's quit record cannot be read; every holder is ended");
            Intent::Unscoped
        }
    }
}

/// Replaces the record in `path` with one that names `targets`. **Blocks.**
fn write(path: &Path, targets: &[Target]) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(&Written {
        version: VERSION,
        targets,
    })
    .map_err(std::io::Error::other)?;
    replace_beside(path, &bytes, Permissions::Private)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holder::registry::RegistryEntry;
    use crate::test_support::ScratchDir;

    fn target(holder_id: &str) -> Target {
        Target {
            holder_id: holder_id.to_string(),
            started: format!("{holder_id}:1"),
        }
    }

    fn listed(registry_dir: &Path, holder_id: &str) {
        registry::write(
            registry_dir,
            &RegistryEntry {
                holder_id: holder_id.to_string(),
                pid: 4242,
                started: format!("{holder_id}:1"),
                port: 51000,
                secret: "s".to_string(),
                version: "1.0.0".to_string(),
                build_id: "b".to_string(),
                exe: "/x".to_string(),
            },
        )
        .unwrap();
    }

    #[test]
    fn an_empty_or_unscoped_record_is_unscoped() {
        assert_eq!(parse(b""), Intent::Unscoped);
        assert_eq!(parse(b" \r\n"), Intent::Unscoped);
        assert_eq!(parse(br#"{"version":1}"#), Intent::Unscoped);
        assert_eq!(parse(br#"{"version":1,"targets":null}"#), Intent::Unscoped);
    }

    #[test]
    fn a_scoped_record_names_its_targets() {
        let record = br#"{"version":1,"targets":[{"holder_id":"a","started":"1:2"}]}"#;
        assert_eq!(
            parse(record),
            Intent::Scoped(vec![Target {
                holder_id: "a".to_string(),
                started: "1:2".to_string(),
            }])
        );
        assert_eq!(
            parse(br#"{"version":1,"targets":[]}"#),
            Intent::Scoped(Vec::new())
        );
    }

    /// A record this build cannot be sure to read is taken as the widest one, which ends more.
    #[test]
    fn a_record_of_an_unknown_version_or_garbage_is_unscoped() {
        let scoped = br#"{"version":2,"targets":[{"holder_id":"a","started":"1:2"}]}"#;
        assert_eq!(parse(scoped), Intent::Unscoped);
        assert_eq!(parse(br#"{"targets":[]}"#), Intent::Unscoped);
        assert_eq!(parse(b"not json"), Intent::Unscoped);
        assert_eq!(parse(br#"{"version":1,"targets":"a"}"#), Intent::Unscoped);
    }

    #[test]
    fn no_record_is_nothing_to_end() {
        let dir = ScratchDir::new("quit-targets-none");
        assert!(reconcile(&dir.join("application-quit"), &dir.join("holders")).is_none());
    }

    #[test]
    fn an_unscoped_record_is_scoped_to_the_holders_listed() {
        let dir = ScratchDir::new("quit-targets-scope");
        let (record, holders) = (dir.join("application-quit"), dir.join("holders"));
        listed(&holders, "a");
        listed(&holders, "b");
        std::fs::write(&record, b"").unwrap();
        let quit = reconcile(&record, &holders).expect("targets");
        let mut named: Vec<String> = quit.remaining().into_iter().map(|t| t.holder_id).collect();
        named.sort();
        assert_eq!(named, ["a", "b"]);
        let Some(Intent::Scoped(mut written)) = read(&record) else {
            panic!("the record was not scoped");
        };
        written.sort_by(|a, b| a.holder_id.cmp(&b.holder_id));
        assert_eq!(written, [target("a"), target("b")]);
        // A holder listed afterwards is no target.
        listed(&holders, "late");
        assert!(!quit.is_target("late", "late:1"));
        assert!(quit.is_target("a", "a:1"));
        assert!(!quit.is_target("a", "a:2"), "the identity is told too");
    }

    #[test]
    fn a_record_with_nothing_to_end_is_removed() {
        let dir = ScratchDir::new("quit-targets-empty");
        let (record, holders) = (dir.join("application-quit"), dir.join("holders"));
        std::fs::write(&record, br#"{"version":1}"#).unwrap();
        assert!(reconcile(&record, &holders).is_none());
        assert!(!record.exists());
        std::fs::write(&record, br#"{"version":1,"targets":[]}"#).unwrap();
        assert!(reconcile(&record, &holders).is_none());
        assert!(!record.exists());
    }

    #[test]
    fn a_scoped_record_is_kept_as_it_is_and_ignores_the_registry() {
        let dir = ScratchDir::new("quit-targets-scoped");
        let (record, holders) = (dir.join("application-quit"), dir.join("holders"));
        listed(&holders, "other");
        write(&record, &[target("a")]).unwrap();
        let quit = reconcile(&record, &holders).expect("targets");
        assert_eq!(quit.remaining(), [target("a")]);
        assert_eq!(read(&record), Some(Intent::Scoped(vec![target("a")])));
    }

    #[test]
    fn resolving_targets_rewrites_the_record_and_removes_it_with_the_last() {
        let dir = ScratchDir::new("quit-targets-resolved");
        let record = dir.join("application-quit");
        write(&record, &[target("a"), target("b")]).unwrap();
        let quit = QuitTargets::new(record.clone(), vec![target("a"), target("b")]);
        quit.resolved("unknown");
        assert_eq!(quit.remaining().len(), 2);
        quit.resolved("a");
        assert_eq!(quit.remaining(), [target("b")]);
        assert_eq!(read(&record), Some(Intent::Scoped(vec![target("b")])));
        quit.resolved("a");
        assert!(record.exists());
        quit.resolved("b");
        assert!(quit.remaining().is_empty());
        assert!(!record.exists());
        quit.resolved("b");
    }

    /// The application writes a record of its own at every quit, even while the daemon runs: a
    /// resolution after that changes the daemon's list only.
    #[test]
    fn a_record_replaced_meanwhile_is_left_in_place() {
        let dir = ScratchDir::new("quit-targets-replaced");
        let record = dir.join("application-quit");
        write(&record, &[target("a"), target("b")]).unwrap();
        let quit = QuitTargets::new(record.clone(), vec![target("a"), target("b")]);
        std::fs::write(&record, br#"{"version":1}"#).unwrap();
        quit.resolved("a");
        assert_eq!(quit.remaining(), [target("b")]);
        assert_eq!(std::fs::read(&record).unwrap(), br#"{"version":1}"#);
        quit.resolved("b");
        assert_eq!(std::fs::read(&record).unwrap(), br#"{"version":1}"#);

        write(&record, &[target("c")]).unwrap();
        let quit = QuitTargets::new(record.clone(), vec![target("a"), target("b")]);
        quit.resolved("a");
        assert_eq!(read(&record), Some(Intent::Scoped(vec![target("c")])));
    }

    /// A record that could not be scoped stays as the application wrote it.
    #[test]
    fn an_unscoped_record_that_could_not_be_rewritten_is_left_alone() {
        let dir = ScratchDir::new("quit-targets-unwritten");
        let record = dir.join("application-quit");
        std::fs::write(&record, b"").unwrap();
        let quit = QuitTargets::with_written(record.clone(), vec![target("a")], None);
        quit.resolved("a");
        assert!(record.exists());
    }

    /// A write replaces the file whole, leaving no staging file beside it.
    #[test]
    fn a_write_leaves_no_staging_file_behind() {
        let dir = ScratchDir::new("quit-targets-atomic");
        let record = dir.join("application-quit");
        write(&record, &[target("a")]).unwrap();
        write(&record, &[target("b")]).unwrap();
        assert_eq!(read(&record), Some(Intent::Scoped(vec![target("b")])));
        let files = std::fs::read_dir(&*dir).unwrap().count();
        assert_eq!(files, 1);
    }
}
