//! The tools with which a session looks at what changed in a project's repository,
//! `list_project_changes` and `read_project_diff`, so that nobody has to run `git status` or
//! `git diff` there: both rewrite the repository's index, and a console session is not to run git
//! in a project at all. The reads, their reach and their gate are `project_browse`'s; what is added
//! here is a bound on what one answer carries into an agent's context: a page of the change list,
//! a window of a diff.

use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use base64::Engine as _;
use serde_json::{json, Map, Value};

use crate::branch_origin;
use crate::browse::budget;
use crate::browse::git::GitEnv;
use crate::browse::wire_path::{self, RelPath};
use crate::protocol::{
    BrowseBody, ChangeEntry, ChangeGroup, ChangeRef, ChangeSide, ComparedChangeRef, ContentKind,
    Event, FileContent, GitSourceInfo, Project, Session, SideKind, SideRead, SideRef, WorktreeInfo,
};
use crate::state::AppState;

use super::args::{optional_bool, optional_count, optional_string};
use super::project_browse::{browse, git_source, plain, readable_project, MakeGit};

/// How many changes `list_project_changes` returns when the caller names no `limit`.
const DEFAULT_CHANGES_LIMIT: usize = 200;

/// The largest `limit` it takes; a larger one is refused.
const MAX_CHANGES_LIMIT: usize = 1000;

/// How much of a diff `read_project_diff` returns when the caller names no `max_bytes`.
const DEFAULT_DIFF_BYTES: usize = 32 * 1024;

/// The most of a diff it returns in one answer, however much is asked for.
const MAX_DIFF_BYTES: usize = 128 * 1024;

pub(super) async fn list_project_changes(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    list_with(state, caller, arguments, GitEnv::from_shell).await
}

pub(super) async fn read_project_diff(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    read_with(state, caller, arguments, GitEnv::from_shell).await
}

/// The worktree `wanted` names: its path or the branch checked out in it, spelled plain or as the
/// daemon spells it (a wire path).
async fn find_worktree<'a>(git: &'a GitSourceInfo, wanted: &str) -> Result<&'a WorktreeInfo> {
    let path = wanted.trim_end_matches('/');
    let wire_path_of_plain = wire_path::encode(path.as_bytes());
    let wire_branch_of_plain = wire_path::encode(wanted.as_bytes());
    // The daemon reports canonical roots, so a path spelled through a link (`/tmp` on macOS) is
    // compared by where it leads; only its metadata is read.
    // A bare branch name is not resolved against the daemon's working directory.
    let canonical = if Path::new(path).is_absolute() {
        tokio::fs::canonicalize(path)
            .await
            .ok()
            .map(|path| wire_path::encode(path.as_os_str().as_bytes()))
    } else {
        None
    };
    let found = git.worktrees.iter().find(|worktree| {
        worktree.root == path
            || worktree.root == wire_path_of_plain
            || canonical.as_deref() == Some(worktree.root.as_str())
            || worktree
                .branch
                .as_deref()
                .is_some_and(|branch| branch == wanted || branch == wire_branch_of_plain)
    });
    found.ok_or_else(|| {
        let known: Vec<String> = git
            .worktrees
            .iter()
            .map(|worktree| match &worktree.branch {
                Some(branch) => format!("{} ({})", plain(&worktree.root), plain(branch)),
                None => format!("{} (detached)", plain(&worktree.root)),
            })
            .collect();
        anyhow!(
            "no worktree of the project's repository is at or on `{wanted}`; its worktrees are: {}",
            known.join(", ")
        )
    })
}

async fn list_with(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    make_git: MakeGit,
) -> Result<Value> {
    let project = readable_project(state, caller, arguments)?;
    let worktree = optional_string(arguments, "worktree");
    let branch = optional_string(arguments, "branch");
    let base = optional_string(arguments, "base");
    let commit = optional_string(arguments, "commit");
    let base_remote = optional_bool(arguments, "base_remote")?.unwrap_or(false);
    let limit =
        optional_count(arguments, "limit", 1, MAX_CHANGES_LIMIT)?.unwrap_or(DEFAULT_CHANGES_LIMIT);
    let offset = optional_count(arguments, "offset", 0, usize::MAX)?.unwrap_or(0);
    match (&branch, &base) {
        (None, _) if base.is_some() || base_remote || commit.is_some() => {
            bail!("`base`, `base_remote` and `commit` go with `branch`")
        }
        (Some(_), None) if base_remote => {
            bail!("`base_remote` goes with `base`: the recorded target branch is a local branch")
        }
        (Some(_), _) if worktree.is_some() => {
            bail!("`worktree` and `branch` choose different views; give one")
        }
        _ => {}
    }
    // Left out, `base` is the branch `branch` was created from, when that is recorded.
    let defaulted = match (&branch, &base) {
        (Some(branch), None) => Some(recorded_target(state, &project, branch).await?),
        _ => None,
    };
    let base = defaulted.clone().or(base);
    let page = Page { offset, limit };
    match (branch, base) {
        (Some(branch), Some(base)) => {
            let compared = Compared {
                branch: branch.clone(),
                base,
                base_remote,
                commit,
            };
            list_branch(state, make_git, &project, compared, page)
                .await
                .map_err(|err| match defaulted {
                    // The recorded target may be a branch that has since been deleted or renamed.
                    Some(target) => anyhow!(
                        "`base` was left out, so it is the target branch `{target}` recorded for \
                         `{branch}`, and the comparison failed: {err}"
                    ),
                    None => err,
                })
        }
        _ => list_uncommitted(state, make_git, &project, worktree, page).await,
    }
}

/// The branch `branch` of `project`'s repository was created from, as the daemon recorded it when
/// a session's git created `branch`: what `list_project_changes` compares it with when the caller
/// names no `base`.
async fn recorded_target(state: &Arc<AppState>, project: &Project, branch: &str) -> Result<String> {
    let wire = wire_path::encode(branch.as_bytes());
    let (state, path) = (state.clone(), project.path.clone());
    let wires = [wire.clone()];
    let mut recorded = tokio::task::spawn_blocking(move || {
        branch_origin::recorded_in_project(&state, &path, &wires)
    })
    .await??;
    recorded
        .remove(&wire)
        .and_then(|recorded| recorded.target_branch)
        .ok_or_else(|| {
            anyhow!(
                "`branch` needs `base`, the branch to compare it with: no target branch is \
                 recorded for `{branch}`, which is recorded only for a branch a session's git \
                 created from another local branch"
            )
        })
}

/// Which changes of a list an answer carries.
#[derive(Clone, Copy)]
struct Page {
    offset: usize,
    limit: usize,
}

/// The branch view's arguments.
struct Compared {
    branch: String,
    base: String,
    base_remote: bool,
    commit: Option<String>,
}

async fn list_uncommitted(
    state: &Arc<AppState>,
    make_git: MakeGit,
    project: &Project,
    worktree: Option<String>,
    page: Page,
) -> Result<Value> {
    let git = git_source(state, make_git, project).await?;
    let at = match &worktree {
        Some(wanted) => find_worktree(&git, wanted).await?,
        None => git
            .worktrees
            .iter()
            .find(|candidate| candidate.id == git.worktree)
            .ok_or_else(|| anyhow!("the project's own worktree is not listed by git"))?,
    };
    let body = BrowseBody::ListProjectChanges {
        project: project.id.clone(),
        worktree: worktree.is_some().then(|| at.id.clone()),
    };
    let (
        Event::ProjectChanges {
            head,
            changes,
            complete,
            ..
        },
        _admission,
    ) = browse(state, make_git, body, budget::CHANGE_LIST_RESERVATION).await?
    else {
        bail!("the daemon answered something other than the changes");
    };
    let mut answer = json!({
        "project": project.id,
        "view": "uncommitted",
        "worktree": {
            "path": plain(&at.root),
            "branch": at.branch.as_deref().map(plain),
            "head": head,
        },
    });
    add_changes(&mut answer, &changes, true, page, complete);
    Ok(answer)
}

async fn list_branch(
    state: &Arc<AppState>,
    make_git: MakeGit,
    project: &Project,
    compared: Compared,
    page: Page,
) -> Result<Value> {
    let Compared {
        branch,
        base,
        base_remote,
        commit,
    } = compared;
    let body = BrowseBody::CompareProjectBranches {
        project: project.id.clone(),
        left: wire_path::encode(base.as_bytes()),
        left_remote: base_remote,
        left_empty: false,
        right: wire_path::encode(branch.as_bytes()),
    };
    let mut answer = json!({ "project": project.id });
    // The comparison's changes are turned into their page before the next read is waited for, so
    // that they are not held outside the budget meanwhile.
    let (base_commit, branch_commit, merge_base) = {
        let (
            Event::ProjectComparison {
                left,
                right,
                base: merge_base,
                changes,
                complete,
                ..
            },
            _admission,
        ) = browse(state, make_git, body, budget::COMPARISON_RESERVATION).await?
        else {
            bail!("the daemon answered something other than a comparison");
        };
        if commit.is_none() {
            add_changes(&mut answer, &changes, false, page, complete);
        }
        (left.commit, right.commit, merge_base)
    };
    let body = BrowseBody::ListProjectCommits {
        project: project.id.clone(),
        base: merge_base.clone(),
        tip: branch_commit.clone(),
    };
    let (
        Event::ProjectCommits {
            commits,
            complete: changes_complete,
            commits_complete,
            ..
        },
        _admission,
    ) = browse(state, make_git, body, budget::COMMITS_RESERVATION).await?
    else {
        bail!("the daemon answered something other than a commit list");
    };
    if let Some(wanted) = commit {
        let Some(listed) = commits.iter().find(|listed| listed.commit == wanted) else {
            match (commits_complete, changes_complete) {
                (true, true) => bail!(
                    "`{wanted}` is not one of the commits `branch` made since it forked from `base`"
                ),
                (false, _) => bail!(
                    "`{wanted}` is not among the newest {} commits `branch` made since it forked \
                     from `base`, which are the ones listed; it may be an older one",
                    budget::MAX_COMMITS
                ),
                (true, false) => bail!(
                    "`{wanted}` is not among the commits listed, which the daemon cut short at its \
                     limit for changes; it may be one of those left out"
                ),
            }
        };
        // Only the last commit listed can hold part of its changes.
        let whole = changes_complete || commits.last().is_some_and(|last| last.commit != wanted);
        answer["view"] = json!("commit");
        answer["commit"] = json!(listed.commit);
        answer["parent"] = json!(listed.parent);
        answer["subject"] = json!(listed.subject);
        add_changes(&mut answer, &listed.changes, false, page, whole);
        return Ok(answer);
    }
    answer["view"] = json!("branch");
    answer["branch"] = json!(branch);
    answer["base"] = json!(base);
    answer["branch_commit"] = json!(branch_commit);
    answer["base_commit"] = json!(base_commit);
    answer["merge_base"] = json!(merge_base);
    answer["commits"] = json!(commits
        .iter()
        .map(|listed| json!({
            "commit": listed.commit,
            "parent": listed.parent,
            "subject": listed.subject,
            "files": listed.changes.len(),
        }))
        .collect::<Vec<Value>>());
    answer["commits_complete"] = json!(commits_complete && changes_complete);
    Ok(answer)
}

/// Puts the page of `changes` the caller asked for, and where it sits in them, on `answer`.
fn add_changes(
    answer: &mut Value,
    changes: &[ChangeEntry],
    grouped: bool,
    page: Page,
    complete: bool,
) {
    let end = page.offset.saturating_add(page.limit).min(changes.len());
    let start = page.offset.min(end);
    answer["changes"] = json!(changes[start..end]
        .iter()
        .map(|change| describe_change(change, grouped))
        .collect::<Vec<Value>>());
    answer["total"] = json!(changes.len());
    answer["offset"] = json!(page.offset);
    answer["next_offset"] = json!((end < changes.len()).then_some(end));
    answer["complete"] = json!(complete);
}

/// One change as the answer lists it. `grouped` puts the uncommitted `group` on it.
fn describe_change(change: &ChangeEntry, grouped: bool) -> Value {
    let (group, old, new, stats) = match change {
        ChangeEntry::Staged { old, new, stats } => ("staged", old, new, stats),
        ChangeEntry::Unstaged { old, new, stats } => ("unstaged", old, new, stats),
        ChangeEntry::Untracked { old, new, stats } => ("untracked", old, new, stats),
        ChangeEntry::Committed { old, new, stats } => ("committed", old, new, stats),
        ChangeEntry::Conflicted { path, conflict } => {
            return json!({
                "group": "conflicted",
                "status": serde_json::to_value(conflict).unwrap_or(Value::Null),
                "path": path,
                "lines_added": null,
                "lines_removed": null,
            });
        }
    };
    let mut entry = Map::new();
    if grouped {
        entry.insert("group".to_string(), json!(group));
    }
    let (status, path, old_path) = match (old, new) {
        (ChangeSide::Absent, ChangeSide::Present { path, .. }) => ("added", Some(path), None),
        (ChangeSide::Present { path, .. }, ChangeSide::Absent) => ("deleted", Some(path), None),
        (
            ChangeSide::Present {
                path: before,
                kind: kind_before,
                ..
            },
            ChangeSide::Present {
                path: after,
                kind: kind_after,
                ..
            },
        ) => {
            if before != after {
                ("renamed", Some(after), Some(before))
            } else if kind_before != kind_after {
                ("type_changed", Some(after), None)
            } else {
                ("modified", Some(after), None)
            }
        }
        (ChangeSide::OutOfScope { .. }, ChangeSide::Present { path, .. }) => {
            ("renamed", Some(path), None)
        }
        (ChangeSide::Present { path, .. }, ChangeSide::OutOfScope { .. }) => {
            ("renamed", Some(path), None)
        }
        _ => ("modified", None, None),
    };
    entry.insert("status".to_string(), json!(status));
    entry.insert("path".to_string(), json!(path));
    if let Some(old_path) = old_path {
        entry.insert("old_path".to_string(), json!(old_path));
    }
    let kind = match (new, old) {
        (ChangeSide::Present { kind, .. }, _) | (_, ChangeSide::Present { kind, .. }) => {
            Some(*kind)
        }
        _ => None,
    };
    if let Some(kind @ (SideKind::Symlink | SideKind::Submodule)) = kind {
        entry.insert(
            "kind".to_string(),
            serde_json::to_value(kind).unwrap_or(Value::Null),
        );
    }
    for (side, other) in [("old", old), ("new", new)] {
        if let ChangeSide::OutOfScope { repository_path } = other {
            entry.insert(
                "outside_project".to_string(),
                json!({ "side": side, "repository_path": repository_path }),
            );
        }
    }
    entry.insert(
        "lines_added".to_string(),
        json!(stats.map(|stats| stats.added)),
    );
    entry.insert(
        "lines_removed".to_string(),
        json!(stats.map(|stats| stats.removed)),
    );
    Value::Object(entry)
}

async fn read_with(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    make_git: MakeGit,
) -> Result<Value> {
    let project = readable_project(state, caller, arguments)?;
    let path = optional_string(arguments, "path").ok_or_else(|| anyhow!("`path` is required"))?;
    let old_path = optional_string(arguments, "old_path");
    for (key, given) in [("path", Some(&path)), ("old_path", old_path.as_ref())] {
        if let Some(given) = given {
            if RelPath::parse(given).is_none_or(|rel| rel.is_root()) {
                bail!(
                    "`{key}` `{given}` is not a path inside the project: it must be relative, \
                     without `.` or `..`, and spelled as `list_project_changes` gives it"
                );
            }
        }
    }
    let group = match optional_string(arguments, "group").as_deref() {
        None => None,
        Some("staged") => Some(ChangeGroup::Staged),
        Some("unstaged") => Some(ChangeGroup::Unstaged),
        Some("untracked") => Some(ChangeGroup::Untracked),
        Some(other) => bail!("`group` must be `staged`, `unstaged` or `untracked`, not `{other}`"),
    };
    let commit = optional_string(arguments, "commit");
    let parent = optional_string(arguments, "parent");
    let worktree = optional_string(arguments, "worktree");
    let offset = optional_count(arguments, "offset", 0, usize::MAX)?.unwrap_or(0);
    let max_bytes =
        optional_count(arguments, "max_bytes", 1, MAX_DIFF_BYTES)?.unwrap_or(DEFAULT_DIFF_BYTES);
    let old = old_path.clone().unwrap_or_else(|| path.clone());
    let present = |path: &String| SideRef::Present { path: path.clone() };

    let (patch, new, _admission) = match (group, commit) {
        (Some(_), Some(_)) => {
            bail!("`group` and `commit` name different kinds of change; give one")
        }
        (None, None) => bail!("name the change: `group` for an uncommitted one, or `commit`"),
        (Some(group), None) => {
            if parent.is_some() {
                bail!("`parent` goes with `commit`");
            }
            if group == ChangeGroup::Untracked && old_path.is_some() {
                bail!("an untracked file has no `old_path`");
            }
            let worktree = match worktree {
                Some(wanted) => {
                    let git = git_source(state, make_git, &project).await?;
                    Some(find_worktree(&git, &wanted).await?.id.clone())
                }
                None => None,
            };
            let change = ChangeRef {
                group,
                old: if group == ChangeGroup::Untracked {
                    SideRef::Absent
                } else {
                    present(&old)
                },
                new: present(&path),
            };
            let body = BrowseBody::ReadProjectChange {
                project: project.id.clone(),
                worktree,
                change,
            };
            let (Event::ProjectChange { patch, new, .. }, admission) =
                browse(state, make_git, body, budget::DIFF_RESERVATION).await?
            else {
                bail!("the daemon answered something other than a change");
            };
            (patch, *new, admission)
        }
        (None, Some(commit)) => {
            if worktree.is_some() {
                bail!("`worktree` goes with `group`: a commit is the same in every worktree");
            }
            let body = BrowseBody::ReadProjectCommitChange {
                project: project.id.clone(),
                commit,
                parent,
                change: ComparedChangeRef {
                    old: present(&old),
                    new: present(&path),
                },
            };
            let (Event::ProjectCommitChange { patch, new, .. }, admission) =
                browse(state, make_git, body, budget::DIFF_RESERVATION).await?
            else {
                bail!("the daemon answered something other than a change");
            };
            (patch, *new, admission)
        }
    };
    let shown = shown_diff(patch, &new);
    let mut answer = json!({ "project": project.id, "path": path });
    if let Some(old_path) = old_path {
        answer["old_path"] = json!(old_path);
    }
    window(&mut answer, shown, offset, max_bytes)?;
    Ok(answer)
}

/// What a read has to show: the kind of text, the text itself when there is one, and a note.
struct Shown {
    format: &'static str,
    text: Option<String>,
    size: u64,
    note: Option<&'static str>,
}

/// The patch of a change, or, where the change has none, the body of its new side: an untracked
/// file's, whose content is all a reader has.
fn shown_diff(patch: Option<FileContent>, new: &SideRead) -> Shown {
    if let Some(patch) = patch {
        let size = patch.size;
        let (text, note) = match (patch.kind, patch.text, patch.data) {
            (ContentKind::Text, Some(text), _) => (text, None),
            (_, _, Some(data)) => match base64::engine::general_purpose::STANDARD.decode(data) {
                Ok(bytes) => (
                    String::from_utf8_lossy(&bytes).into_owned(),
                    Some(
                        "the patch is not plain text; its bytes that are not UTF-8 are shown as \
                         U+FFFD",
                    ),
                ),
                Err(_) => (String::new(), Some("the patch could not be decoded")),
            },
            _ => (String::new(), None),
        };
        let binary = !text.contains("\n@@ ")
            && text
                .lines()
                .any(|line| line.starts_with("Binary files ") || line == "GIT binary patch");
        return Shown {
            format: if binary { "binary" } else { "patch" },
            text: Some(text),
            size,
            note,
        };
    }
    match new {
        SideRead::Present {
            file: Some(file), ..
        } => match (file.kind, &file.text) {
            (ContentKind::Text, Some(text)) => Shown {
                format: "file",
                text: Some(text.clone()),
                size: file.size,
                note: None,
            },
            _ => Shown {
                format: "binary",
                text: None,
                size: file.size,
                note: None,
            },
        },
        _ => Shown {
            format: "none",
            text: None,
            size: 0,
            note: Some(
                "the change has no content to show: a link, a submodule or a side outside the \
                 project",
            ),
        },
    }
}

/// Puts the window of `shown` the caller asked for on `answer`, cut on a line where one lies in
/// the window and always on a character.
fn window(answer: &mut Value, shown: Shown, offset: usize, max_bytes: usize) -> Result<()> {
    let text = shown.text.as_deref();
    let total = text.map_or(0, str::len);
    if offset > total {
        bail!("`offset` {offset} is past the end of the diff, which is {total} bytes");
    }
    let mut start = offset;
    while text.is_some_and(|text| !text.is_char_boundary(start)) {
        start += 1;
    }
    let mut end = start.saturating_add(max_bytes).min(total);
    if let Some(text) = text {
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end < total {
            if let Some(newline) = text[start..end].rfind('\n') {
                end = start + newline + 1;
            }
        }
        // A window too short to hold even one character would never advance.
        if end == start && start < total {
            end = start + text[start..].chars().next().map_or(0, char::len_utf8);
        }
    }
    answer["format"] = json!(shown.format);
    answer["text"] = json!(text.map(|text| &text[start..end]));
    answer["size_bytes"] = json!(shown.size);
    answer["offset"] = json!(start);
    answer["truncated"] = json!(end < total);
    answer["next_offset"] = json!((end < total).then_some(end));
    answer["note"] = json!(shown.note);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, RepoState};
    use crate::mcp::project_browse::test_git;
    use crate::test_support::{git, ScratchDir};

    async fn list(fixture: &RepoState, caller: &str, input: Value) -> Result<Value> {
        let session = fixture.state.session_record(caller)?;
        list_with(&fixture.state, &session, &arguments(input), test_git).await
    }

    async fn read(fixture: &RepoState, caller: &str, input: Value) -> Result<Value> {
        let session = fixture.state.session_record(caller)?;
        read_with(&fixture.state, &session, &arguments(input), test_git).await
    }

    fn paths(answer: &Value) -> Vec<(String, String, String)> {
        answer["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| {
                (
                    change["group"].as_str().unwrap_or("").to_string(),
                    change["status"].as_str().unwrap().to_string(),
                    change["path"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    fn entry(group: &str, status: &str, path: &str) -> (String, String, String) {
        (group.to_string(), status.to_string(), path.to_string())
    }

    /// The uncommitted changes of a project, by group, and the diff of each kind of change, read
    /// by a console session and by project sessions.
    #[tokio::test]
    async fn uncommitted_changes_are_listed_and_their_diffs_read() {
        let fixture = RepoState::new("project-changes-uncommitted");
        let repo = &fixture.repo;
        std::fs::write(repo.join("a"), "a\nmore\n").unwrap();
        std::fs::write(repo.join("staged.txt"), "staged\n").unwrap();
        git(repo, &["add", "staged.txt"]);
        std::fs::write(repo.join("new.txt"), "fresh\n").unwrap();
        std::fs::write(repo.join("blob.bin"), b"\x00\x01\x02").unwrap();

        for caller in ["a", "lead", "lead-kid"] {
            let listed = list(&fixture, caller, json!({ "project": "repo" }))
                .await
                .unwrap();
            assert_eq!(listed["view"], "uncommitted", "{caller}");
            assert_eq!(listed["complete"], true);
            assert_eq!(listed["total"], 4);
            assert_eq!(listed["next_offset"], Value::Null);
            assert_eq!(listed["worktree"]["branch"], "main");
            assert_eq!(
                paths(&listed),
                [
                    entry("unstaged", "modified", "a"),
                    entry("staged", "added", "staged.txt"),
                    entry("untracked", "added", "blob.bin"),
                    entry("untracked", "added", "new.txt"),
                ],
                "{caller}"
            );
            let modified = &listed["changes"][0];
            assert_eq!(modified["lines_added"], 2);
            assert_eq!(modified["lines_removed"], 1);
        }
        // A project session names no project to read its own.
        let own = list(&fixture, "lead", json!({})).await.unwrap();
        assert_eq!(own["project"], "repo");
        let err = list(&fixture, "a", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("`project` is required"), "{err}");

        let unstaged = read(
            &fixture,
            "a",
            json!({ "project": "repo", "path": "a", "group": "unstaged" }),
        )
        .await
        .unwrap();
        assert_eq!(unstaged["format"], "patch");
        assert!(unstaged["text"].as_str().unwrap().contains("+more"));
        assert_eq!(unstaged["truncated"], false);
        assert_eq!(unstaged["next_offset"], Value::Null);
        let staged = read(
            &fixture,
            "lead",
            json!({ "path": "staged.txt", "group": "staged" }),
        )
        .await
        .unwrap();
        assert!(staged["text"].as_str().unwrap().contains("+staged"));
        let untracked = read(
            &fixture,
            "lead-kid",
            json!({ "path": "new.txt", "group": "untracked" }),
        )
        .await
        .unwrap();
        assert_eq!(untracked["format"], "file");
        assert_eq!(untracked["text"], "fresh\n");
        let binary = read(
            &fixture,
            "a",
            json!({ "project": "repo", "path": "blob.bin", "group": "untracked" }),
        )
        .await
        .unwrap();
        assert_eq!(binary["format"], "binary");
        assert_eq!(binary["text"], Value::Null);
        assert_eq!(binary["size_bytes"], 3);
    }

    /// Another worktree is chosen by its path or by its branch, and a name no worktree has is
    /// refused with the ones there are.
    #[tokio::test]
    async fn another_worktree_is_chosen_by_path_or_branch() {
        let fixture = RepoState::new("project-changes-worktree");
        let checkouts = ScratchDir::new("project-changes-worktree-checkouts");
        let path = checkouts.join("side");
        git(
            &fixture.repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "side",
                path.to_str().unwrap(),
            ],
        );
        std::fs::write(path.join("only-here.txt"), "x\n").unwrap();
        for wanted in [path.to_str().unwrap(), "side"] {
            let listed = list(
                &fixture,
                "a",
                json!({ "project": "repo", "worktree": wanted }),
            )
            .await
            .unwrap();
            assert_eq!(
                paths(&listed),
                [entry("untracked", "added", "only-here.txt")]
            );
            assert_eq!(listed["worktree"]["branch"], "side");
            let diff = read(
                &fixture,
                "a",
                json!({
                    "project": "repo", "path": "only-here.txt", "group": "untracked",
                    "worktree": wanted,
                }),
            )
            .await
            .unwrap();
            assert_eq!(diff["text"], "x\n");
        }
        let own = list(&fixture, "a", json!({ "project": "repo" }))
            .await
            .unwrap();
        assert_eq!(own["total"], 0);
        let err = list(
            &fixture,
            "a",
            json!({ "project": "repo", "worktree": "nope" }),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("its worktrees are"), "{err}");
    }

    /// A `%` in a branch name or a worktree's directory is plain in the answer, and either spelling
    /// of it chooses the worktree; the comparison finds a branch whose name holds one.
    #[tokio::test]
    async fn a_percent_in_a_name_is_spelled_plain() {
        let fixture = RepoState::new("project-changes-percent");
        let checkouts = ScratchDir::new("project-changes-percent-checkouts");
        let path = checkouts.join("100%");
        let path = path.to_str().unwrap();
        git(
            &fixture.repo,
            &["worktree", "add", "-q", "-b", "50%off", path],
        );
        std::fs::write(Path::new(path).join("d.txt"), "d\n").unwrap();
        git(Path::new(path), &["add", "d.txt"]);
        git(Path::new(path), &["commit", "-q", "-m", "add d"]);
        // The daemon reports canonical roots (`/private/var` for `/var` on macOS).
        let root = std::fs::canonicalize(path).unwrap();
        let root = root.to_str().unwrap();
        let wire_root = root.replace('%', "%25");
        for wanted in [path, root, wire_root.as_str(), "50%off", "50%25off"] {
            let listed = list(
                &fixture,
                "a",
                json!({ "project": "repo", "worktree": wanted }),
            )
            .await
            .unwrap();
            assert_eq!(listed["worktree"]["path"], root, "{wanted}");
            assert_eq!(listed["worktree"]["branch"], "50%off", "{wanted}");
        }
        let compared = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "50%off", "base": "main" }),
        )
        .await
        .unwrap();
        assert_eq!(paths(&compared), [entry("", "added", "d.txt")]);
        assert_eq!(compared["branch"], "50%off");
    }

    /// A branch compared with its base: the changes, the commits, one commit's changes, and the
    /// diff of a file against the fork and in one commit.
    #[tokio::test]
    async fn a_branch_is_compared_with_its_base() {
        let fixture = RepoState::new("project-changes-branch");
        let repo = &fixture.repo;
        git(repo, &["switch", "-q", "-c", "feat"]);
        std::fs::write(repo.join("a"), "a\nfeat\n").unwrap();
        git(repo, &["commit", "-q", "-am", "edit a"]);
        std::fs::write(repo.join("d.txt"), "d\n").unwrap();
        git(repo, &["add", "d.txt"]);
        git(repo, &["commit", "-q", "-m", "add d"]);
        git(repo, &["switch", "-q", "main"]);

        let listed = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "feat", "base": "main" }),
        )
        .await
        .unwrap();
        assert_eq!(listed["view"], "branch");
        assert_eq!(
            paths(&listed),
            [entry("", "modified", "a"), entry("", "added", "d.txt")]
        );
        assert_eq!(listed["total"], 2);
        let commits = listed["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0]["subject"], "add d");
        assert_eq!(commits[0]["files"], 1);
        assert_eq!(listed["commits_complete"], true);
        assert_eq!(commits[1]["parent"], listed["merge_base"]);
        assert_eq!(listed["base_commit"], listed["merge_base"]);

        // The whole branch against the fork.
        let whole = read(
            &fixture,
            "lead",
            json!({
                "project": "repo", "path": "a",
                "commit": listed["branch_commit"], "parent": listed["merge_base"],
            }),
        )
        .await
        .unwrap();
        assert!(whole["text"].as_str().unwrap().contains("+feat"));
        // One commit's changes, and its file.
        let newest = commits[0]["commit"].as_str().unwrap();
        let one = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "feat", "base": "main", "commit": newest }),
        )
        .await
        .unwrap();
        assert_eq!(one["view"], "commit");
        assert_eq!(paths(&one), [entry("", "added", "d.txt")]);
        let diff = read(
            &fixture,
            "a",
            json!({
                "project": "repo", "path": "d.txt", "commit": newest,
                "parent": commits[0]["parent"],
            }),
        )
        .await
        .unwrap();
        assert!(diff["text"].as_str().unwrap().contains("+d"));

        let unknown = json!({
            "project": "repo", "branch": "feat", "base": "main", "commit": "0".repeat(40),
        });
        let err = list(&fixture, "a", unknown).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("is not one of the commits `branch` made"),
            "{err}"
        );
        let err = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "feat" }),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("needs `base`"), "{err}");
        let err = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "nope", "base": "main" }),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    /// The list is paged and a long diff is read a window at a time, whole once put together.
    #[tokio::test]
    async fn long_lists_and_diffs_are_paged_and_say_so() {
        let fixture = RepoState::new("project-changes-paging");
        let repo = &fixture.repo;
        for index in 0..5 {
            std::fs::write(repo.join(format!("f{index}.txt")), "x\n").unwrap();
        }
        let first = list(&fixture, "a", json!({ "project": "repo", "limit": 2 }))
            .await
            .unwrap();
        assert_eq!(first["changes"].as_array().unwrap().len(), 2);
        assert_eq!(first["total"], 5);
        assert_eq!(first["next_offset"], 2);
        let last = list(
            &fixture,
            "a",
            json!({ "project": "repo", "limit": 2, "offset": 4 }),
        )
        .await
        .unwrap();
        assert_eq!(last["changes"].as_array().unwrap().len(), 1);
        assert_eq!(last["next_offset"], Value::Null);

        let body: String = (0..400).map(|line| format!("line {line} é\n")).collect();
        std::fs::write(repo.join("big.txt"), &body).unwrap();
        let everything = json!({
            "project": "repo", "path": "big.txt", "group": "untracked", "max_bytes": 131072,
        });
        let whole = read(&fixture, "a", everything).await.unwrap();
        assert_eq!(whole["truncated"], false);
        assert_eq!(whole["text"], body.as_str());
        let mut put_together = String::new();
        let mut offset = 0;
        let mut windows = 0;
        loop {
            let window = read(
                &fixture,
                "a",
                json!({
                    "project": "repo", "path": "big.txt", "group": "untracked",
                    "max_bytes": 1000, "offset": offset,
                }),
            )
            .await
            .unwrap();
            windows += 1;
            assert_eq!(window["size_bytes"], body.len());
            put_together.push_str(window["text"].as_str().unwrap());
            match window["next_offset"].as_u64() {
                Some(next) => {
                    assert_eq!(window["truncated"], true);
                    assert!(window["text"].as_str().unwrap().ends_with('\n'));
                    offset = next as usize;
                }
                None => break,
            }
        }
        assert!(windows > 3, "{windows}");
        assert_eq!(put_together, body);
        let err = read(
            &fixture,
            "a",
            json!({ "project": "repo", "path": "big.txt", "group": "untracked", "offset": 999999 }),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("past the end"), "{err}");
    }

    /// A path is checked as a browse request checks it, and a change is named in one way.
    #[tokio::test]
    async fn paths_and_change_names_are_refused_when_they_do_not_hold() {
        let fixture = RepoState::new("project-changes-refusals");
        for path in ["../a", "/etc/passwd", "a/../b", "./a", "a//b", "%zz", "%61"] {
            let err = read(
                &fixture,
                "a",
                json!({ "project": "repo", "path": path, "group": "unstaged" }),
            )
            .await
            .unwrap_err();
            assert!(
                err.to_string().contains("not a path inside the project"),
                "{path}: {err}"
            );
        }
        let err = read(
            &fixture,
            "a",
            json!({ "project": "repo", "path": "a", "old_path": "../a", "group": "staged" }),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("`old_path`"), "{err}");
        for (input, refusal) in [
            (
                json!({ "project": "repo", "path": "a" }),
                "name the change: `group` for an uncommitted one, or `commit`",
            ),
            (
                json!({
                    "project": "repo", "path": "a", "group": "staged", "commit": "0".repeat(40),
                }),
                "`group` and `commit` name different kinds of change; give one",
            ),
            (
                json!({ "project": "repo", "path": "a", "group": "other" }),
                "`group` must be `staged`, `unstaged` or `untracked`, not `other`",
            ),
            (
                json!({
                    "project": "repo", "path": "a", "group": "staged", "parent": "0".repeat(40),
                }),
                "`parent` goes with `commit`",
            ),
            (
                json!({
                    "project": "repo", "path": "a", "commit": "0".repeat(40), "worktree": "x",
                }),
                "`worktree` goes with `group`",
            ),
            (
                json!({ "project": "repo", "path": "a", "group": "untracked", "old_path": "b" }),
                "an untracked file has no `old_path`",
            ),
        ] {
            let err = read(&fixture, "a", input.clone()).await.unwrap_err();
            assert!(err.to_string().contains(refusal), "{input}: {err}");
        }
        // A commit is a full id of a commit that is there.
        let zeros = "0".repeat(40);
        for (commit, refusal) in [
            ("main", "main is not a full commit id".to_string()),
            ("abc", "abc is not a full commit id".to_string()),
            (zeros.as_str(), format!("there is no commit {zeros}")),
        ] {
            let err = read(
                &fixture,
                "a",
                json!({ "project": "repo", "path": "a", "commit": commit }),
            )
            .await
            .unwrap_err();
            assert_eq!(err.to_string(), refusal);
        }
        for (input, refusal) in [
            (
                json!({ "project": "repo", "base": "main" }),
                "`base`, `base_remote` and `commit` go with `branch`",
            ),
            (
                json!({ "project": "repo", "branch": "x", "base": "main", "worktree": "y" }),
                "`worktree` and `branch` choose different views; give one",
            ),
            (json!({ "project": "repo", "limit": 0 }), "limit"),
            (json!({ "project": "repo", "limit": 1001 }), "limit"),
        ] {
            let err = list(&fixture, "a", input.clone()).await.unwrap_err();
            assert!(err.to_string().contains(refusal), "{input}: {err}");
        }
    }

    /// Another console's project is not read, as `list_projects` lists it without its git status;
    /// neither can a session of another console read this console's.
    #[tokio::test]
    async fn only_the_callers_own_consoles_projects_are_read() {
        let fixture = RepoState::new("project-changes-consoles");
        let err = list(&fixture, "a", json!({ "project": "project-2" }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("project of another console"),
            "{err}"
        );
        let err = read(
            &fixture,
            "a",
            json!({ "project": "project-2", "path": "a", "group": "staged" }),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("project of another console"),
            "{err}"
        );
        let err = list(&fixture, "foreign", json!({ "project": "repo" }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("project of another console"),
            "{err}"
        );
        // A project session of the other console reads its own console's project.
        let err = list(&fixture, "foreign", json!({})).await.unwrap_err();
        assert!(!err.to_string().contains("another console"), "{err}");
    }

    /// A branch compared with no `base` is compared with the branch it was recorded as created
    /// from; one with no such record or whose recorded target is gone, or a remote `base`, is
    /// refused.
    #[tokio::test]
    async fn a_branch_without_a_base_is_compared_with_its_recorded_target() {
        let fixture = RepoState::new("project-changes-target");
        let repo = &fixture.repo;
        crate::test_support::branch_made_from(&fixture.state, repo, "feat", "lead", Some("main"));
        git(repo, &["switch", "-q", "feat"]);
        std::fs::write(repo.join("d.txt"), "d\n").unwrap();
        git(repo, &["add", "d.txt"]);
        git(repo, &["commit", "-q", "-m", "add d"]);
        git(repo, &["switch", "-q", "main"]);
        fixture.branch("plain", "lead");

        let listed = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "feat" }),
        )
        .await
        .unwrap();
        assert_eq!(listed["view"], "branch");
        assert_eq!(listed["base"], "main");
        assert_eq!(paths(&listed), [entry("", "added", "d.txt")]);

        // No target is recorded for a branch made from nothing local.
        let err = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "plain" }),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("no target branch is recorded for `plain`"),
            "{err}"
        );
        // A recorded target that is gone since is named, as the base that was defaulted to.
        crate::test_support::branch_made_from(
            &fixture.state,
            repo,
            "orphaned",
            "lead",
            Some("vanished"),
        );
        let err = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "orphaned" }),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("so it is the target branch `vanished` recorded for `orphaned`")
                && err
                    .to_string()
                    .contains("there is no local branch vanished"),
            "{err}"
        );
        let err = list(
            &fixture,
            "a",
            json!({ "project": "repo", "branch": "feat", "base_remote": true }),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("`base_remote` goes with `base`"),
            "{err}"
        );
    }

    #[test]
    fn a_diff_window_ends_on_a_line_and_on_a_character() {
        let shown = |text: &str| Shown {
            format: "patch",
            text: Some(text.to_string()),
            size: text.len() as u64,
            note: None,
        };
        let mut answer = json!({});
        window(&mut answer, shown("ab\ncd\nef\n"), 0, 7).unwrap();
        assert_eq!(answer["text"], "ab\ncd\n");
        assert_eq!(answer["next_offset"], 6);
        let mut answer = json!({});
        window(&mut answer, shown("éééé"), 1, 3).unwrap();
        assert_eq!(answer["text"], "é");
        assert_eq!(answer["offset"], 2);
        assert_eq!(answer["next_offset"], 4);
        let mut answer = json!({});
        window(&mut answer, shown("éé"), 0, 1).unwrap();
        assert_eq!(answer["text"], "é");
    }
}
