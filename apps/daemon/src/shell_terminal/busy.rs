//! Whether a shell terminal's shell is busy: running something a client should ask about before
//! closing it. **Blocking**: on a system that keeps no list of a process's children, it reads every
//! process's record.

use super::ShellProcess;

/// Whether the shell is running something: the terminal's foreground process group is not the
/// shell's own (a program it started in the foreground), or the shell has any child process,
/// which also covers a job sent to the background, such as a server. A shell at its prompt has
/// neither.
pub(super) fn is_busy(process: &ShellProcess) -> bool {
    let shell = process.pid as libc::pid_t;
    process
        .foreground_process_group()
        .is_some_and(|group| group != shell)
        || has_children(shell)
}

#[cfg(target_os = "macos")]
fn has_children(pid: libc::pid_t) -> bool {
    // One is enough to tell; the call fills in as many as fit and counts only those.
    let mut children = [0 as libc::pid_t; 8];
    // SAFETY: the buffer and the size passed describe the same array, which outlives the call.
    let found = unsafe {
        libc::proc_listchildpids(
            pid,
            children.as_mut_ptr().cast(),
            std::mem::size_of_val(&children) as libc::c_int,
        )
    };
    found > 0
}

#[cfg(target_os = "linux")]
fn has_children(pid: libc::pid_t) -> bool {
    // The kernel's own list of a thread's children, which a shell's single thread holds all of.
    // Absent on a kernel built without `CONFIG_PROC_CHILDREN`, which leaves reading every
    // process's parent.
    match std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")) {
        Ok(children) => !children.trim().is_empty(),
        Err(_) => any_process_with_parent(pid),
    }
}

#[cfg(target_os = "linux")]
fn any_process_with_parent(pid: libc::pid_t) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            return false;
        };
        if !name.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{name}/stat")) else {
            return false;
        };
        // `pid (comm) state ppid ...`, where `comm` may itself hold spaces and parentheses, so the
        // fields are counted from the last closing parenthesis.
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1))
            .and_then(|ppid| ppid.parse::<libc::pid_t>().ok())
            == Some(pid)
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn has_children(_pid: libc::pid_t) -> bool {
    false
}
