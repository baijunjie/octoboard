//! Finding the session a tool acts on, and describing a session to the caller.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::branch_origin::session_branches;
use crate::coordinator;
use crate::mcp::args::optional_string;
use crate::protocol::{Role, Session, SessionStatus};
use crate::state::{AppState, WriteHold};

/// Finds a session the caller may read: any session on record, of any console or project. What a
/// session may *do* to another is narrower, and is [`resolve_owned_session`]'s.
pub(in crate::mcp) fn resolve_session(state: &Arc<AppState>, wanted: &str) -> Result<Session> {
    state
        .store
        .get_session(wanted)?
        .ok_or_else(|| anyhow!("there is no session `{wanted}`"))
}

/// The session a read tool's optional `session` argument names, the caller itself when it is left
/// out (or names the caller), read afresh from the store. Reading, like [`resolve_session`].
pub(in crate::mcp) fn session_or_caller(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Session> {
    match optional_string(arguments, "session") {
        Some(wanted) if wanted != caller.id => resolve_session(state, &wanted),
        _ => state.session_record(&caller.id),
    }
}

/// Finds a session the caller may act on: one it can read that is bound to it. Every tool that
/// writes into another session goes through this and nothing else, so the rule that reads are wide
/// and writes are the caller's own is kept in one place. A session that is unbound, or bound to
/// another session, is refused with a reason (naming the owner when there is one), so the model
/// leaves it alone rather than retrying.
pub(in crate::mcp) fn resolve_owned_session(
    state: &Arc<AppState>,
    caller: &Session,
    wanted: &str,
) -> Result<Session> {
    let session = resolve_session(state, wanted)?;
    // `send_message` refuses an archived session, so it is not offered for one.
    let send_message = crate::mcp::qualified_tool_name(caller.agent, "send_message");
    let archived = session.status == SessionStatus::Archived;
    let tell_it = |tail: &str| {
        if archived {
            String::new()
        } else {
            format!(
                " You can still tell it something with `{send_message}`, which it weighs rather \
                 than obeys{tail}"
            )
        }
    };
    // Including a console session's own: one that ended itself would leave its console's project
    // sessions reporting to nothing.
    if wanted == caller.id {
        bail!(
            "session `{wanted}` is your own session, which is not yours to act on; if you meant \
             to say something, say it to the user"
        );
    }
    if session.role == Role::Console {
        bail!(
            "session `{wanted}` is a console session; acting on one is the user's to do, not \
             yours.{}",
            tell_it("")
        );
    }
    match session.bound_to.as_deref() {
        Some(owner) if owner == caller.id => Ok(session),
        Some(owner) => {
            let owner_record = state.store.get_session(owner)?;
            // "Another" only when the owner is the caller's own kind: a project session is not
            // told that a console session is another one of its kind.
            let kind = owner_record
                .as_ref()
                .map_or("session".to_string(), |owner| {
                    let article = if owner.role == caller.role {
                        "another"
                    } else {
                        "a"
                    };
                    format!("{article} {} session", owner.role.kind())
                });
            let title = owner_record
                .map(|owner| format!(" (\"{}\")", owner.title))
                .unwrap_or_default();
            bail!(
                "session `{wanted}` is bound to {kind}, `{owner}`{title}. Only that session \
                 instructs or drives it, so this is not yours to do.{}",
                tell_it(", and its owner is sent a copy")
            )
        }
        None => bail!(
            "session `{wanted}` is not bound to any session: the user opened it themselves and \
             kept it outside the orchestration. It is theirs, not yours to drive.{}",
            tell_it("")
        ),
    }
}

/// How many sessions are bound to `session`, for it to be described alone with (see
/// [`describe_session`]): counted in the store, not by reading the whole table.
pub(in crate::mcp) fn owned_count_of(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<HashMap<String, usize>> {
    coordinator::bindings::owned_count_of(state, session)
}

/// How many sessions are bound to each session, by id, for the sessions of a listing to be
/// described with (see [`describe_session`]). Read once for the listing: counting per session is
/// a scan of the whole table each time.
pub(in crate::mcp) fn owned_counts_of(state: &Arc<AppState>) -> Result<HashMap<String, usize>> {
    Ok(coordinator::bindings::owned_counts(
        &state.store.list_sessions()?,
    ))
}

/// The names of the branches each of `sessions` is responsible for, read on the blocking pool
/// (the store and the reflog files, and no process).
pub(in crate::mcp) async fn branch_names_of(
    state: &Arc<AppState>,
    sessions: Vec<Session>,
) -> Result<HashMap<String, Vec<String>>> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || session_branches::names_by_session(&state, &sessions))
        .await?
}

/// `caller` is the session asking. Everything here is the same whoever asks except `yours`, which
/// is relative to `caller`: a console may hold several console sessions, and each reads a
/// session's owner alike but is told only of its own that they are its to act on.
///
/// `branches` is the names of the branches the sessions described are responsible for, by session
/// id, as [`branch_names_of`] reads them for all the sessions of a listing at once; a session
/// with no entry is responsible for none. `owned` is how many sessions are bound to each, as
/// [`owned_counts_of`] reads them; likewise no entry is none.
///
/// `pub(crate)` for the tests of `shared_config`, which describe a session being restarted.
pub(crate) fn describe_session(
    state: &Arc<AppState>,
    session: &Session,
    caller: &Session,
    branches: &HashMap<String, Vec<String>>,
    owned: &HashMap<String, usize>,
) -> Result<Value> {
    let owner_kind = match &session.bound_to {
        Some(owner) => state
            .store
            .get_session(owner)?
            .map(|owner| owner.role.kind()),
        None => None,
    };
    // Looked at before the status is read, so that the status, which reads the screen as that look
    // left it, and the hold cannot disagree about a dialog that came up in between.
    let write_hold = state.write_hold(&session.id);
    // The account by id and name, as `get_account_quotas` and `switch_session_account` name it:
    // `default` is the agent's default account. Never its directory, which is the user's.
    let account_name = state
        .store
        .account_display_name(session.agent, session.account_id.as_deref())?;
    let mut description = json!({
        "session": session.id,
        "title": session.title,
        // What the session list shows for it: the agent's current task before the title, as the
        // user reads it, so that "the session called X" can be matched to a record.
        "display_title": state.display_title(session),
        "agent": session.agent,
        // The agent's own id of the conversation, `request_resume_session`'s `conversation`.
        // Null until the agent has reported one.
        "agent_session_id": session.agent_session_id,
        "origin": session.origin,
        "status": state.shown_status(session),
        // What an idle session has left running in the background: it takes a message like any
        // idle session, but its work is not over. Empty for every other status.
        "background_tasks": state.shown_background_tasks(session),
        // How full the session's context window is, as the UI shows it: owners weigh it when
        // choosing which session gets a task. Null while no size is known: before the session's
        // first report, and after a compaction until its next request.
        "context": session.context,
        "account": session.account_id.as_deref().unwrap_or("default"),
        "account_name": account_name,
        // What the agent's records show the session running with, as the agent spells it. Null
        // until reported; `default_effort` is what the agent runs that model at by itself, given
        // where the records name the model but no effort and its catalogue knows one.
        "observed_model": session.observed_model,
        "observed_effort": session.observed_effort,
        "default_effort": state.default_effort_of(&session.id),
        // Whether the agent has a conversation of this session to continue. It says a turn was
        // taken, not that the agent still holds the record: a `reopen_session` that finds the
        // record gone is refused as `conversation_unresumable`.
        "has_conversation": session.has_conversation,
        "pinned": session.pinned,
        // A locked session cannot be archived.
        "locked": session.locked,
        // The names only: the worktree each stands in is `get_session_branches`'.
        "branches": branches.get(&session.id).cloned().unwrap_or_default(),
        "role": session.role,
        "orchestration_role": session.orchestration_role,
        "console": session.console_id,
        "project": session.project_id,
        // The session this one reports to. Null for a project session means unbound: the user
        // opened it themselves and kept it outside the orchestration. It is also null for a
        // console session, which is never bound, so `role` is what tells the two apart. Only a
        // session whose owner is the caller is the caller's to act on.
        "owner": session.bound_to,
        // Whether that owner is a console session or a project session; null with no owner.
        "owner_kind": owner_kind,
        // The sessions bound to this one in any status, archived ones included, as the rules on
        // who can own a session and on transferring one count them.
        "owned_sessions": owned.get(&session.id).copied().unwrap_or(0),
        "yours": session.bound_to.as_deref() == Some(caller.id.as_str()),
        "started_at": session.started_at,
        "ended_at": session.ended_at,
        // Messages sent to this session that are waiting to be written into it, including what is
        // kept for an interrupted session to take at its resume.
        "queued_messages": state.queued_count(&session.id)
            + state.kept_unqueued_count(&session.id)?,
        // What keeps them waiting while `status` alone would say the session can take one, so an
        // idle session with messages queued is not left unexplained. Null when nothing holds it.
        "writes_held": write_hold.map(|hold| {
            let mut held = json!({
                "reason": hold.code(),
                "explanation": write_hold_explanation(hold),
            });
            if let Some(dialog) = hold.dialog() {
                held["dialog"] = json!(dialog);
            }
            held
        }),
    });
    // The session's own launch overrides, kept apart from the observed ones: absent follows its
    // project's (else console's) current setting, `null` leaves the choice to the agent, a value
    // pins it.
    if let Some(model) = &session.model {
        description["model"] = json!(model);
    }
    if let Some(effort) = &session.effort {
        description["effort"] = json!(effort);
    }
    Ok(description)
}

/// What an owner is told about a hold on a session's writes, and what, if anything, it can do.
fn write_hold_explanation(hold: WriteHold) -> &'static str {
    match hold {
        WriteHold::Starting => {
            "Its agent has not yet signalled that it is past its startup screens, so nothing is \
             written into it in case one is still up. Messages for it are queued and go in once \
             that signal arrives; if this lasts, the user has to look at the session."
        }
        WriteHold::TrustConfirmation => {
            "Its agent is asking whether to trust its folder. Messages for it are queued until \
             that is answered."
        }
        WriteHold::AgentDialog(_) => {
            "Its agent is showing a dialog that only the user can answer, and a message written \
             into it would answer the dialog instead. Messages for it are queued until the dialog \
             is gone."
        }
        WriteHold::AgentAction => {
            "Its agent is waiting on the user. Messages for it are queued until the user has \
             answered."
        }
        WriteHold::AgentError => {
            "Its agent has just stopped on an error. Messages for it are queued for a moment, \
             until it is clear whether the agent is showing a prompt only the user can answer."
        }
        WriteHold::Handoff => {
            "It is being handed off to another agent. Messages for it are queued and go to the \
             session that takes its place."
        }
        WriteHold::SharingChange => {
            "It is being restarted for a change of its account's sharing of the agent's default \
             configuration, and is resumed on its conversation afterwards. Messages for it are \
             queued and written into the resumed process."
        }
        WriteHold::SubmissionUncertain => {
            "Its native message queue did not confirm accepting a message. That message may already \
             be queued and will not be sent again. Later messages are held for at most three \
             seconds, or until the agent reports another submitted prompt. Check the conversation before retrying manually; its input \
             box still contains the user's own draft."
        }
        WriteHold::UnsentMessage => {
            "A message written into it has not been confirmed sent; the user has been told. \
             After a three-second grace period its submit key is retried once, without repeating \
             the message text, unless a prompt was already submitted. Later messages wait for that \
             attempt or a submitted prompt; dialogs and trust confirmations must be answered first. \
             Do not send the message text again."
        }
    }
}

#[cfg(test)]
mod tests;
