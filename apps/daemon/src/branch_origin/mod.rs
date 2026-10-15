//! Which session created a git branch, kept so the Git mode can mark the relation.
//!
//! The record names the session that created the branch, or, when that cannot be told, the one
//! working in the branch's worktree — never merely whoever touches the branch later. A wrong name
//! is worse than none: every signal below names a session only when it leaves exactly one
//! candidate, and whatever cannot be told is stored as [`BranchOriginSource::Unknown`] and never
//! sent to a client. That is the record of a question this look could not answer, not an answer:
//! a later signal that does name a session replaces it. Everything lives in the daemon's SQLite
//! (`store::BranchOrigin`) — the Git mode never writes to a user's repository, so no ref, note or
//! configuration entry carries it.
//!
//! Three signals attribute a branch, all looking at the result rather than at what ran:
//!
//! - **A tool call in flight.** The hooks bracket every tool call a session makes, its own and its
//!   subagents', with a start and an end event, and the daemon keeps when each arrived
//!   ([`activity`]). When the repository's branches are looked at again after a hook, a branch
//!   that was not there at the previous look was made by whatever ran when its reflog says it was
//!   born — whatever command, alias or script that was. If exactly one session of the repository
//!   had a tool call in flight then, that is [`BranchOriginSource::HookDiff`]. A session between
//!   calls — its model writing, or a question to the user up — runs nothing, so sessions working
//!   in parallel only collide when their calls overlap the birth. The bounds lean towards a call
//!   still running, since reading a session as idle while its call made the branch would hand the
//!   branch to another session, where reading it as busy only leaves the branch unattributed: a
//!   start counts from [`activity::HOOK_LATENCY_MS`] before it arrived, since the agents need not
//!   wait for a hook before running the call it announces; an end with no start reaches back as
//!   far as the record does; a call nothing ends stays open until the session's next turn end, or,
//!   for the session's own calls, until a declined prompt shows up in its transcript
//!   ([`calls_cancelled`]). The same latency holds the decision back: a branch is decided only
//!   once every start that could concern its birth has had time to land, and until then it is
//!   left undecided and given a look of its own (`watch::UndecidedBranch`). The hook look writes
//!   it no row meanwhile; the Git mode's listing may record a weaker answer for it, which the
//!   decision then replaces as the precedence below allows.
//! - **The reflog's first entry.** A branch the daemon never saw appear — made before it ran, or
//!   before this record existed — carries the time it was made in the first line of
//!   `logs/refs/heads/<name>`. If that entry is one a branch's own creation writes
//!   (`git_files::CREATED_FROM`) and its time falls inside the lifetime of exactly one session of
//!   the repository, that is [`BranchOriginSource::ReflogMatch`]. A first entry of any other shape
//!   — the default branch a `git init` leaves, a clone's, a bare `git update-ref` — says the ref
//!   was written, not that a branch was made there, so it backfills nothing however clear the
//!   session is. A session's lifetime is the coarsest timeline the daemon has, so this only ever
//!   fills in a branch that has no row at all.
//! - **The worktree a session works in.** A branch still `unknown` after both is attributed to the
//!   one running session of the repository whose working directory lies in the worktree that has
//!   the branch checked out — the innermost one, since a linked worktree can sit inside the main
//!   one. That is [`BranchOriginSource::WorktreeCwd`]. The directory is the one the session's own
//!   latest hook reported, never a subagent's, and the project's for a session none has reported
//!   one for yet. It too needs a first reflog entry of a branch's creation, made while that
//!   session was running, so a default branch, or one the session only checked out later, is never
//!   named this way. The main worktree, where many sessions sit, is held to the same one-candidate
//!   rule and so rarely names one.
//!
//! Precedence: a tool call in flight replaces an `unknown` row and a worktree row, a worktree row
//! replaces only an `unknown` one, and the reflog fills only a branch with no row. A creation
//! recorded by either of the first two is never replaced: both name the only session that could
//! have made the branch.
//!
//! A row lives and dies with its branch. It is validated against the branch's first reflog entry on
//! every look, so a branch deleted and made again under the same name does not inherit the old
//! attribution, and removed once a full listing taken after the row was written no longer has the
//! branch. Nothing here runs on a timer of its own: the looks are the hook callback ([`on_hook`]),
//! the trailing look it schedules, and the Git mode's own branch listing ([`annotate`]).
//!
//! These limits follow from the design:
//!
//! - The record names the session whose tool call a branch was born during, which is not always
//!   whoever made it. A branch the user makes in their own terminal, or through an agent's shell
//!   escape that fires no tool events, while exactly one session of the repository is running a
//!   tool goes to that session; so does one a background process makes after the call that
//!   started it has ended. A call the daemon never heard start or end at all is invisible, and a
//!   branch made in it can go to another session's call.
//! - A call waiting at a permission prompt counts as running. Nothing reliable says when it is
//!   granted — the grant fires no event, and the prompt's own event names no call — so the time it
//!   actually runs cannot be told from the time it waits. A subagent's whole run likewise counts
//!   as the one call that started it, besides the calls the subagent makes.
//! - The worktree signal reads the working directory the agent reports, so a session working in a
//!   worktree without moving its own directory there does not count as being in it.
//! - A repository whose refs are in reftable format keeps no `logs/refs/heads` tree, so nothing
//!   read out of a reflog is available there: no branch is backfilled or attributed by its
//!   worktree, a tool call is matched against the whole window since the previous look rather
//!   than against the branch's birth, and a row cannot be held against its branch's birth, so a
//!   branch deleted and made again under the same name keeps the first one's attribution until
//!   the row goes with the name. Reading those reflogs means one `git` per branch on the hook
//!   path, which is the cost the stamp and the floor exist to avoid.
//! - Elsewhere a row is held only as long as the reflog keeps the branch's first entry: once
//!   `gc.reflogExpire` (90 days by default) drops it, the branch no longer matches its row, is
//!   recorded again as `unknown`, and that row then blocks the backfill for good. A branch deleted
//!   and made again at the same commit inside one second has the same first entry and keeps the
//!   old row.
//! - Tool calls are remembered for [`activity::CALL_MEMORY_MS`]: a branch first looked at longer
//!   than that after its birth gets no answer from them.
//!
//! The hook path runs on every tool call of every session, so it is cheap by default: recording
//! what a hook says is a map update in memory, a look at the filesystem stamp of the repository's
//! refs rules out nearly all enumerations, and a repository is enumerated at most once per
//! [`watch::FLOOR`] after its last enumeration finished. A hook held back by the floor loses
//! nothing: its calls are in the record whenever the look runs, and one trailing look runs when the
//! floor is over.

mod activity;
mod git_files;
#[cfg(test)]
mod tests;
mod watch;

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;

use crate::browse::compare::{self, BranchList};
use crate::browse::git::GitEnv;
use crate::browse::{budget, wire_path};
use crate::protocol::{now_millis, BranchInfo, BranchOriginInfo, BranchOriginSource, Session};
use crate::state::AppState;
use crate::store::BranchOrigin;
use activity::HOOK_LATENCY_MS;
pub use git_files::repository_of;
use git_files::{first_reflog_entry, ReflogEntry, Worktree};
pub use watch::Watch;
use watch::{Caller, Claim, ClaimedLook, RefsStamp, UndecidedBranch};

/// The seconds of clock difference tolerated between a reflog entry's time and a time the daemon
/// recorded itself: a hook's arrival, the start of a look, or a session's own start and end.
const CLOCK_SLACK_SECS: i64 = 2;
const CLOCK_SLACK_MS: i64 = CLOCK_SLACK_SECS * 1000;

// -- the hook path -----------------------------------------------------------

/// Takes in what a hook says the session is doing, then looks at the repository of the session
/// that fired it if the stamp of its refs says anything changed. Returns at once; the look and any
/// trailing look it needs run on the blocking pool.
pub fn on_hook(state: &Arc<AppState>, session_id: &str, payload: &Value) {
    let agent = state.live_session(session_id).map(|live| live.agent);
    state
        .branch_watch
        .activity()
        .heard(session_id, agent, payload, now_millis());
    let state = Arc::clone(state);
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let first = {
            let state = Arc::clone(&state);
            tokio::task::spawn_blocking(move || hook_arrived(&state, &session_id)).await
        };
        let mut next = first.ok().flatten();
        while let Some((repo, wait)) = next {
            tokio::time::sleep(wait).await;
            let state = Arc::clone(&state);
            let trailing = tokio::task::spawn_blocking(move || {
                trailing_look(&state, &repo).map(|wait| (repo, wait))
            });
            next = trailing.await.ok().flatten();
        }
    });
}

/// Closes the tool calls the session has open of its own, leaving those of subagents it runs in
/// the background. For what shows the session's turn is over although no hook said so: a declined
/// permission prompt or question, which only the agent's transcript records.
pub fn calls_cancelled(state: &AppState, session_id: &str) {
    state
        .branch_watch
        .activity()
        .close_own(session_id, now_millis());
}

/// Handles one hook; returns the repository and how long to wait when a look has to come later.
fn hook_arrived(state: &AppState, session_id: &str) -> Option<(PathBuf, Duration)> {
    let repo = repository_of_session(state, session_id)?;
    let stamp = RefsStamp::read(&repo);
    let claim = state
        .branch_watch
        .claim(&repo, Caller::Hook, &stamp, Instant::now(), now_millis());
    match claim {
        Claim::Nothing => None,
        Claim::Later(wait) => Some((repo, wait)),
        Claim::Run(look) => look_at(state, stamp, look).map(|wait| (repo, wait)),
    }
}

/// The trailing look; returns how long until the next one, when another is needed.
fn trailing_look(state: &AppState, repo: &Path) -> Option<Duration> {
    let stamp = RefsStamp::read(repo);
    let claim =
        state
            .branch_watch
            .claim(repo, Caller::Trailing, &stamp, Instant::now(), now_millis());
    match claim {
        Claim::Nothing => None,
        Claim::Later(wait) => Some(wait),
        Claim::Run(look) => look_at(state, stamp, look),
    }
}

fn look_at(state: &AppState, stamp: RefsStamp, look: ClaimedLook<'_>) -> Option<Duration> {
    let env = match GitEnv::from_shell() {
        Ok(env) => env,
        Err(err) => {
            tracing::debug!(%err, "branch origins: no git to enumerate with");
            return None;
        }
    };
    look_with(state, &env, stamp, look, &now_millis)
}

/// One enumeration of the repository `look` claimed, and what follows from it; returns how long
/// until the trailing look an undecided branch needs. `stamp` is the one read before it, so a ref
/// that changes while it runs makes the next hook look again. `clock` tells the time in
/// milliseconds since the epoch.
fn look_with(
    state: &AppState,
    env: &GitEnv,
    stamp: RefsStamp,
    look: ClaimedLook<'_>,
    clock: &dyn Fn() -> i64,
) -> Option<Duration> {
    let cancel = AtomicBool::new(false);
    let list = match compare::branches_in(env, &look.repo, &cancel) {
        Ok(list) => list,
        Err(err) => {
            tracing::debug!(repository = %look.repo.display(), %err, "branch origins: listing failed");
            return None;
        }
    };
    let listed_ms = clock();
    let window = Window {
        known: look.known.as_ref(),
        fresh: &look.fresh,
        undecided: &look.undecided,
        since_secs: look.previous_secs,
        listed_ms,
        now_ms: listed_ms,
    };
    let undecided = match reconcile(state, &look.repo, &list, Some(&window)) {
        Ok(reconciled) => reconciled.undecided,
        Err(err) => {
            tracing::debug!(repository = %look.repo.display(), %err, "branch origins: recording failed");
            return None;
        }
    };
    let known = list
        .complete
        .then(|| list.branches.iter().map(|b| b.name.clone()).collect());
    look.finish(stamp, known, undecided)
}

// -- the Git mode's path -----------------------------------------------------

/// Brings the origins of the repository in line with a branch listing the Git mode just made, and
/// attaches the ones that name a session to the listing. A failure leaves the listing as it is: the
/// branches are what was asked for, and the attribution is a decoration on them.
pub fn annotate(state: &AppState, repo: &Path, list: &mut BranchList) {
    let Reconciled { rows, owned, .. } = match reconcile(state, repo, list, None) {
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
    /// The branches of the previous look; `None` when there was none.
    known: Option<&'a HashSet<String>>,
    /// The branches a Git mode listing has seen appear since the previous look, which `known`
    /// already holds but this window has yet to attribute.
    fresh: &'a HashSet<String>,
    /// The branches an earlier look found new and left to be decided later.
    undecided: &'a HashMap<String, UndecidedBranch>,
    /// When the previous look began, in seconds since the epoch. `None` when there was no previous
    /// look of this daemon's, which leaves the window open at the far end: see [`born_since`].
    since_secs: Option<i64>,
    /// When the listing was taken, in milliseconds since the epoch.
    listed_ms: i64,
    /// The moment the look decides at, in milliseconds since the epoch.
    now_ms: i64,
}

/// What [`reconcile`] leaves behind.
struct Reconciled {
    /// What is stored for the repository afterwards.
    rows: Vec<BranchOrigin>,
    /// Whether a stored project was still in the repository when it took the gate. When none was,
    /// the repository is not the daemon's to remember anything about, and the in-memory record of
    /// it is dropped.
    owned: bool,
    /// The branches new in the window that it could not decide yet.
    undecided: HashMap<String, UndecidedBranch>,
}

/// Aligns the repository's stored origins with `list`, a `git for-each-ref` of its branches, and
/// reports what is stored afterwards.
///
/// A row is removed for a branch the listing does not have only when the listing is evidence that
/// the branch is gone (`settled`, below). A row whose branch is there but no longer has the first
/// reflog entry it was made against is removed whatever the listing is — the branch was deleted
/// and made again. With a `window`, a branch new in it is attributed to the session whose tool
/// call it was born during, or left undecided until it can be; any branch left with no row is then
/// backfilled from its reflog, and any still unattributed is offered to the session working in its
/// worktree.
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
    // What the surviving rows already answer, as it stands while this reconcile adds to it: any
    // row keeps the reflog backfill off its branch, while the later signals may still answer a
    // row that is not a creation's (see [`answers`]).
    let mut have: HashMap<String, BranchOriginSource> = rows
        .iter()
        .filter(|row| !dropped.contains(&row.branch))
        .map(|row| (row.branch.clone(), row.source))
        .collect();

    let sessions = RepositorySessions::new(state, repo);
    let mut put: HashMap<String, BranchOrigin> = HashMap::new();
    let mut record = |have: &mut HashMap<String, BranchOriginSource>, origin: BranchOrigin| {
        have.insert(origin.branch.clone(), origin.source);
        put.insert(origin.branch.clone(), origin);
    };
    let mut undecided = HashMap::new();
    if let Some(window) = window {
        for branch in branches {
            // The cap bounds what one window writes as it bounds the backfill's rows: a script
            // making branches in bulk would otherwise have a row inserted for each and the trim
            // drop most of them in the same transaction, by a `created_at` that is the branch's
            // birth rather than the order the rows were written. A branch already holding a row
            // adds none, so this never blocks an `Unknown` row's answer.
            let held = have.get(&branch.name).copied();
            if held.is_none() && have.len() >= budget::MAX_BRANCH_ORIGINS {
                continue;
            }
            if !held.is_none_or(|held| answers(BranchOriginSource::HookDiff, held)) {
                continue;
            }
            let waiting = window.undecided.get(&branch.name);
            if waiting.is_none() && !appeared_in(window, branch, &recreated) {
                continue;
            }
            // An undecided branch is judged against the window it was found new in.
            let Some(since_secs) = waiting.map(|w| w.since_secs).or(window.since_secs) else {
                continue;
            };
            let birth = first_reflog_entry(repo, &branch.name);
            if !born_since(since_secs, birth.as_ref()) {
                continue;
            }
            let seen_ms = waiting.map_or(window.listed_ms, |w| w.seen_ms);
            // A reflog time is cut to the second. Where there is no birth to read, as in a
            // reftable repository, the whole window up to the first listing that had the branch
            // stands in for it.
            let (from, to) = match &birth {
                Some(birth) => (
                    birth.time * 1000 - CLOCK_SLACK_MS,
                    birth.time * 1000 + 999 + CLOCK_SLACK_MS,
                ),
                None => (since_secs * 1000 - CLOCK_SLACK_MS, seen_ms),
            };
            let due_ms = to + HOOK_LATENCY_MS;
            if window.now_ms < due_ms {
                let branch = branch.name.clone();
                undecided.insert(
                    branch,
                    UndecidedBranch {
                        since_secs,
                        seen_ms,
                        due_ms,
                    },
                );
                continue;
            }
            // Read before the record is locked, which the hook path takes on every hook.
            let all = sessions.all()?;
            let creator = state
                .branch_watch
                .activity()
                .alone_in_flight(all, from, to)
                .map(Creator::of);
            if creator.is_none() && held.is_some() {
                // Already stored, and this window cannot do better.
                continue;
            }
            let source = if creator.is_some() {
                BranchOriginSource::HookDiff
            } else {
                BranchOriginSource::Unknown
            };
            let origin = origin_of(&key, &branch.name, creator.as_ref(), birth.as_ref(), source);
            record(&mut have, origin);
        }
    }

    // A branch left undecided gets no row from the weaker signals either: its decision is due.
    for branch in branches {
        if have.len() >= budget::MAX_BRANCH_ORIGINS {
            break;
        }
        if have.contains_key(&branch.name) || undecided.contains_key(&branch.name) {
            continue;
        }
        let birth = first_reflog_entry(repo, &branch.name);
        let creator = match birth.as_ref().filter(|birth| birth.is_creation()) {
            Some(birth) => sessions.alive_alone_at(birth.time)?,
            None => None,
        };
        let source = if creator.is_some() {
            BranchOriginSource::ReflogMatch
        } else {
            BranchOriginSource::Unknown
        };
        let origin = origin_of(&key, &branch.name, creator.as_ref(), birth.as_ref(), source);
        record(&mut have, origin);
    }

    let open: Vec<&BranchInfo> = branches
        .iter()
        .filter(|branch| {
            have.get(&branch.name)
                .is_some_and(|&held| answers(BranchOriginSource::WorktreeCwd, held))
        })
        .collect();
    for (branch, worker, birth) in worktree_candidates(state, repo, &sessions, &open)? {
        let origin = origin_of(
            &key,
            &branch.name,
            Some(&Creator::of(worker)),
            Some(&birth),
            BranchOriginSource::WorktreeCwd,
        );
        record(&mut have, origin);
    }
    let mut put: Vec<BranchOrigin> = put.into_values().collect();

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
        undecided.clear();
        state.branch_watch.forget(repo);
    }

    if dropped.is_empty() && put.is_empty() {
        return Ok(Reconciled {
            rows,
            owned,
            undecided,
        });
    }
    let rows =
        state
            .store
            .apply_branch_origins(&key, &dropped, &put, budget::MAX_BRANCH_ORIGINS)?;
    if owned {
        state.branch_watch.wrote(repo, Instant::now());
    }
    Ok(Reconciled {
        rows,
        owned,
        undecided,
    })
}

/// Whether a signal of the kind `signal` may replace a row of the kind `held`. A creation
/// observed beats one guessed from where a session works, which beats none; a creation once
/// recorded is not replaced, since either creation signal names the only session that could have
/// made the branch. The reflog backfill answers only a branch with no row, so it is not asked.
fn answers(signal: BranchOriginSource, held: BranchOriginSource) -> bool {
    use BranchOriginSource::*;
    match signal {
        HookDiff => matches!(held, Unknown | WorktreeCwd),
        WorktreeCwd => held == Unknown,
        ReflogMatch | Unknown => false,
    }
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

/// Whether a branch born then was born inside a window beginning at `since_secs`, the start of the
/// look before it: an older birth is a branch that was not made in it — a reflog expired down to
/// its first surviving entry, a rename, or a name a listing handed over long before the hook
/// arrived. A window with no look behind it has no beginning and holds nothing, since everything
/// it could claim may as well predate the whole daemon. A branch whose birth cannot be read at all
/// (no reflog, as in a reftable repository) is left to the window, which is the only signal there.
fn born_since(since_secs: i64, birth: Option<&ReflogEntry>) -> bool {
    birth.is_none_or(|birth| since_secs <= birth.time + CLOCK_SLACK_SECS)
}

/// The branches of `open` the worktree signal names a session for, with that session and the
/// branch's birth. Called on every listing the Git mode makes, so what needs no store read — the
/// worktrees and the reflog's first entry — is asked first, and settles the branch that is
/// usually left `unknown`, the default one, without reading the sessions.
fn worktree_candidates<'b, 's>(
    state: &AppState,
    repo: &Path,
    sessions: &'s RepositorySessions<'_>,
    open: &[&'b BranchInfo],
) -> Result<Vec<(&'b BranchInfo, &'s Session, ReflogEntry)>> {
    if open.is_empty() {
        return Ok(Vec::new());
    }
    let worktrees = git_files::worktrees(repo);
    // Only a branch checked out somewhere, whose reflog says it was made: the default branch a
    // clone left, the usual one in the main worktree, never is, nor is any branch with no reflog
    // to read, as in a reftable repository.
    let checked_out: Vec<(&BranchInfo, ReflogEntry)> = open
        .iter()
        .filter(|branch| {
            worktrees
                .iter()
                .any(|worktree| worktree.branch.as_deref() == Some(branch.name.as_str()))
        })
        .filter_map(|branch| {
            let birth = first_reflog_entry(repo, &branch.name)?;
            birth.is_creation().then_some((*branch, birth))
        })
        .collect();
    if checked_out.is_empty() {
        return Ok(Vec::new());
    }
    let workers = worktree_workers(state, &worktrees, sessions)?;
    Ok(checked_out
        .into_iter()
        .filter_map(|(branch, birth)| {
            let [worker] = workers.get(&branch.name)?.as_slice() else {
                return None;
            };
            // A branch made before the session ran, or after it ended, is not one it made.
            lived_through(worker, birth.time).then_some((branch, *worker, birth))
        })
        .collect())
}

/// The running sessions of the repository by the branch checked out in the worktree each one's
/// working directory is in: the innermost worktree holding it.
fn worktree_workers<'s>(
    state: &AppState,
    worktrees: &[Worktree],
    sessions: &'s RepositorySessions<'_>,
) -> Result<HashMap<String, Vec<&'s Session>>> {
    let projects = sessions.projects()?;
    let all = sessions.all()?;
    let running: Vec<(&Session, Option<PathBuf>)> = {
        let activity = state.branch_watch.activity();
        all.iter()
            .filter(|session| session.ended_at.is_none())
            .map(|session| (session, activity.working_directory(&session.id).cloned()))
            .collect()
    };
    let mut workers: HashMap<String, Vec<&Session>> = HashMap::new();
    for (session, reported) in running {
        // Launched in its project's directory, until a hook of its own says otherwise.
        let cwd = reported.or_else(|| {
            let project = session.project_id.as_ref()?;
            projects.get(project).map(PathBuf::from)
        });
        let Some(cwd) = cwd else {
            continue;
        };
        let cwd = fs::canonicalize(&cwd).unwrap_or(cwd);
        let innermost = worktrees
            .iter()
            .filter(|worktree| cwd.starts_with(&worktree.path))
            .max_by_key(|worktree| worktree.path.components().count());
        if let Some(Worktree {
            branch: Some(branch),
            ..
        }) = innermost
        {
            workers.entry(branch.clone()).or_default().push(session);
        }
    }
    Ok(workers)
}

/// Whether the session was running at `secs`, within the clock's slack.
fn lived_through(session: &Session, secs: i64) -> bool {
    session.started_at / 1000 <= secs + CLOCK_SLACK_SECS
        && session
            .ended_at
            .is_none_or(|ended| secs <= ended / 1000 + CLOCK_SLACK_SECS)
}

/// A row for `branch`.
fn origin_of(
    repository: &str,
    branch: &str,
    creator: Option<&Creator>,
    birth: Option<&ReflogEntry>,
    source: BranchOriginSource,
) -> BranchOrigin {
    BranchOrigin {
        repository: repository.to_string(),
        branch: branch.to_string(),
        session_id: creator.map(|c| c.id.clone()),
        session_title: creator.map(|c| c.title.clone()),
        source,
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
    projects: OnceCell<HashMap<String, String>>,
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

    /// The stored projects whose directory is in the repository: their directories by id.
    fn projects(&self) -> Result<&HashMap<String, String>> {
        if let Some(projects) = self.projects.get() {
            return Ok(projects);
        }
        let here = self
            .state
            .store
            .list_projects()?
            .into_iter()
            .filter(|p| repository_of(Path::new(&p.path)).as_deref() == Some(self.repo))
            .map(|p| (p.id, p.path))
            .collect();
        Ok(self.projects.get_or_init(|| here))
    }

    /// Whether any stored project is in the repository.
    fn has_project(&self) -> Result<bool> {
        Ok(!self.projects()?.is_empty())
    }

    /// The sessions of the projects in the repository. A console session runs in a directory of
    /// Octoboard's own.
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
            .filter(|s| {
                s.project_id
                    .as_ref()
                    .is_some_and(|id| here.contains_key(id))
            })
            .collect();
        Ok(self.sessions.get_or_init(|| sessions))
    }

    /// The one session whose lifetime holds `secs`, or `None` when none does or several do. The
    /// slack widens who counts as alive, which can only turn a name into none. A session that
    /// never had a turn cannot have made a branch.
    fn alive_alone_at(&self, secs: i64) -> Result<Option<Creator>> {
        let mut alive = self
            .all()?
            .iter()
            .filter(|s| s.has_conversation && lived_through(s, secs));
        let (Some(only), None) = (alive.next(), alive.next()) else {
            return Ok(None);
        };
        Ok(Some(Creator::of(only)))
    }
}

/// The key a repository's rows are stored under: its canonical common directory, as a wire path.
fn repo_key(repo: &Path) -> String {
    wire_path::encode(repo.as_os_str().as_bytes())
}

fn repository_of_session(state: &AppState, session_id: &str) -> Option<PathBuf> {
    let session = state.store.get_session(session_id).ok()??;
    let project = state
        .store
        .get_project(session.project_id.as_deref()?)
        .ok()??;
    repository_of(Path::new(&project.path))
}
