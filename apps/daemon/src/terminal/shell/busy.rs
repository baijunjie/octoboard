//! Whether a shell terminal's shell is busy: running something a client should ask about before
//! closing it. **Blocking**: it reads the records of the shell's descendants, and of every process
//! on a Linux kernel that keeps no list of a process's children.

use std::path::Path;

use super::ShellProcess;

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
            descendant_holds(shell, children_source(shell), |pid| {
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
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn refused(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
}

#[cfg(target_os = "macos")]
fn children_source(_shell: libc::pid_t) -> impl FnMut(libc::pid_t) -> Option<Vec<libc::pid_t>> {
    |pid| Some(children(pid))
}

#[cfg(target_os = "macos")]
fn children(pid: libc::pid_t) -> Vec<libc::pid_t> {
    let mut capacity = 64;
    loop {
        let mut children = vec![0 as libc::pid_t; capacity];
        // SAFETY: the buffer and the size passed describe the same vector, which outlives the call.
        let found = unsafe {
            libc::proc_listchildpids(
                pid,
                children.as_mut_ptr().cast(),
                std::mem::size_of_val(children.as_slice()) as libc::c_int,
            )
        };
        let found = found.max(0) as usize;
        // A buffer that came back full may have cut the list short, so it is asked again larger.
        if found >= capacity && capacity < MAX_CHILD_BUFFER {
            capacity *= 4;
            continue;
        }
        children.truncate(found);
        return children;
    }
}

/// The most entries `children` grows its buffer to for one process.
#[cfg(target_os = "macos")]
const MAX_CHILD_BUFFER: usize = 1 << 16;

/// The head of what `PROC_PIDFDVNODEPATHINFO` fills, `struct proc_fileinfo`, which the libc
/// crate does not carry.
#[cfg(target_os = "macos")]
#[repr(C)]
struct ProcFileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

/// `struct vnode_fdinfowithpath`, which `PROC_PIDFDVNODEPATHINFO` fills.
#[cfg(target_os = "macos")]
#[repr(C)]
struct VnodeFdInfoWithPath {
    file: ProcFileInfo,
    vnode: libc::vnode_info_path,
}

#[cfg(target_os = "macos")]
fn holds_terminal(pid: libc::pid_t, slave: &Path) -> bool {
    const PROC_PIDFDVNODEPATHINFO: libc::c_int = 2;
    let size = std::mem::size_of::<VnodeFdInfoWithPath>();
    (0..3).any(|fd| {
        // SAFETY: the structure is plain integers and characters, for which all zeroes is valid.
        let mut info: VnodeFdInfoWithPath = unsafe { std::mem::zeroed() };
        // SAFETY: the buffer and the size passed describe the same structure, which outlives the
        // call.
        let filled = unsafe {
            libc::proc_pidfdinfo(
                pid,
                fd,
                PROC_PIDFDVNODEPATHINFO,
                (&raw mut info).cast(),
                size as libc::c_int,
            )
        };
        if filled <= 0 {
            return refused(&std::io::Error::last_os_error());
        }
        if (filled as usize) < size {
            return false;
        }
        // SAFETY: the array of arrays of characters is one run of bytes.
        let path = unsafe {
            std::slice::from_raw_parts(
                (&raw const info.vnode.vip_path).cast::<u8>(),
                std::mem::size_of_val(&info.vnode.vip_path),
            )
        };
        let path = &path[..path
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(path.len())];
        path == slave.as_os_str().as_encoded_bytes()
    })
}

/// Reads children from the kernel's list when this kernel keeps one, decided once per check from
/// the shell's own entry, and from a single pass over `/proc` otherwise, which answers `None`
/// when the pass failed.
#[cfg(target_os = "linux")]
fn children_source(shell: libc::pid_t) -> impl FnMut(libc::pid_t) -> Option<Vec<libc::pid_t>> {
    // The list is absent on a kernel built without `CONFIG_PROC_CHILDREN`.
    let listed = std::fs::read_to_string(format!("/proc/{shell}/task/{shell}/children")).is_ok();
    let mut by_parent = None;
    move |pid| {
        if listed {
            Some(listed_children(pid))
        } else {
            by_parent
                .get_or_insert_with(children_by_parent)
                .as_ref()
                .map(|map| map.get(&pid).cloned().unwrap_or_default())
        }
    }
}

/// The kernel's own list of a process's children, read for each of its threads, since a program
/// may start a child from any of them. A thread that exited since the listing is skipped.
#[cfg(target_os = "linux")]
fn listed_children(pid: libc::pid_t) -> Vec<libc::pid_t> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    tasks
        .flatten()
        .filter_map(|task| std::fs::read_to_string(task.path().join("children")).ok())
        .flat_map(|children| {
            children
                .split_whitespace()
                .filter_map(|child| child.parse::<libc::pid_t>().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every process's children, from the parent each one's `stat` names; `None` when the processes
/// could not be listed.
#[cfg(target_os = "linux")]
fn children_by_parent() -> Option<std::collections::HashMap<libc::pid_t, Vec<libc::pid_t>>> {
    let mut by_parent = std::collections::HashMap::<libc::pid_t, Vec<libc::pid_t>>::new();
    for child in crate::processes::pids::all()? {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{child}/stat")) else {
            continue;
        };
        // `pid (comm) state ppid ...`, where `comm` may itself hold spaces and parentheses, so
        // the fields are counted from the last closing parenthesis.
        let parent = stat.rsplit_once(')').and_then(|(_, rest)| {
            rest.split_whitespace()
                .nth(1)
                .and_then(|ppid| ppid.parse::<libc::pid_t>().ok())
        });
        if let Some(parent) = parent {
            by_parent.entry(parent).or_default().push(child);
        }
    }
    Some(by_parent)
}

#[cfg(target_os = "linux")]
fn holds_terminal(pid: libc::pid_t, slave: &Path) -> bool {
    (0..3).any(
        |fd| match std::fs::read_link(format!("/proc/{pid}/fd/{fd}")) {
            Ok(target) => target == slave,
            Err(error) => refused(&error),
        },
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn children_source(_shell: libc::pid_t) -> impl FnMut(libc::pid_t) -> Option<Vec<libc::pid_t>> {
    |_| Some(Vec::new())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn holds_terminal(_pid: libc::pid_t, _slave: &Path) -> bool {
    false
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

    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
    fn the_descriptor_structure_has_the_size_the_system_fills() {
        assert_eq!(std::mem::size_of::<ProcFileInfo>(), 24);
        assert_eq!(std::mem::size_of::<VnodeFdInfoWithPath>(), 24 + 152 + 1024);
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
