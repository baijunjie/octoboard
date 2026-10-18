//! The commits a branch has made since it forked, one by one: which commits there are between a
//! comparison's merge base and its right commit, what each of them changed in the project against
//! its first parent, and the diff of one such change.
//!
//! Like a comparison, a commit list is named by commits rather than branches, so a branch that
//! moves while the list is shown never repoints it, and every read of one of its changes names the
//! commit and the parent it is read between: both are immutable, so nothing read from them is
//! checked again. A commit with no parent — a branch's first commit, or one met where it merged
//! in an unrelated history — is compared with the empty tree, as every change is before a
//! repository's first commit.
//!
//! Each commit's changes are found as a comparison's are: the whole trees are compared, with
//! rename detection on, so a rename across the project's boundary is found against the rest of the
//! repository, and only what touches the project is kept (`tree_diff::committed_record`).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::changes::{self, ChangeRead};
use crate::browse::git::{GitEnv, GitError};
use crate::browse::line_counts;
use crate::browse::tree_diff;
use crate::browse::{blob, budget, git_error, git_failed, Located};
use crate::protocol::{CommitChanges, ComparedChangeRef};
use crate::subprocess::BoundedFailure;

/// The commits between a merge base and a tip, newest first, each with its changes.
#[derive(Debug)]
pub struct CommitList {
    pub commits: Vec<CommitChanges>,
    /// False when the changes were cut at a budget: the last commit listed may hold only part of
    /// its changes, and the commits after it are left out, as is the one the budget ran out in if
    /// none of its changes fit.
    pub complete: bool,
    /// False when there were more commits than [`budget::MAX_COMMITS`]: the newest are listed.
    pub commits_complete: bool,
}

/// The `git log` options that choose which commits a listing holds and against what each is
/// compared, shared by the listing and by the run that counts its lines, so that the two cannot
/// drift apart. Each one fixes something the user's configuration could otherwise change:
///
/// - `-M` turns rename detection on whatever `diff.renames` says, as a comparison does;
/// - `--diff-merges=first-parent` gives a merge commit its changes against its first parent rather
///   than none (`log.diffMerges` aside), and `--root` gives a commit with no parent its changes
///   against the empty tree, whatever `log.showRoot` says;
/// - `--no-show-signature` keeps `log.showSignature` from writing a signature check into the
///   output, and `--no-follow` keeps `log.follow` from following one path when the listing is made
///   within a subdirectory project's own directory.
const LOG_SELECTION: &[&[u8]] = &[
    b"-M",
    b"--diff-merges=first-parent",
    b"--root",
    b"--no-show-signature",
    b"--no-follow",
];

/// `-z --raw --no-abbrev` writes each commit's changes as the records
/// `tree_diff::committed_record` parses, with full object ids and paths as bytes.
const LOG_RECORDS: &[&[u8]] = &[b"-z", b"--raw", b"--no-abbrev"];

/// The listing's output format, around [`LOG_SELECTION`] and before its revision range:
///
/// - `--encoding=UTF-8` re-encodes a subject written in another encoding, so it reads as text;
/// - the format is the commit, its parents and then its subject, one NUL-terminated field each.
const LOG_FORMAT: &[&[u8]] = &[b"--encoding=UTF-8", b"--format=%H %P%x00%s"];

/// The `git log` options the line counts of a listing are read with: [`LOG_SELECTION`] and a
/// format that is only the commit's id.
fn count_options() -> Vec<&'static [u8]> {
    let mut options = LOG_SELECTION.to_vec();
    options.push(b"--format=%H%x00");
    options
}

/// Lists the commits reachable from `tip` and not from `base`, both full ids of commits in the
/// repository (`base` may be the empty tree's id instead, as a comparison from nothing reports
/// it: every commit of `tip`'s history is then listed), newest first as `git log` orders them,
/// each with its changes against its first parent that touch the project's scope. At most
/// [`budget::MAX_COMMITS`] are listed, and their changes together within one change listing's
/// budgets. A `tip` equal to `base` has none, and `git log` is not asked.
///
/// As a comparison is, the listing is made over the whole tree, and within a subdirectory
/// project's own directory when that output is past its budget; `git log` then lists only the
/// commits that touch the directory (`--full-history`, so none is left out for being the same as
/// one of its parents there).
pub fn list(
    env: &GitEnv,
    at: &Located,
    (base, tip): (&str, &str),
    cancel: &AtomicBool,
) -> Result<CommitList> {
    let from_nothing = changes::is_empty_tree(base);
    if !from_nothing {
        blob::verify_commit(env, &at.root, base, cancel)?;
    }
    blob::verify_commit(env, &at.root, tip, cancel)?;
    let mut list = CommitList {
        commits: Vec::new(),
        complete: true,
        commits_complete: true,
    };
    if base == tip {
        return Ok(list);
    }
    // One past the budget, so a cut list is told from one that just fits.
    let count = format!("--max-count={}", budget::MAX_COMMITS + 1);
    let range = if from_nothing {
        tip.to_string()
    } else {
        format!("{base}..{tip}")
    };
    let run = |within_scope: bool| {
        let mut args: Vec<&[u8]> = [LOG_RECORDS, LOG_SELECTION, LOG_FORMAT].concat();
        args.push(count.as_bytes());
        if within_scope {
            args.push(b"--full-history");
        }
        args.extend_from_slice(&[b"--end-of-options", range.as_bytes()]);
        if within_scope {
            // A literal pathspec (`GitEnv::run` sets `GIT_LITERAL_PATHSPECS`): the scope's
            // directory and everything below it.
            args.extend_from_slice(&[b"--", at.repository.scope.as_bytes()]);
        }
        env.run(
            &at.root,
            "log",
            &args,
            tree_diff::tree_diff_stdout(),
            cancel,
        )
    };
    let stdout = match run(false) {
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_)))
            if !at.repository.scope.is_root() =>
        {
            run(true)
        }
        other => other,
    }
    .map_err(git_error)?;

    let mut entries = 0;
    let mut path_bytes = 0;
    // Every field ends in a NUL, the subject's too, so an empty subject is an empty field; the
    // first record of a commit's changes starts with the newline `git` puts between the commit's
    // format and its diff.
    let mut fields = stdout
        .split(|&b| b == 0)
        .map(|field| field.strip_prefix(b"\n").unwrap_or(field))
        .peekable();
    while let Some(header) = fields.next() {
        // What follows the last field's NUL.
        if header.is_empty() {
            continue;
        }
        if list.commits.len() == budget::MAX_COMMITS {
            list.commits_complete = false;
            break;
        }
        let (commit, parent) = commit_header(header)?;
        let subject = subject_line(fields.next().unwrap_or_default());
        let mut commit = CommitChanges {
            commit,
            parent,
            subject,
            changes: Vec::new(),
        };
        // A commit with no parent has nothing on its old side, so no old side names a commit.
        let old = commit.parent.clone().unwrap_or_default();
        while let Some(meta) = fields.next_if(|field| field.starts_with(b":")) {
            let Some(entry) =
                tree_diff::committed_record("log", meta, &mut fields, at, (&old, &commit.commit))?
            else {
                continue;
            };
            let bytes = changes::entry_path_bytes(&entry);
            if entries == budget::MAX_CHANGE_ENTRIES
                || path_bytes + bytes > budget::MAX_CHANGE_PATH_BYTES
            {
                list.complete = false;
                break;
            }
            entries += 1;
            path_bytes += bytes;
            commit.changes.push(entry);
        }
        // A commit whose changes ran out of budget before any was listed would read as one that
        // changed nothing in the project, so it is left out like the commits after it.
        if !list.complete && commit.changes.is_empty() {
            break;
        }
        list.commits.push(commit);
        if !list.complete {
            break;
        }
    }
    // Released before the counts are read, which hold buffers of their own.
    drop(stdout);
    add_line_counts(env, at, (&range, &count), &mut list, cancel)?;
    Ok(list)
}

/// Sets the line counts of the listed changes, from one more `git log` over `range`, cut at
/// `count` commits as the listing was: the commits are told apart by their ids, so a listing that
/// holds only some of them (cut at a budget) takes what it needs. Within a subdirectory project's
/// own directory the log is limited to it, as `line_counts::read_log` limits every run.
fn add_line_counts(
    env: &GitEnv,
    at: &Located,
    (range, count): (&str, &str),
    list: &mut CommitList,
    cancel: &AtomicBool,
) -> Result<()> {
    let wanted: HashMap<Vec<u8>, _> = list
        .commits
        .iter()
        .map(|commit| {
            (
                commit.commit.as_bytes().to_vec(),
                line_counts::wanted(commit.changes.iter()),
            )
        })
        .filter(|(_, paths)| !paths.is_empty())
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let mut args = count_options();
    args.push(count.as_bytes());
    if !at.repository.scope.is_root() {
        args.push(b"--full-history");
    }
    args.extend_from_slice(&[b"--end-of-options", range.as_bytes()]);
    let groups = line_counts::read_log(env, at, &args, &wanted, cancel)?;
    for commit in &mut list.commits {
        let counts = groups
            .iter()
            .find(|(id, _)| id.as_slice() == commit.commit.as_bytes());
        if let Some((_, counts)) = counts {
            line_counts::apply(commit.changes.iter_mut(), counts);
        }
    }
    Ok(())
}

/// A commit's header field, `<commit> <parent>…`: the commit and its first parent, if any. A
/// commit with no parent ends its header with the space before the parents it does not have.
fn commit_header(field: &[u8]) -> Result<(String, Option<String>)> {
    let shown = || String::from_utf8_lossy(field).into_owned();
    let ids: Vec<&[u8]> = field
        .strip_suffix(b" ")
        .unwrap_or(field)
        .split(|&b| b == b' ')
        .collect();
    let is_id = |id: &&[u8]| {
        matches!(id.len(), 40 | 64) && id.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    };
    if !ids.iter().all(is_id) {
        return Err(git_failed(&format!(
            "git log printed `{}`, which is not a commit header it writes",
            shown()
        )));
    }
    let id = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    Ok((id(ids[0]), ids.get(1).map(|parent| id(parent))))
}

/// A commit's subject as it is listed: read as UTF-8 with replacement characters, and cut to
/// [`budget::MAX_COMMIT_SUBJECT_CHARS`].
fn subject_line(field: &[u8]) -> String {
    String::from_utf8_lossy(field)
        .chars()
        .take(budget::MAX_COMMIT_SUBJECT_CHARS)
        .collect()
}

/// The commit a change of `commit` is read against: `parent`, checked as a commit of the
/// repository as `commit` is, or the empty tree for a commit with none. Whether `parent` is the
/// commit's first parent is not asked: a read is held to commits of the repository, as a read of a
/// commit is.
fn verify(
    env: &GitEnv,
    at: &Located,
    (commit, parent): (&str, Option<&str>),
    cancel: &AtomicBool,
) -> Result<String> {
    blob::verify_commit(env, &at.root, commit, cancel)?;
    match parent {
        Some(parent) => {
            blob::verify_commit(env, &at.root, parent, cancel)?;
            Ok(parent.to_string())
        }
        None => changes::empty_tree(env, at, cancel),
    }
}

/// Reads one change of `commit`, its old side from `parent` (the empty tree for none) and its new
/// side from the commit, both checked first.
pub fn read(
    env: &GitEnv,
    at: &Located,
    (commit, parent): (&str, Option<&str>),
    change: &ComparedChangeRef,
    cancel: &AtomicBool,
) -> Result<ChangeRead> {
    let old = verify(env, at, (commit, parent), cancel)?;
    changes::read_between(env, at, (&old, commit), (&change.old, &change.new), cancel)
}

/// Reads the whole bodies of one change of `commit`, checked first as [`read`] checks them, and
/// read only when both sides are the versions `expected` names.
pub fn read_bodies(
    env: &GitEnv,
    at: &Located,
    (commit, parent): (&str, Option<&str>),
    change: &ComparedChangeRef,
    expected: &changes::Expected,
    cancel: &AtomicBool,
) -> Result<changes::BodiesRead> {
    let old = verify(env, at, (commit, parent), cancel)?;
    changes::read_bodies_between(
        env,
        at,
        (&old, commit),
        (&change.old, &change.new),
        expected,
        cancel,
    )
}
