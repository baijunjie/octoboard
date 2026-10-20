//! The agent CLIs the daemon runs under an account's directory outside a session — reading the
//! account's plan quota, listing its models, signing it in — which a change of the account's
//! sharing must not move files under: a Codex starting there opens its state database and
//! history. A change waits for the reads running to end (it stays pending, carried out at a later
//! moment), and a read that starts while a change is being carried out waits for the change.
//!
//! Kept for the whole process rather than per daemon state: the reads run from blocking code
//! that has no state at hand, and an account directory is the same directory whichever state
//! asks.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Condvar, LazyLock, Mutex};

use crate::account_dirs::account_dir_key;

#[derive(Default)]
struct DirUse {
    reads: usize,
    changing: bool,
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

/// A read running under an account's directory, until dropped.
pub(crate) struct Reading(String);

impl Drop for Reading {
    fn drop(&mut self) {
        let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
        if let Some(entry) = dirs.get_mut(&self.0) {
            entry.reads -= 1;
            if entry.reads == 0 && !entry.changing {
                dirs.remove(&self.0);
            }
        }
        REGISTRY.changed.notify_all();
    }
}

/// Marks a run of an agent CLI under `dir`, an account's directory (`None`, the default
/// account's, is never changed by sharing), for as long as the answer lives; waits first while a
/// change of the account's sharing is being carried out there. Blocking.
pub(crate) fn reading_account(dir: Option<&Path>) -> Option<Reading> {
    let key = key(dir?);
    let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
    while dirs.get(&key).is_some_and(|entry| entry.changing) {
        dirs = REGISTRY
            .changed
            .wait(dirs)
            .expect("cli reads lock poisoned");
    }
    dirs.entry(key.clone()).or_default().reads += 1;
    Some(Reading(key))
}

/// Whether a read is running under `dir`.
pub(super) fn is_reading(dir: &Path) -> bool {
    REGISTRY
        .dirs
        .lock()
        .expect("cli reads lock poisoned")
        .get(&key(dir))
        .is_some_and(|entry| entry.reads > 0)
}

/// A change of sharing being carried out on an account's directory, until dropped.
pub(super) struct Changing(String);

impl Drop for Changing {
    fn drop(&mut self) {
        let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
        if let Some(entry) = dirs.get_mut(&self.0) {
            entry.changing = false;
            if entry.reads == 0 {
                dirs.remove(&self.0);
            }
        }
        REGISTRY.changed.notify_all();
    }
}

/// Claims `dir` for carrying out a change, unless a read is running there.
pub(super) fn begin_change(dir: &Path) -> Option<Changing> {
    let key = key(dir);
    let mut dirs = REGISTRY.dirs.lock().expect("cli reads lock poisoned");
    let entry = dirs.entry(key.clone()).or_default();
    if entry.reads > 0 {
        return None;
    }
    entry.changing = true;
    Some(Changing(key))
}
