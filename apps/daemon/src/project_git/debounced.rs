//! The scheduler behind re-reading projects' git status when something reports their refs moved,
//! by whatever key the caller chooses (a project, a repository).
//!
//! Reports are coalesced per key: one arriving while a read is waiting pushes it back by
//! [`Timing::debounce`], up to [`Timing::max_delay`] after the first, and one arriving while a
//! read runs owes the key one more read afterwards, so a burst costs a read or two and the reads
//! of one key never overlap or finish out of order. At most [`BLOCKING_WIDTH`] keys are read at
//! once.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::time::Instant;

use crate::blocking_pass::BLOCKING_WIDTH;

/// How a [`Debounced`] spaces its reads.
#[derive(Clone, Copy)]
pub struct Timing {
    /// How long a key's reports stay quiet before it is read.
    pub debounce: Duration,
    /// The longest a read is pushed back after the first report it answers; `None` lets a steady
    /// stream of reports push it back for as long as it lasts.
    pub max_delay: Option<Duration>,
    /// How long after a read that reports arrived during the key waits for the read it owes,
    /// counted from the end of the read and pushed back by later reports as `debounce` is.
    pub owed: Duration,
}

enum Phase {
    /// A read is due at `at`, pushed back by later reports but never past `latest`.
    Waiting {
        at: Instant,
        latest: Option<Instant>,
    },
    /// A read is running; `again` once a report has arrived since it started.
    Reading { again: bool },
}

/// The keys with a read due or running, and the places for running reads.
pub struct Debounced<K> {
    timing: Timing,
    phases: Mutex<HashMap<K, Phase>>,
    reads: tokio::sync::Semaphore,
}

impl<K: Clone + Eq + Hash + Send + 'static> Debounced<K> {
    pub fn new(timing: Timing) -> Self {
        Self {
            timing,
            phases: Mutex::default(),
            reads: tokio::sync::Semaphore::new(BLOCKING_WIDTH),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<K, Phase>> {
        self.phases.lock().expect("debounced reads lock poisoned")
    }

    fn waiting(&self, from: Instant, quiet: Duration) -> Phase {
        Phase::Waiting {
            at: from + quiet,
            latest: self.timing.max_delay.map(|longest| from + longest),
        }
    }

    /// Takes a report for `key`, and has it read once the reports settle. `read` makes the
    /// function that reads, on a blocking thread, once for each time the key goes from idle to
    /// due; a report for a key already due or being read does not call it. Must be called inside
    /// the runtime.
    pub fn report<R>(self: &Arc<Self>, key: K, read: impl FnOnce() -> R)
    where
        R: Fn() + Send + Sync + 'static,
    {
        let now = Instant::now();
        let mut phases = self.lock();
        match phases.get_mut(&key) {
            Some(Phase::Waiting { at, latest }) => {
                let due = now + self.timing.debounce;
                *at = latest.map_or(due, |latest| due.min(latest));
            }
            Some(Phase::Reading { again }) => *again = true,
            None => {
                phases.insert(key.clone(), self.waiting(now, self.timing.debounce));
                tokio::spawn(Arc::clone(self).settle(key, Arc::new(read())));
            }
        }
    }

    /// Whether no key has a read due or running.
    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        self.lock().is_empty()
    }

    /// Waits out the key's reports, reads it, and goes round again while reports arrived during
    /// the read.
    async fn settle<R>(self: Arc<Self>, key: K, read: Arc<R>)
    where
        R: Fn() + Send + Sync + 'static,
    {
        loop {
            loop {
                let at = match self.lock().get(&key) {
                    Some(Phase::Waiting { at, .. }) => *at,
                    _ => return,
                };
                if Instant::now() >= at {
                    break;
                }
                tokio::time::sleep_until(at).await;
            }
            // A report that arrives while the read waits for a place only moves `at`: the read
            // that follows sees it.
            let place = self
                .reads
                .acquire()
                .await
                .expect("the read places are never closed");
            self.lock()
                .insert(key.clone(), Phase::Reading { again: false });
            let reading = {
                let read = Arc::clone(&read);
                tokio::task::spawn_blocking(move || read())
            };
            if let Err(err) = reading.await {
                tracing::warn!(%err, "a debounced git read failed");
            }
            drop(place);
            let mut phases = self.lock();
            if let Some(Phase::Reading { again: true }) = phases.get(&key) {
                phases.insert(key.clone(), self.waiting(Instant::now(), self.timing.owed));
            } else {
                phases.remove(&key);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TIMING: Timing = Timing {
        debounce: Duration::from_millis(300),
        max_delay: Some(Duration::from_millis(1000)),
        owed: Duration::from_millis(300),
    };

    /// Lets the scheduler's tasks run without moving the paused clock.
    async fn settle_tasks() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    /// A read that counts itself on `reads` and tells `read_tx` when it has run.
    fn counting(
        reads: &Arc<AtomicUsize>,
    ) -> (
        impl Fn() -> Box<dyn Fn() + Send + Sync> + '_,
        tokio::sync::mpsc::UnboundedReceiver<()>,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let make = move || {
            let reads = Arc::clone(reads);
            let sender = sender.clone();
            Box::new(move || {
                reads.fetch_add(1, Ordering::SeqCst);
                sender.send(()).ok();
            }) as Box<dyn Fn() + Send + Sync>
        };
        (make, receiver)
    }

    #[tokio::test(start_paused = true)]
    async fn a_burst_of_reports_costs_one_read_after_it_goes_quiet() {
        let scheduler = Arc::new(Debounced::new(TIMING));
        let reads = Arc::new(AtomicUsize::new(0));
        let (make, mut done) = counting(&reads);
        let start = Instant::now();

        for _ in 0..5 {
            scheduler.report("a", &make);
            tokio::time::advance(Duration::from_millis(100)).await;
            settle_tasks().await;
        }
        // 500 ms since the first report, but only 100 ms since the last.
        assert_eq!(start.elapsed(), Duration::from_millis(500));
        assert_eq!(reads.load(Ordering::SeqCst), 0);

        tokio::time::advance(Duration::from_millis(200)).await;
        done.recv().await.unwrap();
        // The last report came at 400 ms, so the read ran 300 ms after it.
        assert_eq!(start.elapsed(), Duration::from_millis(700));
        settle_tasks().await;
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert!(scheduler.is_idle());
    }

    #[tokio::test(start_paused = true)]
    async fn a_steady_stream_is_read_by_the_longest_delay() {
        let scheduler = Arc::new(Debounced::new(TIMING));
        let reads = Arc::new(AtomicUsize::new(0));
        let (make, mut done) = counting(&reads);
        let start = Instant::now();

        // A report every 200 ms never lets 300 ms pass quietly.
        for _ in 0..5 {
            scheduler.report("a", &make);
            tokio::time::advance(Duration::from_millis(200)).await;
            settle_tasks().await;
        }
        // 1000 ms since the first report: the longest delay, to the millisecond.
        done.recv().await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_millis(1000));
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn without_a_longest_delay_a_steady_stream_is_never_read() {
        let scheduler = Arc::new(Debounced::new(Timing {
            max_delay: None,
            ..TIMING
        }));
        let reads = Arc::new(AtomicUsize::new(0));
        let (make, _done) = counting(&reads);

        for _ in 0..20 {
            scheduler.report("a", &make);
            tokio::time::advance(Duration::from_millis(200)).await;
            settle_tasks().await;
        }
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_report_during_a_read_owes_one_more_that_does_not_overlap() {
        let scheduler = Arc::new(Debounced::new(Timing {
            owed: Duration::ZERO,
            ..TIMING
        }));
        let running = Arc::new(AtomicUsize::new(0));
        let overlapped = Arc::new(AtomicUsize::new(0));
        let reads = Arc::new(AtomicUsize::new(0));
        let (started_tx, mut started) = tokio::sync::mpsc::unbounded_channel();
        let (release_tx, release) = std::sync::mpsc::channel::<()>();
        let release = Arc::new(Mutex::new(release));
        let read = {
            let (running, overlapped, reads) = (running.clone(), overlapped.clone(), reads.clone());
            move || {
                let (running, overlapped, reads) =
                    (running.clone(), overlapped.clone(), reads.clone());
                let (started_tx, release) = (started_tx.clone(), release.clone());
                move || {
                    if running.fetch_add(1, Ordering::SeqCst) > 0 {
                        overlapped.fetch_add(1, Ordering::SeqCst);
                    }
                    reads.fetch_add(1, Ordering::SeqCst);
                    started_tx.send(()).ok();
                    release.lock().unwrap().recv().ok();
                    running.fetch_sub(1, Ordering::SeqCst);
                }
            }
        };

        scheduler.report("a", &read);
        tokio::time::advance(TIMING.debounce).await;
        started.recv().await.unwrap();
        // Reported while the first read runs, once and then again: one read owed.
        scheduler.report("a", &read);
        scheduler.report("a", &read);
        settle_tasks().await;
        assert_eq!(reads.load(Ordering::SeqCst), 1);

        release_tx.send(()).unwrap();
        started.recv().await.unwrap();
        release_tx.send(()).unwrap();
        while !scheduler.is_idle() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        assert_eq!(overlapped.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_owed_read_waits_out_its_own_quiet_period() {
        let scheduler = Arc::new(Debounced::new(TIMING));
        let reads = Arc::new(AtomicUsize::new(0));
        let (started_tx, mut started) = tokio::sync::mpsc::unbounded_channel();
        let (release_tx, release) = std::sync::mpsc::channel::<()>();
        let release = Arc::new(Mutex::new(release));
        let read = {
            let reads = reads.clone();
            move || {
                let (reads, started_tx, release) =
                    (reads.clone(), started_tx.clone(), release.clone());
                move || {
                    reads.fetch_add(1, Ordering::SeqCst);
                    started_tx.send(()).ok();
                    release.lock().unwrap().recv().ok();
                }
            }
        };

        let start = Instant::now();
        scheduler.report("a", &read);
        tokio::time::advance(TIMING.debounce).await;
        started.recv().await.unwrap();
        scheduler.report("a", &read);
        release_tx.send(()).unwrap();
        settle_tasks().await;
        // The first read is over; the second is owed `TIMING.owed` after it.
        tokio::time::advance(TIMING.owed - Duration::from_millis(1)).await;
        settle_tasks().await;
        assert_eq!(reads.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_millis(1)).await;
        started.recv().await.unwrap();
        assert_eq!(start.elapsed(), TIMING.debounce + TIMING.owed);
        release_tx.send(()).unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn only_the_blocking_width_of_keys_is_read_at_once() {
        let scheduler = Arc::new(Debounced::new(TIMING));
        let reads = Arc::new(AtomicUsize::new(0));
        let (started_tx, mut started) = tokio::sync::mpsc::unbounded_channel();
        let (release_tx, release) = std::sync::mpsc::channel::<()>();
        let release = Arc::new(Mutex::new(release));
        let read = {
            let reads = reads.clone();
            move || {
                let (reads, started_tx, release) =
                    (reads.clone(), started_tx.clone(), release.clone());
                move || {
                    reads.fetch_add(1, Ordering::SeqCst);
                    started_tx.send(()).ok();
                    release.lock().unwrap().recv().ok();
                }
            }
        };

        for key in 0..=BLOCKING_WIDTH {
            scheduler.report(key, &read);
        }
        tokio::time::advance(TIMING.debounce).await;
        for _ in 0..BLOCKING_WIDTH {
            started.recv().await.unwrap();
        }
        settle_tasks().await;
        assert_eq!(reads.load(Ordering::SeqCst), BLOCKING_WIDTH);

        release_tx.send(()).unwrap();
        started.recv().await.unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), BLOCKING_WIDTH + 1);
        for _ in 0..BLOCKING_WIDTH {
            release_tx.send(()).unwrap();
        }
        while !scheduler.is_idle() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
