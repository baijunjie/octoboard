//! Reading a file's content out of Git rather than off the disk: the blob a worktree's index
//! stages for a path, or the blob a commit holds for it. Each read reports the object id it read,
//! which never changes meaning, so the reply identifies its content exactly.
//!
//! A path is looked up by listing exactly that path (`ls-files`, `ls-tree`), never by a revision
//! expression such as `:<path>` or `<commit>:<path>`, whose syntax a filename could take part in.
//! A branch name is checked as a reference name and looked up as exactly `refs/heads/<name>`
//! before anything is read; a commit is accepted only as the full object id of a commit.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::git::{GitEnv, GitError};
use crate::browse::live::{too_large, unsupported};
use crate::browse::wire_path::{self, RelPath};
use crate::browse::{budget, git_error, git_failed};
use crate::protocol::{error_code, CodedError};
use crate::subprocess::BoundedFailure;

/// A blob's object id and its bytes.
#[derive(Debug)]
pub struct Blob {
    pub oid: String,
    pub bytes: Vec<u8>,
}

/// A file to look up in Git: `path` as the client named it, relative to the project's directory,
/// which is `scope` inside the repository. Lookups use the two joined; what a client is told
/// about uses `path` alone, the same as a live read.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub scope: &'a RelPath,
    pub path: &'a RelPath,
}

impl Entry<'_> {
    fn in_repository(&self) -> RelPath {
        self.scope.join(self.path)
    }
}

/// The blob staged for `entry` in the index of the worktree at `root`. A path with conflicting
/// stages is refused rather than one stage picked.
pub fn read_index(env: &GitEnv, root: &Path, entry: Entry, cancel: &AtomicBool) -> Result<Blob> {
    let full = entry.in_repository();
    let stdout = match env.run(
        root,
        "ls-files",
        &[b"--stage", b"-z", b"--", full.as_bytes()],
        budget::GIT_METADATA_STDOUT,
        cancel,
    ) {
        Ok(stdout) => stdout,
        // Only a directory, listing everything below it, prints this much for one literal path.
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_))) => {
            return Err(unsupported(entry.path, "directory"))
        }
        Err(err) => return Err(git_error(err)),
    };
    let mut exact = Vec::new();
    let mut below = false;
    for record in stdout.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        // `<mode> <oid> <stage>\t<path>`
        let Some(tab) = record.iter().position(|&b| b == b'\t') else {
            continue;
        };
        let (meta, entry_path) = (&record[..tab], &record[tab + 1..]);
        if entry_path != full.as_bytes() {
            below = true;
            continue;
        }
        if let [mode, oid, stage] = meta.split(|&b| b == b' ').collect::<Vec<_>>()[..] {
            let oid = String::from_utf8_lossy(oid).into_owned();
            exact.push((mode.to_vec(), oid, stage.to_vec()));
        }
    }
    let (mode, oid) = match exact.as_slice() {
        [(mode, oid, stage)] if stage == b"0" => (mode.clone(), oid.clone()),
        [] if below => return Err(unsupported(entry.path, "directory")),
        [] => return Err(not_found(entry.path)),
        _ => return Err(unsupported(entry.path, "unmerged")),
    };
    check_mode(entry.path, &mode)?;
    let stdout = env
        .run(
            root,
            "cat-file",
            &[b"-s", oid.as_bytes()],
            budget::GIT_METADATA_STDOUT,
            cancel,
        )
        .map_err(git_error)?;
    let size = parse_size(&stdout)?;
    read_object(env, root, oid, size, cancel)
}

/// The blob commit `commit` holds for `entry`, read in the worktree at `root`.
pub fn read_commit(
    env: &GitEnv,
    root: &Path,
    commit: &str,
    entry: Entry,
    cancel: &AtomicBool,
) -> Result<Blob> {
    let full = entry.in_repository();
    let stdout = env
        .run(
            root,
            "ls-tree",
            &[
                b"-z",
                b"-l",
                b"--full-tree",
                commit.as_bytes(),
                b"--",
                full.as_bytes(),
            ],
            budget::GIT_METADATA_STDOUT,
            cancel,
        )
        .map_err(git_error)?;
    for record in stdout.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        // `<mode> <type> <oid> <size>\t<path>`, the size padded with spaces.
        let Some(tab) = record.iter().position(|&b| b == b'\t') else {
            continue;
        };
        if &record[tab + 1..] != full.as_bytes() {
            continue;
        }
        let fields: Vec<&[u8]> = record[..tab]
            .split(|&b| b == b' ')
            .filter(|field| !field.is_empty())
            .collect();
        let [mode, kind, oid, size] = fields[..] else {
            continue;
        };
        match kind {
            b"tree" => return Err(unsupported(entry.path, "directory")),
            b"commit" => return Err(unsupported(entry.path, "submodule")),
            _ => {}
        }
        check_mode(entry.path, mode)?;
        let size = parse_size(size)?;
        let oid = String::from_utf8_lossy(oid).into_owned();
        return read_object(env, root, oid, size, cancel);
    }
    Err(not_found(entry.path))
}

/// Refuses a link or a submodule entry: their "content" is not a file body.
fn check_mode(path: &RelPath, mode: &[u8]) -> Result<()> {
    match mode {
        b"120000" => Err(unsupported(path, "symlink")),
        b"160000" => Err(unsupported(path, "submodule")),
        _ => Ok(()),
    }
}

/// A size `git` printed, which a `git` that printed anything else has failed to give.
fn parse_size(text: &[u8]) -> Result<u64> {
    std::str::from_utf8(text)
        .ok()
        .and_then(|size| size.trim().parse().ok())
        .ok_or_else(|| {
            let shown = String::from_utf8_lossy(text);
            git_failed(&format!(
                "git printed `{shown}` where an object size belongs"
            ))
        })
}

/// Reads blob `oid`, whose size is `size`, refusing it before reading when it is over the file
/// budget and, should the size have been wrong, while reading.
fn read_object(
    env: &GitEnv,
    root: &Path,
    oid: String,
    size: u64,
    cancel: &AtomicBool,
) -> Result<Blob> {
    if size > budget::MAX_FILE_BYTES as u64 {
        return Err(too_large(budget::MAX_FILE_BYTES, Some(size)));
    }
    let bytes = match env.run(
        root,
        "cat-file",
        &[b"blob", oid.as_bytes()],
        budget::MAX_FILE_BYTES,
        cancel,
    ) {
        Ok(bytes) => bytes,
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_))) => {
            return Err(too_large(budget::MAX_FILE_BYTES, None))
        }
        Err(err) => return Err(git_error(err)),
    };
    Ok(Blob { oid, bytes })
}

/// The commit local branch `branch` (a wire path: the name below `refs/heads/`) points at.
///
/// The name is checked as a reference name before it reaches `git`, so `@{-1}`, `HEAD~2`, `main^`
/// or an option-like `-x` is refused rather than evaluated, and then looked up as exactly
/// `refs/heads/<name>`: a revision lookup would fall back to other references
/// (`refs/tags/refs/heads/<name>`, …) when the branch does not exist.
pub fn resolve_branch(
    env: &GitEnv,
    root: &Path,
    branch: &str,
    cancel: &AtomicBool,
) -> Result<String> {
    let invalid = || {
        CodedError::raised(
            error_code::INVALID_BRANCH_NAME,
            format!("{branch} is not a branch name"),
            &[("branch", branch)],
        )
    };
    let name = wire_path::decode(branch)
        .filter(|name| wire_path::encode(name) == branch)
        .ok_or_else(invalid)?;
    if name.is_empty() || name.starts_with(b"-") || name.contains(&0) {
        return Err(invalid());
    }
    let mut refname = b"refs/heads/".to_vec();
    refname.extend_from_slice(&name);
    let limit = budget::GIT_METADATA_STDOUT;
    match env.run(root, "check-ref-format", &[&refname], limit, cancel) {
        Ok(_) => {}
        Err(GitError::Failed(_)) => return Err(invalid()),
        Err(err) => return Err(git_error(err)),
    }
    let unknown = || {
        CodedError::raised(
            error_code::UNKNOWN_BRANCH,
            format!("there is no local branch {branch}"),
            &[("branch", branch)],
        )
    };
    let args: &[&[u8]] = &[b"--verify", b"--hash", &refname];
    let target = match env.run(root, "show-ref", args, limit, cancel) {
        Ok(stdout) => String::from_utf8_lossy(&stdout).trim().to_string(),
        Err(GitError::Failed(_)) => return Err(unknown()),
        Err(err) => return Err(git_error(err)),
    };
    match peel_to_commit(env, root, &target, cancel)? {
        Some(commit) if commit == target => Ok(commit),
        _ => Err(unknown()),
    }
}

/// Checks that `oid` is the full id of a commit in the repository — exactly, so a 40-digit prefix
/// in a SHA-256 repository, or the id of a tag pointing at a commit, is refused.
pub fn verify_commit(env: &GitEnv, root: &Path, oid: &str, cancel: &AtomicBool) -> Result<()> {
    let hex = oid.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if !(hex && (oid.len() == 40 || oid.len() == 64)) {
        return Err(CodedError::raised(
            error_code::INVALID_COMMIT,
            format!("{oid} is not a full commit id"),
            &[("commit", oid)],
        ));
    }
    match peel_to_commit(env, root, oid, cancel)? {
        Some(commit) if commit == oid => Ok(()),
        Some(_) => Err(CodedError::raised(
            error_code::INVALID_COMMIT,
            format!("{oid} is not a full commit id"),
            &[("commit", oid)],
        )),
        None => Err(CodedError::raised(
            error_code::UNKNOWN_COMMIT,
            format!("there is no commit {oid}"),
            &[("commit", oid)],
        )),
    }
}

/// The full id of the commit object `oid` (an object id already checked to be hexadecimal) names
/// or peels to, or `None` when it names no object, or one that is not a commit.
fn peel_to_commit(
    env: &GitEnv,
    root: &Path,
    oid: &str,
    cancel: &AtomicBool,
) -> Result<Option<String>> {
    let mut expression = oid.as_bytes().to_vec();
    expression.extend_from_slice(b"^{commit}");
    let args: &[&[u8]] = &[b"--verify", b"--quiet", b"--end-of-options", &expression];
    match env.run(root, "rev-parse", args, budget::GIT_METADATA_STDOUT, cancel) {
        Ok(stdout) => Ok(Some(String::from_utf8_lossy(&stdout).trim().to_string())),
        Err(GitError::Failed(_)) => Ok(None),
        Err(err) => Err(git_error(err)),
    }
}

fn not_found(path: &RelPath) -> anyhow::Error {
    let shown = path.to_wire();
    CodedError::raised(
        error_code::FILE_NOT_FOUND,
        format!("{shown} does not exist in this source"),
        &[("path", &shown)],
    )
}
