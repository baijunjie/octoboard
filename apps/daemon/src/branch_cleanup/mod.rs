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

use std::path::{Path, PathBuf};

use crate::branch_origin::KeyedGates;
use crate::git::run::{Policy, Runner};
use crate::git::worktrees::WORKTREE_GIT_LIMITS;
use crate::host_tools::Tools;

pub mod project;
pub mod remove;
pub mod report;
pub mod survey;
#[cfg(test)]
mod tests;

/// No configured filesystem monitor for the cleanup's writes either, which a read never starts
/// (`crate::git::env`): `git worktree remove` checks the worktree it removes with a status of its
/// own, and a monitor started there would be a program run for a worktree on its way out. A read
/// gets the setting a second time from its guard, which changes nothing.
const UNMONITORED: Policy = Policy::new(&["-c", "core.fsmonitor=false"], &[]);

/// The cleanup's `git`: the user's own, as `tools` hands it out, under [`UNMONITORED`] and
/// [`WORKTREE_GIT_LIMITS`].
pub(crate) fn cleanup_git(tools: &Tools) -> anyhow::Result<Runner> {
    (tools.git)().map(|git| Runner::new(git, UNMONITORED, WORKTREE_GIT_LIMITS))
}

/// What `AppState` holds for the cleanup: one gate per repository, which keeps an archive's and a
/// deletion's removals in the same repository from running at once and acting on each other's
/// half-done work.
#[derive(Default)]
pub struct Cleanups {
    gates: KeyedGates<PathBuf>,
}

impl Cleanups {
    /// Runs `work` holding the gate of `repo`.
    fn serialized<T>(&self, repo: &Path, work: impl FnOnce() -> T) -> T {
        self.gates.serialized(repo, work)
    }
}
