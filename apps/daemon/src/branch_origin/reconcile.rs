//! Bringing the stored origins in line with a branch listing the Git mode made.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use super::{creation_of, is_same_branch, repo_key};
use crate::browse::budget;
use crate::browse::compare::BranchList;
use crate::git::reflog;
use crate::git::repo_files::uses_reftable;
use crate::protocol::BranchOriginInfo;
use crate::state::AppState;
use crate::store::BranchOrigin;
use crate::wire_path;

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
        responsible_session: row.responsible_session,
        responsible_title: row.responsible_title,
        responsible_since: row.responsible_since,
    }
}

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
pub(super) fn reconcile(
    state: &AppState,
    repo: &Path,
    list: &BranchList,
) -> Result<Vec<BranchOrigin>> {
    let key = repo_key(repo);
    state.branch_origins.serialized(repo, || {
        let rows = state.store.branch_origins(&key)?;
        let live: HashSet<&str> = list.branches.iter().map(|b| b.name.as_str()).collect();

        // Whether this listing can be read as the repository's branches now. A cut one proves
        // nothing past the cut, and one taken before the last write proves nothing at all: a
        // `git for-each-ref` that ran before a branch was made is complete and has no trace of it,
        // so the row a report has since written for it would be deleted as the branch's own.
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
            let mut unheld: Vec<&str> =
                live.iter().copied().filter(|b| !held.contains(b)).collect();
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

        // A listing reads the project before it takes the gate, with a `git` or two in between, so
        // a deletion can start and finish in that gap. With no project left in the repository
        // nothing will clear what is written now (`project_deleted` is past). Removals still apply:
        // they only tidy. The same deletion leaves nothing to clear the repository's in-memory
        // record either, so it goes here.
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
    })
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
        let born_alike = creation_of(repo, candidate)
            .is_some_and(|entry| entry.time == row.birth_time && entry.commit == row.birth_commit);
        let mut message = b"Branch: renamed refs/heads/".to_vec();
        message.extend_from_slice(&old);
        message.extend_from_slice(b" to refs/heads/");
        message.extend_from_slice(&new);
        born_alike && reflog::has_message(repo, &new, |line| line == message.as_slice())
    });
    match (found.next(), found.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// Whether any stored project is in the repository. A failed read is not an empty repository:
/// it fails the reconcile, and the next listing tries again.
fn has_project(state: &AppState, repo: &Path) -> Result<bool> {
    crate::project_git::has_projects_in_repository(state, repo)
}
