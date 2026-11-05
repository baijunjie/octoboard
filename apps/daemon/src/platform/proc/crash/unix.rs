//! The Unix kill: the process group of every registered process, its session where asked, and the
//! `SIGABRT` handler that runs it. Listing the processes of a session reads into static buffers,
//! so nothing on the path allocates.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};

use super::SLOTS;
use crate::platform::proc::ChildHandle;

/// Set in a slot whose process has its session killed, helpers in groups of their own included.
/// The rest of the slot is the process's pid.
const SWEEP: u64 = 1 << 32;

/// What a slot holds for `child`: its pid, and whether its session is swept.
pub(super) fn slot_value(child: &ChildHandle, sweep_session: bool) -> Option<u64> {
    Some(u64::from(child.pid().0) | if sweep_session { SWEEP } else { 0 })
}

/// Gives up what a slot held: nothing, for a pid.
pub(super) fn release(_value: u64) {}

/// Whether a slot holding `value` holds `pid`.
#[cfg(test)]
pub(super) fn holds(value: u64, pid: crate::platform::proc::ProcessId) -> bool {
    value & !SWEEP == u64::from(pid.0)
}

/// Kills every registered process outright: its process group, and for a slot that asks for it
/// the rest of its session. Safe from a signal handler in practice: it reads atomics and calls
/// plain system calls, and allocates nothing.
pub(super) fn kill_all() {
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
/// One pass over one listing: the daemon is going down, and a process that forks while it is killed
/// is not chased. The listing is its own rather than `crate::platform::proc_table::pids`'s because that one
/// allocates.
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
    let pids = unsafe { &(&*pids)[..(filled.max(0) as usize).min(65536)] };
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
        let bytes = unsafe { &(&(*buf).0)[..filled as usize] };
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

/// Has `SIGABRT` kill every registered process on the way out, which is what every abort raises.
pub(super) fn install_abort_handler() -> std::io::Result<()> {
    // SAFETY: a zeroed `sigaction` is a valid empty one, and the handler only calls functions that
    // are async-signal-safe.
    let installed = unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_abort as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESETHAND;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGABRT, &action, std::ptr::null_mut()) == 0
    };
    if installed {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
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
