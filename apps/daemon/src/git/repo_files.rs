//! What a repository's administrative files say, read from the filesystem rather than asked of
//! `git`: which repository a directory or a git directory is in, whether its references are in
//! reftable format, whether a reference exists (loose or packed), whether an operation (a merge, a
//! rebase) is in progress and which branch a rebase is on, and the remote default branch a remote's
//! `HEAD` file names.
//!
//! A file read is cheaper than a process, and callers run while other work waits on the
//! repository. For that reason a file is opened only as a regular file and without blocking: a
//! FIFO an agent left in the git directory would otherwise hold them up for good.

use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::git::layout;
use crate::hostfs::open_regular;

/// How much of `packed-refs` is searched for a reference: a line per reference, so this is far
/// more than a repository's branches come to, and a file past it is searched only as far as this.
const MAX_PACKED_REFS_SEARCHED: u64 = 16 * 1024 * 1024;

/// How much of a rebase state's `head-name` file is read: one reference name.
const MAX_HEAD_NAME: usize = 4 * 1024;

/// How much of a symbolic reference's file is read: `ref: ` and a reference name.
const MAX_SYMBOLIC_REF: u64 = 4 * 1024;

/// The canonical common directory of the repository holding `dir`. `None` for a directory in no
/// repository.
pub fn repository_of(dir: &Path) -> Option<PathBuf> {
    let ceiling = layout::home_ceiling();
    let canonical = fs::canonicalize(dir).ok()?;
    let top = layout::find_dot_git(&canonical, ceiling.as_deref())?;
    let admin = layout::admin_dir(&top)?;
    common_dir_of(&admin)
}

/// The canonical common directory of the git directory `git_dir`: the one a linked worktree's
/// `commondir` names, or the directory itself.
pub(crate) fn common_dir_of(git_dir: &Path) -> Option<PathBuf> {
    // A `commondir` that is there but cannot be read leaves the git directory itself, which lies
    // inside the common directory of a linked worktree: a folder holding the repository is still
    // found to hold it, where "no repository" would let a move to the Trash take it.
    let common_dir = layout::common_dir(git_dir).unwrap_or_else(|_| git_dir.to_path_buf());
    fs::canonicalize(common_dir).ok()
}

/// Whether the repository keeps its references in reftable format, which has no reflog files.
pub(crate) fn uses_reftable(common_dir: &Path) -> bool {
    common_dir.join("reftable").is_dir()
}

/// Whether `name` is made of components that can be joined onto a repository's path as they are:
/// none empty, `.` or `..`, and no NUL.
pub(crate) fn is_plain_ref_name(name: &[u8]) -> bool {
    !name.is_empty()
        && !name.contains(&0)
        && name
            .split(|&b| b == b'/')
            .all(|part| !part.is_empty() && part != b"." && part != b"..")
}

/// Whether the reference `name` (`refs/heads/main`) exists in the repository, as a loose file or a
/// line of `packed-refs`. A name with an empty or dot component is no reference, and is never
/// joined onto the repository's path.
pub(crate) fn reference_exists(common_dir: &Path, name: &[u8]) -> bool {
    if !is_plain_ref_name(name) {
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

/// The state a merge, a cherry-pick, a revert, a rebase or a bisect leaves in a git directory.
const OPERATION_MARKERS: [&str; 7] = [
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "rebase-merge",
    "rebase-apply",
    "sequencer",
    "BISECT_LOG",
];

/// Whether an operation is in progress in the worktree whose git directory is `git_dir`. An error
/// where a marker cannot be looked at, which is no answer either way.
pub(crate) fn operation_in_progress(git_dir: &Path) -> std::io::Result<bool> {
    for marker in OPERATION_MARKERS {
        if git_dir.join(marker).try_exists()? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The branch a rebase in the worktree whose git directory is `git_dir` is on, from its state's
/// `head-name` file. HEAD is detached during a rebase, so nothing else says. `None` for no rebase
/// and for a state that cannot be read as one, which the caller takes as not busy.
pub(crate) fn rebasing_branch(git_dir: &Path) -> Option<Vec<u8>> {
    ["rebase-merge", "rebase-apply"].iter().find_map(|state| {
        let head =
            crate::hostfs::read_regular(&git_dir.join(state).join("head-name"), MAX_HEAD_NAME)
                .ok()?;
        let name = head.trim_ascii().strip_prefix(b"refs/heads/")?;
        (!name.is_empty()).then(|| name.to_vec())
    })
}

/// The branch the repository's remote default branch is, read from the files of a repository
/// whose references are files: what `refs/remotes/<remote>/HEAD` is a symbolic reference to, less
/// the remote, for the remote `origin` or else the first in byte order whose `HEAD` names a
/// remote-tracking branch of its own that exists (loose or packed; a symbolic reference is never
/// packed). `None` for none, and for a repository with no such files.
pub(crate) fn remote_default_branch_in_files(common_dir: &Path) -> Option<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git, mkfifo, repo_with};

    #[test]
    fn the_remote_default_branch_is_read_from_the_remotes_head_files() {
        let repo = repo_with("repo-files-remote-head", &[(b"a", b"a")]);
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
    fn an_operation_in_progress_is_told_from_the_markers_in_the_git_directory() {
        let repo = repo_with("repo-files-operation", &[(b"a", b"a")]);
        let git_dir = repository_of(&repo).unwrap();
        assert!(!operation_in_progress(&git_dir).unwrap());
        std::fs::write(git_dir.join("MERGE_HEAD"), "x\n").unwrap();
        assert!(operation_in_progress(&git_dir).unwrap());
        std::fs::remove_file(git_dir.join("MERGE_HEAD")).unwrap();
        std::fs::create_dir(git_dir.join("rebase-merge")).unwrap();
        assert!(operation_in_progress(&git_dir).unwrap());
    }

    #[test]
    fn a_rebase_names_its_branch_and_a_fifo_in_its_place_names_none() {
        let repo = repo_with("repo-files-rebasing", &[(b"a", b"a")]);
        let git_dir = repository_of(&repo).unwrap();
        assert_eq!(rebasing_branch(&git_dir), None);
        std::fs::create_dir(git_dir.join("rebase-merge")).unwrap();
        let head_name = git_dir.join("rebase-merge/head-name");
        std::fs::write(&head_name, "refs/heads/feature/x\n").unwrap();
        assert_eq!(rebasing_branch(&git_dir), Some(b"feature/x".to_vec()));
        std::fs::write(&head_name, "detached HEAD\n").unwrap();
        assert_eq!(rebasing_branch(&git_dir), None);
        std::fs::remove_file(&head_name).unwrap();
        mkfifo(&head_name);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || sender.send(rebasing_branch(&git_dir)));
        let read = receiver.recv_timeout(crate::test_support::PATIENCE);
        assert_eq!(read.expect("the state read blocked"), None);
    }

    #[test]
    fn a_linked_worktree_resolves_to_the_common_directory() {
        let repo = repo_with("repo-files-linked", &[(b"a", b"a")]);
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

    /// Taken for "no repository", it would let a move to the Trash take the folder holding it.
    #[test]
    fn an_unreadable_commondir_still_places_the_repository_inside_the_common_directory() {
        let repo = repo_with("repo-files-commondir-fifo", &[(b"a", b"a")]);
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
}
