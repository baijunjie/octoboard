//! What a removal tells of the items it left, for the wire, an agent's tools and text.

use serde_json::{json, Value};

use super::remove::{consents, names};
use crate::protocol::{
    SessionBranchItem, SessionBranchKind, SessionBranchState, SessionBranchTarget,
};
use crate::wire_path;

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
/// has it checked out, from `items` as [`super::survey::inspect_unforced`] found them: `branch`
/// and `worktree` each as `{state, reason}`, `null` for a worktree it has none and for an item it
/// did not find, and `removed`, whether archiving takes all it has. `null` when `items` hold
/// neither.
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
