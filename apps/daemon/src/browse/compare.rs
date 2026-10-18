//! Comparing a local branch of a project's repository with a base: the branches there are to
//! choose from, the changes between the two that touch the project, and the diff of one of them.
//!
//! A comparison is what the right branch has changed since it forked from the left one, as
//! `git diff left...right` shows: the merge base of the two tips as the old side against the right
//! branch's tip as the new side, so what only the left branch has done is not in it. The left side
//! is a local branch, a remote-tracking branch, or nothing: against the empty tree every file of
//! the right branch's tip is an addition. The branches are resolved to their commits, and those to
//! their merge base, once, when the comparison is listed, and every read of one of its changes
//! names the commits rather than the branches: a branch moved or deleted while a comparison is
//! shown cannot repoint it, and since a commit's content never changes, nothing read from one is
//! checked again. Two branches with no commit in common have no merge base, and comparing them is
//! refused.
//!
//! As with a worktree's changes, the listing compares the whole tree, so that a rename into or out
//! of the project's directory is found against the rest of the repository, and keeps only what
//! touches the project; a side outside it is named by its repository path alone, and nothing of
//! its content is read or returned.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use anyhow::Result;

use crate::browse::changes::{self, ChangeRead};
use crate::browse::git::{GitEnv, GitError};
use crate::browse::line_counts;
use crate::browse::tree_diff;
use crate::browse::wire_path;
use crate::browse::{blob, budget, git_error, git_failed, json_escaped_len, Located};
use crate::protocol::{
    error_code, BranchInfo, ChangeEntry, CodedError, ComparedChangeRef, ComparisonEndpoint,
    UpstreamInfo,
};
use crate::subprocess::BoundedFailure;

/// A repository's local branches, in byte order of their names.
#[derive(Debug)]
pub struct BranchList {
    pub branches: Vec<BranchInfo>,
    /// False when the list was cut at a budget.
    pub complete: bool,
    /// False when the upstream lookup was past a budget: a branch checked out then lacks its
    /// upstream, though it may have one.
    pub upstreams_complete: bool,
    /// When the listing was taken. A list is a snapshot, and what has happened to the repository
    /// since it was taken is not in it; `branch_origin` holds its own records against this rather
    /// than against the time they are read. An [`Instant`] because only the order of the two
    /// matters, and it is the one clock a step of the wall clock cannot move.
    pub listed_at: Instant,
}

/// Lists the local branches of the repository `at` is in, each with the object id its reference
/// names: the commit at its tip, and for the one the main working copy has checked out the
/// remote-tracking branch to compare it with ([`find_upstream`]) when the repository has one. A
/// broken branch — naming an object the repository does not have, or one that is not a commit —
/// is listed as it is rather than looked into, since asking `git` for what a missing object is
/// fails the whole listing; comparing it is refused (`blob::resolve_branch`).
pub fn branches(env: &GitEnv, at: &Located, cancel: &AtomicBool) -> Result<BranchList> {
    branches_in(env, &at.root, &at.repository.common_dir, cancel)
}

/// [`branches`] for the repository `dir` is in, where `dir` is any directory `git` can run in
/// there: a worktree, or the repository's common directory `common_dir` itself.
pub fn branches_in(
    env: &GitEnv,
    dir: &Path,
    common_dir: &Path,
    cancel: &AtomicBool,
) -> Result<BranchList> {
    // Taken before `git` reads the refs, so nothing that happens while it runs is claimed to be
    // in the list.
    let listed_at = Instant::now();
    // One past the budget, so a cut list is told from one that just fits. For a branch checked
    // out in a worktree (`%(worktreepath)`, `git` 2.23), `w` and its upstream's reference, empty
    // for none; `-` for any other.
    let count = format!("--count={}", budget::MAX_BRANCHES + 1);
    let args: &[&[u8]] = &[
        b"--format=%(objectname) %(refname) %(if)%(worktreepath)%(then)w %(upstream)%(else)-%(end)",
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
        upstreams_complete: true,
        listed_at,
    };
    // The branch the main working copy has checked out is named by the `HEAD` of the common
    // directory, and is listed as checked out (in a bare repository too, where `HEAD` names the
    // default branch). The one the UI compares with its upstream, so the only one looked up.
    // `HEAD` is asked for only when some branch is checked out, as none else can be it.
    let mut main = None;
    let mut upstream = None;
    if entries(&stdout).any(|(_, _, checked_out)| checked_out.is_some()) {
        match main_branch(env, common_dir, cancel)? {
            Head::Branch(name) => main = Some(name),
            Head::Detached => {}
            Head::Unknown => list.upstreams_complete = false,
        }
    }
    if let Some(main) = &main {
        let configured = entries(&stdout)
            .find(|&(_, raw, _)| raw == main.as_slice())
            .and_then(|(_, _, configured)| configured);
        if let Some(configured) = configured {
            let (found, complete) = find_upstream(env, dir, (main, configured), cancel)?;
            upstream = found;
            list.upstreams_complete = complete;
        }
    }
    let mut name_bytes = 0;
    for (oid, raw, _) in entries(&stdout) {
        let name = wire_path::encode(raw);
        let upstream = if main.as_deref() == Some(raw) {
            upstream.take()
        } else {
            None
        };
        // The upstream's name, sent beside the branch's, counts with it.
        let bytes = json_escaped_len(name.as_bytes())
            + upstream
                .as_ref()
                .map_or(0, |upstream| json_escaped_len(upstream.name.as_bytes()));
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
            upstream,
            origin: None,
        });
    }
    Ok(list)
}

/// The lines of a branch listing: a branch's commit, its name below `refs/heads/`, and, when it is
/// checked out in a worktree, the reference its upstream names (empty for none). A reference name
/// holds no space, newline or other control character, so one per line, its fields split at the
/// first spaces, is unambiguous.
fn entries(stdout: &[u8]) -> impl Iterator<Item = (&[u8], &[u8], Option<&[u8]>)> {
    stdout
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut fields = line.splitn(3, |&b| b == b' ');
            let (oid, refname, rest) = (fields.next()?, fields.next()?, fields.next()?);
            let raw = refname
                .strip_prefix(b"refs/heads/")
                .filter(|n| !n.is_empty())?;
            // `w <upstream>` for a branch checked out, `-` for any other.
            let checked_out = rest.first() == Some(&b'w');
            Some((
                oid,
                raw,
                checked_out.then(|| rest.get(2..).unwrap_or_default()),
            ))
        })
}

/// What `HEAD` of the main working copy is.
enum Head {
    /// A branch, below `refs/heads/` as raw bytes.
    Branch(Vec<u8>),
    /// Not a branch: detached, or a branch that is not below `refs/heads/`.
    Detached,
    /// `git` could not say.
    Unknown,
}

/// The branch the main working copy has checked out: what `git symbolic-ref` says of the `HEAD`
/// of the repository's common directory, which is the main working copy's own, whatever format
/// the references are kept in. It exits with status 1 for a `HEAD` that is no symbolic reference.
fn main_branch(env: &GitEnv, common_dir: &Path, cancel: &AtomicBool) -> Result<Head> {
    let args: &[&[u8]] = &[b"-q", b"HEAD"];
    match env.run(
        common_dir,
        "symbolic-ref",
        args,
        budget::GIT_METADATA_STDOUT,
        cancel,
    ) {
        Ok(stdout) => {
            let head = stdout.strip_suffix(b"\n").unwrap_or(&stdout);
            Ok(head
                .strip_prefix(b"refs/heads/")
                .filter(|name| !name.is_empty())
                .map_or(Head::Detached, |name| Head::Branch(name.to_vec())))
        }
        Err(GitError::Failed(failed)) if failed.code == Some(1) => Ok(Head::Detached),
        Err(GitError::Failed(_) | GitError::Stopped(BoundedFailure::StdoutExceeded(_))) => {
            Ok(Head::Unknown)
        }
        Err(err) => Err(git_error(err)),
    }
}

/// The remote-tracking branch to compare local branch `name` with, and whether the lookup was
/// complete. That is the upstream it is set to track (`configured`, the reference `git` resolved
/// it to, empty for none), else the branch of its name on a configured remote — `origin`'s, else
/// the first remote's in byte order. An upstream that is another local branch, or whose reference
/// is gone, is none. A reference is a remote's branch only as `refs/remotes/<remote>/<name>` for a
/// configured `<remote>`, so `refs/remotes/origin/backup/main` is not `main` on a remote
/// `origin/backup` that was never added.
///
/// Only the references asked for are looked up, by exact name: first the upstream and, when it is
/// none, `git remote` and the same name on each remote, so a repository with thousands of
/// remote-tracking references is no slower, and a branch tracking one costs one `git` run. A
/// lookup that passes a budget ([`budget::UPSTREAM_PATTERN_BYTES`] of names asked for, the output
/// of `git remote` or of a lookup) gives up with what it has found so far, the upstream if that
/// was found, and says it was not complete.
fn find_upstream(
    env: &GitEnv,
    dir: &Path,
    (name, configured): (&[u8], &[u8]),
    cancel: &AtomicBool,
) -> Result<(Option<UpstreamInfo>, bool)> {
    if configured.starts_with(b"refs/remotes/") {
        let Some(stdout) = lookup(env, dir, vec![configured.to_vec()], cancel)? else {
            return Ok((None, false));
        };
        if let Some(found) = upstream_of(&tips(&stdout), configured) {
            return Ok((Some(found), true));
        }
    }
    if name == b"HEAD" {
        return Ok((None, true));
    }
    let stdout = match env.run(dir, "remote", &[], budget::UPSTREAM_REMOTES_STDOUT, cancel) {
        Ok(stdout) => stdout,
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_))) => return Ok((None, false)),
        Err(err) => return Err(git_error(err)),
    };
    let remotes = || {
        stdout
            .split(|&b| b == b'\n')
            .filter(|remote| !remote.is_empty())
    };
    let refname = |remote: &[u8]| [b"refs/remotes/", remote, b"/", name].concat();
    let Some(lookup_stdout) = lookup(env, dir, remotes().map(refname).collect(), cancel)? else {
        return Ok((None, false));
    };
    let tips = tips(&lookup_stdout);
    // `origin`'s, else the first in byte order of the remotes that have the branch.
    let remote = remotes()
        .filter(|&remote| tips.contains_key(refname(remote).as_slice()))
        .min_by_key(|&remote| (remote != b"origin", remote));
    Ok((
        remote.and_then(|remote| upstream_of(&tips, &refname(remote))),
        true,
    ))
}

/// What a lookup printed, by reference: the object id each reference it was asked for names.
type Tips<'a> = HashMap<&'a [u8], &'a [u8]>;

/// The lines of a lookup's output, `<object id> <reference>`, indexed by reference.
fn tips(stdout: &[u8]) -> Tips<'_> {
    stdout
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            let space = line.iter().position(|&b| b == b' ')?;
            Some((&line[space + 1..], &line[..space]))
        })
        .collect()
}

/// The tip of remote-tracking reference `refname`, when `tips` (a lookup's answer) has it.
fn upstream_of(tips: &Tips, refname: &[u8]) -> Option<UpstreamInfo> {
    let commit = String::from_utf8_lossy(tips.get(refname)?).into_owned();
    let below = refname.strip_prefix(b"refs/remotes/")?;
    Some(UpstreamInfo {
        name: wire_path::encode(below),
        commit,
    })
}

/// Asks `git for-each-ref` for the references `names` by their exact names, and gives back what
/// it printed. `None` when the names are past [`budget::UPSTREAM_PATTERN_BYTES`],
/// or the output passes its budget.
fn lookup(
    env: &GitEnv,
    dir: &Path,
    names: Vec<Vec<u8>>,
    cancel: &AtomicBool,
) -> Result<Option<Vec<u8>>> {
    if names.is_empty() {
        return Ok(Some(Vec::new()));
    }
    if names.iter().map(|name| name.len() + 1).sum::<usize>() > budget::UPSTREAM_PATTERN_BYTES {
        return Ok(None);
    }
    let mut args: Vec<&[u8]> = vec![b"--format=%(objectname) %(refname)"];
    args.extend(names.iter().map(Vec::as_slice));
    match env.run(
        dir,
        "for-each-ref",
        &args,
        budget::UPSTREAM_LOOKUP_STDOUT,
        cancel,
    ) {
        Ok(stdout) => Ok(Some(stdout)),
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_))) => Ok(None),
        Err(err) => Err(git_error(err)),
    }
}

/// Two branches resolved to their commits, the commit they forked at, and the changes from that
/// commit to the right branch's that touch the project.
#[derive(Debug)]
pub struct Comparison {
    pub left: ComparisonEndpoint,
    pub right: ComparisonEndpoint,
    /// The merge base of the two commits; the two commits themselves when they are one, and the
    /// empty tree when the comparison is from nothing.
    pub base: String,
    pub changes: Vec<ChangeEntry>,
    /// False when the list was cut at a budget.
    pub complete: bool,
}

/// What a comparison starts from: a local branch, a remote-tracking branch (names as wire paths,
/// below `refs/heads/` and `refs/remotes/`), or nothing, the empty tree, which makes every file of
/// the right branch's tip an addition.
#[derive(Debug, Clone, Copy)]
pub enum Base<'a> {
    Local(&'a str),
    Remote(&'a str),
    Empty,
}

/// Resolves the `left` base and branch `right` (a wire path, a name below `refs/heads/`) to the
/// commits at their tips and lists the changes from their merge base to the second's tip that
/// touch the project's scope, in the order `git` gives them. Two branches at the same commit have
/// none, and `git` is not asked; two with no common commit are refused (`no_merge_base`, or
/// `no_merge_base_shallow` in a shallow clone).
///
/// The whole tree is compared, so a rename across the scope's boundary is found. When that output
/// is past its budget — far-apart branches of a large repository — a project below the
/// repository's root is compared within its own directory instead: a rename across its boundary
/// then reads as the addition or the deletion it is inside, as an unpaired one always does.
pub fn list(
    env: &GitEnv,
    at: &Located,
    left: Base,
    right: &str,
    cancel: &AtomicBool,
) -> Result<Comparison> {
    let left = match left {
        Base::Local(branch) => ComparisonEndpoint {
            commit: blob::resolve_branch(env, &at.root, branch, cancel)?,
            branch: branch.to_string(),
            remote: false,
            empty: false,
        },
        Base::Remote(branch) => ComparisonEndpoint {
            commit: blob::resolve_remote_branch(env, &at.root, branch, cancel)?,
            branch: branch.to_string(),
            remote: true,
            empty: false,
        },
        Base::Empty => ComparisonEndpoint {
            commit: changes::empty_tree(env, at, cancel)?,
            branch: String::new(),
            remote: false,
            empty: true,
        },
    };
    let right = ComparisonEndpoint {
        commit: blob::resolve_branch(env, &at.root, right, cancel)?,
        branch: right.to_string(),
        remote: false,
        empty: false,
    };
    // The empty tree is the old side itself: no commit has it as an ancestor.
    let base = if left.empty {
        left.commit.clone()
    } else {
        merge_base(env, at, &left, &right, cancel)?
    };
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
    // Released before the counts are read, which hold buffers of their own.
    drop(stdout);
    let wanted = line_counts::wanted(comparison.changes.iter());
    if !wanted.is_empty() {
        let args: &[&[u8]] = &[
            b"-r",
            b"-M",
            comparison.base.as_bytes(),
            comparison.right.commit.as_bytes(),
        ];
        let counts = line_counts::read_diff(env, at, "diff-tree", args, &wanted, cancel)?;
        line_counts::apply(comparison.changes.iter_mut(), &counts);
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

/// Checks each of `left` and `right`: a branch name that is one (a remote-tracking one when the
/// endpoint says so), and a commit that is a commit of this repository; and `base` as such a
/// commit. Whether `base` is the two commits' merge base is not asked again: a read is held to
/// commits of the repository, as a read of a commit is.
fn verify_endpoints(
    env: &GitEnv,
    at: &Located,
    (left, right, base): Compared,
    cancel: &AtomicBool,
) -> Result<()> {
    // Only the left side may stand for something other than a local branch; a client's `right`
    // that says it does would skip the checks below.
    if right.empty || right.remote {
        return Err(CodedError::raised(
            error_code::INVALID_BRANCH_NAME,
            format!("{} is not a local branch name", right.branch),
            &[("branch", &right.branch)],
        ));
    }
    for endpoint in [left, right] {
        if endpoint.empty {
            // The empty tree has no branch and is no commit; `base` is checked to be it below.
            continue;
        }
        if endpoint.remote {
            blob::remote_branch_refname(env, &at.root, &endpoint.branch, cancel)?;
        } else {
            blob::branch_refname(env, &at.root, &endpoint.branch, cancel)?;
        }
        blob::verify_commit(env, &at.root, &endpoint.commit, cancel)?;
    }
    if left.empty {
        if !changes::is_empty_tree(base) {
            return Err(CodedError::raised(
                error_code::INVALID_COMMIT,
                format!("{base} is not the empty tree"),
                &[("commit", base)],
            ));
        }
        return Ok(());
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
