//! Which session created a git branch, kept so the Git mode can mark the relation.
//!
//! The record is the creator's, not "whoever is working on the branch now", and a wrong name is
//! worse than none: whatever cannot be told is stored as [`BranchOriginSource::Unknown`] and never
//! sent to a client. That is the record of a question this look could not answer, not an answer —
//! a later hook window that does name a session replaces it. Everything lives in the daemon's
//! SQLite (`store::BranchOrigin`) — the Git mode never writes to a user's repository, so no ref,
//! note or configuration entry carries it.
//!
//! Two signals attribute a branch, and both look at the result rather than at what ran:
//!
//! - **A hook window.** Every hook callback names the session that fired it. When the repository's
//!   branches are looked at again after a hook, a branch that was not there at the previous look is
//!   the work of whoever acted in between — whatever command, alias or script made it. That is
//!   [`BranchOriginSource::HookDiff`], and it is claimed only when exactly one session could have
//!   acted: a hook of another session of the repository in the same window, or another one still
//!   `Working`, makes the branch `unknown`.
//! - **The reflog's first entry.** A branch the daemon never saw appear — made before it ran, or
//!   before this record existed — carries the time it was made in the first line of
//!   `logs/refs/heads/<name>`. If that entry is one a branch's own creation writes
//!   ([`CREATED_FROM`]) and its time falls inside the lifetime of exactly one session of the
//!   repository, that is [`BranchOriginSource::ReflogMatch`]. A first entry of any other shape —
//!   the default branch a `git init` leaves, a clone's, a bare `git update-ref` — says the ref was
//!   written, not that a branch was made there, so it backfills nothing however clear the session
//!   is. A session's lifetime is the coarsest timeline the daemon has (its start and end, not its
//!   turns), so this is the weaker signal and only ever fills in what the first one did not see.
//!
//! A row lives and dies with its branch. It is validated against the branch's first reflog entry on
//! every look, so a branch deleted and made again under the same name does not inherit the old
//! attribution, and removed once a full listing taken after the row was written no longer has the
//! branch. Nothing here runs on a timer: the looks are the hook callback ([`on_hook`]) and the Git
//! mode's own branch listing ([`annotate`]).
//!
//! These limits follow from the design:
//!
//! - The record names the session a branch appeared alongside, which is not always whoever ran the
//!   command. A branch the user makes in their own terminal while exactly one session of the
//!   repository is mid-turn is attributed to that session: its next hook finds a branch that was
//!   not there at the previous look, with no other session that could have made it. Only a branch
//!   nothing was acting for is left unattributed.
//! - A repository whose refs are in reftable format keeps no `logs/refs/heads` tree, so both of
//!   the things read out of a reflog are unavailable there: no branch is backfilled, and a row
//!   cannot be held against its branch's birth, so a branch deleted and made again under the same
//!   name keeps the first one's attribution until the row goes with the name. Reading those
//!   reflogs means one `git` per branch on the hook path, which is the cost the stamp and the
//!   floor exist to avoid. The hook window itself works there unchanged: it reads only the stamp
//!   and the branch listing.
//! - Elsewhere a row is held only as long as the reflog keeps the branch's first entry: once
//!   `gc.reflogExpire` (90 days by default) drops it, the branch no longer matches its row, is
//!   recorded again as `unknown`, and that row then blocks the backfill for good. A branch deleted
//!   and made again at the same commit inside one second has the same first entry and keeps the
//!   old row.
//!
//! The hook path runs on every tool call of every session, so it is cheap by default: a look at
//! the filesystem stamp of the repository's refs ([`Stamp`]) rules out nearly all of them, and a
//! repository is enumerated at most once per [`FLOOR`] after its last enumeration finished. A
//! hook held back by the floor is not lost: its session is remembered as an actor of the window,
//! and one trailing look runs when the floor is over.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;

use crate::browse::compare::{self, BranchList};
use crate::browse::git::GitEnv;
use crate::browse::{budget, source, wire_path};
use crate::protocol::{
    now_millis, BranchInfo, BranchOriginInfo, BranchOriginSource, Session, SessionStatus,
};
use crate::state::AppState;
use crate::store::BranchOrigin;

/// The least time between the end of one enumeration of a repository and the start of the next
/// one the hook path makes. A commit rewrites the checked-out branch's ref, which changes the
/// [`Stamp`] as a new branch does, so a session committing in a loop would otherwise run a
/// `git for-each-ref` per tool call; agents' calls come seconds apart and a branch is only ever
/// attributed to the window it appeared in, so five seconds bounds the cost without moving
/// attribution anywhere that matters. It stays short enough that the trailing look after a held
/// back hook lands while the session is still in the same piece of work.
const FLOOR: Duration = Duration::from_secs(5);

/// How many directories below `refs/heads` a [`Stamp`] looks at. A branch named `a/b/c` lives in
/// directories of its own, and a repository with more than this has its stamp cut: it still
/// changes whenever the part it covers does.
const MAX_STAMPED_DIRS: usize = 4_096;

/// The seconds of clock difference tolerated between a reflog entry's time and a time the daemon
/// recorded itself: the start of a look, or a session's own start and end.
const CLOCK_SLACK_SECS: i64 = 2;

/// How much of a reflog's first line is read. A line is two object ids, an identity, a time and
/// a message.
const MAX_REFLOG_LINE: u64 = 64 * 1024;

/// What `git branch`, `checkout -b`, `switch -c` and `worktree add -b` write as the message of a
/// branch's first reflog entry.
const CREATED_FROM: &[u8] = b"branch: Created from";

/// The daemon's memory of each repository's branches between looks. Held on `AppState`; none of it
/// is persisted, so a restart begins every repository with no previous look.
#[derive(Default)]
pub struct Watch {
    repos: Mutex<HashMap<PathBuf, RepoWatch>>,
}

#[derive(Default)]
struct RepoWatch {
    /// The stamp taken before the last successful enumeration.
    stamp: Option<Stamp>,
    /// The branches the last full enumeration found; `None` before the first one, when nothing
    /// can be told apart as new.
    known: Option<HashSet<String>>,
    /// When the last enumeration ended, however it ended.
    completed: Option<Instant>,
    /// When the last enumeration began, in seconds since the epoch.
    looked_at_secs: Option<i64>,
    running: bool,
    /// A trailing look is already waiting for the floor to end.
    trailing: bool,
    /// The sessions whose hooks arrived since the last enumeration began and changed the stamp.
    actors: HashSet<String>,
    /// The branches the Git mode's listings have seen appear since the last look after a hook,
    /// which the next such look treats as new whatever `known` now holds. A listing moves `known`
    /// on to what it found, and it runs every few seconds while the panel is open: without this,
    /// a listing landing between a branch's creation and the look that would attribute it buries
    /// the branch for good.
    fresh: HashSet<String>,
    /// When the last reconcile that changed this repository's rows ran; see [`Watch::wrote_at`].
    wrote_at: Option<Instant>,
    /// Serializes this repository's reconciles; see [`Watch::gate`].
    gate: Arc<Mutex<()>>,
}

/// What the filesystem says about a repository's refs, compared for equality between two hooks.
///
/// A new branch, a deleted one and a moved one each rewrite a ref by creating and renaming a file,
/// which changes the modification time of the directory holding it — `refs/heads` itself, or the
/// directory of a name with a slash in it, so the whole tree of directories is taken (never the
/// files in it). Packed refs and the reftable list are the other places a ref can change.
///
/// The resolution of those times is the filesystem's. On one that keeps modification times to the
/// whole second, a branch made in the same second as the previous look's stamp leaves the stamp
/// equal and the hook sees nothing to look at; the branch is then attributed by the Git mode's
/// listing from its reflog, or not at all.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    directories: usize,
    newest: Option<SystemTime>,
    packed_refs: Option<SystemTime>,
    reftable: Option<SystemTime>,
}

impl Stamp {
    fn read(common_dir: &Path) -> Self {
        let modified = |path: &Path| fs::metadata(path).and_then(|m| m.modified()).ok();
        let mut stamp = Stamp {
            directories: 0,
            newest: None,
            packed_refs: modified(&common_dir.join("packed-refs")),
            reftable: modified(&common_dir.join("reftable/tables.list")),
        };
        let mut pending = vec![common_dir.join("refs/heads")];
        while let Some(dir) = pending.pop() {
            let Ok(metadata) = fs::symlink_metadata(&dir) else {
                continue;
            };
            if stamp.directories == MAX_STAMPED_DIRS {
                break;
            }
            if !metadata.is_dir() {
                continue;
            }
            stamp.directories += 1;
            stamp.newest = stamp.newest.max(metadata.modified().ok());
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            // In a fixed order, since the walk stops counting past the limit: `read_dir` gives no
            // order of its own, so without this two walks of a repository nothing changed in could
            // cover different directories and disagree.
            let mut children: Vec<PathBuf> = entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.path())
                .collect();
            children.sort();
            pending.extend(children);
        }
        stamp
    }
}

/// Who is asking for a look.
enum Caller<'a> {
    Hook(&'a str),
    /// The look scheduled for when the floor is over.
    Trailing,
}

enum Claim<'a> {
    /// Nothing to look at, or somebody else is already looking.
    Nothing,
    /// Look at the repository after this long.
    Later(Duration),
    Run(Scan<'a>),
}

/// The right to enumerate one repository, released on `Drop` even if the enumeration panics: a
/// repository whose claim is never released would never be looked at again.
struct Scan<'a> {
    watch: &'a Watch,
    repo: PathBuf,
    actors: HashSet<String>,
    known: Option<HashSet<String>>,
    fresh: HashSet<String>,
    previous_secs: Option<i64>,
    finished: bool,
}

impl Scan<'_> {
    /// Records the look as done: `stamp` as it stood before the enumeration, and `known` when the
    /// enumeration was complete.
    fn finish(mut self, stamp: Stamp, known: Option<HashSet<String>>) {
        let mut repos = self.watch.lock();
        if let Some(entry) = repos.get_mut(&self.repo) {
            entry.stamp = Some(stamp);
            if known.is_some() {
                entry.known = known;
            }
            entry.running = false;
            entry.completed = Some(Instant::now());
        }
        self.finished = true;
    }
}

impl Drop for Scan<'_> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // A look that failed leaves the next one everything this one was given: the window its
        // actors acted in is still open, the branches a listing handed it are still unattributed,
        // and the time it started from is still the baseline a branch made since has to be newer
        // than.
        let mut repos = self.watch.lock();
        if let Some(entry) = repos.get_mut(&self.repo) {
            entry.running = false;
            entry.completed = Some(Instant::now());
            entry.actors.extend(self.actors.drain());
            entry.fresh.extend(self.fresh.drain());
            entry.looked_at_secs = self.previous_secs;
        }
    }
}

impl Watch {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, RepoWatch>> {
        self.repos.lock().expect("branch watch lock poisoned")
    }

    fn claim(&self, repo: &Path, caller: Caller<'_>, stamp: &Stamp, now: Instant) -> Claim<'_> {
        let mut repos = self.lock();
        let entry = repos.entry(repo.to_path_buf()).or_default();
        match caller {
            Caller::Hook(session) => {
                if entry.stamp.as_ref() == Some(stamp) {
                    return Claim::Nothing;
                }
                entry.actors.insert(session.to_string());
            }
            Caller::Trailing => {
                entry.trailing = false;
                if entry.stamp.as_ref() == Some(stamp) {
                    entry.actors.clear();
                    return Claim::Nothing;
                }
            }
        }
        if entry.running {
            return Claim::Nothing;
        }
        if let Some(done) = entry.completed {
            let since = now.saturating_duration_since(done);
            if since < FLOOR {
                if entry.trailing {
                    return Claim::Nothing;
                }
                entry.trailing = true;
                return Claim::Later(FLOOR - since);
            }
        }
        entry.running = true;
        Claim::Run(Scan {
            watch: self,
            repo: repo.to_path_buf(),
            actors: std::mem::take(&mut entry.actors),
            known: entry.known.clone(),
            fresh: std::mem::take(&mut entry.fresh),
            previous_secs: entry.looked_at_secs.replace(now_millis() / 1000),
            finished: false,
        })
    }

    /// Takes a full listing the Git mode made as the repository's branches, and hands the names it
    /// is the first to see to the next look after a hook. The listing is not a window: it says
    /// which branches exist, not who was acting, so a branch it sees first is still the next
    /// look's to attribute. A name this listing no longer has is dropped rather than kept waiting
    /// for a look that may never come, and loses nothing: a name outside `known` is new to the
    /// next look on `known` alone.
    fn observe(&self, repo: &Path, names: HashSet<String>) {
        let mut repos = self.lock();
        let entry = repos.entry(repo.to_path_buf()).or_default();
        if let Some(known) = &entry.known {
            entry.fresh.retain(|name| names.contains(name));
            entry
                .fresh
                .extend(names.iter().filter(|name| !known.contains(*name)).cloned());
        }
        entry.known = Some(names);
    }

    /// The gate serializing every write to one repository's rows. Both reconciling paths read
    /// every row of the repository, decide from that snapshot and then write, so two of them at
    /// once lose an attribution or delete a row the other just wrote; the hook path's own claim
    /// serializes hook looks against each other only, and several clients can be listing at the
    /// same time. The clearing a project's deletion does takes it for the same reason.
    ///
    /// It guards an order rather than any state, so a panic inside it poisons nothing worth
    /// keeping: a poisoned gate is taken all the same, since refusing it would leave the
    /// repository unattributable for the rest of the daemon's life.
    fn gate(&self, repo: &Path) -> Arc<Mutex<()>> {
        Arc::clone(&self.lock().entry(repo.to_path_buf()).or_default().gate)
    }

    /// When a reconcile last changed the repository's rows. A branch listing taken before then is
    /// no evidence about which branches exist now, however complete it is, since what that
    /// reconcile recorded cannot be in it. A reconcile that changed nothing leaves this alone: a
    /// listing older than that one is as good as a newer one.
    ///
    /// Only the order of the two matters, so both are [`Instant`]s: a wall clock stepped back —
    /// by NTP, or by the user — would otherwise put every listing before the last write for as
    /// long as the step, which stops a row ever being removed for a deleted branch.
    fn wrote_at(&self, repo: &Path) -> Option<Instant> {
        self.lock().get(repo).and_then(|entry| entry.wrote_at)
    }

    fn wrote(&self, repo: &Path, at: Instant) {
        self.lock().entry(repo.to_path_buf()).or_default().wrote_at = Some(at);
    }

    fn forget(&self, repo: &Path) {
        self.lock().remove(repo);
    }
}

// -- the hook path -----------------------------------------------------------

/// Looks at the repository of the session that fired a hook, if the stamp of its refs says anything
/// changed. Returns at once; the work runs on the blocking pool.
pub fn on_hook(state: &Arc<AppState>, session_id: &str) {
    let state = Arc::clone(state);
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let first = {
            let state = Arc::clone(&state);
            tokio::task::spawn_blocking(move || hook_arrived(&state, &session_id)).await
        };
        if let Ok(Some((repo, wait))) = first {
            tokio::time::sleep(wait).await;
            let _ = tokio::task::spawn_blocking(move || trailing_look(&state, &repo)).await;
        }
    });
}

/// Handles one hook; returns the repository and how long to wait when the look has to come later.
fn hook_arrived(state: &AppState, session_id: &str) -> Option<(PathBuf, Duration)> {
    let repo = repository_of_session(state, session_id)?;
    let stamp = Stamp::read(&repo);
    match state
        .branch_watch
        .claim(&repo, Caller::Hook(session_id), &stamp, Instant::now())
    {
        Claim::Nothing => None,
        Claim::Later(wait) => Some((repo, wait)),
        Claim::Run(scan) => {
            look(state, stamp, scan);
            None
        }
    }
}

fn trailing_look(state: &AppState, repo: &Path) {
    let stamp = Stamp::read(repo);
    if let Claim::Run(scan) =
        state
            .branch_watch
            .claim(repo, Caller::Trailing, &stamp, Instant::now())
    {
        look(state, stamp, scan);
    }
}

fn look(state: &AppState, stamp: Stamp, scan: Scan<'_>) {
    let env = match GitEnv::from_shell() {
        Ok(env) => env,
        Err(err) => {
            tracing::debug!(%err, "branch origins: no git to enumerate with");
            return;
        }
    };
    look_with(state, &env, stamp, scan);
}

/// One enumeration of the repository `scan` claimed, and what follows from it. `stamp` is the one
/// read before it, so a ref that changes while it runs makes the next hook look again.
fn look_with(state: &AppState, env: &GitEnv, stamp: Stamp, scan: Scan<'_>) {
    let cancel = AtomicBool::new(false);
    let list = match compare::branches_in(env, &scan.repo, &cancel) {
        Ok(list) => list,
        Err(err) => {
            tracing::debug!(repository = %scan.repo.display(), %err, "branch origins: listing failed");
            return;
        }
    };
    let window = Window {
        actors: &scan.actors,
        known: scan.known.as_ref(),
        fresh: &scan.fresh,
        since_secs: scan.previous_secs,
    };
    if let Err(err) = reconcile(state, &scan.repo, &list, Some(&window)) {
        tracing::debug!(repository = %scan.repo.display(), %err, "branch origins: recording failed");
        return;
    }
    let known = list
        .complete
        .then(|| list.branches.iter().map(|b| b.name.clone()).collect());
    scan.finish(stamp, known);
}

// -- the Git mode's path -----------------------------------------------------

/// Brings the origins of the repository in line with a branch listing the Git mode just made, and
/// attaches the ones that name a session to the listing. A failure leaves the listing as it is: the
/// branches are what was asked for, and the attribution is a decoration on them.
pub fn annotate(state: &AppState, repo: &Path, list: &mut BranchList) {
    let Reconciled { rows, owned } = match reconcile(state, repo, list, None) {
        Ok(reconciled) => reconciled,
        Err(err) => {
            tracing::warn!(repository = %repo.display(), %err, "branch origins: reconciling failed");
            return;
        }
    };
    if list.complete && owned {
        let names = list.branches.iter().map(|b| b.name.clone()).collect();
        state.branch_watch.observe(repo, names);
    }
    let mut by_branch: HashMap<String, BranchOriginInfo> = rows
        .into_iter()
        .filter_map(|row| Some((row.branch.clone(), shown(row)?)))
        .collect();
    for branch in &mut list.branches {
        branch.origin = by_branch.remove(&branch.name);
    }
}

fn shown(row: BranchOrigin) -> Option<BranchOriginInfo> {
    if row.source == BranchOriginSource::Unknown {
        return None;
    }
    Some(BranchOriginInfo {
        source: row.source,
        session: row.session_id?,
        title: row.session_title.unwrap_or_default(),
        created_at: row.created_at,
    })
}

// -- a project's deletion ----------------------------------------------------

/// Forgets the origins of the repository a deleted project was in, unless another project is in it
/// too. Called after the project is gone from the store. A project whose directory has since gone
/// cannot be traced to its repository, and neither can any repository when the store's list of
/// projects cannot be read: in both the rows stay, bounded by the per-repository cap.
pub fn project_deleted(state: &AppState, project_path: &str) {
    project_deleted_in(state, repository_of(Path::new(project_path)));
}

/// [`project_deleted`] for a repository found earlier, for a deletion that takes the directory
/// away first, after which it can no longer be traced.
pub fn project_deleted_in(state: &AppState, repo: Option<PathBuf>) {
    let Some(repo) = repo else {
        return;
    };
    let shared = match state.store.list_projects() {
        Ok(projects) => projects
            .iter()
            .any(|project| repository_of(Path::new(&project.path)).as_deref() == Some(&repo)),
        Err(_) => true,
    };
    if shared {
        return;
    }
    // Under the repository's gate, as every other write to its rows is: a reconcile that has read
    // its snapshot and is still deciding would otherwise write that snapshot back over the delete,
    // leaving a repository with rows, no project to clean them up and nothing that runs here again.
    let gate = state.branch_watch.gate(&repo);
    let _serialized = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Err(err) = state
        .store
        .delete_repository_branch_origins(&repo_key(&repo))
    {
        tracing::warn!(repository = %repo.display(), %err, "branch origins: clearing failed");
    }
    state.branch_watch.forget(&repo);
}

// -- reconciling -------------------------------------------------------------

/// What a look after a hook knows about the window since the previous one. It borrows the look's
/// own sets, which it only reads and which outlive it.
struct Window<'a> {
    /// The sessions that fired a hook in it.
    actors: &'a HashSet<String>,
    /// The branches of the previous look; `None` when there was none.
    known: Option<&'a HashSet<String>>,
    /// The branches a Git mode listing has seen appear since the previous look, which `known`
    /// already holds but this window has yet to attribute (see [`RepoWatch::fresh`]).
    fresh: &'a HashSet<String>,
    /// When the previous look began, in seconds since the epoch. `None` when there was no previous
    /// look of this daemon's, which leaves the window open at the far end: see [`born_in`].
    since_secs: Option<i64>,
}

/// What [`reconcile`] leaves behind.
struct Reconciled {
    /// What is stored for the repository afterwards.
    rows: Vec<BranchOrigin>,
    /// Whether a stored project was still in the repository when it took the gate. When none was,
    /// the repository is not the daemon's to remember anything about, and the in-memory record of
    /// it is dropped.
    owned: bool,
}

/// Aligns the repository's stored origins with `list`, a `git for-each-ref` of its branches, and
/// reports what is stored afterwards.
///
/// A row is removed for a branch the listing does not have only when the listing is evidence that
/// the branch is gone (`settled`, below). A row whose branch is there but no longer has the first
/// reflog entry it was made against is removed whatever the listing is — the branch was deleted
/// and made again. With a `window`, a branch new in it is attributed to the session that acted;
/// any branch left with no row is then backfilled from its reflog.
///
/// The whole of it runs under the repository's gate and leaves one transaction behind, because
/// what it writes follows from the snapshot of rows it opened with.
fn reconcile(
    state: &AppState,
    repo: &Path,
    list: &BranchList,
    window: Option<&Window<'_>>,
) -> Result<Reconciled> {
    let branches = &list.branches;
    let key = repo_key(repo);
    let gate = state.branch_watch.gate(repo);
    let _serialized = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let rows = state.store.branch_origins(&key)?;
    let live: HashSet<&str> = branches.iter().map(|b| b.name.as_str()).collect();

    // Whether this listing can be read as the repository's branches now. A cut one proves nothing
    // past the cut, and one taken before the last reconcile wrote anything proves nothing at all:
    // a `git for-each-ref` that ran before a branch was made is complete and has no trace of it,
    // so the row the hook path has since written for it would be deleted as the branch's own.
    let settled = list.complete
        && state
            .branch_watch
            .wrote_at(repo)
            .is_none_or(|wrote| wrote <= list.listed_at);

    let mut dropped = Vec::new();
    let mut recreated = HashSet::new();
    for row in &rows {
        if !live.contains(row.branch.as_str()) {
            if settled {
                dropped.push(row.branch.clone());
            }
        } else if !is_same_branch(repo, row) {
            dropped.push(row.branch.clone());
            recreated.insert(row.branch.clone());
        }
    }
    // What the surviving rows already answer. `have` keeps the reflog backfill off a branch that
    // has a row of any kind; the window path consults `attributed` instead, since an `Unknown` row
    // is a question left open and a window that can name a session answers it.
    let mut have = HashSet::new();
    let mut attributed = HashSet::new();
    for row in &rows {
        if dropped.contains(&row.branch) {
            continue;
        }
        have.insert(row.branch.clone());
        if row.source != BranchOriginSource::Unknown {
            attributed.insert(row.branch.clone());
        }
    }

    let sessions = RepositorySessions::new(state, repo);
    let mut put = Vec::new();
    if let Some(window) = window {
        let actor = window_actor(state, &sessions, window)?;
        for branch in branches {
            // The cap bounds what one window writes as it bounds the backfill's rows: a script
            // making branches in bulk would otherwise have a row inserted for each and the trim
            // drop most of them in the same transaction, by a `created_at` that is the branch's
            // birth rather than the order the rows were written. A branch already holding a row
            // adds none, so this never blocks an `Unknown` row's answer.
            if have.len() >= budget::MAX_BRANCH_ORIGINS && !have.contains(&branch.name) {
                continue;
            }
            if attributed.contains(&branch.name) || !appeared_in(window, branch, &recreated) {
                continue;
            }
            let birth = first_reflog_entry(repo, &branch.name);
            if !born_in(window, birth.as_ref()) {
                continue;
            }
            // Whoever acted in the window did not make a branch born before their session started
            // or after it ended, however alone they were in it.
            let creator = actor
                .as_ref()
                .filter(|actor| actor.was_running_at(birth.as_ref()))
                .map(|actor| &actor.creator);
            if creator.is_none() && have.contains(&branch.name) {
                // Already stored as unattributable, and this window cannot do better.
                continue;
            }
            let mut origin = origin_of(&key, &branch.name, creator, birth.as_ref());
            origin.source = if creator.is_some() {
                BranchOriginSource::HookDiff
            } else {
                BranchOriginSource::Unknown
            };
            put.push(origin);
            have.insert(branch.name.clone());
        }
    }

    for branch in branches {
        if have.len() >= budget::MAX_BRANCH_ORIGINS {
            break;
        }
        if have.contains(&branch.name) {
            continue;
        }
        let birth = first_reflog_entry(repo, &branch.name);
        let creator = match birth
            .as_ref()
            .filter(|birth| birth.message.starts_with(CREATED_FROM))
        {
            Some(birth) => sessions.alive_alone_at(birth.time)?,
            None => None,
        };
        let mut origin = origin_of(&key, &branch.name, creator.as_ref(), birth.as_ref());
        origin.source = if creator.is_some() {
            BranchOriginSource::ReflogMatch
        } else {
            BranchOriginSource::Unknown
        };
        put.push(origin);
        have.insert(branch.name.clone());
    }

    // A listing and a hook both read the project before they take the gate, with a `git` or two
    // in between, so a deletion can start and finish in that gap. With no project left in the
    // repository nothing will clear what is written now (`project_deleted` is past), and every
    // such row would be `Unknown` for want of sessions to name, which then blocks the backfill
    // for the branch if a project is ever added back. Removals still apply: they only tidy. The
    // same deletion leaves nothing to clear the repository's in-memory record either, so it goes
    // here, and the caller does not set it up again.
    let owned = sessions.has_project()?;
    if !owned {
        put.clear();
        state.branch_watch.forget(repo);
    }

    if dropped.is_empty() && put.is_empty() {
        return Ok(Reconciled { rows, owned });
    }
    let rows =
        state
            .store
            .apply_branch_origins(&key, &dropped, &put, budget::MAX_BRANCH_ORIGINS)?;
    if owned {
        state.branch_watch.wrote(repo, Instant::now());
    }
    Ok(Reconciled { rows, owned })
}

/// Whether `branch` is one the window made: absent from the previous look, one a Git mode listing
/// saw appear since that look, or a name the previous look had whose row just proved to be of an
/// earlier branch.
fn appeared_in(window: &Window<'_>, branch: &BranchInfo, recreated: &HashSet<String>) -> bool {
    match window.known {
        Some(known) => {
            !known.contains(&branch.name)
                || window.fresh.contains(&branch.name)
                || recreated.contains(&branch.name)
        }
        None => false,
    }
}

/// Whether a branch born then was born inside the window. The window begins at the previous look:
/// an older birth is a branch that was not made in it — a reflog expired down to its first
/// surviving entry, a rename, or a name a listing handed over long before the hook arrived.
///
/// A window with no previous look behind it has no beginning, and holds nothing: everything it
/// could claim may as well predate the whole daemon. A branch whose birth cannot be read at all
/// (no reflog, as in a reftable repository) is left to the window, which is the only signal there.
fn born_in(window: &Window<'_>, birth: Option<&ReflogEntry>) -> bool {
    match (window.since_secs, birth) {
        (Some(since), Some(birth)) => since <= birth.time + CLOCK_SLACK_SECS,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// The session a window's branches are attributed to: the only one that acted in it, with no other
/// session of the repository mid-turn that could have made a branch before its own hook fired.
fn window_actor(
    state: &AppState,
    sessions: &RepositorySessions<'_>,
    window: &Window<'_>,
) -> Result<Option<WindowActor>> {
    let mut actors = window.actors.iter();
    let (Some(actor), None) = (actors.next(), actors.next()) else {
        return Ok(None);
    };
    // `all` leaves out a session that has had no turn yet, which the actor (read below without
    // that filter) may be. A session is `Working` only from a hook that also starts its
    // conversation, so one that has no conversation is not mid-turn and cannot be a peer.
    let peers_working = sessions
        .all()?
        .iter()
        .any(|s| s.id != *actor && s.status == SessionStatus::Working);
    if peers_working {
        return Ok(None);
    }
    let Some(session) = state.store.get_session(actor)? else {
        return Ok(None);
    };
    Ok(Some(WindowActor {
        creator: Creator::of(&session),
        started_secs: session.started_at / 1000,
        ended_secs: session.ended_at.map(|ended| ended / 1000),
    }))
}

/// The session a hook window names, with the lifetime a branch it is given has to fall in.
struct WindowActor {
    creator: Creator,
    started_secs: i64,
    ended_secs: Option<i64>,
}

impl WindowActor {
    /// Whether the session was running when the branch was made. A branch whose birth cannot be
    /// read is left to the window alone, which is all a reftable repository has.
    fn was_running_at(&self, birth: Option<&ReflogEntry>) -> bool {
        let Some(birth) = birth else {
            return true;
        };
        self.started_secs <= birth.time + CLOCK_SLACK_SECS
            && self
                .ended_secs
                .is_none_or(|ended| birth.time <= ended + CLOCK_SLACK_SECS)
    }
}

/// A row for `branch`, its source still to be set by the caller.
fn origin_of(
    repository: &str,
    branch: &str,
    creator: Option<&Creator>,
    birth: Option<&ReflogEntry>,
) -> BranchOrigin {
    BranchOrigin {
        repository: repository.to_string(),
        branch: branch.to_string(),
        session_id: creator.map(|c| c.id.clone()),
        session_title: creator.map(|c| c.title.clone()),
        source: BranchOriginSource::Unknown,
        created_at: birth.map_or_else(now_millis, |b| b.time * 1000),
        birth_commit: birth.map(|b| b.commit.clone()),
        birth_time: birth.map(|b| b.time),
    }
}

/// Whether the branch is still the one `row` was made for. A row made without a reflog to hold it
/// against cannot be told apart, and stands until the branch goes.
fn is_same_branch(repo: &Path, row: &BranchOrigin) -> bool {
    let (Some(commit), Some(time)) = (&row.birth_commit, row.birth_time) else {
        return true;
    };
    first_reflog_entry(repo, &row.branch)
        .is_some_and(|entry| entry.time == time && entry.commit == *commit)
}

/// A session as a row remembers it.
#[derive(Debug, Clone)]
struct Creator {
    id: String,
    title: String,
}

impl Creator {
    fn of(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            title: session
                .title
                .chars()
                .take(budget::MAX_ORIGIN_TITLE_CHARS)
                .collect(),
        }
    }
}

/// The stored sessions that run in a repository, read once and only if something asks.
///
/// A failed read is not an empty repository: read that way, every branch is backfilled as
/// unattributable, and a row of that kind is what keeps the backfill off the branch for good — so
/// one busy moment of the store would leave the repository permanently unattributable. The failure
/// fails the whole look instead, and the next one tries again.
struct RepositorySessions<'a> {
    state: &'a AppState,
    repo: &'a Path,
    projects: OnceCell<HashSet<String>>,
    sessions: OnceCell<Vec<Session>>,
}

impl<'a> RepositorySessions<'a> {
    fn new(state: &'a AppState, repo: &'a Path) -> Self {
        Self {
            state,
            repo,
            projects: OnceCell::new(),
            sessions: OnceCell::new(),
        }
    }

    /// The ids of the stored projects whose directory is in the repository.
    fn projects(&self) -> Result<&HashSet<String>> {
        if let Some(projects) = self.projects.get() {
            return Ok(projects);
        }
        let here = self
            .state
            .store
            .list_projects()?
            .into_iter()
            .filter(|p| repository_of(Path::new(&p.path)).as_deref() == Some(self.repo))
            .map(|p| p.id)
            .collect();
        Ok(self.projects.get_or_init(|| here))
    }

    /// Whether any stored project is in the repository.
    fn has_project(&self) -> Result<bool> {
        Ok(!self.projects()?.is_empty())
    }

    /// The sessions of the projects in the repository. A console session runs in a directory of
    /// Octoboard's own, and a session that never had a turn cannot have made a branch.
    fn all(&self) -> Result<&[Session]> {
        if let Some(sessions) = self.sessions.get() {
            return Ok(sessions);
        }
        let here = self.projects()?;
        let sessions = self
            .state
            .store
            .list_sessions()?
            .into_iter()
            .filter(|s| s.project_id.as_ref().is_some_and(|id| here.contains(id)))
            .filter(|s| s.has_conversation)
            .collect();
        Ok(self.sessions.get_or_init(|| sessions))
    }

    /// The one session whose lifetime holds `secs`, or `None` when none does or several do. The
    /// slack widens who counts as alive, which can only turn a name into none.
    fn alive_alone_at(&self, secs: i64) -> Result<Option<Creator>> {
        let mut alive = self.all()?.iter().filter(|s| {
            s.started_at / 1000 <= secs + CLOCK_SLACK_SECS
                && s.ended_at
                    .is_none_or(|ended| secs <= ended / 1000 + CLOCK_SLACK_SECS)
        });
        let (Some(only), None) = (alive.next(), alive.next()) else {
            return Ok(None);
        };
        Ok(Some(Creator::of(only)))
    }
}

// -- repositories and reflogs ------------------------------------------------

/// The key a repository's rows are stored under: its canonical common directory, as a wire path.
fn repo_key(repo: &Path) -> String {
    wire_path::encode(repo.as_os_str().as_bytes())
}

/// The canonical common directory of the repository holding `dir`, found from the filesystem
/// alone: this runs for every hook, and spawning `git` to ask would defeat the point of the stamp.
/// `None` for a directory in no repository.
pub fn repository_of(dir: &Path) -> Option<PathBuf> {
    let ceiling = source::home_ceiling();
    let canonical = fs::canonicalize(dir).ok()?;
    let top = source::find_dot_git(&canonical, ceiling.as_deref())?;
    let admin = source::admin_dir(&top)?;
    // A linked worktree's git directory names the common one; the main worktree's is it.
    match fs::read(admin.join("commondir")) {
        Ok(pointer) => {
            let pointer = pointer.strip_suffix(b"\n").unwrap_or(&pointer);
            fs::canonicalize(admin.join(OsStr::from_bytes(pointer))).ok()
        }
        Err(_) => Some(admin),
    }
}

fn repository_of_session(state: &AppState, session_id: &str) -> Option<PathBuf> {
    let session = state.store.get_session(session_id).ok()??;
    let project = state
        .store
        .get_project(session.project_id.as_deref()?)
        .ok()??;
    repository_of(Path::new(&project.path))
}

/// The first entry of a branch's reflog.
#[derive(Debug, PartialEq, Eq)]
struct ReflogEntry {
    /// The commit the branch named after the entry.
    commit: String,
    /// Seconds since the epoch.
    time: i64,
    message: Vec<u8>,
}

/// The first line of `logs/refs/heads/<branch>`, if the branch has a reflog.
fn first_reflog_entry(common_dir: &Path, branch: &str) -> Option<ReflogEntry> {
    let name = wire_path::decode(branch)?;
    let path = common_dir
        .join("logs/refs/heads")
        .join(OsStr::from_bytes(&name));
    let file = fs::File::open(path).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(MAX_REFLOG_LINE))
        .read_until(b'\n', &mut line)
        .ok()?;
    parse_reflog_line(&line)
}

/// `<old> <new> <identity> <seconds> <zone>\t<message>`. The identity holds spaces of its own, so
/// the time is found from the right.
fn parse_reflog_line(line: &[u8]) -> Option<ReflogEntry> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let (head, message) = match line.iter().position(|&b| b == b'\t') {
        Some(at) => (&line[..at], &line[at + 1..]),
        None => (line, &[][..]),
    };
    let mut fields = head.splitn(3, |&b| b == b' ');
    fields.next()?;
    let commit = std::str::from_utf8(fields.next()?).ok()?.to_string();
    let mut tail = fields.next()?.rsplitn(3, |&b| b == b' ');
    tail.next()?;
    let time = std::str::from_utf8(tail.next()?).ok()?.parse().ok()?;
    Some(ReflogEntry {
        commit,
        time,
        message: message.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Agent, Console, Origin, Project, ProjectAccount, ProjectSource, Role};
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{
        app_state, git, git_stdin_env, isolated_git_env, move_directory_time_back, repo_with,
        ScratchDir, PATIENCE, SYSTEM_GIT,
    };

    struct Fixture {
        state: Arc<AppState>,
        repo: ScratchDir,
        common_dir: PathBuf,
        _dir: ScratchDir,
    }

    /// A project of the fixture's console at `path`.
    fn project(id: &str, path: &Path) -> Project {
        Project {
            id: id.to_string(),
            console_id: "console".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: id.to_string(),
            path: path.to_string_lossy().into_owned(),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            claude_account_id: ProjectAccount::Inherit,
            codex_account_id: ProjectAccount::Inherit,
            grok_account_id: ProjectAccount::Inherit,
            claude_session_model: None,
            claude_session_effort: None,
            codex_session_model: None,
            codex_session_effort: None,
            grok_session_model: None,
            grok_session_effort: None,
            trust_consent: false,
            pinned: false,
            tags: Vec::new(),
        }
    }

    impl Fixture {
        fn new() -> Self {
            let (state, dir) = app_state("branch-origin");
            let workdir = dir.join("workdir");
            std::fs::create_dir(&workdir).unwrap();
            state
                .store
                .insert_console(&Console {
                    id: "console".to_string(),
                    name: "Console".to_string(),
                    workdir: workdir.to_string_lossy().into_owned(),
                    console_session_agent: Agent::Claude,
                    default_agent: Agent::Claude,
                    claude_account_id: None,
                    codex_account_id: None,
                    grok_account_id: None,
                    claude_console_session_model: None,
                    claude_console_session_effort: None,
                    codex_console_session_model: None,
                    codex_console_session_effort: None,
                    grok_console_session_model: None,
                    grok_console_session_effort: None,
                    icon: None,
                    created_at: 0,
                })
                .unwrap();
            let repo = repo_with("branch-origin-repo", &[(b"a", b"a")]);
            state
                .store
                .insert_project(&project("project", &repo))
                .unwrap();
            let common_dir = repository_of(&repo).unwrap();
            Self {
                state,
                repo,
                common_dir,
                _dir: dir,
            }
        }

        /// A session of the project that started `started_secs` ago and is still running.
        fn session(&self, id: &str, started_secs: i64, status: SessionStatus) {
            self.session_between(id, now_millis() - started_secs * 1000, None, status);
        }

        /// A session of the project with a lifetime of its own, in milliseconds since the epoch.
        fn session_between(
            &self,
            id: &str,
            started_at: i64,
            ended_at: Option<i64>,
            status: SessionStatus,
        ) {
            self.state
                .store
                .insert_session(&Session {
                    id: id.to_string(),
                    agent: Agent::Claude,
                    agent_session_id: None,
                    console_id: "console".to_string(),
                    project_id: Some("project".to_string()),
                    host_id: LOCAL_HOST_ID.to_string(),
                    role: Role::Project,
                    origin: Origin::User,
                    title: format!("Session {id}"),
                    status,
                    terminal_title: None,
                    model: None,
                    effort: None,
                    has_conversation: true,
                    bound_to: None,
                    lead: false,
                    colour: None,
                    ordinal: None,
                    account_id: None,
                    config_dir: None,
                    pinned: false,
                    started_at,
                    ended_at,
                })
                .unwrap();
        }

        fn env(&self) -> GitEnv {
            GitEnv::new(SYSTEM_GIT.to_string(), isolated_git_env())
        }

        fn list(&self) -> BranchList {
            compare::branches_in(&self.env(), &self.common_dir, &AtomicBool::new(false)).unwrap()
        }

        /// A look after a hook of `actors`, with the previous one having found `known`.
        fn hook_look(&self, actors: &[&str], known: &HashSet<String>) -> Vec<BranchOrigin> {
            let actors: HashSet<String> = actors.iter().map(|a| a.to_string()).collect();
            let fresh = HashSet::new();
            let window = Window {
                actors: &actors,
                known: Some(known),
                fresh: &fresh,
                since_secs: Some(now_millis() / 1000 - 60),
            };
            reconcile(&self.state, &self.common_dir, &self.list(), Some(&window))
                .unwrap()
                .rows
        }

        /// The hook path's own look, driven through the watch so the window is the one the watch
        /// hands out rather than one the test wrote. Far enough past the floor to run at once.
        fn look_after_hook(&self, session: &str) {
            self.run_claim(Caller::Hook(session));
        }

        /// The look the floor held back, as `on_hook` runs it once its wait is over.
        fn look_when_the_floor_is_over(&self) {
            self.run_claim(Caller::Trailing);
        }

        fn run_claim(&self, caller: Caller<'_>) {
            let stamp = Stamp::read(&self.common_dir);
            let claim = self.state.branch_watch.claim(
                &self.common_dir,
                caller,
                &stamp,
                Instant::now() + FLOOR,
            );
            let Claim::Run(scan) = claim else {
                panic!("the look is claimed");
            };
            look_with(&self.state, &self.env(), stamp, scan);
        }

        /// Moves `refs/heads` back in time, so a ref written after a look changes the stamp
        /// whatever the filesystem's time resolution is. That directory only: a branch whose name
        /// has a slash in it is written in one below it, which this leaves as it is.
        fn age_the_refs(&self) {
            move_directory_time_back(&self.common_dir.join("refs/heads"));
        }

        /// Branches at `HEAD`, each with the first reflog entry a branch's own creation writes,
        /// carrying `at_secs` as its time. One `git` for the lot, where a `git branch` each would
        /// be a thousand processes.
        fn branches_at(&self, names: &[String], at_secs: i64) {
            let head = String::from_utf8(git(&self.repo, &["rev-parse", "HEAD"])).unwrap();
            let refs: String = names
                .iter()
                .map(|name| format!("create refs/heads/{name} {}\n", head.trim()))
                .collect();
            git_stdin_env(
                &self.repo,
                &["update-ref", "-m", "branch: Created from HEAD", "--stdin"],
                &[("GIT_COMMITTER_DATE", &format!("{at_secs} +0000"))],
                refs.as_bytes(),
            );
        }

        fn names(&self) -> HashSet<String> {
            self.list().branches.into_iter().map(|b| b.name).collect()
        }

        fn rows(&self) -> Vec<BranchOrigin> {
            self.state
                .store
                .branch_origins(&repo_key(&self.common_dir))
                .unwrap()
        }

        fn row(&self, branch: &str) -> Option<BranchOrigin> {
            self.rows().into_iter().find(|row| row.branch == branch)
        }

        fn reconcile_full(&self) -> Vec<BranchOrigin> {
            let list = self.list();
            reconcile(&self.state, &self.common_dir, &list, None)
                .unwrap()
                .rows
        }
    }

    #[test]
    fn a_branch_new_in_a_hook_window_belongs_to_the_session_that_fired_it() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Working);
        let known = fixture.names();
        git(&fixture.repo, &["switch", "-q", "-c", "feature"]);

        let rows = fixture.hook_look(&["a"], &known);

        let row = rows.iter().find(|row| row.branch == "feature").unwrap();
        assert_eq!(row.source, BranchOriginSource::HookDiff);
        assert_eq!(row.session_id.as_deref(), Some("a"));
        assert_eq!(row.session_title.as_deref(), Some("Session a"));
        assert!(row.birth_commit.is_some(), "held against the reflog");
        assert!(
            !rows
                .iter()
                .any(|row| row.branch == "main" && row.source == BranchOriginSource::HookDiff),
            "a branch already known is not new"
        );
    }

    #[test]
    fn two_sessions_in_one_window_leave_the_branch_unknown() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        fixture.session("b", 600, SessionStatus::Idle);
        let known = fixture.names();
        git(&fixture.repo, &["branch", "shared"]);

        fixture.hook_look(&["a", "b"], &known);
        assert_eq!(
            fixture.row("shared").unwrap().source,
            BranchOriginSource::Unknown
        );

        // A session mid-turn elsewhere in the repository could have made one as well.
        fixture.session("c", 600, SessionStatus::Working);
        let known = fixture.names();
        git(&fixture.repo, &["branch", "other"]);
        fixture.hook_look(&["a"], &known);
        let row = fixture.row("other").unwrap();
        assert_eq!(
            (row.source, row.session_id),
            (BranchOriginSource::Unknown, None)
        );
    }

    /// A window names whoever acted in it, and a session cannot have made a branch that was
    /// already there when it started.
    #[test]
    fn a_branch_older_than_the_session_that_fired_the_hook_is_not_its() {
        let fixture = Fixture::new();
        git(&fixture.repo, &["branch", "earlier"]);
        // The session starts after the branch exists: its first hook still opens a window, and
        // the branch is still one the previous look did not have.
        fixture.session_between("a", now_millis() + 60_000, None, SessionStatus::Working);

        fixture.hook_look(&["a"], &HashSet::from(["main".to_string()]));

        let row = fixture.row("earlier").unwrap();
        assert_eq!(
            (row.source, row.session_id),
            (BranchOriginSource::Unknown, None)
        );
    }

    /// The window begins at the previous look. With no look behind it, nothing says how long ago
    /// what a listing handed it appeared, so it names nobody.
    #[test]
    fn a_window_with_no_look_behind_it_attributes_nothing() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        // A second session alive at the branch's birth, so the reflog cannot name one either and
        // the row left behind is the window's own answer.
        fixture.session("b", 600, SessionStatus::Idle);
        git(&fixture.repo, &["branch", "loose"]);
        let known = fixture.names();
        let actors = HashSet::from(["a".to_string()]);
        let fresh = HashSet::from(["loose".to_string()]);
        let window = Window {
            actors: &actors,
            known: Some(&known),
            fresh: &fresh,
            since_secs: None,
        };

        reconcile(
            &fixture.state,
            &fixture.common_dir,
            &fixture.list(),
            Some(&window),
        )
        .unwrap();

        let row = fixture.row("loose").unwrap();
        assert_eq!(
            (row.source, row.session_id),
            (BranchOriginSource::Unknown, None)
        );
    }

    /// One window writes no more rows than the cap. Past it the trim would drop most of them again
    /// in the same transaction, and by the branches' birth times rather than by what this window
    /// found.
    #[test]
    fn a_window_past_the_cap_writes_no_more_rows_than_it() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Working);
        let names: Vec<String> = (0..budget::MAX_BRANCH_ORIGINS)
            .map(|n| format!("b{n:05}"))
            .collect();
        // Inside the window, and older than the branch made after them, which is what the trim
        // would keep over them.
        fixture.branches_at(&names, now_millis() / 1000 - 30);
        git(&fixture.repo, &["branch", "zz-last"]);

        fixture.hook_look(&["a"], &HashSet::from(["main".to_string()]));

        assert_eq!(fixture.rows().len(), budget::MAX_BRANCH_ORIGINS);
        assert!(
            fixture.row("zz-last").is_none(),
            "the branches past the cap are the ones the listing reaches last"
        );
    }

    /// A read of the store that fails is not "no session of this repository": recorded that way,
    /// the branch is stored as unattributable and the reflog is never consulted for it again.
    #[test]
    fn a_failed_read_of_the_sessions_fails_the_look() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        git(&fixture.repo, &["branch", "earlier"]);
        let list = fixture.list();
        fixture
            .state
            .store
            .connection()
            .execute_batch("DROP TABLE sessions")
            .unwrap();

        assert!(reconcile(&fixture.state, &fixture.common_dir, &list, None).is_err());
        assert!(fixture.rows().is_empty(), "nothing is recorded from it");
    }

    #[test]
    fn a_branch_the_panel_listed_first_is_still_the_next_look_to_attribute() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Working);
        fixture.session("b", 600, SessionStatus::Idle);
        fixture.age_the_refs();
        fixture.look_after_hook("a");
        git(&fixture.repo, &["switch", "-q", "-c", "feature"]);

        // The panel polls while the hook's own look is still held back by the floor: two sessions
        // were alive at the branch's birth, so the listing can name none, and it moves the
        // baseline of known branches on to what it found.
        let mut list = fixture.list();
        annotate(&fixture.state, &fixture.common_dir, &mut list);
        assert_eq!(
            fixture.row("feature").unwrap().source,
            BranchOriginSource::Unknown
        );

        fixture.look_after_hook("a");

        let row = fixture.row("feature").unwrap();
        assert_eq!(
            (row.source, row.session_id.as_deref()),
            (BranchOriginSource::HookDiff, Some("a")),
            "what the listing could not tell, the window still can"
        );
    }

    #[test]
    fn the_look_the_floor_held_back_attributes_what_appeared_in_its_window() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Working);
        fixture.age_the_refs();
        fixture.look_after_hook("a");
        git(&fixture.repo, &["switch", "-q", "-c", "feature"]);

        let (repo, wait) =
            hook_arrived(&fixture.state, "a").expect("the look is held back, not dropped");
        assert_eq!(repo, fixture.common_dir);
        assert!(wait <= FLOOR);
        assert!(fixture.row("feature").is_none(), "nothing looked yet");

        fixture.look_when_the_floor_is_over();

        let row = fixture.row("feature").unwrap();
        assert_eq!(
            (row.source, row.session_id.as_deref()),
            (BranchOriginSource::HookDiff, Some("a"))
        );
    }

    #[test]
    fn a_branch_with_no_session_to_name_is_not_sent() {
        let fixture = Fixture::new();
        git(&fixture.repo, &["branch", "mine"]);
        let mut list = fixture.list();

        annotate(&fixture.state, &fixture.common_dir, &mut list);

        assert!(list.branches.iter().all(|branch| branch.origin.is_none()));
        assert_eq!(
            fixture.row("mine").unwrap().source,
            BranchOriginSource::Unknown
        );
    }

    #[test]
    fn a_branch_the_daemon_never_saw_appear_is_matched_by_its_reflog() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        git(&fixture.repo, &["branch", "earlier"]);

        let mut list = fixture.list();
        annotate(&fixture.state, &fixture.common_dir, &mut list);

        let origin = list
            .branches
            .iter()
            .find(|branch| branch.name == "earlier")
            .and_then(|branch| branch.origin.clone())
            .unwrap();
        assert_eq!(
            (origin.source, origin.session.as_str()),
            (BranchOriginSource::ReflogMatch, "a")
        );

        // With a second session alive at that second it cannot be told.
        fixture.session("b", 600, SessionStatus::Idle);
        git(&fixture.repo, &["branch", "later"]);
        fixture.reconcile_full();
        assert_eq!(
            fixture.row("later").unwrap().source,
            BranchOriginSource::Unknown
        );
    }

    #[test]
    fn a_session_that_ended_a_second_before_the_branch_still_counts_as_alive_for_it() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        let ended = now_millis();
        fixture.session_between("b", ended - 600_000, Some(ended), SessionStatus::Idle);
        fixture.branches_at(&["close".to_string()], ended / 1000 + 1);

        fixture.reconcile_full();

        assert_eq!(
            fixture.row("close").unwrap().source,
            BranchOriginSource::Unknown,
            "two sessions are within the clock's slack of the branch"
        );
    }

    #[test]
    fn a_branch_made_again_under_the_same_name_does_not_inherit_the_attribution() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        let known = fixture.names();
        git(&fixture.repo, &["branch", "topic"]);
        fixture.hook_look(&["a"], &known);
        assert_eq!(
            fixture.row("topic").unwrap().session_id.as_deref(),
            Some("a")
        );

        // A different tip, so the new reflog's first entry cannot equal the old one even within
        // the same second.
        git(&fixture.repo, &["branch", "-q", "-D", "topic"]);
        git(
            &fixture.repo,
            &["commit", "-q", "--allow-empty", "-m", "next"],
        );
        git(&fixture.repo, &["branch", "topic"]);
        fixture.session("b", 600, SessionStatus::Idle);
        let rows = fixture.reconcile_full();

        let row = rows.iter().find(|row| row.branch == "topic").unwrap();
        assert_ne!(row.session_id.as_deref(), Some("a"), "the old row is void");
        assert_eq!(
            row.source,
            BranchOriginSource::Unknown,
            "two sessions were alive"
        );
    }

    #[test]
    fn a_row_goes_with_its_branch_but_only_on_a_full_listing() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        git(&fixture.repo, &["branch", "gone"]);
        fixture.reconcile_full();
        assert!(fixture.row("gone").is_some());
        git(&fixture.repo, &["branch", "-q", "-D", "gone"]);

        let cut = BranchList {
            complete: false,
            ..fixture.list()
        };
        reconcile(&fixture.state, &fixture.common_dir, &cut, None).unwrap();
        assert!(
            fixture.row("gone").is_some(),
            "a cut listing proves nothing"
        );

        fixture.reconcile_full();
        assert!(fixture.row("gone").is_none());
    }

    #[test]
    fn a_listing_older_than_the_last_record_does_not_remove_it() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Working);
        // A reply listed before the branch was made, as a client's request that overlapped the
        // hook's look carries.
        let stale = fixture.list();
        let known = fixture.names();
        git(&fixture.repo, &["switch", "-q", "-c", "feature"]);
        fixture.hook_look(&["a"], &known);
        assert_eq!(
            fixture.row("feature").unwrap().source,
            BranchOriginSource::HookDiff
        );

        let mut list = stale;
        annotate(&fixture.state, &fixture.common_dir, &mut list);

        assert!(
            fixture.row("feature").is_some(),
            "the branch is missing from the listing because it was made after it"
        );
    }

    #[test]
    fn deleting_a_project_leaves_the_origins_of_another_in_the_same_repository() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        let inner = fixture.repo.join("inner");
        std::fs::create_dir(&inner).unwrap();
        fixture
            .state
            .store
            .insert_project(&project("other", &inner))
            .unwrap();
        git(&fixture.repo, &["branch", "topic"]);
        fixture.reconcile_full();
        assert!(fixture.row("topic").is_some());

        fixture.state.store.delete_project("project").unwrap();
        project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
        assert!(
            fixture.row("topic").is_some(),
            "another project is still in the repository"
        );

        fixture.state.store.delete_project("other").unwrap();
        project_deleted(&fixture.state, &inner.to_string_lossy());
        assert!(fixture.rows().is_empty(), "the last project's deletion");
    }

    /// The rows of a repository are written under its gate, a project's deletion included: a
    /// reconcile holding the gate has read the rows it is about to write, and would put back what
    /// the deletion just removed — leaving a repository with rows, no project to clear them and
    /// nothing that runs here for it again.
    #[test]
    fn deleting_a_project_waits_for_a_reconcile_holding_the_gate() {
        let fixture = Fixture::new();
        git(&fixture.repo, &["branch", "topic"]);
        fixture.reconcile_full();
        assert!(fixture.row("topic").is_some());

        let gate = fixture.state.branch_watch.gate(&fixture.common_dir);
        let held = gate.lock().unwrap();
        let (done, deleted) = std::sync::mpsc::channel();
        let state = Arc::clone(&fixture.state);
        let path = fixture.repo.to_string_lossy().into_owned();
        let worker = std::thread::spawn(move || {
            state.store.delete_project("project").unwrap();
            project_deleted(&state, &path);
            done.send(()).unwrap();
        });

        // The worker is at the gate once it is past the store's deletion; before that, nothing
        // has been waited for.
        let start = Instant::now();
        while fixture
            .state
            .store
            .get_project("project")
            .unwrap()
            .is_some()
        {
            assert!(start.elapsed() < PATIENCE, "the worker never started");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(fixture.row("topic").is_some());
        assert!(
            deleted.try_recv().is_err(),
            "the deletion waits for the gate"
        );
        drop(held);

        deleted
            .recv_timeout(PATIENCE)
            .expect("the deletion goes on");
        worker.join().unwrap();
        assert!(fixture.rows().is_empty());
    }

    /// A reconcile that read its project before a deletion finished and takes the gate after it
    /// has nothing to name branches for, and nothing would clear what it wrote.
    #[test]
    fn a_reconcile_after_the_last_project_went_writes_nothing() {
        let fixture = Fixture::new();
        git(&fixture.repo, &["branch", "topic"]);
        let list = fixture.list();
        fixture.reconcile_full();
        fixture.state.store.delete_project("project").unwrap();
        project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
        assert!(fixture.rows().is_empty());

        reconcile(&fixture.state, &fixture.common_dir, &list, None).unwrap();

        assert!(fixture.rows().is_empty());
    }

    /// The same late listing must not set the in-memory record of the repository up again either:
    /// nothing is left to clear it, and it would hold every branch name until the daemon restarts.
    #[test]
    fn a_listing_after_the_last_project_went_leaves_no_record_of_the_repository() {
        let fixture = Fixture::new();
        let mut list = fixture.list();
        fixture.state.store.delete_project("project").unwrap();
        project_deleted(&fixture.state, &fixture.repo.to_string_lossy());
        assert!(fixture.state.branch_watch.lock().is_empty());

        annotate(&fixture.state, &fixture.common_dir, &mut list);

        assert!(fixture.state.branch_watch.lock().is_empty());
    }

    /// What the `list_project_branches` reply reserves room for (`budget::BRANCH_ORIGIN_BYTES`)
    /// holds only because one listing attaches no more origins than the cap.
    #[test]
    fn a_listing_attaches_no_more_origins_than_the_cap() {
        let fixture = Fixture::new();
        fixture.session("a", 600, SessionStatus::Idle);
        let names: Vec<String> = (0..=budget::MAX_BRANCH_ORIGINS)
            .map(|n| format!("b{n:05}"))
            .collect();
        fixture.branches_at(&names, now_millis() / 1000);

        let mut list = fixture.list();
        annotate(&fixture.state, &fixture.common_dir, &mut list);

        assert!(list.complete, "the listing itself is not cut");
        assert_eq!(
            list.branches
                .iter()
                .filter(|branch| branch.origin.is_some())
                .count(),
            budget::MAX_BRANCH_ORIGINS
        );
    }

    #[test]
    fn rows_past_the_cap_are_dropped_oldest_first() {
        let fixture = Fixture::new();
        let key = repo_key(&fixture.common_dir);
        let put: Vec<BranchOrigin> = (0..5)
            .map(|n| {
                let mut row = origin_of(&key, &format!("b{n}"), None, None);
                row.created_at = n;
                row
            })
            .collect();
        fixture
            .state
            .store
            .apply_branch_origins(&key, &[], &put, 2)
            .unwrap();
        let kept: Vec<_> = fixture.rows().into_iter().map(|row| row.branch).collect();
        assert_eq!(kept, ["b3", "b4"]);
    }

    #[test]
    fn the_stamp_sees_a_branch_below_a_directory_that_already_existed() {
        let fixture = Fixture::new();
        git(&fixture.repo, &["branch", "feat/one"]);
        // Every directory the stamp covers, not just the top one, so that what the next branch
        // changes is the directory below rather than the one `age_the_refs` would have moved.
        for dir in ["refs/heads", "refs/heads/feat"] {
            move_directory_time_back(&fixture.common_dir.join(dir));
        }
        let before = Stamp::read(&fixture.common_dir);
        assert_eq!(before, Stamp::read(&fixture.common_dir));

        git(&fixture.repo, &["branch", "feat/two"]);

        assert_ne!(before, Stamp::read(&fixture.common_dir));
    }

    #[test]
    fn a_hook_inside_the_floor_waits_for_it_and_remembers_its_session() {
        let watch = Watch::default();
        let repo = Path::new("/repo");
        let stamp = |n| Stamp {
            directories: n,
            newest: None,
            packed_refs: None,
            reftable: None,
        };
        let now = Instant::now();

        let Claim::Run(scan) = watch.claim(repo, Caller::Hook("a"), &stamp(1), now) else {
            panic!("the first hook looks");
        };
        scan.finish(stamp(1), Some(HashSet::new()));
        assert!(matches!(
            watch.claim(repo, Caller::Hook("a"), &stamp(1), now),
            Claim::Nothing
        ));

        let Claim::Later(wait) = watch.claim(repo, Caller::Hook("b"), &stamp(2), Instant::now())
        else {
            panic!("a changed stamp inside the floor waits");
        };
        assert!(wait <= FLOOR);
        assert!(
            matches!(
                watch.claim(repo, Caller::Hook("c"), &stamp(2), Instant::now()),
                Claim::Nothing
            ),
            "one trailing look is enough"
        );

        let later = Instant::now() + FLOOR;
        let Claim::Run(scan) = watch.claim(repo, Caller::Trailing, &stamp(2), later) else {
            panic!("the trailing look runs");
        };
        let mut actors: Vec<_> = scan.actors.iter().cloned().collect();
        actors.sort();
        assert_eq!(actors, ["b", "c"]);
    }

    #[test]
    fn a_look_that_failed_leaves_the_next_one_its_whole_window() {
        let watch = Watch::default();
        let repo = Path::new("/repo");
        let stamp = |n| Stamp {
            directories: n,
            newest: None,
            packed_refs: None,
            reftable: None,
        };
        let main = HashSet::from(["main".to_string()]);
        let Claim::Run(first) = watch.claim(repo, Caller::Hook("a"), &stamp(1), Instant::now())
        else {
            panic!("the first hook looks");
        };
        first.finish(stamp(1), Some(main.clone()));
        watch.observe(
            repo,
            HashSet::from(["main".to_string(), "feature".to_string()]),
        );
        // A baseline well in the past, so advancing it would be visible inside one second.
        let baseline = Some(1_700_000_000);
        watch.lock().get_mut(repo).unwrap().looked_at_secs = baseline;

        let later = Instant::now() + FLOOR;
        let Claim::Run(failed) = watch.claim(repo, Caller::Hook("b"), &stamp(2), later) else {
            panic!("the second hook looks");
        };
        assert_eq!(failed.previous_secs, baseline);
        assert_eq!(failed.fresh, HashSet::from(["feature".to_string()]));
        drop(failed);

        // The failed look ended just now, so the next one comes once its own floor is over.
        let past_the_floor = Instant::now() + FLOOR;
        let Claim::Run(next) = watch.claim(repo, Caller::Hook("c"), &stamp(2), past_the_floor)
        else {
            panic!("the next hook looks");
        };
        assert_eq!(
            next.previous_secs, baseline,
            "a branch made before the failed look is still inside the window"
        );
        assert_eq!(next.fresh, HashSet::from(["feature".to_string()]));
        let mut actors: Vec<_> = next.actors.iter().cloned().collect();
        actors.sort();
        assert_eq!(actors, ["b", "c"]);
    }

    #[test]
    fn a_reflog_line_is_read_from_the_right() {
        let line = b"0000000000000000000000000000000000000000 1234567890abcdef1234567890abcdef12345678 A B <a@b.c> 1700000000 +0900\tbranch: Created from HEAD\n";
        let entry = parse_reflog_line(line).unwrap();
        assert_eq!(entry.commit, "1234567890abcdef1234567890abcdef12345678");
        assert_eq!(entry.time, 1_700_000_000);
        assert_eq!(entry.message, b"branch: Created from HEAD");
    }

    #[test]
    fn a_linked_worktree_resolves_to_the_common_directory() {
        let fixture = Fixture::new();
        let linked = fixture._dir.join("linked");
        git(
            &fixture.repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt",
                linked.to_str().unwrap(),
            ],
        );
        assert_eq!(repository_of(&linked), Some(fixture.common_dir.clone()));
    }
}
