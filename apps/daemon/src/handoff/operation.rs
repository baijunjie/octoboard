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

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use uuid::Uuid;

use crate::coordinator::{self, OpenRequest};
use crate::mcp::qualified_tool_name;
use crate::protocol::{error_code, notice_code, Agent, CodedError, Notice, Session, SessionStatus};
use crate::reporting::{self, WhenBlocked};
use crate::sharing::{named_session, one_line, place_of, quote};
use crate::state::{AppState, OutboxHold};
use crate::term;

/// What a handoff is asked for. The receiving session is another agent's; `account`, `model` and
/// `effort` are as in [`OpenRequest`].
pub struct Handoff {
    pub session: String,
    pub agent: Agent,
    pub account: Option<Option<String>>,
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    /// What the owner asking says to the receiving session, put in its opening prompt.
    pub note: Option<String>,
    /// The owner that made the call, when one did: it is not sent the message that tells an owner
    /// of the handoff, since it knows, and the result of its call says what moved.
    pub asked_by: Option<String>,
}

/// Hands `request.session` off and returns what moved: the receiving session, running in the
/// original's place, with the original archived, and what it took over. Refused, with nothing
/// changed, for a session that is missing, archived, locked or not at rest, a receiving agent that
/// is the session's own, and what opening the receiving session would refuse (an agent that is not
/// available or not added, an account of another agent, a model or effort the agent does not offer,
/// an owner that can no longer be reported to).
pub async fn hand_off(state: &Arc<AppState>, request: Handoff) -> Result<Moved> {
    hand_off_with(state, request, term::launch).await
}

pub(crate) async fn hand_off_with(
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
    coordinator::refuse_unless_still_askers(&original, request.asked_by.as_deref())?;
    refuse_unless_it_can_be_handed_off(&original, request.agent)?;

    let key = Uuid::new_v4().to_string();
    let open = OpenRequest {
        console_id: original.console_id.clone(),
        project_id: original.project_id.clone(),
        agent: Some(request.agent),
        account: request.account,
        model: request.model,
        effort: request.effort,
        task: None,
        // The original's title, so the work is recognised in the list.
        title: Some(original.title.clone()),
        origin: original.origin,
        // A bound session's owner is reported to from the first launch. A lead session is bound
        // after the launch instead, for it launches as an unbound one does, with the tools of one.
        bound_to: original
            .has_bound_role()
            .then(|| original.bound_to.clone())
            .flatten(),
        resume_agent_session_id: None,
        session_id: Some(key.clone()),
    };

    // Refused here for what opening would refuse, before the package is read and written.
    coordinator::check_open(state, &open).await?;

    // The original's brief is not kept anywhere, so the package has none; the first user message in
    // its conversation is the same text.
    let path = {
        let (state, original, key) = (state.clone(), original.clone(), key.clone());
        tokio::task::spawn_blocking(move || super::prepare(&state, &original, None, &key)).await??
    };
    let open = OpenRequest {
        task: Some(brief(&original, &path, request.note.as_deref())),
        ..open
    };
    let receiving = match coordinator::open_session_with(state, open, launch).await {
        Ok(receiving) => receiving,
        Err(err) => {
            if let Err(remove_err) = super::remove(state, &key) {
                tracing::warn!(%remove_err, "removing the package of a handoff that did not start failed");
            }
            return Err(err);
        }
    };

    let moved = move_place(state, &original, &receiving, &hold).await?;
    tell(state, &original, &moved, request.asked_by.as_deref()).await;
    forward_queued(state, &original.id, &moved.receiving.id).await;
    // No branch cleanup follows the archive: the branches the original was responsible for have
    // moved to the receiving session, and any that could not move are not the archive's to remove.
    coordinator::archive_session(state, &original.id).map_err(|err| {
        anyhow!(
            "the work was handed off to `{}`, but archiving the original session failed: {err:#}",
            receiving.id
        )
    })?;
    // Dropping the hold forwards a message queued after the forward above to the receiving session,
    // as it does on every path out of here once the place has moved.
    drop(hold);
    Ok(moved)
}

fn refuse_unless_it_can_be_handed_off(original: &Session, agent: Agent) -> Result<()> {
    let named = format!("`{}` (\"{}\")", original.id, one_line(&original.title));
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
        SessionStatus::WaitingUser | SessionStatus::Idle | SessionStatus::Interrupted => {}
    }
    coordinator::ensure_unlocked([original.clone()])?;
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

/// The receiving session's opening prompt: where the package is, to be read before anything else,
/// and the note of the owner that asked for the handoff, when it left one.
fn brief(original: &Session, package: &Path, note: Option<&str>) -> String {
    let note = note.map_or_else(String::new, |note| {
        format!(
            "\n\nThe owner that handed the work off left you this note:\n\n{}",
            quote(note)
        )
    });
    format!(
        "You are taking over the work of the {kind} session `{id}` (\"{title}\"), a {agent} \
         session. Octoboard is handing it off to you because the work moves to another agent. Its \
         conversation cannot be resumed here, so Octoboard built a handoff package from its \
         records:\n\n{path}\n\nRead that file first, all of it, before you do anything else. It \
         holds the whole conversation (every user message and every reply, with tool output left \
         out), its latest todo list, the files its edits changed, the git state of the branches \
         it was responsible for, and the sessions it owned. Work out the goal, what was tried and \
         what comes next from it, and carry on where the original stopped; do not start over.\n\n\
         Once you are running, Octoboard moves the original's place to you: its owner, its \
         branches and its sessions. It sends you a message saying what you now hold, and archives \
         the original; do not message the original.{note}",
        kind = original.role.kind(),
        id = original.id,
        title = one_line(&original.title),
        agent = original.agent.label(),
        path = package.display(),
    )
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
        .with_context(|| {
            format!(
                "the work was not fully handed over: the new session `{}` was started and is left \
                 running",
                receiving.id
            )
        })
}

async fn move_place_inner(
    state: &Arc<AppState>,
    original: &Session,
    receiving: &Session,
    hold: &OutboxHold,
) -> Result<Moved> {
    let lead_of = original.lead.then(|| original.bound_to.clone()).flatten();
    let ids = state
        .store
        .hand_over_binding(&original.id, &receiving.id, lead_of.as_deref())?
        .ok_or_else(|| {
            anyhow!(
                "`{}` could not take the original's place as a lead session: it was bound meanwhile",
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
            crate::branch_origin::hand_over_all(&state, &original, &receiving)
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
/// A session without a process (an archived one, or an interrupted owner) cannot be told: a
/// relaunch tells it who its owner is, and the owner learns of the new session from its list.
async fn tell(state: &Arc<AppState>, original: &Session, moved: &Moved, asked_by: Option<&str>) {
    let receiving = &moved.receiving;
    let tool = |agent: Agent, name: &str| qualified_tool_name(agent, name);
    let owner = receiving
        .bound_to
        .as_deref()
        .and_then(|id| state.store.get_session(id).ok().flatten());

    for session in moved.owned.iter().filter(|s| !s.status.is_dormant()) {
        let text = format!(
            "Message from Octoboard: your owner has changed. {from} was handed off to {to}, which \
             is your owner from now on and took its place: take work only from it, or from the \
             user typing in this terminal, and report to it with `{report}`. This replaces what you \
             were told of your owner when you were started. `{from_id}` no longer drives you; it \
             is archived.",
            from = named_session(original),
            to = named_session(receiving),
            report = tool(session.agent, "report"),
            from_id = original.id,
        );
        send(state, &session.id, text).await;
    }

    let to_tell = owner
        .as_ref()
        .filter(|owner| !owner.status.is_dormant() && asked_by != Some(owner.id.as_str()));
    if let Some(owner) = to_tell {
        let text = format!(
            "Message from Octoboard: {from}, which reported to you, was handed off to a session \
             of {agent}, because its work moves to another agent. {to}{place} took its place and \
             reports to you from now on: you drive it like any session of yours, with \
             `{send_message}`, `{archive_session}` and `{reopen_session}`. The original is \
             archived. The new session read a package built from the original's conversation \
             and has its branches and its sessions.",
            from = named_session(original),
            agent = receiving.agent.label(),
            to = named_session(receiving),
            place = place_of(state, receiving),
            send_message = tool(owner.agent, "send_message"),
            archive_session = tool(owner.agent, "archive_session"),
            reopen_session = tool(owner.agent, "reopen_session"),
        );
        send(state, &owner.id, text).await;
    }

    let mut held = String::new();
    match &owner {
        Some(owner) if receiving.lead => held.push_str(&format!(
            "\n- You are a lead session of {owner}: it is your owner, so take work only from it, \
             or from the user typing in this terminal, and report to it with `{report}` once per \
             round of work. This replaces anything your role description says about being \
             unbound and not calling `{report}`. `done` with no open items archives you, and is \
             refused while a session you started is not archived; `needs_decision` or open items \
             leave you running. You keep the sessions you own and go on driving them with \
             `{new_session}` and the tools that go with it.",
            owner = named_session(owner),
            report = tool(receiving.agent, "report"),
            new_session = tool(receiving.agent, "new_session"),
        )),
        Some(owner) => held.push_str(&format!(
            "\n- Your owner is {}: take work only from it, or from the user typing in this \
             terminal, and report to it with `{}`.",
            named_session(owner),
            tool(receiving.agent, "report"),
        )),
        None => held.push_str("\n- You have no owner: you report to the user."),
    }
    match &moved.branches {
        Ok(branches) if branches.is_empty() => {}
        Ok(branches) => {
            let (noun, them) = crate::session_request::branch_noun_and_pronoun(branches);
            held.push_str(&format!(
                "\n- You are now the session responsible for the {noun} {}; the original no \
                 longer is, so carry on the work on {them}.",
                crate::session_request::named_branches(branches)
            ));
        }
        Err(reason) => held.push_str(&format!(
            "\n- The branches the original was responsible for could not be moved to you ({reason}); \
             they are still named after it. The package lists them."
        )),
    }
    for session in &moved.owned {
        held.push_str(&format!(
            "\n- You own {} ({}).",
            named_session(session),
            word_of(session.status)
        ));
    }
    let text = format!(
        "Message from Octoboard about the handoff: you took over the place of {from}. The \
         original is archived; what it held is yours now:{held}",
        from = named_session(original),
    );
    send(state, &receiving.id, text).await;
}

/// Forwards what was queued for the original to the receiving session, in order, through the same
/// delivery any message takes.
async fn forward_queued(state: &Arc<AppState>, original: &str, receiving: &str) {
    for text in state.take_queued(original) {
        if let Err(err) =
            reporting::write_message_off_runtime(state, receiving, text, WhenBlocked::Queue).await
        {
            tracing::warn!(session = %receiving, %err, "forwarding a message queued for the original session failed");
            // A lost queue was announced as `queued_messages_dropped` by the drain that lost it.
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
}

fn word_of(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Starting => "starting",
        SessionStatus::Working => "working",
        SessionStatus::WaitingUser => "waiting for the user",
        SessionStatus::Idle => "awaiting instructions",
        SessionStatus::Interrupted => "interrupted",
        SessionStatus::Archived => "archived",
    }
}

/// Delivers `text` to a session through the ordinary message delivery. A session that cannot take
/// it is told nothing more: the record is already right, and a relaunch tells it.
async fn send(state: &Arc<AppState>, id: &str, text: String) {
    if let Err(err) =
        reporting::write_message_off_runtime(state, id, text, WhenBlocked::Queue).await
    {
        tracing::warn!(session = %id, %err, "telling a session of a handoff failed");
    }
}

#[cfg(test)]
mod tests;
