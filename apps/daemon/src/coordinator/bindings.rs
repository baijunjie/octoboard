//! Which session reports to which: binding, transferring and releasing a session, and the refusals
//! of an owner. The reads of who is bound to whom are `crate::state::bindings`'.

use std::sync::Arc;

use anyhow::Result;

use crate::coordinator::pages;
use crate::messaging::naming::label;
use crate::messaging::owner_notices;
use crate::protocol::{error_code, CodedError, Role, Session, SessionKind, SessionStatus};
use crate::session_tools;
use crate::state::bindings::bound_sessions;
use crate::state::AppState;

/// What a new session should be bound to, as the owner's record: always `None` for a console
/// session, which is never bound; for a project session, whatever `requested` names, which must be
/// a console session of `console_id`, or a project session of the new session's own project that is
/// unbound or a bound coordinator session, or the request is refused with `unknown_session` — a
/// binding changes afterwards only at the session's or its owner's own request, so naming one that
/// does not exist, or belongs to another console, must not be let through to be discovered later. A
/// project session with the role of a worker is refused as an owner for the same reason, and this
/// is where the depth limit is enforced, not only by which tools are offered: a session bound to a
/// bound coordinator would otherwise make a third level. A session still starting is refused too:
/// its launch may fail, and a failed launch deletes the session whatever is bound to it.
pub(super) fn resolve_owner(
    state: &Arc<AppState>,
    console_id: &str,
    project_id: Option<&str>,
    kind: SessionKind,
    requested: Option<String>,
) -> Result<Option<Session>> {
    if kind == SessionKind::Console {
        return Ok(None);
    }
    let Some(target_id) = requested else {
        return Ok(None);
    };
    let target = state
        .store
        .get_session(&target_id)?
        .filter(|target| {
            target.console_id == console_id
                && target.status != SessionStatus::Starting
                && match target.kind {
                    SessionKind::Console => true,
                    SessionKind::Project => {
                        !target.is_worker()
                            && target.project_id.is_some()
                            && target.project_id.as_deref() == project_id
                    }
                }
        })
        .ok_or_else(|| CodedError::unknown_session(&target_id))?;
    Ok(Some(target))
}

/// Why `owner` cannot own `session`, in prose for the model, or `None` when it can. These are the
/// rules every binding made after a session was opened keeps, whether the session binds itself
/// ([`bind_session`]) or its owner hands it to another ([`transfer_session`]): an owner is a
/// running console session of the session's console, or a running project session of its project
/// that can own sessions (unbound, or a bound coordinator); and a session bound to a project
/// session owns none, so one that has sessions of its own, archived ones included since its owner
/// can reopen them, can only be bound to a console session. Orchestration stays at most two levels
/// deep, and no binding can close a cycle.
pub(crate) fn owner_refusal(
    state: &Arc<AppState>,
    session: &Session,
    owner: &Session,
) -> Result<Option<String>> {
    if owner.id == session.id {
        return Ok(Some(format!("{} cannot be its own owner", label(session))));
    }
    if owner.console_id != session.console_id {
        return Ok(Some(format!(
            "{} belongs to another console, and an owner must be in the console of the session \
             it owns",
            label(owner)
        )));
    }
    if owner.kind == SessionKind::Project {
        if owner.project_id != session.project_id {
            return Ok(Some(format!(
                "{} is a session of another project, and a project session owns only sessions of \
                 its own project",
                label(owner)
            )));
        }
        if owner.is_worker() {
            return Ok(Some(format!(
                "{} reports to a session and cannot own sessions of its own: orchestration is at \
                 most two levels deep",
                label(owner)
            )));
        }
        let owned = bound_sessions(state, session)?;
        if !owned.is_empty() {
            return Ok(Some(format!(
                "{} has sessions of its own ({}), and a session bound to a project session cannot \
                 own any: only a console session can own it",
                label(session),
                owned
                    .iter()
                    .map(|owned| format!("`{}`", owned.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    // A session still starting is not running yet either: its launch may fail, and a failed
    // launch deletes the session whatever is bound to it.
    if owner.status.has_no_process()
        || owner.status == SessionStatus::Starting
        || !state.has_process(&owner.id)
    {
        return Ok(Some(format!("{} is not running", label(owner))));
    }
    Ok(None)
}

/// Takes the exclusive claim on `session_id` (see [`AppState::begin_switch`]) for as long as a
/// change of the owner it is bound to takes. What a handoff or an account switch holds goes stale
/// if it changes meanwhile: the owner read at the start (`bound_to` in the `OpenRequest` for a
/// worker, the owner a coordinator reports to for a coordinator, [`owner_of`] for the switch's
/// relaunch). Taken before the session is read, so what is checked after it holds until the
/// write; refused ([`error_code::SESSION_CHANGING`]) when a launch, a resume, a handoff, a transfer
/// of branches, a switch or another binding change of it is under way.
fn claim_for_binding(state: &Arc<AppState>, session_id: &str) -> Result<crate::state::LaunchClaim> {
    state.begin_switch(session_id).map_err(|_| {
        CodedError::raised(
            error_code::SESSION_CHANGING,
            format!(
                "`{session_id}` is being started, resumed, handed off, switched to another \
                 account, having its branches transferred or having its owner changed, so its \
                 owner cannot change now: look again once that is done"
            ),
            &[("session", session_id)],
        )
    })
}

/// Refuses ([`error_code::SESSION_OWNER_CHANGING`]) when `owner_id`, the owner a binding change
/// gains or loses a session for, has its claim held — a handoff of it would otherwise move and
/// archive it with a session bound to it meanwhile. Only looked at, not claimed: an owner is not
/// rewritten, and claims on owners would refuse concurrent binds to the same one. So the check is
/// not made under a claim, which is acceptable: a write that lands before the handoff's
/// `transfer_binding` is moved by it in its transaction. Skipped when the owner is the session
/// itself, which [`owner_refusal`] refuses by its own reason.
fn refuse_if_owner_claimed(state: &Arc<AppState>, session_id: &str, owner_id: &str) -> Result<()> {
    if owner_id != session_id && state.is_launching(owner_id) {
        return Err(CodedError::raised(
            error_code::SESSION_OWNER_CHANGING,
            format!(
                "`{owner_id}` is being started, resumed, handed off, switched to another account \
                 or having its owner changed, so sessions cannot be bound to or moved from it \
                 now: look again once that is done"
            ),
            &[("session", session_id), ("owner", owner_id)],
        ));
    }
    Ok(())
}

/// Binds the unbound project session `session_id` to `owner_id`, at its own request, and tells
/// every client. Bound to a console session it stays a coordinator, keeping the sessions it
/// started and the tools it drives them with; bound to a project session it becomes a worker,
/// which only one with no sessions of its own may (see [`owner_refusal`]).
///
/// Refused, with nothing changed, for a session or an owner under a claim (see
/// [`claim_for_binding`], [`refuse_if_owner_claimed`]), a console session, a session that is
/// already bound, and an owner [`owner_refusal`] refuses. Returns the session as it is now bound,
/// and the owner as it was checked.
pub fn bind_session(
    state: &Arc<AppState>,
    session_id: &str,
    owner_id: &str,
) -> Result<(Session, Session)> {
    let _claim = claim_for_binding(state, session_id)?;
    let session = state.session_record(session_id)?;
    if session.kind == SessionKind::Console {
        anyhow::bail!("a console session cannot be bound to another session");
    }
    if let Some(owner) = &session.bound_to {
        anyhow::bail!(
            "this session is already bound to `{owner}`: only an unbound session binds itself to \
             an owner"
        );
    }
    refuse_if_owner_claimed(state, session_id, owner_id)?;
    let owner = state.session_record(owner_id)?;
    if let Some(reason) = owner_refusal(state, &session, &owner)? {
        anyhow::bail!(reason);
    }
    // Under a console session it stays the coordinator it is; under a project session it becomes
    // a worker, which may then have no sessions of its own.
    let to_console = owner.kind == SessionKind::Console;
    let role = if to_console {
        Role::Coordinator
    } else {
        Role::Worker
    };
    let bound = state
        .store
        .set_binding(session_id, None, owner_id, role, !to_console, false)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the binding changed while it was being made — this session was bound, it gained \
                 sessions of its own, or `{owner_id}` can no longer own sessions; look again \
                 before retrying"
            )
        })?;
    state.publish_session(&bound);
    withdraw_pages_if_now_worker(state, &session, &bound);
    Ok((bound, owner))
}

/// Withdraws the report-panel pages of a session that a binding change has just given the role of
/// a worker: it has no tool left to list or delete them, and the panel would go on offering
/// their form. The binding is written by now, so a failure is logged and leaves the pages for
/// [`submit_page`] to refuse.
///
/// [`submit_page`]: super::pages::submit_page
fn withdraw_pages_if_now_worker(state: &Arc<AppState>, before: &Session, after: &Session) {
    if !before.is_worker() && after.is_worker() {
        if let Err(err) = pages::withdraw_pages(state, &after.id) {
            tracing::warn!(
                session = %after.id, %err, "withdrawing the pages of a worker session failed"
            );
        }
    }
}

/// Hands `session_id`, bound to `from_id`, to the owner `to_id`, at `from_id`'s request, and tells
/// every client. What the session is bound to changes and nothing else: the sessions bound to it
/// stay bound to it. A coordinator handed to another console session stays one; handed to a
/// project session it no longer can own sessions, which only one with none of its own may be (see
/// [`owner_refusal`]).
///
/// Refused, with nothing changed, for the session or either owner under a claim (see
/// [`claim_for_binding`], [`refuse_if_owner_claimed`]), a session that is not bound to `from_id`,
/// handing a session to the owner it already has, and an owner [`owner_refusal`] refuses. Returns
/// the session as it is now bound, its new owner as it was checked, and whether the session has a
/// process to be told in; one that has none is left owed the notice for its next launch, decided
/// while the claim is held and recorded by the binding write itself, as [`release_session`] does.
pub fn transfer_session(
    state: &Arc<AppState>,
    session_id: &str,
    from_id: &str,
    to_id: &str,
) -> Result<(Session, Session, bool)> {
    let _claim = claim_for_binding(state, session_id)?;
    let session = state.session_record(session_id)?;
    if session.bound_to.as_deref() != Some(from_id) {
        anyhow::bail!("`{session_id}` is not bound to `{from_id}`, so it is not its to transfer");
    }
    if to_id == from_id {
        anyhow::bail!("`{session_id}` is already bound to `{to_id}`");
    }
    refuse_if_owner_claimed(state, session_id, from_id)?;
    refuse_if_owner_claimed(state, session_id, to_id)?;
    let owner = state.session_record(to_id)?;
    if let Some(reason) = owner_refusal(state, &session, &owner)? {
        anyhow::bail!(reason);
    }
    let to_project = owner.kind == SessionKind::Project;
    let role = if owner.kind == SessionKind::Console && session.role == Role::Coordinator {
        Role::Coordinator
    } else {
        Role::Worker
    };
    // Decided here, under the claim, and recorded by the binding write itself when there is no
    // process to tell.
    let running = owner_notices::is_running(state, &session);
    let bound = state
        .store
        .set_binding(session_id, Some(from_id), to_id, role, to_project, !running)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the binding changed while it was being transferred — `{session_id}` was bound \
                 elsewhere, it gained sessions of its own, or `{to_id}` can no longer own \
                 sessions; look again before retrying"
            )
        })?;
    state.publish_session(&bound);
    withdraw_pages_if_now_worker(state, &session, &bound);
    Ok((bound, owner, running))
}

/// What [`release_session`] did.
#[derive(Debug)]
pub struct Released {
    /// The session as it is now.
    pub session: Session,
    /// Whether it has a process to be told in.
    pub running: bool,
    /// The owner it was released from.
    pub former_owner: String,
}

/// Unbinds `session_id` from its owner, at the owner's request (`owner_id` names it) or the
/// user's (`None`), and tells every client. The session becomes an ordinary unbound project
/// session: `bound_to` is cleared and it is a coordinator, and what it owns stays bound to it,
/// so a coordinator that reported to a console session keeps its sessions. Its branches stay with
/// it. Any running or no-process state will do.
///
/// Refused, with nothing changed, for the session or its owner under a claim (see
/// [`claim_for_binding`], [`refuse_if_owner_claimed`]), a console session
/// ([`error_code::CONSOLE_SESSION_NEVER_BOUND`]), a session that is not bound
/// ([`error_code::SESSION_NOT_BOUND`]), and, with `owner_id`, one not bound to it
/// ([`error_code::SESSION_NOT_BOUND_TO_OWNER`]); [`error_code::SESSION_BINDING_CHANGED`] when the
/// binding moved between the check and the write. Returns the session as it is now, whether it has
/// a process to be told in, and the owner it was released from, read under the claim; one that has
/// no process is left owed the notice for its next launch, decided while the claim is held and
/// recorded by the binding write itself, with the owner's parting `note` (given only by an owner,
/// already trimmed and not blank). Telling a running one, and the owner, is the caller's.
pub fn release_session(
    state: &Arc<AppState>,
    session_id: &str,
    owner_id: Option<&str>,
    note: Option<&str>,
) -> Result<Released> {
    let _claim = claim_for_binding(state, session_id)?;
    let session = state.session_record(session_id)?;
    if session.kind == SessionKind::Console {
        return Err(CodedError::raised(
            error_code::CONSOLE_SESSION_NEVER_BOUND,
            format!(
                "`{session_id}` is a console session, which is never bound, so it has no owner \
                 to be released from"
            ),
            &[("session", session_id)],
        ));
    }
    let Some(current) = session.bound_to.as_deref() else {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_BOUND,
            format!("`{session_id}` is not bound to any owner, so there is nothing to release"),
            &[("session", session_id)],
        ));
    };
    if let Some(owner) = owner_id.filter(|owner| *owner != current) {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_BOUND_TO_OWNER,
            format!("`{session_id}` is not bound to `{owner}`, so it is not its to release"),
            &[("session", session_id), ("owner", owner)],
        ));
    }
    refuse_if_owner_claimed(state, session_id, current)?;
    let running = owner_notices::is_running(state, &session);
    let released = state
        .store
        .clear_binding(session_id, current, !running, note)?
        .ok_or_else(|| {
            CodedError::raised(
                error_code::SESSION_BINDING_CHANGED,
                format!(
                    "the binding changed while it was being released — `{session_id}` was bound \
                     elsewhere; look again before retrying"
                ),
                &[("session", session_id)],
            )
        })?;
    state.publish_session(&released);
    Ok(Released {
        session: released,
        running,
        former_owner: current.to_string(),
    })
}

/// Releases the session `id` from its binding at the user's request, then tells it and its former
/// owner.
pub(super) async fn release_requested(state: &Arc<AppState>, id: String) -> Result<()> {
    let Released {
        session: released,
        running,
        former_owner,
    } = release_session(state, &id, None, None)?;
    // Nobody asked for the notices: a session that cannot be told now is still released,
    // and each notice is told, and its failure logged, on its own.
    let (told, told_owner) = tokio::join!(
        owner_notices::tell_released(state, &released, running, None),
        owner_notices::tell_former_owner(state, &former_owner, &released),
    );
    if let Err(err) = told {
        tracing::warn!(session = %id, %err, "telling a released session failed");
    }
    if let Err(err) = told_owner {
        tracing::warn!(
            session = %former_owner, %err, "telling the owner of a released session failed"
        );
    }
    Ok(())
}

/// Who `session` reports to, for the launch that tells it so: `None` for a session with no parent,
/// which launches with an unbound project session's role description and tools. A coordinator has
/// a console session for a parent, and its role description names it. An owner that is no longer
/// on record reads as a console session: the text is the generic one, and a report to it fails on
/// its own.
pub(super) fn owner_of(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<Option<session_tools::Owner>> {
    let Some(owner_id) = session.bound_to.as_ref() else {
        return Ok(None);
    };
    Ok(Some(match state.store.get_session(owner_id)? {
        Some(owner) if owner.kind == SessionKind::Project => session_tools::Owner::Project {
            session_id: owner.id,
        },
        _ if session.is_worker() => session_tools::Owner::Console,
        _ => session_tools::Owner::ConsoleOfCoordinator,
    }))
}

/// How many owners up `session`'s bindings go: 0 for an unbound session, 1 for one bound to an
/// owner that is itself unbound, 2 for one bound to a bound coordinator.
pub(super) fn binding_depth(state: &Arc<AppState>, session: &Session) -> Result<usize> {
    let Some(owner) = &session.bound_to else {
        return Ok(0);
    };
    let owner_bound = state
        .store
        .get_session(owner)?
        .is_some_and(|owner| owner.bound_to.is_some());
    Ok(if owner_bound { 2 } else { 1 })
}

/// Refuses, in prose, an operation an owner asked for on a session that is no longer its own. The
/// asker checked before the operation took its claim; this repeats it under the claim, with
/// `session` read there. `None` is the user, who owns nothing to check.
pub(crate) fn refuse_unless_still_askers(session: &Session, asked_by: Option<&str>) -> Result<()> {
    match asked_by {
        Some(asker) if session.bound_to.as_deref() != Some(asker) => anyhow::bail!(
            "{} is no longer yours: it was handed to another owner meanwhile",
            label(session)
        ),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
