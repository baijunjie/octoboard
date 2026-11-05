//! Ending every process the daemon runs on a PTY — each agent, and each shell terminal's shell —
//! when the daemon goes down on an internal failure, and only then. Those processes lead sessions
//! of their own, out of reach of anything that ends the daemon's own group; a failure the daemon
//! survives must leave them running. Which processes are
//! registered, and when they leave, is for the callers to decide.
//!
//! The kill runs from a panic hook and from an abort handler, so nothing on its path may take a
//! lock or allocate: a signal can interrupt a thread holding either. The running processes are
//! therefore kept in a fixed table of atomics rather than a collection behind a lock.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread::ThreadId;

use super::ChildHandle;

#[cfg(test)]
mod tests;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
use unix::*;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows::*;

/// How many processes the table holds at once; one past that runs uncovered, with a warning. Far
/// above the number of agents and shells one machine runs side by side.
const CAPACITY: usize = 1024;

/// Each slot is empty (0) or holds what the host's kill needs of one running process, an agent or
/// a shell, and whether what it started is killed too: never 0 (see the host's `slot_value`).
static SLOTS: [AtomicU64; CAPACITY] = [const { AtomicU64::new(0) }; CAPACITY];

/// One process's place in the table, freed when it is dropped.
///
/// The kill reaches the process by what its slot holds, which is only safe while the process is
/// still the one registered: on Unix while the child is unreaped, its pid and process group then
/// reserved. So the holder must drop the entry once the child has exited and before it reaps it.
pub struct Entry {
    index: usize,
}

/// Takes a slot for `child`, or returns `None` when the table is full or the host cannot hold the
/// child in it. With `sweep_session`, the kill reaches everything `child` started, wherever it
/// went, not just what is in its process group.
pub fn register(child: &ChildHandle, sweep_session: bool) -> Option<Entry> {
    let value = slot_value(child, sweep_session)?;
    let taken = SLOTS.iter().enumerate().find_map(|(index, slot)| {
        slot.compare_exchange(0, value, Ordering::AcqRel, Ordering::Relaxed)
            .ok()
            .map(|_| Entry { index })
    });
    if taken.is_none() {
        release(value);
    }
    taken
}

impl Drop for Entry {
    fn drop(&mut self) {
        release(SLOTS[self.index].swap(0, Ordering::AcqRel));
    }
}

/// Whether `pid` holds a place in the table.
#[cfg(test)]
pub fn is_registered(pid: super::ProcessId) -> bool {
    SLOTS
        .iter()
        .any(|slot| holds(slot.load(Ordering::Acquire), pid))
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
/// apart on stable Rust. Every abort, that one included, ends in the host's abort handler where it
/// has one (Unix's `SIGABRT`), which kills every registered process on the way out; where it has
/// none (Windows ends an aborting process at once), what is left is for the next start to end. A
/// panic on any thread is logged through tracing first, so it reaches the log file.
pub fn install() {
    MAIN_THREAD.get_or_init(|| std::thread::current().id());
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if MAIN_THREAD.get() == Some(&std::thread::current().id()) {
            kill_all();
            tracing::error!(%info, "the main thread panicked; every agent and shell process was killed");
        } else {
            // The default hook's report goes to stderr only, which nothing reads once the
            // application is gone; this puts it in the log file too.
            tracing::error!(%info, "a thread panicked");
        }
        default_hook(info);
    }));

    if let Err(err) = install_abort_handler() {
        tracing::warn!(%err, "installing the abort handler failed; an abort will leave agents and shells running");
    }
}
