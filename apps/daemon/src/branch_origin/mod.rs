//! Which session is responsible for a git branch, the one working on it now, kept so the Git mode
//! can mark the relation: the record of branch origins and targets ([`record`], [`target`]), the
//! reconcile that holds the rows against the repository's branches ([`reconcile`]), the forks
//! listing ([`forks`]), the handover of a branch to another session ([`handover`]), the listing
//! of a session's branches, and what is bound to a session ([`bound`]). The reflog files they lean
//! on are read by [`crate::git::reflog`].
//!
//! A branch is first attributed, to the session that made it, only when that session's own git says
//! it created it: a project session's agent runs with `core.hooksPath` pointed at Octoboard's hook
//! dispatchers ([`crate::session_git_hooks`]), and when a reference transaction of a git anywhere
//! in that agent's process tree commits a branch that did not exist while it was prepared —
//! whatever command, alias or script made it — the `reference-transaction` hook reports it, under
//! the session it was started for, to [`ref_created`]. A forced update of an existing branch names
//! no old value either, which is why existence is asked before the transaction writes anything.
//! Nothing else names a session: no timing, no working directory, no guess. A branch the hook never
//! reported has no responsible session, which is no failure, and cannot be handed over.
//!
//! A report is taken only for a repository the session's own project is in, and only while the
//! branch is still the one it reports: in a repository whose references are files, the branch's
//! reflog has to begin with its creation at the reported commit, at the time of the report. The
//! record lives in the daemon's SQLite (`store::BranchOrigin`) — the Git mode never writes to a
//! user's repository, so no ref, note or configuration entry carries it.
//!
//! A row also keeps the branch the new one was created from, its target branch ([`target`]); the
//! commit it was created at, its `birth_commit`, is the fork point. The revert gate holds the
//! branch's later work against both ([`crate::session_git_hooks::gate_inputs`]). A rename carries
//! the target along and a handover leaves it alone.
//!
//! A row's responsible session is the reporting session when the row is written, and another only
//! through an explicit handover ([`handover::hand_over`]) — never inferred. The row keeps the title
//! the session had then, to name it once it is gone; who reported the branch is not kept apart from
//! it.
//!
//! A row lives and dies with its branch. It is validated against the branch's first reflog entry on
//! every listing, so a branch deleted and made again under the same name does not inherit the old
//! attribution, and removed once a full listing taken after the row was written no longer has the
//! branch — unless the branch was renamed, which git records in the new name's reflog, and then the
//! row moves to the new name. A copy (`git branch -c`) is a branch the session did not create, and
//! gets nothing.
//!
//! These limits follow from the design:
//!
//! - In a repository whose references are in reftable format there are no reflog files to read, so
//!   a row is taken at the reported commit and the report's time, cannot be held against the
//!   branch's birth afterwards, and a rename is not followed: a branch deleted and made again under
//!   the same name between two listings keeps the first one's row until its name goes, and a
//!   branch deleted since the last listing is still taken for the one its row was made for.
//! - Elsewhere a row is held only as long as the reflog keeps the branch's first entry: once
//!   `gc.reflogExpire` (90 days by default) drops it, the row goes. A branch deleted and made again
//!   at the same commit inside one second has the same first entry and keeps the old row.

pub(crate) mod bound;
mod forks;
mod gate;
pub mod handover;
mod reconcile;
mod record;
pub mod session_branches;
mod target;
#[cfg(test)]
mod tests;

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::browse::budget;
use crate::git::reflog::{self, Creation};
use crate::git::repo_files::{common_dir_of, repository_of, uses_reftable};
use crate::state::AppState;
use crate::store::BranchOrigin;
use crate::wire_path;
pub use forks::{forks_in_project, recorded_in_project, targets_in, targets_in_project};
pub use gate::{Gates, KeyedGates};
#[cfg(test)]
pub use handover::hand_over;
pub use handover::{first_not_held_by, hand_over_all};
pub use reconcile::annotate;
pub use record::ref_created;

/// What a session's `reference-transaction` hook reports of a committed transaction: the branches
/// it created, whether it moved any of the refs a branch header reads, and where git ran the hook.
/// Paths and reference names are wire paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefReport {
    /// The directory git ran the hook in: the top of the worktree, or the repository itself when it
    /// is bare.
    pub cwd: String,
    /// The `GIT_DIR` git set for the hook, which it does whenever the repository is not the `.git`
    /// found in `cwd`; relative to `cwd` when relative.
    pub git_dir: Option<String>,
    /// The branch (`refs/heads/...`) the creating git's `HEAD` was on while the transaction was
    /// prepared, for a start point of `HEAD` to name a branch by; absent for a detached `HEAD`, or
    /// from a dispatcher that did not capture it.
    #[serde(default)]
    pub head: Option<String>,
    pub refs: Vec<CreatedRef>,
    /// Whether the transaction touched `HEAD`, a local branch or a remote-tracking branch, which
    /// has the repository's projects re-read (`crate::project_git::ref_moves`); absent from a
    /// dispatcher that reports creations alone.
    #[serde(default)]
    pub moved: bool,
    /// When the report was made, in seconds since the epoch.
    pub unix_time: i64,
}

/// One reference a transaction created: its full name and the object id it was created at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedRef {
    pub name: String,
    pub oid: String,
}

/// The canonical common directory of the repository a hook ran in, from the directory git ran it in
/// and the `GIT_DIR` it set (wire paths): the one `GIT_DIR` names when git set it, else the one
/// holding the directory.
pub(crate) fn reported_repository(cwd: &str, git_dir: Option<&str>) -> Option<PathBuf> {
    let cwd = PathBuf::from(std::ffi::OsStr::from_bytes(&wire_path::decode(cwd)?));
    let Some(git_dir) = git_dir else {
        return repository_of(&cwd);
    };
    let git_dir = cwd.join(std::ffi::OsStr::from_bytes(&wire_path::decode(git_dir)?));
    common_dir_of(&git_dir)
}

/// The creation entry of the branch (a wire path) in the reflog of the repository at `repo`.
fn creation_of(repo: &Path, branch: &str) -> Option<Creation> {
    reflog::creation_of(repo, &wire_path::decode(branch)?)
}

// -- a project's deletion ----------------------------------------------------

/// Forgets the origins of the repository a deleted project was in, unless another project is in it
/// too. Called after the project is gone from the store. A project whose directory has since gone
/// cannot be traced to its repository, and neither can any repository when the store's list of
/// projects cannot be read: in both the rows stay, bounded by the per-repository cap.
pub fn project_deleted(state: &AppState, project_path: &str) {
    project_deleted_in(state, repository_of(Path::new(project_path)));
}

/// [`project_deleted`] for a repository found earlier, for a deletion that takes the directory
/// away first, after which it can no longer be traced.
pub fn project_deleted_in(state: &AppState, repo: Option<PathBuf>) {
    let Some(repo) = repo else {
        return;
    };
    let shared = crate::project_git::has_projects_in_repository(state, &repo).unwrap_or(true);
    if shared {
        return;
    }
    // Under the repository's gate, as every other write to its rows is: a reconcile or a report
    // that has read its snapshot and is still deciding would otherwise write that snapshot back
    // over the delete, leaving a repository with rows, no project to clean them up and nothing
    // that runs here again.
    state.branch_origins.serialized(&repo, || {
        if let Err(err) = state
            .store
            .delete_repository_branch_origins(&repo_key(&repo))
        {
            tracing::warn!(repository = %repo.display(), %err, "branch origins: clearing failed");
        }
        state.branch_origins.forget(&repo);
    });
}

/// Whether the branch is still the one `row` was made for. A reftable repository keeps no reflog
/// files to hold it against, and the row stands until the branch goes.
pub(crate) fn is_same_branch(repo: &Path, row: &BranchOrigin) -> bool {
    if uses_reftable(repo) {
        return true;
    }
    creation_of(repo, &row.branch)
        .is_some_and(|entry| entry.time == row.birth_time && entry.commit == row.birth_commit)
}

/// A session title as a record keeps it, so it outlives the session at a bounded size.
fn recorded_title(title: &str) -> String {
    title.chars().take(budget::MAX_ORIGIN_TITLE_CHARS).collect()
}

/// The key a repository's rows are stored under: its canonical common directory, as a wire path.
pub(crate) fn repo_key(repo: &Path) -> String {
    wire_path::encode(repo.as_os_str().as_bytes())
}
