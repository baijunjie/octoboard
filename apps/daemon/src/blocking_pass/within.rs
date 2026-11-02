//! A pass of blocking work on threads of its own, waited for only until a deadline, for a caller
//! that is not on the runtime (a thread of its own, or one already on the blocking pool). An async
//! caller looking at paths takes `hostfs::place::looked_within` instead.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use super::{StalledPasses, BLOCKING_WIDTH};

/// What [`within_deadline`] found, and the keys it found nothing for in time.
pub struct WithinDeadline<K, T, R> {
    /// What the work found, item by item, with the items it found nothing for left out.
    pub found: Vec<(T, R)>,
    /// The keys whose work was still running when the pass stopped waiting.
    pub timed_out: Vec<K>,
    /// The keys left out without any work, since their work from an earlier pass is still stuck.
    pub stuck: Vec<K>,
}

/// What a thread of [`within_deadline`] tells the caller.
enum Message<K, T, R> {
    /// It began the work for the key, with a weak handle to the work, which its thread holds alive
    /// for as long as the work runs.
    Begun(K, Weak<()>, Instant),
    /// It finished the work for the key, with what it found.
    Done(K, Vec<(T, R)>),
}

/// The keys of one pass left to work on, each with its items.
type Queue<K, T> = Arc<Mutex<Vec<(K, Vec<T>)>>>;

/// What `work` finds for each item of `groups`, whose items are grouped by what the work for any
/// of them may block on alike (a path, a process). At most [`BLOCKING_WIDTH`] keys are worked on
/// at once, each on a thread of the pass's own, and the pass waits for no longer than `deadline`
/// in all. The items of a key whose work has not ended by then are left out, and its thread is
/// left to finish on its own, recorded in `stalled` as stuck from once it has run for `deadline`;
/// while it stays stuck, a later pass leaves the key out at once instead of giving it another
/// thread that the same cause would hold. **Blocks** for up to `deadline`, so the caller holds no
/// lock others wait on.
pub fn within_deadline<K, T, R>(
    stalled: &StalledPasses<K>,
    groups: HashMap<K, Vec<T>>,
    deadline: Duration,
    work: impl Fn(&T) -> Option<R> + Send + Sync + 'static,
) -> WithinDeadline<K, T, R>
where
    K: Eq + Hash + Clone + Send + 'static,
    T: Send + 'static,
    R: Send + 'static,
{
    let ends = Instant::now() + deadline;
    let (queue, stuck): (Vec<(K, Vec<T>)>, Vec<(K, Vec<T>)>) = groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .partition(|(key, _)| !stalled.is_stuck(key));
    let mut pass = WithinDeadline {
        found: Vec::new(),
        timed_out: Vec::new(),
        stuck: stuck.into_iter().map(|(key, _)| key).collect(),
    };
    let total = queue.len();
    if total == 0 {
        return pass;
    }
    let queue: Queue<K, T> = Arc::new(Mutex::new(queue));
    let work = Arc::new(work);
    let (sender, answers) = mpsc::channel();
    for _ in 0..total.min(BLOCKING_WIDTH) {
        let (queue, work, sender) = (queue.clone(), work.clone(), sender.clone());
        let spawned = std::thread::Builder::new()
            .name("blocking-pass-within".to_string())
            .spawn(move || work_through(&queue, &*work, &sender));
        if let Err(err) = spawned {
            tracing::warn!(%err, "starting a thread of a blocking pass failed");
        }
    }
    // Only the threads hold a sender from here on, so the channel closes once they all ended.
    drop(sender);
    let mut begun: HashMap<K, (Weak<()>, Instant)> = HashMap::new();
    let mut done = 0;
    while done < total {
        match answers.recv_timeout(ends.saturating_duration_since(Instant::now())) {
            Ok(message) => done += usize::from(take(message, &mut begun, &mut pass.found)),
            Err(_) => break,
        }
    }
    // Emptied under the lock a thread takes a key under, so no thread starts another key from
    // here on, and a thread stuck on its key holds none of the keys never started.
    queue
        .lock()
        .expect("blocking pass queue lock poisoned")
        .clear();
    // A thread announces a key under that lock too, so whatever it began before the queue was
    // emptied, even after the deadline, is in the channel by now and is recorded with the rest.
    for message in answers.try_iter() {
        take(message, &mut begun, &mut pass.found);
    }
    for (key, (alive, at)) in begun {
        stalled.record_from(key.clone(), alive, at + deadline);
        pass.timed_out.push(key);
    }
    pass
}

/// Takes in one message of a thread of [`within_deadline`], and says whether a key is done.
fn take<K: Eq + Hash, T, R>(
    message: Message<K, T, R>,
    begun: &mut HashMap<K, (Weak<()>, Instant)>,
    found: &mut Vec<(T, R)>,
) -> bool {
    match message {
        Message::Begun(key, alive, at) => {
            begun.insert(key, (alive, at));
            false
        }
        Message::Done(key, results) => {
            begun.remove(&key);
            found.extend(results);
            true
        }
    }
}

/// One thread of [`within_deadline`]: works through the keys left in `queue` until there are none,
/// which is also how the pass tells it to stop.
fn work_through<K, T, R>(
    queue: &Queue<K, T>,
    work: &(impl Fn(&T) -> Option<R> + ?Sized),
    sender: &mpsc::Sender<Message<K, T, R>>,
) where
    K: Clone,
{
    loop {
        let alive = Arc::new(());
        let (key, items) = {
            let mut queue = queue.lock().expect("blocking pass queue lock poisoned");
            let Some((key, items)) = queue.pop() else {
                return;
            };
            let begun = Message::Begun(key.clone(), Arc::downgrade(&alive), Instant::now());
            if sender.send(begun).is_err() {
                return;
            }
            (key, items)
        };
        let found = items
            .into_iter()
            .filter_map(|item| work(&item).map(|value| (item, value)))
            .collect();
        drop(alive);
        if sender.send(Message::Done(key, found)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::within_deadline;
    use crate::blocking_pass::{StalledPasses, BLOCKING_WIDTH};
    use crate::test_support::PATIENCE;

    fn one(path: &PathBuf, item: u32) -> HashMap<PathBuf, Vec<u32>> {
        HashMap::from([(path.clone(), vec![item])])
    }

    /// Work that does not come back within the deadline leaves its items out, and once it has
    /// run past its deadline its key is not worked on again while it is still stuck, so a mount
    /// that does not answer neither holds a caller past the deadline nor takes a thread at every
    /// pass; the other keys of a pass are worked on all the same, and the stuck one again once
    /// its work has ended.
    #[test]
    fn work_that_outruns_its_deadline_is_left_out_and_skipped_while_it_is_stuck() {
        let stalled = StalledPasses::default();
        let stuck = PathBuf::from("/octoboard-test/stuck");
        let fine = PathBuf::from("/octoboard-test/fine");
        let (release, held) = std::sync::mpsc::channel::<()>();
        let held = Mutex::new(held);
        let deadline = Duration::from_millis(100);

        let begun = Instant::now();
        let pass = within_deadline(&stalled, one(&stuck, 1), deadline, move |_| {
            held.lock().unwrap().recv().ok();
            Some(())
        });
        assert!(pass.found.is_empty());
        assert_eq!(pass.timed_out, vec![stuck.clone()]);
        assert!(
            begun.elapsed() < PATIENCE,
            "the pass waited past its deadline"
        );
        // Past its own deadline, the work that is still running counts as stuck.
        std::thread::sleep(Duration::from_millis(150));

        let started = Arc::new(Mutex::new(Vec::new()));
        let counted = started.clone();
        let mut both = one(&stuck, 1);
        both.insert(fine.clone(), vec![2]);
        let again = within_deadline(&stalled, both, deadline, move |item| {
            counted.lock().unwrap().push(*item);
            Some(*item)
        });
        assert_eq!(again.found, vec![(2, 2)], "another key is still worked on");
        assert_eq!(again.stuck, vec![stuck.clone()]);
        assert_eq!(
            *started.lock().unwrap(),
            vec![2],
            "no second work on the stuck key is started"
        );

        release.send(()).expect("release");
        let patience = Instant::now() + PATIENCE;
        while within_deadline(&stalled, one(&stuck, 1), deadline, |_| Some(()))
            .found
            .is_empty()
        {
            assert!(Instant::now() < patience, "the key is worked on again");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Once a pass stops waiting, the keys no thread started are let go at once: a thread stuck on
    /// its own key does not keep the rest of the pass's work alive for as long as it is stuck.
    #[test]
    fn the_keys_never_started_are_let_go_when_the_pass_stops_waiting() {
        let stalled = StalledPasses::default();
        let token = Arc::new(());
        let groups: HashMap<u32, Vec<Arc<()>>> = (0..=BLOCKING_WIDTH as u32)
            .map(|key| (key, vec![token.clone()]))
            .collect();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let held = Mutex::new(held);
        let began = Arc::new(AtomicUsize::new(0));
        let counted = began.clone();

        // A deadline long enough that every thread has taken its key before it runs out, even
        // on a loaded machine; the count says so when it was not.
        let pass = within_deadline(&stalled, groups, Duration::from_secs(2), move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            held.lock().unwrap().recv().ok();
            Some(())
        });
        assert_eq!(
            began.load(Ordering::SeqCst),
            BLOCKING_WIDTH,
            "every thread began a key before the deadline"
        );
        assert!(pass.found.is_empty());
        assert_eq!(
            pass.timed_out.len(),
            BLOCKING_WIDTH,
            "every key begun is recorded"
        );
        assert_eq!(
            Arc::strong_count(&token),
            1 + BLOCKING_WIDTH,
            "only the keys being worked on are still held"
        );
        drop(release);
    }
}
