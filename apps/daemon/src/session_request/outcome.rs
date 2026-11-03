//! What a session is told of an approved request: the wording of the outcome, in the call's result
//! and in the message written into the caller's session alike. What was done is `super`'s; this
//! only describes it.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::branch_origin::transfer::{branch_noun_and_pronoun, named_branches, TransferOutcome};
use crate::coordinator;
use crate::messaging::naming::{label, one_line, place_of, reports_to, ReportsTo};
use crate::protocol::Session;
use crate::session_tools::catalogue::{qualified_tool_name, tool_by_name};
use crate::state::AppState;

use super::Action;

/// What every outcome says of a session the caller asked for, which it can read and inform but not
/// drive.
const NOT_YOURS_TO_DRIVE: &str =
    "It is not yours to drive — you cannot instruct, archive or reopen it.";

/// How the user's approval is worded in an outcome: whose it was, and, for an automatic one, which
/// of the two settings it was under, which `action` tells.
pub(super) fn approval_wording(automatic: bool, action: &Action) -> &'static str {
    match (automatic, action) {
        (false, _) => "The user approved the request",
        (true, Action::Start { .. }) => {
            "Octoboard approved the request automatically, as the user has chosen to have session \
             requests approved without asking"
        }
        (true, Action::Resume { .. } | Action::ResumeRecord { .. }) => {
            "Octoboard approved the request automatically, as the user has chosen to have resume \
             requests approved without asking"
        }
    }
}

/// What a resuming request did, as the call's result names it in `action`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ResumeAction {
    /// Opened a session on an agent's own record of the conversation.
    Opened,
    /// Brought the archived session back.
    Reopened,
    /// Brought the interrupted session back.
    Resumed,
    /// Found the session running by then and did nothing.
    AlreadyRunning,
}

impl ResumeAction {
    /// The value the call's result carries.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Reopened => "reopened",
            Self::Resumed => "resumed",
            Self::AlreadyRunning => "already_running",
        }
    }
}

/// A session as an outcome names it: its kind, id, title and agent, and where it runs.
fn describe(state: &Arc<AppState>, session: &Session) -> String {
    format!(
        "the {kind} session `{id}` (\"{title}\", {agent}){place}",
        kind = session.kind.name(),
        id = session.id,
        title = one_line(&session.title),
        agent = session.agent.label(),
        place = place_of(state, session),
    )
}

/// What a session is told of the session it asked for, in the call's result and in the message
/// alike: where it was started, that it reports to the user, what became of the branches it asked
/// to transfer, and what that leaves the caller able to do with the session.
pub(super) fn outcome_note(
    state: &Arc<AppState>,
    caller: &Session,
    started: &Session,
    transferred: Option<&TransferOutcome>,
    approval: &str,
) -> String {
    let branches = transferred.map(branch_outcome).unwrap_or_default();
    format!(
        "{approval}: Octoboard started {described}, with your brief as its opening prompt. It has \
         no owner: it reports to the user, not to you. {NOT_YOURS_TO_DRIVE} Follow the user's \
         direction on whether to continue your own work or end after passing the work on.\
         {branches} {reach}",
        described = describe(state, started),
        reach = reach_note(state, caller, started),
    )
}

/// [`outcome_note`] for a session opened on an agent's own record of a conversation: there is no
/// brief, and what it continues is a conversation the user knows.
pub(super) fn record_outcome_note(
    state: &Arc<AppState>,
    caller: &Session,
    started: &Session,
    approval: &str,
) -> String {
    format!(
        "{approval}: Octoboard opened {described}, continuing the conversation you asked for. It \
         has no owner: it reports to the user, not to you, and it was given no instruction, so it \
         waits at its prompt. {NOT_YOURS_TO_DRIVE} Follow the user's direction on whether to \
         continue your own work or end. {reach}",
        described = describe(state, started),
        reach = reach_note(state, caller, started),
    )
}

/// [`outcome_note`] for a session that held the conversation and was brought back (or found
/// running): `action` is what was done, as the result names it. It is where it was and reports to
/// whom it did, since nothing here changes an owner.
pub(super) fn resume_outcome_note(
    state: &Arc<AppState>,
    caller: &Session,
    session: &Session,
    action: ResumeAction,
    reopened_owners: &[String],
    rename_failure: Option<&str>,
    approval: &str,
) -> String {
    let described = describe(state, session);
    let done = match action {
        ResumeAction::Reopened => {
            format!("Octoboard reopened {described}, continuing its conversation")
        }
        ResumeAction::Resumed => {
            format!("Octoboard resumed {described}, continuing its conversation")
        }
        ResumeAction::AlreadyRunning => {
            format!("{described} was already running by then, so Octoboard did nothing")
        }
        // A session opened on an agent's own record is worded by `record_outcome_note`.
        ResumeAction::Opened => unreachable!("an opened record is not a resume of a session"),
    };
    let owners = if reopened_owners.is_empty() {
        String::new()
    } else {
        let names = reopened_owners
            .iter()
            .map(|id| match state.store.get_session(id) {
                Ok(Some(owner)) => label(&owner),
                _ => format!("`{id}`"),
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(" Its archived owners were reopened with it: {names}.")
    };
    let reports_to = match reports_to(state, session) {
        ReportsTo::Session(owner) => {
            format!("reports to its owner {}, as before", label(&owner))
        }
        ReportsTo::Unreadable => "reports to its owner, as before".to_string(),
        ReportsTo::User => "reports to the user, as before".to_string(),
    };
    let rename = rename_failure.map_or_else(String::new, |reason| {
        format!(" It could not be renamed ({reason}); it keeps its name.")
    });
    // A session found running was not given anything by this request.
    let waits = if action == ResumeAction::AlreadyRunning {
        ""
    } else {
        ", and it was given no instruction, so it waits at its prompt"
    };
    format!(
        "{approval}: {done}.{owners}{rename} It {reports_to}, not to you{waits}. \
         {NOT_YOURS_TO_DRIVE} Follow the user's direction on whether to continue your own work or \
         end. {reach}",
        reach = reach_note(state, caller, session),
    )
}

/// The result of a resuming request, the same whichever way it was carried out.
pub(super) fn resume_result(
    action: ResumeAction,
    session: &Session,
    reopened_owners: &[String],
    renamed: Option<bool>,
    note: &str,
) -> Value {
    let mut result = json!({
        "action": action.as_str(),
        "session": session.id,
        "kind": session.kind,
        "agent": session.agent,
        "title": session.title,
        "console": session.console_id,
        "project": session.project_id,
        "owner": session.bound_to,
        "reopened_owners": reopened_owners,
        "note": note,
    });
    if let Some(renamed) = renamed {
        result["renamed"] = json!(renamed);
    }
    result
}

/// What a session asked for leaves the caller able to do with it, as the end of an outcome note
/// says: share information, read it where the caller can read sessions, and bind itself to it
/// where the caller could.
fn reach_note(state: &Arc<AppState>, caller: &Session, session: &Session) -> String {
    let tool = |name: &str| qualified_tool_name(caller.agent, name);
    let has = |name: &str| tool_by_name(caller.kind, caller.is_worker(), name).is_some();
    let reads = if has("get_session") {
        format!(", and `{}` reads it", tool("get_session"))
    } else {
        String::new()
    };
    let can_bind =
        has("bind_to_session") && caller.bound_to.is_none() && can_own(state, caller, session);
    let bind = if can_bind {
        format!(
            " Should you want it to drive your work instead, `{}` with `owner` set to `{}` makes \
             you its session; that call's result tells you what then changes.",
            tool("bind_to_session"),
            session.id
        )
    } else {
        String::new()
    };
    format!(
        "`{inform_session}` tells it something that touches its work{reads}.{bind}",
        inform_session = tool("inform_session"),
    )
}

/// Whether `owner` could own `session` right now, by the rules a binding keeps.
fn can_own(state: &Arc<AppState>, session: &Session, owner: &Session) -> bool {
    matches!(
        coordinator::bindings::owner_refusal(state, session, owner),
        Ok(None)
    )
}

/// What became of the branches a request transferred, as a sentence or two for the outcome note,
/// each opening with a space.
pub(crate) fn branch_outcome(outcome: &TransferOutcome) -> String {
    let mut text = String::new();
    match outcome {
        TransferOutcome::Moved(moved) => {
            if !moved.moved.is_empty() {
                let (_, them) = branch_noun_and_pronoun(&moved.moved);
                text.push_str(&format!(
                    " Transferred to it: {}. It is now the session responsible for {them} and you \
                     no longer are.",
                    named_branches(&moved.moved)
                ));
            }
            if !moved.unmoved.is_empty() {
                let (them, they, were, stay) = match moved.unmoved.as_slice() {
                    [_] => ("it", "It", "it was", "stays as it was"),
                    _ => ("them", "They", "they were", "stay as they were"),
                };
                text.push_str(&format!(
                    " Not transferred, because by then you were no longer responsible for {them} \
                     or {were} gone: {}. {they} {stay}.",
                    named_branches(&moved.unmoved)
                ));
            }
        }
        TransferOutcome::Failed(branches) => {
            let (_, them) = branch_noun_and_pronoun(branches);
            text.push_str(&format!(
                " Octoboard could not record the transfer of {}, so you are still responsible for \
                 {them}.",
                named_branches(branches)
            ));
        }
    }
    text
}
