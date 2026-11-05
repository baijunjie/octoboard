//! The reflog files of a repository: how a branch's creation is told from them, for the attribution
//! that holds a recorded branch against what git wrote when it was created or renamed, and for the
//! revert gate's fallback fork point.
//!
//! They are read from the filesystem rather than asked of `git`, and opened only as regular files
//! and without blocking: a FIFO an agent left in the git directory would otherwise block a read
//! that runs while other work waits on the repository. A repository in reftable format keeps no
//! reflog files ([`uses_reftable`](super::repo_files::uses_reftable)), so nothing here finds
//! anything in one.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use crate::git::repo_files::is_plain_ref_name;
use crate::hostfs::open_regular;
use crate::platform::names;

/// How much of a reflog's first line is read. A line is two object ids, an identity, a time and
/// a message.
const MAX_REFLOG_LINE: u64 = 64 * 1024;

/// How much of a reflog is searched for a message. A branch's reflog grows by a line per commit,
/// and what is looked for is written at its creation or rename, so a reflog past this is searched
/// only as far as this.
const MAX_REFLOG_SEARCHED: u64 = 4 * 1024 * 1024;

/// The entry that created a branch: the oldest one in its reflog, provided it records no value
/// before it. Whatever made the branch wrote it (`git branch`, `git switch -c`, `git update-ref`
/// from a null value), so what its message says is for the caller to judge. A reflog that was
/// expired, or never kept, has an oldest entry that does not begin at nothing, and so no creation.
#[derive(Debug, PartialEq, Eq)]
pub struct Creation {
    /// The commit the branch named after the entry.
    pub commit: String,
    /// Seconds since the epoch.
    pub time: i64,
    /// The entry's message, as git wrote it (`branch: Created from main`).
    pub message: Vec<u8>,
}

fn reflog_path(common_dir: &Path, branch: &[u8]) -> Option<PathBuf> {
    if !is_plain_ref_name(branch) {
        return None;
    }
    names::path_from_name_bytes(branch)
        .map(|branch| common_dir.join("logs/refs/heads").join(branch))
}

/// How a reflog line begins: whether its old value is no value at all.
struct Line<'a> {
    from_nothing: bool,
    commit: String,
    time: i64,
    message: &'a [u8],
}

/// The creation of the local branch `branch` (its name as bytes) in the repository at
/// `common_dir`, from the first line of `logs/refs/heads/<branch>`.
pub fn creation_of(common_dir: &Path, branch: &[u8]) -> Option<Creation> {
    let file = open_regular(&reflog_path(common_dir, branch)?).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(MAX_REFLOG_LINE))
        .read_until(b'\n', &mut line)
        .ok()?;
    let line = parse_reflog_line(&line).filter(|line| line.from_nothing)?;
    Some(Creation {
        commit: line.commit,
        time: line.time,
        message: line.message.to_vec(),
    })
}

/// Whether any entry of the branch's reflog has a message `matches` accepts.
pub fn has_message(common_dir: &Path, branch: &[u8], matches: impl Fn(&[u8]) -> bool) -> bool {
    let Some(file) = reflog_path(common_dir, branch).and_then(|path| open_regular(&path).ok())
    else {
        return false;
    };
    BufReader::new(file.take(MAX_REFLOG_SEARCHED))
        .split(b'\n')
        .map_while(Result::ok)
        .any(|line| parse_reflog_line(&line).is_some_and(|line| matches(line.message)))
}

/// `<old> <new> <identity> <seconds> <zone>\t<message>`. The identity holds spaces of its own, so
/// the time is found from the right.
fn parse_reflog_line(line: &[u8]) -> Option<Line<'_>> {
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
    Some(Line {
        from_nothing,
        commit,
        time,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo_files::repository_of;
    #[cfg(unix)]
    use crate::test_support::mkfifo;
    use crate::test_support::{git, repo_with};

    #[test]
    fn a_reflog_line_is_read_from_the_right() {
        let line = b"0000000000000000000000000000000000000000 1234567890abcdef1234567890abcdef12345678 A B <a@b.c> 1700000000 +0900\tbranch: Created from HEAD\n";
        let line = parse_reflog_line(line).unwrap();
        assert!(line.from_nothing);
        assert_eq!(line.commit, "1234567890abcdef1234567890abcdef12345678");
        assert_eq!(line.time, 1_700_000_000);
        assert_eq!(line.message, b"branch: Created from HEAD");
    }

    /// The same definition holds whoever made the branch, so a message other than git's own does
    /// not hide a creation, and an expired reflog is no creation.
    #[test]
    fn a_creation_is_the_entry_that_begins_at_nothing_whatever_made_it() {
        let repo = repo_with("reflog-creation", &[(b"a", b"a")]);
        let common = repository_of(&repo).unwrap();
        let head = String::from_utf8(git(&repo, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_string();

        git(&repo, &["checkout", "-q", "-b", "made-by-checkout"]);
        let made = creation_of(&common, b"made-by-checkout").unwrap();
        assert_eq!(made.commit, head);
        assert_eq!(made.message, b"branch: Created from HEAD");

        git(
            &repo,
            &[
                "update-ref",
                "-m",
                "imported",
                "refs/heads/made-by-ref",
                &head,
            ],
        );
        let made = creation_of(&common, b"made-by-ref").unwrap();
        assert_eq!(made.commit, head);
        assert_eq!(made.message, b"imported");

        git(
            &repo,
            &["reflog", "expire", "--expire=all", "refs/heads/made-by-ref"],
        );
        git(
            &repo,
            &[
                "update-ref",
                "-m",
                "reset: moving",
                "refs/heads/made-by-ref",
                &head,
            ],
        );
        assert_eq!(creation_of(&common, b"made-by-ref"), None);
        assert_eq!(creation_of(&common, b"missing"), None);
        assert_eq!(creation_of(&common, b"../HEAD"), None);
    }

    #[test]
    fn a_message_is_found_anywhere_in_the_reflog() {
        let repo = repo_with("reflog-message", &[(b"a", b"a")]);
        let common = repository_of(&repo).unwrap();
        git(&repo, &["branch", "topic"]);
        git(&repo, &["branch", "-m", "topic", "renamed"]);
        assert!(has_message(&common, b"renamed", |m| m
            == b"Branch: renamed refs/heads/topic to refs/heads/renamed"));
        assert!(!has_message(&common, b"renamed", |m| m == b"nothing"));
    }

    /// The read runs under the repository's gate, so a FIFO there must not block it.
    #[cfg(unix)]
    #[test]
    fn a_reflog_that_is_no_regular_file_is_read_as_none() {
        let repo = repo_with("reflog-fifo", &[(b"a", b"a")]);
        let common = repository_of(&repo).unwrap();
        git(&repo, &["branch", "topic"]);
        mkfifo(&common.join("logs/refs/heads/topic"));
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || sender.send(creation_of(&common, b"topic")));
        let read = receiver.recv_timeout(crate::test_support::PATIENCE);
        assert_eq!(read.expect("the reflog read blocked"), None);
    }
}
