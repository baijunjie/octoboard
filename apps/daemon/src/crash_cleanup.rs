//! Ending every process the daemon runs on a PTY — each agent, and each shell terminal's shell —
//! when the daemon goes down on an internal failure, and only then. `portable-pty` calls
//! `setsid()`, so those processes are not in the daemon's process group and nothing else would
//! reach them; a failure the daemon survives must leave them running.
//!
//! The kill runs from a panic hook and from a `SIGABRT` handler, so nothing on its path may take a
//! lock or allocate: a signal can interrupt a thread holding either. The running processes are
//! therefore kept in a fixed table of atomics rather than a collection behind a lock, and listing
//! the processes of a session reads into static buffers.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread::ThreadId;

/// How many processes the table holds at once; one past that runs uncovered, with a warning. Far
/// above the number of agents and shells one machine runs side by side.
const CAPACITY: usize = 1024;

/// Each slot is empty (0) or holds the pid of one running process, an agent or a shell, in its low
/// 32 bits, and in [`SWEEP`] whether its whole session is killed. A pid is never 0.
static SLOTS: [AtomicU64; CAPACITY] = [const { AtomicU64::new(0) }; CAPACITY];

/// Set in a slot whose process has its session killed, helpers in groups of their own included.
const SWEEP: u64 = 1 << 32;

/// One process's place in the table, freed when it is dropped.
///
/// The kill signals the pid by number, which is only safe while the child is unreaped: until then
/// its pid and process group stay reserved. So the holder must drop the entry once the child has
/// exited and before it reaps it.
pub struct Entry {
    index: usize,
}

/// Takes a slot for `pid`, or returns `None` when the table is full. With `sweep_session`, the
/// kill reaches every process in the session `pid` leads, not just its process group.
pub fn register(pid: u32, sweep_session: bool) -> Option<Entry> {
    let value = u64::from(pid) | if sweep_session { SWEEP } else { 0 };
    SLOTS.iter().enumerate().find_map(|(index, slot)| {
        slot.compare_exchange(0, value, Ordering::AcqRel, Ordering::Relaxed)
            .ok()
            .map(|_| Entry { index })
    })
}

impl Drop for Entry {
    fn drop(&mut self) {
        SLOTS[self.index].store(0, Ordering::Release);
    }
}

/// Whether `pid` holds a place in the table.
#[cfg(test)]
pub fn is_registered(pid: u32) -> bool {
    SLOTS
        .iter()
        .any(|slot| slot.load(Ordering::Acquire) & !SWEEP == u64::from(pid))
}

/// Kills every registered process outright: its process group, and for a slot that asks for it
/// the rest of its session. Safe from a signal handler in practice: it reads atomics and calls
/// plain system calls, and allocates nothing.
fn kill_all() {
    let mut sweeping = false;
    for slot in &SLOTS {
        let value = slot.load(Ordering::Acquire);
        let pid = value & !SWEEP;
        if pid != 0 {
            sweeping |= value & SWEEP != 0;
            // The process's pgid is its own pid (`setsid()`). What it started without moving it
            // is in that same group; a shell's jobs each have a group of their own and are not
            // killed here, but the daemon going down closes the PTY they are on, which hangs it
            // up.
            // SAFETY: plain `kill(2)` on a child that was unreaped when its pid was read (see
            // `Entry`). Another thread can free the slot and reap the child between that read and
            // this call, and the pid be reused in that window; a window that narrow is accepted.
            unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };
        }
    }
    if sweeping {
        kill_sessions();
    }
}

/// What listing the system's processes reads into, so that doing it allocates nothing. Handed out
/// by [`SCRATCH_CLAIMED`].
struct Scratch {
    /// `proc_listallpids` fills this; its length bounds the processes seen, far above what a
    /// machine runs.
    #[cfg(target_os = "macos")]
    pids: UnsafeCell<[libc::pid_t; 65536]>,
    /// `getdents64` of `/proc` fills this; aligned for the entries' 8-byte fields.
    #[cfg(target_os = "linux")]
    entries: UnsafeCell<Aligned>,
}

#[cfg(target_os = "linux")]
#[repr(align(8))]
struct Aligned([u8; 32 * 1024]);

// SAFETY: only the holder of `SCRATCH_CLAIMED` touches the contents.
unsafe impl Sync for Scratch {}

static SCRATCH: Scratch = Scratch {
    #[cfg(target_os = "macos")]
    pids: UnsafeCell::new([0; 65536]),
    #[cfg(target_os = "linux")]
    entries: UnsafeCell::new(Aligned([0; 32 * 1024])),
};

/// Whether something is using [`SCRATCH`]: a second kill, from another thread or from a signal
/// that interrupted the first, does without the sweep and keeps to the group kills.
static SCRATCH_CLAIMED: AtomicBool = AtomicBool::new(false);

/// Kills every process in the session of a registered process that asks for it, the process
/// itself included. This reaches helpers that moved to a process group of their own, which the
/// group kill misses; one that called `setsid()` has left the session and is out of reach. The
/// session id is that of an unreaped process, so no other process can have it (see `Entry`).
///
/// One pass over one listing: the daemon is going down, and a process that forks while it is
/// killed is not chased. The listing is its own rather than the one in `crate::pty` because that
/// one allocates.
fn kill_sessions() {
    if SCRATCH_CLAIMED.swap(true, Ordering::AcqRel) {
        return;
    }
    for_each_process(|pid| {
        // SAFETY: `getsid` only reads; a pid that is gone in the meantime reports an error.
        let sid = unsafe { libc::getsid(pid) };
        if sid > 0 && leads_a_swept_session(sid) {
            // SAFETY: the process is in the session of an unreaped leader, as above.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    });
    SCRATCH_CLAIMED.store(false, Ordering::Release);
}

/// Whether a registered process that has its session killed has the pid `sid`.
fn leads_a_swept_session(sid: libc::pid_t) -> bool {
    SLOTS.iter().any(|slot| {
        let value = slot.load(Ordering::Acquire);
        value & SWEEP != 0 && value & !SWEEP == sid as u64
    })
}

/// Calls `visit` with the pid of every process there is, reading into [`SCRATCH`], which the
/// caller has claimed. `proc_listallpids` and `getdents64` are plain system calls.
#[cfg(target_os = "macos")]
fn for_each_process(mut visit: impl FnMut(libc::pid_t)) {
    let pids = SCRATCH.pids.get();
    // SAFETY: the caller holds `SCRATCH_CLAIMED`, so nothing else reads or writes the buffer, and
    // it is writable for the size given.
    let filled = unsafe {
        libc::proc_listallpids(
            pids.cast(),
            std::mem::size_of::<[libc::pid_t; 65536]>() as i32,
        )
    };
    // SAFETY: as above; the call filled `filled` entries.
    let pids = unsafe { &(*pids)[..(filled.max(0) as usize).min(65536)] };
    // Pid 0 is the kernel, which `getsid(0)` would answer for the caller instead.
    pids.iter()
        .filter(|&&pid| pid > 0)
        .for_each(|&pid| visit(pid));
}

/// As the macOS one, from the numeric entries of `/proc`.
#[cfg(target_os = "linux")]
fn for_each_process(mut visit: impl FnMut(libc::pid_t)) {
    // Offsets in a `linux_dirent64`: `d_reclen` after the 8-byte `d_ino` and `d_off`, then
    // `d_type`, then the NUL-terminated `d_name`.
    const RECLEN: usize = 16;
    const NAME: usize = 19;
    let buf = SCRATCH.entries.get();
    // SAFETY: a plain `open(2)` of a constant NUL-terminated path.
    let dir = unsafe {
        libc::open(
            c"/proc".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if dir < 0 {
        return;
    }
    loop {
        // SAFETY: the caller holds `SCRATCH_CLAIMED`, so the buffer is ours, and it is writable
        // for the length given.
        let filled =
            unsafe { libc::syscall(libc::SYS_getdents64, dir, buf.cast::<u8>(), 32 * 1024usize) };
        if filled <= 0 {
            break;
        }
        // SAFETY: as above; the call filled `filled` bytes.
        let bytes = unsafe { &(*buf).0[..filled as usize] };
        let mut at = 0;
        while at + NAME < bytes.len() {
            let reclen = u16::from_ne_bytes([bytes[at + RECLEN], bytes[at + RECLEN + 1]]) as usize;
            // A record never ends before its name starts; anything shorter would make the name
            // slice panic, here in a signal handler.
            if reclen < NAME {
                break;
            }
            let name = &bytes[at + NAME..(at + reclen).min(bytes.len())];
            let digits = name.iter().take_while(|&&byte| byte != 0);
            let mut pid: libc::pid_t = 0;
            let mut numeric = name.first().is_some_and(|&byte| byte != 0);
            for &byte in digits {
                if !byte.is_ascii_digit() {
                    numeric = false;
                    break;
                }
                pid = pid
                    .saturating_mul(10)
                    .saturating_add(libc::pid_t::from(byte - b'0'));
            }
            if numeric && pid > 0 {
                visit(pid);
            }
            at += reclen;
        }
    }
    // SAFETY: closing the descriptor opened above.
    unsafe { libc::close(dir) };
}

/// The thread [`install`] ran on: the daemon's main thread.
static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Installs the kill on the daemon's way down. Call it from the daemon's main thread.
///
/// A panic unwinds (the daemon keeps the default panic strategy), so whether it is fatal depends on
/// the thread. On the main thread nothing catches it: it unwinds out of `main` and ends the
/// process, so the hook kills every registered process first, before anything is dropped. Anywhere
/// else it is caught where it happens — tokio hands a task's panic to its `JoinHandle`, a plain
/// thread just ends — and the daemon carries on, and so do its agents and shells. A panic that
/// cannot unwind (one raised by a destructor while another is unwinding, or in a function that
/// cannot unwind) aborts the process from whatever thread raised it, and the hook cannot tell it
/// apart on stable Rust. Every abort, that one included, raises `SIGABRT`, whose handler kills
/// every registered process on the way out.
pub fn install() {
    MAIN_THREAD.get_or_init(|| std::thread::current().id());
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if MAIN_THREAD.get() == Some(&std::thread::current().id()) {
            kill_all();
            tracing::error!(%info, "the main thread panicked; every agent and shell process was killed");
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
        tracing::warn!(%err, "installing the SIGABRT handler failed; an abort will leave agents and shells running");
    }
}

/// Kills every registered process, then ends the daemon with the signal's default action, which
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
        assert_gone_by, fake_live_session, is_running, read_pid_file, KillOnDrop, ScratchDir,
        PATIENCE,
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
    /// It has a helper in a process group of its own, which only a registration that asks for the
    /// session to be killed reaches; the `_shell` case registers without that, as a shell
    /// terminal does, and the helper survives.
    #[test]
    fn agents_are_killed_when_the_daemon_goes_down_and_only_then() {
        for (case, goes_down) in [
            ("recoverable", false),
            ("main_thread", true),
            ("abort", true),
            ("signal", true),
            ("signal_shell", true),
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
            let helper = read_pid_file(&dir.join("helper"))
                .unwrap_or_else(|| panic!("{case}: no helper pid; child said {output:?}"));
            let mut helper_guard = KillOnDrop(vec![helper]);
            // The child got as far as its case: a clean exit, a failed test, or a `SIGABRT`.
            let reached = match case {
                "recoverable" => output.status.success(),
                "main_thread" => output.status.code() == Some(101),
                _ => output.status.signal() == Some(libc::SIGABRT),
            };
            assert!(reached, "{case}: unexpected end: {output:?}");

            if goes_down {
                assert_gone_by(pid, Instant::now() + PATIENCE);
                if case.ends_with("_shell") {
                    assert!(is_running(helper), "{case}: the helper was killed");
                } else {
                    assert_gone_by(helper, Instant::now() + PATIENCE);
                    helper_guard.0.clear();
                }
            } else {
                assert!(is_running(helper), "{case}: the helper was killed");
                let alive = is_running(pid);
                // SAFETY: the stand-in's own group; nothing reaps it before this signal lands.
                unsafe { libc::kill(-pid, libc::SIGKILL) };
                assert!(alive, "{case}: the agent was killed");
                assert_gone_by(pid, Instant::now() + PATIENCE);
            }
        }
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
        let helper_file = Path::new(&pid_file).with_file_name("helper");
        let helper_file = helper_file.display();
        super::install();
        // Outlives `PATIENCE`, so a stand-in still there when the parent looks was left running
        // rather than not yet killed.
        let stand_in = fake_live_session(
            "crash-cleanup",
            Agent::Claude,
            80,
            24,
            &format!(
                "trap '' HUP; set -m; sleep 600 & echo $! > '{helper_file}'; set +m; \
                 echo $$ > '{pid_file}'; exec sleep 600"
            ),
        );
        // The pid is written after the trap is set, so from here the stand-in ignores `SIGHUP`.
        let deadline = Instant::now() + PATIENCE;
        while read_pid_file(Path::new(&pid_file)).is_none() {
            assert!(Instant::now() < deadline, "the stand-in never started");
            std::thread::sleep(Duration::from_millis(20));
        }
        match case.strip_suffix("_shell") {
            // Registered as a shell terminal is: its group is killed, its session is not.
            Some(_) => std::mem::forget(super::register(stand_in.pid, false)),
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
