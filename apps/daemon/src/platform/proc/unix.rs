use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

use super::{
    tag_value, Polite, ProcessId, ProcessIdentity, Sent, SetEnv, Signal, Swept, Unreaped,
    TAG_VARIABLE,
};
use crate::platform::proc_table::{self, pids};

/// What tells one launch's processes from any other's: the tag every one of them carries in its
/// environment, which a process inherits from its parent. The host lists no members, so what
/// carries the tag is found by reading the environments of the processes it lists.
#[derive(Debug, Clone)]
pub struct Containment {
    tag: String,
}

impl Containment {
    /// The containment of the launch `launch` of the daemon instance `instance`.
    pub fn create(instance: &str, launch: &str) -> io::Result<Self> {
        Ok(Self {
            tag: tag_value(instance, launch),
        })
    }

    /// Puts what makes the processes started with `environment` the launch's into it.
    pub fn prepare(&self, environment: &mut impl SetEnv) {
        environment.set_env(TAG_VARIABLE, &self.tag);
    }

    /// The processes the host says are in the containment, or `None` when it cannot say, and the
    /// callers find them by their environment.
    pub fn members(&self) -> Option<Vec<ProcessId>> {
        None
    }

    /// The containment of a launch of an earlier run of the daemon, found again by its name.
    pub fn reopen(instance: &str, launch: &str) -> Option<Self> {
        Self::create(instance, launch).ok()
    }
}

/// A child process the caller holds unreaped, so that its pid, and with it its process group and
/// its session, are still its own. Made for such a child and dropped once it is reaped.
#[derive(Debug)]
pub struct ChildHandle {
    pid: ProcessId,
}

impl ChildHandle {
    /// A handle on the child `pid`, just started leading what `_reach` names ([`lead_group`],
    /// [`lead_session`]), which the caller holds unreaped. A containment's tag is in the
    /// environment the child was started with, so being in it asks nothing more here.
    pub fn started(
        pid: ProcessId,
        _reach: super::Reach,
        _containment: Option<&Containment>,
    ) -> io::Result<Self> {
        Ok(Self { pid })
    }

    pub fn pid(&self) -> ProcessId {
        self.pid
    }

    /// Whether the child has exited, found out without reaping it, or `None` when the host cannot
    /// say, as for a child already reaped.
    pub fn exited_unreaped(&self) -> Option<bool> {
        unreaped_state(self.pid).map(|state| state != Unreaped::Running)
    }

    /// Asks the child itself to end as `how` says, leaving what it started alone.
    pub fn stop(&self, how: Polite) -> Sent {
        signal_process(self.pid, Signal::Polite(how))
    }

    /// Kills the child and what it started without moving it elsewhere.
    pub fn kill(&self) -> Sent {
        signal_group(self.pid, Signal::Kill)
    }

    /// Kills what the child started, wherever it went, other than the child itself and the
    /// processes `keep` names. Best made once the child has exited, since what it starts after
    /// the sweep is missed.
    pub fn kill_leftovers(&self, keep: &[ProcessIdentity]) -> Swept {
        kill_members(self.pid, keep)
    }

    /// Whether the process group the child leads has a member other than the child, or `None`
    /// when the system cannot list its processes; see [`group_has_members`].
    pub fn group_has_members(&self) -> Option<bool> {
        group_has_members(self.pid)
    }

    /// Reaps the child if it has exited, and says whether it did. For a child nothing else
    /// reaps, such as one started on a terminal; once it is reaped its pid is free to pass to
    /// another process, so nothing is sent through this handle any more.
    pub fn reap_if_exited(&self) -> io::Result<bool> {
        let mut status = 0;
        // SAFETY: `status` is a valid place for the status, and `WNOHANG` never blocks.
        match unsafe { libc::waitpid(raw(self.pid), &mut status, libc::WNOHANG) } {
            0 => Ok(false),
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(true),
        }
    }
}

/// The system's own `pid_t` for `pid`.
fn raw(pid: ProcessId) -> libc::pid_t {
    pid.0 as libc::pid_t
}

fn number(signal: Signal) -> libc::c_int {
    match signal {
        Signal::Polite(Polite::Terminate) => libc::SIGTERM,
        Signal::Polite(Polite::HangUp) => libc::SIGHUP,
        Signal::Kill => libc::SIGKILL,
    }
}

/// Has `command` start in a session of its own (`setsid()`), which it leads, and so in a process
/// group of its own too. Not to be combined with [`lead_group`]: a process that already leads a
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

/// Has `command` start in a process group of its own, which it leads (`setpgid(0, 0)`).
pub(crate) fn lead_group(command: &mut Command) {
    command.process_group(0);
}

/// Has `command` start apart from the daemon, for an application that outlives it, which nothing
/// waits on or ends: in a session of its own, as [`lead_session`].
pub(crate) fn detach(command: &mut Command) {
    lead_session(command);
}

/// Has the calling process leave its session and its process group for new ones of its own, so
/// that a signal to the group it was started in, which whatever started it may send as soon as it
/// returns, does not reach it. For a process that outlives the command that started it.
pub(crate) fn leave_session() {
    // SAFETY: no arguments, and it only changes this process's own session.
    unsafe {
        libc::setsid();
    }
}

/// How the child `pid` stands, found out without reaping it (`WNOWAIT`), or `None` when `waitid`
/// cannot say — as when the child has already been reaped, its pid then free to pass to another
/// process, so `None` never allows a sweep.
pub(crate) fn unreaped_state(pid: ProcessId) -> Option<Unreaped> {
    // SAFETY: a zeroed `siginfo_t` is a valid buffer for `waitid` to fill in, and it is only read
    // once `waitid` has succeeded.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        let flags = libc::WEXITED | libc::WNOHANG | libc::WNOWAIT;
        if libc::waitid(libc::P_PID, pid.0 as libc::id_t, &mut info, flags) != 0 {
            return None;
        }
        // With `WNOHANG`, a child that has not exited leaves `si_pid` as it was: 0.
        if info.si_pid() == 0 {
            return Some(Unreaped::Running);
        }
        let success = info.si_code == libc::CLD_EXITED && info.si_status() == 0;
        Some(Unreaped::Exited { success })
    }
}

fn send(target: libc::pid_t, signal: Signal) -> Sent {
    // SAFETY: a plain signal; what makes `target` still the process or group meant is the
    // caller's, as the callers of the public functions state.
    if unsafe { libc::kill(target, number(signal)) } == 0 {
        return Sent::Delivered;
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        Sent::Gone
    } else {
        Sent::Failed(err)
    }
}

/// Sends `signal` to the process `pid`. The caller holds it unreaped, or knows by its start time
/// that the pid is still its own, so the pid has not passed to another process.
pub(crate) fn signal_process(pid: ProcessId, signal: Signal) -> Sent {
    send(raw(pid), signal)
}

/// Sends `signal` to the process `id` names. A signal goes by pid alone, so the caller reads the
/// start time just before ([`ProcessIdentity::is_running`]).
pub(crate) fn signal_identified(id: &ProcessIdentity, signal: Signal) -> Sent {
    signal_process(id.pid, signal)
}

/// Sends `signal` to the process group `leader` leads, which a session leader does too. The
/// caller holds `leader` unreaped, or knows the group still has a member, so the group id is still
/// its own.
pub(crate) fn signal_group(leader: ProcessId, signal: Signal) -> Sent {
    send(-raw(leader), signal)
}

/// Whether the process group `leader` leads has a member other than the leader, or `None` when
/// the system cannot list its processes. A process found in that group is the leader's when the
/// caller holds `leader` unreaped, or knows the group has kept a member ever since the leader was
/// reaped; a caller asking only after the reap accepts that an emptied group whose id was taken
/// over within that gap answers for the newcomer.
pub(crate) fn group_has_members(leader: ProcessId) -> Option<bool> {
    let leader = raw(leader);
    // SAFETY: `getpgid` only reads; a pid that is gone in the meantime reports an error.
    Some(
        pids::all()?
            .into_iter()
            .any(|pid| pid != leader && unsafe { libc::getpgid(pid) } == leader),
    )
}

/// Kills every process in the session `leader` leads, other than the leader and the processes
/// `keep` names. The caller holds `leader` unreaped, so a process found in that session is the leader's; it may still be
/// running, but then a process it forks after the sweep is missed, so a leader is best swept
/// once it has exited.
fn kill_members(leader: ProcessId, keep: &[ProcessIdentity]) -> Swept {
    let leader = raw(leader);
    // A process killed in one pass may have forked just before it died, so passes repeat until
    // one signals no process it had not already signalled. A killed process can stay in the list
    // as a zombie, which is why those are remembered.
    const MAX_PASSES: usize = 3;
    let mut signalled = std::collections::HashSet::new();
    for _ in 0..MAX_PASSES {
        let Some(candidates) = pids::all() else {
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
            if keep.iter().any(|kept| {
                kept.pid.0 as libc::pid_t == pid
                    && proc_table::start_of(pid) == Some(kept.start_micros)
            }) {
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

/// How many bytes of arguments a caller hands one program it starts, with room to spare below
/// the system's limit on what an `exec` carries (arguments and environment together, 256 KiB on
/// macOS and more on Linux).
pub(crate) const ARGUMENT_BYTES: usize = 96 * 1024;

/// Whether `pid` is a process the host runs to give others a console: none on Unix, where a
/// terminal is the kernel's.
pub(crate) fn is_console_host(_pid: ProcessId) -> bool {
    false
}

/// Whether the process `id` is attached to a console that ends with the process `leader`: never on
/// Unix, where a terminal outlives the process that leads its session, and what sits in an agent's
/// own terminal session is never listed to be kept.
pub(crate) fn shares_console(_leader: &ProcessIdentity, _id: &ProcessIdentity) -> Option<bool> {
    Some(false)
}

/// How often [`watch_parent`] looks for its process.
const PARENT_POLL: std::time::Duration = std::time::Duration::from_secs(1);

/// Calls `on_gone` once the process `pid` no longer exists, polling for it. The caller is in the
/// runtime: the watch is a task of it.
pub(crate) fn watch_parent(pid: u32, on_gone: impl FnOnce() + Send + 'static) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(PARENT_POLL).await;
            // SAFETY: signal 0 only probes whether the pid exists.
            if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0
                && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                on_gone();
                break;
            }
        }
    });
}

/// Resolves when the host asks the daemon to end: Ctrl-C, SIGTERM, or SIGHUP, which a closing
/// terminal sends it.
pub(crate) async fn shutdown_requested() {
    use tokio::signal::unix::{signal, SignalKind};

    // A signal whose handler cannot be installed never arrives, so the others still end the daemon.
    let install = |kind: SignalKind, name: &str| match signal(kind) {
        Ok(stream) => Some(stream),
        Err(err) => {
            tracing::warn!(%err, "installing the {name} handler failed");
            None
        }
    };
    let mut sigterm = install(SignalKind::terminate(), "SIGTERM");
    let mut sighup = install(SignalKind::hangup(), "SIGHUP");
    async fn arrival(stream: &mut Option<tokio::signal::unix::Signal>) {
        match stream {
            Some(stream) => {
                stream.recv().await;
            }
            None => std::future::pending().await,
        }
    }
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = arrival(&mut sigterm) => {}
        _ = arrival(&mut sighup) => {}
    }
}
