//! Where a directory really is on the file system, for deciding whether one directory is another
//! or lies below it.
//!
//! The paths' text cannot answer that: a link inside a directory can lead anywhere, and a spelling
//! through a link or in another letter case can name a directory that is inside. So a path is
//! resolved (`canonicalize`), and the directory it leads to and each directory above that one up
//! to the root are told apart by device and inode ([`Place`]). Containment is then membership: the
//! outer directory's identity is one of the identities on the inner one's real path. Comparing
//! identities rather than the resolved text holds whatever the file system does with letter case
//! (`canonicalize` returns the on-disk spelling on APFS, but that is the file system's doing, not
//! a promise), and through a bind mount or a macOS firmlink, which give one directory two real
//! paths.
//!
//! A path that cannot be resolved has no place: nothing at it, one that cannot be read, a link
//! that leads nowhere, a file system that reports no inode, or a look that outran its deadline
//! ([`looked_within`]). Which way that counts is the caller's to decide, by what a match does.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::{identity, lexically_normalise};
use crate::state::StalledPasses;

/// A directory told apart by what it is on the file system: its device and inode.
pub type Identity = (u64, u64);

/// How many paths are looked at at once, this daemon's figure for a pass of blocking threads.
const LOOKS_AT_ONCE: usize = 4;

#[cfg(test)]
thread_local! {
    /// Lowers [`LOOKS_AT_ONCE`] for a test, on the test's own thread.
    static LOOKS_AT_ONCE_OVERRIDE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

fn looks_at_once() -> usize {
    #[cfg(test)]
    if let Some(cap) = LOOKS_AT_ONCE_OVERRIDE.with(std::cell::Cell::get) {
        return cap;
    }
    LOOKS_AT_ONCE
}

/// A directory as it actually is on the file system: its own identity, and the identities of every
/// directory on its real path, itself included.
pub struct Place {
    own: Identity,
    within: HashSet<Identity>,
}

impl Place {
    /// The place `path` leads to, or `None` when it is not absolute or cannot be resolved to a
    /// directory. Blocking: it reads the file system.
    pub fn of(path: &Path) -> Option<Place> {
        Self::resolved(path).map(|(place, _)| place)
    }

    /// [`Place::of`], with the real path `path` resolved to.
    pub fn resolved(path: &Path) -> Option<(Place, PathBuf)> {
        let path = lexically_normalise(path);
        if !path.is_absolute() {
            return None;
        }
        let real = std::fs::canonicalize(&path).ok()?;
        let own = resolved_directory_identity(&real)?;
        // An ancestor that cannot be read only loses the match it could have made.
        let within = real
            .ancestors()
            .filter_map(resolved_directory_identity)
            .collect();
        Some((Place { own, within }, real))
    }

    /// Whether this place is `directory` or lies below it.
    pub fn lies_in(&self, directory: &Place) -> bool {
        self.within.contains(&directory.own)
    }

    /// Whether this place is `other`.
    pub fn is(&self, other: &Place) -> bool {
        self.own == other.own
    }

    /// Whether this place is one of `identities`.
    pub fn is_one_of(&self, identities: &HashSet<Identity>) -> bool {
        identities.contains(&self.own)
    }

    /// The identities of this directory and of every directory above it.
    pub fn path_identities(&self) -> impl Iterator<Item = Identity> + '_ {
        self.within.iter().copied()
    }
}

/// The identity of the directory at `path`, a component of a path `canonicalize` has just
/// resolved. Its last component is not followed, so one that has become a link since is not a
/// directory and gives nothing; the components before it are followed as usual. `None` too where
/// it cannot be read or the file system reports no inode (0).
fn resolved_directory_identity(path: &Path) -> Option<Identity> {
    directory_identity(&std::fs::symlink_metadata(path).ok()?)
}

/// The identity of the directory `path` leads to, following links. `None` as for
/// [`resolved_directory_identity`].
pub fn followed_directory_identity(path: &Path) -> Option<Identity> {
    directory_identity(&std::fs::metadata(path).ok()?)
}

fn directory_identity(metadata: &std::fs::Metadata) -> Option<Identity> {
    let identity = identity(metadata);
    (metadata.is_dir() && identity.1 != 0).then_some(identity)
}

/// What a look at a path does while an earlier look at it, which outran its deadline, is still
/// stuck on its thread.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stuck {
    /// The path is absent from the result at once.
    Skip,
    /// The look waits for the earlier one to end, for what is left of its deadline, and then looks
    /// afresh: the earlier look's answer is lost with its thread, so it cannot be used. The path is
    /// absent if the earlier look has not ended by then, or no time is left to look. Paths that are
    /// not stuck are looked at at once, with the whole deadline. The time left is a target, not a
    /// hard bound: a look may also wait for a permit of the shared cap on looks at once.
    Wait,
}

/// How the paths of one call are looked at.
#[derive(Clone, Copy)]
pub struct Looking {
    /// How long a look is waited for; past it the path is absent from the result.
    pub deadline: Duration,
    /// How long a look has run when its path counts as stuck: a look left behind at `deadline`
    /// that is still running by then is recorded, and not before. Until then its thread runs on
    /// unrecorded, so other work still looks at the path as if nothing were stuck.
    pub stuck_after: Duration,
    /// What a path whose earlier look is still stuck gets.
    pub stuck: Stuck,
}

impl Looking {
    /// Looks that are waited for `deadline` and are stuck once they outrun it, skipping a path
    /// that already is.
    pub const fn within(deadline: Duration) -> Self {
        Self {
            deadline,
            stuck_after: deadline,
            stuck: Stuck::Skip,
        }
    }
}

/// How often a look that waits for a stuck one asks whether it has ended.
const STUCK_POLL: Duration = Duration::from_millis(20);

/// What `work` finds at each of `paths`, each on a blocking thread, at most [`LOOKS_AT_ONCE`] at a
/// time, keyed by the path as given. A path is absent from the result when `work` found nothing,
/// its thread panicked, or its look outran `deadline`. A look that outran it is left to finish on
/// its own and recorded in `stalled` until it does; while it runs, a later look at the same path is
/// not started and the path is absent at once, so a stuck mount does not take another thread at
/// every decision ([`Stuck::Skip`]). The caller holds no lock of the store while this runs. A
/// timed-out look counts as stuck as soon as `deadline` passes, so a caller with a deadline shorter
/// than the other users of `stalled` takes [`looked_within_stuck`] with a longer
/// [`Looking::stuck_after`] instead, or it marks merely slow paths stuck for all of them.
pub async fn looked_within<T: Send + 'static>(
    stalled: &StalledPasses<PathBuf>,
    paths: Vec<PathBuf>,
    deadline: Duration,
    work: impl Fn(&Path) -> Option<T> + Send + Sync + 'static,
) -> HashMap<PathBuf, T> {
    looked_within_stuck(stalled, paths, Looking::within(deadline), work).await
}

/// [`looked_within`], with `looking` deciding how long a look is waited for, when a look left
/// behind counts as stuck, and what a path whose earlier look is still stuck gets.
pub async fn looked_within_stuck<T: Send + 'static>(
    stalled: &StalledPasses<PathBuf>,
    paths: Vec<PathBuf>,
    looking: Looking,
    work: impl Fn(&Path) -> Option<T> + Send + Sync + 'static,
) -> HashMap<PathBuf, T> {
    let work = Arc::new(work);
    let slots = Arc::new(tokio::sync::Semaphore::new(looks_at_once()));
    let unique: HashSet<PathBuf> = paths.into_iter().collect();
    if looking.stuck == Stuck::Skip {
        return looked_at(
            stalled,
            unique,
            looking.deadline,
            looking.stuck_after,
            &slots,
            &work,
        )
        .await;
    }
    let (stuck, ready): (HashSet<PathBuf>, HashSet<PathBuf>) = unique
        .into_iter()
        .partition(|path| stalled.is_stuck(&lexically_normalise(path)));
    let waiting = async {
        let left = waited_for_stuck(stalled, &stuck, looking.deadline).await;
        if left.is_zero() {
            return HashMap::new();
        }
        looked_at(stalled, stuck, left, looking.stuck_after, &slots, &work).await
    };
    let (mut found, waited) = tokio::join!(
        looked_at(
            stalled,
            ready,
            looking.deadline,
            looking.stuck_after,
            &slots,
            &work
        ),
        waiting
    );
    found.extend(waited);
    found
}

/// [`looked_within_stuck`]'s looks at `paths`, each waited for `deadline`, a path whose earlier
/// look is still stuck skipped. Each look holds one of `slots`, which the groups of one call share,
/// until it ends or outruns its deadline.
async fn looked_at<T: Send + 'static>(
    stalled: &StalledPasses<PathBuf>,
    paths: HashSet<PathBuf>,
    deadline: Duration,
    stuck_after: Duration,
    slots: &Arc<tokio::sync::Semaphore>,
    work: &Arc<impl Fn(&Path) -> Option<T> + Send + Sync + 'static>,
) -> HashMap<PathBuf, T> {
    let mut running = tokio::task::JoinSet::new();
    let mut found = HashMap::new();
    for path in paths {
        let key = lexically_normalise(&path);
        if stalled.is_stuck(&key) {
            continue;
        }
        let slot = slots.clone().acquire_owned().await.ok();
        let work = work.clone();
        let stuck_from = std::time::Instant::now() + stuck_after;
        running.spawn(async move {
            let _slot = slot;
            let alive = Arc::new(());
            let pass = Arc::downgrade(&alive);
            let looking = path.clone();
            let look = tokio::task::spawn_blocking(move || {
                let value = work(&looking);
                drop(alive);
                value
            });
            match tokio::time::timeout(deadline, look).await {
                Ok(Ok(value)) => value.map(|value| Look::Found(path, value)),
                Ok(Err(_)) => None,
                Err(_) => {
                    tracing::warn!(path = %path.display(), "looking at a directory timed out; it counts as unresolved");
                    Some(Look::TimedOut(key, pass, stuck_from))
                }
            }
        });
    }
    while let Some(joined) = running.join_next().await {
        match joined.ok().flatten() {
            Some(Look::Found(path, value)) => {
                found.insert(path, value);
            }
            Some(Look::TimedOut(key, pass, stuck_from)) => {
                stalled.record_from(key, pass, stuck_from)
            }
            None => {}
        }
    }
    found
}

/// Waits until none of `paths` has a look still stuck, for at most `deadline`, and returns what is
/// left of it.
async fn waited_for_stuck(
    stalled: &StalledPasses<PathBuf>,
    paths: &HashSet<PathBuf>,
    deadline: Duration,
) -> Duration {
    let started = std::time::Instant::now();
    let stuck = || {
        paths
            .iter()
            .any(|path| stalled.is_stuck(&lexically_normalise(path)))
    };
    while stuck() && started.elapsed() < deadline {
        tokio::time::sleep(STUCK_POLL.min(deadline.saturating_sub(started.elapsed()))).await;
    }
    deadline.saturating_sub(started.elapsed())
}

/// How one path's look in [`looked_within`] ended, when it gave anything to keep.
enum Look<T> {
    Found(PathBuf, T),
    /// It outran its deadline; the key it is recorded under while its thread stays stuck, and when
    /// it counts as stuck.
    TimedOut(PathBuf, std::sync::Weak<()>, std::time::Instant),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::{looked_within, looked_within_stuck, Looking, Stuck, LOOKS_AT_ONCE_OVERRIDE};
    use crate::state::StalledPasses;
    use crate::test_support::PATIENCE;

    fn skipping(deadline: Duration) -> Looking {
        Looking::within(deadline)
    }

    fn waiting(deadline: Duration) -> Looking {
        Looking {
            stuck: Stuck::Wait,
            ..Looking::within(deadline)
        }
    }

    /// A look at a path that does not come back within its deadline counts as unresolved, and
    /// while its thread is still stuck the same path is not looked at again, so a mount that does
    /// not answer neither holds a decision past the deadline nor takes a thread at every one; the
    /// other paths of a pass are looked at all the same.
    #[tokio::test]
    async fn a_look_that_outruns_its_deadline_is_unresolved_and_not_started_again_while_stuck() {
        let stalled = StalledPasses::default();
        let stuck = PathBuf::from("/octoboard-test/stuck");
        let fine = PathBuf::from("/octoboard-test/fine");
        let (release, held) = std::sync::mpsc::channel::<()>();
        let held = Mutex::new(held);
        let found = looked_within(
            &stalled,
            vec![stuck.clone()],
            Duration::from_millis(100),
            move |_| {
                held.lock().unwrap().recv().ok();
                Some(())
            },
        )
        .await;
        assert!(found.is_empty(), "{:?}", found.keys());

        let started = Arc::new(Mutex::new(Vec::new()));
        let counted = started.clone();
        let again = looked_within(
            &stalled,
            vec![stuck.clone(), fine.clone()],
            PATIENCE,
            move |path| {
                counted.lock().unwrap().push(path.to_path_buf());
                Some(())
            },
        )
        .await;
        assert!(!again.contains_key(&stuck));
        assert!(again.contains_key(&fine), "another path is still looked at");
        assert_eq!(
            *started.lock().unwrap(),
            std::slice::from_ref(&fine),
            "no second look at the stuck path is started"
        );

        // Once the stuck thread ends, the path is looked at again.
        release.send(()).expect("release");
        let deadline = Instant::now() + PATIENCE;
        loop {
            let found = looked_within(&stalled, vec![stuck.clone()], PATIENCE, |_| Some(())).await;
            if found.contains_key(&stuck) {
                break;
            }
            assert!(Instant::now() < deadline, "the path is looked at again");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// However many paths a pass has, no more than the cap are looked at at once.
    #[tokio::test]
    async fn no_more_paths_are_looked_at_at_once_than_the_cap() {
        LOOKS_AT_ONCE_OVERRIDE.with(|cap| cap.set(Some(2)));
        let busy = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let (counting, highest) = (busy.clone(), peak.clone());
        let paths: Vec<PathBuf> = (0..6)
            .map(|n| PathBuf::from(format!("/octoboard-test/{n}")))
            .collect();
        let found = looked_within(&StalledPasses::default(), paths, PATIENCE, move |_| {
            let now = counting.fetch_add(1, Ordering::SeqCst) + 1;
            highest.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            counting.fetch_sub(1, Ordering::SeqCst);
            Some(())
        })
        .await;
        LOOKS_AT_ONCE_OVERRIDE.with(|cap| cap.set(None));
        assert_eq!(found.len(), 6, "every path is looked at");
        let peak = peak.load(Ordering::SeqCst);
        assert!(peak <= 2, "{peak} at once");
    }

    /// The paths that were stuck and the ones that were not share the cap: waiting for the stuck
    /// ones does not let both groups together take more threads than it allows.
    #[tokio::test]
    async fn the_cap_is_shared_by_the_paths_waited_for_and_the_ones_not() {
        LOOKS_AT_ONCE_OVERRIDE.with(|cap| cap.set(Some(2)));
        let stalled = StalledPasses::default();
        let guard = Arc::new(());
        let paths: Vec<PathBuf> = (0..6)
            .map(|n| PathBuf::from(format!("/octoboard-test/{n}")))
            .collect();
        for path in &paths[..3] {
            stalled.record(super::lexically_normalise(path), Arc::downgrade(&guard));
        }
        let ending = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(guard);
        });
        let busy = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let (counting, highest) = (busy.clone(), peak.clone());
        let found = looked_within_stuck(&stalled, paths, waiting(PATIENCE), move |_| {
            let now = counting.fetch_add(1, Ordering::SeqCst) + 1;
            highest.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            counting.fetch_sub(1, Ordering::SeqCst);
            Some(())
        })
        .await;
        LOOKS_AT_ONCE_OVERRIDE.with(|cap| cap.set(None));
        ending.await.unwrap();
        assert_eq!(found.len(), 6, "every path is looked at");
        let peak = peak.load(Ordering::SeqCst);
        assert!(peak <= 2, "{peak} at once");
    }

    /// A path whose earlier look is still stuck is waited for, for what is left of the deadline,
    /// and looked at afresh once it ends; where it does not end, it is absent.
    #[tokio::test]
    async fn a_look_can_wait_for_a_stuck_one_to_end() {
        let stalled = StalledPasses::default();
        let path = PathBuf::from("/octoboard-test/waited");
        let guard = Arc::new(());
        stalled.record(super::lexically_normalise(&path), Arc::downgrade(&guard));

        let skipped = looked_within_stuck(&stalled, vec![path.clone()], skipping(PATIENCE), |_| {
            Some(())
        })
        .await;
        assert!(skipped.is_empty());

        let started = Instant::now();
        let unended = looked_within_stuck(
            &stalled,
            vec![path.clone()],
            waiting(Duration::from_millis(150)),
            |_| Some(()),
        )
        .await;
        assert!(unended.is_empty(), "still stuck at the deadline");
        assert!(started.elapsed() >= Duration::from_millis(140));

        let ending = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(guard);
        });
        let waited =
            looked_within_stuck(
                &stalled,
                vec![path.clone()],
                waiting(PATIENCE),
                |_| Some(()),
            )
            .await;
        ending.await.unwrap();
        assert!(
            waited.contains_key(&path),
            "looked at once the stuck look ended"
        );
    }

    /// A path stuck from an earlier look does not hold up the others: they are looked at at once
    /// with the whole deadline, and the stuck one, waited for in the meantime, is left absent and
    /// not looked at when no time is left.
    #[tokio::test]
    async fn a_stuck_path_does_not_take_the_time_of_the_others() {
        let stalled = StalledPasses::default();
        let stuck = PathBuf::from("/octoboard-test/stuck");
        let healthy = PathBuf::from("/octoboard-test/healthy");
        let guard = Arc::new(());
        stalled.record(super::lexically_normalise(&stuck), Arc::downgrade(&guard));
        let started = Arc::new(Mutex::new(Vec::new()));
        let counted = started.clone();
        let begun = Instant::now();
        let found = looked_within_stuck(
            &stalled,
            vec![stuck.clone(), healthy.clone()],
            waiting(Duration::from_millis(300)),
            move |path| {
                counted.lock().unwrap().push(path.to_path_buf());
                std::thread::sleep(Duration::from_millis(200));
                Some(())
            },
        )
        .await;
        assert!(begun.elapsed() >= Duration::from_millis(290));
        assert!(
            found.contains_key(&healthy),
            "the healthy path is looked at in full"
        );
        assert!(!found.contains_key(&stuck));
        assert_eq!(
            *started.lock().unwrap(),
            std::slice::from_ref(&healthy),
            "no look is started at the stuck path with no time left"
        );
        assert!(stalled.is_stuck(&super::lexically_normalise(&stuck)));
        drop(guard);
    }

    /// A look left behind at its deadline counts as stuck only once it has run for the stuck
    /// threshold: a slow one is not recorded, so a later look at the path still starts, and one
    /// that outruns the threshold is.
    #[tokio::test]
    async fn a_look_counts_as_stuck_only_past_the_stuck_threshold() {
        let stalled = StalledPasses::default();
        let slow = PathBuf::from("/octoboard-test/slow");
        let key = super::lexically_normalise(&slow);
        let looking = Looking {
            stuck_after: Duration::from_secs(1),
            ..Looking::within(Duration::from_millis(50))
        };
        let found = looked_within_stuck(&stalled, vec![slow.clone()], looking, |_| {
            std::thread::sleep(Duration::from_millis(150));
            Some(())
        })
        .await;
        assert!(found.is_empty(), "the answer was not waited for");
        assert!(!stalled.is_stuck(&key), "a slow look is not a stuck one");
        let again = looked_within(&stalled, vec![slow.clone()], PATIENCE, |_| Some(())).await;
        assert!(
            again.contains_key(&slow),
            "a later look at the path still starts"
        );

        let (release, held) = std::sync::mpsc::channel::<()>();
        let held = Mutex::new(held);
        let looking = Looking {
            stuck_after: Duration::from_secs(1),
            ..Looking::within(Duration::from_millis(50))
        };
        looked_within_stuck(&stalled, vec![slow.clone()], looking, move |_| {
            held.lock().unwrap().recv().ok();
            Some(())
        })
        .await;
        assert!(!stalled.is_stuck(&key), "not yet past the threshold");
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(stalled.is_stuck(&key), "past the threshold, still running");
        release.send(()).expect("release");
    }
}
