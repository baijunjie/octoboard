//! The handoff operation: a session handed off to a new session of another agent, which takes its
//! whole place in Octoboard. The conversation does not move (the agents' records cannot resume one
//! another), so the receiving session reads a package built from the original's records; what
//! moves is what only Octoboard holds: the owner binding with its role, the branches the original
//! is responsible for, and the sessions it owns.
//!
//! **Nothing changes until the receiving session is running.** The session's own refusals come
//! first, then the package is written and the session started with it in its brief; a start that
//! fails, whether opening refuses it or the agent does not come up, removes the package and leaves
//! the original, its branches and its sessions as they were. Only then do the binding, the branches
//! and the owned sessions move, the sessions concerned are told, and the original is archived,
//! last, once nothing is bound to it.
//!
//! **The original's message queue is held from the moment the handoff claims it.** Messages for it
//! are accepted and queued but not written, so none starts a turn on a session whose work is being
//! moved. Once the receiving session has taken its place the queue is forwarded to it, and when the
//! handoff fails before that, the hold is released and the queue is written to the original as usual.
//!
//! The package is named by the receiving session's id, chosen before the session exists, so that
//! deleting the session removes it (`AppState::forget_handoff_package`).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use uuid::Uuid;

use super::open::OpenRequest;
use crate::handoff::status_word;
use crate::messaging::delivery::{self, WhenBlocked};
use crate::messaging::naming::{label, named_session, place_of};
use crate::messaging::owner_notices::{
    duties_under_new_parent, new_owner_notice, parent_changed_notice, INDEPENDENT_COORDINATOR_ROLE,
};
use crate::messaging::texts::{takeover_prompt, NOTICE_PREFIX};
use crate::protocol::{
    error_code, notice_code, AgentId, CodedError, Notice, Role, Session, SessionStatus,
    TerminalAppearance,
};
use crate::session::term;
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::refusals::ensure_unlocked;
use crate::state::{AppState, OutboxHold};

/// What a handoff is asked for. The receiving session is another agent's; `account`, `model` and
/// `effort` are as in [`OpenRequest`].
pub struct Handoff {
    pub session: String,
    pub agent: AgentId,
    pub account: Option<Option<String>>,
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    /// The receiving session's title; `None` takes the name the original is shown under.
    pub title: Option<String>,
    /// What the owner asking says to the receiving session, put in its opening prompt.
    pub note: Option<String>,
    /// The owner that made the call, when one did: it is not sent the message that tells an owner
    /// of the handoff, since it knows, and the result of its call says what moved.
    pub asked_by: Option<String>,
    /// The appearance of the terminal the receiving session will be shown in.
    pub terminal_appearance: Option<TerminalAppearance>,
}

/// Hands `request.session` off and returns what moved: the receiving session, running in the
/// original's place, with the original archived, and what it took over. Refused, with nothing
/// changed, for a session that is missing, archived, locked or not at rest, a receiving agent that
/// is the session's own, and what opening the receiving session would refuse (an agent that is not
/// available or not added, an account of another agent, a disabled account, an account whose plan
/// quota is used up, an account the last quota reading found not logged in, a model or effort the
/// agent does not offer, an owner that can no longer be reported to).
pub async fn handoff_session(state: &Arc<AppState>, request: Handoff) -> Result<Moved> {
    handoff_session_with(state, request, term::launch).await
}

pub(crate) async fn handoff_session_with(
    state: &Arc<AppState>,
    request: Handoff,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<Moved> {
    // Claimed before the record is read, so the status checked is the one the handoff goes on
    // with. Held to the end, so that nothing resumes, switches or deletes the original meanwhile.
    let _claim = state.begin_switch(&request.session)?;
    // Held from the claim on, before anything is read: a refusal just drops it.
    let hold = state.hold_outbox(&request.session);
    let original = state.session_record(&request.session)?;
    super::bindings::refuse_unless_still_askers(&original, request.asked_by.as_deref())?;
    refuse_unless_it_can_be_handed_off(&original, request.agent)?;
    // Its conversation is read from its account's directory, which a change of sharing moves.
    crate::accounts::shared_config::refuse_if_changing(state, original.account_id.as_deref())?;

    let key = Uuid::new_v4().to_string();
    let open = receiving_request(state, &original, &request, Some(key.clone()));

    // Refused here for what opening would refuse, before the package is read and written.
    super::open::check_open(state, &open).await?;

    // The original's brief is not kept anywhere, so the package has none; the first user message in
    // its conversation is the same text.
    let path = {
        let (state, original, key) = (state.clone(), original.clone(), key.clone());
        tokio::task::spawn_blocking(move || crate::handoff::prepare(&state, &original, None, &key))
            .await??
    };
    let open = OpenRequest {
        task: Some(takeover_prompt(&original, &path, request.note.as_deref())),
        ..open
    };
    let receiving = match super::open::open_session_with(state, open, launch).await {
        Ok(receiving) => receiving,
        Err(err) => {
            if let Err(remove_err) = crate::handoff::remove(state, &key) {
                tracing::warn!(%remove_err, "removing the package of a handoff that did not start failed");
            }
            return Err(err);
        }
    };

    let moved = move_place(state, &original, &receiving, &hold).await?;
    tell(state, &original, &moved, request.asked_by.as_deref()).await;
    forward_queued(state, &original.id, &moved.receiving.id).await;
    // What the original's process started is carried over to the receiving session when the
    // archive ends that process, rather than stopped with it.
    let carried = state.live_session(&original.id).map(|live| {
        state
            .process_ledger
            .mark_carry_over(&live.launch_id, &moved.receiving.id);
        live.launch_id.clone()
    });
    // No branch cleanup follows the archive: the branches the original was responsible for have
    // moved to the receiving session, and any that could not move are not the archive's to remove.
    super::lifecycle::archive_session(state, &original.id).map_err(|err| {
        // The original stays as it is, and what it started stays its own.
        if let Some(launch) = &carried {
            state.process_ledger.withdraw_carry_over(launch);
        }
        archive_failed(&receiving.id, &err)
    })?;
    // Dropping the hold forwards a message queued after the forward above to the receiving session,
    // as it does on every path out of here once the place has moved.
    drop(hold);
    Ok(moved)
}

/// The error for the original's archive failing once the work has moved to `receiving`. It nests
/// the archive's own failure as a `launch_failed` does (`reason`, `reason_code` and that failure's
/// params, its own `session` kept), beside the `receiving_session`, so a client words the cause
/// itself (a data folder it could not write, say) instead of showing an internal error.
fn archive_failed(receiving: &str, err: &anyhow::Error) -> anyhow::Error {
    CodedError::nesting(
        error_code::HANDOFF_ARCHIVE_FAILED,
        err,
        &[("receiving_session", receiving)],
        |reason| {
            format!(
                "the work was handed off to `{receiving}`, but archiving the original session \
                 failed: {reason}"
            )
        },
    )
}

/// The error for moving the place (the binding, the owned sessions, the pinned mark) failing once
/// `receiving` is running, which is left running. It nests the failure as [`archive_failed`] does.
fn move_failed(receiving: &str, err: &anyhow::Error) -> anyhow::Error {
    CodedError::nesting(
        error_code::HANDOFF_MOVE_FAILED,
        err,
        &[("receiving_session", receiving)],
        |reason| {
            format!(
                "the work was not fully handed off: the new session `{receiving}` was started and \
                 is left running: {reason}"
            )
        },
    )
}

/// What opening the receiving session of `request` asks for, `session_id` being the id to record it
/// under (`None` for a request only checked, before the user is asked): the original's console,
/// project and origin, the agent, account, model and effort asked for, and the name the original is
/// shown under, task title included, unless the owner asking names it anew, so the work is
/// recognised in the list: the receiving agent starts a new conversation and has no task title of
/// its own yet.
pub(crate) fn receiving_request(
    state: &Arc<AppState>,
    original: &Session,
    request: &Handoff,
    session_id: Option<String>,
) -> OpenRequest {
    OpenRequest {
        console_id: original.console_id.clone(),
        project_id: original.project_id.clone(),
        agent: Some(request.agent),
        account: request.account.clone(),
        model: request.model.clone(),
        effort: request.effort.clone(),
        task: None,
        title: Some(
            request
                .title
                .clone()
                .unwrap_or_else(|| state.display_title(original)),
        ),
        origin: original.origin,
        // A worker's owner is reported to from the first launch. A coordinator under a console
        // session is bound after the launch instead, by `transfer_binding`, which binds it and
        // moves the sessions the original owns to it in one transaction once the handoff has gone
        // through, and it is told its parent with the handoff package.
        bound_to: original
            .is_worker()
            .then(|| original.bound_to.clone())
            .flatten(),
        resume_agent_session_id: None,
        session_id,
        terminal_appearance: request.terminal_appearance,
    }
}

pub(crate) fn refuse_unless_it_can_be_handed_off(original: &Session, agent: AgentId) -> Result<()> {
    let named = label(original);
    match original.status {
        SessionStatus::Archived => {
            return Err(CodedError::raised(
                error_code::SESSION_ARCHIVED,
                format!(
                    "{named} is archived, so there is no work running to hand off; reopen it first"
                ),
                &[("session", &original.id)],
            ));
        }
        SessionStatus::Working | SessionStatus::Starting => {
            return Err(CodedError::raised(
                error_code::SESSION_WORKING,
                format!(
                    "{named} is {}: a session still working goes on writing to its worktree while \
                     the new session starts on it. Hand it off once it is waiting for instructions \
                     or for the user, or has been interrupted",
                    if original.status == SessionStatus::Working {
                        "in the middle of a turn"
                    } else {
                        "still starting"
                    }
                ),
                &[("session", &original.id)],
            ));
        }
        SessionStatus::WaitingUser
        | SessionStatus::Dormant
        | SessionStatus::Idle
        | SessionStatus::Interrupted => {}
    }
    ensure_unlocked([original.clone()])?;
    if agent == original.agent {
        return Err(CodedError::raised(
            error_code::HANDOFF_TO_SAME_AGENT,
            format!(
                "{named} already runs on {}: a handoff goes to another agent. To move it to \
                 another account of the same agent, switch its account instead, which keeps the \
                 whole conversation",
                agent.label()
            ),
            &[("session", &original.id), ("agent", agent.label())],
        ));
    }
    Ok(())
}

/// What moved to the receiving session.
#[derive(Debug)]
pub struct Moved {
    /// The receiving session as it stands after the move.
    pub receiving: Session,
    /// The sessions the original owned, now bound to the receiving session, archived ones included.
    pub owned: Vec<Session>,
    /// The branches now named after the receiving session, as wire paths, or why they could not be
    /// moved.
    pub branches: std::result::Result<Vec<String>, String>,
}

/// Moves the binding, the owned sessions and the branches to `receiving`, and tells every client.
async fn move_place(
    state: &Arc<AppState>,
    original: &Session,
    receiving: &Session,
    hold: &OutboxHold,
) -> Result<Moved> {
    move_place_inner(state, original, receiving, hold)
        .await
        .map_err(|err| move_failed(&receiving.id, &err))
}

async fn move_place_inner(
    state: &Arc<AppState>,
    original: &Session,
    receiving: &Session,
    hold: &OutboxHold,
) -> Result<Moved> {
    let coordinator_of = (original.role == Role::Coordinator)
        .then(|| original.bound_to.clone())
        .flatten();
    let ids = state
        .store
        .transfer_binding(&original.id, &receiving.id, coordinator_of.as_deref())?
        .ok_or_else(|| {
            anyhow!(
                "`{}` could not take the original's place as a coordinator: it was bound meanwhile",
                receiving.id
            )
        })?;
    // The senders' sessions are the receiving session's now: from here on the queue goes to it,
    // whichever way the rest goes.
    hold.forward_to(&receiving.id);
    let mut owned = Vec::new();
    for id in ids {
        owned.extend(state.store.get_session(&id)?);
    }
    // The user's own mark on the session, carried to the session that takes its place.
    if original.pinned {
        state.store.set_session_pinned(&receiving.id, true)?;
    }
    let receiving = state.session_record(&receiving.id)?;
    state.publish_session(&receiving);
    owned
        .iter()
        .for_each(|session| state.publish_session(session));

    let branches = if receiving.project_id.is_some() {
        let (state, original, receiving) = (state.clone(), original.clone(), receiving.clone());
        tokio::task::spawn_blocking(move || {
            crate::branch_origin::move_all_branch_records(&state, &original, &receiving)
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|moved| moved)
        .map_err(|err| format!("{err:#}"))
    } else {
        Ok(Vec::new())
    };
    Ok(Moved {
        receiving,
        owned,
        branches,
    })
}

/// Tells the receiving session what it took over, each moved session that its owner changed, and
/// the owner that its session was handed off, unless that owner is `asked_by`, which made the call.
/// A moved session without a process (an archived or interrupted one) is told when it next starts,
/// from its binding as it stands then; an owner without one learns of the new session from its
/// list.
pub(super) async fn tell(
    state: &Arc<AppState>,
    original: &Session,
    moved: &Moved,
    asked_by: Option<&str>,
) {
    let receiving = &moved.receiving;
    let tool = |agent: AgentId, name: &str| qualified_tool_name(agent, name);
    let owner = receiving
        .bound_to
        .as_deref()
        .and_then(|id| state.store.get_session(id).ok().flatten());

    for owned in &moved.owned {
        // Read again: what `Moved` holds is from the move, and the session may have started or
        // ended since. One with no process is left owed the notice for its next launch.
        let session = match state.store.get_session(&owned.id) {
            Ok(Some(session)) => session,
            Ok(None) => continue,
            Err(err) => {
                tracing::warn!(session = %owned.id, %err, "reading a moved session to tell it failed");
                continue;
            }
        };
        let text = parent_changed_notice(
            &format!(
                ". {from} was handed off to {to}, which took its place.",
                from = named_session(original),
                to = named_session(receiving),
            ),
            session.agent,
            "It",
            &format!(". `{}` no longer drives you; it is archived.", original.id),
        );
        let told = match crate::messaging::owner_notices::running_or_leave(state, &session) {
            Ok(running) => {
                crate::messaging::owner_notices::tell_now_or_leave(
                    state,
                    &session.id,
                    text,
                    running,
                    None,
                )
                .await
            }
            Err(err) => Err(err),
        };
        if let Err(err) = told {
            tracing::warn!(session = %session.id, %err, "telling a session of a handoff failed");
        }
    }

    let to_tell = owner.as_ref().filter(|owner| {
        owner.status != SessionStatus::Archived && asked_by != Some(owner.id.as_str())
    });
    if let Some(owner) = to_tell {
        let text = new_owner_notice(
            owner.agent,
            &format!(
                "{from}, which reported to you, was handed off to a session of {agent}, because \
                 its work moves to another agent. {to}{place} took its place and reports to you \
                 from now on:",
                from = named_session(original),
                agent = receiving.agent.label(),
                to = named_session(receiving),
                place = place_of(state, receiving),
            ),
            " The original is archived. The new session read a package built from the original's \
             conversation and has its branches and its child sessions.",
        );
        // Kept for an owner that is interrupted, as a session's own report to it would be.
        if let Err(err) =
            delivery::write_notice_to_owner_off_runtime(state, owner, original, text).await
        {
            tracing::warn!(session = %owner.id, %err, "telling an owner of a handoff failed");
        }
    }

    let mut held = String::new();
    match &owner {
        Some(owner) if receiving.role == Role::Coordinator => held.push_str(&format!(
            "\n- Your role is coordinator and your parent session is {}: {}",
            named_session(owner),
            duties_under_new_parent(receiving.agent, true),
        )),
        Some(owner) => held.push_str(&format!(
            "\n- Your role is worker and your parent session is {}: take work only from it, or \
             from the user typing in this terminal, and report to it with `{}`. You cannot create \
             or manage child sessions.",
            named_session(owner),
            tool(receiving.agent, "report"),
        )),
        None => held.push_str(&format!(
            "\n- {INDEPENDENT_COORDINATOR_ROLE}: you answer directly to the user and have no \
             parent session."
        )),
    }
    match &moved.branches {
        Ok(branches) if branches.is_empty() => {}
        Ok(branches) => {
            let (noun, them) = crate::branch_origin::transfer::branch_noun_and_pronoun(branches);
            let targets = targets_of(state, receiving, branches).await;
            held.push_str(&format!(
                "\n- You are now the session responsible for the {noun} {}; the original no \
                 longer is, so carry on the work on {them}.",
                crate::branch_origin::transfer::named_branches_with_targets(branches, &targets)
            ));
        }
        Err(reason) => held.push_str(&format!(
            "\n- The branches the original was responsible for could not be moved to you ({reason}); \
             they are still named after it. The package lists them."
        )),
    }
    for session in &moved.owned {
        held.push_str(&format!(
            "\n- Your child session: {} ({}).",
            named_session(session),
            status_word(session.status)
        ));
    }
    let text = format!(
        "{NOTICE_PREFIX} about the handoff: you took over the place of {from}. The \
         original is archived; what it held is yours now:{held}",
        from = named_session(original),
    );
    send(state, &receiving.id, text).await;
}

/// The target branch recorded for each of `branches` (wire paths) that `receiving` now holds, by
/// branch; empty when the session is in no project or they cannot be read.
async fn targets_of(
    state: &Arc<AppState>,
    receiving: &Session,
    branches: &[String],
) -> HashMap<String, String> {
    let Ok(Some(project)) = receiving
        .project_id
        .as_deref()
        .map(|id| state.store.get_project(id))
        .transpose()
        .map(Option::flatten)
    else {
        return HashMap::new();
    };
    let (state, branches) = (state.clone(), branches.to_vec());
    tokio::task::spawn_blocking(move || {
        crate::branch_origin::targets_in_project(&state, &project.path, &branches)
    })
    .await
    .unwrap_or_default()
}

/// Forwards what was queued for the original to the receiving session, in order, through the same
/// delivery any message takes: its queue, then the messages kept in the store for it while it
/// was interrupted that is not in it, which the archive that follows would otherwise drop.
async fn forward_queued(state: &Arc<AppState>, original: &str, receiving: &str) {
    // The queue in order, then what is kept and was not queued: a kept message is in both once the
    // session has been resumed and not yet written to.
    let queued = state.take_queued_with_rows(original);
    let in_queue: Vec<i64> = queued.iter().filter_map(|(_, row)| *row).collect();
    let mut texts: Vec<String> = queued.into_iter().map(|(text, _)| text).collect();
    match state.store.kept_messages(original) {
        Ok(kept) => texts.extend(
            kept.into_iter()
                .filter(|(row, _)| !in_queue.contains(row))
                .map(|(_, text)| text),
        ),
        Err(err) => tracing::warn!(
            session = %original, %err,
            "reading the messages kept for the original session failed"
        ),
    }
    for text in texts {
        if let Err(err) =
            delivery::write_message_off_runtime(state, receiving, text, WhenBlocked::Queue).await
        {
            tracing::warn!(session = %receiving, %err, "forwarding a message queued for the original session failed");
            // The drain that lost it told the user: a lost queue as `queued_messages_dropped`, a
            // message written in part while the draft was stashed as `typing_blocked`.
            if err
                .downcast_ref::<CodedError>()
                .is_some_and(|coded| coded.code == error_code::QUEUED_MESSAGES_LOST)
            {
                continue;
            }
            state.broadcast(
                Notice::new(
                    notice_code::HANDOFF_MESSAGES_DROPPED,
                    format!(
                        "{} could not forward a message queued for the session that was handed off \
                         to the session that took its place: {err:#}",
                        crate::APP_NAME
                    ),
                    &[],
                )
                .about(receiving),
            );
        }
    }
    // Forwarded, or announced as dropped: either way not the original's to keep.
    if let Err(err) = state.store.clear_kept_messages(original) {
        tracing::warn!(
            session = %original, %err,
            "clearing the messages kept for the original session failed"
        );
    }
}

/// Delivers `text` to a session through the ordinary message delivery. A session that cannot take
/// it is told nothing more: the record is already right, and a relaunch tells it.
async fn send(state: &Arc<AppState>, id: &str, text: String) {
    if let Err(err) = delivery::write_message_off_runtime(state, id, text, WhenBlocked::Queue).await
    {
        tracing::warn!(session = %id, %err, "telling a session of a handoff failed");
    }
}

#[cfg(test)]
mod tests;
