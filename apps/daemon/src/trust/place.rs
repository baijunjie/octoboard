//! Where a directory really is on the file system, for deciding which trusted directories cover a
//! project.
//!
//! A trusted directory covers a project when the project's directory, once every symbolic link on
//! its path is resolved, is that directory or lies below it. The paths' text cannot answer that: a
//! link inside a trusted directory can lead anywhere, and a spelling through a link or in another
//! letter case can name a directory that is inside. So the project's path is resolved
//! (`canonicalize`), and the directory it leads to and each directory above that one up to the root
//! are told apart by device and inode, as is the directory each trusted path leads to. Containment
//! is then membership: the trusted directory's identity is one of the identities on the project's
//! real path. Comparing identities rather than the resolved text holds whatever the file system
//! does with letter case (`canonicalize` returns the on-disk spelling on APFS, but that is the
//! file system's doing, not a promise), and through a bind mount or a macOS firmlink, which give
//! one directory two real paths.
//!
//! Anything that cannot be resolved covers nothing and is covered by nothing: a path with nothing
//! at it, one that cannot be read, a link that leads nowhere, a file system that reports no inode,
//! or a look that outran its deadline ([`looked_within`]). There is no fallback to the text, which
//! would let a project reached through a link count as inside again.
//!
//! A trusted directory whose real directory is the filesystem root, the home directory or one
//! above it covers nothing ([`too_broad_to_trust`]). None is ever added as one, but a trusted path
//! that is a link can be repointed later, and it must not then cover everything the user has.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::hostfs::{identity, lexically_normalise};
use crate::paths;
use crate::state::StalledPasses;

/// A directory told apart by what it is on the file system: its device and inode.
type Identity = (u64, u64);

/// How long looking at one path on the file system may take. Past it the path counts as one that
/// cannot be resolved, so a directory on a mount that does not answer holds up a trust decision by
/// this much at most, and never pushes a confirmation through.
pub(super) const LOOK_DEADLINE: Duration = Duration::from_secs(5);

/// How many paths are looked at at once, this daemon's figure for a pass of blocking threads.
const LOOKS_AT_ONCE: usize = 4;

#[cfg(test)]
thread_local! {
    /// Lowers [`LOOKS_AT_ONCE`] for a test, on the test's own thread.
    pub(super) static LOOKS_AT_ONCE_OVERRIDE: std::cell::Cell<Option<usize>> =
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
pub(super) struct Place {
    own: Identity,
    within: HashSet<Identity>,
}

impl Place {
    /// The place `path` leads to, or `None` when it is not absolute or cannot be resolved to a
    /// directory. Blocking: it reads the file system.
    pub(super) fn of(path: &Path) -> Option<Place> {
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
        Some(Place { own, within })
    }

    /// Whether this place is `directory` or lies below it.
    pub(super) fn lies_in(&self, directory: &Place) -> bool {
        self.within.contains(&directory.own)
    }

    /// Whether this place is one of `identities`.
    pub(super) fn is_one_of(&self, identities: &HashSet<Identity>) -> bool {
        identities.contains(&self.own)
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
fn followed_directory_identity(path: &Path) -> Option<Identity> {
    directory_identity(&std::fs::metadata(path).ok()?)
}

fn directory_identity(metadata: &std::fs::Metadata) -> Option<Identity> {
    let identity = identity(metadata);
    (metadata.is_dir() && identity.1 != 0).then_some(identity)
}

/// The directories no trusted directory may be, as identities: the filesystem root, and the home
/// directory and every directory above it, as `home` is written and as it really is — the same
/// directories a folder offered for trusting is refused for (`trustable_parent`). With no usable
/// home directory, the root alone. Blocking.
pub(super) fn too_broad_to_trust(home: &Path) -> HashSet<Identity> {
    let mut broad: HashSet<Identity> = followed_directory_identity(Path::new("/"))
        .into_iter()
        .collect();
    if let Some(home) = paths::known_home(home) {
        broad.extend(home.ancestors().filter_map(followed_directory_identity));
        if let Some(place) = Place::of(&home) {
            broad.extend(place.within);
        }
    }
    broad
}

/// What `work` finds at each of `paths`, each on a blocking thread, at most [`LOOKS_AT_ONCE`] at a
/// time, keyed by the path as given. A path is absent from the result when `work` found nothing,
/// its thread panicked, or its look outran `deadline`. A look that outran it is left to finish on
/// its own and recorded in `stalled` until it does; while it runs, a later look at the same path is
/// not started and the path is absent at once, so a stuck mount does not take another thread at
/// every decision. The caller holds no lock of the store while this runs.
pub(super) async fn looked_within<T: Send + 'static>(
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
                    tracing::warn!(path = %path.display(), "looking at a directory for its trust timed out; it counts as unresolved");
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
