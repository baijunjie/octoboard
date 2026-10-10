//! Ending every agent process when the daemon goes down on an internal failure, and only then.
//! `portable-pty` calls `setsid()`, so agents are not in the daemon's process group and nothing
//! else would reach them; a failure the daemon survives must leave them running.
//!
//! The kill runs from a panic hook and from a `SIGABRT` handler, so nothing on its path may take a
//! lock or allocate: a signal can interrupt a thread holding either. The running agents are
//! therefore kept in a fixed table of atomics rather than a collection behind a lock.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::thread::ThreadId;

/// How many agents the table holds at once; one past that runs uncovered, with a warning. Far
/// above the number of agents one machine runs side by side.
const CAPACITY: usize = 1024;

/// Each slot is empty (0) or holds the pid of one running agent. A pid is never 0.
static SLOTS: [AtomicU32; CAPACITY] = [const { AtomicU32::new(0) }; CAPACITY];

/// One agent's place in the table, freed when it is dropped.
///
/// The kill signals the pid by number, which is only safe while the child is unreaped: until then
/// its pid and process group stay reserved. So the holder must drop the entry once the child has
/// exited and before it reaps it.
pub struct Entry {
    index: usize,
}

/// Takes a slot for `pid`, or returns `None` when the table is full.
pub fn register(pid: u32) -> Option<Entry> {
    SLOTS.iter().enumerate().find_map(|(index, slot)| {
        slot.compare_exchange(0, pid, Ordering::AcqRel, Ordering::Relaxed)
            .ok()
            .map(|_| Entry { index })
    })
}

impl Drop for Entry {
    fn drop(&mut self) {
        SLOTS[self.index].store(0, Ordering::Release);
    }
}

/// Kills the process group of every registered agent outright. Safe from a signal handler: it
/// reads atomics and calls `kill`, which is async-signal-safe.
fn kill_all() {
    for slot in &SLOTS {
        let pid = slot.load(Ordering::Acquire);
        if pid != 0 {
            // The agent's pgid is its own pid (`setsid()`), and its tool subprocesses are in that
            // same group.
            // SAFETY: plain `kill(2)` on a child that was unreaped when its pid was read (see
            // `Entry`). Another thread can free the slot and reap the child between that read and
            // this call, and the pid be reused in that window; a window that narrow is accepted.
            unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };
        }
    }
}

/// The thread [`install`] ran on: the daemon's main thread.
static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Installs the kill on the daemon's way down. Call it from the daemon's main thread.
///
/// A panic unwinds (the daemon keeps the default panic strategy), so whether it is fatal depends on
/// the thread. On the main thread nothing catches it: it unwinds out of `main` and ends the
/// process, so the hook kills every agent first, before anything is dropped. Anywhere else it is
/// caught where it happens — tokio hands a task's panic to its `JoinHandle`, a plain thread just
/// ends — and the daemon carries on, and so do its agents. A panic that cannot unwind (one raised
/// by a destructor while another is unwinding, or in a function that cannot unwind) aborts the
/// process from whatever thread raised it, and the hook cannot tell it apart on stable Rust. Every
/// abort, that one included, raises `SIGABRT`, whose handler kills every agent on the way out.
pub fn install() {
    MAIN_THREAD.get_or_init(|| std::thread::current().id());
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if MAIN_THREAD.get() == Some(&std::thread::current().id()) {
            kill_all();
            tracing::error!(%info, "the main thread panicked; every agent process was killed");
        }
        default_hook(info);
    }));

    // SAFETY: a zeroed `sigaction` is a valid empty one, and the handler only calls functions that
    // are async-signal-safe.
    let installed = unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_abort as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESETHAND;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGABRT, &action, std::ptr::null_mut()) == 0
    };
    if !installed {
        let err = std::io::Error::last_os_error();
        tracing::warn!(%err, "installing the SIGABRT handler failed; an abort will leave agents running");
    }
}

/// Kills every agent, then ends the process with the signal's default action, which
/// `SA_RESETHAND` restored on entry. Raising it again is what ends the process when the signal
/// came from outside (`kill -ABRT`): returning would carry on as if nothing had happened.
extern "C" fn on_abort(_signal: libc::c_int) {
    kill_all();
    // SAFETY: `raise` is async-signal-safe.
    unsafe { libc::raise(libc::SIGABRT) };
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use crate::protocol::Agent;
    use crate::test_support::{
        assert_gone_by, fake_live_session, read_pid_file, ScratchDir, PATIENCE,
    };

    /// Where [`crash_cleanup_child`] reads its case and writes its stand-in's pid.
    const CASE_VAR: &str = "OCTOBOARDD_TEST_CRASH_CASE";
    const PID_FILE_VAR: &str = "OCTOBOARDD_TEST_CRASH_PID_FILE";

    /// A panic the daemon survives leaves its agents running; going down by a panic unwinding out
    /// of the main thread, by an abort, or by a `SIGABRT` from outside kills them first. Each case
    /// runs in a process of its own, since the cleanup is process-wide and would kill every other
    /// test's registered agents.
    ///
    /// The stand-in ignores `SIGHUP`, so the end of the process holding its PTY does not end it
    /// too: the case where it survives that is what makes its death in the others the cleanup's.
    #[test]
    fn agents_are_killed_when_the_daemon_goes_down_and_only_then() {
        for (case, goes_down) in [
            ("recoverable", false),
            ("main_thread", true),
            ("abort", true),
            ("signal", true),
        ] {
            let dir = ScratchDir::new("crash-cleanup");
            let pid_file = dir.join("pid");
            let output = std::process::Command::new(std::env::current_exe().expect("the binary"))
                .args([
                    "--exact",
                    "crash_cleanup::tests::crash_cleanup_child",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CASE_VAR, case)
                .env(PID_FILE_VAR, &pid_file)
                .output()
                .expect("the child test ran");
            let pid = read_pid_file(&pid_file)
                .unwrap_or_else(|| panic!("{case}: no stand-in pid; child said {output:?}"));
            // The child got as far as its case: a clean exit, a failed test, or a `SIGABRT`.
            let reached = match case {
                "recoverable" => output.status.success(),
                "main_thread" => output.status.code() == Some(101),
                _ => output.status.signal() == Some(libc::SIGABRT),
            };
            assert!(reached, "{case}: unexpected end: {output:?}");

            if goes_down {
                assert_gone_by(pid, Instant::now() + PATIENCE);
            } else {
                let alive = is_running(pid);
                // SAFETY: the stand-in's own group; nothing reaps it before this signal lands.
                unsafe { libc::kill(-pid, libc::SIGKILL) };
                assert!(alive, "{case}: the agent was killed");
                assert_gone_by(pid, Instant::now() + PATIENCE);
            }
        }
    }

    /// Whether `pid` is a running process rather than gone or a zombie, which `kill(pid, 0)` alone
    /// cannot tell apart.
    fn is_running(pid: i32) -> bool {
        let ps = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps ran");
        let stat = String::from_utf8_lossy(&ps.stdout);
        !stat.trim().is_empty() && !stat.trim_start().starts_with('Z')
    }

    /// One case of [`agents_are_killed_when_the_daemon_goes_down_and_only_then`], run by it in a
    /// child process; does nothing when run any other way. The thread running the test stands in
    /// for the daemon's main thread.
    #[test]
    #[ignore = "run in a child process by agents_are_killed_when_the_daemon_goes_down_and_only_then"]
    fn crash_cleanup_child() {
        struct PanicsOnDrop;
        impl Drop for PanicsOnDrop {
            fn drop(&mut self) {
                panic!("a destructor panics while the task unwinds");
            }
        }

        let (Ok(case), Ok(pid_file)) = (std::env::var(CASE_VAR), std::env::var(PID_FILE_VAR))
        else {
            return;
        };
        super::install();
        // Outlives `PATIENCE`, so a stand-in still there when the parent looks was left running
        // rather than not yet killed.
        let stand_in = fake_live_session(
            "crash-cleanup",
            Agent::Claude,
            80,
            24,
            &format!("trap '' HUP; echo $$ > '{pid_file}'; exec sleep 600"),
        );
        // The pid is written after the trap is set, so from here the stand-in ignores `SIGHUP`.
        let deadline = Instant::now() + PATIENCE;
        while read_pid_file(Path::new(&pid_file)).is_none() {
            assert!(Instant::now() < deadline, "the stand-in never started");
            std::thread::sleep(Duration::from_millis(20));
        }
        stand_in.enter_crash_cleanup();
        // Left running when this process ends, whichever way it does: the parent checks on it.
        std::mem::forget(stand_in);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        match case.as_str() {
            "recoverable" => {
                let joined = runtime.block_on(runtime.spawn_blocking(|| {
                    panic!("a blocking task panics");
                }));
                assert!(joined.is_err());
            }
            "main_thread" => panic!("the main thread panics"),
            "abort" => {
                runtime
                    .block_on(runtime.spawn(async {
                        let _guard = PanicsOnDrop;
                        panic!("a task panics");
                    }))
                    .ok();
                unreachable!("the destructor's panic aborts the process");
            }
            "signal" => {
                // Sent to the whole process, as `kill -ABRT` from outside is. Should it not end the
                // process, the test returns and the child exits cleanly, which the parent rejects.
                // SAFETY: signalling this process.
                unsafe { libc::kill(libc::getpid(), libc::SIGABRT) };
                std::thread::sleep(PATIENCE);
            }
            other => panic!("unknown case {other}"),
        }
    }
}
