//! Blocking work that outran its deadline and whose thread is still running.

use std::collections::HashMap;
use std::sync::Mutex;

/// Blocking work that outran its deadline and whose thread is still running, per key — a console
/// whose project directories were being looked at, a path being looked at. Each thread is held
/// weakly: it owns the strong reference, so its entry reads as gone once it has ended. Once a
/// thread has run long enough to count as stuck ([`StalledPasses::record_from`]; at once for
/// [`StalledPasses::record`]), later work for the same key takes its fallback at once instead of
/// starting another thread that would be stuck the same way; before that its entry only keeps its
/// place and the key reads as not stuck. A key can hold several threads, the later ones started
/// while the earlier were not yet stuck: each keeps its own time, so a later record never pushes
/// back when an earlier thread counts as stuck, and the key is stuck while any of them is.
pub struct StalledPasses<K>(Mutex<HashMap<K, Vec<StalledThread>>>);

/// A thread of a [`StalledPasses`] key: its pass, and from when it counts as stuck.
type StalledThread = (std::sync::Weak<()>, std::time::Instant);

impl<K> Default for StalledPasses<K> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

impl<K: Eq + std::hash::Hash> StalledPasses<K> {
    /// Whether work recorded for `key` is still stuck on its thread, and has been long enough to
    /// count as stuck ([`StalledPasses::record_from`]).
    pub fn is_stuck<Q>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + std::hash::Hash + ?Sized,
    {
        let mut passes = self.0.lock().expect("stalled pass lock poisoned");
        let Some(threads) = passes.get_mut(key) else {
            return false;
        };
        threads.retain(|(pass, _)| pass.strong_count() > 0);
        let now = std::time::Instant::now();
        let stuck = threads.iter().any(|(_, from)| now >= *from);
        if threads.is_empty() {
            passes.remove(key);
        }
        stuck
    }

    /// Drops what was recorded for `key`, which is gone: its threads are left to finish on their
    /// own, with nothing waiting on them.
    pub fn forget<Q>(&self, key: &Q)
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + std::hash::Hash + ?Sized,
    {
        self.0
            .lock()
            .expect("stalled pass lock poisoned")
            .remove(key);
    }

    /// Records that the work for `key` outran its deadline; `pass` is held strongly by the thread
    /// that is still running it.
    pub fn record(&self, key: K, pass: std::sync::Weak<()>) {
        self.record_from(key, pass, std::time::Instant::now());
    }

    /// [`StalledPasses::record`], where the work counts as stuck only from `from` on: until then
    /// its thread is running on and later work for the key starts as if nothing were recorded. An
    /// earlier thread of the key that is still running keeps its own entry and time. The entries
    /// of threads that have ended are dropped at the next record of any key, so a key that is
    /// never asked about again stays until then.
    pub fn record_from(&self, key: K, pass: std::sync::Weak<()>, from: std::time::Instant) {
        let mut passes = self.0.lock().expect("stalled pass lock poisoned");
        passes.retain(|_, threads| {
            threads.retain(|(earlier, _)| earlier.strong_count() > 0);
            !threads.is_empty()
        });
        passes.entry(key).or_default().push((pass, from));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A record for a key whose earlier thread is still running keeps the earlier entry and its
    /// time, and the later thread counts too once the earlier has ended.
    #[test]
    fn a_live_stalled_entry_is_not_replaced_by_a_later_record() {
        let passes = StalledPasses::<String>::default();
        let key = "key".to_string();
        let begun = Instant::now();
        // A time no run of this test reaches.
        let later = begun + Duration::from_secs(60);

        let waiting = Arc::new(());
        passes.record_from("not yet".to_string(), Arc::downgrade(&waiting), later);
        assert!(!passes.is_stuck("not yet"), "not stuck before its time");

        // The earlier thread's time is not pushed back by a later record.
        let first = Arc::new(());
        passes.record_from(key.clone(), Arc::downgrade(&first), begun);
        let second = Arc::new(());
        passes.record_from(key.clone(), Arc::downgrade(&second), later);
        assert!(passes.is_stuck(&key), "the first's own time stands");

        // Once it ended, the later thread is what the key waits on, from its own time.
        drop(first);
        assert!(!passes.is_stuck(&key), "the second's time is not yet");
        drop(second);
        assert!(!passes.is_stuck(&key), "and with it gone nothing is left");
        let third = Arc::new(());
        passes.record_from(key.clone(), Arc::downgrade(&third), begun);
        assert!(passes.is_stuck(&key), "a record after the end is taken");
    }

    /// The entries of ended threads do not stay under keys that are never asked about again.
    #[test]
    fn the_entries_of_ended_threads_are_dropped_by_a_later_record() {
        let passes = StalledPasses::<u32>::default();
        let ended = Arc::new(());
        passes.record(1, Arc::downgrade(&ended));
        let running = Arc::new(());
        passes.record(2, Arc::downgrade(&running));
        drop(ended);

        let latest = Arc::new(());
        passes.record(3, Arc::downgrade(&latest));

        let mut keys: Vec<u32> = passes.0.lock().unwrap().keys().copied().collect();
        keys.sort();
        assert_eq!(keys, [2, 3]);
    }
}
