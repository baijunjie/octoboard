//! The records the tools describe to an agent: a session, a project and a console as JSON. Every
//! tool that reports one gives this same view, so a field means the same wherever it is read.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};

use crate::accounts::choice;
use crate::branch_origin::session_branches;
use crate::coordinator;
use crate::git_workflow;
use crate::messaging::idle_children;
use crate::project_git::remote_url;
use crate::protocol::{AgentId, Console, Project, ProjectAccount, Session, SessionStatus};
use crate::state::{bindings, AppState, WriteHold};

/// How many sessions are bound to each session, by id, for the sessions of a listing to be
/// described with (see [`describe_session`]). Read once for the listing: counting per session is
/// a scan of the whole table each time.
pub(super) fn owned_counts_of(state: &Arc<AppState>) -> Result<HashMap<String, usize>> {
    Ok(bindings::owned_counts(&state.store.list_sessions()?))
}

/// The names of the branches each of `sessions` is responsible for, read on the blocking pool
/// (the store and the reflog files, and no process).
pub(super) async fn branch_names_of(
    state: &Arc<AppState>,
    sessions: Vec<Session>,
) -> Result<HashMap<String, Vec<String>>> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || session_branches::names_by_session(&state, &sessions))
        .await?
}

/// The `account` and `account_name` fields of a result for a session of `agent` on `account_id`:
/// the account by id and name, as `list_agents` and `switch_session_account` name it, where
/// `default` is the agent's default account. Never its directory, which is the user's.
pub(super) fn account_fields(
    state: &Arc<AppState>,
    agent: AgentId,
    account_id: Option<&str>,
) -> Result<(String, String)> {
    let name = state.store.account_display_name(agent, account_id)?;
    Ok((account_id.unwrap_or("default").to_string(), name))
}

/// Puts the `model` and `effort` the session has of its own into `result`; a session that follows
/// its project's (else console's) setting has neither field, which is different from a `null`,
/// an explicit unset.
pub(super) fn insert_own_launch(result: &mut Value, session: &Session) {
    if let Some(model) = &session.model {
        result["model"] = json!(model);
    }
    if let Some(effort) = &session.effort {
        result["effort"] = json!(effort);
    }
}

/// `caller` is the session asking. Everything here is the same whoever asks except `yours`, which
/// is relative to `caller`: a console may hold several console sessions, and each reads a
/// session's owner alike but is told only of its own that they are its to act on.
///
/// `branches` is the names of the branches the sessions described are responsible for, by session
/// id, as [`branch_names_of`] reads them for all the sessions of a listing at once; a session
/// with no entry is responsible for none. `owned` is how many sessions are bound to each, as
/// [`owned_counts_of`] reads them; likewise no entry is none.
pub(super) fn describe_session(
    state: &Arc<AppState>,
    session: &Session,
    caller: &Session,
    branches: &HashMap<String, Vec<String>>,
    owned: &HashMap<String, usize>,
) -> Result<Value> {
    Ok(describe_session_shown(state, session, caller, branches, owned)?.0)
}

/// [`describe_session`], with the status the description shows, for a caller that decides more
/// by it than the description says: read once, so the two cannot differ.
pub(super) fn describe_session_shown(
    state: &Arc<AppState>,
    session: &Session,
    caller: &Session,
    branches: &HashMap<String, Vec<String>>,
    owned: &HashMap<String, usize>,
) -> Result<(Value, SessionStatus)> {
    let owner_kind = match &session.bound_to {
        Some(owner) => state
            .store
            .get_session(owner)?
            .map(|owner| owner.kind.name()),
        None => None,
    };
    // Looked at before the status is read, so that the status, which reads the screen as that look
    // left it, and the hold cannot disagree about a dialog that came up in between.
    let write_hold = state.write_hold(&session.id);
    let shown = state.shown_status(session);
    let (account, account_name) =
        account_fields(state, session.agent, session.account_id.as_deref())?;
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
        "status": shown,
        // Whether an idle session's last turn ended abnormally; false for every other status.
        "paused": state.shown_paused(session, shown),
        // What an idle session has left running in the background: it takes a message like any
        // idle session, but its work is not over. Empty for every other status.
        "background_tasks": state.shown_background_tasks(&session.id, shown),
        // How full the session's context window is, as the UI shows it: owners weigh it when
        // choosing which session gets a task. Null while no size is known: before the session's
        // first report, and after a compaction until its next request.
        "context": session.context,
        "account": account,
        "account_name": account_name,
        // What the agent's records show the session running with, as the agent spells it. Null
        // until reported; `default_effort` is what the agent runs that model at by itself, given
        // where the records name the model but no effort and its catalogue knows one.
        "observed_model": session.observed_model,
        "observed_effort": session.observed_effort,
        "default_effort": state.default_effort_of(&session.id),
        // Whether the agent has a conversation of this session to continue. It says a turn was
        // taken, not that the agent still holds the record: a `resume_session` that finds the
        // record gone is refused as `conversation_unresumable`.
        "has_conversation": session.has_conversation,
        "pinned": session.pinned,
        // A locked session cannot be archived.
        "locked": session.locked,
        // The names only: the worktree each stands in is `list_session_branches`'.
        "branches": branches.get(&session.id).cloned().unwrap_or_default(),
        "kind": session.kind,
        "role": session.role,
        "console": session.console_id,
        "project": session.project_id,
        // Whether the session is idle after its last report, its own or synthesised, reached its
        // owner: at its prompt with nothing running, nothing queued for it, and no turn of its own
        // started since. An owner tells a child that is finished but not archived from one awaiting
        // instructions by it. False for a session with no owner.
        "idle_after_report": idle_children::idle_after_report_shown(state, session, shown)
            .is_some(),
        // The session this one reports to. Null for a project session means unbound: the user
        // opened it themselves and kept it outside the orchestration. It is also null for a
        // console session, which is never bound, so `kind` is what tells the two apart. Only a
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
    insert_own_launch(&mut description, session);
    Ok((description, shown))
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
            "Its agent's own message queue did not confirm taking a message. That message may \
             already be queued or running and will not be sent again, so do not send it again \
             without checking the session's history first. Later messages are held for at most \
             three seconds, or until the agent reports another submitted prompt. Octoboard did not \
             touch its input box."
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

/// An object keyed by agent, one entry for each.
fn per_agent_object(entry: impl Fn(AgentId) -> Value) -> Value {
    Value::Object(
        AgentId::ALL
            .iter()
            .map(|agent| (agent.wire_name().to_string(), entry(*agent)))
            .collect(),
    )
}

/// A project's git state as the daemon holds it, read from memory without going to the remote:
/// `null` while it holds none (no check has run since it started, e.g. a project of a console never
/// shown), else the `GitStatus` fields except `project` and `checked_seconds_ago`, how long ago
/// the last check completed (`null` when none has since it started, so the counts then come from
/// the local repository alone). A credential in the error `git` reported is removed, as the
/// remote's is.
fn describe_git_status(state: &Arc<AppState>, project: &Project) -> Value {
    let Some((status, checked)) = state.git_status_with_age(&project.id) else {
        return Value::Null;
    };
    json!({
        "repository": status.repository,
        "branch": status.branch,
        "detached": status.detached,
        "upstream": status.upstream,
        "ahead": status.ahead,
        "behind": status.behind,
        "activity": status.activity,
        "error": status.error.as_deref().map(without_credentials_in),
        "checked_seconds_ago": checked.map(|age| age.as_secs()),
    })
}

/// `text` with the credential of any URL in it removed.
fn without_credentials_in(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let url = word.trim_matches(|c: char| c.is_whitespace() || "'\"`(),".contains(c));
            if url.contains("://") && remote_url::has_credentials(url) {
                word.replace(url, &remote_url::without_credentials(url))
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A project's settings as every tool that reports one gives them: the account setting of each
/// agent as `inherit`, `default` or an account id, and the remote without any credential it
/// carries, and its git workflow settings as the project's own choices (`null` follows the
/// application-wide value) beside the values in force, and its git state as last read (see
/// [`describe_git_status`]). The agent, account and launch settings are given as stored, with what
/// they resolve to under the console in `effective`: a project session cannot read the console.
pub(super) fn describe_project(state: &Arc<AppState>, project: &Project) -> Result<Value> {
    let console = state.store.require_console(&project.console_id)?;
    Ok(json!({
        "project": project.id,
        "name": project.name,
        "description": project.description,
        "host": project.host_id,
        "path": project.path,
        "source": project.source,
        "remote_url": project.remote_url.as_deref().map(remote_url::without_credentials),
        "default_agent": project.default_agent,
        "pinned": project.pinned,
        "tags": project.tags,
        "trust_consent": project.trust_consent,
        "accounts": per_agent_object(|agent| match project.accounts.get(agent) {
            ProjectAccount::Inherit => json!("inherit"),
            ProjectAccount::Default => json!("default"),
            ProjectAccount::Account { id } => json!(id),
        }),
        "launch": per_agent_object(|agent| json!(project.launch.get(agent))),
        // The same resolution a launch makes, so that what an agent reads is what a session
        // started in this project gets when nothing names otherwise.
        "effective": {
            "default_agent": choice::session_agent(None, Some(project), &console),
            "accounts": per_agent_object(|agent| {
                json!(console
                    .resolve_project_account(agent, project.accounts.get(agent))
                    .as_deref()
                    .unwrap_or("default"))
            }),
            "launch": per_agent_object(|agent| {
                json!(coordinator::launch::project_launch_settings(project, &console, agent))
            }),
        },
        "git_workflow": {
            "override": project.git_workflow,
            "effective": git_workflow::effective(&state.store, project)?,
        },
        "git_status": describe_git_status(state, project),
    }))
}

/// A console's settings as `get_console` and `update_console` give them. The avatar is a large
/// `data:` URL of no use to the caller, so only whether there is one is said.
pub(super) fn describe_console(console: &Console) -> Value {
    json!({
        "console": console.id,
        "name": console.name,
        "default_agent": console.default_agent,
        "project_session_agent": console.project_session_agent,
        "accounts": per_agent_object(|agent| {
            json!(console.accounts.get(agent).as_deref().unwrap_or("default"))
        }),
        "launch": per_agent_object(|agent| json!(console.launch.get(agent))),
        "project_session_launch": per_agent_object(|agent| {
            json!(console.project_session_launch.get(agent))
        }),
        "has_icon": console.icon.is_some(),
        "workdir": console.workdir,
    })
}

#[cfg(test)]
mod tests;
