//! Watching each git project's refs, so a commit made outside the daemon (an agent session, a
//! terminal) shows in the project's badge without waiting for the client's next poll.
//!
//! Only the files that move a branch header are watched: the git directory's `HEAD`, the common
//! directory's `packed-refs` and everything under its `refs/heads` and `refs/remotes` (a fetch or
//! push from a terminal changes ahead and behind too). The working tree is never watched, and
//! neither is the rest of the git directory: `git status` itself rewrites the index there, so a
//! watch on it would re-read on its own reads. A change is debounced, then answered with
//! `header::publish_reread`, the local-only read that leaves an overlapping check's activity
//! alone; nothing here goes to the remote.
//!
//! A linked worktree keeps its `HEAD` in its own git directory and everything else in the
//! repository's common directory, so a project is watched through both ([`Dirs`]). A project is
//! one the same way `is_git_repo` says: it has `.git` in its own directory.
//!
//! A watch that cannot be set up is logged and left out; the client's poll still covers that
//! project, as it covers a change the platform never reports: under heavy load macOS
//! delivers file events late or not at all, without flagging any as dropped. A session's own git
//! does not wait on either: its hooks report the refs it moves (`super::ref_moves`). A project
//! that is not a repository when it is added, or at daemon start, is picked up by the first check
//! that finds it is one (`status::run_checks`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::git::layout::is_git_repo;
use crate::project_git::debounced::{Debounced, Timing};
use crate::project_git::header;
use crate::protocol::Project;
use crate::state::AppState;

/// How long refs stay quiet before a project is re-read. A commit or a fetch rewrites several
/// files in a burst (the lock file, the ref, the reflog), and one read answers all of them.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// The two directories a repository's refs live in. They are one for an ordinary repository.
#[derive(Debug, PartialEq, Eq)]
struct Dirs {
    /// Holds this working tree's `HEAD`.
    git_dir: PathBuf,
    /// Holds `packed-refs` and `refs/`, shared by every worktree of the repository.
    common_dir: PathBuf,
}

impl Dirs {
    /// Reads where `project_dir`'s `.git` points, without running `git`: a directory is the git
    /// directory itself, a file is `gitdir: <path>` (relative to the project) and names the git
    /// directory of a linked worktree, whose `commondir` file names the common one. Both come
    /// back canonical, since a watch reports canonical paths (`/private/var` for `/var`).
    fn resolve(project_dir: &Path) -> Option<Self> {
        let git_dir = crate::git::layout::git_dir(project_dir)?;
        let common_dir = crate::git::layout::common_dir(&git_dir).ok()?;
        Some(Self {
            git_dir: crate::hostfs::canonical(&git_dir).ok()?,
            common_dir: crate::hostfs::canonical(&common_dir).ok()?,
        })
    }

    /// What is watched for this repository: directories read for the two files directly in them
    /// (the platform may report more, which [`Dirs::concerns`] drops), and the refs tree.
    fn roots(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut roots = vec![(self.git_dir.clone(), RecursiveMode::NonRecursive)];
        if self.common_dir != self.git_dir {
            roots.push((self.common_dir.clone(), RecursiveMode::NonRecursive));
        }
        roots.push((self.common_dir.join("refs"), RecursiveMode::Recursive));
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
    }
}

struct Tracked {
    dirs: Dirs,
    roots: Vec<(PathBuf, RecursiveMode)>,
}

struct Inner {
    watcher: RecommendedWatcher,
    projects: HashMap<String, Tracked>,
    /// How many projects watch each path: linked worktrees of one repository share its common
    /// directory, and one of them going away must not end the others' watch.
    watched: HashMap<PathBuf, usize>,
}

/// The daemon's ref watcher. Empty until [`start`] arms it, so a state built without it (every
/// test) watches nothing.
#[derive(Default)]
pub struct Watch {
    inner: Mutex<Option<Inner>>,
}

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

    /// Starts watching `projects` that are repositories and not watched yet, applying all their
    /// paths in one batch: the macOS backend restarts its whole event stream on every change to
    /// what it watches, so one change per path is quadratic over many projects and loses events
    /// in each gap. What each project resolves to is read before the lock is taken.
    fn track(&self, state: &AppState, projects: &[Project]) {
        let resolved: Vec<(&Project, Dirs)> = projects
            .iter()
            .filter(|project| self.wants(&project.id))
            .filter(|project| is_git_repo(Path::new(&project.path)))
            .filter_map(|project| {
                let dirs = Dirs::resolve(Path::new(&project.path));
                if dirs.is_none() {
                    tracing::debug!(project = %project.id, "the repository's directories could not be resolved");
                }
                Some((project, dirs?))
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
        let resolved: Vec<(&Project, Dirs)> = resolved
            .into_iter()
            .filter(|(project, _)| {
                !inner.projects.contains_key(&project.id)
                    && matches!(state.store.get_project(&project.id), Ok(Some(_)))
            })
            .collect();
        // Applying even an empty batch restarts the macOS event stream, which loses events.
        if resolved.is_empty() {
            return;
        }
        let mut paths = inner.watcher.paths_mut();
        let mut added = Vec::new();
        for (project, dirs) in resolved {
            let mut roots = Vec::new();
            for (path, mode) in dirs.roots() {
                let count = inner.watched.entry(path.clone()).or_insert(0);
                if *count == 0 {
                    if let Err(err) = paths.add(&path, mode) {
                        tracing::warn!(project = %project.id, path = %path.display(), %err, "watching the repository's refs failed");
                        inner.watched.remove(&path);
                        continue;
                    }
                }
                *inner.watched.get_mut(&path).expect("entry made above") += 1;
                roots.push((path, mode));
            }
            added.push((project.id.clone(), Tracked { dirs, roots }));
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

    /// The projects a change at any of `paths` concerns.
    fn concerned(&self, paths: &[PathBuf]) -> Vec<String> {
        let guard = self.lock();
        let Some(inner) = guard.as_ref() else {
            return Vec::new();
        };
        inner
            .projects
            .iter()
            .filter(|(_, tracked)| paths.iter().any(|path| tracked.dirs.concerns(path)))
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Inner {
    fn untrack(&mut self, project_id: &str) {
        let Some(tracked) = self.projects.remove(project_id) else {
            return;
        };
        let mut released = Vec::new();
        for (path, _) in tracked.roots {
            let Some(count) = self.watched.get_mut(&path) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                self.watched.remove(&path);
                released.push(path);
            }
        }
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

/// Whether an event of `kind` can mean the repository changed. An access changes nothing, and
/// Linux's inotify reports every file opened, git's own reads included: the `git status` a change
/// starts would otherwise report itself and start the next one, for ever. A close after writing
/// goes too: the write itself already arrived as a modification or a rename, so the close adds
/// nothing.
fn changes_something(kind: &notify::EventKind) -> bool {
    !matches!(kind, notify::EventKind::Access(_))
}

/// The watcher's callback: sends the paths of every event that can be a change to `sender`, and
/// logs an error the platform reports.
fn forward(
    sender: UnboundedSender<Vec<PathBuf>>,
) -> impl Fn(notify::Result<notify::Event>) + Send + 'static {
    move |result| match result {
        Ok(event) if changes_something(&event.kind) => {
            sender.send(event.paths).ok();
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

/// As [`add`], on the calling thread, which must be one that may block. For a check that finds a
/// repository the watch does not know.
pub fn watch_now(state: &AppState, project: &Project) {
    state.git_watch.track(state, std::slice::from_ref(project));
}

/// Turns the paths a watch reports into debounced re-reads: a project is read once its refs have
/// been quiet for [`DEBOUNCE`] ([`super::debounced`]), and one more time straight after a read
/// during which a change arrived. Ends when the watcher, which holds the sending half, is dropped.
async fn rereads(state: Arc<AppState>, mut receiver: UnboundedReceiver<Vec<PathBuf>>) {
    let scheduler = Arc::new(Debounced::new(Timing {
        debounce: DEBOUNCE,
        max_delay: None,
        owed: Duration::ZERO,
    }));
    while let Some(paths) = receiver.recv().await {
        for id in state.git_watch.concerned(&paths) {
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

        let first = tokio::time::timeout(FILE_EVENT_PATIENCE, forwarded.recv())
            .await
            .expect("the write after the reads is reported")
            .expect("the watch is alive");
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
        let (state, dir) = app_state("git-watch-commit");
        let remote = dir.join("remote.git");
        git(&dir, &["init", "-q", "--bare", remote.to_str().unwrap()]);
        let repo = repo_with("git-watch-commit-repo", &[(b"a", b"a")]);
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
        let mut events = state.subscribe();
        start(&state);
        // The projects are watched off the runtime; a commit made before that would be missed.
        while !state
            .git_watch
            .lock()
            .as_ref()
            .is_some_and(|inner| inner.projects.contains_key("p1"))
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        git(&repo, &["commit", "-q", "--allow-empty", "-m", "outside"]);

        // The platform may never report the commit's own writes (see `FILE_EVENT_PATIENCE`), so
        // the ref it moved is touched again until a re-read reports the commit.
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
                    retouch_main(&repo);
                    retouch_at = Instant::now() + RETOUCH;
                }
            }
        }
    }
}
