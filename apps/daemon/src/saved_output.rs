//! The last terminal output of each session whose process has ended, one file per session, so that
//! a client selecting the session later still has something to show. The ring buffer it comes from
//! goes with the process, and the conversation itself is the agent's, not Octoboard's.
//!
//! Nothing here is ever an error to the caller: a file that is missing, unreadable or unwritable
//! only means there is no saved output to show, so failures are logged and swallowed.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Prefix of a file still being written. A leading dot keeps it apart from every session id, so a
/// half-written file is never read as one.
const PARTIAL_PREFIX: &str = ".partial-";

pub struct SavedOutput {
    dir: PathBuf,
    next_partial: AtomicU64,
}

impl SavedOutput {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            next_partial: AtomicU64::new(0),
        }
    }

    /// Replaces the session's saved output. Written to a file of its own and renamed into place, so
    /// a reader never sees half of it and a daemon killed mid-write leaves the previous one intact.
    pub fn write(&self, id: &str, output: &[u8]) {
        let Some(path) = self.file(id) else {
            return;
        };
        let partial = self.dir.join(format!(
            "{PARTIAL_PREFIX}{}",
            self.next_partial.fetch_add(1, Ordering::Relaxed)
        ));
        let written = std::fs::create_dir_all(&self.dir)
            .and_then(|()| {
                // Terminal output can hold secrets, so it is the owner's alone.
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&partial)?;
                file.write_all(output)?;
                file.sync_all()
            })
            .and_then(|()| std::fs::rename(&partial, &path));
        if let Err(err) = written {
            tracing::warn!(session = %id, path = %path.display(), %err, "saving the session's last output failed");
            std::fs::remove_file(&partial).ok();
        }
    }

    pub fn read(&self, id: &str) -> Option<Vec<u8>> {
        let path = self.file(id)?;
        match std::fs::read(&path) {
            Ok(output) => Some(output),
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(session = %id, path = %path.display(), %err, "reading the session's saved output failed");
                }
                None
            }
        }
    }

    pub fn remove(&self, id: &str) {
        if let Some(path) = self.file(id) {
            remove_quietly(&path);
        }
    }

    /// Removes every file not belonging to a session `keep` names, and every partial file. Run once
    /// at startup, when no other daemon can be writing here: it is what collects the output of a
    /// session deleted while the daemon that held it was not running to remove it.
    pub fn sweep(&self, keep: impl Fn(&str) -> bool) {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %self.dir.display(), %err, "listing saved session output failed");
                }
                return;
            }
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let kept = name
                .to_str()
                .is_some_and(|name| is_session_file_name(name) && keep(name));
            if !kept {
                remove_quietly(&entry.path());
            }
        }
    }

    /// The file of one session. `None` for anything that is not a bare name: the id can come from a
    /// request path, and must not be able to reach outside this directory.
    fn file(&self, id: &str) -> Option<PathBuf> {
        is_session_file_name(id).then(|| self.dir.join(id))
    }
}

/// Session ids are UUIDs; anything else, a separator or a leading dot above all, is not one.
fn is_session_file_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn remove_quietly(path: &Path) {
    if let Err(err) = std::fs::remove_file(path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %path.display(), %err, "removing saved session output failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SavedOutput;
    use crate::test_support::ScratchDir;

    #[test]
    fn a_session_keeps_only_its_latest_output() {
        let dir = ScratchDir::new("saved-output-latest");
        let saved = SavedOutput::new(dir.join("output"));
        assert_eq!(saved.read("s"), None, "nothing saved yet");

        saved.write("s", b"first");
        saved.write("s", b"second");
        assert_eq!(saved.read("s"), Some(b"second".to_vec()));

        saved.remove("s");
        assert_eq!(saved.read("s"), None);
    }

    #[test]
    fn a_saved_file_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("saved-output-mode");
        let saved = SavedOutput::new(dir.join("output"));
        saved.write("s", b"secret");
        let mode = std::fs::metadata(dir.join("output").join("s"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_id_cannot_name_a_file_outside_the_directory() {
        let dir = ScratchDir::new("saved-output-traversal");
        std::fs::write(dir.join("secret"), b"not output").unwrap();
        let saved = SavedOutput::new(dir.join("output"));
        std::fs::create_dir_all(dir.join("output")).unwrap();

        for id in ["../secret", "..", ".partial-0", ""] {
            saved.write(id, b"x");
            assert_eq!(saved.read(id), None, "{id:?}");
        }
        assert_eq!(std::fs::read(dir.join("secret")).unwrap(), b"not output");
        assert_eq!(std::fs::read_dir(dir.join("output")).unwrap().count(), 0);
    }

    #[test]
    fn the_sweep_keeps_the_named_sessions_and_nothing_else() {
        let dir = ScratchDir::new("saved-output-sweep");
        let saved = SavedOutput::new(dir.to_path_buf());
        saved.write("kept", b"k");
        saved.write("deleted", b"d");
        std::fs::write(dir.join(".partial-7"), b"half").unwrap();

        saved.sweep(|id| id == "kept");

        let mut left: Vec<String> = std::fs::read_dir(&*dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["kept"]);
    }
}
