//! A repository's worktrees: the parsed list `git worktree list --porcelain -z` gives, with each
//! one's root and branch ([`worktrees`]), and whether one holds changes a removal would discard
//! ([`changes`]). The reads run through the caller's [`Runner`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::git::run::{GitError, Limits, Runner};
use crate::platform::names;

/// What each `git` that reads or removes worktrees is held to: a deadline of two minutes, since a
/// forced removal deletes a whole worktree and a status walks a large one, and no output ceiling,
/// the output being small by nature.
pub(crate) const WORKTREE_GIT_LIMITS: Limits = Limits::deadline(Duration::from_secs(120));

/// One worktree of the repository, as `git worktree list` gives it.
pub(crate) struct Listed {
    pub(crate) root: PathBuf,
    pub(crate) branch: Option<Vec<u8>>,
    pub(crate) main: bool,
    pub(crate) locked: bool,
    /// Its directory is gone, and only git's record of it is left.
    pub(crate) prunable: bool,
}

/// The repository's worktrees, the main one first.
pub(crate) fn worktrees(git: &Runner, repo: &Path) -> Result<Vec<Listed>, GitError> {
    let stdout = git.read(repo, ["worktree", "list", "--porcelain", "-z"])?;
    Ok(parse_worktree_list(&stdout)
        .iter()
        .enumerate()
        .filter(|(_, record)| !record.bare)
        .filter_map(|(at, record)| {
            let path = names::path_from_name_bytes(&record.path)?;
            Some(Listed {
                root: crate::hostfs::canonical(&path).unwrap_or(path),
                branch: record.branch.clone(),
                main: at == 0,
                locked: record.locked,
                prunable: record.prunable,
            })
        })
        .collect())
}

/// Whether the worktree at `root` holds anything `git worktree remove` refuses to discard:
/// modified tracked files or untracked ones, the same `git status` it asks itself.
pub(crate) fn changes(git: &Runner, root: &Path) -> Result<bool, GitError> {
    let stdout = git.read(
        root,
        [
            "status",
            "--porcelain",
            "-z",
            "--ignore-submodules=none",
            "--untracked-files=normal",
        ],
    )?;
    Ok(!stdout.is_empty())
}

/// One record of `git worktree list --porcelain -z`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct WorktreeRecord {
    pub(crate) path: Vec<u8>,
    pub(crate) head: Option<String>,
    /// The checked-out branch, below `refs/heads/`, as raw bytes.
    pub(crate) branch: Option<Vec<u8>>,
    pub(crate) bare: bool,
    pub(crate) locked: bool,
    pub(crate) prunable: bool,
}

/// Parses `git worktree list --porcelain -z`: NUL-terminated attribute lines, each record ended
/// by an empty one. Paths are taken as bytes, so a worktree path holding a newline is read whole.
pub(crate) fn parse_worktree_list(stdout: &[u8]) -> Vec<WorktreeRecord> {
    let mut records = Vec::new();
    let mut current: Option<WorktreeRecord> = None;
    for field in stdout.split(|&b| b == 0) {
        if field.is_empty() {
            records.extend(current.take());
            continue;
        }
        let (key, value) = match field.iter().position(|&b| b == b' ') {
            Some(at) => (&field[..at], Some(&field[at + 1..])),
            None => (field, None),
        };
        if key == b"worktree" {
            records.extend(current.take());
            current = Some(WorktreeRecord {
                path: value.unwrap_or_default().to_vec(),
                ..WorktreeRecord::default()
            });
            continue;
        }
        let Some(record) = current.as_mut() else {
            continue;
        };
        match key {
            b"HEAD" => {
                record.head = value
                    .map(|oid| String::from_utf8_lossy(oid).into_owned())
                    .filter(|oid| oid.bytes().any(|b| b != b'0'));
            }
            b"branch" => {
                record.branch = value
                    .and_then(|name| name.strip_prefix(b"refs/heads/"))
                    .map(<[u8]>::to_vec);
            }
            b"bare" => record.bare = true,
            b"locked" => record.locked = true,
            b"prunable" => record.prunable = true,
            _ => {}
        }
    }
    records.extend(current);
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_records_are_read_as_bytes() {
        let stdout = [
            &b"worktree /r/main\0HEAD 1111111111111111111111111111111111111111\0"[..],
            b"branch refs/heads/main\0\0",
            b"worktree /r/new\nline\0HEAD 2222222222222222222222222222222222222222\0detached\0\0",
            b"worktree /r/gone\0HEAD 3333333333333333333333333333333333333333\0",
            b"branch refs/heads/caf\xe9\0prunable gitdir file points to non-existent location\0\0",
        ]
        .concat();
        let records = parse_worktree_list(&stdout);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].branch.as_deref(), Some(&b"main"[..]));
        assert_eq!(records[1].path, b"/r/new\nline");
        assert_eq!(records[1].branch, None);
        assert_eq!(records[2].branch.as_deref(), Some(&b"caf\xe9"[..]));
        assert!(records[2].prunable);
    }
}
