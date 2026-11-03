//! Carrying out an approval: what each kind of request does once the user has said yes, or the
//! setting has said it for them.

use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};

use crate::branch_origin::handover::move_branches;
use crate::coordinator::{self, open::OpenRequest};
use crate::protocol::{error_code, CodedError, Session, SessionStatus};
use crate::state::AppState;

use super::outcome::{
    outcome_note, record_outcome_note, resume_outcome_note, resume_result, ResumeAction,
};
use super::request::{Action, Ask, Request};
use super::{not_waiting, outcome_message, Edits, Performed};

/// An approval carried out: the call's result, and the same outcome as a message for the session
/// that asked.
pub(super) struct Approved {
    pub(super) result: Value,
    pub(super) caller_id: String,
    /// The session the approval started or brought back, or found running.
    pub(super) started: Session,
    pub(super) message: String,
}

/// Carries out an approval. A start opens the session the caller described, unbound, then hands
/// it the branches the request names. A resume brings back the session that holds the
/// conversation, or opens one on the record of it. A switch or a handoff runs the operation of its
/// tool with the values the user ended the dialog on. The caller keeps running, and does not own
/// what it asked for, except for the session a switch or a handoff leaves in its care.
pub(super) async fn carry_out<F, Fut, R, RFut>(
    state: &Arc<AppState>,
    request: &Request,
    edits: Edits,
    launch: F,
    resume: R,
) -> Result<Approved>
where
    F: FnOnce(OpenRequest) -> Fut,
    Fut: Future<Output = Result<Session>>,
    R: FnOnce(String) -> RFut,
    RFut: Future<Output = Result<()>>,
{
    let caller = state.session_record(&request.session)?;
    // Withdrawn when its process ended, but that can land while the answer is on its way.
    if caller.status.has_no_process() || !state.has_process(&caller.id) {
        return Err(not_waiting(&caller.id));
    }
    let (started, result, note) = match &request.ask {
        Ask::Session(action @ Action::Start { open, handover }) => {
            let started = launch(open.clone()).await?;
            let handed = match handover {
                Some(handover) => Some(move_branches(state, handover, &caller, &started).await),
                None => None,
            };
            let note = outcome_note(
                state,
                &caller,
                &started,
                handed.as_ref(),
                request.approval(action),
            );
            let result = json!({
                "session": started.id,
                "role": started.role,
                "agent": started.agent,
                "title": started.title,
                "console": started.console_id,
                "project": started.project_id,
                "note": note,
            });
            (started, result, note)
        }
        Ask::Session(action @ Action::ResumeRecord { open, .. }) => {
            let started = launch(open.clone()).await?;
            let note = record_outcome_note(state, &caller, &started, request.approval(action));
            let result = resume_result(ResumeAction::Opened, &started, &[], None, &note);
            (started, result, note)
        }
        Ask::Session(action @ Action::Resume { target, rename, .. }) => {
            let approval = request.approval(action);
            bring_back(state, approval, &caller, target, rename.as_deref(), resume).await?
        }
        Ask::Change(change) => {
            let Performed { session, result } = change.perform().start(edits)?.await?;
            let note = result["note"].as_str().unwrap_or_default().to_string();
            (session, result, note)
        }
    };
    let message = outcome_message(caller.agent, request.tool(), &note);
    Ok(Approved {
        result,
        caller_id: caller.id,
        started,
        message,
    })
}

/// Brings back `target`, which held the conversation when the request was made: reopens or
/// resumes it through `resume`, with the owners it brings back, then gives it the name asked for.
/// What it was found as now decides what is done: one that is running by now has nothing to
/// resume, which is told rather than failed, and a name is given only to a session this reopened
/// from the archive. A name that cannot be given is told too, since the session is back by then.
async fn bring_back<R, RFut>(
    state: &Arc<AppState>,
    approval: &'static str,
    caller: &Session,
    target: &str,
    rename: Option<&str>,
    resume: R,
) -> Result<(Session, Value, String)>
where
    R: FnOnce(String) -> RFut,
    RFut: Future<Output = Result<()>>,
{
    let before = state.session_record(target)?;
    let mut action = match before.status {
        SessionStatus::Archived => ResumeAction::Reopened,
        status if status.has_no_process() => ResumeAction::Resumed,
        _ => ResumeAction::AlreadyRunning,
    };
    let owners = if action == ResumeAction::AlreadyRunning {
        Vec::new()
    } else {
        coordinator::resume::archived_owners(state, target)?
    };
    if action != ResumeAction::AlreadyRunning {
        if let Err(err) = resume(target.to_string()).await {
            if refused_as_running(state, &err, target)? {
                action = ResumeAction::AlreadyRunning;
            } else {
                return Err(err);
            }
        }
    }
    let mut renamed = None;
    let mut rename_failure = None;
    if let Some(title) = rename {
        if action != ResumeAction::Reopened {
            renamed = Some(false);
            rename_failure = Some(format!(
                "only a session reopened from the archive is renamed, and this one was {}",
                if action == ResumeAction::Resumed {
                    "not archived"
                } else {
                    "already running"
                }
            ));
        } else if state.session_record(target)?.title != title {
            match coordinator::lifecycle::rename_session(state, target, title) {
                Ok(_) => renamed = Some(true),
                Err(err) => {
                    renamed = Some(false);
                    rename_failure = Some(format!("{err:#}"));
                }
            }
        }
    }
    // Read again: the resume and the rename changed what the session records.
    let session = state.session_record(target)?;
    // The owners the resume reopened: those it listed that are no longer archived.
    let reopened_owners: Vec<String> = if action == ResumeAction::AlreadyRunning {
        Vec::new()
    } else {
        owners
            .into_iter()
            .filter(|id| {
                matches!(
                    state.store.get_session(id),
                    Ok(Some(owner)) if owner.status != SessionStatus::Archived
                )
            })
            .collect()
    };
    let note = resume_outcome_note(
        state,
        caller,
        &session,
        action,
        &reopened_owners,
        rename_failure.as_deref(),
        approval,
    );
    let result = resume_result(action, &session, &reopened_owners, renamed, &note);
    Ok((session, result, note))
}

/// Whether a resume of `target` was refused because the session is running, or its launch is
/// under way, which the user's own resume counts as done. A refusal for a claim that is not a
/// launch of `target` (an archived owner's, or one held for an account switch, a handoff, a
/// handover of branches or a binding change) counts only when `target` is found out of its
/// no-process status by now; otherwise the approval could not be carried out.
pub(super) fn refused_as_running(
    state: &Arc<AppState>,
    err: &anyhow::Error,
    target: &str,
) -> Result<bool> {
    let Some(coded) = err.downcast_ref::<CodedError>() else {
        return Ok(false);
    };
    let for_target = coded.params.get("session").map(String::as_str) == Some(target);
    let launching = match coded.code {
        error_code::SESSION_ALREADY_RUNNING => for_target,
        // A switch's claim raises the same code as a launch's. A claim gone by now falls through
        // to the no-process re-check.
        error_code::SESSION_ALREADY_STARTING => for_target && state.is_held_by_launch(target),
        _ => return Ok(false),
    };
    Ok(launching || !state.session_record(target)?.status.has_no_process())
}
