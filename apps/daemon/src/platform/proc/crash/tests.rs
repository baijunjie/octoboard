use std::path::Path;
use std::time::{Duration, Instant};

use crate::protocol::AgentId;
use crate::test_support::{
    assert_gone_by, is_running, read_pid_file, KillOnDrop, ScratchDir, PATIENCE,
};

/// Where [`crash_cleanup_child`] reads its case and writes its stand-in's pid.
const CASE_VAR: &str = "OCTOBOARDD_TEST_CRASH_CASE";
const PID_FILE_VAR: &str = "OCTOBOARDD_TEST_CRASH_PID_FILE";

/// What a case leaves running once the child process has ended.
struct Left {
    agent: bool,
    helper: bool,
    /// On Windows, whether the helper on the stand-in's console is still running when the child
    /// has just ended.
    #[cfg(windows)]
    on_console: bool,
}

/// A panic the daemon survives leaves its agents running; going down by a panic unwinding out
/// of the main thread, by an abort, or by a `SIGABRT` from outside kills them first. Each case
/// runs in a process of its own, since the cleanup is process-wide and would kill every other
/// test's registered agents.
///
/// On Unix the stand-in ignores `SIGHUP`, so the end of the process holding its PTY does not end
/// it too: the case where it survives that is what makes its death in the others the cleanup's.
/// It has a helper in a process group of its own, which only a registration that asks for the
/// session to be killed reaches; the `_shell` case registers without that, as a shell terminal
/// does, and the helper survives. The `SIGABRT` cases are Unix's alone.
///
/// On Windows a process's pseudoconsoles close when it ends, whichever way, and the processes
/// attached to them end with them: the stand-in never outlives the child, so the child checks
/// itself that the stand-in outlived a recoverable panic. The stand-in has two helpers there. One
/// is on its console and holds on when the console closes, which the host ends only after a grace
/// of about five seconds: still running when the child has just ended, unless the cleanup's sweep
/// of the stand-in's session, what is on its console, ended it when the main thread panicked. The
/// other was started apart from the stand-in, so it has left the session, as a tree a launch keeps
/// or carries over has: it is in the stand-in's job, which nothing can leave, and survives every
/// case. Nothing is cleaned up on an abort, which ends a Windows process without running anything
/// of its own.
#[test]
fn agents_are_killed_when_the_daemon_goes_down_and_only_then() {
    let cases: &[(&str, Left)] = &[
        #[cfg(unix)]
        (
            "recoverable",
            Left {
                agent: true,
                helper: true,
            },
        ),
        #[cfg(windows)]
        (
            "recoverable",
            Left {
                agent: false,
                helper: true,
                on_console: true,
            },
        ),
        (
            "main_thread",
            Left {
                agent: false,
                helper: cfg!(windows),
                #[cfg(windows)]
                on_console: false,
            },
        ),
        (
            "abort",
            Left {
                agent: false,
                helper: cfg!(windows),
                #[cfg(windows)]
                on_console: true,
            },
        ),
        #[cfg(unix)]
        (
            "signal",
            Left {
                agent: false,
                helper: false,
            },
        ),
        #[cfg(unix)]
        (
            "signal_shell",
            Left {
                agent: false,
                helper: true,
            },
        ),
    ];
    for (case, left) in cases {
        let case = *case;
        let dir = ScratchDir::new("crash-cleanup");
        let pid_file = dir.join("pid");
        let output = std::process::Command::new(std::env::current_exe().expect("the binary"))
            .args([
                "--exact",
                "platform::proc::crash::tests::crash_cleanup_child",
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
        let helper = read_pid_file(&dir.join("helper"))
            .unwrap_or_else(|| panic!("{case}: no helper pid; child said {output:?}"));
        let mut helper_guard = KillOnDrop(vec![helper]);
        // The child got as far as its case: a clean exit, a failed test, or a `SIGABRT`.
        let reached = match case {
            "recoverable" => output.status.success(),
            "main_thread" => output.status.code() == Some(101),
            _ => aborted(&output.status),
        };
        assert!(reached, "{case}: unexpected end: {output:?}");
        // At once: the host ends the helper on the console only after its grace.
        #[cfg(windows)]
        {
            let on_console = read_pid_file(&dir.join("on-console"))
                .unwrap_or_else(|| panic!("{case}: no pid of the helper on the console"));
            let _on_console_guard = KillOnDrop(vec![on_console]);
            assert_eq!(
                is_running(on_console),
                left.on_console,
                "{case}: whether the helper on the stand-in's console was left running"
            );
        }

        if left.helper {
            assert!(is_running(helper), "{case}: the helper was killed");
        } else {
            assert_gone_by(helper, Instant::now() + PATIENCE);
            helper_guard.0.clear();
        }
        if left.agent {
            let alive = is_running(pid);
            kill_stand_in(pid);
            assert!(alive, "{case}: the agent was killed");
        }
        assert_gone_by(pid, Instant::now() + PATIENCE);
    }
}

/// Whether the child ended by an abort.
#[cfg(unix)]
fn aborted(status: &std::process::ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    status.signal() == Some(libc::SIGABRT)
}

/// Whether the child ended by an abort: Rust's abort on Windows is a fail-fast, which ends the
/// process with `STATUS_STACK_BUFFER_OVERRUN`.
#[cfg(windows)]
fn aborted(status: &std::process::ExitStatus) -> bool {
    status.code() == Some(0xC000_0409_u32 as i32)
}

/// Kills the stand-in that survived a recoverable panic, together with what runs in its group.
fn kill_stand_in(pid: i32) {
    // SAFETY: the stand-in's own group; nothing reaps it before this signal lands.
    #[cfg(unix)]
    unsafe {
        libc::kill(-pid, libc::SIGKILL)
    };
    #[cfg(windows)]
    crate::test_support::kill(pid);
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

    let (Ok(case), Ok(pid_file)) = (std::env::var(CASE_VAR), std::env::var(PID_FILE_VAR)) else {
        return;
    };
    let helper_file = Path::new(&pid_file).with_file_name("helper");
    super::install();
    let stand_in = stand_in(&pid_file, &helper_file);
    // The pid is written last, once the helper's is: on Unix after the trap is set, so from here
    // the stand-in ignores `SIGHUP`.
    let deadline = Instant::now() + PATIENCE;
    while read_pid_file(Path::new(&pid_file)).is_none() {
        assert!(Instant::now() < deadline, "the stand-in never started");
        std::thread::sleep(Duration::from_millis(20));
    }
    match case.strip_suffix("_shell") {
        // Registered as a shell terminal is: its group is killed, its session is not.
        Some(_) => std::mem::forget(super::register(stand_in.handle(), false)),
        None => stand_in.enter_crash_cleanup(),
    }
    // Left running when this process ends, whichever way it does: the parent checks on it.
    std::mem::forget(stand_in);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    match case.trim_end_matches("_shell") {
        "recoverable" => {
            let joined = runtime.block_on(runtime.spawn_blocking(|| {
                panic!("a blocking task panics");
            }));
            assert!(joined.is_err());
            #[cfg(windows)]
            {
                let stand_in = read_pid_file(Path::new(&pid_file)).expect("the stand-in's pid");
                let helper = read_pid_file(&helper_file).expect("the helper's pid");
                let on_console = read_pid_file(&helper_file.with_file_name("on-console"))
                    .expect("the pid of the helper on the console");
                assert!(
                    is_running(stand_in) && is_running(helper) && is_running(on_console),
                    "a recoverable panic ended the agent"
                );
            }
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
        #[cfg(unix)]
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

/// The stand-in agent of [`crash_cleanup_child`], with a helper in a process group of its own,
/// whose pid it writes to `helper_file` before it writes its own to `pid_file`. It ignores
/// `SIGHUP` and outlives `PATIENCE`, so a stand-in still there when the parent looks was left
/// running rather than not yet killed.
#[cfg(unix)]
fn stand_in(pid_file: &str, helper_file: &Path) -> crate::test_support::StandIn {
    let helper_file = helper_file.display();
    crate::test_support::fake_live_session(
        "crash-cleanup",
        AgentId::CLAUDE,
        80,
        24,
        &format!(
            "trap '' HUP; set -m; sleep 600 & echo $! > '{helper_file}'; set +m; \
             echo $$ > '{pid_file}'; exec sleep 600"
        ),
    )
}

/// The stand-in agent of [`crash_cleanup_child`]: a fixture process (`test_support::fixture`) with
/// a helper on its console, which holds on when the console closes and whose pid it writes beside
/// `helper_file` as `on-console`, and a helper started apart from it, whose pid it writes to
/// `helper_file`, before it writes its own to `pid_file`.
#[cfg(windows)]
fn stand_in(pid_file: &str, helper_file: &Path) -> crate::test_support::StandIn {
    use crate::test_support::fixture::Steps;
    let on_console = helper_file.with_file_name("on-console");
    let steps = Steps::new()
        .spawn_on_console(
            "on-console",
            Steps::new()
                .hold_on_close()
                .write_pid(&on_console)
                .sleep(600),
        )
        .wait_for(&on_console)
        .spawn("apart", Steps::new().sleep(600))
        .write_child_pid(helper_file)
        .write_pid(Path::new(pid_file))
        .sleep(600);
    crate::test_support::fake_live_session_running(
        "crash-cleanup",
        AgentId::CLAUDE,
        80,
        24,
        &steps.program("agent"),
    )
}
