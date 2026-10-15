//! What the daemon holds in memory about each repository's branch origins: the gate serializing
//! every write to its rows, and when the last write happened. Held on `AppState`; none of it is
//! persisted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

#[derive(Default)]
pub struct Gates {
    repos: Mutex<HashMap<PathBuf, Repository>>,
}

#[derive(Default)]
struct Repository {
    /// When the last write that added rows ran; see [`Gates::wrote_at`].
    wrote_at: Option<Instant>,
    /// Serializes this repository's writes; see [`Gates::gate`].
    gate: Arc<Mutex<()>>,
}

impl Gates {
    fn lock(&self) -> MutexGuard<'_, HashMap<PathBuf, Repository>> {
        self.repos.lock().expect("branch origin gates poisoned")
    }

    /// The gate serializing every write to one repository's rows. A report, a listing's reconcile
    /// and a project's deletion each read the repository's rows, decide from that snapshot and
    /// then write, so two of them at once lose an attribution or delete a row the other just
    /// wrote; several clients can be listing at once, and reports come from background processes
    /// in any number.
    ///
    /// It guards an order rather than any state, so a panic inside it poisons nothing worth
    /// keeping: a poisoned gate is taken all the same, since refusing it would leave the
    /// repository unattributable for the rest of the daemon's life.
    pub(super) fn gate(&self, repo: &Path) -> Arc<Mutex<()>> {
        Arc::clone(&self.lock().entry(repo.to_path_buf()).or_default().gate)
    }

    /// When rows were last added to the repository. A branch listing taken before then is no
    /// evidence about which branches exist now, however complete it is, since the branch that
    /// write recorded cannot be in it.
    ///
    /// Only the order of the two matters, so both are [`Instant`]s: a wall clock stepped back —
    /// by NTP, or by the user — would otherwise put every listing before the last write for as
    /// long as the step, which stops a row ever being removed for a deleted branch.
    pub(super) fn wrote_at(&self, repo: &Path) -> Option<Instant> {
        self.lock().get(repo).and_then(|entry| entry.wrote_at)
    }

    pub(super) fn wrote(&self, repo: &Path, at: Instant) {
        self.lock().entry(repo.to_path_buf()).or_default().wrote_at = Some(at);
    }

    pub(super) fn forget(&self, repo: &Path) {
        self.lock().remove(repo);
    }

    #[cfg(test)]
    pub(super) fn holds_no_repository(&self) -> bool {
        self.lock().is_empty()
    }
}
