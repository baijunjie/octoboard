//! Which session created a git branch, kept so the Git mode can mark the relation.
//!
//! A branch is attributed only when the session's own git says it created it: a project session's
//! agent runs with `core.hooksPath` pointed at Octoboard's hook dispatchers ([`git_hooks`]), and
//! when a reference transaction of a git anywhere in that agent's process tree commits a branch
//! that did not exist while it was prepared — whatever command, alias or script made it — the
//! `reference-transaction` hook reports it, under the session it was started for, to
//! [`ref_created`]. A forced update of an existing branch names no old value either, which is why
//! existence is asked before the transaction writes anything. Nothing else names a session: no
//! timing, no working directory, no guess. A branch the hook never reported carries no creator,
//! which is no failure.
//!
//! A report is taken only for a repository the session's own project is in, and only while the
//! branch is still the one it reports: in a repository whose references are files, the branch's
//! reflog has to begin with its creation at the reported commit, at the time of the report. The
//! record lives in the daemon's SQLite (`store::BranchOrigin`) — the Git mode never writes to a
//! user's repository, so no ref, note or configuration entry carries it.
//!
//! A row lives and dies with its branch. It is validated against the branch's first reflog entry on
//! every listing, so a branch deleted and made again under the same name does not inherit the old
//! attribution, and removed once a full listing taken after the row was written no longer has the
//! branch — unless the branch was renamed, which git records in the new name's reflog, and then the
//! row moves to the new name. A copy (`git branch -c`) is a branch the session did not create, and
//! gets nothing.
//!
//! These limits follow from the design:
//!
//! - In a repository whose references are in reftable format there are no reflog files to read, so
//!   a row is taken at the reported commit and the report's time, cannot be held against the
//!   branch's birth afterwards, and a rename is not followed: a branch deleted and made again under
//!   the same name between two listings keeps the first one's creator until its name goes.
//! - Elsewhere a row is held only as long as the reflog keeps the branch's first entry: once
//!   `gc.reflogExpire` (90 days by default) drops it, the row goes. A branch deleted and made again
//!   at the same commit inside one second has the same first entry and keeps the old row.

mod gate;
mod git_files;
pub mod git_hooks;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::browse::compare::BranchList;
use crate::browse::{budget, wire_path};
use crate::protocol::{BranchOriginInfo, BranchOriginSource, Session};
use crate::state::AppState;
use crate::store::BranchOrigin;
pub use gate::Gates;
pub use git_files::repository_of;
use git_files::{first_reflog_entry, reflog_has_message, uses_reftable};

/// How far apart the time a report gives and the time of the reflog entry it is about may lie. The
/// report is made by a process git starts right after the entry is written, so anything further
/// apart is about another branch of the same name; the margin is for a machine whose process
/// starts stall for seconds.
const REPORT_SLACK_SECS: i64 = 60;

/// What a session's `reference-transaction` hook reports of a committed transaction: the branches
/// it created, and where git ran the hook. Paths and reference names are wire paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefReport {
    /// The directory git ran the hook in: the top of the worktree, or the repository itself when it
    /// is bare.
    pub cwd: String,
    /// The `GIT_DIR` git set for the hook, which it does whenever the repository is not the `.git`
    /// found in `cwd`; relative to `cwd` when relative.
    pub git_dir: Option<String>,
    pub refs: Vec<CreatedRef>,
    /// When the report was made, in seconds since the epoch.
    pub unix_time: i64,
}

/// One reference a transaction created: its full name and the object id it was created at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedRef {
    pub name: String,
    pub oid: String,
}

// -- the report --------------------------------------------------------------

/// Records the branches a session's git reports it created. A report that cannot be taken — from
/// a session that is gone or is no project's, about a repository the session's project is not in,
/// or about a branch that is no longer the one reported — records nothing and says nothing: the
/// hook that sent it has long returned.
pub fn ref_created(state: &AppState, session_id: &str, report: &RefReport) {
    if let Err(err) = record_created(state, session_id, report) {
        tracing::debug!(session = %session_id, %err, "branch origins: recording a report failed");
    }
}

fn record_created(state: &AppState, session_id: &str, report: &RefReport) -> Result<()> {
    let Some(session) = state.store.get_session(session_id)? else {
        return Ok(());
    };
    let Some(project_id) = session.project_id.clone() else {
        return Ok(());
    };
    let Some(project) = state.store.get_project(&project_id)? else {
        return Ok(());
    };
    let Some(repo) = repository_of(Path::new(&project.path)) else {
        return Ok(());
    };
    if reported_repository(report).as_deref() != Some(repo.as_path()) {
        tracing::debug!(session = %session_id, "branch origins: a report about another repository");
        return Ok(());
    }
    let key = repo_key(&repo);
    let creator = Creator::of(&session);

    let gate = state.branch_origins.gate(&repo);
    let _serialized = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // The project was read before the gate: a deletion that has cleared the repository since
    // would leave what is written now with nothing to clear it.
    if state.store.get_project(&project_id)?.is_none() {
        return Ok(());
    }
    let rows = state.store.branch_origins(&key)?;
    let mut put: Vec<BranchOrigin> = Vec::new();
    for created in &report.refs {
        let Some(branch) = branch_of(&created.name) else {
            continue;
        };
        if !is_object_id(&created.oid) {
            continue;
        }
        let Some(birth_time) = birth_of(&repo, &branch, &created.oid, report.unix_time) else {
            continue;
        };
        let recorded = rows.iter().any(|row| {
            row.branch == branch && row.birth_commit == created.oid && row.birth_time == birth_time
        });
        if recorded {
            continue;
        }
        put.push(BranchOrigin {
            repository: key.clone(),
            branch,
            session_id: creator.id.clone(),
            session_title: creator.title.clone(),
            source: BranchOriginSource::RefHook,
            created_at: birth_time * 1000,
            birth_commit: created.oid.clone(),
            birth_time,
        });
    }
    if put.is_empty() {
        return Ok(());
    }
    state
        .store
        .apply_branch_origins(&key, &[], &put, budget::MAX_BRANCH_ORIGINS)?;
    state.branch_origins.wrote(&repo, Instant::now());
    Ok(())
}

/// The canonical common directory of the repository a report is about: the one `GIT_DIR` names
/// when git set it, else the one holding the directory the hook ran in.
fn reported_repository(report: &RefReport) -> Option<PathBuf> {
    let cwd = PathBuf::from(std::ffi::OsStr::from_bytes(&wire_path::decode(
        &report.cwd,
    )?));
    let Some(git_dir) = &report.git_dir else {
        return repository_of(&cwd);
    };
    let git_dir = cwd.join(std::ffi::OsStr::from_bytes(&wire_path::decode(git_dir)?));
    git_files::common_dir_of(&git_dir)
}

/// The branch a created reference is, as a wire path; `None` for a reference outside
/// `refs/heads/`.
fn branch_of(reference: &str) -> Option<String> {
    let name = wire_path::decode(reference)?;
    let branch = name.strip_prefix(b"refs/heads/")?;
    (!branch.is_empty()).then(|| wire_path::encode(branch))
}

/// A full object id: SHA-1's 40 hex digits or SHA-256's 64.
fn is_object_id(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// When the reported branch was born, in seconds since the epoch, provided it is still the branch
/// the report is about: its reflog begins with a creation at the reported commit, at a time close
/// to the report's, and says nothing of the branch being a rename or a copy of another. A
/// repository whose references are files but keeps no reflog for the branch cannot tell it from a
/// later branch of the same name, and gives no birth. A reftable repository has no reflog files at
/// all, and the report's own time stands in.
fn birth_of(repo: &Path, branch: &str, oid: &str, reported_secs: i64) -> Option<i64> {
    if uses_reftable(repo) {
        return Some(reported_secs);
    }
    let entry = first_reflog_entry(repo, branch)?;
    if !entry.from_nothing
        || entry.commit != oid
        || (entry.time - reported_secs).abs() > REPORT_SLACK_SECS
    {
        return None;
    }
    let name = wire_path::decode(branch)?;
    let moved_here = [b"Branch: renamed ".as_slice(), b"Branch: copied "]
        .iter()
        .any(|kind| reflog_has_message(repo, branch, |message| is_move_into(message, kind, &name)));
    (!moved_here).then_some(entry.time)
}

/// Whether a reflog message is git's record of a rename or copy (`kind`) that ends at the branch
/// `name`: `Branch: renamed refs/heads/<old> to refs/heads/<name>`.
fn is_move_into(message: &[u8], kind: &[u8], name: &[u8]) -> bool {
    let mut target = b" to refs/heads/".to_vec();
    target.extend_from_slice(name);
    message.starts_with(kind) && message.ends_with(&target)
}

// -- the Git mode's path -----------------------------------------------------

/// Brings the origins of the repository in line with a branch listing the Git mode just made, and
/// attaches them to the listing. A failure leaves the listing as it is: the branches are what was
/// asked for, and the attribution is a decoration on them.
pub fn annotate(state: &AppState, repo: &Path, list: &mut BranchList) {
    let rows = match reconcile(state, repo, list) {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(repository = %repo.display(), %err, "branch origins: reconciling failed");
            return;
        }
    };
    let mut by_branch: HashMap<String, BranchOriginInfo> = rows
        .into_iter()
        .map(|row| (row.branch.clone(), shown(row)))
        .collect();
    for branch in &mut list.branches {
        branch.origin = by_branch.remove(&branch.name);
    }
}

fn shown(row: BranchOrigin) -> BranchOriginInfo {
    BranchOriginInfo {
        source: row.source,
        session: row.session_id,
        title: row.session_title,
        created_at: row.created_at,
    }
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
    // Under the repository's gate, as every other write to its rows is: a reconcile or a report
    // that has read its snapshot and is still deciding would otherwise write that snapshot back
    // over the delete, leaving a repository with rows, no project to clean them up and nothing
    // that runs here again.
    let gate = state.branch_origins.gate(&repo);
    let _serialized = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Err(err) = state
        .store
        .delete_repository_branch_origins(&repo_key(&repo))
    {
        tracing::warn!(repository = %repo.display(), %err, "branch origins: clearing failed");
    }
    state.branch_origins.forget(&repo);
}

// -- reconciling -------------------------------------------------------------

/// Aligns the repository's stored origins with `list`, a `git for-each-ref` of its branches, and
/// returns what is stored afterwards.
///
/// A row is removed for a branch the listing does not have only when the listing is evidence that
/// the branch is gone (`settled`, below), and then moved instead to the branch it was renamed to,
/// where git's reflog says so. A row whose branch is there but no longer has the first reflog entry
/// it was made against is removed whatever the listing is — the branch was deleted and made again.
///
/// The whole of it runs under the repository's gate and leaves one transaction behind, because
/// what it writes follows from the snapshot of rows it opened with.
fn reconcile(state: &AppState, repo: &Path, list: &BranchList) -> Result<Vec<BranchOrigin>> {
    let key = repo_key(repo);
    let gate = state.branch_origins.gate(repo);
    let _serialized = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let rows = state.store.branch_origins(&key)?;
    let live: HashSet<&str> = list.branches.iter().map(|b| b.name.as_str()).collect();

    // Whether this listing can be read as the repository's branches now. A cut one proves nothing
    // past the cut, and one taken before the last write proves nothing at all: a
    // `git for-each-ref` that ran before a branch was made is complete and has no trace of it, so
    // the row a report has since written for it would be deleted as the branch's own.
    let settled = list.complete
        && state
            .branch_origins
            .wrote_at(repo)
            .is_none_or(|wrote| wrote <= list.listed_at);

    let mut dropped = Vec::new();
    let mut gone = Vec::new();
    for row in &rows {
        if !live.contains(row.branch.as_str()) {
            if settled {
                dropped.push(row.branch.clone());
                gone.push(row);
            }
        } else if !is_same_branch(repo, row) {
            dropped.push(row.branch.clone());
        }
    }
    let mut put = Vec::new();
    if !gone.is_empty() {
        let held: HashSet<&str> = rows.iter().map(|row| row.branch.as_str()).collect();
        let mut unheld: Vec<&str> = live.iter().copied().filter(|b| !held.contains(b)).collect();
        for row in gone {
            if let Some(at) = renamed_to(repo, row, &unheld) {
                unheld.retain(|branch| *branch != at);
                put.push(BranchOrigin {
                    branch: at.to_string(),
                    ..row.clone()
                });
            }
        }
    }

    // A listing reads the project before it takes the gate, with a `git` or two in between, so a
    // deletion can start and finish in that gap. With no project left in the repository nothing
    // will clear what is written now (`project_deleted` is past). Removals still apply: they only
    // tidy. The same deletion leaves nothing to clear the repository's in-memory record either,
    // so it goes here.
    if !has_project(state, repo)? {
        put.clear();
        state.branch_origins.forget(repo);
    }

    if dropped.is_empty() && put.is_empty() {
        return Ok(rows);
    }
    let rows =
        state
            .store
            .apply_branch_origins(&key, &dropped, &put, budget::MAX_BRANCH_ORIGINS)?;
    if !put.is_empty() {
        state.branch_origins.wrote(repo, Instant::now());
    }
    Ok(rows)
}

/// The branch among `candidates` that `row`'s branch was renamed to. A rename carries the whole
/// reflog over, so the new name begins with the row's birth and ends with git's
/// `Branch: renamed refs/heads/<old> to refs/heads/<new>`; a copy carries it too, but says
/// `copied`, and the original is still there. Needs the reflog files, so never in a reftable
/// repository.
fn renamed_to<'b>(repo: &Path, row: &BranchOrigin, candidates: &[&'b str]) -> Option<&'b str> {
    if uses_reftable(repo) {
        return None;
    }
    let old = wire_path::decode(&row.branch)?;
    let mut found = candidates.iter().copied().filter(|candidate| {
        let Some(new) = wire_path::decode(candidate) else {
            return false;
        };
        let born_alike = first_reflog_entry(repo, candidate)
            .is_some_and(|entry| entry.time == row.birth_time && entry.commit == row.birth_commit);
        let mut message = b"Branch: renamed refs/heads/".to_vec();
        message.extend_from_slice(&old);
        message.extend_from_slice(b" to refs/heads/");
        message.extend_from_slice(&new);
        born_alike && reflog_has_message(repo, candidate, |line| line == message.as_slice())
    });
    match (found.next(), found.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// Whether the branch is still the one `row` was made for. A reftable repository keeps no reflog
/// files to hold it against, and the row stands until the branch goes.
fn is_same_branch(repo: &Path, row: &BranchOrigin) -> bool {
    if uses_reftable(repo) {
        return true;
    }
    first_reflog_entry(repo, &row.branch)
        .is_some_and(|entry| entry.time == row.birth_time && entry.commit == row.birth_commit)
}

/// Whether any stored project is in the repository. A failed read is not an empty repository:
/// it fails the reconcile, and the next listing tries again.
fn has_project(state: &AppState, repo: &Path) -> Result<bool> {
    Ok(state
        .store
        .list_projects()?
        .iter()
        .any(|p| repository_of(Path::new(&p.path)).as_deref() == Some(repo)))
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

/// The key a repository's rows are stored under: its canonical common directory, as a wire path.
fn repo_key(repo: &Path) -> String {
    wire_path::encode(repo.as_os_str().as_bytes())
}
