//! Where a worktree's git directories are, read from the files `git` leaves for it rather than by
//! running `git`: a `.git` file naming a linked worktree's or a submodule's git directory, and the
//! `commondir` file in a linked worktree's git directory naming the repository's common one.
//!
//! What sits at `.git` is whatever the user or an agent put there: a FIFO whose open blocks until a
//! writer appears, a link to a device that reads forever. So a pointer file is opened without
//! blocking, taken only when it is a regular file, and read only up to [`MAX_POINTER`]. A `.git`
//! that is neither a directory nor a regular file resolves to nothing, as `git` itself refuses it;
//! such a `commondir` is an error its caller weighs.

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// The most bytes a pointer file is read to. It holds one path; `git` refuses a `.git` file past a
/// megabyte, and no real path comes near this.
const MAX_POINTER: usize = 64 * 1024;

/// The path a pointer file holds, with the line ending `git` strips: trailing CRs and LFs.
fn pointer(mut bytes: &[u8]) -> &[u8] {
    while let [rest @ .., b'\n' | b'\r'] = bytes {
        bytes = rest;
    }
    bytes
}

/// The git directory of the worktree whose `.git` entry is in `top`, as `git` reads it: the entry
/// itself when it is a directory, else the path a regular `.git` file names after `gitdir: `,
/// relative to `top` when written relative. Not resolved further; `None` where `.git` is neither.
pub fn git_dir(top: &Path) -> Option<PathBuf> {
    let dot_git = top.join(".git");
    if std::fs::metadata(&dot_git).ok()?.is_dir() {
        return Some(dot_git);
    }
    let contents = super::read_regular(&dot_git, MAX_POINTER).ok()?;
    let path = pointer(contents.strip_prefix(b"gitdir: ")?);
    (!path.is_empty()).then(|| top.join(OsStr::from_bytes(path)))
}

/// The common directory of the git directory `git_dir`: the one its `commondir` file names
/// (relative to `git_dir` when written relative), or `git_dir` itself when there is no such file.
/// Not resolved further. An error where a `commondir` is there but cannot be read as one, which
/// each caller weighs for itself.
pub fn common_dir(git_dir: &Path) -> std::io::Result<PathBuf> {
    match super::read_regular(&git_dir.join("commondir"), MAX_POINTER) {
        Ok(contents) => match pointer(&contents) {
            [] => Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "empty commondir",
            )),
            path => Ok(git_dir.join(OsStr::from_bytes(path))),
        },
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(git_dir.to_path_buf()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ScratchDir, PATIENCE};

    fn mkfifo(path: &Path) {
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    }

    /// Runs `resolve` on a thread, so a read that blocks fails the test instead of hanging it.
    fn within_patience<T: Send + 'static>(resolve: impl FnOnce() -> T + Send + 'static) -> T {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || sender.send(resolve()));
        receiver
            .recv_timeout(PATIENCE)
            .expect("resolving the git directory blocked")
    }

    #[test]
    fn a_dot_git_file_names_the_git_directory_relative_to_its_worktree() {
        let dir = ScratchDir::new("git-dir-pointer");
        std::fs::write(dir.join(".git"), "gitdir: ../main/.git/worktrees/linked\n").unwrap();
        assert_eq!(
            git_dir(&dir),
            Some(dir.join("../main/.git/worktrees/linked"))
        );
    }

    #[test]
    fn a_commondir_names_the_common_directory_relative_to_the_git_directory() {
        let git_dir = ScratchDir::new("git-dir-commondir");
        std::fs::write(git_dir.join("commondir"), "../..\r\n").unwrap();
        assert_eq!(common_dir(&git_dir).unwrap(), git_dir.join("../.."));
    }

    #[test]
    fn a_dot_git_that_is_no_regular_file_resolves_to_nothing_without_blocking() {
        let fifo = ScratchDir::new("git-dir-fifo");
        mkfifo(&fifo.join(".git"));
        let device = ScratchDir::new("git-dir-device");
        std::os::unix::fs::symlink("/dev/zero", device.join(".git")).unwrap();

        for top in [fifo.to_path_buf(), device.to_path_buf()] {
            assert_eq!(within_patience(move || git_dir(&top)), None);
        }
    }

    #[test]
    fn a_dot_git_file_past_the_bound_resolves_to_nothing() {
        let dir = ScratchDir::new("git-dir-oversized");
        let mut contents = b"gitdir: ".to_vec();
        contents.resize(MAX_POINTER + 1, b'a');
        std::fs::write(dir.join(".git"), contents).unwrap();
        assert_eq!(git_dir(&dir), None);
    }

    #[test]
    fn a_commondir_that_is_no_regular_file_is_an_error_without_blocking() {
        let git_dir = ScratchDir::new("git-dir-commondir-fifo");
        mkfifo(&git_dir.join("commondir"));
        let git_dir = git_dir.to_path_buf();
        assert!(within_patience(move || common_dir(&git_dir)).is_err());
    }
}
