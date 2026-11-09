use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;

use super::cgroup::{Group, Subtree};

use super::{
    tag_value, Polite, ProcessId, ProcessIdentity, Sent, SetEnv, Signal, Swept, Unreaped,
    TAG_VARIABLE,
};
use crate::platform::proc_table::{self, pids};

/// What tells one launch's processes from any other's: the tag every one of them carries in its
/// environment, which a process inherits from its parent, and on Linux, for an agent launch where
/// the host lets the daemon make one, a cgroup v2 group they are all in ([`Self::for_launch`]).
/// Without a group the host lists no members, so what carries the tag is found by reading the
/// environments of the processes it lists.
#[derive(Debug, Clone)]
pub struct Containment {
    tag: String,
    group: Option<Arc<Group>>,
}

impl Containment {
    /// The containment of the launch `launch` of the daemon instance `instance`: its tag alone.
    pub fn create(instance: &str, launch: &str) -> io::Result<Self> {
        Ok(Self {
            tag: tag_value(instance, launch),
            group: None,
        })
    }

    /// The containment of the agent launch `launch` of the daemon instance `instance`: as
    /// [`Self::create`], and on Linux, where the cgroup the calling process is in lets it make
    /// one, a group of its own below that cgroup, which [`Self::end`] kills and removes. Where it
    /// does not, the launch goes without, which is not an error.
    pub fn for_launch(instance: &str, launch: &str) -> io::Result<Self> {
        Ok(Self {
            group: Subtree::below_own()
                .and_then(|base| Group::create(&base, instance, launch))
                .map(Arc::new),
            ..Self::create(instance, launch)?
        })
    }

    /// Puts what makes the processes started with `environment` the launch's into it.
    pub fn prepare(&self, environment: &mut impl SetEnv) {
        environment.set_env(TAG_VARIABLE, &self.tag);
    }

    /// The processes the host says are in the containment, or `None` when it cannot say, and the
    /// callers find them by their environment.
    pub fn members(&self) -> Option<Vec<ProcessId>> {
        self.group.as_ref()?.members()
    }

    /// Whether processes can be moved out of the containment, as [`Self::release`] does: where it
    /// has a group, and not for a tag alone.
    pub fn releases_members(&self) -> bool {
        self.group.is_some()
    }

    /// The containment of the launch `launch` of the daemon instance `instance`, which another
    /// process made, for a process that starts the launch's program on its behalf (a holder): its
    /// tag, and the group that process made for it where this process sees it below its own
    /// cgroup, as one started from that process does. The group is left to its maker to end and
    /// remove; without it, the launch's processes started here are told by their tag alone.
    pub fn join_launch(instance: &str, launch: &str) -> io::Result<Self> {
        Ok(Self {
            group: Subtree::below_own()
                .and_then(|base| Group::join(&base, instance, launch))
                .map(Arc::new),
            ..Self::create(instance, launch)?
        })
    }

    /// The containment of a launch of an earlier run of the daemon, found again by its name, with
    /// its group while that has not been removed.
    pub fn reopen(instance: &str, launch: &str) -> Option<Self> {
        Some(Self {
            tag: tag_value(instance, launch),
            group: Subtree::below_own()
                .and_then(|base| Group::open(&base, instance, launch))
                .map(Arc::new),
        })
    }

    /// The launches of the daemon instance `instance` whose containments are still there from an
    /// earlier run, where the host can list them: the groups left below the calling process's own
    /// cgroup.
    pub fn leftovers(instance: &str) -> Vec<String> {
        Subtree::below_own().map_or_else(Vec::new, |base| Group::leftovers(&base, instance))
    }

    /// Moves the processes `ids` names that are still running into the containment's group, from
    /// wherever they are; without a group, nothing is done. A process that cannot be moved is
    /// left where it is.
    pub fn adopt(&self, ids: &[ProcessIdentity]) {
        if let Some(group) = &self.group {
            group.adopt_running(ids);
        }
    }

    /// Moves those of the processes `ids` names that are in the containment's group out of it, so
    /// that its end does not reach them: into `into`'s group, or without one into the cgroup the
    /// containment's group is in. Answers whether none of them is left in the group.
    pub fn release(&self, ids: &[ProcessIdentity], into: Option<&Containment>) -> bool {
        self.group
            .as_ref()
            .is_none_or(|group| group.release(ids, into.and_then(|into| into.group.as_deref())))
    }

    /// Kills whatever is still in the containment's group and removes the group, other than the
    /// processes `spared` names, which are moved first, each into the group of the containment it
    /// names or into the cgroup its group is in, with the children they started that are still in
    /// the group; when one of them cannot be moved, nothing is killed. Without a group,
    /// nothing is done. Answers whether no group is left. **Blocks**, for a moment.
    pub fn end(&self, spared: &[(ProcessIdentity, Option<&Containment>)]) -> bool {
        let Some(group) = &self.group else {
            return true;
        };
        let spared: Vec<(ProcessIdentity, Option<&Group>)> = spared
            .iter()
            .map(|(id, into)| (*id, into.and_then(|into| into.group.as_deref())))
            .collect();
        group.end(&spared)
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
    /// environment the child was started with; where the containment has a group, the child is
    /// moved into it now, with what it has started already. Nothing here hooks the child before
    /// its program runs, so a process it forked and that moved on before that is the ledger's to
    /// find; a move that fails is logged and leaves the child to the tag, never failing the start.
    pub fn started(
        pid: ProcessId,
        _reach: super::Reach,
        containment: Option<&Containment>,
    ) -> io::Result<Self> {
        if let Some(group) = containment.and_then(|containment| containment.group.as_ref()) {
            if let Err(err) = group.adopt_child(pid) {
                tracing::warn!(pid = pid.0, %err, "moving a process into its launch's group failed; it is followed by its tag");
            }
        }
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

/// Starts `command` as a process that outlives the daemon, with no stdio, in a session of its own
/// (so a signal to the daemon's process group never reaches it), and reaps it on another thread
/// while the daemon runs. `application` is the desktop application's pid, which Windows may need
/// to name; there is nothing to do with it here.
pub(crate) fn start_detached(mut command: Command, _application: Option<u32>) -> io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    lead_session(&mut command);
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        child.wait().ok();
    });
    Ok(())
}

/// Has `command` start in a session of its own with no controlling terminal, for the daemon the
/// command line starts to keep running after the login that started it has closed.
pub(crate) fn run_in_background(command: &mut Command) -> io::Result<()> {
    lead_session(command);
    Ok(())
}

/// Whether [`run_in_background`] can start a daemon here: always on Unix.
pub(crate) fn runs_in_background() -> bool {
    true
}

/// Whether a service manager restarts this daemon on the status a planned restart ends it with:
/// `asked` (`--service-managed`, which a unit that restarts the daemon on that status passes), or
/// the environment showing this process to be the main process of a systemd service.
pub(crate) fn restarted_by_service_manager(asked: bool) -> bool {
    let invocation = std::env::var("INVOCATION_ID").ok();
    let exec_pid = std::env::var("SYSTEMD_EXEC_PID").ok();
    restart_is_managed(
        asked,
        invocation.as_deref(),
        exec_pid.as_deref(),
        std::process::id(),
    )
}

/// [`restarted_by_service_manager`] over the values it reads: `INVOCATION_ID`, `SYSTEMD_EXEC_PID`
/// and the process's own pid. systemd gives every process it starts for a unit `INVOCATION_ID` and,
/// from version 248, `SYSTEMD_EXEC_PID`, the pid of the process it started. A process that only
/// inherited them, such as a daemon started by hand from a shell a terminal emulator's service
/// runs, has another pid than the one named, so it is not the service; where systemd names no pid
/// there is no evidence either way. Both have to be present and well formed, the pid equal to
/// `own_pid`.
fn restart_is_managed(
    asked: bool,
    invocation: Option<&str>,
    exec_pid: Option<&str>,
    own_pid: u32,
) -> bool {
    let well_formed = invocation.is_some_and(|id| {
        !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    asked
        || (well_formed && exec_pid.and_then(|pid| pid.trim().parse::<u32>().ok()) == Some(own_pid))
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
    send_number(target, number(signal))
}

fn send_number(target: libc::pid_t, number: libc::c_int) -> Sent {
    // SAFETY: a plain signal; what makes `target` still the process or group meant is the
    // caller's, as the callers of the public functions state.
    if unsafe { libc::kill(target, number) } == 0 {
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

/// Sends `signal` to the process `id` names, if it still runs: the start time is read right
/// before the signal, which goes by pid alone, so a pid passed to another process is not signalled
/// but for the moment between the read and the signal.
pub(crate) fn signal_identified(id: &ProcessIdentity, signal: Signal) -> Sent {
    if !id.is_running() {
        return Sent::Gone;
    }
    signal_process(id.pid, signal)
}

/// Asks the daemon `id` names for a planned restart, if it still runs: `SIGUSR1`, which
/// [`restart_requested`] waits for. Its start time is read right before the signal, as for
/// [`signal_identified`]. A process that installed no handler for it is ended by it, so the caller
/// sends it only to a daemon whose owner record says it takes the request.
pub(crate) fn request_restart(id: &ProcessIdentity) -> Sent {
    if !id.is_running() {
        return Sent::Gone;
    }
    send_number(raw(id.pid), libc::SIGUSR1)
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
/// terminal sends it. The handlers are installed by this call, not by the first poll of what it
/// returns, so a request made after it returns is not lost to the default action.
pub(crate) fn shutdown_requested() -> impl std::future::Future<Output = ()> {
    use tokio::signal::unix::{signal, SignalKind};

    // A signal whose handler cannot be installed never arrives, so the others still end the daemon.
    let install = |kind: SignalKind, name: &str| match signal(kind) {
        Ok(stream) => Some(stream),
        Err(err) => {
            tracing::warn!(%err, "installing the {name} handler failed");
            None
        }
    };
    let mut sigint = install(SignalKind::interrupt(), "SIGINT");
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
    async move {
        tokio::select! {
            _ = arrival(&mut sigint) => {}
            _ = arrival(&mut sigterm) => {}
            _ = arrival(&mut sighup) => {}
        }
    }
}

/// The planned-restart requests the host delivers to the daemon: `SIGUSR1`, which
/// [`request_restart`] sends.
pub(crate) struct RestartRequests(Option<tokio::signal::unix::Signal>);

impl RestartRequests {
    /// Whether the handler is installed, so that a request is taken rather than ending the daemon.
    pub(crate) fn installed(&self) -> bool {
        self.0.is_some()
    }

    /// Resolves at the next request; never, when the handler could not be installed.
    pub(crate) async fn next(&mut self) {
        match &mut self.0 {
            Some(stream) => {
                if stream.recv().await.is_none() {
                    std::future::pending::<()>().await;
                }
            }
            None => std::future::pending().await,
        }
    }
}

/// Installs the handler of the planned-restart request, so that from this call on the request is
/// taken rather than ending the daemon, which is what `SIGUSR1` does by default. Called before the
/// owner record that tells a `restart` the daemon takes it is written.
pub(crate) fn restart_requested() -> RestartRequests {
    use tokio::signal::unix::{signal, SignalKind};

    match signal(SignalKind::user_defined1()) {
        Ok(stream) => RestartRequests(Some(stream)),
        Err(err) => {
            tracing::warn!(%err, "installing the SIGUSR1 handler failed");
            RestartRequests(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::restart_is_managed;

    fn by_environment(invocation: Option<&str>, exec_pid: Option<&str>, own_pid: u32) -> bool {
        restart_is_managed(false, invocation, exec_pid, own_pid)
    }

    #[test]
    fn no_invocation_id_is_no_service() {
        assert!(!by_environment(None, None, 7));
        assert!(!by_environment(None, Some("7"), 7));
        assert!(!by_environment(Some(""), Some("7"), 7));
    }

    #[test]
    fn the_process_systemd_started_is_the_service() {
        assert!(by_environment(Some("abc"), Some("7"), 7));
        assert!(by_environment(Some("abc"), Some(" 7\n"), 7));
    }

    /// A daemon started by hand from a shell a service runs inherits both variables, with the pid
    /// of that shell.
    #[test]
    fn a_process_that_only_inherited_the_variables_is_not() {
        assert!(!by_environment(Some("abc"), Some("8"), 7));
        assert!(!by_environment(Some("abc"), Some("not a pid"), 7));
        assert!(!by_environment(Some("abc"), Some(""), 7));
    }

    /// Without the pid there is no evidence that this process is the one the unit started, so an
    /// inherited `INVOCATION_ID` alone does not make it a service.
    #[test]
    fn an_invocation_id_without_a_pid_is_not_enough() {
        assert!(!by_environment(Some("abc"), None, 7));
    }

    #[test]
    fn a_malformed_invocation_id_is_no_service() {
        assert!(!by_environment(Some("a b"), Some("7"), 7));
        assert!(!by_environment(Some("abc\n"), Some("7"), 7));
    }

    /// The explicit mode needs no evidence, and no environment withdraws it.
    #[test]
    fn the_explicit_mode_is_managed_whatever_the_environment_says() {
        assert!(restart_is_managed(true, None, None, 7));
        assert!(restart_is_managed(true, Some("abc"), Some("8"), 7));
        assert!(!restart_is_managed(false, None, None, 7));
    }
}
