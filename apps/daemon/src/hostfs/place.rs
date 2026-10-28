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

/// What `work` finds at each of `paths`, each on a blocking thread, at most [`LOOKS_AT_ONCE`] at a
/// time, keyed by the path as given. A path is absent from the result when `work` found nothing,
/// its thread panicked, or its look outran `deadline`. A look that outran it is left to finish on
/// its own and recorded in `stalled` until it does; while it runs, a later look at the same path is
/// not started and the path is absent at once, so a stuck mount does not take another thread at
/// every decision. The caller holds no lock of the store while this runs.
pub async fn looked_within<T: Send + 'static>(
    stalled: &StalledPasses<PathBuf>,
    paths: Vec<PathBuf>,
    deadline: Duration,
    work: impl Fn(&Path) -> Option<T> + Send + Sync + 'static,
) -> HashMap<PathBuf, T> {
    let work = Arc::new(work);
    let unique: HashSet<PathBuf> = paths.into_iter().collect();
    let mut running = tokio::task::JoinSet::new();
    let mut found = HashMap::new();
    let mut take = |outcome: Option<Look<T>>| match outcome {
        Some(Look::Found(path, value)) => {
            found.insert(path, value);
        }
        Some(Look::TimedOut(key, pass)) => stalled.record(key, pass),
        None => {}
    };
    for path in unique {
        let key = lexically_normalise(&path);
        if stalled.is_stuck(&key) {
            continue;
        }
        if running.len() >= looks_at_once() {
            if let Some(joined) = running.join_next().await {
                take(joined.ok().flatten());
            }
        }
        let work = work.clone();
        running.spawn(async move {
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
                    Some(Look::TimedOut(key, pass))
                }
            }
        });
    }
    while let Some(joined) = running.join_next().await {
        take(joined.ok().flatten());
    }
    found
}

/// How one path's look in [`looked_within`] ended, when it gave anything to keep.
enum Look<T> {
    Found(PathBuf, T),
    /// It outran its deadline; the key it is recorded under while its thread stays stuck.
    TimedOut(PathBuf, std::sync::Weak<()>),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::{looked_within, LOOKS_AT_ONCE_OVERRIDE};
    use crate::state::StalledPasses;
    use crate::test_support::PATIENCE;

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
}
