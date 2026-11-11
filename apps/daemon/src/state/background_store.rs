//! The background work of each running session, kept in `background-tasks.json` in the data
//! directory (owner-only, replaced whole) so that a daemon that restarts while the agents go on
//! running, planned or not, still knows what each left running in the background: nothing the
//! agent reports tells it again until its next subagent stop or turn end, and a session adopted
//! with an empty list reads as done in the meantime.
//!
//! The file follows [`AppState::background_tasks`]: written again on every change of a session's
//! list, and also when an emptying starts waiting (the saved copy is empty during the wait, while
//! the list is not). The session's entry is removed with the session's other bookkeeping, once its
//! process has ended, on the blocking pool and not if the session has a process again by then. A
//! daemon that dies leaves it as it was, which is what the next one reads. Each entry carries the
//! launch it was written for, and is taken only by a session adopted running that launch
//! ([`AppState::restore_background_work`]): any other session's process is not the one that left
//! the work running.
//!
//! The next daemon loads it whole at start into [`AppState::background_tasks`] as well
//! ([`AppState::load_saved_background`]), so the reports that follow apply on top of it; the
//! adoption keeps that work or clears it by the launch, unless a full list reported meanwhile has
//! replaced it. An entry stays in the file, and its work in memory, until its session is settled:
//! [`AppState::retain_saved_background`] drops those of sessions the start neither adopted nor
//! claimed for a holder it could not link to, and one claimed is dropped when its claim is
//! released without a process adopted for it (a holder linked to only later still finds its work
//! meanwhile, though nothing shows it until the process is adopted). A file that does not read,
//! or is of another version, is ignored and rewritten by the next change.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::background::BackgroundWork;
use super::AppState;
use crate::hostfs::replace::{remove_replaced, replace_beside, Permissions};
use crate::protocol::BackgroundTask;

/// The version of the file's format this build writes and reads; a file of another is ignored.
const VERSION: u32 = 1;

/// One session's background work, as saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SavedWork {
    /// The launch of the session's process the work was left running by.
    pub launch: String,
    pub tasks: Vec<BackgroundTask>,
    /// The descriptions seen for each subagent id (`BackgroundWork::descriptions`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub descriptions: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedFile {
    #[serde(default)]
    version: u32,
    sessions: BTreeMap<String, SavedWork>,
}

/// The entries of the file, and where it is. Without a path (until
/// [`AppState::load_saved_background`] names one) nothing is written.
#[derive(Default)]
pub(super) struct SavedBackground {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    path: Option<PathBuf>,
    sessions: BTreeMap<String, SavedWork>,
}

impl Inner {
    /// Replaces the file with the entries, or removes it with none left. A write that fails is
    /// reported and not retried until the next change.
    fn write(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let written = if self.sessions.is_empty() {
            remove_replaced(path)
        } else {
            let file = SavedFile {
                version: VERSION,
                sessions: self.sessions.clone(),
            };
            serde_json::to_vec(&file)
                .map_err(std::io::Error::other)
                .and_then(|bytes| replace_beside(path, &bytes, Permissions::Private))
        };
        if let Err(err) = written {
            tracing::warn!(path = %path.display(), %err, "saving the sessions' background work failed; a restart now would not know of it");
        }
    }
}

/// What [`SavedBackground::sync`] finds the session's work to be.
pub(super) enum Current {
    /// The session has no process to read it for: the saved entry stays as it is, which keeps the
    /// work of a session claimed for a holder and not adopted yet.
    Unchanged,
    /// The work to save, `None` for none.
    Work(Option<SavedWork>),
}

impl SavedBackground {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("saved background lock poisoned")
    }

    /// Sets the saved entry of `id` to what `current` gives, which is read under the lock so that
    /// the last of two changes of one session to get here writes what is current, whichever made
    /// its change first; [`Current::Work`] of `None` removes it. Writes the file only when the
    /// entry changed. **Blocks** on the file.
    pub(super) fn sync(&self, id: &str, current: impl FnOnce() -> Current) {
        let mut inner = self.lock();
        let Current::Work(work) = current() else {
            return;
        };
        if inner.sessions.get(id) == work.as_ref() {
            return;
        }
        match work {
            Some(work) => inner.sessions.insert(id.to_string(), work),
            None => inner.sessions.remove(id),
        };
        inner.write();
    }

    fn retain(&self, keep: &impl Fn(&str) -> bool) {
        let mut inner = self.lock();
        let before = inner.sessions.len();
        inner.sessions.retain(|id, _| keep(id));
        if inner.sessions.len() != before {
            inner.write();
        }
    }
}

impl AppState {
    /// Reads the file at `path`, which the entries are written to from now on. Called at start,
    /// before anything is adopted. **Blocks** on the file.
    pub fn load_saved_background(&self, path: &Path) {
        let sessions = match std::fs::read(path) {
            Ok(content) => match serde_json::from_slice::<SavedFile>(&content) {
                Ok(file) if file.version == VERSION => file.sessions,
                Ok(file) => {
                    tracing::warn!(path = %path.display(), version = file.version, "the background work the previous daemon saved is of a version this build does not read; it is ignored");
                    BTreeMap::new()
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), %err, "the background work the previous daemon saved does not read; it is ignored");
                    BTreeMap::new()
                }
            },
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %path.display(), %err, "reading the background work the previous daemon saved failed");
                }
                BTreeMap::new()
            }
        };
        let mut all = self
            .background_tasks
            .lock()
            .expect("background tasks lock poisoned");
        for (id, saved) in &sessions {
            all.insert(id.clone(), BackgroundWork::restored(saved.clone()));
        }
        drop(all);
        let mut inner = self.saved_background.lock();
        inner.path = Some(path.to_path_buf());
        inner.sessions = sessions;
    }

    /// Drops the saved entry, and the work held in memory, of every session `keep` says no to.
    /// **Blocks** on the file.
    pub fn retain_saved_background(&self, keep: impl Fn(&str) -> bool) {
        self.saved_background.retain(&keep);
        self.background_tasks
            .lock()
            .expect("background tasks lock poisoned")
            .retain(|id, _| keep(id));
    }
}

#[cfg(test)]
mod tests;
