//! The git branches and linked worktrees bound to sessions that are archived or deleted, and their
//! removal.
//!
//! Sessions store neither. A branch is bound to a session when its row names that session as its
//! responsible session (see [`crate::branch_origin`]) and it is still the branch the row was made
//! for; a linked worktree is bound when it has such a branch checked out.
//!
//! What can go safely goes without anyone being asked: a worktree through `git worktree remove`
//! without `--force`, which git refuses for one with modified or untracked files, initialized
//! submodules or a lock (one whose directory is already gone it takes, record and all), and then a
//! branch through `git branch -d`, git's own check that it is merged into its upstream (into
//! `HEAD` when it has none). Every command runs in the repository's common directory, where `HEAD`
//! is the main worktree's, and which outlives any worktree. The main worktree is never removed, nor
//! a worktree that is a project's folder or holds one, nor the home directory, the root of a disk
//! or one holding Octoboard's data, nor one of which that cannot be told
//! ([`survey::WorktreeRemovability`]); a branch still checked out in a worktree that stays is left,
//! as git would refuse it anyway.
//!
//! What git would refuse — a worktree with uncommitted changes or submodules, an unmerged branch, a
//! locked worktree — goes only when the caller names it as forced (`--force`, `-D`, `--force` twice
//! for a lock), and only while it still stands as the caller was shown it: one that has changed
//! since is left and reported rather than forced further than the user agreed to. Only the user's
//! confirmation forces anything: an agent's own archive never does.
//!
//! Removal waits for the sessions' processes to end, since an agent still running may be working
//! in its worktree, and skips a session that is no longer archived by then (one resumed in the
//! meantime). The removals of one repository run one at a time. A failure to remove an item never
//! fails the archive or deletion it follows: the item is reported as left, unless it turns out to
//! be gone all the same.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub mod project;
pub mod remove;
pub mod report;
pub mod survey;
#[cfg(test)]
mod tests;

/// What `AppState` holds for the cleanup: one gate per repository, which keeps an archive's and a
/// deletion's removals in the same repository from running at once and acting on each other's
/// half-done work. A gate is dropped once nobody holds or waits on it.
#[derive(Default)]
pub struct Cleanups {
    gates: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl Cleanups {
    fn gates(&self) -> MutexGuard<'_, HashMap<PathBuf, Arc<Mutex<()>>>> {
        self.gates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs `work` holding the gate of `repo`. It guards an order rather than any state, so a
    /// poisoned one is taken all the same.
    fn serialized<T>(&self, repo: &Path, work: impl FnOnce() -> T) -> T {
        let gate = Arc::clone(self.gates().entry(repo.to_path_buf()).or_default());
        let done = {
            let _held = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            work()
        };
        let mut gates = self.gates();
        // The map's own handle and this one: nobody else holds or waits on it.
        if Arc::strong_count(&gate) == 2 {
            gates.remove(repo);
        }
        done
    }
}
