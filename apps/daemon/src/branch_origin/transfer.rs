//! Moving the responsibility for git branches between sessions, for the callers that have checked
//! the branches are the giver's: `request_session` on approval, `open_session` with `branches`, and
//! the owner's `transfer_branches`. The record move itself is [`move_branch_records`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::{is_same_branch, recorded_title, repo_key};
use crate::protocol::Session;
use crate::state::AppState;
use crate::store::BranchMove;
use crate::wire_path;

/// The branches of one repository that are transferred to a session, checked to be the giver's to
/// give when the call was made.
#[derive(Debug, Clone)]
pub struct BranchTransfer {
    /// The canonical common directory of the repository, as `super::repository_of` gives it.
    pub repository: PathBuf,
    /// Names below `refs/heads/`, as wire paths. Never empty.
    pub branches: Vec<String>,
    /// The target branch recorded for each of `branches` that has one, as text, by the branch's
    /// wire path: the branch it was created from, which the session taking it is told to merge it
    /// into.
    pub targets: HashMap<String, String>,
}

/// What became of the branches a transfer named.
pub enum TransferOutcome {
    /// The compare-and-set ran: some branches moved, the rest were no longer the giver's, or were
    /// gone.
    Moved(BranchMove),
    /// The move could not be recorded, so every branch is still the giver's.
    Failed(Vec<String>),
}

/// Moves `transfer`'s branches from `from` to `to`, each only where `from` is still its
/// responsible session. A failure to record the move is told in the answer, not raised: whoever
/// calls this has a session running by then.
pub async fn move_branches(
    state: &Arc<AppState>,
    transfer: &BranchTransfer,
    from: &Session,
    to: &Session,
) -> TransferOutcome {
    let moving = {
        let state = state.clone();
        let repository = transfer.repository.clone();
        let branches = transfer.branches.clone();
        let (from, to_id) = (from.id.clone(), to.id.clone());
        let to_title = to.title.clone();
        tokio::task::spawn_blocking(move || {
            move_branch_records(&state, &repository, &branches, &from, &to_id, &to_title)
        })
    };
    match moving.await {
        Ok(Ok(moved)) => TransferOutcome::Moved(moved),
        Ok(Err(err)) => {
            tracing::warn!(session = %from.id, %err, "transferring the branches failed");
            TransferOutcome::Failed(transfer.branches.clone())
        }
        Err(err) => {
            tracing::warn!(session = %from.id, %err, "transferring the branches did not finish");
            TransferOutcome::Failed(transfer.branches.clone())
        }
    }
}

/// The first of `branches` of `repo` that `session` is not responsible for, or `None` when it is
/// responsible for all of them. A branch is not the session's when it has no row, when its row
/// names another responsible session, or when it is no longer the branch the row was made for. In
/// a reftable repository the last cannot be told (see the header of [`super`]), so a branch
/// deleted since the last listing still passes there. The answer is a snapshot:
/// [`move_branch_records`] compares again when it moves.
pub fn first_not_held_by(
    state: &AppState,
    repo: &Path,
    branches: &[String],
    session: &str,
) -> Result<Option<String>> {
    let rows = state.store.branch_origins(&repo_key(repo))?;
    Ok(branches
        .iter()
        .find(|branch| {
            !rows.iter().any(|row| {
                row.branch == **branch
                    && row.responsible_session == session
                    && is_same_branch(repo, row)
            })
        })
        .cloned())
}

/// Moves the responsible session of `branches` of `repo` from `from` to `to`, called `to_title`
/// (cut as attribution cuts it), only where it is `from` now, and says which moved and which did
/// not. Under the repository's gate, as every other write to its rows is: a reconcile that has
/// read its snapshot would otherwise write the old responsible session back over the move.
pub fn move_branch_records(
    state: &AppState,
    repo: &Path,
    branches: &[String],
    from: &str,
    to: &str,
    to_title: &str,
) -> Result<BranchMove> {
    state.branch_origins.serialized(repo, || {
        Ok(state.store.move_branch_responsibility(
            &repo_key(repo),
            branches,
            from,
            to,
            &recorded_title(to_title),
            crate::time::now_millis(),
        )?)
    })
}

/// Moves every branch `from` is responsible for to `to`, a session of the same project, and
/// returns the branches moved, as wire paths. Blocking: it reads the repository's origins and
/// takes its gate.
pub fn move_all_branch_records(
    state: &AppState,
    from: &Session,
    to: &Session,
) -> Result<Vec<String>> {
    let mut moved = Vec::new();
    for repo in super::bound::bound_in(state, std::slice::from_ref(from))? {
        // `bound_in` lists only the branches `from` is responsible for.
        let branches: Vec<String> = repo
            .branches
            .into_iter()
            .map(|branch| branch.branch)
            .collect();
        moved.extend(
            move_branch_records(state, &repo.repo, &branches, &from.id, &to.id, &to.title)?.moved,
        );
    }
    Ok(moved)
}

/// A branch name given as a wire path, as text.
pub fn branch_name(branch: &str) -> String {
    String::from_utf8_lossy(&wire_path::decode(branch).unwrap_or_default()).into_owned()
}

/// Branch names, as wire paths, set in backticks and separated by commas, for prose.
pub fn named_branches(branches: &[String]) -> String {
    named_branches_with_targets(branches, &HashMap::new())
}

/// [`named_branches`], each branch followed by its target branch from `targets` (by wire path)
/// where it has one.
pub fn named_branches_with_targets(
    branches: &[String],
    targets: &HashMap<String, String>,
) -> String {
    branches
        .iter()
        .map(|branch| match targets.get(branch) {
            Some(target) => format!("`{}` (target branch `{target}`)", branch_name(branch)),
            None => format!("`{}`", branch_name(branch)),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The noun and the pronoun that stand for `branches` in a sentence: "branch" and "it" for one,
/// "branches" and "them" for several.
pub fn branch_noun_and_pronoun(branches: &[String]) -> (&'static str, &'static str) {
    match branches {
        [_] => ("branch", "it"),
        _ => ("branches", "them"),
    }
}
