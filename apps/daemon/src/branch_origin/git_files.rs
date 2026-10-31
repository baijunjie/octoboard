//! The reflog files of a repository, which the attribution reads to hold a recorded branch against
//! what git wrote when it was created or renamed.
//!
//! They are read from the filesystem rather than asked of `git`, and opened only as regular files
//! and without blocking: a FIFO an agent left in the git directory would otherwise block a read
//! that runs while other work waits on the repository.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::hostfs::open_regular;
use crate::wire_path;

/// How much of a reflog's first line is read. A line is two object ids, an identity, a time and
/// a message.
const MAX_REFLOG_LINE: u64 = 64 * 1024;

/// How much of a reflog is searched for a message. A branch's reflog grows by a line per commit,
/// and what is looked for is written at its creation or rename, so a reflog past this is searched
/// only as far as this.
const MAX_REFLOG_SEARCHED: u64 = 4 * 1024 * 1024;

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
    first_reflog_line(common_dir, branch).map(|(entry, _)| entry)
}

/// [`first_reflog_entry`] with the entry's message, as git wrote it (`branch: Created from main`).
pub(super) fn first_reflog_line(common_dir: &Path, branch: &str) -> Option<(ReflogEntry, Vec<u8>)> {
    let file = open_regular(&reflog_path(common_dir, branch)?).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(MAX_REFLOG_LINE))
        .read_until(b'\n', &mut line)
        .ok()?;
    parse_reflog_line(&line).map(|(entry, message)| (entry, message.to_vec()))
}

/// Whether any entry of the branch's reflog has a message `matches` accepts.
pub(super) fn reflog_has_message(
    common_dir: &Path,
    branch: &str,
    matches: impl Fn(&[u8]) -> bool,
) -> bool {
    let Some(file) = reflog_path(common_dir, branch).and_then(|path| open_regular(&path).ok())
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
    use crate::git::repo_files::repository_of;
    use crate::test_support::{git, mkfifo, repo_with};

    #[test]
    fn a_reflog_line_is_read_from_the_right() {
        let line = b"0000000000000000000000000000000000000000 1234567890abcdef1234567890abcdef12345678 A B <a@b.c> 1700000000 +0900\tbranch: Created from HEAD\n";
        let (entry, message) = parse_reflog_line(line).unwrap();
        assert!(entry.from_nothing);
        assert_eq!(entry.commit, "1234567890abcdef1234567890abcdef12345678");
        assert_eq!(entry.time, 1_700_000_000);
        assert_eq!(message, b"branch: Created from HEAD");
    }

    /// The read runs under the repository's gate, so a FIFO there must not block it.
    #[test]
    fn a_reflog_that_is_no_regular_file_is_read_as_none() {
        let repo = repo_with("branch-origin-reflog-fifo", &[(b"a", b"a")]);
        let common = repository_of(&repo).unwrap();
        git(&repo, &["branch", "topic"]);
        mkfifo(&common.join("logs/refs/heads/topic"));
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || sender.send(first_reflog_entry(&common, "topic")));
        let read = receiver.recv_timeout(crate::test_support::PATIENCE);
        assert_eq!(read.expect("the reflog read blocked"), None);
    }
}
