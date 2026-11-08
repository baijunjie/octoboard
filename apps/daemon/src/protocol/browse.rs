//! The browse requests and the records they answer with: a project's source, directory listings,
//! file contents, changes, branches and commits. See "Browsing a project" in
//! `apps/daemon/PROTOCOL.md`.

use serde::{Deserialize, Serialize};

use super::fields::is_false;

/// Where a project's files are read from, as `project_source` reports it. See "Project sources" in
/// `apps/daemon/PROTOCOL.md`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSourceInfo {
    pub project: String,
    /// The project's path as stored.
    pub root: String,
    /// The directory that path resolves to, symbolic links followed, as a wire path.
    pub resolved_root: String,
    /// The resolved directory's identity; it changes when the directory is replaced.
    pub root_id: String,
    /// The repository holding the project's directory; `null` when it is in none.
    pub git: Option<GitSourceInfo>,
    /// Why the repository could not be read, verbatim, when its directory looks like one but `git`
    /// failed on it; `null` otherwise.
    pub git_error: Option<String>,
    /// The code of the failure `git_error` describes where the daemon has one, so a client words it
    /// itself: only `binary_not_found`, for a `git` that cannot be found on the login shell's
    /// `PATH`. Left out for any other failure, and with no `git_error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub git_error_code: Option<super::codes::Code>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct GitSourceInfo {
    /// The repository's identity: that of its common git directory.
    pub repository: String,
    pub common_dir: String,
    /// The worktree holding the project's directory.
    pub worktree: String,
    /// The project's directory relative to that worktree's root, as a wire path; empty at the root.
    pub scope: String,
    pub worktrees: Vec<WorktreeInfo>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct WorktreeInfo {
    pub id: String,
    pub root: String,
    pub main: bool,
    /// The commit checked out; `null` in a repository with no commit yet.
    pub head: Option<String>,
    /// The branch checked out, as a wire path; `null` when `HEAD` is detached or unborn.
    pub branch: Option<String>,
    /// Whether the project's scope exists as a directory in this worktree.
    pub scope_present: bool,
    /// Why it does not: `source_unavailable`'s `cause`, or `permission_denied` for a read the system
    /// denied; absent while it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub scope_cause: Option<&'static str>,
}

/// One entry of a project directory listing.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct BrowseEntry {
    /// The entry's name, as a wire path.
    pub name: String,
    pub kind: EntryKind,
    /// A regular file's size in bytes; `null` for anything else.
    pub size: Option<u64>,
    /// A regular file's version, equal to the `version` a live read of it would report while it
    /// stays unchanged; `null` for anything else.
    pub version: Option<String>,
    /// What a symbolic link resolves to; `null` for anything else.
    pub target: Option<LinkTarget>,
    /// Whether Git ignores the entry; sent only when true. See `browse::ignored`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub ignored: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    /// A FIFO, socket or device: listed, never read.
    Other,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    File,
    Directory,
    Other,
    /// The link resolves to nothing.
    Missing,
    /// The link resolves outside the scope, and is never followed.
    Outside,
}

/// Which content a `read_project_file` reads. Absent means `live`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReadFrom {
    #[default]
    Live,
    Index,
    /// A local branch's tip; `branch` is the name below `refs/heads/`, as a wire path.
    Branch {
        branch: String,
    },
    /// A commit, by its full object id.
    Commit {
        commit: String,
    },
}

/// What a file body was actually read from, so a reply identifies the content it carries.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentSource {
    /// The file on disk; `version` changes whenever its content may have.
    /// `root_id` is the identity of the directory the read was scoped to.
    Live { root_id: String, version: String },
    /// The blob staged in a worktree's index.
    Index { worktree: String, blob: String },
    /// The blob in a commit; `branch` is the branch it was resolved from, when it was.
    Commit {
        commit: String,
        #[serde(default)]
        branch: Option<String>,
        blob: String,
    },
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// Valid UTF-8 with no NUL byte; carried in `text`.
    Text,
    /// Anything else; carried base64-encoded in `data`.
    Binary,
}

/// A file body. Exactly one of `text` and `data` is set, by `kind`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct FileContent {
    pub size: u64,
    pub kind: ContentKind,
    /// The image type the bytes start like (`image/png`, …), a hint only; `null` when none.
    pub media_type: Option<&'static str>,
    pub text: Option<String>,
    pub data: Option<String>,
}

/// Which of a worktree's uncommitted changes an entry is: `HEAD` against the index (`staged`), the
/// index against the files on disk (`unstaged`), or a file on disk Git does not track and does not
/// ignore (`untracked`). Part of a change's identity, so one path can be a change in two groups.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGroup {
    Staged,
    Unstaged,
    Untracked,
}

/// What one side of a change is.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideKind {
    File,
    Symlink,
    Submodule,
}

/// One side of a change in a change listing. See "Changes and comparisons" in
/// `apps/daemon/PROTOCOL.md`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ChangeSide {
    /// `path` is a wire path relative to the project's directory; `source` the version listed.
    Present {
        path: String,
        kind: SideKind,
        source: ContentSource,
    },
    /// No such side: the old side of an addition, the new side of a deletion.
    Absent,
    /// A side outside the project's scope; `repository_path` is a wire path relative to the
    /// repository's root, for display only.
    OutOfScope { repository_path: String },
}

/// How a path is in conflict, as `git status` tells the stages apart.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    BothModified,
    BothAdded,
    BothDeleted,
    AddedByUs,
    AddedByThem,
    DeletedByUs,
    DeletedByThem,
}

/// The lines a change adds and removes, as `git diff --numstat` counts them. See "Changes and
/// comparisons" in `apps/daemon/PROTOCOL.md`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LineStats {
    pub added: u64,
    pub removed: u64,
}

/// One entry of a change listing: a change between two sides, or a path in conflict, which is
/// not a two-sided change at all. `stats` is what the change adds and removes, absent where the
/// daemon does not count it (see "Changes and comparisons" in `apps/daemon/PROTOCOL.md`).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "group", rename_all = "snake_case")]
pub enum ChangeEntry {
    Staged {
        old: ChangeSide,
        new: ChangeSide,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        stats: Option<LineStats>,
    },
    Unstaged {
        old: ChangeSide,
        new: ChangeSide,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        stats: Option<LineStats>,
    },
    Untracked {
        old: ChangeSide,
        new: ChangeSide,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        stats: Option<LineStats>,
    },
    /// `path` is a wire path relative to the project's directory.
    Conflicted {
        path: String,
        conflict: ConflictKind,
    },
    /// A change between two branch tips: never part of a worktree's listing, only of a
    /// comparison's, where the old side is the left tip's and the new side the right tip's.
    Committed {
        old: ChangeSide,
        new: ChangeSide,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        stats: Option<LineStats>,
    },
}

/// How a `read_project_change` or a `read_project_comparison_change` names one side of the
/// change to read.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SideRef {
    /// `path` is a wire path relative to the project's directory.
    Present {
        path: String,
    },
    Absent,
    OutOfScope,
}

/// The change a `read_project_change` reads: its group and its two sides, as the listing gave them.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChangeRef {
    pub group: ChangeGroup,
    pub old: SideRef,
    pub new: SideRef,
}

/// One local branch of a project's repository, as `list_project_branches` lists it: its name below
/// `refs/heads/`, as a wire path, and the object id its reference named when it was listed — the
/// commit at its tip, unless the branch is broken (a missing object, or not a commit).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub commit: String,
    /// For the branch the main working copy has checked out, and no other, the remote-tracking
    /// branch to compare it with: the remote branch it is set to track, else the branch of the
    /// same name on a configured remote (`origin`'s preferred), when the repository has that
    /// reference. A branch whose upstream is another local branch, or whose upstream reference is
    /// gone, is treated as having no upstream; absent when neither exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub upstream: Option<UpstreamInfo>,
    /// The session responsible for the branch, when the daemon can name one; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub origin: Option<BranchOriginInfo>,
}

/// A branch's upstream: the remote-tracking branch, as a wire path below `refs/remotes/` (such as
/// `origin/main`), and the object id its reference names.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamInfo {
    pub name: String,
    pub commit: String,
}

/// The session working on a branch now, with the title that session had when it became
/// responsible — the record outlives the session, which a project's deletion takes with it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchOriginInfo {
    /// The session working on the branch: the one whose own git reported creating it until the
    /// branch is handed to another. It may name a session that no longer exists.
    pub responsible_session: String,
    /// That session's title when it became responsible.
    pub responsible_title: String,
    /// When that session became responsible (milliseconds since the epoch): the branch's creation,
    /// or the transfer.
    pub responsible_since: i64,
}

/// One end of a branch comparison: the branch, as a wire path, and the full id of the commit it was
/// resolved to. A comparison reply reports both; a read of one of its changes names both back, with
/// the comparison's `base`, and the commits alone decide what is read. `remote` says the branch is
/// a remote-tracking one, named below `refs/remotes/`, rather than a local one.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonEndpoint {
    pub branch: String,
    pub commit: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub remote: bool,
    /// The comparison starts from the empty tree: `branch` is empty and `commit` is the empty
    /// tree's id, which is the comparison's `base` too.
    #[serde(default, skip_serializing_if = "is_false")]
    pub empty: bool,
}

/// One commit a branch made since it forked, as `list_project_commits` lists it: its full id, its
/// first parent's (`None` for a commit with no parent, whose changes are against the empty tree),
/// its subject line, and its changes against that parent that touch the project, each in the
/// `committed` group.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitChanges {
    pub commit: String,
    pub parent: Option<String>,
    pub subject: String,
    pub changes: Vec<ChangeEntry>,
}

/// The change a `read_project_comparison_change` or a `read_project_commit_change` reads: its two
/// sides, as the listing gave them.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ComparedChangeRef {
    pub old: SideRef,
    pub new: SideRef,
}

/// The version of one side of a change as a diff reply reported it, which a body request names back
/// so the bodies are read only from the sides the patch was made from.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SideVersion {
    Present { source: ContentSource },
    Absent,
}

/// One side of a change as `read_project_change` or `read_project_comparison_change` read it.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SideRead {
    /// `file` is the side's body, when the reply carries it: only a file side's, and only when no
    /// patch shows the change's content (a binary change, a side alone).
    ///
    /// `lines` is the number of lines the side's stored content holds (its newlines, and one more
    /// when the last line is unterminated), counted of the very blob the patch was made from;
    /// `None` where the daemon read no content for the side.
    Present {
        path: String,
        kind: SideKind,
        source: ContentSource,
        file: Option<FileContent>,
        lines: Option<u64>,
    },
    Absent,
    OutOfScope,
}

/// One browse request frame: the envelope's id, the optional slot a newer request supersedes an
/// older one in, and the request itself. Kept apart from `Request` because these are served
/// under their own budgets and on their own outbound lane — see "Browsing a project" in
/// `apps/daemon/PROTOCOL.md`.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
pub struct BrowseRequest {
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub id: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub slot: Option<String>,
    /// Whether a person has just asked for this reply (opened a menu, pressed Try again or
    /// Refresh), rather than a client asking again while it is on screen: a `git` the cached shell
    /// snapshot does not find is then looked for once more in a fresh one.
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub asked_by_person: bool,
    #[serde(flatten)]
    pub body: BrowseBody,
}

/// The `type`s that are browse requests rather than [`RequestBody`] ones.
///
/// [`RequestBody`]: super::RequestBody
pub const BROWSE_REQUEST_TYPES: &[&str] = &[
    "get_project_source",
    "list_project_dir",
    "search_project_files",
    "read_project_file",
    "list_project_changes",
    "read_project_change",
    "list_project_branches",
    "compare_project_branches",
    "read_project_comparison_change",
    "read_project_change_bodies",
    "read_project_comparison_change_bodies",
    "list_project_commits",
    "read_project_commit_change",
    "read_project_commit_change_bodies",
];

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowseBody {
    GetProjectSource {
        project: String,
    },
    ListProjectDir {
        project: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
        path: String,
    },
    SearchProjectFiles {
        project: String,
        query: String,
        /// Whether files Git ignores, and what is inside ignored folders, can match.
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        include_ignored: bool,
    },
    ReadProjectFile {
        project: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
        path: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        from: ReadFrom,
    },
    ListProjectChanges {
        project: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
    },
    ReadProjectChange {
        project: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
        change: ChangeRef,
    },
    ListProjectBranches {
        project: String,
    },
    /// `left` and `right` are branch names below `refs/heads/`, as wire paths; the changes are
    /// those the right branch has made since it forked from the left one. With `left_remote`, `left`
    /// is a name below `refs/remotes/` instead; with `left_empty`, `left` is ignored and the changes
    /// are against the empty tree.
    CompareProjectBranches {
        project: String,
        left: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        left_remote: bool,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        left_empty: bool,
        right: String,
    },
    ReadProjectComparisonChange {
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        /// The commit the comparison's changes start from, as its reply gave it.
        base: String,
        change: ComparedChangeRef,
    },
    /// The full bodies of the two sides of a change `read_project_change` made a patch of; `old`
    /// and `new` are the versions that reply reported for its sides.
    ReadProjectChangeBodies {
        project: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        worktree: Option<String>,
        change: ChangeRef,
        old: SideVersion,
        new: SideVersion,
    },
    /// As `ReadProjectChangeBodies`, for a change of a comparison.
    ReadProjectComparisonChangeBodies {
        project: String,
        left: ComparisonEndpoint,
        right: ComparisonEndpoint,
        base: String,
        change: ComparedChangeRef,
        old: SideVersion,
        new: SideVersion,
    },
    /// The commits reachable from `tip` and not from `base`, both full commit ids — a comparison's
    /// right commit and its merge base — newest first, each with its own changes.
    ListProjectCommits {
        project: String,
        base: String,
        tip: String,
    },
    /// One change of a commit, against its first parent as the commit list gave it: `parent` is
    /// `None` for a commit with no parent.
    ReadProjectCommitChange {
        project: String,
        commit: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        parent: Option<String>,
        change: ComparedChangeRef,
    },
    /// As `ReadProjectChangeBodies`, for a change of a commit.
    ReadProjectCommitChangeBodies {
        project: String,
        commit: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional = nullable))]
        parent: Option<String>,
        change: ComparedChangeRef,
        old: SideVersion,
        new: SideVersion,
    },
}
