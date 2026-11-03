//! The branches and worktrees bound to a session, as an inspection reports them and an archive, a
//! deletion or a project removal names them back. See "Branches and worktrees bound to a session"
//! in `apps/daemon/PROTOCOL.md`.

use serde::{Deserialize, Serialize};

/// A git branch or a linked worktree bound to a session: a branch whose responsible session it
/// is, or a linked worktree with such a branch checked out. What `inspect_session_branches`
/// reports, and what an archive or a deletion leaves behind. A project's removal also takes
/// worktrees bound to no session (`inspect_project_removal`).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionBranchItem {
    pub kind: SessionBranchKind,
    /// The repository's canonical common directory, as a wire path.
    pub repository: String,
    /// The branch, as a wire path; for a worktree, the branch it has checked out, `None` for one
    /// with a detached `HEAD`.
    pub branch: Option<String>,
    /// The worktree's canonical root, as a wire path; `None` for a branch.
    pub path: Option<String>,
    /// Whether the worktree has modified or untracked files, whatever its state; false for a
    /// branch, and where it was not read.
    pub uncommitted: bool,
    /// The commit a branch points at, as a full object id: what a forced removal is pinned to.
    /// Absent for a worktree, and where it could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub tip: Option<String>,
    /// The project of the session the item was found through.
    pub project: String,
    /// The branch's responsible session; `None` for a worktree a project's removal takes although
    /// no session is bound to it.
    pub session: Option<String>,
    pub state: SessionBranchState,
    /// Git's own message, for `failed`.
    pub detail: Option<String>,
    /// Why a `failed` item is left, where a client words that itself; `detail` then says it in
    /// English.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub cause: Option<SessionBranchCause>,
}

/// Why a `failed` [`SessionBranchItem`] is left, as a client words it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBranchCause {
    /// Whether the worktree is or holds another project's folder could not be told, so it is kept.
    ProjectFolderUnknown,
    /// The worktree is the home directory, the root of a disk or a directory holding Octoboard's
    /// data, so it is kept.
    ProtectedFolder,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBranchKind {
    Worktree,
    Branch,
}

/// Where a bound item stands. `uncommitted_changes`, `submodules`, `unmerged` and `locked` are what
/// a forced removal takes; `checked_out`, `project_folder` and `failed` are left whatever is asked.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBranchState {
    /// Goes without asking: a clean, unlocked worktree, or a branch git takes as merged.
    Removable,
    /// A worktree with modified or untracked files.
    UncommittedChanges,
    /// A worktree with initialized submodules, which git removes only by force.
    Submodules,
    /// A branch not merged into its upstream, or into `HEAD` when it has none.
    Unmerged,
    /// A branch checked out in a worktree that stays.
    CheckedOut,
    /// A locked worktree.
    Locked,
    /// A worktree that is a project's folder, or holds one.
    ProjectFolder,
    /// Git refused or failed to remove it, or to say where it stands, a worktree is the home
    /// directory, the root of a disk or a directory holding Octoboard's data, or whether it is or
    /// holds a project's folder could not be told.
    Failed,
}

/// A bound item a request names, as `SessionBranchItem` names it and in the state the user was
/// shown it in: a forced removal goes only while the item still stands that way.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionBranchTarget {
    pub kind: SessionBranchKind,
    pub repository: String,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub branch: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub path: Option<String>,
    pub state: SessionBranchState,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub uncommitted: bool,
    /// The branch's `tip` as the user was shown it: an archive's or a deletion's forced
    /// `git branch -D` goes only while the branch still points there.
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub tip: Option<String>,
}
