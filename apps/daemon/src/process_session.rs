//! Starting a process as the leader of a session of its own, and killing what it left in that
//! session. An agent CLI moves some helpers (a REPL or code-execution host, a background shell
//! task) into process groups of their own, so a signal to its process group leaves them running;
//! they keep its session, which is what reaches them. A helper that called `setsid()` itself has
//! left the session and is out of reach.
//!
//! The session is told by its id, which is the leader's pid, so it is swept only while the leader
//! is unreaped: until then its pid, and with it the session id, cannot pass to another process.

use std::os::unix::process::CommandExt;
use std::process::Command;

/// Has `command` start in a session of its own (`setsid()`), which it leads, and so in a process
/// group of its own too. Not to be combined with `process_group`: a process that already leads a
/// group cannot start a session.
pub(crate) fn lead_session(command: &mut Command) {
    // SAFETY: `setsid` takes no arguments and only affects this child, which has not returned
    // from `exec` yet. If it fails, the closure returns that error, `spawn` ends the child before
    // `exec` and returns the error, and nothing is started.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Whether the child `pid` has exited, found out without reaping it (`WNOWAIT`), or `None` when
/// `waitid` cannot say — as when the child has already been reaped, its pid then free to pass to
/// another process, so `None` never allows a sweep.
pub(crate) fn exited_unreaped(pid: libc::pid_t) -> Option<bool> {
    // SAFETY: a zeroed `siginfo_t` is a valid buffer for `waitid` to fill in, and it is only read
    // once `waitid` has succeeded.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        let flags = libc::WEXITED | libc::WNOHANG | libc::WNOWAIT;
        if libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, flags) != 0 {
            return None;
        }
        // With `WNOHANG`, a child that has not exited leaves `si_pid` as it was: 0.
        Some(info.si_pid() != 0)
    }
}

/// Kills the process group `leader` leads, which a session leader does too. The caller holds
/// `leader` unreaped, or knows the group still has a member, so the group id is still its own.
pub(crate) fn kill_group(leader: libc::pid_t) {
    // SAFETY: a plain signal to a process group whose id is still its own, as above.
    unsafe {
        libc::kill(-leader, libc::SIGKILL);
    }
}

/// What [`kill_members`] did.
pub(crate) struct Swept {
    pub killed: usize,
    /// `false` when listing the processes failed, so some may be left.
    pub complete: bool,
}

impl Swept {
    /// Logs the sweep of the session `leader` leads, named `process` in the log, when it killed
    /// anything or could not finish.
    pub(crate) fn log(&self, leader: libc::pid_t, process: &str) {
        if !self.complete {
            tracing::warn!(
                process,
                pid = leader,
                killed = self.killed,
                "listing processes failed; the sweep of the process's session may be incomplete"
            );
        } else if self.killed > 0 {
            tracing::info!(
                process,
                pid = leader,
                killed = self.killed,
                "killed processes left in the process's session"
            );
        }
    }
}

/// Kills every process in the session `leader` leads, other than the leader. The caller holds
/// `leader` unreaped, so a process found in that session is the leader's; it may still be
/// running, but then a process it forks after the sweep is missed, so a leader is best swept
/// once it has exited.
pub(crate) fn kill_members(leader: libc::pid_t) -> Swept {
    // A process killed in one pass may have forked just before it died, so passes repeat until
    // one signals no process it had not already signalled. A killed process can stay in the list
    // as a zombie, which is why those are remembered.
    const MAX_PASSES: usize = 3;
    let mut signalled = std::collections::HashSet::new();
    for _ in 0..MAX_PASSES {
        let Some(candidates) = candidates() else {
            return Swept {
                killed: signalled.len(),
                complete: false,
            };
        };
        let before = signalled.len();
        for pid in candidates {
            if pid == leader || signalled.contains(&pid) {
                continue;
            }
            // SAFETY: `getsid` only reads; a pid that is gone in the meantime reports an error.
            if unsafe { libc::getsid(pid) } != leader {
                continue;
            }
            // SAFETY: the pid was in the session of the unreaped leader a moment ago. It may have
            // exited since, which is the `ESRCH` that is ignored.
            if unsafe { libc::kill(pid, libc::SIGKILL) } == 0 {
                signalled.insert(pid);
            }
        }
        if signalled.len() == before {
            break;
        }
    }
    Swept {
        killed: signalled.len(),
        complete: true,
    }
}

/// Every process id on the system, a superset of the session's members for the caller to filter
/// with `getsid`, or `None` when the system cannot list them.
#[cfg(target_os = "linux")]
fn candidates() -> Option<Vec<libc::pid_t>> {
    let entries = std::fs::read_dir("/proc").ok()?;
    Some(
        entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .collect(),
    )
}

/// As the Linux one, from `libproc`.
#[cfg(target_os = "macos")]
fn candidates() -> Option<Vec<libc::pid_t>> {
    // SAFETY: a null buffer of size 0 only asks how many processes there are.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return None;
    }
    // Room beyond that count for processes started since.
    let mut pids = vec![0 as libc::pid_t; count as usize + 256];
    let size = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
    // SAFETY: `pids` is writable for `size` bytes.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
    if filled <= 0 {
        return None;
    }
    pids.truncate(filled as usize);
    // Pid 0 is the kernel, which `getsid(0)` would answer for the caller instead.
    pids.retain(|&pid| pid > 0);
    Some(pids)
}
