//! Comparing two local branches of a project's repository: the branches there are to choose from,
//! the changes between two branch tips that touch the project, and the diff of one of them.
//!
//! A comparison is the left branch's tip as the old side against the right branch's tip as the new
//! side — the two commits themselves, never their merge base. The branches are resolved to their
//! commits once, when the comparison is listed, and every read of one of its changes names those
//! commits rather than the branches: a branch moved or deleted while a comparison is shown cannot
//! repoint it, and since a commit's content never changes, nothing read from one is checked again.
//!
//! As with a worktree's changes, the listing compares the whole tree, so that a rename into or out
//! of the project's directory is found against the rest of the repository, and keeps only what
//! touches the project; a side outside it is named by its repository path alone, and nothing of
//! its content is read or returned.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::changes::{self, ChangeRead};
use crate::browse::git::{GitEnv, GitError};
use crate::browse::wire_path;
use crate::browse::{blob, budget, git_error, git_failed, json_escaped_len, Located};
use crate::protocol::{BranchInfo, ChangeEntry, ChangeSide, ComparedChangeRef, ComparisonEndpoint};
use crate::subprocess::BoundedFailure;

/// A repository's local branches, in byte order of their names.
#[derive(Debug)]
pub struct BranchList {
    pub branches: Vec<BranchInfo>,
    /// False when the list was cut at a budget.
    pub complete: bool,
}

/// Lists the local branches of the repository `at` is in, each with the object id its reference
/// names: the commit at its tip. A broken branch — naming an object the repository does not have,
/// or one that is not a commit — is listed as it is rather than looked into, since asking `git` for
/// what a missing object is fails the whole listing; comparing it is refused
/// (`blob::resolve_branch`).
pub fn branches(env: &GitEnv, at: &Located, cancel: &AtomicBool) -> Result<BranchList> {
    // One past the budget, so a cut list is told from one that just fits.
    let count = format!("--count={}", budget::MAX_BRANCHES + 1);
    let args: &[&[u8]] = &[
        b"--format=%(objectname) %(refname)",
        count.as_bytes(),
        b"refs/heads/",
    ];
    let stdout = env
        .run(
            &at.root,
            "for-each-ref",
            args,
            budget::BRANCH_LIST_STDOUT,
            cancel,
        )
        .map_err(git_error)?;
    let mut list = BranchList {
        branches: Vec::new(),
        complete: true,
    };
    let mut name_bytes = 0;
    // A reference name holds no space, newline or other control character, so one per line, its
    // fields split at the first space, is unambiguous.
    for line in stdout.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let mut fields = line.splitn(2, |&b| b == b' ');
        let (Some(oid), Some(refname)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Some(name) = refname
            .strip_prefix(b"refs/heads/")
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        let name = wire_path::encode(name);
        let bytes = json_escaped_len(name.as_bytes());
        if list.branches.len() == budget::MAX_BRANCHES
            || name_bytes + bytes > budget::MAX_BRANCH_NAME_BYTES
        {
            list.complete = false;
            break;
        }
        name_bytes += bytes;
        list.branches.push(BranchInfo {
            name,
            commit: String::from_utf8_lossy(oid).into_owned(),
        });
    }
    Ok(list)
}

/// Two branches resolved to their commits, and the changes between those that touch the project.
#[derive(Debug)]
pub struct Comparison {
    pub left: ComparisonEndpoint,
    pub right: ComparisonEndpoint,
    pub changes: Vec<ChangeEntry>,
    /// False when the list was cut at a budget.
    pub complete: bool,
}

/// Resolves branches `left` and `right` (wire paths, names below `refs/heads/`) to the commits at
/// their tips and lists the changes from the first to the second that touch the project's scope,
/// in the order `git` gives them. Two branches at the same commit have none, and `git` is not
/// asked.
///
/// The whole tree is compared, so a rename across the scope's boundary is found. When that output
/// is past its budget — far-apart branches of a large repository — a project below the
/// repository's root is compared within its own directory instead: a rename across its boundary
/// then reads as the addition or the deletion it is inside, as an unpaired one always does.
pub fn list(
    env: &GitEnv,
    at: &Located,
    left: &str,
    right: &str,
    cancel: &AtomicBool,
) -> Result<Comparison> {
    let left = ComparisonEndpoint {
        commit: blob::resolve_branch(env, &at.root, left, cancel)?,
        branch: left.to_string(),
    };
    let right = ComparisonEndpoint {
        commit: blob::resolve_branch(env, &at.root, right, cancel)?,
        branch: right.to_string(),
    };
    let mut comparison = Comparison {
        changes: Vec::new(),
        complete: true,
        left,
        right,
    };
    if comparison.left.commit == comparison.right.commit {
        return Ok(comparison);
    }
    let mut args: Vec<&[u8]> = vec![
        b"-r",
        b"-z",
        b"-M",
        b"--raw",
        b"--no-abbrev",
        comparison.left.commit.as_bytes(),
        comparison.right.commit.as_bytes(),
    ];
    let limit = comparison_stdout();
    let scope = &at.repository.scope;
    let stdout = match env.run(&at.root, "diff-tree", &args, limit, cancel) {
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_))) if !scope.is_root() => {
            // A literal pathspec (`GitEnv::run` sets `GIT_LITERAL_PATHSPECS`): the scope's
            // directory and everything below it.
            args.extend_from_slice(&[b"--", scope.as_bytes()]);
            env.run(&at.root, "diff-tree", &args, limit, cancel)
        }
        other => other,
    }
    .map_err(git_error)?;
    let commits = (
        comparison.left.commit.as_str(),
        comparison.right.commit.as_str(),
    );
    let mut path_bytes = 0;
    let mut fields = stdout.split(|&b| b == 0).filter(|f| !f.is_empty());
    while let Some(meta) = fields.next() {
        let Some(entry) = record(meta, &mut fields, at, commits)? else {
            continue;
        };
        let bytes = changes::entry_path_bytes(&entry);
        if comparison.changes.len() == budget::MAX_CHANGE_ENTRIES
            || path_bytes + bytes > budget::MAX_CHANGE_PATH_BYTES
        {
            comparison.complete = false;
            break;
        }
        path_bytes += bytes;
        comparison.changes.push(entry);
    }
    Ok(comparison)
}

/// One `git diff-tree --raw -z` record, `:<old mode> <new mode> <old id> <new id> <status>`
/// followed by its path — or, for a rename (`R<score>`), the path it came from and the one it went
/// to — as the change it is to the project, or `None` when neither side is inside it.
fn record<'a>(
    meta: &[u8],
    fields: &mut impl Iterator<Item = &'a [u8]>,
    at: &Located,
    (old_commit, new_commit): (&str, &str),
) -> Result<Option<ChangeEntry>> {
    let malformed = || {
        let shown = String::from_utf8_lossy(meta);
        git_failed(&format!(
            "git diff-tree printed `{shown}`, which is not a record it writes"
        ))
    };
    let parts: Vec<&[u8]> = meta
        .strip_prefix(b":")
        .ok_or_else(malformed)?
        .split(|&b| b == b' ')
        .collect();
    let [old_mode, new_mode, old_oid, new_oid, status] = parts[..] else {
        return Err(malformed());
    };
    let path = fields.next().ok_or_else(malformed)?;
    // Only `-C` would add copies, which name two paths as well; renames are all there are.
    let to = match status.first() {
        Some(b'R') => Some(fields.next().ok_or_else(malformed)?),
        _ => None,
    };
    let scope = &at.repository.scope;
    let side = |commit: &str, mode: &[u8], oid: &[u8], path: &[u8]| {
        let rel = changes::in_scope(scope, path);
        match rel {
            Some(rel) => changes::head_side(Some(commit), mode, oid, &rel),
            None => ChangeSide::OutOfScope {
                repository_path: wire_path::encode(path),
            },
        }
    };
    // A rename's record names the path it came from first and the one it went to second.
    let (old, new) = (
        side(old_commit, old_mode, old_oid, path),
        side(new_commit, new_mode, new_oid, to.unwrap_or(path)),
    );
    // Kept when a side is inside the project; one of an addition or a deletion outside it is
    // named outside on its other side too, so it is never kept.
    let inside = |side: &ChangeSide| matches!(side, ChangeSide::Present { .. });
    Ok((inside(&old) || inside(&new)).then_some(ChangeEntry::Committed { old, new }))
}

/// Reads one change of a comparison of `left` and `right`, each checked first: a branch name that
/// is one, and a commit that is a commit of this repository. The branches are not looked up again;
/// the commits alone decide what is read.
pub fn read(
    env: &GitEnv,
    at: &Located,
    left: &ComparisonEndpoint,
    right: &ComparisonEndpoint,
    change: &ComparedChangeRef,
    cancel: &AtomicBool,
) -> Result<ChangeRead> {
    for endpoint in [left, right] {
        blob::branch_refname(env, &at.root, &endpoint.branch, cancel)?;
        blob::verify_commit(env, &at.root, &endpoint.commit, cancel)?;
    }
    changes::read_between(
        env,
        at,
        (&left.commit, &right.commit),
        (&change.old, &change.new),
        cancel,
    )
}

/// Reads the whole bodies of one change of a comparison of `left` and `right`, checked first as
/// [`read`] checks them, and read only when both sides are the versions `expected` names.
pub fn read_bodies(
    env: &GitEnv,
    at: &Located,
    left: &ComparisonEndpoint,
    right: &ComparisonEndpoint,
    change: &ComparedChangeRef,
    expected: &changes::Expected,
    cancel: &AtomicBool,
) -> Result<changes::BodiesRead> {
    for endpoint in [left, right] {
        blob::branch_refname(env, &at.root, &endpoint.branch, cancel)?;
        blob::verify_commit(env, &at.root, &endpoint.commit, cancel)?;
    }
    changes::read_bodies_between(
        env,
        at,
        (&left.commit, &right.commit),
        (&change.old, &change.new),
        expected,
        cancel,
    )
}

/// The budget of the `git diff-tree` behind a comparison; a test can lower it, as nothing short of
/// tens of thousands of changed files reaches the real one.
fn comparison_stdout() -> usize {
    #[cfg(test)]
    if let Some(limit) = COMPARISON_STDOUT_OVERRIDE.with(std::cell::Cell::get) {
        return limit;
    }
    budget::COMPARISON_STDOUT
}

#[cfg(test)]
thread_local! {
    pub(super) static COMPARISON_STDOUT_OVERRIDE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}
