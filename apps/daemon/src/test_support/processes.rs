//! Waits on the processes a test spawned (the pid a fixture recorded, its being gone, and its
//! detaching), and [`within`], a condition poll run off the async runtime.

use super::PATIENCE;
use std::path::Path;
#[cfg(unix)]
use std::process::Command;
use std::time::{Duration, Instant};

/// The pid `path` holds, if the fixture shell has written one. Read once, without waiting:
/// the shell writes it within moments of starting, and the tests that read it do so after a
/// timeout of seconds.
pub(crate) fn read_pid_file(path: &std::path::Path) -> Option<i32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Waits up to [`PATIENCE`] for `path` to hold a pid, as a fixture writes it whole (see
/// [`perl_write`]).
pub(crate) fn wait_for_pid(path: &Path) -> i32 {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(pid) = read_pid_file(path) {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Perl that writes `$value` (a Perl expression: `$$`, a child's pid) to `path` whole, through a
/// rename, so [`wait_for_pid`] never reads half of it.
#[cfg(unix)]
pub(crate) fn perl_write(path: &Path, value: &str) -> String {
    let path = path.display();
    format!(
        "open(my $f, \">\", \"{path}.tmp\"); print $f {value}; close $f; rename \"{path}.tmp\", \"{path}\";"
    )
}

/// Children of the test process, killed and reaped when dropped.
pub(crate) struct Children(pub Vec<std::process::Child>);

impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
            child.kill().ok();
            child.wait().ok();
        }
    }
}

/// Perl that runs its arguments as a process of their own, outside any process of the test's:
/// in a terminal session of its own, its parent gone at once. It prints that process's pid. The
/// process ledger never attributes a direct child of the daemon — the test process, here — nor
/// lists one as left behind, so a fixture standing for an agent's leftover is started this way.
#[cfg(unix)]
const DETACH: &str = "my $c = fork(); if ($c == 0) { POSIX::setsid(); \
    open(STDIN, '<', '/dev/null'); open(STDOUT, '>', '/dev/null'); open(STDERR, '>', '/dev/null'); \
    exec @ARGV or POSIX::_exit(1) } print \"$c\\n\";";

/// Runs `program` with `args` detached (see [`DETACH`]) in `cwd`, with `env` set or, for `None`,
/// removed, and answers its pid. The caller kills it.
#[cfg(unix)]
pub(crate) fn detached_in(
    cwd: &Path,
    program: &str,
    args: &[&str],
    env: &[(&str, Option<&str>)],
) -> i32 {
    let mut command = Command::new("/usr/bin/perl");
    command
        .args(["-MPOSIX", "-e", DETACH, "--", program])
        .args(args)
        .current_dir(cwd);
    for (key, value) in env {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    let output = command.output().expect("perl ran");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("the detached process's pid")
}

/// A `sleep` of `seconds` working in `cwd`, detached as an agent's leftover is.
pub(crate) fn detached_sleep_in(cwd: &Path, seconds: u32) -> i32 {
    #[cfg(unix)]
    return detached_in(cwd, "/bin/sleep", &[&seconds.to_string()], &[]);
    #[cfg(windows)]
    return super::fixture::detached_sleep(cwd, seconds);
}

/// [`detached_in`] the test process's own directory.
#[cfg(unix)]
pub(crate) fn detached(program: &str, args: &[&str], env: &[(&str, Option<&str>)]) -> i32 {
    detached_in(
        &std::env::current_dir().expect("the current directory"),
        program,
        args,
        env,
    )
}

/// Runs `attempt` until it returns a value, for up to [`PATIENCE`]. An attempt returns `None`
/// when its fixture shell was killed by the timeout under test before it got as far as
/// recording the pids the test checks: when an exec stalls for longer than that timeout, as it
/// can on a machine running endpoint-security software, nothing was tested, so the scenario is
/// run again from scratch instead.
pub(crate) fn until_the_shell_ran<T>(mut attempt: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(done) = attempt() {
            return done;
        }
        assert!(
            Instant::now() < deadline,
            "the fixture shell was killed before it recorded its pids, again and again"
        );
    }
}

/// Bash, for a test's scripts and shells. On Windows, Git for Windows' bash, which has the tools
/// the scripts use (`perl` among them) at their Unix paths; the pids it reports for `$$` and `$!`
/// are its own, not Windows'.
pub(crate) fn bash() -> std::path::PathBuf {
    #[cfg(unix)]
    return "/bin/bash".into();
    #[cfg(windows)]
    return crate::platform::shell::command_runner(crate::platform::shell::Dialect::Posix).program;
}

/// Shell text that expands to the pid of the [`bash`] running it as the host numbers processes:
/// `$$`, or under Git Bash the Windows pid behind its own.
pub(crate) const SHELL_PID: &str = if cfg!(windows) {
    "$(cat /proc/$$/winpid)"
} else {
    "$$"
};

/// Ends `pid` at once, without a chance to clean up; a pid already gone is no error.
pub(crate) fn kill(pid: i32) {
    host::kill(pid);
}

/// A process the test holds from now on, so that the code it ended with can still be read once it
/// has: what tells the daemon's kill, which ends a process with 1, from the close of its console,
/// which ends it with `STATUS_CONTROL_C_EXIT`.
#[cfg(windows)]
pub(crate) struct Watched(host::Held);

#[cfg(windows)]
impl Watched {
    /// Holds `pid`, which must still be running.
    pub(crate) fn new(pid: i32) -> Self {
        Self(host::Held::new(pid))
    }

    /// The code the process ended with, waiting up to [`PATIENCE`] for it to end; `None` while it
    /// still runs.
    pub(crate) fn exit_code(&self) -> Option<u32> {
        self.0.exit_code(PATIENCE)
    }
}

/// Kills the pids it holds when dropped, so a failing test leaves nothing running.
pub(crate) struct KillOnDrop(pub Vec<i32>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        // The guard is cleared once its pids are observed gone, so a pid that may have been reused
        // is killed only when the test is already failing.
        for &pid in &self.0 {
            kill(pid);
        }
    }
}

/// Whether `pid` is a running process rather than gone or a zombie, which `kill(pid, 0)` alone
/// cannot tell apart.
pub(crate) fn is_running(pid: i32) -> bool {
    host::is_running(pid)
}

/// The start of a [`bash`] `-c` script that stands in for an agent CLI moving a background task
/// into a process group of its own: job control starts a helper in one, and its pid is written to
/// `pid_file` (whole, through a rename). The helper keeps none of the script's stdio open, so how
/// the run ends is up to the script. Followed by `; ` and the rest of the script. On Windows the
/// stand-in is a fixture process (`fixture`), whose helper is on its console.
#[cfg(unix)]
pub(crate) fn helper_in_its_own_group(pid_file: &Path) -> String {
    format!(
        "set -m; /bin/sleep 300 </dev/null >/dev/null 2>&1 & echo $! > '{file}.tmp'; \
         mv '{file}.tmp' '{file}'; set +m",
        file = pid_file.display()
    )
}

/// Waits up to [`PATIENCE`] for `pid` to stop running (gone or a zombie), failing the test if it
/// does not.
pub(crate) fn assert_stops_running(pid: i32) {
    let deadline = Instant::now() + PATIENCE;
    while is_running(pid) {
        assert!(Instant::now() < deadline, "pid {pid} kept running");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for `pid` to no longer exist, failing the test if it outlives `deadline`. Used to
/// confirm a process the fixture backgrounded was actually reaped rather than leaked, whether
/// by the production code's own group kill or by a test cleaning up after a successful
/// snapshot that (correctly) never touched it.
pub(crate) fn assert_gone_by(pid: i32, deadline: Instant) {
    while host::exists(pid) {
        if Instant::now() >= deadline {
            panic!("pid {pid} outlived the test's deadline");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Polls `done` until it holds or `timeout` runs out, and says which. The poll runs off the async
/// runtime, so a task the test is waiting on — a record watch, say — keeps running meanwhile.
pub(crate) async fn within(timeout: Duration, done: impl FnMut() -> bool + Send + 'static) -> bool {
    tokio::task::spawn_blocking(move || crate::session::wait::wait_for(timeout, done))
        .await
        .expect("the wait ran")
}

#[cfg(unix)]
mod host {
    use std::process::Command;

    pub(super) fn kill(pid: i32) {
        // SAFETY: a plain signal to a pid of the test's own; a pid gone is answered with an error
        // that is ignored.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }

    pub(super) fn is_running(pid: i32) -> bool {
        let ps = Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps ran");
        let stat = String::from_utf8_lossy(&ps.stdout);
        !stat.trim().is_empty() && !stat.trim_start().starts_with('Z')
    }

    /// Whether `pid` still exists, a zombie included.
    pub(super) fn exists(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the pid exists.
        let signalled = unsafe { libc::kill(pid, 0) };
        signalled == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
}

#[cfg(windows)]
mod host {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, TerminateProcess, WaitForSingleObject,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };

    /// Runs `with` on a handle to `pid` opened for `access`, or answers `None` where it cannot be
    /// opened (most often: the process is gone).
    fn with_process<T>(pid: i32, access: u32, with: impl FnOnce(HANDLE) -> T) -> Option<T> {
        // SAFETY: `OpenProcess` takes plain values; a non-null handle is closed below.
        let handle = unsafe { OpenProcess(access, 0, pid as u32) };
        if handle.is_null() {
            return None;
        }
        let result = with(handle);
        // SAFETY: the handle was opened above and is closed once.
        unsafe { CloseHandle(handle) };
        Some(result)
    }

    pub(super) fn kill(pid: i32) {
        // SAFETY: the handle is open for termination for the call.
        with_process(pid, PROCESS_TERMINATE, |handle| unsafe {
            TerminateProcess(handle, 1)
        });
    }

    pub(super) fn is_running(pid: i32) -> bool {
        with_process(pid, PROCESS_QUERY_LIMITED_INFORMATION, |handle| {
            let mut code = 0u32;
            // SAFETY: the handle is open for the query, and `code` outlives the call.
            let read = unsafe { GetExitCodeProcess(handle, &mut code) };
            read != 0 && code == STILL_ACTIVE as u32
        })
        .unwrap_or(false)
    }

    /// Whether `pid` still exists: Windows keeps no zombie, so a process that exited is gone.
    pub(super) fn exists(pid: i32) -> bool {
        is_running(pid)
    }

    /// A handle on a process, which keeps its exit code readable.
    pub(super) struct Held(std::os::windows::io::OwnedHandle);

    impl Held {
        pub(super) fn new(pid: i32) -> Self {
            use std::os::windows::io::FromRawHandle;
            // SAFETY: `OpenProcess` takes plain values; the handle is checked and owned here.
            let handle = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    pid as u32,
                )
            };
            assert!(!handle.is_null(), "pid {pid} is not running");
            // SAFETY: a handle just opened, owned by nothing else.
            Self(unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle) })
        }

        pub(super) fn exit_code(&self, within: std::time::Duration) -> Option<u32> {
            use std::os::windows::io::AsRawHandle;
            let handle = self.0.as_raw_handle();
            let millis = u32::try_from(within.as_millis()).unwrap_or(u32::MAX - 1);
            // SAFETY: the handle is open for synchronisation.
            if unsafe { WaitForSingleObject(handle, millis) } != WAIT_OBJECT_0 {
                return None;
            }
            let mut code = 0u32;
            // SAFETY: the handle is open for the query, and `code` outlives the call.
            (unsafe { GetExitCodeProcess(handle, &mut code) } != 0).then_some(code)
        }
    }
}

/// Asserts that the process `pid`, which was started by the process `parent`, has been
/// reparented, as a double fork leaves it, and leads a session of its own.
#[cfg(target_os = "linux")]
pub(crate) fn assert_detached_session_leader(pid: i32, parent: u32) {
    use crate::platform::proc::ContainerId;

    let info = crate::platform::proc_table::list()
        .expect("the process table is listed")
        .into_iter()
        .find(|info| info.pid == pid)
        .expect("the process runs");
    assert_ne!(info.ppid, parent as i32, "the process was reparented");
    assert_eq!(
        info.sid,
        Some(ContainerId(pid as u32)),
        "it leads its own session"
    );
}
