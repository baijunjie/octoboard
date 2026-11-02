//! What the daemon holds in memory about each repository's branch origins: the gate serializing
//! every write to its rows, and when the last write happened. Held on `AppState`; none of it is
//! persisted.
//!
//! [`KeyedGates`], the keyed mutex the gate is made with, is shared with the branch cleanup.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

/// One mutex per key, made when first asked for and dropped once nobody holds or waits on it.
/// It guards an order rather than any state, so a panic inside a gate poisons nothing worth
/// keeping: a poisoned gate is taken all the same, since refusing it would leave its key
/// unusable for the rest of the daemon's life.
pub struct KeyedGates<K> {
    gates: Mutex<HashMap<K, Arc<Mutex<()>>>>,
}

impl<K> Default for KeyedGates<K> {
    fn default() -> Self {
        Self {
            gates: Mutex::default(),
        }
    }
}

impl<K: Eq + Hash> KeyedGates<K> {
    fn gates(&self) -> MutexGuard<'_, HashMap<K, Arc<Mutex<()>>>> {
        self.gates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs `work` holding the gate of `key`. A panic inside `work` skips the removal, so the
    /// entry stays until the key's next use.
    pub fn serialized<Q, T>(&self, key: &Q, work: impl FnOnce() -> T) -> T
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ToOwned<Owned = K> + ?Sized,
    {
        let gate = Arc::clone(self.gates().entry(key.to_owned()).or_default());
        let done = {
            let _held = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            work()
        };
        let mut gates = self.gates();
        // The map's own handle and this one: nobody else holds or waits on it. Taking a handle
        // happens under the map's lock too, so a gate removed here is one nobody can still be
        // about to lock, and two callers never hold different gates for one key. This handle is
        // dropped under the lock as well, so two holders finishing together cannot each still
        // see the other's and both leave the entry behind.
        if Arc::strong_count(&gate) == 2 {
            gates.remove(key);
        }
        drop(gate);
        drop(gates);
        done
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.gates().is_empty()
    }
}

#[derive(Default)]
pub struct Gates {
    /// Serializes each repository's writes; see [`Gates::serialized`].
    gates: KeyedGates<PathBuf>,
    /// When the last write that added rows ran, by repository; see [`Gates::wrote_at`].
    wrote_at: Mutex<HashMap<PathBuf, Instant>>,
}

impl Gates {
    fn wrote_ats(&self) -> MutexGuard<'_, HashMap<PathBuf, Instant>> {
        self.wrote_at.lock().expect("branch origin gates poisoned")
    }

    /// Runs `work` holding the gate that serializes every write to one repository's rows. A
    /// report, a listing's reconcile and a project's deletion each read the repository's rows,
    /// decide from that snapshot and then write, so two of them at once lose an attribution or
    /// delete a row the other just wrote; several clients can be listing at once, and reports
    /// come from background processes in any number.
    pub(super) fn serialized<T>(&self, repo: &Path, work: impl FnOnce() -> T) -> T {
        self.gates.serialized(repo, work)
    }

    /// When rows were last added to the repository. A branch listing taken before then is no
    /// evidence about which branches exist now, however complete it is, since the branch that
    /// write recorded cannot be in it.
    ///
    /// Only the order of the two matters, so both are [`Instant`]s: a wall clock stepped back —
    /// by NTP, or by the user — would otherwise put every listing before the last write for as
    /// long as the step, which stops a row ever being removed for a deleted branch.
    pub(super) fn wrote_at(&self, repo: &Path) -> Option<Instant> {
        self.wrote_ats().get(repo).copied()
    }

    pub(super) fn wrote(&self, repo: &Path, at: Instant) {
        self.wrote_ats().insert(repo.to_path_buf(), at);
    }

    pub(super) fn forget(&self, repo: &Path) {
        self.wrote_ats().remove(repo);
    }

    #[cfg(test)]
    pub(super) fn holds_no_repository(&self) -> bool {
        self.gates.is_empty() && self.wrote_ats().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn an_idle_gate_is_dropped() {
        let gates = KeyedGates::<PathBuf>::default();
        assert_eq!(gates.serialized(Path::new("/repo"), || 7), 7);
        assert!(gates.is_empty());
    }

    #[test]
    fn callers_of_one_key_never_hold_it_together_while_idle_gates_come_and_go() {
        let gates = Arc::new(KeyedGates::<PathBuf>::default());
        let inside = Arc::new(AtomicUsize::new(0));
        let overlapped = Arc::new(AtomicUsize::new(0));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let (gates, inside, overlapped) =
                    (gates.clone(), inside.clone(), overlapped.clone());
                std::thread::spawn(move || {
                    for _ in 0..200 {
                        gates.serialized(Path::new("/repo"), || {
                            if inside.fetch_add(1, Ordering::SeqCst) > 0 {
                                overlapped.fetch_add(1, Ordering::SeqCst);
                            }
                            std::thread::yield_now();
                            inside.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(overlapped.load(Ordering::SeqCst), 0);
        assert!(gates.is_empty());
    }

    #[test]
    fn a_panic_in_a_gate_leaves_it_usable() {
        let gates = Arc::new(KeyedGates::<PathBuf>::default());
        let panicking = gates.clone();
        let outcome = std::thread::spawn(move || {
            panicking.serialized(Path::new("/repo"), || panic!("inside the gate"))
        })
        .join();
        assert!(outcome.is_err());
        assert_eq!(gates.serialized(Path::new("/repo"), || 1), 1);
    }
}
