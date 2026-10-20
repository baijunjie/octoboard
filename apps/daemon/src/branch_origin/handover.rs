//! Moving the responsibility for git branches between sessions, for the callers that have checked
//! the branches are the giver's: `request_session` on approval, `new_session` with `branches`, and
//! the owner's `handover_branches`. The record move itself is [`super::hand_over`].

use std::path::PathBuf;
use std::sync::Arc;

use crate::browse::wire_path;
use crate::protocol::Session;
use crate::state::AppState;
use crate::store::BranchMove;

/// The branches of one repository that are handed to a session, checked to be the giver's to give
/// when the call was made.
#[derive(Debug, Clone)]
pub struct Handover {
    /// The canonical common directory of the repository, as `super::repository_of` gives it.
    pub repository: PathBuf,
    /// Names below `refs/heads/`, as wire paths. Never empty.
    pub branches: Vec<String>,
}

/// What became of the branches a handover named.
pub enum Handed {
    /// The compare-and-set ran: some branches moved, the rest were no longer the giver's, or were
    /// gone.
    Moved(BranchMove),
    /// The move could not be recorded, so every branch is still the giver's.
    Failed(Vec<String>),
}

/// Moves `handover`'s branches from `from` to `to`, each only where `from` is still its
/// responsible session. A failure to record the move is told in the answer, not raised: whoever
/// calls this has a session running by then.
pub async fn move_branches(
    state: &Arc<AppState>,
    handover: &Handover,
    from: &Session,
    to: &Session,
) -> Handed {
    let moving = {
        let state = state.clone();
        let repository = handover.repository.clone();
        let branches = handover.branches.clone();
        let (from, to_id) = (from.id.clone(), to.id.clone());
        let to_title = to.title.clone();
        tokio::task::spawn_blocking(move || {
            super::hand_over(&state, &repository, &branches, &from, &to_id, &to_title)
        })
    };
    match moving.await {
        Ok(Ok(moved)) => Handed::Moved(moved),
        Ok(Err(err)) => {
            tracing::warn!(session = %from.id, %err, "handing the branches over failed");
            Handed::Failed(handover.branches.clone())
        }
        Err(err) => {
            tracing::warn!(session = %from.id, %err, "handing the branches over did not finish");
            Handed::Failed(handover.branches.clone())
        }
    }
}

/// A branch name given as a wire path, as text.
pub fn branch_name(branch: &str) -> String {
    String::from_utf8_lossy(&wire_path::decode(branch).unwrap_or_default()).into_owned()
}

/// Branch names, as wire paths, set in backticks and separated by commas, for prose.
pub fn named_branches(branches: &[String]) -> String {
    branches
        .iter()
        .map(|branch| format!("`{}`", branch_name(branch)))
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
