//! The agent CLIs the daemon runs under an account's directory outside a session, which a change of
//! the account's sharing, or the move of its directory to the Trash when the account is removed,
//! must not move files under: a Codex starting there opens its state database and history. They are
//! of two kinds. A short read — the account's plan quota, its models — ends within seconds and a
//! kick (one model request) within a minute, so a change or move holds new ones back and waits for
//! those running to end. A long run — signing the account in — lasts as long as the user takes, so
//! a change or move is refused while one runs. A run of either kind that starts while a change or
//! move holds the directory waits for it.
//!
//! Kept for the whole process rather than per daemon state: the runs start from blocking code
//! that has no state at hand, and an account directory is the same directory whichever state
//! asks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, LazyLock, Mutex};

use crate::account_dirs::account_dir_key;

#[derive(Default)]
struct DirUse {
    reads: usize,
    long_runs: usize,
    /// Changes and moves holding the directory; several can hold one at once: an account's removal
    /// holds it on top of the hold kept by turning its sharing off, and accounts of different
    /// agents may be pointed at the same directory.
    holds: usize,
}

impl DirUse {
    fn is_unused(&self) -> bool {
        self.reads == 0 && self.long_runs == 0 && self.holds == 0
    }
}

#[derive(Default)]
struct Registry {
    dirs: Mutex<HashMap<String, DirUse>>,
    changed: Condvar,
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::default);

fn key(dir: &Path) -> String {
    account_dir_key(&crate::hostfs::lexically_normalise(dir).to_string_lossy())
}

/// Updates `key`'s entry, dropping it once nothing uses the directory, and wakes the waiters.
fn release(key: &str, update: impl FnOnce(&mut DirUse)) {
    let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
    if let Some(entry) = dirs.get_mut(key) {
        update(entry);
        if entry.is_unused() {
            dirs.remove(key);
        }
    }
    REGISTRY.changed.notify_all();
}

/// A run of an agent CLI under an account's directory, until dropped.
pub(crate) struct Run {
    key: String,
    long: bool,
}

impl Drop for Run {
    fn drop(&mut self) {
        let long = self.long;
        release(&self.key, |entry| {
            if long {
                entry.long_runs -= 1;
            } else {
                entry.reads -= 1;
            }
        });
    }
}

/// Waits while a change holds `dir`, then counts a run there. Blocking.
fn start(dir: Option<&Path>, long: bool) -> Option<Run> {
    let key = key(dir?);
    let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
    while dirs.get(&key).is_some_and(|entry| entry.holds > 0) {
        dirs = REGISTRY
            .changed
            .wait(dirs)
            .expect("cli reads lock poisoned");
    }
    let entry = dirs.entry(key.clone()).or_default();
    if long {
        entry.long_runs += 1;
    } else {
        entry.reads += 1;
    }
    Some(Run { key, long })
}

/// Marks a short read under `dir`, an account's directory (`None`, the default account's, is
/// never changed by sharing), for as long as the answer lives. Blocking.
pub(crate) fn reading_account(dir: Option<&Path>) -> Option<Run> {
    start(dir, false)
}

/// An account's directory that is gone.
pub(crate) struct Missing(pub PathBuf);

/// As [`reading_account`], but refused when `dir` is gone once the read is admitted: a removal of
/// the account may have moved the directory while the read waited, and the CLI would create it
/// again. Blocking.
pub(crate) fn admitted_reading(dir: Option<&Path>) -> Result<Option<Run>, Missing> {
    let run = reading_account(dir);
    match dir.filter(|dir| is_missing(dir)) {
        Some(dir) => Err(Missing(dir.to_path_buf())),
        None => Ok(run),
    }
}

/// Whether `dir` is gone: only `NotFound` counts; a path that cannot be looked at, or a file in
/// its place, is not.
pub(crate) fn is_missing(dir: &Path) -> bool {
    matches!(std::fs::metadata(dir), Err(err) if err.kind() == std::io::ErrorKind::NotFound)
}

/// Marks a long run under `dir` (a sign-in), as [`reading_account`] does a short read. Blocking.
pub(crate) fn running_long(dir: Option<&Path>) -> Option<Run> {
    start(dir, true)
}

/// Whether a long run is running under `dir`.
pub(super) fn is_running_long(dir: &Path) -> bool {
    REGISTRY
        .dirs
        .lock()
        .expect("cli reads lock poisoned")
        .get(&key(dir))
        .is_some_and(|entry| entry.long_runs > 0)
}

/// A change of sharing, or an account removal's move of its directory, holding the directory
/// until dropped: no run starts there.
pub(crate) struct Hold(String);

impl Hold {
    /// What waits for the short reads running under the held directory to end.
    pub fn waiter(&self) -> Waiter {
        Waiter(self.0.clone())
    }
}

pub(crate) struct Waiter(String);

impl Waiter {
    /// Waits for the short reads running under the directory to end. Blocking.
    pub fn wait_for_reads(self) {
        let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
        while dirs.get(&self.0).is_some_and(|entry| entry.reads > 0) {
            dirs = REGISTRY
                .changed
                .wait(dirs)
                .expect("cli reads lock poisoned");
        }
    }
}

/// Whether `dir` is held.
#[cfg(test)]
pub(crate) fn is_held(dir: &Path) -> bool {
    REGISTRY
        .dirs
        .lock()
        .expect("cli reads lock poisoned")
        .get(&key(dir))
        .is_some_and(|entry| entry.holds > 0)
}

impl Drop for Hold {
    fn drop(&mut self) {
        release(&self.0, |entry| entry.holds -= 1);
    }
}

/// Holds `dir` for a change or move, unless a long run is running there.
pub(crate) fn hold(dir: &Path) -> Option<Hold> {
    let key = key(dir);
    let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
    let entry = dirs.entry(key.clone()).or_default();
    if entry.long_runs > 0 {
        return None;
    }
    entry.holds += 1;
    Some(Hold(key))
}
