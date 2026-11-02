//! Waits on the processes a test spawned (the pid a fixture recorded, its being gone, and its
//! detaching), and [`within`], a condition poll run off the async runtime.

use super::PATIENCE;
use std::path::Path;
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
const DETACH: &str = "my $c = fork(); if ($c == 0) { POSIX::setsid(); \
    open(STDIN, '<', '/dev/null'); open(STDOUT, '>', '/dev/null'); open(STDERR, '>', '/dev/null'); \
    exec @ARGV or POSIX::_exit(1) } print \"$c\\n\";";

/// Runs `program` with `args` detached (see [`DETACH`]) in `cwd`, with `env` set or, for `None`,
/// removed, and answers its pid. The caller kills it.
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

/// [`detached_in`] the test process's own directory.
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

fn errno_is_esrch() -> bool {
    std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Kills the pids it holds when dropped, so a failing test leaves nothing running.
pub(crate) struct KillOnDrop(pub Vec<libc::pid_t>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        for &pid in &self.0 {
            // SAFETY: a test's own helpers. The guard is cleared once they are observed gone,
            // so a pid that may have been reused is signalled only when the test is already
            // failing.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
}

/// Whether `pid` is a running process rather than gone or a zombie, which `kill(pid, 0)` alone
/// cannot tell apart.
pub(crate) fn is_running(pid: i32) -> bool {
    let ps = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("ps ran");
    let stat = String::from_utf8_lossy(&ps.stdout);
    !stat.trim().is_empty() && !stat.trim_start().starts_with('Z')
}

/// The start of a `/bin/bash -c` script that stands in for an agent CLI moving a background task
/// into a process group of its own: job control starts a helper in one, and its pid is written to
/// `pid_file` (whole, through a rename). The helper keeps none of the script's stdio open, so how
/// the run ends is up to the script. Followed by `; ` and the rest of the script.
pub(crate) fn helper_in_its_own_group(pid_file: &Path) -> String {
    format!(
        "set -m; /bin/sleep 300 </dev/null >/dev/null 2>&1 & echo $! > '{file}.tmp'; \
         mv '{file}.tmp' '{file}'; set +m",
        file = pid_file.display()
    )
}

/// Waits up to [`PATIENCE`] for `pid` to stop running (gone or a zombie), failing the test if it
/// does not.
pub(crate) fn assert_stops_running(pid: libc::pid_t) {
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
    loop {
        if unsafe { libc::kill(pid, 0) } == -1 && errno_is_esrch() {
            return;
        }
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
