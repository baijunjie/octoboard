//! Whether a shell terminal's shell is busy: running something a client should ask about before
//! closing it. **Blocking**: it reads the records of the shell's descendants, and of every process
//! on a Linux kernel that keeps no list of a process's children.

use std::path::Path;

use super::ShellProcess;
use crate::processes::native;

/// The most descendants of the shell one check looks at, so a fork bomb cannot hold it up.
const MAX_DESCENDANTS: usize = 512;

/// Whether the shell is running something: the terminal's foreground process group is not the
/// shell's own (a program it started in the foreground), or a descendant of the shell still has
/// the terminal open as its stdin, stdout or stderr, which covers a job sent to the background,
/// such as a server. A helper the prompt keeps beside the shell with all three streams
/// redirected away from the terminal (a prompt's git status daemon, say) does not count. A shell
/// at its prompt has neither.
pub(super) fn is_busy(process: &ShellProcess) -> bool {
    let shell = process.pid as libc::pid_t;
    process
        .foreground_process_group()
        .is_some_and(|group| group != shell)
        || process.slave_path().is_some_and(|slave| {
            descendant_holds(shell, native::children_lister(shell), |pid| {
                holds_terminal(pid, &slave)
            })
        })
}

/// Whether any descendant of `shell` satisfies `holds`, walking the tree depth first. Reaching
/// `MAX_DESCENDANTS` with descendants left unlooked-at, or a listing of children that failed
/// (`None`), counts as holding: what cannot be ruled out is asked about.
fn descendant_holds(
    shell: libc::pid_t,
    mut children: impl FnMut(libc::pid_t) -> Option<Vec<libc::pid_t>>,
    mut holds: impl FnMut(libc::pid_t) -> bool,
) -> bool {
    let Some(mut pending) = children(shell) else {
        return true;
    };
    let mut seen = 0;
    while let Some(pid) = pending.pop() {
        if seen == MAX_DESCENDANTS {
            return true;
        }
        if holds(pid) {
            return true;
        }
        seen += 1;
        let Some(found) = children(pid) else {
            return true;
        };
        pending.extend(found);
    }
    false
}

/// Whether a failed look at a process's descriptors was a refusal, as opposed to the process or
/// the descriptor being gone. A refusal (another user's process, such as `sudo tail -f x &`)
/// leaves it unknown whether the process holds the terminal, which counts as holding it.
fn refused(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
}

/// Whether `pid` has the terminal `slave` open as its stdin, stdout or stderr.
fn holds_terminal(pid: libc::pid_t, slave: &Path) -> bool {
    (0..3).any(|fd| match native::fd_path(pid, fd) {
        Ok(path) => path.is_some_and(|path| path == slave),
        Err(error) => refused(&error),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_past_the_cap_reads_as_holding() {
        // Every process has one child, forever, and none holds the terminal.
        assert!(descendant_holds(1, |pid| Some(vec![pid + 1]), |_| false));
    }

    #[test]
    fn a_tree_within_the_cap_holds_only_through_a_member_that_does() {
        let children = |pid: libc::pid_t| match pid {
            1 => Some(vec![2, 3]),
            3 => Some(vec![4]),
            _ => Some(Vec::new()),
        };
        assert!(!descendant_holds(1, children, |_| false));
        assert!(descendant_holds(1, children, |pid| pid == 4));
    }

    #[test]
    fn the_shell_itself_is_not_looked_at() {
        assert!(!descendant_holds(1, |_| Some(Vec::new()), |_| true));
    }

    #[test]
    fn a_listing_of_children_that_failed_reads_as_holding() {
        assert!(descendant_holds(1, |_| None, |_| false));
        // The failure is met below the shell, after a member that does not hold.
        let children = |pid: libc::pid_t| (pid == 1).then(|| vec![2]);
        assert!(descendant_holds(1, children, |_| false));
    }

    #[test]
    fn only_a_refusal_counts_as_unknown() {
        for errno in [libc::EPERM, libc::EACCES] {
            assert!(refused(&std::io::Error::from_raw_os_error(errno)));
        }
        for errno in [libc::ESRCH, libc::ENOENT, libc::EBADF] {
            assert!(!refused(&std::io::Error::from_raw_os_error(errno)));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_process_of_another_user_reads_as_holding() {
        // `launchd` belongs to root, so its descriptors are refused to an ordinary user.
        // SAFETY: `geteuid` takes no arguments and cannot fail.
        if unsafe { libc::geteuid() } != 0 {
            assert!(holds_terminal(1, Path::new("/dev/ttys999")));
        }
    }
}
