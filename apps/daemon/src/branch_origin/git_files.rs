//! What the attribution reads out of a repository's administrative files: which repository a
//! directory or a git directory is in, whether its references are in reftable format, a branch's
//! reflog, whether a reference exists (loose or packed), and the remote default branch a remote's
//! `HEAD` file names.
//!
//! All of it is read from the filesystem rather than asked of `git`: a report and a listing are
//! handled while other work waits on the repository's gate, and a file read is cheaper than a
//! process. For the same reason a file is opened only as a regular file and without blocking: a
//! FIFO an agent left in the git directory would otherwise hold the gate for good.

use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::browse::{source, wire_path};
use crate::hostfs::open_regular;

/// How much of a reflog's first line is read. A line is two object ids, an identity, a time and
/// a message.
const MAX_REFLOG_LINE: u64 = 64 * 1024;

/// How much of a reflog is searched for a message. A branch's reflog grows by a line per commit,
/// and what is looked for is written at its creation or rename, so a reflog past this is searched
/// only as far as this.
const MAX_REFLOG_SEARCHED: u64 = 4 * 1024 * 1024;

/// How much of `packed-refs` is searched for a reference: a line per reference, so this is far
/// more than a repository's branches come to, and a file past it is searched only as far as this.
const MAX_PACKED_REFS_SEARCHED: u64 = 16 * 1024 * 1024;

/// How much of a symbolic reference's file is read: `ref: ` and a reference name.
const MAX_SYMBOLIC_REF: u64 = 4 * 1024;

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
    // A `commondir` that is there but cannot be read leaves the git directory itself, which lies
    // inside the common directory of a linked worktree: a folder holding the repository is still
    // found to hold it, where "no repository" would let a move to the Trash take it.
    let common_dir =
        crate::hostfs::git_dir::common_dir(git_dir).unwrap_or_else(|_| git_dir.to_path_buf());
    fs::canonicalize(common_dir).ok()
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

/// Whether the reference `name` (`refs/heads/main`) exists in the repository, as a loose file or a
/// line of `packed-refs`. A name with an empty or dot component is no reference, and is never
/// joined onto the repository's path.
pub(super) fn reference_exists(common_dir: &Path, name: &[u8]) -> bool {
    let plain = !name.is_empty()
        && name
            .split(|&b| b == b'/')
            .all(|part| !part.is_empty() && part != b"." && part != b"..");
    if !plain || name.contains(&0) {
        return false;
    }
    if common_dir.join(OsStr::from_bytes(name)).is_file() {
        return true;
    }
    let Ok(file) = open_regular(&common_dir.join("packed-refs")) else {
        return false;
    };
    BufReader::new(file.take(MAX_PACKED_REFS_SEARCHED))
        .split(b'\n')
        .map_while(Result::ok)
        .any(|line| {
            line.iter()
                .position(|&b| b == b' ')
                .is_some_and(|at| &line[at + 1..] == name)
        })
}

/// The branch the repository's remote default branch is, read from the files of a repository
/// whose references are files: what `refs/remotes/<remote>/HEAD` is a symbolic reference to, less
/// the remote, for the remote `origin` or else the first in byte order whose `HEAD` names a
/// remote-tracking branch of its own that exists (loose or packed; a symbolic reference is never
/// packed). `None` for none, and for a repository with no such files.
pub(super) fn remote_default_branch_in_files(common_dir: &Path) -> Option<Vec<u8>> {
    let remotes = fs::read_dir(common_dir.join("refs/remotes")).ok()?;
    let mut found: Vec<(Vec<u8>, Vec<u8>)> = remotes
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let remote = entry.file_name().as_bytes().to_vec();
            let mut text = Vec::new();
            open_regular(&entry.path().join("HEAD"))
                .ok()?
                .take(MAX_SYMBOLIC_REF)
                .read_to_end(&mut text)
                .ok()?;
            let target = text.strip_prefix(b"ref: ")?.trim_ascii_end();
            let prefix = [b"refs/remotes/", remote.as_slice(), b"/"].concat();
            let branch = target
                .strip_prefix(prefix.as_slice())
                .filter(|b| !b.is_empty())?;
            reference_exists(common_dir, target).then(|| (remote, branch.to_vec()))
        })
        .collect();
    found.sort_by(|a, b| (a.0 != b"origin", &a.0).cmp(&(b.0 != b"origin", &b.0)));
    found.into_iter().next().map(|(_, branch)| branch)
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
    fn the_remote_default_branch_is_read_from_the_remotes_head_files() {
        let repo = repo_with("branch-origin-remote-head", &[(b"a", b"a")]);
        let common = repository_of(&repo).unwrap();
        assert_eq!(remote_default_branch_in_files(&common), None);

        for remote in ["zeta", "alpha", "origin"] {
            let branch = format!("{remote}-main");
            git(
                &repo,
                &[
                    "update-ref",
                    &format!("refs/remotes/{remote}/{branch}"),
                    "HEAD",
                ],
            );
            git(
                &repo,
                &[
                    "symbolic-ref",
                    &format!("refs/remotes/{remote}/HEAD"),
                    &format!("refs/remotes/{remote}/{branch}"),
                ],
            );
        }
        assert_eq!(
            remote_default_branch_in_files(&common),
            Some(b"origin-main".to_vec())
        );

        git(&repo, &["pack-refs", "--all"]);
        assert_eq!(
            remote_default_branch_in_files(&common),
            Some(b"origin-main".to_vec())
        );

        git(
            &repo,
            &["update-ref", "-d", "refs/remotes/origin/origin-main"],
        );
        assert_eq!(
            remote_default_branch_in_files(&common),
            Some(b"alpha-main".to_vec()),
            "a head naming a branch that is gone does not count"
        );
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

    fn mkfifo(path: &Path) {
        let _ = fs::remove_file(path);
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    }

    /// Taken for "no repository", it would let a move to the Trash take the folder holding it.
    #[test]
    fn an_unreadable_commondir_still_places_the_repository_inside_the_common_directory() {
        let repo = repo_with("branch-origin-commondir-fifo", &[(b"a", b"a")]);
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
        let common = repository_of(&repo).unwrap();
        mkfifo(&common.join("worktrees/linked/commondir"));
        assert!(repository_of(&linked).is_some_and(|found| found.starts_with(&common)));
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
