//! Comparing two local branches of a project's repository: the branches there are to choose from,
//! the changes between two branch tips that touch the project, and the diff of one of them.
//!
//! A comparison is what the right branch has changed since it forked from the left one, as
//! `git diff left...right` shows: the merge base of the two tips as the old side against the right
//! branch's tip as the new side, so what only the left branch has done is not in it. The branches
//! are resolved to their commits, and those to their merge base, once, when the comparison is
//! listed, and every read of one of its changes names the commits rather than the branches: a
//! branch moved or deleted while a comparison is shown cannot repoint it, and since a commit's
//! content never changes, nothing read from one is checked again. Two branches with no commit in
//! common have no merge base, and comparing them is refused.
//!
//! As with a worktree's changes, the listing compares the whole tree, so that a rename into or out
//! of the project's directory is found against the rest of the repository, and keeps only what
//! touches the project; a side outside it is named by its repository path alone, and nothing of
//! its content is read or returned.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use anyhow::Result;

use crate::browse::changes::{self, ChangeRead};
use crate::browse::git::{GitEnv, GitError};
use crate::browse::tree_diff;
use crate::browse::wire_path;
use crate::browse::{blob, budget, git_error, git_failed, json_escaped_len, Located};
use crate::protocol::{
    error_code, BranchInfo, ChangeEntry, CodedError, ComparedChangeRef, ComparisonEndpoint,
};
use crate::subprocess::BoundedFailure;

/// A repository's local branches, in byte order of their names.
#[derive(Debug)]
pub struct BranchList {
    pub branches: Vec<BranchInfo>,
    /// False when the list was cut at a budget.
    pub complete: bool,
    /// When the listing was taken. A list is a snapshot, and what has happened to the repository
    /// since it was taken is not in it; `branch_origin` holds its own records against this rather
    /// than against the time they are read. An [`Instant`] because only the order of the two
    /// matters, and it is the one clock a step of the wall clock cannot move.
    pub listed_at: Instant,
}

/// Lists the local branches of the repository `at` is in, each with the object id its reference
/// names: the commit at its tip. A broken branch — naming an object the repository does not have,
/// or one that is not a commit — is listed as it is rather than looked into, since asking `git` for
/// what a missing object is fails the whole listing; comparing it is refused
/// (`blob::resolve_branch`).
pub fn branches(env: &GitEnv, at: &Located, cancel: &AtomicBool) -> Result<BranchList> {
    branches_in(env, &at.root, cancel)
}

/// [`branches`] for the repository `dir` is in, where `dir` is any directory `git` can run in
/// there: a worktree, or the repository's common directory itself.
pub fn branches_in(env: &GitEnv, dir: &Path, cancel: &AtomicBool) -> Result<BranchList> {
    // Taken before `git` reads the refs, so nothing that happens while it runs is claimed to be
    // in the list.
    let listed_at = Instant::now();
    // One past the budget, so a cut list is told from one that just fits.
    let count = format!("--count={}", budget::MAX_BRANCHES + 1);
    let args: &[&[u8]] = &[
        b"--format=%(objectname) %(refname)",
        count.as_bytes(),
        b"refs/heads/",
    ];
    let stdout = env
        .run(
            dir,
            "for-each-ref",
            args,
            budget::BRANCH_LIST_STDOUT,
            cancel,
        )
        .map_err(git_error)?;
    let mut list = BranchList {
        branches: Vec::new(),
        complete: true,
        listed_at,
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
            origin: None,
        });
    }
    Ok(list)
}

/// Two branches resolved to their commits, the commit they forked at, and the changes from that
/// commit to the right branch's that touch the project.
#[derive(Debug)]
pub struct Comparison {
    pub left: ComparisonEndpoint,
    pub right: ComparisonEndpoint,
    /// The merge base of the two commits; the two commits themselves when they are one.
    pub base: String,
    pub changes: Vec<ChangeEntry>,
    /// False when the list was cut at a budget.
    pub complete: bool,
}

/// Resolves branches `left` and `right` (wire paths, names below `refs/heads/`) to the commits at
/// their tips and lists the changes from their merge base to the second's tip that touch the
/// project's scope, in the order `git` gives them. Two branches at the same commit have none, and
/// `git` is not asked; two with no common commit are refused (`no_merge_base`, or
/// `no_merge_base_shallow` in a shallow clone).
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
    let base = merge_base(env, at, &left, &right, cancel)?;
    let mut comparison = Comparison {
        changes: Vec::new(),
        complete: true,
        left,
        right,
        base,
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
        comparison.base.as_bytes(),
        comparison.right.commit.as_bytes(),
    ];
    let limit = tree_diff::tree_diff_stdout();
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
    let commits = (comparison.base.as_str(), comparison.right.commit.as_str());
    let mut path_bytes = 0;
    let mut fields = stdout.split(|&b| b == 0).filter(|f| !f.is_empty());
    while let Some(meta) = fields.next() {
        let Some(entry) = tree_diff::committed_record("diff-tree", meta, &mut fields, at, commits)?
        else {
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

/// The commit the two endpoints forked at: `git merge-base`'s best common ancestor, as `git diff
/// left...right` takes it. The commit itself when both are at one. `git merge-base` exits with
/// status 1 and prints nothing when there is none, which is told from a failure to run.
fn merge_base(
    env: &GitEnv,
    at: &Located,
    left: &ComparisonEndpoint,
    right: &ComparisonEndpoint,
    cancel: &AtomicBool,
) -> Result<String> {
    if left.commit == right.commit {
        return Ok(left.commit.clone());
    }
    let args: &[&[u8]] = &[left.commit.as_bytes(), right.commit.as_bytes()];
    match env.run(
        &at.root,
        "merge-base",
        args,
        budget::GIT_METADATA_STDOUT,
        cancel,
    ) {
        Ok(stdout) => {
            let base = String::from_utf8_lossy(&stdout).trim().to_string();
            let hex = base.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
            if hex && matches!(base.len(), 40 | 64) {
                Ok(base)
            } else {
                Err(git_failed(&format!(
                    "git merge-base printed `{base}`, which is not a commit id"
                )))
            }
        }
        // Status 1 is `git merge-base`'s answer for no common ancestor; a failure to run it is
        // another status.
        Err(GitError::Failed(failed)) if failed.code == Some(1) => {
            // A shallow clone lacks the history that would show the common commit, so the two
            // may well have one; the reason is told apart for it.
            let shallow = is_shallow(env, at, cancel);
            let code = if shallow {
                error_code::NO_MERGE_BASE_SHALLOW
            } else {
                error_code::NO_MERGE_BASE
            };
            Err(CodedError::raised(
                code,
                format!(
                    "{} and {} have no commit in common{}",
                    left.branch,
                    right.branch,
                    if shallow {
                        " in this shallow clone's history"
                    } else {
                        ""
                    }
                ),
                &[("left", &left.branch), ("right", &right.branch)],
            ))
        }
        Err(err) => Err(git_error(err)),
    }
}

/// Whether the repository is a shallow clone. A failure to ask counts as no: the answer only
/// words a refusal that is made either way.
fn is_shallow(env: &GitEnv, at: &Located, cancel: &AtomicBool) -> bool {
    let args: &[&[u8]] = &[b"--is-shallow-repository"];
    env.run(
        &at.root,
        "rev-parse",
        args,
        budget::GIT_METADATA_STDOUT,
        cancel,
    )
    .is_ok_and(|stdout| stdout.trim_ascii() == b"true")
}

/// The two endpoints of a comparison and the commit it was made from, as a read names them.
type Compared<'a> = (&'a ComparisonEndpoint, &'a ComparisonEndpoint, &'a str);

/// Checks each of `left` and `right`: a branch name that is one, and a commit that is a commit of
/// this repository; and `base` as such a commit. Whether `base` is the two commits' merge base is
/// not asked again: a read is held to commits of the repository, as a read of a commit is.
fn verify_endpoints(
    env: &GitEnv,
    at: &Located,
    (left, right, base): Compared,
    cancel: &AtomicBool,
) -> Result<()> {
    for endpoint in [left, right] {
        blob::branch_refname(env, &at.root, &endpoint.branch, cancel)?;
        blob::verify_commit(env, &at.root, &endpoint.commit, cancel)?;
    }
    blob::verify_commit(env, &at.root, base, cancel)
}

/// Reads one change of a comparison of `left` and `right` made from `base`, each checked first
/// ([`verify_endpoints`]). The branches are not looked up again; the commits alone decide what is
/// read: the old side from `base` and the new side from the right commit.
pub fn read(
    env: &GitEnv,
    at: &Located,
    compared: Compared,
    change: &ComparedChangeRef,
    cancel: &AtomicBool,
) -> Result<ChangeRead> {
    verify_endpoints(env, at, compared, cancel)?;
    let (_, right, base) = compared;
    changes::read_between(
        env,
        at,
        (base, &right.commit),
        (&change.old, &change.new),
        cancel,
    )
}

/// Reads the whole bodies of one change of a comparison, checked first as [`read`] checks them,
/// and read only when both sides are the versions `expected` names.
pub fn read_bodies(
    env: &GitEnv,
    at: &Located,
    compared: Compared,
    change: &ComparedChangeRef,
    expected: &changes::Expected,
    cancel: &AtomicBool,
) -> Result<changes::BodiesRead> {
    verify_endpoints(env, at, compared, cancel)?;
    let (_, right, base) = compared;
    changes::read_bodies_between(
        env,
        at,
        (base, &right.commit),
        (&change.old, &change.new),
        expected,
        cancel,
    )
}
