//! What the attribution reads out of a repository's administrative files: which repository a
//! directory is in, which branch each worktree has checked out, and a branch's first reflog entry.
//!
//! All of it is read from the filesystem rather than asked of `git`: these run on the hook path,
//! once per hook, and spawning `git` there would defeat the point of the refs stamp.

use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::browse::{source, wire_path};

/// How much of a reflog's first line is read. A line is two object ids, an identity, a time and
/// a message.
const MAX_REFLOG_LINE: u64 = 64 * 1024;

/// What `git branch`, `checkout -b`, `switch -c` and `worktree add -b` write as the message of a
/// branch's first reflog entry.
pub(super) const CREATED_FROM: &[u8] = b"branch: Created from";

/// The canonical common directory of the repository holding `dir`. `None` for a directory in no
/// repository.
pub fn repository_of(dir: &Path) -> Option<PathBuf> {
    let ceiling = source::home_ceiling();
    let canonical = fs::canonicalize(dir).ok()?;
    let top = source::find_dot_git(&canonical, ceiling.as_deref())?;
    let admin = source::admin_dir(&top)?;
    // A linked worktree's git directory names the common one; the main worktree's is it.
    match fs::read(admin.join("commondir")) {
        Ok(pointer) => {
            let pointer = pointer.strip_suffix(b"\n").unwrap_or(&pointer);
            fs::canonicalize(admin.join(OsStr::from_bytes(pointer))).ok()
        }
        Err(_) => Some(admin),
    }
}

/// A worktree of a repository and what it has checked out.
pub(super) struct Worktree {
    /// Its directory, canonical.
    pub(super) path: PathBuf,
    /// The branch checked out there, as a wire path; `None` for a detached `HEAD`.
    pub(super) branch: Option<String>,
}

/// Each worktree of the repository whose common directory is `common_dir`. A worktree whose
/// directory has gone is left out.
pub(super) fn worktrees(common_dir: &Path) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut add = |path: &Path, admin: &Path| {
        let (Ok(path), Ok(head)) = (fs::canonicalize(path), fs::read(admin.join("HEAD"))) else {
            return;
        };
        let branch = head
            .trim_ascii_end()
            .strip_prefix(b"ref: refs/heads/")
            .map(wire_path::encode);
        worktrees.push(Worktree { path, branch });
    };
    // The main worktree is the directory holding `.git`; a bare repository has none.
    if common_dir.file_name() == Some(OsStr::new(".git")) {
        if let Some(main) = common_dir.parent() {
            add(main, common_dir);
        }
    }
    let Ok(entries) = fs::read_dir(common_dir.join("worktrees")) else {
        return worktrees;
    };
    for entry in entries.flatten() {
        let admin = entry.path();
        // The worktree's own `.git` file, as an absolute path or one relative to `admin`.
        let Ok(gitdir) = fs::read(admin.join("gitdir")) else {
            continue;
        };
        let gitdir = admin.join(OsStr::from_bytes(gitdir.trim_ascii_end()));
        if let Some(path) = gitdir.parent() {
            add(path, &admin);
        }
    }
    worktrees
}

/// The first entry of a branch's reflog.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReflogEntry {
    /// The commit the branch named after the entry.
    pub(super) commit: String,
    /// Seconds since the epoch.
    pub(super) time: i64,
    pub(super) message: Vec<u8>,
}

impl ReflogEntry {
    /// Whether the entry is one a branch's own creation writes, rather than a reference merely
    /// being written: the default branch a `git init` leaves, a clone's, a bare `git update-ref`.
    pub(super) fn is_creation(&self) -> bool {
        self.message.starts_with(CREATED_FROM)
    }
}

/// The first line of `logs/refs/heads/<branch>`, if the branch has a reflog.
pub(super) fn first_reflog_entry(common_dir: &Path, branch: &str) -> Option<ReflogEntry> {
    let name = wire_path::decode(branch)?;
    let path = common_dir
        .join("logs/refs/heads")
        .join(OsStr::from_bytes(&name));
    let file = fs::File::open(path).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(MAX_REFLOG_LINE))
        .read_until(b'\n', &mut line)
        .ok()?;
    parse_reflog_line(&line)
}

/// `<old> <new> <identity> <seconds> <zone>\t<message>`. The identity holds spaces of its own, so
/// the time is found from the right.
fn parse_reflog_line(line: &[u8]) -> Option<ReflogEntry> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let (head, message) = match line.iter().position(|&b| b == b'\t') {
        Some(at) => (&line[..at], &line[at + 1..]),
        None => (line, &[][..]),
    };
    let mut fields = head.splitn(3, |&b| b == b' ');
    fields.next()?;
    let commit = std::str::from_utf8(fields.next()?).ok()?.to_string();
    let mut tail = fields.next()?.rsplitn(3, |&b| b == b' ');
    tail.next()?;
    let time = std::str::from_utf8(tail.next()?).ok()?.parse().ok()?;
    Some(ReflogEntry {
        commit,
        time,
        message: message.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git, repo_with};

    #[test]
    fn a_reflog_line_is_read_from_the_right() {
        let line = b"0000000000000000000000000000000000000000 1234567890abcdef1234567890abcdef12345678 A B <a@b.c> 1700000000 +0900\tbranch: Created from HEAD\n";
        let entry = parse_reflog_line(line).unwrap();
        assert_eq!(entry.commit, "1234567890abcdef1234567890abcdef12345678");
        assert_eq!(entry.time, 1_700_000_000);
        assert_eq!(entry.message, b"branch: Created from HEAD");
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
