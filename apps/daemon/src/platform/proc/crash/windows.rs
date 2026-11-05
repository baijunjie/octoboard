//! The Windows kill: every registered process by a handle of the table's own, and for a slot that
//! asks for it the rest of its session: what in the process's reach, its job, is on its console,
//! one by one. What left the console, started detached or on a console of its own, is left
//! running, as a tree a launch keeps or carries over to another must be: nothing can leave a job.
//! There is no abort handler: an aborting process on Windows ends at once, and what it leaves is
//! ended by the next start of the daemon. Listing a job reads into a static buffer, so nothing on
//! the path allocates.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    IsProcessInJob, JobObjectBasicProcessIdList, QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE,
};

use super::super::{console_of, Console};
use super::SLOTS;
use crate::platform::proc::ChildHandle;

/// The exit code a killed process is given.
const KILLED: u32 = 1;

/// A copy of `handle` of the table's own, or `None` when it cannot be made. Windows keeps a
/// handle's value within 32 bits, so the copy fits half a slot.
fn duplicate(handle: HANDLE) -> Option<u64> {
    let mut copy: HANDLE = std::ptr::null_mut();
    // SAFETY: both handles are open for the call; the copy is closed by `release`.
    let made = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut copy,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    (made != 0 && !copy.is_null()).then_some(copy as usize as u64 & 0xFFFF_FFFF)
}

/// The handle in a half of a slot.
fn handle(half: u64) -> HANDLE {
    half as usize as HANDLE
}

/// What a slot holds for `child`: a handle on the process in the low half, and for a sweep a
/// handle on its reach in the high half. Copies of the child's own, so that they stay open
/// however long the child's handle lives, until [`release`] closes them.
pub(super) fn slot_value(child: &ChildHandle, sweep_session: bool) -> Option<u64> {
    let process = duplicate(child.process())?;
    if !sweep_session {
        return Some(process);
    }
    match duplicate(child.reach().raw()) {
        Some(job) => Some(process | job << 32),
        None => {
            release(process);
            None
        }
    }
}

/// Closes the handles a slot held.
pub(super) fn release(value: u64) {
    for half in [value & 0xFFFF_FFFF, value >> 32] {
        if half != 0 {
            // SAFETY: a handle `slot_value` made and nothing else closes.
            unsafe { CloseHandle(handle(half)) };
        }
    }
}

/// Whether a slot holding `value` holds the process `pid`.
#[cfg(test)]
pub(super) fn holds(value: u64, pid: crate::platform::proc::ProcessId) -> bool {
    // SAFETY: a plain query of a handle the slot holds.
    value != 0
        && unsafe {
            windows_sys::Win32::System::Threading::GetProcessId(handle(value & 0xFFFF_FFFF))
        } == pid.0
}

/// What listing a job reads into: a `JOBOBJECT_BASIC_PROCESS_ID_LIST` with room for far more
/// processes than one launch runs. Handed out by [`SCRATCH_CLAIMED`].
#[repr(C)]
struct PidList {
    assigned: u32,
    listed: u32,
    pids: [usize; 4096],
}

struct Scratch(UnsafeCell<PidList>);

// SAFETY: only the holder of `SCRATCH_CLAIMED` touches the contents.
unsafe impl Sync for Scratch {}

static SCRATCH: Scratch = Scratch(UnsafeCell::new(PidList {
    assigned: 0,
    listed: 0,
    pids: [0; 4096],
}));

/// Whether something is using [`SCRATCH`]: a second kill from another thread does without the
/// sweeps and keeps to the processes themselves.
static SCRATCH_CLAIMED: AtomicBool = AtomicBool::new(false);

/// Kills every registered process outright, and for a slot that asks for it what is in its reach
/// on its console. It reads atomics and calls plain system calls, and allocates nothing.
///
/// Another thread can free a slot, closing its handles, between the read of the slot and the
/// calls here; the value may then name a closed handle, or one reused for another object, which
/// the calls refuse unless it is a process handle again. A window that narrow is accepted, as on
/// Unix.
pub(super) fn kill_all() {
    let claimed = !SCRATCH_CLAIMED.swap(true, Ordering::AcqRel);
    for slot in &SLOTS {
        let value = slot.load(Ordering::Acquire);
        if value == 0 {
            continue;
        }
        let process = handle(value & 0xFFFF_FFFF);
        let job = value >> 32;
        // Read before the kill, though the system still answers it for a process that has ended.
        let console = (job != 0 && claimed).then(|| console_of(process));
        // SAFETY: a handle the slot holds.
        unsafe { TerminateProcess(process, KILLED) };
        // A process on no console, or on one the system does not name, is swept no further.
        if let Some(Console::Host(host)) = console {
            kill_members(handle(job), host);
        }
    }
    if claimed {
        SCRATCH_CLAIMED.store(false, Ordering::Release);
    }
}

/// Kills every process in `job` attached to the console the host with pid `host` serves, opened by
/// the pid the job lists and checked to be in the job still. One pass: the daemon is going down,
/// and a process that starts another while it is killed is not chased. The caller holds
/// `SCRATCH_CLAIMED`.
fn kill_members(job: HANDLE, host: u32) {
    let list = SCRATCH.0.get();
    // SAFETY: the caller holds `SCRATCH_CLAIMED`, so nothing else reads or writes the list, which
    // is writable for the size given.
    let read = unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicProcessIdList,
            list.cast(),
            std::mem::size_of::<PidList>() as u32,
            std::ptr::null_mut(),
        )
    };
    if read == 0 {
        return;
    }
    // SAFETY: as above; the call filled `listed` entries.
    let pids = unsafe { &(&*list).pids[..((*list).listed as usize).min(4096)] };
    for &pid in pids {
        // SAFETY: plain calls on a handle opened here and closed below.
        unsafe {
            let process = OpenProcess(
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid as u32,
            );
            if process.is_null() {
                continue;
            }
            let mut member = 0;
            if IsProcessInJob(process, job, &mut member) != 0
                && member != 0
                && console_of(process) == Console::Host(host)
            {
                TerminateProcess(process, KILLED);
            }
            CloseHandle(process);
        }
    }
}

/// Nothing to install: an abort on Windows ends the process without running anything of its own.
pub(super) fn install_abort_handler() -> std::io::Result<()> {
    Ok(())
}
