//! Watching each git project's refs, so a commit made outside the daemon (an agent session, a
//! terminal) shows in the project's badge without waiting for the client's next poll.
//!
//! Only the files that move a branch header are watched: the git directory's `HEAD`, the common
//! directory's `packed-refs` and everything under its `refs/heads` and `refs/remotes` (a fetch or
//! push from a terminal changes ahead and behind too). A repository that keeps its refs in the
//! reftable format has none of those but a stub: its refs, `HEAD` included, live in `reftable/`
//! stacks, one in the common directory and one per worktree in its git directory, and every
//! change to a stack lands as a rewrite of that stack's `tables.list`. The working tree is never
//! watched, and neither is the rest of the git directory: `git status` itself rewrites the index
//! there, so a watch on it would re-read on its own reads. A change is debounced, then answered
//! with `header::publish_reread`, the local-only read that leaves an overlapping check's activity
//! alone; nothing here goes to the remote.
//!
//! A linked worktree keeps its `HEAD` in its own git directory and everything else in the
//! repository's common directory, so a project is watched through both ([`Dirs`]). A project is
//! one the same way `is_git_repo` says: it has `.git` in its own directory.
//!
//! When the platform reports that it lost events (inotify's queue overflowed, FSEvents asks for a
//! rescan), any ref may have moved unseen, so every watched project is re-read.
//!
//! A watch is on a directory, not on its path: on Linux and Windows it ends, or moves with the
//! directory, when the directory is removed or renamed, and a directory made again at the same
//! path (`git refs migrate` rewriting `refs/`, a worktree removed and added again) is not watched
//! by it. So the watch keeps, for each directory it watches, whether its watch is in place and the
//! identity (inode, file id) of the directory it was set on. A removal or rename of a watched
//! directory is logged, marks its watch lost and re-reads the projects holding it, as does a
//! directory made where a lost watch was and its parent is watched (`refs/`, a `reftable/` stack);
//! that re-read, and every check, sets up again each watch that was lost, that failed (inotify out
//! of watches), or that is on a directory since replaced at the same path. On a file system that
//! reports no file number, a replacement is seen only through the removal it reports.
//!
//! A watch that cannot be set up is logged and tried again at the next check; until then the
//! client's poll still covers that project, as it covers a change the platform never reports:
//! under heavy load macOS delivers file events late or not at all, without flagging any as dropped,
//! and the Windows backend drops a directory's watch on an error it does not recognise without
//! saying so. A session's own git does not wait on either: its hooks report the refs it moves
//! (`super::ref_moves`). A project that is not a repository when it is added, or at daemon start,
//! is picked up by the first check that finds it is one (`status::run_checks`), and one whose
//! layout changes after it is watched (converted to reftable) is watched anew by the next check or
//! watcher-driven re-read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{PathsMut, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::git::layout::is_git_repo;
use crate::platform::fs::{self, Identity, Links};
use crate::project_git::debounced::{Debounced, Timing};
use crate::project_git::header;
use crate::protocol::Project;
use crate::state::AppState;

/// How long refs stay quiet before a project is re-read. A commit or a fetch rewrites several
/// files in a burst (the lock file, the ref, the reflog), and one read answers all of them.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// The two directories a repository's refs live in. They are one for an ordinary repository.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Dirs {
    /// Holds this working tree's `HEAD`.
    git_dir: PathBuf,
    /// Holds `packed-refs` and `refs/`, shared by every worktree of the repository.
    common_dir: PathBuf,
    /// Whether the refs are kept in reftable stacks (`reftable/` in both directories) rather than
    /// as files.
    reftable: bool,
}

impl Dirs {
    /// Reads where `project_dir`'s `.git` points, without running `git`: a directory is the git
    /// directory itself, a file is `gitdir: <path>` (relative to the project) and names the git
    /// directory of a linked worktree, whose `commondir` file names the common one. Both come
    /// back canonical, since a watch reports canonical paths (`/private/var` for `/var`).
    fn resolve(project_dir: &Path) -> Option<Self> {
        let git_dir = crate::git::layout::git_dir(project_dir)?;
        let common_dir = crate::git::layout::common_dir(&git_dir).ok()?;
        let reftable = crate::git::repo_files::uses_reftable(&common_dir);
        Some(Self {
            git_dir: crate::hostfs::canonical(&git_dir).ok()?,
            common_dir: crate::hostfs::canonical(&common_dir).ok()?,
            reftable,
        })
    }

    /// What is watched for this repository: directories read for the files directly in them
    /// (the platform may report more, which [`Dirs::concerns`] drops), the refs tree, and for a
    /// reftable repository the stacks of both directories.
    fn roots(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let linked = self.common_dir != self.git_dir;
        let mut roots = vec![(self.git_dir.clone(), RecursiveMode::NonRecursive)];
        if linked {
            roots.push((self.common_dir.clone(), RecursiveMode::NonRecursive));
        }
        roots.push((self.common_dir.join("refs"), RecursiveMode::Recursive));
        if self.reftable {
            roots.push((
                self.common_dir.join("reftable"),
                RecursiveMode::NonRecursive,
            ));
            if linked {
                roots.push((self.git_dir.join("reftable"), RecursiveMode::NonRecursive));
            }
        }
        roots
    }

    /// Whether a change at `path` can move a branch header. A `.lock` file is a write in
    /// progress, whose result arrives as the file it is renamed to.
    fn concerns(&self, path: &Path) -> bool {
        if path
            .extension()
            .is_some_and(|extension| extension == "lock")
        {
            return false;
        }
        path == self.git_dir.join("HEAD")
            || path == self.common_dir.join("packed-refs")
            || path.starts_with(self.common_dir.join("refs/heads"))
            || path.starts_with(self.common_dir.join("refs/remotes"))
            || path == self.common_dir.join("reftable/tables.list")
            || path == self.git_dir.join("reftable/tables.list")
    }
}

struct Tracked {
    dirs: Dirs,
    /// Every root of `dirs`, each counted in [`Inner::watched`] whether or not its watch is in
    /// place: one that failed or was lost is set up again by a later [`Watch::track`].
    roots: Vec<(PathBuf, RecursiveMode)>,
}

/// What is at a root's path when a project is resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnDisk {
    /// Nothing, or not a directory: there is nothing to watch until one is made there.
    Missing,
    /// A directory, with its identity where the file system tells one.
    Present(Option<Identity>),
}

impl OnDisk {
    fn of(path: &Path) -> Self {
        if path.is_dir() {
            Self::Present(fs::directory_identity(path, Links::Follow))
        } else {
            Self::Missing
        }
    }
}

/// One path the watcher is asked to watch, shared by every project whose roots include it.
struct Root {
    /// How many projects hold it among their roots: linked worktrees of one repository share its
    /// common directory, and one of them going away must not end the others' watch.
    holders: usize,
    /// Whether the platform's watch is in place as far as the daemon knows: false until it is
    /// added, after adding it failed, and after the platform reported the directory removed or
    /// renamed, which on Linux and Windows ends the watch or moves it with the directory.
    armed: bool,
    /// The identity of the directory the watch was set on. A directory replaced at the same path
    /// (`git refs migrate` rewriting `refs/`, a worktree removed and added again) has another, and
    /// a watch on the old one sees nothing of the new.
    identity: Option<Identity>,
    /// Whether the last attempt to watch it failed, so a retry that fails again is not logged as a
    /// new failure.
    failing: bool,
}

impl Root {
    /// Whether the watch on this root has to be set up (again) for the directory now `on_disk`:
    /// it is not in place and there is a directory to watch, or it is on a directory that has
    /// since been replaced.
    fn needs_arming(&self, on_disk: OnDisk) -> bool {
        match on_disk {
            OnDisk::Missing => false,
            OnDisk::Present(now) => !self.armed || self.replaced_by(now),
        }
    }

    fn replaced_by(&self, now: Option<Identity>) -> bool {
        matches!((self.identity, now), (Some(then), Some(now)) if then != now)
    }
}

struct Inner {
    watcher: RecommendedWatcher,
    projects: HashMap<String, Tracked>,
    watched: HashMap<PathBuf, Root>,
}

/// The daemon's ref watcher. Empty until [`start`] arms it, so a state built without it (every
/// test) watches nothing.
#[derive(Default)]
pub struct Watch {
    inner: Mutex<Option<Inner>>,
}

/// A project [`Watch::track`] resolved: what the watch held it with before, its layout now, and
/// what is at each of that layout's roots, in the order of [`Dirs::roots`].
type Resolved<'a> = (&'a Project, Option<Dirs>, Dirs, Vec<OnDisk>);

impl Watch {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Inner>> {
        self.inner.lock().expect("git watch lock poisoned")
    }

    /// Whether the watch is armed and does not already hold `project_id`.
    fn wants(&self, project_id: &str) -> bool {
        self.lock()
            .as_ref()
            .is_some_and(|inner| !inner.projects.contains_key(project_id))
    }

    /// The layout the watch holds `project_id` with, if it holds it, and whether it is settled:
    /// held as `dirs` with every root watched on the directory now there.
    fn held(&self, project_id: &str, dirs: &Dirs, on_disk: &[OnDisk]) -> (Option<Dirs>, bool) {
        let guard = self.lock();
        let Some(inner) = guard.as_ref() else {
            return (None, true);
        };
        let held = inner.held(project_id).cloned();
        let settled = held.as_ref() == Some(dirs) && inner.settled(dirs, on_disk);
        (held, settled)
    }

    /// Starts watching `projects` that are repositories and not watched yet, re-watches those
    /// whose layout changed since they were watched (a repository converted to reftable with
    /// `git refs migrate`), and sets up again each root whose watch failed, was lost, or is on a
    /// directory since replaced at the same path, applying all their paths in one batch: the macOS
    /// backend restarts its whole event stream on every change to what it watches, so one change
    /// per path is quadratic over many projects and loses events in each gap. What each project
    /// resolves to is read before the lock is taken, so what the watch held is noted before that
    /// read, and a project whose entry another call changed meanwhile is left to that call: its
    /// read may be the newer one, and acting on this one could put an old layout back.
    fn track(&self, state: &AppState, projects: &[Project]) {
        if self.lock().is_none() {
            return;
        }
        let resolved: Vec<Resolved<'_>> = projects
            .iter()
            .filter(|project| is_git_repo(Path::new(&project.path)))
            .filter_map(|project| {
                let Some(dirs) = Dirs::resolve(Path::new(&project.path)) else {
                    tracing::debug!(project = %project.id, "the repository's directories could not be resolved");
                    return None;
                };
                let on_disk: Vec<OnDisk> = dirs
                    .roots()
                    .iter()
                    .map(|(path, _)| OnDisk::of(path))
                    .collect();
                let (held, settled) = self.held(&project.id, &dirs, &on_disk);
                (!settled).then_some((project, held, dirs, on_disk))
            })
            .collect();
        if resolved.is_empty() {
            return;
        }
        let mut guard = self.lock();
        let Some(inner) = guard.as_mut() else {
            return;
        };
        // Under the lock, so a project deleted before this runs is not left watched forever: its
        // removal either came first, and is seen here, or finds the entry made below.
        let resolved: Vec<(&Project, bool, Dirs, Vec<OnDisk>)> = resolved
            .into_iter()
            .filter(|(project, held, dirs, on_disk)| {
                inner.held(&project.id) == held.as_ref()
                    && !(held.as_ref() == Some(dirs) && inner.settled(dirs, on_disk))
                    && matches!(state.store.get_project(&project.id), Ok(Some(_)))
            })
            .map(|(project, _, dirs, on_disk)| {
                let relaid = inner.held(&project.id) != Some(&dirs);
                (project, relaid, dirs, on_disk)
            })
            .collect();
        // Applying even an empty batch restarts the macOS event stream, which loses events.
        if resolved.is_empty() {
            return;
        }
        // What a re-watched project watched before is let go only after its new roots are added,
        // so a path both layouts share is never unwatched in between.
        let mut replaced = Vec::new();
        let mut added = Vec::new();
        for (project, relaid, dirs, _) in &resolved {
            if !relaid {
                continue;
            }
            replaced.extend(inner.projects.remove(&project.id));
            let roots = dirs.roots();
            for (path, _) in &roots {
                inner
                    .watched
                    .entry(path.clone())
                    .or_insert(Root {
                        holders: 0,
                        armed: false,
                        identity: None,
                        failing: false,
                    })
                    .holders += 1;
            }
            added.push((
                project.id.clone(),
                Tracked {
                    dirs: dirs.clone(),
                    roots,
                },
            ));
        }
        let mut paths = inner.watcher.paths_mut();
        for (project, _, dirs, on_disk) in &resolved {
            for ((path, mode), on_disk) in dirs.roots().into_iter().zip(on_disk) {
                let root = inner
                    .watched
                    .get_mut(&path)
                    .expect("every root of a held layout is counted");
                if root.needs_arming(*on_disk) {
                    arm(paths.as_mut(), &project.id, &path, mode, root, *on_disk);
                }
            }
        }
        for tracked in replaced {
            for path in release(&mut inner.watched, tracked.roots) {
                // A path that is already gone has nothing left to unwatch.
                paths.remove(&path).ok();
            }
        }
        if let Err(err) = paths.commit() {
            tracing::warn!(%err, "applying the repository watches failed");
        }
        inner.projects.extend(added);
    }

    /// Stops watching a project's repository. Called wherever a project is deleted.
    pub fn untrack(&self, project_id: &str) {
        if let Some(inner) = self.lock().as_mut() {
            inner.untrack(project_id);
        }
    }

    /// Notes that the platform's watch on each of `paths` that is a watched root no longer watches
    /// that path: the directory was removed or renamed, which on Linux and Windows ends the watch
    /// or moves it with the directory, so whatever is at that path afterwards is not watched until
    /// the root is set up again.
    fn lose(&self, paths: &[PathBuf]) {
        let mut guard = self.lock();
        let Some(inner) = guard.as_mut() else {
            return;
        };
        for path in paths {
            if let Some(root) = inner.watched.get_mut(path) {
                if root.armed {
                    root.armed = false;
                    tracing::info!(path = %path.display(), "a watched repository directory was removed or renamed; it is watched again at its next check or re-read");
                }
            }
        }
    }

    /// The projects a report concerns: those a change at any of its paths concerns, those that
    /// watch a directory it removed or one made where a watch they hold is not in place, or every
    /// one after events were lost.
    fn concerned(&self, report: &Report) -> Vec<String> {
        let guard = self.lock();
        let Some(inner) = guard.as_ref() else {
            return Vec::new();
        };
        inner
            .projects
            .iter()
            .filter(|(_, tracked)| match report {
                // A directory made at a root whose watch is not in place is that root back: the
                // re-read sets its watch up.
                Report::Changed(paths) => paths.iter().any(|path| {
                    tracked.dirs.concerns(path)
                        || (tracked.roots.iter().any(|(root, _)| root == path)
                            && inner.watched.get(path).is_some_and(|root| !root.armed))
                }),
                Report::Removed(paths) => paths.iter().any(|path| {
                    tracked.dirs.concerns(path)
                        || tracked.roots.iter().any(|(root, _)| root == path)
                }),
                Report::Lost => true,
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// Sets up the watch on `root` at `path` for the directory now there, in place of any the
/// platform still holds for that path: one on a directory replaced since is removed first, and
/// adding a path the platform's watch already holds would otherwise leave the old one behind
/// (inotify). A failure is logged once until the root is watched again, and whatever part of a
/// recursive watch the platform set up before failing is removed, so the next attempt starts clean
/// and nothing is watched that the daemon does not count.
fn arm(
    paths: &mut dyn PathsMut,
    project_id: &str,
    path: &Path,
    mode: RecursiveMode,
    root: &mut Root,
    on_disk: OnDisk,
) {
    let OnDisk::Present(now) = on_disk else {
        return;
    };
    if root.armed && root.replaced_by(now) {
        tracing::warn!(project = %project_id, path = %path.display(), "a watched repository directory was replaced; watching the new one");
    }
    // A path the platform no longer watches has nothing to remove.
    paths.remove(path).ok();
    match paths.add(path, mode) {
        Ok(()) => {
            if root.failing {
                tracing::info!(project = %project_id, path = %path.display(), "watching the repository's refs again");
            }
            *root = Root {
                armed: true,
                identity: now,
                failing: false,
                ..*root
            };
        }
        Err(err) => {
            paths.remove(path).ok();
            if root.failing {
                tracing::debug!(project = %project_id, path = %path.display(), %err, "watching the repository's refs failed again");
            } else {
                tracing::warn!(project = %project_id, path = %path.display(), %err, "watching the repository's refs failed; it is tried again at the next check");
            }
            *root = Root {
                armed: false,
                identity: None,
                failing: true,
                ..*root
            };
        }
    }
}

impl Inner {
    /// The layout `project_id` is held with, if it is.
    fn held(&self, project_id: &str) -> Option<&Dirs> {
        self.projects.get(project_id).map(|tracked| &tracked.dirs)
    }

    /// Whether every root of `dirs` is counted and needs no arming for what is `on_disk` there.
    fn settled(&self, dirs: &Dirs, on_disk: &[OnDisk]) -> bool {
        dirs.roots()
            .iter()
            .zip(on_disk)
            .all(|((path, _), on_disk)| {
                self.watched
                    .get(path)
                    .is_some_and(|root| !root.needs_arming(*on_disk))
            })
    }

    fn untrack(&mut self, project_id: &str) {
        let Some(tracked) = self.projects.remove(project_id) else {
            return;
        };
        let released = release(&mut self.watched, tracked.roots);
        // Applying a change that changes nothing restarts the macOS event stream, which loses
        // events.
        if released.is_empty() {
            return;
        }
        let mut paths = self.watcher.paths_mut();
        for path in &released {
            // A path that is already gone has nothing left to unwatch.
            paths.remove(path).ok();
        }
        if let Err(err) = paths.commit() {
            tracing::debug!(%err, "applying the repository watches failed");
        }
    }
}

/// Counts one project fewer holding each of `roots`, and returns those no project holds any more,
/// for the caller to unwatch.
fn release(
    watched: &mut HashMap<PathBuf, Root>,
    roots: Vec<(PathBuf, RecursiveMode)>,
) -> Vec<PathBuf> {
    let mut released = Vec::new();
    for (path, _) in roots {
        let Some(root) = watched.get_mut(&path) else {
            continue;
        };
        root.holders -= 1;
        if root.holders == 0 {
            watched.remove(&path);
            released.push(path);
        }
    }
    released
}

/// Whether an event of `kind` can mean the repository changed. An access changes nothing, and
/// Linux's inotify reports every file opened, git's own reads included: the `git status` a change
/// starts would otherwise report itself and start the next one, for ever. A close after writing
/// goes too: the write itself already arrived as a modification or a rename, so the close adds
/// nothing.
fn changes_something(kind: &notify::EventKind) -> bool {
    !matches!(kind, notify::EventKind::Access(_))
}

/// Whether an event of `kind` removes or renames what it names: for a watched root, its watch no
/// longer watching its path on Linux and Windows.
fn removes(kind: &notify::EventKind) -> bool {
    use notify::event::ModifyKind;
    matches!(
        kind,
        notify::EventKind::Remove(_) | notify::EventKind::Modify(ModifyKind::Name(_))
    )
}

/// What the watcher's callback passes on.
#[derive(Debug, PartialEq, Eq)]
enum Report {
    /// These paths may have changed.
    Changed(Vec<PathBuf>),
    /// These paths were removed or renamed, from or to. Any of them may also be a watched root,
    /// whose watch then no longer watches its path ([`Watch::lose`]).
    Removed(Vec<PathBuf>),
    /// The platform lost events, so anything may have changed: inotify's queue overflowed, or
    /// FSEvents asks for a rescan. Such an event names no path (inotify) or a directory to
    /// rescan (FSEvents), never the ref that moved.
    Lost,
}

/// The watcher's callback: sends `sender` the paths of every event that can be a change, and
/// [`Report::Lost`] for one that says events were lost; logs an error the platform reports.
fn forward(
    sender: UnboundedSender<Report>,
) -> impl Fn(notify::Result<notify::Event>) + Send + 'static {
    move |result| match result {
        Ok(event) if event.need_rescan() => {
            sender.send(Report::Lost).ok();
        }
        Ok(event) if removes(&event.kind) => {
            sender.send(Report::Removed(event.paths)).ok();
        }
        Ok(event) if changes_something(&event.kind) => {
            sender.send(Report::Changed(event.paths)).ok();
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(%err, "the repository watch reported an error"),
    }
}

/// Arms the watcher and starts watching every project the store holds. Run once at daemon start,
/// inside the runtime. The projects are added off the runtime, since resolving a project on a
/// stalled volume can block.
pub fn start(state: &Arc<AppState>) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let watcher = notify::recommended_watcher(forward(sender));
    let watcher = match watcher {
        Ok(watcher) => watcher,
        Err(err) => {
            tracing::warn!(%err, "watching repositories is unavailable; git status follows the client's poll");
            return;
        }
    };
    *state.git_watch.lock() = Some(Inner {
        watcher,
        projects: HashMap::new(),
        watched: HashMap::new(),
    });
    tokio::spawn(rereads(state.clone(), receiver));
    let projects = match state.store.list_projects() {
        Ok(projects) => projects,
        Err(err) => {
            tracing::warn!(%err, "listing projects to watch their repositories failed");
            return;
        }
    };
    let state = state.clone();
    tokio::task::spawn_blocking(move || state.git_watch.track(&state, &projects));
}

/// Starts watching a project's repository, if it is one and is not watched yet. Called when a
/// project is created.
pub fn add(state: &Arc<AppState>, project: &Project) {
    if !state.git_watch.wants(&project.id) {
        return;
    }
    let state = state.clone();
    let project = project.clone();
    tokio::task::spawn_blocking(move || watch_now(&state, &project));
}

/// As [`add`], on the calling thread, which must be one that may block, and re-watches a project
/// whose layout changed since it was watched. For a check, which may find a repository the watch
/// does not know or one whose layout changed, and for a watcher-driven re-read.
pub fn watch_now(state: &AppState, project: &Project) {
    state.git_watch.track(state, std::slice::from_ref(project));
}

/// Turns what a watch reports into debounced re-reads: a project is read once its refs have been
/// quiet for [`DEBOUNCE`] ([`super::debounced`]), and one more time straight after a read during
/// which a change arrived. Lost events count as a change to every project. Ends when the
/// watcher, which holds the sending half, is dropped.
async fn rereads(state: Arc<AppState>, mut receiver: UnboundedReceiver<Report>) {
    let scheduler = Arc::new(Debounced::new(Timing {
        debounce: DEBOUNCE,
        max_delay: None,
        owed: Duration::ZERO,
    }));
    while let Some(report) = receiver.recv().await {
        if let Report::Removed(paths) = &report {
            state.git_watch.lose(paths);
        }
        for id in state.git_watch.concerned(&report) {
            scheduler.report(id.clone(), || {
                let state = state.clone();
                move || reread_project(&state, &id)
            });
        }
    }
}

/// Reads the project `id` names, if the store still has it, and re-reads it.
fn reread_project(state: &AppState, id: &str) {
    match state.store.get_project(id) {
        Ok(Some(project)) => reread(state, &project),
        Ok(None) => {}
        Err(err) => {
            tracing::warn!(project = %id, %err, "reading a project to re-read its git status failed")
        }
    }
}

/// One watcher-driven re-read. A project whose `.git` is gone is left alone: `git status` would
/// walk up to an enclosing repository and publish its branch as the project's, and the next check
/// says `repository: false` as it does for any directory that is not one.
fn reread(state: &AppState, project: &Project) {
    if is_git_repo(Path::new(&project.path)) {
        // A conversion to reftable rewrites the refs it was watched for, so the read it starts is
        // the first to see the new layout.
        watch_now(state, project);
        header::publish_reread(state, project);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Event;
    use crate::test_support::{app_state, git, repo_with};
    use tokio::time::Instant;

    /// How long a test waits for the watch to report a ref change. Long: while the
    /// crate's tests run in parallel, macOS's file events go missing for tens of seconds at a time
    /// and arrive up to two minutes late, with no flag saying any were dropped, and an FSEvents
    /// stream of an unrelated process loses them at the same moments. A wait returns as soon as
    /// the change is seen, so a passing run pays nothing for it.
    const FILE_EVENT_PATIENCE: Duration = Duration::from_secs(150);

    /// How long a test lets a ref change go unreported before it touches the ref again.
    const RETOUCH: Duration = Duration::from_secs(2);

    /// Rewrites `repo`'s `main` with the commit it already names, as `git` writes a ref (a lock
    /// file renamed over it), so the watch is told of the same path again and nothing it reads
    /// changes.
    fn retouch_main(repo: &Path) {
        let heads = repo.join(".git/refs/heads");
        let commit = std::fs::read(heads.join("main")).expect("main is a loose ref");
        std::fs::write(heads.join("main.lock"), commit).unwrap();
        std::fs::rename(heads.join("main.lock"), heads.join("main")).unwrap();
    }

    #[test]
    fn an_access_is_not_a_change() {
        use notify::event::{AccessKind, AccessMode};
        for kind in [
            AccessKind::Any,
            AccessKind::Read,
            AccessKind::Open(AccessMode::Any),
            AccessKind::Open(AccessMode::Read),
            AccessKind::Open(AccessMode::Write),
            AccessKind::Open(AccessMode::Execute),
            AccessKind::Open(AccessMode::Other),
            AccessKind::Close(AccessMode::Any),
            AccessKind::Close(AccessMode::Read),
            AccessKind::Close(AccessMode::Write),
            AccessKind::Close(AccessMode::Execute),
            AccessKind::Close(AccessMode::Other),
            AccessKind::Other,
        ] {
            assert!(
                !changes_something(&notify::EventKind::Access(kind)),
                "{kind:?} changes nothing"
            );
        }
    }

    #[test]
    fn every_other_kind_of_event_can_be_a_change() {
        use notify::event::{
            CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind, RenameMode,
        };
        use notify::EventKind;
        for kind in [
            EventKind::Any,
            EventKind::Other,
            EventKind::Create(CreateKind::Any),
            EventKind::Create(CreateKind::File),
            EventKind::Create(CreateKind::Folder),
            EventKind::Create(CreateKind::Other),
            EventKind::Modify(ModifyKind::Any),
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            EventKind::Modify(ModifyKind::Data(DataChange::Size)),
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            EventKind::Modify(ModifyKind::Data(DataChange::Other)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Permissions)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Ownership)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Extended)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Other)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            EventKind::Modify(ModifyKind::Other),
            EventKind::Remove(RemoveKind::Any),
            EventKind::Remove(RemoveKind::File),
            EventKind::Remove(RemoveKind::Folder),
            EventKind::Remove(RemoveKind::Other),
        ] {
            assert!(changes_something(&kind), "{kind:?} can be a change");
        }
    }

    /// What the platform reports for files that are only read must not count as a change, or a
    /// re-read's own reads would start the next one. Inotify, which reports opens, is the platform
    /// where that happens. The reads are followed by a write: events arrive in the order they
    /// happened, so the write's paths must be the first the watch's callback forwards, and the
    /// wait is on that write, not on a window of quiet. A second, raw watcher shows the reads did
    /// reach the platform's watch as accesses of those very files.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn reading_a_repository_forwards_no_change() {
        let repo = repo_with("git-watch-reads", &[(b"a", b"a")]);
        let git_dir = repo.join(".git");
        let head = git_dir.join("HEAD");
        let main = git_dir.join("refs/heads/main");
        let marker = git_dir.join("refs/heads/read-marker");

        let (sender, mut forwarded) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(forward(sender)).expect("a watcher");
        watcher
            .watch(&git_dir, RecursiveMode::Recursive)
            .expect("the git directory is watched");
        let (raw_sender, raw) = std::sync::mpsc::channel();
        let mut raw_watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                raw_sender
                    .send(result.expect("the watch reports no error"))
                    .ok();
            })
            .expect("a raw watcher");
        raw_watcher
            .watch(&git_dir, RecursiveMode::Recursive)
            .expect("the git directory is watched");

        std::fs::read(&head).unwrap();
        std::fs::read(&main).unwrap();
        std::fs::write(&marker, b"0").unwrap();

        let Report::Changed(first) = tokio::time::timeout(FILE_EVENT_PATIENCE, forwarded.recv())
            .await
            .expect("the write after the reads is reported")
            .expect("the watch is alive")
        else {
            panic!("the watch lost events");
        };
        assert!(
            first.contains(&marker),
            "{first:?} was caused by reading, not by a change"
        );

        let (mut head_read, mut main_read) = (false, false);
        loop {
            let event = raw
                .recv_timeout(FILE_EVENT_PATIENCE)
                .expect("the raw watch reports the write too");
            if matches!(event.kind, notify::EventKind::Access(_)) {
                head_read |= event.paths.contains(&head);
                main_read |= event.paths.contains(&main);
            }
            if event.paths.contains(&marker) {
                break;
            }
        }
        assert!(
            head_read && main_read,
            "inotify reports the reads as accesses"
        );
    }

    #[test]
    fn a_linked_worktree_keeps_its_head_apart_from_the_common_refs() {
        let repo = repo_with("git-watch-linked", &[(b"a", b"a")]);
        let linked = repo.join("linked");
        git(&repo, &["worktree", "add", "-q", linked.to_str().unwrap()]);

        let main = Dirs::resolve(&repo).expect("an ordinary repository resolves");
        assert_eq!(main.git_dir, main.common_dir);
        let linked = Dirs::resolve(&linked).expect("a linked worktree resolves");
        assert_eq!(linked.common_dir, main.common_dir);
        assert_eq!(
            linked.git_dir,
            main.common_dir.join("worktrees").join("linked")
        );

        assert!(linked.concerns(&linked.git_dir.join("HEAD")));
        assert!(!linked.concerns(&main.git_dir.join("HEAD")));
        assert!(linked.concerns(&linked.common_dir.join("refs/heads/feature/x")));
        assert!(linked.concerns(&linked.common_dir.join("refs/remotes/origin/main")));
        assert!(linked.concerns(&linked.common_dir.join("packed-refs")));
        assert!(!linked.concerns(&linked.common_dir.join("refs/heads/main.lock")));
        assert!(!linked.concerns(&linked.git_dir.join("index")));
        assert!(!linked.concerns(&linked.common_dir.join("refs/tags/v1")));
    }

    #[tokio::test]
    async fn a_commit_made_outside_the_daemon_moves_the_ahead_count() {
        let repo = repo_with("git-watch-commit-repo", &[(b"a", b"a")]);
        a_commit_made_outside_is_noticed("git-watch-commit", &repo, retouch_main).await;
    }

    #[tokio::test]
    async fn a_commit_made_outside_the_daemon_is_noticed_in_a_reftable_repository() {
        let Some(repo) = reftable_repo_with("git-watch-reftable-commit-repo") else {
            return;
        };
        a_commit_made_outside_is_noticed("git-watch-reftable-commit", &repo, retouch_tables).await;
    }

    /// Watches `repo` as the daemon does, commits in it outside the daemon, and waits for a
    /// re-read that reports the commit. The platform may never report the commit's own writes
    /// (see `FILE_EVENT_PATIENCE`), so `retouch` rewrites what the commit wrote, unchanged, until
    /// one does.
    async fn a_commit_made_outside_is_noticed(label: &str, repo: &Path, retouch: fn(&Path)) {
        let (state, dir) = app_state(label);
        let remote = dir.join("remote.git");
        git(&dir, &["init", "-q", "--bare", remote.to_str().unwrap()]);
        git(repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(repo, &["push", "-q", "-u", "origin", "main"]);
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        crate::test_support::insert_project(&state, repo);
        let mut events = state.subscribe();
        start(&state);
        // The projects are watched off the runtime; a commit made before that would be missed.
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !state
            .git_watch
            .lock()
            .as_ref()
            .is_some_and(|inner| inner.projects.contains_key("p1"))
        {
            assert!(Instant::now() < deadline, "the project is watched");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        git(repo, &["commit", "-q", "--allow-empty", "-m", "outside"]);
        wait_for_the_commit_to_be_reported(&mut events, repo, retouch).await;
    }

    /// Waits on `events` for a `project_git_status` that says `repo` is one commit ahead of its
    /// upstream, as a commit made outside the daemon makes it. The platform may never report the
    /// commit's own writes (see `FILE_EVENT_PATIENCE`), so `retouch` rewrites what the commit
    /// wrote, unchanged, until one does.
    async fn wait_for_the_commit_to_be_reported(
        events: &mut tokio::sync::broadcast::Receiver<Event>,
        repo: &Path,
        retouch: fn(&Path),
    ) {
        let deadline = Instant::now() + FILE_EVENT_PATIENCE;
        let mut retouch_at = Instant::now() + RETOUCH;
        loop {
            match tokio::time::timeout_at(retouch_at.min(deadline), events.recv()).await {
                Ok(event) => {
                    if let Event::ProjectGitStatus { status } = event.unwrap() {
                        if status.ahead == 1 {
                            assert_eq!(status.upstream.as_deref(), Some("origin/main"));
                            break;
                        }
                    }
                }
                Err(_) => {
                    assert!(
                        Instant::now() < deadline,
                        "a project_git_status with the commit arrives"
                    );
                    retouch(repo);
                    retouch_at = Instant::now() + RETOUCH;
                }
            }
        }
    }

    /// A repository created with the reftable ref format, with one commit holding a file. `None`
    /// where `git` does not know the format (before Git 2.45), and the test is skipped
    /// ([`init_reftable`](crate::test_support::init_reftable)).
    fn reftable_repo_with(label: &str) -> Option<crate::test_support::ScratchDir> {
        let dir = crate::test_support::ScratchDir::new(label);
        if !crate::test_support::init_reftable(&dir) {
            return None;
        }
        std::fs::write(dir.join("a"), b"a").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "fixture"]);
        Some(dir)
    }

    /// Rewrites `repo`'s `reftable/tables.list` with the content it has, as `git` writes it (a lock
    /// file renamed over it), so the watch is told of the same path again and nothing it reads
    /// changes. Such a repository has no loose ref file to rewrite.
    fn retouch_tables(repo: &Path) {
        let stack = repo.join(".git/reftable");
        let list = std::fs::read(stack.join("tables.list")).expect("a stack lists its tables");
        std::fs::write(stack.join("tables.list.lock"), list).unwrap();
        std::fs::rename(stack.join("tables.list.lock"), stack.join("tables.list")).unwrap();
    }

    /// A [`Watch`] armed with a watcher that watches nothing, tracking each of `projects`
    /// (`(id, repository directory)`) with its roots counted as watched on the directories there
    /// now, without watching them, so no real file event can reach it and a check finds nothing
    /// to set up.
    fn watch_tracking(projects: &[(&str, &Path)]) -> Watch {
        let watcher =
            notify::recommended_watcher(|_: notify::Result<notify::Event>| {}).expect("a watcher");
        let mut watched: HashMap<PathBuf, Root> = HashMap::new();
        let projects = projects
            .iter()
            .map(|(id, path)| {
                let dirs = Dirs::resolve(path).expect("a repository resolves");
                let roots = dirs.roots();
                for (root, _) in &roots {
                    let identity = identity_at(root);
                    watched
                        .entry(root.clone())
                        .or_insert(Root {
                            holders: 0,
                            armed: true,
                            identity,
                            failing: false,
                        })
                        .holders += 1;
                }
                (id.to_string(), Tracked { dirs, roots })
            })
            .collect();
        Watch {
            inner: Mutex::new(Some(Inner {
                watcher,
                projects,
                watched,
            })),
        }
    }

    /// What the watcher's callback sends for `result`, if anything.
    fn forwarded(result: notify::Result<notify::Event>) -> Option<Report> {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        forward(sender)(result);
        receiver.try_recv().ok()
    }

    fn rescan(kind: notify::EventKind) -> notify::Event {
        notify::Event::new(kind).set_flag(notify::event::Flag::Rescan)
    }

    #[test]
    fn only_a_reftable_repository_is_resolved_as_one() {
        let files = repo_with("git-watch-files-backend", &[(b"a", b"a")]);
        assert!(!Dirs::resolve(&files).expect("a repository").reftable);

        let Some(reftable) = reftable_repo_with("git-watch-reftable-backend") else {
            return;
        };
        assert!(reftable.join(".git/reftable").is_dir());
        assert!(Dirs::resolve(&reftable).expect("a repository").reftable);
    }

    #[test]
    fn a_reftable_repository_watches_its_stack_and_a_files_repository_does_not() {
        let files = repo_with("git-watch-roots-files", &[(b"a", b"a")]);
        let dirs = Dirs::resolve(&files).unwrap();
        assert!(
            dirs.roots()
                .iter()
                .all(|(path, _)| !path.components().any(|part| part.as_os_str() == "reftable")),
            "a files repository has no reftable stack: {:?}",
            dirs.roots()
        );

        let Some(repo) = reftable_repo_with("git-watch-roots-reftable") else {
            return;
        };
        let dirs = Dirs::resolve(&repo).unwrap();
        let roots = dirs.roots();
        assert!(roots.contains(&(
            dirs.common_dir.join("reftable"),
            RecursiveMode::NonRecursive
        )));
        assert_eq!(
            roots
                .iter()
                .filter(|(path, _)| path.ends_with("reftable"))
                .count(),
            1,
            "an ordinary repository's one stack is watched once: {roots:?}"
        );
    }

    #[test]
    fn a_linked_worktree_of_a_reftable_repository_watches_both_stacks() {
        let Some(repo) = reftable_repo_with("git-watch-roots-linked-reftable") else {
            return;
        };
        let linked = repo.join("linked");
        git(&repo, &["worktree", "add", "-q", linked.to_str().unwrap()]);

        let dirs = Dirs::resolve(&linked).expect("a linked worktree resolves");
        assert!(dirs.reftable);
        assert_ne!(dirs.git_dir, dirs.common_dir);
        let roots = dirs.roots();
        for stack in [
            dirs.common_dir.join("reftable"),
            dirs.git_dir.join("reftable"),
        ] {
            assert!(
                stack.is_dir(),
                "git keeps a stack at {} for the repository",
                stack.display()
            );
            assert!(
                roots.contains(&(stack.clone(), RecursiveMode::NonRecursive)),
                "{} is watched: {roots:?}",
                stack.display()
            );
        }
    }

    #[test]
    fn only_the_stack_list_of_a_reftable_repository_is_a_ref_change() {
        let Some(repo) = reftable_repo_with("git-watch-concerns-reftable") else {
            return;
        };
        let linked = repo.join("linked");
        git(&repo, &["worktree", "add", "-q", linked.to_str().unwrap()]);
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                repo.join("other").to_str().unwrap(),
            ],
        );

        let dirs = Dirs::resolve(&linked).unwrap();
        let other = Dirs::resolve(&repo.join("other")).unwrap();
        let common_stack = dirs.common_dir.join("reftable");
        let own_stack = dirs.git_dir.join("reftable");

        assert!(dirs.concerns(&common_stack.join("tables.list")));
        assert!(dirs.concerns(&own_stack.join("tables.list")));
        // The lock file is the half-written list: the rename that completes it is the change.
        assert!(!dirs.concerns(&common_stack.join("tables.list.lock")));
        assert!(!dirs.concerns(&own_stack.join("tables.list.lock")));
        // The tables themselves are written before the list names them.
        let table = std::fs::read_dir(&common_stack)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|ext| ext == "ref"))
            .expect("the common stack holds a table");
        assert!(!dirs.concerns(&table));
        assert!(!dirs.concerns(&common_stack));
        // Another worktree's HEAD lives in its own stack.
        assert_ne!(other.git_dir, dirs.git_dir);
        assert!(!dirs.concerns(&other.git_dir.join("reftable/tables.list")));
        assert!(other.concerns(&other.git_dir.join("reftable/tables.list")));
    }

    #[test]
    fn a_lost_event_is_forwarded_as_lost_whatever_its_kind_or_paths() {
        use notify::event::AccessKind;
        use notify::EventKind;
        // The platforms report it as `Other`; an access must not hide it either.
        let kinds = [EventKind::Other, EventKind::Access(AccessKind::Any)];
        for kind in kinds {
            assert_eq!(
                forwarded(Ok(rescan(kind))),
                Some(Report::Lost),
                "{kind:?} without a path"
            );
            assert_eq!(
                forwarded(Ok(rescan(kind).add_path(PathBuf::from("/repo/.git/refs")))),
                Some(Report::Lost),
                "{kind:?} with a path"
            );
        }
    }

    #[test]
    fn a_change_is_forwarded_with_its_paths_and_an_access_is_not_forwarded() {
        use notify::event::{AccessKind, CreateKind};
        use notify::EventKind;
        let paths = vec![
            PathBuf::from("/repo/.git/HEAD"),
            PathBuf::from("/repo/.git/x"),
        ];
        let mut change = notify::Event::new(EventKind::Create(CreateKind::File));
        change.paths = paths.clone();
        assert_eq!(forwarded(Ok(change)), Some(Report::Changed(paths)));

        let access = notify::Event::new(EventKind::Access(AccessKind::Read))
            .add_path(PathBuf::from("/repo/.git/HEAD"));
        assert_eq!(forwarded(Ok(access)), None);
    }

    #[test]
    fn lost_events_concern_every_tracked_project_and_a_change_only_its_own() {
        let first = repo_with("git-watch-concerned-first", &[(b"a", b"a")]);
        let second = repo_with("git-watch-concerned-second", &[(b"a", b"a")]);
        let watch = watch_tracking(&[("first", &first), ("second", &second)]);
        let sorted = |mut ids: Vec<String>| {
            ids.sort();
            ids
        };

        assert_eq!(sorted(watch.concerned(&Report::Lost)), ["first", "second"]);

        let first_head = Dirs::resolve(&first).unwrap().git_dir.join("HEAD");
        assert_eq!(
            watch.concerned(&Report::Changed(vec![first_head.clone()])),
            ["first"]
        );
        let second_head = Dirs::resolve(&second).unwrap().git_dir.join("HEAD");
        assert_eq!(
            sorted(watch.concerned(&Report::Changed(vec![first_head, second_head]))),
            ["first", "second"]
        );
        assert!(watch
            .concerned(&Report::Changed(vec![first.join("a")]))
            .is_empty());
        assert!(watch.concerned(&Report::Changed(Vec::new())).is_empty());
    }

    /// A commit nobody told the watch about is noticed once the platform says it lost events:
    /// nothing is watched here, so the re-read can only come from the `Lost` the callback sent.
    #[tokio::test]
    async fn lost_events_make_a_tracked_project_be_read_again() {
        let (state, dir) = app_state("git-watch-lost");
        let remote = dir.join("remote.git");
        git(&dir, &["init", "-q", "--bare", remote.to_str().unwrap()]);
        let repo = repo_with("git-watch-lost-repo", &[(b"a", b"a")]);
        git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&repo, &["push", "-q", "-u", "origin", "main"]);
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        crate::test_support::insert_project(&state, &repo);
        *state.git_watch.lock() = watch_tracking(&[("p1", &repo)]).lock().take();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let callback = forward(sender);
        tokio::spawn(rereads(state.clone(), receiver));
        let mut events = state.subscribe();

        git(&repo, &["commit", "-q", "--allow-empty", "-m", "outside"]);
        callback(Ok(rescan(notify::EventKind::Other)));

        tokio::time::timeout(crate::test_support::PATIENCE, async {
            loop {
                if let Event::ProjectGitStatus { status } = events.recv().await.unwrap() {
                    if status.ahead == 1 {
                        break;
                    }
                }
            }
        })
        .await
        .expect("a project_git_status with the commit arrives");
    }

    /// An `AppState` holding the project `p1` at `repo`, whose watch is armed as [`start`] arms
    /// it, with a real watcher that adds the paths it is given and drops every event it reports,
    /// and no project tracked yet.
    fn state_watching(
        label: &str,
        repo: &Path,
    ) -> (Arc<AppState>, crate::test_support::ScratchDir, Project) {
        let (state, dir) = app_state(label);
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let project = crate::test_support::insert_project(&state, repo);
        let watcher =
            notify::recommended_watcher(|_: notify::Result<notify::Event>| {}).expect("a watcher");
        *state.git_watch.lock() = Some(Inner {
            watcher,
            projects: HashMap::new(),
            watched: HashMap::new(),
        });
        (state, dir, project)
    }

    /// The project `p2` at `path`, inserted into `state`'s store next to `first`.
    fn insert_second_project(state: &AppState, first: &Project, path: &Path) -> Project {
        let mut project = first.clone();
        project.id = "p2".to_string();
        project.path = path.to_string_lossy().into_owned();
        state.store.insert_project(&project).unwrap();
        project
    }

    /// Converts the repository at `repo` to the reftable ref format in place. `false` where `git`
    /// cannot be relied on to migrate a repository with reflogs, as every fixture has: reflog
    /// migration came in Git 2.48 and `--no-reflog`, probed here, in 2.49, so 2.48 skips too
    /// ([`skip_without_reftable`](crate::test_support::skip_without_reftable)).
    fn migrated_to_reftable(repo: &Path) -> bool {
        let known = crate::test_support::fixture_git(repo)
            .args(["refs", "migrate", "-h"])
            .output()
            .unwrap();
        // `-h` prints the usage to stdout (exiting 129) where the subcommand exists; an older `git`
        // knows no `refs` or no `migrate`, or predates `--no-reflog` (Git 2.49).
        if !String::from_utf8_lossy(&known.stdout).contains("--no-reflog") {
            crate::test_support::skip_without_reftable(
                repo,
                "migrate a repository with reflogs to reftable (Git 2.49)",
            );
            return false;
        }
        let migrate = crate::test_support::fixture_git(repo)
            .args(["refs", "migrate", "--ref-format=reftable"])
            .output()
            .unwrap();
        assert!(
            migrate.status.success(),
            "git refs migrate failed: {}",
            String::from_utf8_lossy(&migrate.stderr)
        );
        true
    }

    /// The roots the watch holds for `id`, after checking that it holds the project as `repo` is
    /// laid out on disk now.
    fn roots_held_as_on_disk(
        watch: &Watch,
        id: &str,
        repo: &Path,
    ) -> Vec<(PathBuf, RecursiveMode)> {
        let guard = watch.lock();
        let tracked = guard
            .as_ref()
            .expect("the watch is armed")
            .projects
            .get(id)
            .unwrap_or_else(|| panic!("{id} is tracked"));
        assert_eq!(
            tracked.dirs,
            Dirs::resolve(repo).expect("a repository resolves"),
            "{id} is held as its repository is laid out now"
        );
        tracked.roots.clone()
    }

    /// How many projects watch each path, as the watch counts it.
    fn watched_counts(watch: &Watch) -> HashMap<PathBuf, usize> {
        watch
            .lock()
            .as_ref()
            .expect("the watch is armed")
            .watched
            .iter()
            .map(|(path, root)| (path.clone(), root.holders))
            .collect()
    }

    /// How many of the tracked projects hold each path among their roots: what the watch's own
    /// count must come to, whatever order the projects were tracked and re-watched in.
    fn held_counts(watch: &Watch) -> HashMap<PathBuf, usize> {
        let guard = watch.lock();
        let mut counts = HashMap::new();
        for tracked in guard
            .as_ref()
            .expect("the watch is armed")
            .projects
            .values()
        {
            for (path, _) in &tracked.roots {
                *counts.entry(path.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    #[tokio::test]
    async fn a_repository_converted_to_reftable_after_it_is_tracked_is_watched_by_its_stack() {
        let repo = repo_with("git-watch-migrate-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-migrate", &repo);
        let common = Dirs::resolve(&repo).unwrap().common_dir;
        let stack = (common.join("reftable"), RecursiveMode::NonRecursive);

        watch_now(&state, &project);
        let before = roots_held_as_on_disk(&state.git_watch, "p1", &repo);
        assert!(!before.contains(&stack), "a files repository has no stack");
        assert_eq!(
            watched_counts(&state.git_watch),
            held_counts(&state.git_watch)
        );
        assert!(!before.is_empty());

        if !migrated_to_reftable(&repo) {
            return;
        }
        watch_now(&state, &project);

        let after = roots_held_as_on_disk(&state.git_watch, "p1", &repo);
        assert!(
            Dirs::resolve(&repo).unwrap().reftable,
            "the repository is reftable now"
        );
        assert!(after.contains(&stack), "the stack is watched: {after:?}");
        for root in &before {
            assert!(after.contains(root), "{root:?} is still watched");
        }
        let watched = watched_counts(&state.git_watch);
        assert_eq!(watched, held_counts(&state.git_watch));
        // Each path both layouts share is held once, not dropped by the release of the old
        // layout and not counted twice by the add of the new one.
        assert_eq!(watched.get(&stack.0), Some(&1));
        for (path, _) in &before {
            assert_eq!(watched.get(path), Some(&1), "{}", path.display());
        }
        assert_eq!(watched.len(), after.len());
    }

    #[tokio::test]
    async fn a_watcher_driven_re_read_picks_up_a_conversion_to_reftable() {
        let repo = repo_with("git-watch-reread-migrate-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-reread-migrate", &repo);
        let stack = (
            Dirs::resolve(&repo).unwrap().common_dir.join("reftable"),
            RecursiveMode::NonRecursive,
        );
        watch_now(&state, &project);
        assert!(!roots_held_as_on_disk(&state.git_watch, "p1", &repo).contains(&stack));

        if !migrated_to_reftable(&repo) {
            return;
        }
        reread(&state, &project);

        let after = roots_held_as_on_disk(&state.git_watch, "p1", &repo);
        assert!(Dirs::resolve(&repo).unwrap().reftable);
        assert!(after.contains(&stack), "the stack is watched: {after:?}");
        assert_eq!(
            watched_counts(&state.git_watch),
            held_counts(&state.git_watch)
        );
        assert_eq!(watched_counts(&state.git_watch).get(&stack.0), Some(&1));
    }

    #[tokio::test]
    async fn a_re_read_watches_a_project_the_watch_does_not_hold() {
        let repo = repo_with("git-watch-reread-new-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-reread-new", &repo);

        reread(&state, &project);

        roots_held_as_on_disk(&state.git_watch, "p1", &repo);
        assert_eq!(
            watched_counts(&state.git_watch),
            held_counts(&state.git_watch)
        );
        assert!(!watched_counts(&state.git_watch).is_empty());
    }

    #[tokio::test]
    async fn a_project_whose_layout_is_unchanged_is_left_as_it_is() {
        let repo = repo_with("git-watch-unchanged-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-unchanged", &repo);
        watch_now(&state, &project);
        let roots = roots_held_as_on_disk(&state.git_watch, "p1", &repo);
        let watched = watched_counts(&state.git_watch);

        watch_now(&state, &project);
        watch_now(&state, &project);

        assert_eq!(roots_held_as_on_disk(&state.git_watch, "p1", &repo), roots);
        assert_eq!(watched_counts(&state.git_watch), watched);
        assert!(watched.values().all(|count| *count == 1));
        assert_eq!(state.git_watch.lock().as_ref().unwrap().projects.len(), 1);
    }

    /// A repository is read as reftable by the `reftable` directory in its common directory (a
    /// linked worktree's own stack is made too, since a root that is missing cannot be watched),
    /// and `git refs migrate` refuses a repository that has worktrees, so the tests of paths shared
    /// between projects make that directory themselves: nothing here runs `git` after it.
    #[tokio::test]
    async fn a_path_a_linked_worktree_shares_with_the_main_one_is_not_unwatched_by_a_re_watch() {
        let repo = repo_with("git-watch-shared-repo", &[(b"a", b"a")]);
        let linked_path = repo.join("linked");
        git(
            &repo,
            &["worktree", "add", "-q", linked_path.to_str().unwrap()],
        );
        let (state, _dir, main) = state_watching("git-watch-shared", &repo);
        let linked = insert_second_project(&state, &main, &linked_path);
        let watch = &state.git_watch;
        watch.track(&state, &[main.clone(), linked.clone()]);

        let common = Dirs::resolve(&repo).unwrap().common_dir;
        let linked_git_dir = Dirs::resolve(&linked_path).unwrap().git_dir;
        let refs = common.join("refs");
        let common_stack = common.join("reftable");
        let linked_stack = linked_git_dir.join("reftable");
        let main_roots = roots_held_as_on_disk(watch, "p1", &repo);
        roots_held_as_on_disk(watch, "p2", &linked_path);
        let shared = watched_counts(watch);
        assert_eq!(shared, held_counts(watch));
        assert_eq!(shared.get(&common), Some(&2));
        assert_eq!(shared.get(&refs), Some(&2));
        assert_eq!(shared.get(&linked_git_dir), Some(&1));

        std::fs::create_dir(&common_stack).unwrap();
        std::fs::create_dir(&linked_stack).unwrap();
        watch_now(&state, &linked);

        // The linked worktree is held as reftable now; the main one, not asked again, is not.
        roots_held_as_on_disk(watch, "p2", &linked_path);
        {
            let guard = watch.lock();
            let held = &guard.as_ref().unwrap().projects["p1"];
            assert!(!held.dirs.reftable);
            assert_eq!(held.roots, main_roots);
        }
        let watched = watched_counts(watch);
        assert_eq!(watched, held_counts(watch));
        assert_eq!(
            watched.get(&common),
            Some(&2),
            "the common directory is still shared"
        );
        assert_eq!(
            watched.get(&refs),
            Some(&2),
            "the refs tree is still shared"
        );
        assert_eq!(watched.get(&linked_git_dir), Some(&1));
        assert_eq!(watched.get(&common_stack), Some(&1));
        assert_eq!(watched.get(&linked_stack), Some(&1));

        // The main worktree's turn: it joins the stack the linked one already watches.
        watch_now(&state, &main);
        roots_held_as_on_disk(watch, "p1", &repo);
        let watched = watched_counts(watch);
        assert_eq!(watched, held_counts(watch));
        assert_eq!(watched.get(&common), Some(&2));
        assert_eq!(watched.get(&refs), Some(&2));
        assert_eq!(watched.get(&common_stack), Some(&2));
        assert_eq!(watched.get(&linked_git_dir), Some(&1));
        assert_eq!(watched.get(&linked_stack), Some(&1));
    }

    #[tokio::test]
    async fn untracking_one_of_two_worktrees_keeps_the_paths_the_other_still_watches() {
        let repo = repo_with("git-watch-untrack-repo", &[(b"a", b"a")]);
        let linked_path = repo.join("linked");
        git(
            &repo,
            &["worktree", "add", "-q", linked_path.to_str().unwrap()],
        );
        let (state, _dir, main) = state_watching("git-watch-untrack", &repo);
        let linked = insert_second_project(&state, &main, &linked_path);
        let watch = &state.git_watch;
        watch.track(&state, &[main, linked]);
        let main_roots = roots_held_as_on_disk(watch, "p1", &repo);
        let linked_git_dir = Dirs::resolve(&linked_path).unwrap().git_dir;

        watch.untrack("p2");

        let watched = watched_counts(watch);
        assert_eq!(watched, held_counts(watch));
        assert_eq!(watched.len(), main_roots.len());
        assert!(!watched.contains_key(&linked_git_dir));
        assert!(watched.values().all(|count| *count == 1));
    }

    #[tokio::test]
    async fn a_path_only_the_old_layout_had_is_unwatched_by_a_re_watch() {
        let repo = repo_with("git-watch-reverted-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-reverted", &repo);
        let common = Dirs::resolve(&repo).unwrap().common_dir;
        let stack = common.join("reftable");
        std::fs::create_dir(&stack).unwrap();
        watch_now(&state, &project);
        assert_eq!(watched_counts(&state.git_watch).get(&stack), Some(&1));

        std::fs::remove_dir(&stack).unwrap();
        watch_now(&state, &project);

        let roots = roots_held_as_on_disk(&state.git_watch, "p1", &repo);
        assert!(roots.iter().all(|(path, _)| *path != stack));
        let watched = watched_counts(&state.git_watch);
        assert!(!watched.contains_key(&stack), "no project watches it now");
        assert_eq!(watched, held_counts(&state.git_watch));
        assert_eq!(watched.get(&common), Some(&1));
        assert_eq!(watched.get(&common.join("refs")), Some(&1));
    }

    #[test]
    fn releasing_roots_counts_one_project_fewer_and_returns_the_paths_no_project_watches() {
        let non_recursive = RecursiveMode::NonRecursive;
        let recursive = RecursiveMode::Recursive;
        let held_by = |holders| Root {
            holders,
            armed: true,
            identity: None,
            failing: false,
        };
        let mut watched = HashMap::from([
            (PathBuf::from("/repo/shared"), held_by(2)),
            (PathBuf::from("/repo/own"), held_by(1)),
            (PathBuf::from("/repo/other"), held_by(3)),
        ]);

        let released = release(
            &mut watched,
            vec![
                (PathBuf::from("/repo/shared"), non_recursive),
                (PathBuf::from("/repo/own"), recursive),
                (PathBuf::from("/repo/never-watched"), non_recursive),
            ],
        );

        assert_eq!(released, [PathBuf::from("/repo/own")]);
        let holders: HashMap<PathBuf, usize> = watched
            .iter()
            .map(|(path, root)| (path.clone(), root.holders))
            .collect();
        assert_eq!(
            holders,
            HashMap::from([
                (PathBuf::from("/repo/shared"), 1),
                (PathBuf::from("/repo/other"), 3),
            ])
        );
        assert!(release(&mut watched, Vec::new()).is_empty());
    }

    /// What the watch holds of one root, to compare before and after a [`Watch::track`].
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RootState {
        holders: usize,
        armed: bool,
        identity: Option<Identity>,
        failing: bool,
    }

    /// The state of the root at `path`; the root must be counted.
    fn root_state(watch: &Watch, path: &Path) -> RootState {
        let guard = watch.lock();
        let root = guard
            .as_ref()
            .expect("the watch is armed")
            .watched
            .get(path)
            .unwrap_or_else(|| panic!("{} is counted", path.display()));
        RootState {
            holders: root.holders,
            armed: root.armed,
            identity: root.identity,
            failing: root.failing,
        }
    }

    /// Whether the watch holds a project and every root it counts is armed.
    fn all_roots_armed(watch: &Watch) -> bool {
        let guard = watch.lock();
        let Some(inner) = guard.as_ref() else {
            return false;
        };
        !inner.projects.is_empty()
            && !inner.watched.is_empty()
            && inner.watched.values().all(|root| root.armed)
    }

    /// The identity of the directory at `path` now.
    fn identity_at(path: &Path) -> Option<Identity> {
        fs::directory_identity(path, Links::Follow)
    }

    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// Replaces the directory at `dir` with a new one holding the same content, as `git refs
    /// migrate` rewrites `refs/`. The old one is moved aside, not removed: a removed directory's
    /// inode may be handed to the new one at once, and the two would then be indistinguishable.
    fn replace_dir(dir: &Path) {
        let old = dir.with_extension("old");
        std::fs::rename(dir, &old).unwrap();
        copy_tree(&old, dir);
    }

    /// As [`state_watching`], with the project's repository pushed to an upstream, so a commit made
    /// outside moves its ahead count.
    fn state_with_upstream(
        label: &str,
        repo: &Path,
    ) -> (Arc<AppState>, crate::test_support::ScratchDir, Project) {
        let (state, dir, project) = state_watching(label, repo);
        let remote = dir.join("remote.git");
        git(&dir, &["init", "-q", "--bare", remote.to_str().unwrap()]);
        git(repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(repo, &["push", "-q", "-u", "origin", "main"]);
        (state, dir, project)
    }

    fn sorted(mut ids: Vec<String>) -> Vec<String> {
        ids.sort();
        ids
    }

    /// A [`PathsMut`] that records what it is asked and fails as it is told to.
    struct RecordingPaths {
        calls: Vec<PathCall>,
        remove_fails: bool,
        add_fails: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum PathCall {
        Remove(PathBuf),
        Add(PathBuf, RecursiveMode),
    }

    impl RecordingPaths {
        fn new() -> Self {
            Self {
                calls: Vec::new(),
                remove_fails: false,
                add_fails: false,
            }
        }
    }

    impl PathsMut for RecordingPaths {
        fn add(&mut self, path: &Path, recursive_mode: RecursiveMode) -> notify::Result<()> {
            self.calls
                .push(PathCall::Add(path.to_path_buf(), recursive_mode));
            if self.add_fails {
                Err(notify::Error::generic("out of watches"))
            } else {
                Ok(())
            }
        }

        fn remove(&mut self, path: &Path) -> notify::Result<()> {
            self.calls.push(PathCall::Remove(path.to_path_buf()));
            if self.remove_fails {
                Err(notify::Error::generic("not watched"))
            } else {
                Ok(())
            }
        }

        fn commit(self: Box<Self>) -> notify::Result<()> {
            Ok(())
        }
    }

    fn seen(root: &Root) -> RootState {
        state_of(root.holders, root.armed, root.identity, root.failing)
    }

    fn state_of(
        holders: usize,
        armed: bool,
        identity: Option<Identity>,
        failing: bool,
    ) -> RootState {
        RootState {
            holders,
            armed,
            identity,
            failing,
        }
    }

    fn root_of(holders: usize, armed: bool, identity: Option<Identity>, failing: bool) -> Root {
        Root {
            holders,
            armed,
            identity,
            failing,
        }
    }

    #[test]
    fn what_is_at_a_path_is_told_as_missing_or_as_a_directory_with_its_identity() {
        let dir = crate::test_support::ScratchDir::new("git-watch-on-disk");
        std::fs::write(dir.join("file"), b"x").unwrap();

        assert_eq!(OnDisk::of(&dir.join("absent")), OnDisk::Missing);
        assert_eq!(OnDisk::of(&dir.join("file")), OnDisk::Missing);
        assert_eq!(OnDisk::of(&dir), OnDisk::Present(identity_at(&dir)));
        assert!(identity_at(&dir).is_some());
    }

    #[test]
    fn a_root_needs_arming_when_it_is_unarmed_or_on_a_replaced_directory_and_there_is_one() {
        let first = crate::test_support::ScratchDir::new("git-watch-needs-first");
        let second = crate::test_support::ScratchDir::new("git-watch-needs-second");
        let first = identity_at(&first).expect("the file system tells directories apart");
        let second = identity_at(&second).expect("the file system tells directories apart");
        assert_ne!(first, second);

        // Nothing to watch where there is no directory, whatever the watch's state.
        assert!(!root_of(1, true, Some(first), false).needs_arming(OnDisk::Missing));
        assert!(!root_of(1, false, None, true).needs_arming(OnDisk::Missing));
        assert!(!root_of(1, false, Some(first), false).needs_arming(OnDisk::Missing));

        // A watch not in place is set up where a directory is.
        assert!(root_of(1, false, None, false).needs_arming(OnDisk::Present(Some(first))));
        assert!(root_of(1, false, None, true).needs_arming(OnDisk::Present(Some(first))));
        assert!(root_of(1, false, Some(first), false).needs_arming(OnDisk::Present(Some(first))));
        assert!(root_of(1, false, None, false).needs_arming(OnDisk::Present(None)));

        // A watch in place on the directory now there is left alone; on another it is set up again.
        assert!(!root_of(1, true, Some(first), false).needs_arming(OnDisk::Present(Some(first))));
        assert!(root_of(1, true, Some(first), false).needs_arming(OnDisk::Present(Some(second))));
    }

    #[test]
    fn arming_a_root_adds_its_watch_in_place_of_any_held_and_notes_the_directory() {
        let dir = crate::test_support::ScratchDir::new("git-watch-arm-ok");
        let now = identity_at(&dir);
        let mut paths = RecordingPaths::new();
        let mut root = root_of(2, false, None, false);

        arm(
            &mut paths,
            "p1",
            &dir,
            RecursiveMode::Recursive,
            &mut root,
            OnDisk::Present(now),
        );

        assert_eq!(
            paths.calls,
            [
                PathCall::Remove(dir.to_path_buf()),
                PathCall::Add(dir.to_path_buf(), RecursiveMode::Recursive),
            ]
        );
        assert_eq!(seen(&root), state_of(2, true, now, false));
    }

    #[test]
    fn arming_a_root_does_not_depend_on_the_platform_having_a_watch_to_remove() {
        let dir = crate::test_support::ScratchDir::new("git-watch-arm-no-old");
        let now = identity_at(&dir);
        let mut paths = RecordingPaths::new();
        paths.remove_fails = true;
        let mut root = root_of(1, false, None, false);

        arm(
            &mut paths,
            "p1",
            &dir,
            RecursiveMode::NonRecursive,
            &mut root,
            OnDisk::Present(now),
        );

        assert!(paths.calls.contains(&PathCall::Add(
            dir.to_path_buf(),
            RecursiveMode::NonRecursive
        )));
        assert_eq!(seen(&root), state_of(1, true, now, false));
    }

    #[test]
    fn arming_a_root_on_a_replaced_directory_notes_the_new_one() {
        let old = crate::test_support::ScratchDir::new("git-watch-arm-old");
        let new = crate::test_support::ScratchDir::new("git-watch-arm-new");
        let (old, new) = (identity_at(&old), identity_at(&new));
        let mut paths = RecordingPaths::new();
        let mut root = root_of(1, true, old, false);

        arm(
            &mut paths,
            "p1",
            Path::new("/repo/refs"),
            RecursiveMode::Recursive,
            &mut root,
            OnDisk::Present(new),
        );

        assert_eq!(seen(&root), state_of(1, true, new, false));
        assert!(matches!(paths.calls.last(), Some(PathCall::Add(..))));
    }

    #[test]
    fn a_root_whose_watch_cannot_be_added_stays_counted_unarmed_and_failing_with_nothing_left_watched(
    ) {
        let dir = crate::test_support::ScratchDir::new("git-watch-arm-fail");
        let now = identity_at(&dir);
        let mut paths = RecordingPaths::new();
        paths.add_fails = true;
        let mut root = root_of(3, false, None, false);

        arm(
            &mut paths,
            "p1",
            &dir,
            RecursiveMode::Recursive,
            &mut root,
            OnDisk::Present(now),
        );

        assert_eq!(seen(&root), state_of(3, false, None, true));
        // Whatever part of the watch was set up before failing is let go.
        assert_eq!(
            paths.calls.last(),
            Some(&PathCall::Remove(dir.to_path_buf()))
        );

        // A retry that fails again leaves it failing; one that succeeds arms it.
        arm(
            &mut paths,
            "p1",
            &dir,
            RecursiveMode::Recursive,
            &mut root,
            OnDisk::Present(now),
        );
        assert_eq!(seen(&root), state_of(3, false, None, true));
        paths.add_fails = false;
        arm(
            &mut paths,
            "p1",
            &dir,
            RecursiveMode::Recursive,
            &mut root,
            OnDisk::Present(now),
        );
        assert_eq!(seen(&root), state_of(3, true, now, false));
    }

    #[test]
    fn a_failure_to_re_arm_a_replaced_directory_leaves_the_root_unarmed() {
        let old = crate::test_support::ScratchDir::new("git-watch-arm-fail-old");
        let new = crate::test_support::ScratchDir::new("git-watch-arm-fail-new");
        let mut paths = RecordingPaths::new();
        paths.add_fails = true;
        let mut root = root_of(1, true, identity_at(&old), false);

        arm(
            &mut paths,
            "p1",
            &new,
            RecursiveMode::Recursive,
            &mut root,
            OnDisk::Present(identity_at(&new)),
        );

        assert!(!root.armed);
        assert!(root.failing);
        assert_eq!(root.holders, 1);
    }

    #[test]
    fn a_root_with_no_directory_at_its_path_is_not_armed() {
        let mut paths = RecordingPaths::new();
        let mut root = root_of(1, false, None, false);

        arm(
            &mut paths,
            "p1",
            Path::new("/repo/absent"),
            RecursiveMode::Recursive,
            &mut root,
            OnDisk::Missing,
        );

        assert!(paths.calls.is_empty(), "no watch is attempted");
        assert_eq!(seen(&root), state_of(1, false, None, false));
    }

    #[tokio::test]
    async fn every_root_of_a_tracked_project_is_armed_on_the_directory_it_was_set_on() {
        let repo = repo_with("git-watch-armed-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-armed", &repo);

        watch_now(&state, &project);

        let roots = roots_held_as_on_disk(&state.git_watch, "p1", &repo);
        assert!(!roots.is_empty());
        for (path, _) in &roots {
            assert_eq!(
                root_state(&state.git_watch, path),
                RootState {
                    holders: 1,
                    armed: true,
                    identity: identity_at(path),
                    failing: false,
                },
                "{}",
                path.display()
            );
        }
    }

    #[tokio::test]
    async fn a_directory_replaced_at_its_path_is_watched_again_and_its_new_identity_noted() {
        let repo = repo_with("git-watch-replaced-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-replaced", &repo);
        let watch = &state.git_watch;
        watch_now(&state, &project);
        let refs = Dirs::resolve(&repo).unwrap().common_dir.join("refs");
        let roots = roots_held_as_on_disk(watch, "p1", &repo);
        let others: Vec<_> = roots
            .iter()
            .map(|(path, _)| path)
            .filter(|path| **path != refs)
            .map(|path| (path.clone(), root_state(watch, path)))
            .collect();
        let before = root_state(watch, &refs);
        assert!(before.identity.is_some());

        replace_dir(&refs);
        let new = identity_at(&refs);
        assert_ne!(before.identity, new, "the fixture made another directory");
        watch_now(&state, &project);

        assert_eq!(
            root_state(watch, &refs),
            RootState {
                holders: 1,
                armed: true,
                identity: new,
                failing: false,
            }
        );
        for (path, state_before) in &others {
            assert_eq!(&root_state(watch, path), state_before, "{}", path.display());
        }
        assert_eq!(roots_held_as_on_disk(watch, "p1", &repo), roots);
        assert_eq!(watched_counts(watch), held_counts(watch));
        // Settled on the new directory: asking again changes nothing.
        watch_now(&state, &project);
        assert_eq!(root_state(watch, &refs).identity, new);
        assert_eq!(watched_counts(watch), held_counts(watch));
    }

    #[tokio::test]
    async fn a_watch_reported_lost_is_armed_again_by_the_next_check_even_on_the_same_directory() {
        let repo = repo_with("git-watch-lost-root-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-lost-root", &repo);
        let watch = &state.git_watch;
        watch_now(&state, &project);
        let dirs = Dirs::resolve(&repo).unwrap();
        let refs = dirs.common_dir.join("refs");
        let git_dir = dirs.git_dir.clone();
        let identity = root_state(watch, &refs).identity;

        watch.lose(std::slice::from_ref(&refs));

        assert!(!root_state(watch, &refs).armed);
        assert_eq!(root_state(watch, &refs).holders, 1);
        assert!(
            root_state(watch, &git_dir).armed,
            "another root is not lost"
        );
        assert_eq!(watched_counts(watch), held_counts(watch));

        watch_now(&state, &project);

        assert_eq!(
            root_state(watch, &refs),
            RootState {
                holders: 1,
                armed: true,
                identity,
                failing: false,
            }
        );
        assert!(root_state(watch, &git_dir).armed);
        assert_eq!(watched_counts(watch), held_counts(watch));
        roots_held_as_on_disk(watch, "p1", &repo);
    }

    #[tokio::test]
    async fn a_removal_below_or_beside_a_watched_root_loses_no_watch() {
        let repo = repo_with("git-watch-lose-other-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-lose-other", &repo);
        let watch = &state.git_watch;
        watch_now(&state, &project);
        let refs = Dirs::resolve(&repo).unwrap().common_dir.join("refs");

        watch.lose(&[
            refs.join("heads"),
            refs.join("heads/main"),
            repo.join("a"),
            PathBuf::from("/nowhere/at/all"),
        ]);
        watch.lose(&[]);

        assert!(all_roots_armed(watch));
        // A watched root itself is lost, and reporting it again changes nothing.
        watch.lose(std::slice::from_ref(&refs));
        watch.lose(std::slice::from_ref(&refs));
        assert!(!root_state(watch, &refs).armed);
        assert_eq!(root_state(watch, &refs).holders, 1);
    }

    #[tokio::test]
    async fn a_root_missing_when_its_project_is_tracked_is_counted_unarmed_and_armed_once_it_exists(
    ) {
        let repo = repo_with("git-watch-missing-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-missing", &repo);
        let watch = &state.git_watch;
        let refs = Dirs::resolve(&repo).unwrap().common_dir.join("refs");
        std::fs::remove_dir_all(&refs).unwrap();

        watch_now(&state, &project);

        let roots = roots_held_as_on_disk(watch, "p1", &repo);
        assert!(roots.contains(&(refs.clone(), RecursiveMode::Recursive)));
        let unarmed = RootState {
            holders: 1,
            armed: false,
            identity: None,
            failing: false,
        };
        assert_eq!(root_state(watch, &refs), unarmed);
        for (path, _) in roots.iter().filter(|(path, _)| *path != refs) {
            assert!(root_state(watch, path).armed, "{}", path.display());
        }
        assert_eq!(watched_counts(watch), held_counts(watch));

        // Nothing is there to watch, so nothing is tried, and nothing has failed.
        watch_now(&state, &project);
        assert_eq!(root_state(watch, &refs), unarmed);

        std::fs::create_dir(&refs).unwrap();
        watch_now(&state, &project);

        assert_eq!(
            root_state(watch, &refs),
            RootState {
                holders: 1,
                armed: true,
                identity: identity_at(&refs),
                failing: false,
            }
        );
        assert_eq!(roots_held_as_on_disk(watch, "p1", &repo), roots);
        assert_eq!(watched_counts(watch), held_counts(watch));
    }

    #[tokio::test]
    async fn a_root_removed_and_made_again_is_armed_on_the_new_directory() {
        let repo = repo_with("git-watch-remade-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-remade", &repo);
        let watch = &state.git_watch;
        watch_now(&state, &project);
        let refs = Dirs::resolve(&repo).unwrap().common_dir.join("refs");

        std::fs::remove_dir_all(&refs).unwrap();
        watch.lose(std::slice::from_ref(&refs));
        watch_now(&state, &project);

        // Gone: still counted for the project, with no watch.
        assert!(!root_state(watch, &refs).armed);
        assert_eq!(root_state(watch, &refs).holders, 1);
        assert_eq!(watched_counts(watch), held_counts(watch));

        std::fs::create_dir_all(refs.join("heads")).unwrap();
        watch_now(&state, &project);

        assert_eq!(
            root_state(watch, &refs),
            RootState {
                holders: 1,
                armed: true,
                identity: identity_at(&refs),
                failing: false,
            }
        );
        assert_eq!(watched_counts(watch), held_counts(watch));
    }

    #[tokio::test]
    async fn a_settled_project_is_not_settled_once_a_watch_is_lost_or_a_directory_replaced() {
        let repo = repo_with("git-watch-settled-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_watching("git-watch-settled", &repo);
        let watch = &state.git_watch;
        let dirs = Dirs::resolve(&repo).unwrap();
        let refs = dirs.common_dir.join("refs");
        let on_disk = |dirs: &Dirs| -> Vec<OnDisk> {
            dirs.roots()
                .iter()
                .map(|(path, _)| OnDisk::of(path))
                .collect()
        };
        let settled = |dirs: &Dirs| {
            let guard = watch.lock();
            guard.as_ref().unwrap().settled(dirs, &on_disk(dirs))
        };
        assert!(!settled(&dirs), "nothing is counted before it is tracked");

        watch_now(&state, &project);
        assert!(settled(&dirs));

        watch.lose(std::slice::from_ref(&refs));
        assert!(!settled(&dirs));
        watch_now(&state, &project);
        assert!(settled(&dirs));

        replace_dir(&refs);
        assert!(!settled(&dirs));
        watch_now(&state, &project);
        assert!(settled(&dirs));
    }

    #[tokio::test]
    async fn re_arming_a_root_shared_by_worktrees_once_serves_every_holder() {
        let repo = repo_with("git-watch-shared-rearm-repo", &[(b"a", b"a")]);
        let linked_path = repo.join("linked");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                linked_path.to_str().unwrap(),
            ],
        );
        let (state, _dir, main) = state_watching("git-watch-shared-rearm", &repo);
        let linked = insert_second_project(&state, &main, &linked_path);
        let watch = &state.git_watch;
        watch.track(&state, &[main.clone(), linked.clone()]);
        let refs = Dirs::resolve(&repo).unwrap().common_dir.join("refs");
        assert_eq!(root_state(watch, &refs).holders, 2);
        let own = Dirs::resolve(&linked_path).unwrap().git_dir;
        let own_before = root_state(watch, &own);

        // Asked by one holder only, the replaced directory is watched again for both.
        replace_dir(&refs);
        watch_now(&state, &linked);

        let rearmed = RootState {
            holders: 2,
            armed: true,
            identity: identity_at(&refs),
            failing: false,
        };
        assert_eq!(root_state(watch, &refs), rearmed);
        assert_eq!(root_state(watch, &own), own_before);
        assert_eq!(watched_counts(watch), held_counts(watch));
        watch_now(&state, &main);
        assert_eq!(root_state(watch, &refs), rearmed);
        assert_eq!(watched_counts(watch), held_counts(watch));

        // A watch lost is lost for both holders, and the other holder's check sets it up again.
        watch.lose(std::slice::from_ref(&refs));
        assert!(!root_state(watch, &refs).armed);
        assert_eq!(root_state(watch, &refs).holders, 2);
        watch_now(&state, &main);
        assert_eq!(root_state(watch, &refs), rearmed);
        assert_eq!(watched_counts(watch), held_counts(watch));
    }

    #[tokio::test]
    async fn a_worktree_removed_and_added_again_is_watched_again() {
        let repo = repo_with("git-watch-readded-repo", &[(b"a", b"a")]);
        let linked_path = repo.join("linked");
        let add = |repo: &Path| {
            git(
                repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "--detach",
                    linked_path.to_str().unwrap(),
                ],
            )
        };
        add(&repo);
        let (state, _dir, main) = state_watching("git-watch-readded", &repo);
        let linked = insert_second_project(&state, &main, &linked_path);
        let watch = &state.git_watch;
        watch.track(&state, &[main, linked.clone()]);
        let old_git_dir = Dirs::resolve(&linked_path).unwrap().git_dir;
        assert!(root_state(watch, &old_git_dir).armed);

        git(
            &repo,
            &[
                "worktree",
                "remove",
                "--force",
                linked_path.to_str().unwrap(),
            ],
        );
        // What the platform's removal event does where it ends the watch.
        watch.lose(std::slice::from_ref(&old_git_dir));
        watch_now(&state, &linked);
        assert_eq!(watched_counts(watch), held_counts(watch));

        add(&repo);
        watch_now(&state, &linked);

        let git_dir = Dirs::resolve(&linked_path).unwrap().git_dir;
        assert_eq!(
            root_state(watch, &git_dir),
            RootState {
                holders: 1,
                armed: true,
                identity: identity_at(&git_dir),
                failing: false,
            }
        );
        roots_held_as_on_disk(watch, "p2", &linked_path);
        assert_eq!(watched_counts(watch), held_counts(watch));
    }

    #[tokio::test]
    async fn a_removal_concerns_the_projects_that_watch_what_it_removed() {
        let repo = repo_with("git-watch-removed-concerns-repo", &[(b"a", b"a")]);
        let linked_path = repo.join("linked");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                linked_path.to_str().unwrap(),
            ],
        );
        let (state, _dir, main) = state_watching("git-watch-removed-concerns", &repo);
        let linked = insert_second_project(&state, &main, &linked_path);
        let watch = &state.git_watch;
        watch.track(&state, &[main, linked]);
        let common = Dirs::resolve(&repo).unwrap().common_dir;
        let linked_git_dir = Dirs::resolve(&linked_path).unwrap().git_dir;
        let refs = common.join("refs");
        let removed = |paths: Vec<PathBuf>| sorted(watch.concerned(&Report::Removed(paths)));
        let changed = |paths: Vec<PathBuf>| watch.concerned(&Report::Changed(paths));

        // A watched root that went: every project that watches it, and only those.
        assert_eq!(removed(vec![refs.clone()]), ["p1", "p2"]);
        assert_eq!(removed(vec![common.clone()]), ["p1", "p2"]);
        assert_eq!(removed(vec![linked_git_dir.clone()]), ["p2"]);
        assert_eq!(
            removed(vec![linked_git_dir.clone(), refs.clone()]),
            ["p1", "p2"]
        );
        // A removed ref concerns a project as a change at it does.
        assert_eq!(removed(vec![refs.join("heads/main")]), ["p1", "p2"]);
        assert!(removed(vec![repo.join("a")]).is_empty());
        assert!(removed(Vec::new()).is_empty());

        // A change at a root's own path is not a ref change while its watch is in place.
        assert!(changed(vec![refs.clone()]).is_empty());
        assert!(changed(vec![linked_git_dir.clone()]).is_empty());

        // Once its watch is lost, a directory made there is the root back: the projects holding
        // it are read again, which sets the watch up.
        watch.lose(&[refs.clone(), linked_git_dir.clone()]);
        assert_eq!(sorted(changed(vec![refs.clone()])), ["p1", "p2"]);
        assert_eq!(changed(vec![linked_git_dir.clone()]), ["p2"]);
        assert!(
            changed(vec![common.clone()]).is_empty(),
            "its watch is in place"
        );
    }

    #[test]
    fn a_removal_or_a_rename_is_forwarded_as_removed_with_its_paths() {
        use notify::event::{ModifyKind, RemoveKind, RenameMode};
        use notify::EventKind;
        let paths = vec![
            PathBuf::from("/repo/.git/refs"),
            PathBuf::from("/repo/.git/refs.old"),
        ];
        for kind in [
            EventKind::Remove(RemoveKind::Any),
            EventKind::Remove(RemoveKind::File),
            EventKind::Remove(RemoveKind::Folder),
            EventKind::Remove(RemoveKind::Other),
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Other)),
        ] {
            let mut event = notify::Event::new(kind);
            event.paths = paths.clone();
            assert_eq!(
                forwarded(Ok(event)),
                Some(Report::Removed(paths.clone())),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_other_changes_are_still_forwarded_as_changed_and_a_rescan_as_lost() {
        use notify::event::{CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind};
        use notify::EventKind;
        let paths = vec![PathBuf::from("/repo/.git/refs/heads/main")];
        for kind in [
            EventKind::Any,
            EventKind::Other,
            EventKind::Create(CreateKind::File),
            EventKind::Create(CreateKind::Folder),
            EventKind::Modify(ModifyKind::Any),
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)),
            EventKind::Modify(ModifyKind::Other),
        ] {
            let mut event = notify::Event::new(kind);
            event.paths = paths.clone();
            assert_eq!(
                forwarded(Ok(event)),
                Some(Report::Changed(paths.clone())),
                "{kind:?}"
            );
        }
        // An event that says events were lost is that, even where it also names a removal.
        assert_eq!(
            forwarded(Ok(
                rescan(EventKind::Remove(RemoveKind::Folder)).add_path(paths[0].clone())
            )),
            Some(Report::Lost)
        );
    }

    /// A removal of a watched root reported by the platform makes the projects holding it be read
    /// again, and the root be watched anew: nothing is watched here and nobody told the watch of
    /// the commit, so the re-read can only come from the `Removed` the callback sent. The refs
    /// directory is replaced, and the callback is given as it names that directory only, as inotify
    /// does when it is removed or renamed.
    #[tokio::test]
    async fn a_watched_root_reported_removed_makes_its_projects_be_read_again_and_watched_anew() {
        let repo = repo_with("git-watch-removed-reread-repo", &[(b"a", b"a")]);
        let (state, _dir, project) = state_with_upstream("git-watch-removed-reread", &repo);
        watch_now(&state, &project);
        let refs = Dirs::resolve(&repo).unwrap().common_dir.join("refs");
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let callback = forward(sender);
        tokio::spawn(rereads(state.clone(), receiver));
        let mut events = state.subscribe();

        replace_dir(&refs);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "outside"]);
        callback(Ok(notify::Event::new(notify::EventKind::Remove(
            notify::event::RemoveKind::Folder,
        ))
        .add_path(refs.clone())));

        tokio::time::timeout(crate::test_support::PATIENCE, async {
            loop {
                if let Event::ProjectGitStatus { status } = events.recv().await.unwrap() {
                    if status.ahead == 1 {
                        break;
                    }
                }
            }
        })
        .await
        .expect("a project_git_status with the commit arrives");
        assert_eq!(
            root_state(&state.git_watch, &refs),
            RootState {
                holders: 1,
                armed: true,
                identity: identity_at(&refs),
                failing: false,
            }
        );
    }

    /// Through the real watcher: a directory the watch is on is replaced, the re-read the
    /// replacement's own events start sets the watch up on the new one, and a commit made after
    /// that read is noticed, which it can only be if the new directory is watched: the commit and
    /// the retouches write only inside it. Not on macOS: FSEvents watches by path, so it sees the
    /// new directory whether or not the watch is set up again, and it may never deliver the
    /// replacement's events, leaving nothing to wait for.
    #[cfg(not(target_os = "macos"))]
    #[tokio::test]
    async fn a_commit_is_noticed_in_a_refs_directory_replaced_after_it_was_watched() {
        let repo = repo_with("git-watch-replaced-commit-repo", &[(b"a", b"a")]);
        let (state, _dir, _) = state_with_upstream("git-watch-replaced-commit", &repo);
        let mut events = state.subscribe();
        start(&state);
        // The projects are watched off the runtime; replacing a directory before that is no test.
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !all_roots_armed(&state.git_watch) {
            assert!(Instant::now() < deadline, "every root is watched");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let refs = Dirs::resolve(&repo).unwrap().common_dir.join("refs");
        let old = root_state(&state.git_watch, &refs).identity;

        replace_dir(&refs);
        // The replacement's own events re-read the project, which sets the watch up on the new
        // directory (once it is there: a read that ran before it was made leaves that to the
        // read its making starts). Nothing else touches the watch here.
        let deadline = Instant::now() + FILE_EVENT_PATIENCE;
        loop {
            let now = root_state(&state.git_watch, &refs);
            if now.armed && now.identity == identity_at(&refs) {
                assert_ne!(now.identity, old);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the replacement of a watched directory sets its watch up again"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // A commit made before that re-read published would be reported by it, whatever the new
        // watch sees.
        tokio::time::timeout(FILE_EVENT_PATIENCE, async {
            while !matches!(events.recv().await.unwrap(), Event::ProjectGitStatus { .. }) {}
        })
        .await
        .expect("the re-read that set the watch up publishes");

        git(&repo, &["commit", "-q", "--allow-empty", "-m", "outside"]);
        wait_for_the_commit_to_be_reported(&mut events, &repo, retouch_main).await;
    }
}
