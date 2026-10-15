//! What the attribution reads out of a repository's administrative files: which repository a
//! directory or a git directory is in, whether its references are in reftable format, and a
//! branch's reflog.
//!
//! All of it is read from the filesystem rather than asked of `git`: a report and a listing are
//! handled while other work waits on the repository's gate, and a file read is cheaper than a
//! process.

use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::browse::{source, wire_path};

/// How much of a reflog's first line is read. A line is two object ids, an identity, a time and
/// a message.
const MAX_REFLOG_LINE: u64 = 64 * 1024;

/// How much of a reflog is searched for a message. A branch's reflog grows by a line per commit,
/// and what is looked for is written at its creation or rename, so a reflog past this is searched
/// only as far as this.
const MAX_REFLOG_SEARCHED: u64 = 4 * 1024 * 1024;

/// The canonical common directory of the repository holding `dir`. `None` for a directory in no
/// repository.
pub fn repository_of(dir: &Path) -> Option<PathBuf> {
    let ceiling = source::home_ceiling();
    let canonical = fs::canonicalize(dir).ok()?;
    let top = source::find_dot_git(&canonical, ceiling.as_deref())?;
    let admin = source::admin_dir(&top)?;
    common_dir_of(&admin)
}

/// The canonical common directory of the git directory `git_dir`: the one a linked worktree's
/// `commondir` names, or the directory itself.
pub(super) fn common_dir_of(git_dir: &Path) -> Option<PathBuf> {
    match fs::read(git_dir.join("commondir")) {
        Ok(pointer) => {
            let pointer = pointer.strip_suffix(b"\n").unwrap_or(&pointer);
            fs::canonicalize(git_dir.join(OsStr::from_bytes(pointer))).ok()
        }
        Err(_) => fs::canonicalize(git_dir).ok(),
    }
}

/// Whether the repository keeps its references in reftable format, which has no reflog files.
pub(super) fn uses_reftable(common_dir: &Path) -> bool {
    common_dir.join("reftable").is_dir()
}

/// The first entry of a branch's reflog.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReflogEntry {
    /// Whether the entry records no value before it, as a branch's creation does.
    pub(super) from_nothing: bool,
    /// The commit the branch named after the entry.
    pub(super) commit: String,
    /// Seconds since the epoch.
    pub(super) time: i64,
}

fn reflog_path(common_dir: &Path, branch: &str) -> Option<PathBuf> {
    let name = wire_path::decode(branch)?;
    Some(
        common_dir
            .join("logs/refs/heads")
            .join(OsStr::from_bytes(&name)),
    )
}

/// The first line of `logs/refs/heads/<branch>`, if the branch has a reflog.
pub(super) fn first_reflog_entry(common_dir: &Path, branch: &str) -> Option<ReflogEntry> {
    let file = fs::File::open(reflog_path(common_dir, branch)?).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(MAX_REFLOG_LINE))
        .read_until(b'\n', &mut line)
        .ok()?;
    parse_reflog_line(&line).map(|(entry, _)| entry)
}

/// Whether any entry of the branch's reflog has a message `matches` accepts.
pub(super) fn reflog_has_message(
    common_dir: &Path,
    branch: &str,
    matches: impl Fn(&[u8]) -> bool,
) -> bool {
    let Some(file) = reflog_path(common_dir, branch).and_then(|path| fs::File::open(path).ok())
    else {
        return false;
    };
    BufReader::new(file.take(MAX_REFLOG_SEARCHED))
        .split(b'\n')
        .map_while(Result::ok)
        .filter_map(|line| parse_reflog_line(&line).map(|(_, message)| message.to_vec()))
        .any(|message| matches(&message))
}

/// `<old> <new> <identity> <seconds> <zone>\t<message>`. The identity holds spaces of its own, so
/// the time is found from the right.
fn parse_reflog_line(line: &[u8]) -> Option<(ReflogEntry, &[u8])> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let (head, message) = match line.iter().position(|&b| b == b'\t') {
        Some(at) => (&line[..at], &line[at + 1..]),
        None => (line, &[][..]),
    };
    let mut fields = head.splitn(3, |&b| b == b' ');
    let old = fields.next()?;
    let from_nothing = !old.is_empty() && old.iter().all(|&b| b == b'0');
    let commit = std::str::from_utf8(fields.next()?).ok()?.to_string();
    let mut tail = fields.next()?.rsplitn(3, |&b| b == b' ');
    tail.next()?;
    let time = std::str::from_utf8(tail.next()?).ok()?.parse().ok()?;
    Some((
        ReflogEntry {
            from_nothing,
            commit,
            time,
        },
        message,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git, repo_with};

    #[test]
    fn a_reflog_line_is_read_from_the_right() {
        let line = b"0000000000000000000000000000000000000000 1234567890abcdef1234567890abcdef12345678 A B <a@b.c> 1700000000 +0900\tbranch: Created from HEAD\n";
        let (entry, message) = parse_reflog_line(line).unwrap();
        assert!(entry.from_nothing);
        assert_eq!(entry.commit, "1234567890abcdef1234567890abcdef12345678");
        assert_eq!(entry.time, 1_700_000_000);
        assert_eq!(message, b"branch: Created from HEAD");
    }

    #[test]
    fn a_linked_worktree_resolves_to_the_common_directory() {
        let repo = repo_with("branch-origin-linked", &[(b"a", b"a")]);
        let linked = repo.join("linked");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt",
                linked.to_str().unwrap(),
            ],
        );
        assert_eq!(repository_of(&linked), repository_of(&repo));
        assert!(repository_of(&repo).is_some());
    }
}
