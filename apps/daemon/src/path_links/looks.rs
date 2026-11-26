//! The deadlines of the looks path links make at the disk, and the seam that lets a test stall
//! one. A look runs on a thread of its own per place (`crate::blocking_pass::within_deadline`), so
//! a mount that hangs holds one thread and is refused at once while it stays stuck: the roots of a
//! project are found within [`ROOTS_DEADLINE`], and the path within [`PROBE_DEADLINE`].

use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use std::time::Duration;

use super::resolve::{place_of, reach, Resolved, Unreached};
use super::{project_roots, roots_key_path, ProjectRoots, TempRoots};

/// How long the paths of one `probe_path_links` request are waited for, all together. A path on a
/// healthy disk answers in well under a millisecond; one on a disk still waking from sleep can take
/// a few seconds. The places that have not answered by then are left out, and their paths are
/// answered `none`, rather than holding one of [`super::PROBE_PERMITS`] on a stalled mount.
pub(super) const PROBE_DEADLINE: Duration = Duration::from_secs(2);

/// How long the roots of a project are waited for when a probe asks for them: finding them runs
/// `git` (a cold lookup is the slow case), and reads the project's directory.
pub(super) const ROOTS_DEADLINE: Duration = Duration::from_secs(5);

/// How long the looks of one request are waited for, and in a test which looks block.
#[derive(Clone)]
pub(super) struct Looks {
    pub probe_deadline: Duration,
    pub roots_deadline: Duration,
    #[cfg(test)]
    held: Option<Arc<HeldLooks>>,
}

impl Looks {
    /// The deadlines in force, read on the request's own thread before any work moves to another.
    pub fn current() -> Self {
        #[cfg(test)]
        if let Some(trial) = LOOKS_OVERRIDE.with_borrow(Clone::clone) {
            return Self {
                probe_deadline: trial.deadline,
                // The real roots deadline stands unless the test holds a roots lookup.
                roots_deadline: if trial.held.holds_roots() {
                    trial.deadline
                } else {
                    ROOTS_DEADLINE
                },
                held: Some(trial.held),
            };
        }
        Self {
            probe_deadline: PROBE_DEADLINE,
            roots_deadline: ROOTS_DEADLINE,
            #[cfg(test)]
            held: None,
        }
    }

    /// Called by a look of `path` on the disk before it starts: in a test, blocks while the look
    /// is held.
    pub fn before_look(&self, #[cfg_attr(not(test), allow(unused_variables))] path: &str) {
        #[cfg(test)]
        if let Some(held) = &self.held {
            held.begin_look(path);
        }
    }

    /// Called by the lookup of the roots of `project` before it starts: in a test, blocks while
    /// the lookup is held.
    fn before_roots(&self, #[cfg_attr(not(test), allow(unused_variables))] project: &str) {
        #[cfg(test)]
        if let Some(held) = &self.held {
            held.begin_roots(project);
        }
    }
}

/// The roots of `project` as `RootsCache` holds them if they are fresh and `fresh` is off, else
/// found within [`ROOTS_DEADLINE`] (a pass of its own, so a stalled project directory holds one
/// thread, and a lookup still stuck is not started again); none when they were not found in time.
pub(super) fn roots_within_deadline(
    state: &Arc<crate::state::AppState>,
    project: &str,
    fresh: bool,
    looks: &Looks,
) -> Option<ProjectRoots> {
    let path = roots_key_path(state.store.get_project(project).ok().flatten().as_ref());
    if !fresh {
        if let Some(roots) = state.path_link_roots.get(project, &path) {
            return Some(roots);
        }
    }
    let groups = HashMap::from([(project.to_owned(), vec![()])]);
    let (owned, id, looks_here) = (Arc::clone(state), project.to_owned(), looks.clone());
    let pass = crate::blocking_pass::within_deadline(
        &state.path_root_lookups,
        groups,
        looks.roots_deadline,
        move |()| {
            looks_here.before_roots(&id);
            Some(project_roots(&owned, &id, fresh))
        },
    );
    for project in &pass.timed_out {
        tracing::warn!(%project, "finding a project's roots for path links timed out");
    }
    pass.found.into_iter().next().map(|(_, roots)| roots)
}

/// What `path` names, looked at within [`PROBE_DEADLINE`] on a thread of the place's own
/// ([`place_of`]), with the project's `roots`; none when the look did not end in time or the place
/// is already known to be stuck. A thread left running is recorded in `path_probes`.
pub(super) fn reach_within_deadline(
    state: &crate::state::AppState,
    looks: &Looks,
    path: &str,
    temp: &TempRoots,
    roots: &ProjectRoots,
) -> Option<Result<Resolved, Unreached>> {
    let home = crate::paths::known_home_dir();
    let place = place_of(path, home.as_deref(), roots);
    let (raw, temp, held, looks) = (path.to_owned(), temp.clone(), roots.clone(), looks.clone());
    let pass = crate::blocking_pass::within_deadline(
        &state.path_probes,
        HashMap::from([(place, vec![()])]),
        looks.probe_deadline,
        move |()| {
            looks.before_look(&raw);
            Some(reach(&raw, home.as_deref(), &temp, &|| held.clone()))
        },
    );
    for place in &pass.timed_out {
        tracing::warn!(place = %place.display(), "looking at a path link timed out");
    }
    pass.found.into_iter().next().map(|(_, found)| found)
}

/// What a test sets to shorten the deadlines to `deadline` and hold some looks.
#[cfg(test)]
#[derive(Clone)]
pub(super) struct LooksTrial {
    pub deadline: Duration,
    pub held: Arc<HeldLooks>,
}

/// The paths whose looks, and the projects whose roots lookups, block until
/// [`HeldLooks::release`], and how many of each began. The roots deadline is shortened only when
/// a roots lookup is held.
#[cfg(test)]
pub(super) struct HeldLooks {
    paths: Vec<String>,
    projects: Vec<String>,
    released: Mutex<bool>,
    release_signal: std::sync::Condvar,
    pub began: std::sync::atomic::AtomicUsize,
    pub roots_began: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl HeldLooks {
    #[cfg(unix)]
    pub fn new(paths: &[&str], projects: &[&str]) -> Arc<Self> {
        let own = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
        Arc::new(Self {
            paths: own(paths),
            projects: own(projects),
            released: Mutex::new(false),
            release_signal: std::sync::Condvar::new(),
            began: std::sync::atomic::AtomicUsize::new(0),
            roots_began: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Lets every held look go on, now and from here on.
    #[cfg(unix)]
    pub fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.release_signal.notify_all();
    }

    fn holds_roots(&self) -> bool {
        !self.projects.is_empty()
    }

    fn begin_look(&self, path: &str) {
        if self.paths.iter().any(|held| held == path) {
            self.wait(&self.began);
        }
    }

    fn begin_roots(&self, project: &str) {
        if self.projects.iter().any(|held| held == project) {
            self.wait(&self.roots_began);
        }
    }

    fn wait(&self, count: &std::sync::atomic::AtomicUsize) {
        // Counted before the wait, so it is final once the look is known to be blocked.
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let released = self.released.lock().unwrap();
        drop(
            self.release_signal
                .wait_while(released, |released| !*released)
                .unwrap(),
        );
    }
}

#[cfg(test)]
thread_local! {
    /// The deadline and held looks [`Looks::current`] takes in place of the real ones, for a test,
    /// on the test's own thread: the request reads it before any work moves to another thread.
    pub(super) static LOOKS_OVERRIDE: std::cell::RefCell<Option<LooksTrial>> =
        const { std::cell::RefCell::new(None) };
}
