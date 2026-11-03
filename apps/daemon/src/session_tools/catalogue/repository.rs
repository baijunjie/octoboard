//! The read tools for a project's git repository: its changes, branches and diff, and the
//! branches a session is responsible for.

use serde_json::json;

use super::schemas::object_schema;
use super::ToolDef;

/// The branches a session is responsible for, with their worktrees: what `transfer_branches`
/// needs the caller to be responsible for, and the branches and worktrees archiving would try to
/// remove. Any session may read any session's.
pub(super) const LIST_SESSION_BRANCHES: ToolDef = ToolDef {
    name: "list_session_branches",
    description: "The git branches a session is responsible for, read-only, each with the \
                  project (`project`) it is in and the worktree it is checked out in: \
                  `worktree_status` is `checked_out`, `gone` (git's record of the worktree is \
                  left, the directory is not), `none` (no worktree has it) or `unknown` (it \
                  could not be read), `worktree` its path when there is one, and \
                  `uncommitted_changes` whether a checked-out worktree holds uncommitted changes \
                  (null when there is no worktree or it could not be read). Each branch also \
                  has `target_branch` (the local branch it was created from, the one to merge \
                  it into; null when it was created from something that is no local branch), \
                  `fork_point` (the commit it was created at) and `responsible_since` (when the \
                  session became responsible for it, in milliseconds since the epoch: its \
                  creation, or a handover), and `archive`: what archiving the session would do \
                  with it, as `archive_session` or a `done` report with no open items does, \
                  which never forces anything. `archive.removed` is true when the branch and \
                  its worktree both go; `archive.branch` and `archive.worktree` (null when no \
                  linked worktree has the branch: the main working copy is never removed, so a \
                  branch checked out there has `worktree_status` `checked_out` and a null \
                  `archive.worktree`) are each `{state, reason}`, with `state` one of \
                  `removable`, `uncommitted_changes`, `submodules`, `unmerged`, `checked_out`, \
                  `locked`, `project_folder` or `failed`, and everything but `removable` stays. \
                  `archive` is null when that could not be judged. Read it before archiving to \
                  decide on `keep`. Use it also to see which branches `transfer_branches`, or the \
                  `branches` of `open_session` and `request_session`, can take (each must be one \
                  the session is responsible for). `session` is any session, of any console or \
                  project, and defaults to this one. A console session is responsible for no \
                  branch, so its list is empty. `branches` in `get_session` and `list_sessions` \
                  names the same branches without the worktrees; `list_project_branches` lists \
                  a project's branches whoever is responsible for them.",
    schema: || {
        object_schema(
            json!({
                "session": {
                    "type": "string",
                    "description": "The session's id. Defaults to this session.",
                },
            }),
            &[],
        )
    },
};

/// What has changed in a project's repository, read through the browse layer instead of by running
/// `git status` or `git diff` there. See `project_changes`.
pub(super) const LIST_PROJECT_CHANGES: ToolDef = ToolDef {
    name: "list_project_changes",
    description: "Read-only: what has changed in a project's git repository, without running git \
                  there (`git status` and `git diff` rewrite the repository's index; this does \
                  not). The arguments choose one of three views. With neither `worktree` nor \
                  `branch`: the uncommitted changes of the worktree holding the project's \
                  directory (not necessarily this session's own worktree: reach that, or any \
                  other, with `worktree`). With `worktree`: the uncommitted changes of that \
                  worktree of the project's repository. With `branch` (and `base`, which \
                  defaults to the branch `branch` was created from when that is recorded): what \
                  `branch` has changed since it forked from `base` (as `git diff base...branch` \
                  shows it), and the commits it made since; with `commit` besides, the changes \
                  of that one commit. Only changes that touch the project's directory are \
                  listed. Every answer has `project` (its id), `view` (`uncommitted`, `branch` or \
                  `commit`), `changes`, `total` (all the changes there are), `offset` (where \
                  `changes` starts), `next_offset` (where the next page starts, `null` on the \
                  last) and `complete` (false when the daemon cut the list at its own limit, so \
                  `total` and the changes after it are not known). Each change has `status` \
                  (`added`, `deleted`, `modified`, `renamed`, `type_changed`, or for a conflicted \
                  path how it conflicts: `both_modified`, `both_added`, `both_deleted`, \
                  `added_by_us`, `added_by_them`, `deleted_by_us`, `deleted_by_them`), `path` \
                  (inside the project, spelled as `read_project_diff` takes it, with `%XX` escapes \
                  for a name that is not UTF-8 or holds a `%`; the removed path for a deletion), \
                  `old_path` (a rename's former path), `kind` (only for a `symlink` or a \
                  `submodule`; for a type change, the new side's), `outside_project` (only when \
                  the other end of a rename lies outside the project's directory: `side`, `old` or \
                  `new`, and its `repository_path`; such a change is still `renamed`, with no \
                  `old_path`, and inside the project amounts to an addition or a deletion), and \
                  `lines_added` and `lines_removed`, `null` where git counts none (a binary file, \
                  a link, a submodule, a change with a side outside the project, a very large \
                  change). In the uncommitted views each change also has `group`: `staged`, \
                  `unstaged`, `untracked` or `conflicted`; one path can be \
                  in two groups, and a conflicted path has no diff to read. The uncommitted \
                  views add `worktree`: `path` and `branch` as plain text, as \
                  `list_session_branches` gives them (`branch` is `null` when HEAD is detached or \
                  unborn), and `head`, the commit the staged changes are against (`null` before \
                  the first commit). The `branch` view adds `branch`, `base`, `branch_commit` \
                  and `base_commit` (the full ids of the two tips), `merge_base` (the commit \
                  they forked at; the changes are between it and `branch_commit`), and \
                  `commits`, newest first and at most 100: `commit`, `parent` (its first \
                  parent, `null` for a root commit), `subject` and `files` (how many project \
                  files it changed); `commits_complete` is false when there were more commits \
                  than listed or the daemon cut the last commit's files short. The `commit` view \
                  has the commit's `commit`, `parent` and `subject`. Read the diff of a change \
                  with `read_project_diff`. Reading a changing worktree twice can give \
                  different lists, so a page of one is not a page of the other. To check a \
                  branch a session reports, give the branch alone: `base` is then its recorded \
                  target branch, the `target_branch` of `list_session_branches` and \
                  `list_project_branches`, which is refused as missing for a branch no session's \
                  git created from another local branch. For such a branch, or to compare with \
                  another, give the `base` it was made against, usually the target branch in \
                  the session's report or the branch the project's directory has checked out \
                  (`git_status.branch` in `list_projects`). Only projects of this session's own \
                  console can be read, even when the worktree or branch came from a session of \
                  another console.",
    schema: || {
        object_schema(
            json!({
                "project": {
                    "type": "string",
                    "description": "The project's id, or its name when that is unambiguous. \
                                    Defaults to this session's own project; a console session \
                                    has to give it. It must be a project of this session's own \
                                    console.",
                },
                "worktree": {
                    "type": "string",
                    "description": "Uncommitted changes of this worktree of the project's \
                                    repository instead of the one holding the project's \
                                    directory: its path (`worktree` in `list_session_branches`) \
                                    or the branch checked out in it. It must be a worktree of \
                                    this project's repository. Not with `branch`.",
                },
                "branch": {
                    "type": "string",
                    "description": "A local branch to list the changes of since it forked from \
                                    `base`. Needs `base` unless a target branch is recorded \
                                    for it. Not with `worktree`.",
                },
                "base": {
                    "type": "string",
                    "description": "With `branch`: the branch it is compared with, a local \
                                    branch (a remote-tracking one, such as `origin/main`, with \
                                    `base_remote`). Left out, the target branch recorded for \
                                    `branch`, which `base_remote` does not go with.",
                },
                "base_remote": {
                    "type": "boolean",
                    "description": "With `branch`: `base` names a remote-tracking branch, as of \
                                    the last fetch; nothing contacts the remote. Defaults to \
                                    false.",
                },
                "commit": {
                    "type": "string",
                    "description": "With `branch`: the full id of one of the commits the \
                                    answer's `commits` lists; the answer then holds that \
                                    commit's changes against its first parent.",
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "How many changes to return, 200 when left out.",
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many changes: a previous answer's `next_offset`. \
                                    Defaults to 0.",
                },
            }),
            &[],
        )
    },
};

/// The local branches of a project's repository with the session each is recorded as the
/// responsibility of, read through the browse layer. See `project_branches`.
pub(super) const LIST_PROJECT_BRANCHES: ToolDef = ToolDef {
    name: "list_project_branches",
    description: "Read-only: the local branches of a project's git repository, without running \
         git there, in byte order of their names. Each has `branch`, `tip` (the commit it names), \
         `upstream` (`branch` and `commit` of the remote-tracking branch, only for the branch the \
         repository's main working copy has checked out, and only when it has one; null \
         otherwise), `worktree` (null, or the worktree that has it checked out: `path`, `main` \
         for the repository's main working copy, `holds_project_directory` for the one the \
         project's directory is in) and `origin`: who the daemon has recorded as responsible for \
         it, null when nobody is. `origin` has `session` (its id, which may name a session that \
         no longer exists), `title` (the session's title when it became responsible), \
         `session_state` (`live`, `interrupted`, `archived` or `gone`: no record of the session \
         is left), `since` (milliseconds since the epoch: the branch's creation by that session, \
         or a handover), `target_branch` (the local branch it was created from, the one to merge \
         it into; null when it was created from something that is no local branch) and \
         `fork_point` (the commit it was created at). Use it to see who holds a branch, or \
         whether the session that did is gone, instead of searching every session; a branch of a \
         session that is `gone` cannot be taken with `transfer_branches`. The answer has \
         `project`, `total` (all the branches there are), `offset`, `next_offset` (`null` on the \
         last page), `complete` (false when the daemon cut the list at its own limit), \
         `upstreams_complete` (false when the lookup of the main working copy's upstream was cut \
         short) and `default_branch` (`branch` and `commit` of the remote's default branch, given \
         only when the main working copy's branch has no `upstream` and the daemon found it, as \
         the Git mode compares that branch with it; null otherwise). Only projects of this \
         session's own console can be read.",
    schema: || {
        object_schema(
            json!({
                "project": {
                    "type": "string",
                    "description": "The project's id, or its name when that is unambiguous. \
                                    Defaults to this session's own project; a console session \
                                    has to give it. It must be a project of this session's own \
                                    console.",
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "How many branches to return, 200 when left out.",
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many branches: a previous answer's `next_offset`. \
                                    Defaults to 0.",
                },
            }),
            &[],
        )
    },
};

/// The diff of one change `list_project_changes` lists. See `project_changes`.
pub(super) const READ_PROJECT_DIFF: ToolDef = ToolDef {
    name: "read_project_diff",
    description: "Read-only: the diff of one changed file of a project, as `list_project_changes` \
                  lists the change, without running git there. Name the file with `path` (and \
                  `old_path` for a rename), then say which change it is: an uncommitted one with \
                  `group` (and `worktree` for a worktree other than the one holding the \
                  project's directory), or one between two commits with `commit` and `parent`: \
                  for a commit of the `branch` view pass its `commit` and `parent`, and for the \
                  whole branch against where it forked pass the answer's `branch_commit` as \
                  `commit` and its `merge_base` as `parent`. The answer has `project`, `path`, \
                  `old_path` (when given), `format`, `text`, `size_bytes`, `offset`, \
                  `next_offset`, `truncated` and `note`. `format` says what `text` holds: \
                  `patch` is the unified diff as git writes it (a file changed only in its mode \
                  or only renamed has a patch with no hunk; an empty `text` means the path has \
                  no difference there, which is no change); `file` is the whole content of an \
                  untracked file, which has no diff, every line added; `binary` is a binary \
                  change, whose `text` is git's one-line `Binary files ... differ` for a patch \
                  and `null` for an untracked file; `none` is a link or submodule with no \
                  content, `text` `null`. `size_bytes` is the full size of the diff or file in \
                  bytes as git has it. At most `max_bytes` of `text` are returned, ending on a \
                  line where it can: when the diff is longer `truncated` is true and \
                  `next_offset` is the `offset` to pass to read on (`null` otherwise); `offset` \
                  and `next_offset` count the bytes of the returned text, which differ from \
                  `size_bytes` when `note` says bytes were replaced. An uncommitted change is \
                  read as it is now, so reading on after it changed can give a different text. \
                  A diff over 4 MiB, and a file over 4 MiB, are refused rather than cut. `note` \
                  is `null` unless something about `text` is worth knowing (a patch that is not \
                  plain text reads the bytes that are not UTF-8 as U+FFFD). Only projects of \
                  this session's own console can be read.",
    schema: || {
        object_schema(
            json!({
                "project": {
                    "type": "string",
                    "description": "The project's id, or its name when that is unambiguous. \
                                    Defaults to this session's own project; a console session \
                                    has to give it. It must be a project of this session's own \
                                    console.",
                },
                "path": {
                    "type": "string",
                    "description": "The file, relative to the project's directory, as a change \
                                    of `list_project_changes` gives it: a name that is not \
                                    UTF-8 or holds a `%` is spelled with `%XX` escapes there, \
                                    and the same here.",
                },
                "old_path": {
                    "type": "string",
                    "description": "For a renamed file: its former path, the change's \
                                    `old_path`.",
                },
                "group": {
                    "type": "string",
                    "enum": ["staged", "unstaged", "untracked"],
                    "description": "Which uncommitted change of the path: the change's `group`. \
                                    Not with `commit`.",
                },
                "worktree": {
                    "type": "string",
                    "description": "With `group`: the worktree the change is in, as for \
                                    `list_project_changes`. Defaults to the one holding the \
                                    project's directory.",
                },
                "commit": {
                    "type": "string",
                    "description": "The full id of the commit holding the new version. Not with \
                                    `group`.",
                },
                "parent": {
                    "type": "string",
                    "description": "With `commit`: the full id of the commit holding the old \
                                    version. Left out, the old version is nothing, which is a \
                                    root commit's.",
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Where in the returned text to start, in bytes: a previous \
                                    answer's `next_offset`. Defaults to 0.",
                },
                "max_bytes": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 131072,
                    "description": "The most bytes of the diff to return, 32768 when left out.",
                },
            }),
            &["path"],
        )
    },
};
