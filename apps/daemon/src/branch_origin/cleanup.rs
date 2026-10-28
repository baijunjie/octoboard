//! The git branches and linked worktrees bound to sessions that are archived or deleted, and their
//! removal.
//!
//! Sessions store neither. A branch is bound to a session when its row names that session as its
//! responsible session (see the module above) and it is still the branch the row was made for; a
//! linked worktree is bound when it has such a branch checked out.
//!
//! What can go safely goes without anyone being asked: a worktree through `git worktree remove`
//! without `--force`, which git refuses for one with modified or untracked files, initialized
//! submodules or a lock (one whose directory is already gone it takes, record and all), and then a
//! branch through `git branch -d`, git's own check that it is merged into its upstream (into
//! `HEAD` when it has none). Every command runs in the repository's common directory, where `HEAD`
//! is the main worktree's, and which outlives any worktree. The main worktree is never removed, nor
//! a worktree that is a project's folder or holds one, nor one of which that cannot be told
//! ([`Holders`]); a branch still checked out in a worktree that stays is left, as git would refuse
//! it anyway.
//!
//! What git would refuse — a worktree with uncommitted changes or submodules, an unmerged branch, a
//! locked worktree — goes only when the caller names it as forced (`--force`, `-D`, `--force` twice
//! for a lock), and only while it still stands as the caller was shown it: one that has changed
//! since is left and reported rather than forced further than the user agreed to. Only the user's
//! confirmation forces anything: an agent's own archive never does.
//!
//! Removal waits for the sessions' processes to end, since an agent still running may be working
//! in its worktree, and skips a session that is no longer archived by then (one resumed in the
//! meantime). The removals of one repository run one at a time. A failure to remove an item never
//! fails the archive or deletion it follows: the item is reported as left, unless it turns out to
//! be gone all the same.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::{json, Value};

use super::bound::{bound_in, Bound, BoundBranch};
use super::git::{changes, worktrees, Git, Listed};
use super::git_files::uses_reftable;
use super::repo_key;
use crate::browse::source::admin_dir;
use crate::browse::wire_path;
use crate::hostfs::place::{looked_within, Place};
use crate::hostfs::{self, lexically_normalise};
use crate::project_trash::{Seen, TRASH_LOOK_DEADLINE};
use crate::protocol::{
    Session, SessionBranchCause, SessionBranchItem, SessionBranchKind, SessionBranchState,
    SessionBranchTarget, SessionStatus,
};
use crate::state::AppState;

/// How long removal waits for the sessions' processes to end before going ahead anyway. Far above
/// the graceful period an ending process gets; past it, a process that would not die is left to
/// itself, and an unforced removal still refuses a worktree with changes.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// What `AppState` holds for the cleanup: one gate per repository, which keeps an archive's and a
/// deletion's removals in the same repository from running at once and acting on each other's
/// half-done work. A gate is dropped once nobody holds or waits on it.
#[derive(Default)]
pub struct Cleanups {
    gates: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl Cleanups {
    fn gates(&self) -> MutexGuard<'_, HashMap<PathBuf, Arc<Mutex<()>>>> {
        self.gates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs `work` holding the gate of `repo`. It guards an order rather than any state, so a
    /// poisoned one is taken all the same.
    fn serialized<T>(&self, repo: &Path, work: impl FnOnce() -> T) -> T {
        let gate = Arc::clone(self.gates().entry(repo.to_path_buf()).or_default());
        let done = {
            let _held = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            work()
        };
        let mut gates = self.gates();
        // The map's own handle and this one: nobody else holds or waits on it.
        if Arc::strong_count(&gate) == 2 {
            gates.remove(repo);
        }
        done
    }
}

/// What is bound to `sessions`, and where each item stands now. Runs no `git` when none of the
/// sessions is responsible for a branch, which is most sessions. Blocking, waiting for its looks
/// as [`Holders::look`] says.
pub fn inspect(state: &AppState, sessions: &[Session]) -> Result<Vec<SessionBranchItem>> {
    let bound = bound_in(state, sessions)?;
    if bound.is_empty() {
        return Ok(Vec::new());
    }
    let git = (state.host_tools.get().git)()?;
    let mut holders = Holders::of_projects(state, None)?;
    let mut items = Vec::new();
    for repo in &bound {
        match survey(state, &git, repo, &mut holders) {
            Ok(survey) => items.extend(survey.items()),
            Err(detail) => items.extend(repo.all_failed(&detail)),
        }
    }
    Ok(items)
}

/// Removes, once none of `sessions` has a process any more, what can go of what is bound to those
/// of them still archived or deleted by then, together with what `forced` names, and returns what
/// is left, each item with the reason. What cannot be done at all — no `git` to run, the removal
/// not finishing — leaves every bound item as `failed`; only a store that cannot be read leaves
/// nothing to name, and is logged.
pub async fn remove_once_stopped(
    state: Arc<AppState>,
    sessions: Vec<Session>,
    forced: Vec<SessionBranchTarget>,
) -> Vec<SessionBranchItem> {
    let ids: Vec<String> = sessions.iter().map(|session| session.id.clone()).collect();
    wait_until_stopped(&state, &ids).await;
    let reading = Arc::clone(&state);
    let found = tokio::task::spawn_blocking(move || {
        let ended: Vec<Session> = sessions
            .into_iter()
            .filter(|session| has_ended(&reading, &session.id))
            .collect();
        bound_in(&reading, &ended)
    })
    .await;
    let bound = match found {
        Ok(Ok(bound)) => bound,
        Ok(Err(err)) => {
            tracing::warn!(%err, "reading what archived sessions are bound to failed");
            return Vec::new();
        }
        Err(err) => {
            tracing::warn!(%err, "reading what archived sessions are bound to did not finish");
            return Vec::new();
        }
    };
    if bound.is_empty() {
        return Vec::new();
    }
    let unfinished = bound.clone();
    let removed = tokio::task::spawn_blocking(move || {
        let git = match (state.host_tools.get().git)() {
            Ok(git) => git,
            Err(err) => return all_failed(&bound, &format!("{err:#}")),
        };
        let mut holders = match Holders::of_projects(&state, None) {
            Ok(holders) => holders,
            Err(err) => return all_failed(&bound, &format!("{err:#}")),
        };
        let mut left = Vec::new();
        for repo in &bound {
            left.extend(state.branch_cleanups.serialized(&repo.repo, || {
                remove_in(&state, &git, repo, &mut holders, &forced)
            }));
        }
        left
    })
    .await;
    removed.unwrap_or_else(|err| {
        all_failed(&unfinished, &format!("the removal did not finish: {err}"))
    })
}

/// Whether the session `id` is archived, or gone from the store: one resumed or reopened since is
/// neither, and what is bound to it is its own again.
fn has_ended(state: &AppState, id: &str) -> bool {
    match state.store.get_session(id) {
        Ok(Some(session)) => session.status == SessionStatus::Archived,
        Ok(None) => true,
        Err(_) => false,
    }
}

/// Waits until none of the sessions `ids` that are still archived or deleted has a process, or
/// [`STOP_TIMEOUT`] has passed.
async fn wait_until_stopped(state: &Arc<AppState>, ids: &[String]) {
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        let running: Vec<String> = ids
            .iter()
            .filter(|id| state.has_process(id))
            .cloned()
            .collect();
        if running.is_empty() {
            return;
        }
        // Whether they still count is a store read, so it goes to the blocking pool.
        let reading = Arc::clone(state);
        let waited_on =
            tokio::task::spawn_blocking(move || running.iter().any(|id| has_ended(&reading, id)))
                .await
                .unwrap_or(false);
        if !waited_on {
            return;
        }
        if Instant::now() >= deadline {
            tracing::warn!("removing the branches of sessions whose process is still up");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// What an agent's own archive of `sessions` will leave, judged before anything is removed: every
/// bound item that cannot go without forcing. A failure is logged and judges nothing. Blocking.
pub fn will_be_left(state: &AppState, sessions: &[Session]) -> Vec<SessionBranchItem> {
    match inspect(state, sessions) {
        Ok(items) => items
            .into_iter()
            .filter(|item| item.state != SessionBranchState::Removable)
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "inspecting the branches of sessions being archived failed");
            Vec::new()
        }
    }
}

/// [`will_be_left`] off the runtime's workers.
pub async fn left_by(state: &Arc<AppState>, sessions: Vec<Session>) -> Vec<SessionBranchItem> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || will_be_left(&state, &sessions))
        .await
        .unwrap_or_else(|err| {
            tracing::warn!(%err, "inspecting the branches of sessions being archived did not finish");
            Vec::new()
        })
}

/// [`remove_once_stopped`] with nothing forced, logging what is left rather than telling any
/// client: for an archive or a deletion no one waits on the removal of.
pub async fn remove_logging_left(state: Arc<AppState>, sessions: Vec<Session>) {
    let left = remove_once_stopped(state, sessions, Vec::new()).await;
    if !left.is_empty() {
        tracing::info!(left = ?lines(&left), "branches of an archived or deleted session were left");
    }
}

/// [`remove_logging_left`] off the caller's path.
pub fn remove_in_background(state: &Arc<AppState>, sessions: Vec<Session>) {
    if sessions.is_empty() {
        return;
    }
    tokio::spawn(remove_logging_left(Arc::clone(state), sessions));
}

/// Whether `target` names `item`.
pub fn names(target: &SessionBranchTarget, item: &SessionBranchItem) -> bool {
    target.kind == item.kind
        && target.repository == item.repository
        && match item.kind {
            SessionBranchKind::Branch => target.branch == item.branch,
            SessionBranchKind::Worktree => target.path == item.path,
        }
}

/// Whether `target` names `item` as it stands now: a worktree in the same state, without changes
/// the user was not shown; a branch in the same state, or merged by now, which takes no force.
pub fn consents(target: &SessionBranchTarget, item: &SessionBranchItem) -> bool {
    names(target, item)
        && match item.kind {
            SessionBranchKind::Branch => {
                target.state == item.state || item.state == SessionBranchState::Removable
            }
            SessionBranchKind::Worktree => {
                target.state == item.state && (!item.uncommitted || target.uncommitted)
            }
        }
}

/// Of what a removal the user asked for `left`, what they are to be told of: what could not be
/// removed, what was forced and is still there, and what git refuses that the confirmation did not
/// list as kept in the state it stands in now — it became so during the wait, or was never shown.
/// A branch checked out in a worktree that stays, and a project's folder, are left by design.
pub fn to_report(
    left: Vec<SessionBranchItem>,
    forced: &[SessionBranchTarget],
    keep: &[SessionBranchTarget],
) -> Vec<SessionBranchItem> {
    left.into_iter()
        .filter(|item| {
            item.state == SessionBranchState::Failed
                || forced.iter().any(|target| names(target, item))
                || (matches!(
                    item.state,
                    SessionBranchState::UncommittedChanges
                        | SessionBranchState::Submodules
                        | SessionBranchState::Unmerged
                        | SessionBranchState::Locked
                ) && !keep.iter().any(|target| consents(target, item)))
        })
        .collect()
}

/// Each item as a model reads it: what it is, in plain text, and why it stands as it does.
pub fn for_agent(items: &[SessionBranchItem]) -> Value {
    json!(items
        .iter()
        .map(|item| {
            let mut entry = json!({ "kind": item.kind, "reason": reason(item) });
            if let Some(branch) = &item.branch {
                entry["branch"] = json!(shown(branch));
            }
            if let Some(path) = &item.path {
                entry["path"] = json!(shown(path));
            }
            entry
        })
        .collect::<Vec<_>>())
}

/// What an agent's own archive would do with the branch `wire` (a wire path) and the worktree that
/// has it checked out, from `items` as [`inspect`] found them: `branch` and `worktree` each as
/// `{state, reason}`, `null` for a worktree it has none and for an item [`inspect`] did not find,
/// and `removed`, whether archiving takes all it has. `null` when `items` hold neither.
pub fn archive_outlook(items: &[SessionBranchItem], wire: &str) -> Value {
    let named = |kind| {
        items
            .iter()
            .find(|item| item.kind == kind && item.branch.as_deref() == Some(wire))
    };
    let (branch, worktree) = (
        named(SessionBranchKind::Branch),
        named(SessionBranchKind::Worktree),
    );
    if branch.is_none() && worktree.is_none() {
        return Value::Null;
    }
    let outlook = |item: Option<&SessionBranchItem>| {
        item.map(|item| json!({ "state": item.state, "reason": reason(item) }))
    };
    let removed = [branch, worktree]
        .into_iter()
        .flatten()
        .all(|item| item.state == SessionBranchState::Removable);
    json!({
        "removed": removed,
        "branch": outlook(branch),
        "worktree": outlook(worktree),
    })
}

/// One line per item, in English: what it is and why it stands as it does.
pub fn lines(items: &[SessionBranchItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| format!("{}: {}", label(item), reason(item)))
        .collect()
}

/// What names an item, with no word of any language around it: the worktree's path, or the
/// branch's name.
fn name(item: &SessionBranchItem) -> String {
    shown(
        item.path
            .as_deref()
            .or(item.branch.as_deref())
            .unwrap_or_default(),
    )
}

/// What an item is, in English: the worktree's path, or the branch's name.
fn label(item: &SessionBranchItem) -> String {
    match (&item.path, &item.branch) {
        (Some(path), Some(branch)) => {
            format!("worktree {} (branch {})", shown(path), shown(branch))
        }
        (Some(path), None) => format!("worktree {}", shown(path)),
        (None, branch) => format!("branch {}", shown(branch.as_deref().unwrap_or_default())),
    }
}

/// Each item as the `items` param of `session_branches_not_removed` carries it: a JSON list of
/// `{kind, name, branch, state, uncommitted, detail, cause}`, so a client can word each reason and
/// set each name apart without splitting a string a name may hold any character of.
pub fn for_client(items: &[SessionBranchItem]) -> String {
    json!(items
        .iter()
        .map(|item| json!({
            "kind": item.kind,
            "name": name(item),
            "branch": item.branch.as_deref().map(shown),
            "state": item.state,
            "uncommitted": item.uncommitted,
            "detail": item.detail,
            "cause": item.cause,
        }))
        .collect::<Vec<_>>())
    .to_string()
}

fn reason(item: &SessionBranchItem) -> String {
    let changes = if item.uncommitted {
        ", with uncommitted changes"
    } else {
        ""
    };
    match item.state {
        SessionBranchState::Removable => "can be removed".to_string(),
        SessionBranchState::UncommittedChanges => "it has uncommitted changes".to_string(),
        SessionBranchState::Submodules => format!("it has initialized submodules{changes}"),
        SessionBranchState::Unmerged => "it is not merged".to_string(),
        SessionBranchState::CheckedOut => "it is checked out in a worktree that stays".to_string(),
        SessionBranchState::Locked => format!("the worktree is locked{changes}"),
        SessionBranchState::ProjectFolder => "the worktree is a project's folder".to_string(),
        SessionBranchState::Failed => format!(
            "it was not removed: {}",
            item.detail.as_deref().unwrap_or("no reason given")
        ),
    }
}

/// A wire path as text, with any byte that is not UTF-8 replaced.
fn shown(wire: &str) -> String {
    wire_path::decode(wire)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_else(|| wire.to_string())
}

fn failed(mut item: SessionBranchItem, detail: String) -> SessionBranchItem {
    item.state = SessionBranchState::Failed;
    item.detail = Some(detail);
    item.cause = None;
    item
}

/// `item` as one kept because whether it is or holds another project's folder could not be told.
fn untold(item: SessionBranchItem) -> SessionBranchItem {
    SessionBranchItem {
        cause: Some(SessionBranchCause::ProjectFolderUnknown),
        ..failed(item, HOLDING_UNTOLD.to_string())
    }
}

// -- what is bound -------------------------------------------------------------

impl Bound {
    /// Every branch of the repository as failed, for a repository whose worktrees cannot be
    /// listed.
    fn all_failed(&self, detail: &str) -> Vec<SessionBranchItem> {
        self.branches
            .iter()
            .map(|bound| {
                failed(
                    item_of(
                        &self.repo,
                        SessionBranchKind::Branch,
                        Some(bound.branch.clone()),
                        None,
                        &bound.project,
                        Some(&bound.session),
                        SessionBranchState::Failed,
                    ),
                    detail.to_string(),
                )
            })
            .collect()
    }
}

fn all_failed(bound: &[Bound], detail: &str) -> Vec<SessionBranchItem> {
    bound
        .iter()
        .flat_map(|repo| repo.all_failed(detail))
        .collect()
}

fn item_of(
    repo: &Path,
    kind: SessionBranchKind,
    branch: Option<String>,
    path: Option<String>,
    project: &str,
    session: Option<&str>,
    state: SessionBranchState,
) -> SessionBranchItem {
    SessionBranchItem {
        kind,
        repository: repo_key(repo),
        branch,
        path,
        project: project.to_string(),
        session: session.map(str::to_string),
        state,
        uncommitted: false,
        detail: None,
        cause: None,
    }
}

// -- what holds another project's folder ------------------------------------------

/// Why a worktree is kept when whether it is or holds another project's folder could not be told,
/// as a `failed` item's `detail`.
const HOLDING_UNTOLD: &str = "whether it is or holds another project's folder could not be told";

/// Whether a worktree is another project's folder or holds one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holding {
    No,
    Yes,
    /// It could not be told. A miss would remove that project's files with the worktree, so the
    /// worktree is kept, as for `Yes`, though named as one that could not be told.
    Unknown,
}

/// The root of a worktree the cleanup may remove, as looked at.
struct Root {
    /// Where it really is; `None` where nothing is at it.
    place: Option<Place>,
    /// Its path as given and, where it resolves, as resolved: what a path told only by its text
    /// is compared with.
    texts: Vec<PathBuf>,
}

impl Root {
    /// What is at `root`, `None` where that cannot be told: permission to read it is refused,
    /// another I/O error, something other than a directory, a directory that cannot be resolved (no
    /// inode). Blocking.
    fn of(root: &Path) -> Option<Self> {
        let written = lexically_normalise(root);
        match std::fs::symlink_metadata(&written) {
            Err(err) if hostfs::is_not_found(&err) => {
                return Some(Self {
                    place: None,
                    texts: vec![written],
                })
            }
            Err(_) => return None,
            Ok(_) => {}
        }
        let (place, real) = Place::resolved(&written)?;
        Some(Self {
            place: Some(place),
            texts: vec![written, real],
        })
    }
}

/// What one path of [`Holders::look`] was found to be, in each role it was looked at for.
struct Looked {
    seen: Option<Seen>,
    root: Option<Root>,
}

/// Whether a worktree the cleanup may remove is another project's folder or holds one, told by
/// device and inode, never by the paths' text, which misses a folder with a second real path (a
/// bind mount, a macOS firmlink). A miss removes that project's files with the worktree, so what
/// cannot be told counts as held: another project's path that cannot be resolved (an I/O error
/// other than a refused permission, no inode, a look that outran its deadline or is still stuck
/// from an earlier one), and the worktree's root when it cannot be read or resolved. Only another
/// project's path with nothing at it or that cannot be read for want of permission is compared by
/// its text, as written and as resolved, with the root's, as is a root with nothing at it, which
/// holds nothing else.
///
/// The paths are read off the store's lock, each distinct one once per cleanup, a few at a time,
/// each bounded by [`TRASH_LOOK_DEADLINE`] ([`looked_within`]).
pub(super) struct Holders {
    /// The other projects' paths, as stored.
    others: Vec<PathBuf>,
    /// Whether `others` have been looked at.
    others_looked: bool,
    roots_looked: HashSet<PathBuf>,
    seen: HashMap<PathBuf, Seen>,
    roots: HashMap<PathBuf, Root>,
}

impl Holders {
    /// For the paths of every project but `except`, none of them looked at yet.
    pub(super) fn of_projects(state: &AppState, except: Option<&str>) -> Result<Self> {
        Ok(Self {
            others: state
                .store
                .list_projects()?
                .into_iter()
                .filter(|project| Some(project.id.as_str()) != except)
                .map(|project| PathBuf::from(project.path))
                .collect(),
            others_looked: false,
            roots_looked: HashSet::new(),
            seen: HashMap::new(),
            roots: HashMap::new(),
        })
    }

    /// Looks at the other projects' paths and at the worktree roots `roots`, those not looked at
    /// yet. Nothing is looked at while there is no other project, as no worktree holds one then.
    ///
    /// Blocking: this thread waits for the looks, which the runtime runs on blocking threads of
    /// their own so each can be left behind at its deadline. So it runs on a blocking thread of
    /// the runtime (`spawn_blocking`), as every blocking function here does, never on one of its
    /// workers; off any runtime nothing is looked at, and every worktree counts as untold.
    pub(super) fn look(&mut self, state: &AppState, roots: impl IntoIterator<Item = PathBuf>) {
        if self.others.is_empty() {
            return;
        }
        let as_other: HashSet<PathBuf> = if self.others_looked {
            HashSet::new()
        } else {
            self.others.iter().cloned().collect()
        };
        let as_root: HashSet<PathBuf> = roots
            .into_iter()
            .filter(|root| !self.roots_looked.contains(root))
            .collect();
        if as_root.is_empty() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::warn!("no runtime to look at worktrees with; each counts as untold");
            return;
        };
        self.others_looked = true;
        self.roots_looked.extend(as_root.iter().cloned());
        let paths: Vec<PathBuf> = as_other.union(&as_root).cloned().collect();
        let found = runtime.block_on(looked_within(
            &state.directory_looks,
            paths,
            TRASH_LOOK_DEADLINE,
            move |path| {
                Some(Looked {
                    seen: as_other
                        .contains(path)
                        .then(|| Seen::of(path, true))
                        .flatten(),
                    root: as_root.contains(path).then(|| Root::of(path)).flatten(),
                })
            },
        ));
        for (path, looked) in found {
            if let Some(seen) = looked.seen {
                self.seen.insert(path.clone(), seen);
            }
            if let Some(root) = looked.root {
                self.roots.insert(path, root);
            }
        }
    }

    /// Whether the worktree at `root`, looked at through [`Holders::look`], is another project's
    /// folder or holds one.
    fn holding(&self, root: &Path) -> Holding {
        if self.others.is_empty() {
            return Holding::No;
        }
        let Some(root) = self.roots.get(root) else {
            return Holding::Unknown;
        };
        let mut untold = false;
        for other in &self.others {
            match self.seen.get(other) {
                Some(seen)
                    if seen.lies_in(root.place.as_ref(), &root.texts)
                        || root
                            .place
                            .as_ref()
                            .is_some_and(|place| seen.repository_lies_in(place)) =>
                {
                    return Holding::Yes
                }
                Some(_) => {}
                // Nothing at the root holds no files of anyone's.
                None => untold |= root.place.is_some(),
            }
        }
        if untold {
            Holding::Unknown
        } else {
            Holding::No
        }
    }
}

/// The branches of `bound` that are still there, each with its name as bytes. A reftable
/// repository has no reflog files for `is_same_branch` to find gone, so there it is asked.
fn stored_names<'b>(
    git: &Git,
    repo: &Path,
    bound: &'b [BoundBranch],
) -> Vec<(Vec<u8>, &'b BoundBranch)> {
    let reftable = uses_reftable(repo);
    bound
        .iter()
        .filter_map(|branch| {
            let name = wire_path::decode(&branch.branch)?;
            (!reftable || exists(git, repo, &name)).then_some((name, branch))
        })
        .collect()
}

// -- where it stands -----------------------------------------------------------

/// The branches some worktree of the repository has checked out now.
fn held_branches(git: &Git, repo: &Path) -> Result<HashSet<Vec<u8>>, String> {
    Ok(worktrees(git, repo)?
        .into_iter()
        .filter_map(|worktree| worktree.branch)
        .collect())
}

/// Where a bound linked worktree that is not, as far as can be told, another project's folder
/// and holds none stands, and whether it has uncommitted changes.
fn worktree_state(git: &Git, worktree: &Listed) -> (SessionBranchState, bool, Option<String>) {
    // A status that cannot be read counts as changes, so a force never takes changes nobody saw.
    let uncommitted = || !worktree.prunable && changes(git, &worktree.root).unwrap_or(true);
    if worktree.locked {
        return (SessionBranchState::Locked, uncommitted(), None);
    }
    if worktree.prunable {
        return (SessionBranchState::Removable, false, None);
    }
    if has_submodules(git, &worktree.root) {
        return (SessionBranchState::Submodules, uncommitted(), None);
    }
    match changes(git, &worktree.root) {
        Ok(false) => (SessionBranchState::Removable, false, None),
        Ok(true) => (SessionBranchState::UncommittedChanges, true, None),
        Err(detail) => (SessionBranchState::Failed, false, Some(detail)),
    }
}

struct WorktreeSurvey {
    item: SessionBranchItem,
    root: PathBuf,
}

struct BranchSurvey {
    item: SessionBranchItem,
    name: Vec<u8>,
    merged: bool,
}

/// Where every item bound in one repository stands, before anything is removed.
struct Survey {
    worktrees: Vec<WorktreeSurvey>,
    branches: Vec<BranchSurvey>,
}

impl Survey {
    fn items(self) -> Vec<SessionBranchItem> {
        self.worktrees
            .into_iter()
            .map(|worktree| worktree.item)
            .chain(self.branches.into_iter().map(|branch| branch.item))
            .collect()
    }
}

fn survey(
    state: &AppState,
    git: &Git,
    bound: &Bound,
    holders: &mut Holders,
) -> Result<Survey, String> {
    let listed = worktrees(git, &bound.repo)?;
    let names = stored_names(git, &bound.repo, &bound.branches);
    let bound_branch = |worktree: &Listed| {
        names
            .iter()
            .find(|(name, _)| worktree.branch.as_deref() == Some(name.as_slice()))
            .map(|(_, branch)| *branch)
    };
    holders.look(
        state,
        listed
            .iter()
            .filter(|worktree| !worktree.main && bound_branch(worktree).is_some())
            .map(|worktree| worktree.root.clone()),
    );

    let mut worktrees = Vec::new();
    for worktree in &listed {
        if worktree.main {
            continue;
        }
        let Some(branch) = bound_branch(worktree) else {
            continue;
        };
        let item = |state| {
            item_of(
                &bound.repo,
                SessionBranchKind::Worktree,
                Some(branch.branch.clone()),
                Some(wire_path::encode(worktree.root.as_os_str().as_bytes())),
                &branch.project,
                Some(&branch.session),
                state,
            )
        };
        let item = match holders.holding(&worktree.root) {
            Holding::Yes => item(SessionBranchState::ProjectFolder),
            Holding::Unknown => untold(item(SessionBranchState::Failed)),
            Holding::No => {
                let (state, uncommitted, detail) = worktree_state(git, worktree);
                SessionBranchItem {
                    uncommitted,
                    detail,
                    ..item(state)
                }
            }
        };
        worktrees.push(WorktreeSurvey {
            item,
            root: worktree.root.clone(),
        });
    }

    let mut branches = Vec::new();
    for (name, branch) in names {
        let holder_states: Vec<Option<SessionBranchState>> = listed
            .iter()
            .filter(|worktree| worktree.branch.as_deref() == Some(name.as_slice()))
            .map(|holder| {
                worktrees
                    .iter()
                    .find(|worktree| worktree.root == holder.root)
                    .map(|worktree| worktree.item.state)
            })
            .collect();
        // A holder that is not a bound worktree this cleanup could remove keeps the branch
        // whatever is asked; one git refuses keeps it unless it is forced.
        let refused = |state: &Option<SessionBranchState>| {
            matches!(
                state,
                Some(
                    SessionBranchState::UncommittedChanges
                        | SessionBranchState::Submodules
                        | SessionBranchState::Locked
                )
            )
        };
        let stays = holder_states
            .iter()
            .any(|state| *state != Some(SessionBranchState::Removable) && !refused(state));
        let merged = is_merged(git, &bound.repo, &name);
        let state = if stays {
            SessionBranchState::CheckedOut
        } else if !merged {
            SessionBranchState::Unmerged
        } else if holder_states.iter().any(refused) {
            SessionBranchState::CheckedOut
        } else {
            SessionBranchState::Removable
        };
        branches.push(BranchSurvey {
            item: item_of(
                &bound.repo,
                SessionBranchKind::Branch,
                Some(branch.branch.clone()),
                None,
                &branch.project,
                Some(&branch.session),
                state,
            ),
            name,
            merged,
        });
    }
    Ok(Survey {
        worktrees,
        branches,
    })
}

/// Whether the branch `name` exists.
fn exists(git: &Git, repo: &Path, name: &[u8]) -> bool {
    let reference = [b"refs/heads/".as_slice(), name].concat();
    git.read(
        repo,
        &[
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::from_bytes(&reference),
        ],
    )
    .is_ok()
}

/// Whether the worktree at `root` has initialized submodules, which `git worktree remove` refuses
/// without force: the same two signs it looks for, a `modules` directory in the worktree's own git
/// directory, or a gitlink in the index whose path holds a repository.
fn has_submodules(git: &Git, root: &Path) -> bool {
    if admin_dir(root).is_some_and(|admin| admin.join("modules").is_dir()) {
        return true;
    }
    let Ok(stdout) = git.read(
        root,
        &[
            OsStr::new("ls-files"),
            OsStr::new("--stage"),
            OsStr::new("-z"),
        ],
    ) else {
        return false;
    };
    stdout
        .split(|&b| b == 0)
        .filter_map(|entry| {
            let (head, path) = entry.split_at(entry.iter().position(|&b| b == b'\t')?);
            head.starts_with(b"160000 ").then(|| &path[1..])
        })
        .any(|path| root.join(OsStr::from_bytes(path)).join(".git").exists())
}

/// Whether `git branch -d` would take the branch `name` as merged: into its upstream when that
/// exists, into `HEAD` otherwise. Anything that cannot be told counts as not merged.
fn is_merged(git: &Git, repo: &Path, name: &[u8]) -> bool {
    let reference = [b"refs/heads/".as_slice(), name].concat();
    let upstream = git
        .read(
            repo,
            &[
                OsStr::new("for-each-ref"),
                OsStr::new("--format=%(upstream)"),
                OsStr::from_bytes(&reference),
            ],
        )
        .ok()
        .and_then(|stdout| {
            let first = stdout.split(|&b| b == b'\n').next()?.to_vec();
            (!first.is_empty()).then_some(first)
        })
        .filter(|upstream| {
            let commit = [upstream.as_slice(), b"^{commit}"].concat();
            git.read(
                repo,
                &[
                    OsStr::new("rev-parse"),
                    OsStr::new("--verify"),
                    OsStr::new("--quiet"),
                    OsStr::from_bytes(&commit),
                ],
            )
            .is_ok()
        });
    let against = upstream.unwrap_or_else(|| b"HEAD".to_vec());
    git.read(
        repo,
        &[
            OsStr::new("merge-base"),
            OsStr::new("--is-ancestor"),
            OsStr::from_bytes(&reference),
            OsStr::from_bytes(&against),
        ],
    )
    .is_ok()
}

// -- removing --------------------------------------------------------------------

fn remove_in(
    state: &AppState,
    git: &Git,
    bound: &Bound,
    holders: &mut Holders,
    forced: &[SessionBranchTarget],
) -> Vec<SessionBranchItem> {
    let survey = match survey(state, git, bound, holders) {
        Ok(survey) => survey,
        Err(detail) => return bound.all_failed(&detail),
    };
    let consented = |item: &SessionBranchItem| forced.iter().any(|target| consents(target, item));
    let mut left = Vec::new();
    for WorktreeSurvey { item, root } in survey.worktrees {
        // A locked worktree takes the force twice, as git asks for it.
        let force: &[&OsStr] = match item.state {
            SessionBranchState::Removable => &[],
            SessionBranchState::UncommittedChanges | SessionBranchState::Submodules
                if consented(&item) =>
            {
                &[OsStr::new("--force")]
            }
            SessionBranchState::Locked if consented(&item) => {
                &[OsStr::new("--force"), OsStr::new("--force")]
            }
            _ => {
                left.push(item);
                continue;
            }
        };
        let mut args = vec![OsStr::new("worktree"), OsStr::new("remove")];
        args.extend(force);
        args.extend([OsStr::new("--"), root.as_os_str()]);
        if let Err(detail) = git.write(&bound.repo, &args) {
            let still_there = worktrees(git, &bound.repo).map_or(true, |listed| {
                listed.iter().any(|worktree| worktree.root == root)
            });
            if still_there {
                left.push(failed(item, detail));
            }
        }
    }

    // Which branches are still checked out somewhere, now that the worktrees that could go have.
    let held = match held_branches(git, &bound.repo) {
        Ok(held) => held,
        Err(detail) => {
            left.extend(
                survey
                    .branches
                    .into_iter()
                    .map(|branch| failed(branch.item, detail.clone())),
            );
            return left;
        }
    };
    for BranchSurvey {
        mut item,
        name,
        merged,
    } in survey.branches
    {
        let flag = if held.contains(&name) {
            item.state = if merged {
                SessionBranchState::CheckedOut
            } else {
                SessionBranchState::Unmerged
            };
            left.push(item);
            continue;
        } else if !merged && consented(&item) {
            "-D"
        } else if merged {
            "-d"
        } else {
            item.state = SessionBranchState::Unmerged;
            left.push(item);
            continue;
        };
        let args = [
            OsStr::new("branch"),
            OsStr::new(flag),
            OsStr::new("--"),
            OsStr::from_bytes(&name),
        ];
        if let Err(detail) = git.write(&bound.repo, &args) {
            if exists(git, &bound.repo, &name) {
                left.push(failed(item, detail));
            }
        }
    }
    left
}

pub mod project;
#[cfg(test)]
mod tests;
