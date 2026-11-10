//! Writing a message into a session: the gate on what the session can take now, the queue that
//! keeps messages in order, and what became of each, for every message Octoboard or a session sends
//! into a session. Writing to an owner about one of its sessions goes through here too: it waits
//! out a claim on the owner, follows a session that was handed to another owner meanwhile, and has
//! the message kept for an owner that is interrupted, to be written into it once it is resumed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::messaging::idle_children::idle_after_report;
use crate::messaging::outbox::{Drain, Ticket};
use crate::messaging::texts::{
    idle_notice_session, is_idle_notice, is_waiting_notice, waiting_notice_session,
};
use crate::messaging::write_queue::FirstMessage;
use crate::protocol::{error_code, AgentId, CodedError, Session, SessionStatus};
use crate::session::term;
use crate::session_tools::catalogue;
use crate::state::AppState;
use crate::time;
use crate::time::now_millis;

/// What became of a message handed to [`write_message`], or to a delivery that keeps a message for
/// a session with no process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Written,
    /// Accepted and queued. It goes in as soon as the session can take one, so the sender must not
    /// send it again.
    Queued,
    /// Written, but the session's agent did not take it as a message to its conversation: the text
    /// is waiting in its input box for the user, or went to a view of its background agents
    /// instead. The user has been told. The session has not read it, and it must not be sent
    /// again.
    Unsent,
    /// Kept in the store for a session that is interrupted, and written into it once it is
    /// resumed (see [`AppState::keep_message_if_not_running`]). Never what [`write_message`]
    /// answers: keeping is for the deliveries that have a reason to wait for a resume, shared
    /// information and what an owner is sent about its sessions.
    Kept,
}

/// Who sent what a [`Delivery::note`] is about, which decides whether it warns against sending it
/// again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SentBy {
    /// The session the note is for: what is queued or kept arrives by itself, and sending it
    /// again would deliver it twice.
    Reader,
    /// Octoboard, on the reader's behalf, so there is nothing for the reader to send again.
    Octoboard,
}

impl Delivery {
    /// What a session is told became of `what`, delivered to `to`: `what` begins a sentence ("The
    /// message", "Your report") and `to` names the session it went to within one ("this
    /// session", "its owner"). One wording per outcome wherever a session is told what became of
    /// a delivery, so that it reads every one the same way and knows when not to send again.
    pub(crate) fn note(self, what: &str, to: &str, sent_by: SentBy) -> String {
        let (so, and) = match sent_by {
            SentBy::Reader => (", so do not send it again", "; do not send it again"),
            SentBy::Octoboard => ("", ""),
        };
        match self {
            Delivery::Written => format!("{what} was delivered to {to}."),
            Delivery::Queued => format!(
                "{what} is queued: {to} cannot take a message right now. It will be delivered as \
                 soon as {to} can take one{so}."
            ),
            Delivery::Unsent => format!(
                "{what} was written into the input of {to}, but {to}'s agent did not send it to \
                 the conversation (it is in the input box, or went to a view of its background \
                 agents), so {to} has not read it. The user has been told to check {to}{and}."
            ),
            Delivery::Kept => format!(
                "{what} is kept for {to}, which is interrupted with no process running: it will be \
                 delivered once {to} is resumed{so}."
            ),
        }
    }
}

/// What a sender is told when a drain it triggered ran into a message the agent only partly took.
///
/// It does not say whose message was the one that failed, because the drain writes whatever is queued
/// and that may have been someone else's — only that something queued for this session was dropped,
/// and that the session wants looking at before anything else is sent. "Do not resend" would be
/// wrong half the time, and so would "resend".
fn lost_message() -> String {
    format!(
        "A message queued for this session could not be written in full, so it and everything \
         queued behind it were dropped: {}",
        term::FRAGMENT_HAZARD
    )
}

/// The param `queued_messages_lost` carries when its message was written in part while the
/// session's draft was stashed ([`Drain::Fragmented`]): only that message was lost.
pub(crate) const HELD_FRAGMENT_PARAM: (&str, &str) = ("fragment", "held");

/// Whether `error` is the loss of a message written in part while the session's draft was stashed,
/// the one message it lost ([`HELD_FRAGMENT_PARAM`]).
pub(crate) fn is_held_fragment(error: &CodedError) -> bool {
    error.code == error_code::QUEUED_MESSAGES_LOST
        && error
            .params
            .get(HELD_FRAGMENT_PARAM.0)
            .is_some_and(|value| value == HELD_FRAGMENT_PARAM.1)
}

/// What a sender is told when its own message was the one written in part while the session's
/// draft was stashed ([`Drain::Fragmented`]): only it is gone, and what is queued behind it stays.
fn fragmented_message() -> String {
    format!(
        "The message could not be written in full, so it was dropped; messages queued behind it \
         stay queued: {}",
        term::HELD_FRAGMENT
    )
}

/// What to do when the session cannot be written to right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhenBlocked {
    /// Refuse, so the sender can be told why. The right answer for the user: they are the one who
    /// has to answer the prompt that is blocking it.
    Refuse,
    /// Queue it. The right answer for an owner and for a report, neither of which has anyone to
    /// tell and neither of which may be dropped.
    Queue,
}

/// Writes a message into a running session, or queues it until the session can take one.
///
/// **The gate is the session's hook-reported state, never the terminal's modes.** No agent signals
/// its modal state through terminal modes, and a write while a modal dialog is up has its trailing Enter
/// confirm whatever option is highlighted — at Claude Code's trust dialog that exits the session, at
/// Grok's approval modal it selects always-approve. Working and idle are both safe: every agent
/// queues a message written mid-turn and consumes it when the turn ends. The one thing that comes
/// before any hook is each agent's own trust screen, which the trailing Enter would answer — Codex
/// and Grok Build would take it as trusting the folder — so a session whose trust watch still holds
/// writes (`TrustState::holds_writes`) is blocked as well. Claude Code's hold outlasts its first hook
/// until it has also turned bracketed paste on, as its early-input capture would swallow a write
/// before then.
///
/// A session a sharing change is restarting takes none of this: its queue is held from the claim
/// until the resume, so a message for it is queued, whatever the process's state, and `when_blocked`
/// does not apply.
///
/// Once a planned stop has raised its handover barrier, a message is queued and not written, and
/// carried over to the next daemon with the rest of the queue (`super::restart_outbox`); once the
/// queues have been saved, it is refused with `daemon_shutting_down`, for the sender to send again.
///
/// **Blocks** on the PTY write and, for an idle Claude Code session, on the few seconds' wait for
/// its agent to send it (twice, for a message a dialog discarded and that is written again);
/// callers on the runtime are responsible for keeping it off a worker.
pub fn write_message(
    state: &Arc<AppState>,
    id: &str,
    text: &str,
    when_blocked: WhenBlocked,
) -> Result<Delivery> {
    write_message_inner(state, id, text, when_blocked, None)
}

/// The refusal of a message once a planned stop has saved the queues for the next daemon
/// (`super::restart_outbox`): nothing would write it, nor carry it over, so it is not accepted, and
/// the sender may send it again once the next daemon is up.
fn outbox_saved() -> anyhow::Error {
    CodedError::raised(
        error_code::DAEMON_SHUTTING_DOWN,
        "the daemon is stopping and has saved the queued messages for the next daemon, so this \
         message is not accepted; send it again once the daemon is back",
        &[],
    )
}

/// The refusal of a message with nothing left in it once its control and invisible characters
/// are removed.
pub(crate) fn blank_text() -> anyhow::Error {
    CodedError::raised(
        error_code::FIELD_REQUIRED,
        "the message is empty once its control and invisible characters are removed",
        &[("field", "text")],
    )
}

fn write_message_inner(
    state: &Arc<AppState>,
    id: &str,
    text: &str,
    when_blocked: WhenBlocked,
    mut first: Option<&mut FirstMessage>,
) -> Result<Delivery> {
    // Written, it would be an empty prompt the agent does not send, and nobody would learn why.
    if term::is_blank_once_stripped(text) {
        return Err(blank_text());
    }
    let session = state.session_record(id)?;
    // Held from the claim on until the resume, so queued rather than refused whatever state its
    // process is in meanwhile: the account's hold keeps the queue and the resumed process gets it,
    // or its drop reports the queue as dropped when the session is not relaunched.
    let restarting = match first.as_mut() {
        Some(first) => state
            .queue_if_restarting_with(id, || first.enqueue(text))
            .transpose()?
            .is_some(),
        None => match state.queue_if_restarting(id, text) {
            Some(Some(_)) => true,
            Some(None) => return Err(outbox_saved()),
            None => false,
        },
    };
    if restarting {
        return Ok(Delivery::Queued);
    }
    // Nothing to write to and nothing to wait for: a resume starts the agent at its prompt rather
    // than replaying a queue, so queuing here would lose the message silently.
    if session.status.has_no_process() || state.live_session(id).is_none() {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_RUNNING,
            "this session is not running",
            &[("session", id)],
        ));
    }
    // The agent's own status, not the record's: a session request's hand is answered in a
    // dialog and leaves the agent's prompt as free to take a message as it was.
    let agent_status = state.agent_status(&session);
    let writable = matches!(agent_status, SessionStatus::Working | SessionStatus::Idle);
    if !writable && when_blocked == WhenBlocked::Refuse {
        return Err(CodedError::raised(
            error_code::SESSION_WAITING_FOR_USER,
            "this session is waiting for you — answer it in the terminal first",
            &[("session", id)],
        ));
    }
    let at_trust_screen = state
        .live_session(id)
        .is_some_and(|live| live.trust.holds_writes());
    if at_trust_screen && when_blocked == WhenBlocked::Refuse {
        return Err(CodedError::raised(
            error_code::SESSION_TRUST_PENDING,
            "this session's agent is showing, or may be about to show, its folder-trust \
             confirmation; send the message once that has been answered",
            &[("session", id)],
        ));
    }

    // Queued even when the session looks ready, so messages cannot overtake one another.
    let ticket = match first {
        Some(first) => {
            state.cancel_recoveries_silently(id);
            let ticket = first.enqueue(text)?;
            state.note_activity(id);
            ticket
        }
        None => state
            .queue_message_unless_sealed(id, text)
            .ok_or_else(outbox_saved)?,
    };
    let drain = state.flush_outbox(id, agent_status);
    // Told first, as the flush may have stopped before it told the session; then taken by ticket,
    // whichever drain met it: one running on another thread may have.
    state.announce_refusals(id, session.agent);
    if let Some(reason) = state.outbox.take_refusal(ticket) {
        return Err(refused_error(session.agent, &reason));
    }
    delivery_of(drain, ticket, session.agent)
}

/// The error a sender gets for a message its session's agent refused outright, for `reason`.
fn refused_error(agent: AgentId, reason: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::MESSAGE_REFUSED,
        format!(
            "{} refused the message, so it was not sent: {reason}",
            agent.label()
        ),
        &[("agent", agent.label()), ("detail", reason)],
    )
}

/// Delivers a first message through the same checks as every automatic message, preserving the
/// place it reserved before the session was published.
pub(crate) async fn write_first_message_off_runtime(
    state: &Arc<AppState>,
    mut first: FirstMessage,
    text: String,
) -> Result<Delivery> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let id = first.id().to_string();
        write_message_inner(&state, &id, &text, WhenBlocked::Queue, Some(&mut first))
    })
    .await?
}

/// Whether `error` says a message may have been taken by its agent's own queue without that being
/// confirmed: possibly delivered, so neither to be sent again nor taken as undelivered.
pub(crate) fn acceptance_uncertain(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<CodedError>()
        .is_some_and(|error| error.code == error_code::MESSAGE_ACCEPTANCE_UNCONFIRMED)
}

/// What a drain's outcome means for the message with `ticket`, queued before the drain began, in a
/// session of `agent`.
fn delivery_of(drain: Drain, ticket: Ticket, agent: AgentId) -> Result<Delivery> {
    match drain {
        Drain::Clear => Ok(Delivery::Written),
        Drain::Pending => Ok(Delivery::Queued),
        Drain::Unsent(unsent) if unsent == ticket => Ok(Delivery::Unsent),
        // Tickets follow the queue's order: an earlier message left unsent holds this one back, a
        // later one was written after this one went in.
        Drain::Unsent(unsent) if unsent < ticket => Ok(Delivery::Queued),
        Drain::Unsent(_) => Ok(Delivery::Written),
        // The discarded message is back at the front of the queue, so this message is queued
        // whether it is that one or waits behind it; one queued before it was written first.
        Drain::Discarded(discarded) if discarded <= ticket => Ok(Delivery::Queued),
        Drain::Discarded(_) => Ok(Delivery::Written),
        // Taken by a view of background agents, so the conversation has not read it, which is what
        // `Unsent` says of a message; a later one waits for the view to be left, an earlier one
        // was written before it.
        Drain::Diverted(diverted) if diverted == ticket => Ok(Delivery::Unsent),
        Drain::Diverted(diverted) if diverted < ticket => Ok(Delivery::Queued),
        Drain::Diverted(_) => Ok(Delivery::Written),
        Drain::Uncertain(uncertain) if uncertain < ticket => Ok(Delivery::Queued),
        Drain::Uncertain(uncertain) if uncertain > ticket => Ok(Delivery::Written),
        Drain::Uncertain(_) => Err(CodedError::raised(
            error_code::MESSAGE_ACCEPTANCE_UNCONFIRMED,
            format!(
                "{} did not confirm that it accepted the message. It may already be queued or \
                 running, and it will not be sent again: do not send it again before checking \
                 the conversation.",
                agent.label()
            ),
            &[("agent", agent.label())],
        )),
        Drain::Lost => Err(CodedError::raised(
            error_code::QUEUED_MESSAGES_LOST,
            lost_message(),
            &[],
        )),
        // Only the message written in part is lost; one queued behind it waits for the session's
        // hold to end, one queued before it was written first.
        Drain::Fragmented(fragmented) if fragmented == ticket => Err(CodedError::raised(
            error_code::QUEUED_MESSAGES_LOST,
            fragmented_message(),
            &[HELD_FRAGMENT_PARAM],
        )),
        Drain::Fragmented(fragmented) if fragmented < ticket => Ok(Delivery::Queued),
        Drain::Fragmented(_) => Ok(Delivery::Written),
    }
}

/// [`write_message`] for a caller on the runtime. The write blocks on the PTY for as long as the
/// child is not draining, and on the wait for an idle Claude Code session to send it, so it goes to
/// a blocking thread rather than a runtime worker.
pub async fn write_message_off_runtime(
    state: &Arc<AppState>,
    id: &str,
    text: String,
    when_blocked: WhenBlocked,
) -> Result<Delivery> {
    let state = state.clone();
    let id = id.to_string();
    tokio::task::spawn_blocking(move || write_message(&state, &id, &text, when_blocked)).await?
}

/// Whether a delivery to an owner was refused for the owner being archived.
pub(crate) fn is_archived_refusal(delivered: &Result<Delivery>) -> bool {
    delivered.as_ref().err().is_some_and(|err| {
        err.downcast_ref::<CodedError>()
            .is_some_and(|coded| coded.code == error_code::SESSION_ARCHIVED)
    })
}

/// The owner that the session `session_id` is bound to now, when it is not `previous` and is not
/// archived: the session was handed to another owner while a write waited for `previous`.
pub(crate) fn rebound_owner(
    state: &Arc<AppState>,
    session_id: &str,
    previous: &str,
) -> Result<Option<Session>> {
    let Some(now) = state.session_record(session_id)?.bound_to else {
        return Ok(None);
    };
    if now == previous {
        return Ok(None);
    }
    Ok(state
        .store
        .get_session(&now)?
        .filter(|owner| owner.status != SessionStatus::Archived))
}

/// [`write_to_owner`] for the notice `notice` about `about`, kept with the line that says it was.
///
/// **Blocks** like [`write_message`].
pub(crate) fn write_notice_to_owner(
    state: &Arc<AppState>,
    owner: &Session,
    about: &Session,
    notice: &str,
) -> Result<Delivery> {
    write_to_owner(state, owner, about, notice, |kept| {
        format!("{notice}\n\n{kept}")
    })
}

/// [`write_notice_to_owner`] for a caller on the runtime, whose notice is not one of the
/// `tell_owner_*` ones: a handoff's or a transfer's, about the session `about` that moved.
pub(crate) async fn write_notice_to_owner_off_runtime(
    state: &Arc<AppState>,
    owner: &Session,
    about: &Session,
    notice: String,
) -> Result<Delivery> {
    let (state, owner, about) = (state.clone(), owner.clone(), about.clone());
    tokio::task::spawn_blocking(move || write_notice_to_owner(&state, &owner, &about, &notice))
        .await?
}

/// How long [`write_to_owner`] waits for a claim on the owner to end.
#[cfg_attr(test, allow(dead_code))]
const OWNER_CLAIM_WAIT: Duration = Duration::from_secs(15);

/// Read on the thread waiting out the claim, so a timeout test's override is local to it.
fn owner_claim_wait() -> Duration {
    #[cfg(test)]
    {
        OWNER_CLAIM_WAIT_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    OWNER_CLAIM_WAIT
}

#[cfg(test)]
thread_local! {
    pub(super) static OWNER_CLAIM_WAIT_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
    static OWNER_CLAIM_WAITING: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> = const { std::cell::RefCell::new(None) };
}

/// Runs `change` only after `deliver` has found the owner's claim and begun waiting for it.
#[cfg(test)]
pub(super) fn when_owner_claim_waits<T: Send>(
    claim: crate::state::LaunchClaim,
    change: impl FnOnce(),
    deliver: impl FnOnce() -> T + Send,
) -> T {
    let (waiting, observed) = std::sync::mpsc::channel();
    std::thread::scope(|threads| {
        let delivery = threads.spawn(move || {
            OWNER_CLAIM_WAITING.with(|slot| slot.replace(Some(waiting)));
            let result = deliver();
            OWNER_CLAIM_WAITING.with(|slot| slot.replace(None));
            result
        });
        observed
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("the delivery began waiting for the owner's claim");
        change();
        drop(claim);
        delivery.join().unwrap()
    })
}

/// Waits up to [`OWNER_CLAIM_WAIT`] for the claim on the session `id` to be released.
///
/// **Blocks** the thread, which is a blocking one.
fn wait_out_claim(state: &Arc<AppState>, id: &str) {
    let deadline = Instant::now() + owner_claim_wait();
    #[cfg(test)]
    if state.is_launching(id) {
        OWNER_CLAIM_WAITING.with(|slot| {
            if let Some(waiting) = slot.borrow_mut().take() {
                let _ = waiting.send(());
            }
        });
    }
    while state.is_launching(id) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The sentence that dates a message kept for a session while it was interrupted, as of now: every
/// kept message carries it, so the session reading it after its resume knows when it was written.
pub(crate) fn kept_dating() -> String {
    format!(
        "Kept for you while you were interrupted; it dates from {}.",
        time::readable_utc_minute(now_millis())
    )
}

/// Writes `text`, a report or a notice about the session `about`, into `owner`, queued while it
/// cannot take a message; an owner that is interrupted has it kept instead, built by `kept` around
/// the line that says so, and written into it once it is resumed, in the order it came with the
/// rest of what was kept for it, shared information included. An owner that is archived is
/// refused.
///
/// An owner under a claim (a resume, an account switch, a handoff, a transfer of branches or a
/// change of owner) has no process to write into and takes nothing kept either, and what is sent
/// to it is an event that will not come again, so the write waits out the claim, with a bound
/// ([`OWNER_CLAIM_WAIT`]), and reads the owner again.
///
/// What is kept is read only after the owner is back, when what it says of the session may no
/// longer hold, so the line dates it and has the owner look at the session before acting on it.
/// A notice that the session waits for the user, or that it stayed idle after its last report,
/// goes out of date as soon as anything later about the session is kept — another wait, a report,
/// a notice of its end — so that one drops it rather than the owner reading a state that is long
/// over; the rest are events, each kept.
///
/// **Blocks** like [`write_message`], and on the claim, so it is for a blocking thread.
pub(crate) fn write_to_owner(
    state: &Arc<AppState>,
    owner: &Session,
    about: &Session,
    text: &str,
    kept: impl Fn(&str) -> String,
) -> Result<Delivery> {
    let kept_text = || {
        let line = format!(
            "{} What it says of the session may no longer hold, so check the session with `{}` \
             before acting on it.",
            kept_dating(),
            catalogue::qualified_tool_name(owner.agent, "get_session"),
        );
        kept(&line)
    };
    write_or_keep(
        state,
        &owner.id,
        text,
        "this session is archived, so it takes nothing until it is reopened",
        kept_text,
        Some(about),
    )
}

/// Writes `text` into the session `id`, queued while it cannot take a message: an archived session
/// is refused with `archived` as the reason, and an interrupted one has `kept_text()` kept for it
/// instead, to be written into it once it is resumed ([`Delivery::Kept`]).
///
/// `about` is the session an owner is told about, for the write into an owner: such a write waits
/// out a claim on the owner (see [`write_to_owner`]), and what it keeps drops the notices that
/// `about` waits for the user, or stayed idle after its last report, which were kept before it. A
/// write with none does neither.
///
/// **Blocks** like [`write_message`], and on the claim when `about` is given.
pub(crate) fn write_or_keep(
    state: &Arc<AppState>,
    id: &str,
    text: &str,
    archived: &str,
    kept_text: impl Fn() -> String,
    about: Option<&Session>,
) -> Result<Delivery> {
    let mut waited = false;
    loop {
        // Read again each time round: the session may have been archived, or resumed, since.
        let status = state.session_record(id)?.status;
        if status == SessionStatus::Archived {
            return Err(CodedError::raised(
                error_code::SESSION_ARCHIVED,
                archived,
                &[("session", id)],
            ));
        }
        if status == SessionStatus::Interrupted {
            if let Some(row) = state.keep_message_if_not_running(id, &kept_text())? {
                if let Some(about) = about {
                    drop_kept_superseded(state, id, about, row);
                }
                return Ok(Delivery::Kept);
            }
        }
        if about.is_some() && !waited && state.live_session(id).is_none() && state.is_launching(id)
        {
            wait_out_claim(state, id);
            waited = true;
            continue;
        }
        return write_message(state, id, text, WhenBlocked::Queue);
    }
}

/// Drops the notices that `about` waits for the user, or stayed idle after its last report, kept
/// for `owner_id` before the row `row`.
fn drop_kept_superseded(state: &Arc<AppState>, owner_id: &str, about: &Session, row: i64) {
    let superseded: Vec<i64> = match state.store.kept_messages(owner_id) {
        Ok(kept) => kept
            .into_iter()
            .filter(|(kept_row, text)| {
                *kept_row < row && (is_waiting_notice(text, about) || is_idle_notice(text, about))
            })
            .map(|(kept_row, _)| kept_row)
            .collect(),
        Err(err) => {
            tracing::warn!(owner = %owner_id, %err, "reading what is kept for an owner failed");
            return;
        }
    };
    if superseded.is_empty() {
        return;
    }
    if let Err(err) = state.store.delete_kept_messages(&superseded) {
        tracing::warn!(owner = %owner_id, %err, "dropping a superseded kept notice failed");
    }
}

/// What is kept for `owner_id` ([`crate::store::Store::kept_messages`]) without the notices that
/// a session waits for the user, or stayed idle after its last report, when it no longer does:
/// nothing superseded them, but the owner would read a wait, or an idleness, that is over. Those
/// rows are deleted. For a resume to queue.
pub(crate) fn current_kept_messages(
    state: &Arc<AppState>,
    owner_id: &str,
) -> Result<Vec<(i64, String)>> {
    let mut kept = state.store.kept_messages(owner_id)?;
    let stale: Vec<i64> = kept
        .iter()
        .filter(|(_, text)| {
            waiting_notice_session(text).is_some_and(|id| {
                !matches!(
                    state.store.get_session(id),
                    Ok(Some(session)) if state.shown_status(&session) == SessionStatus::WaitingUser
                )
            }) || idle_notice_session(text).is_some_and(|id| {
                !matches!(
                    state.store.get_session(id),
                    Ok(Some(session)) if idle_after_report(state, &session).is_some()
                )
            })
        })
        .map(|(row, _)| *row)
        .collect();
    if stale.is_empty() {
        return Ok(kept);
    }
    if let Err(err) = state.store.delete_kept_messages(&stale) {
        tracing::warn!(owner = %owner_id, %err, "dropping a stale kept wait failed");
    }
    kept.retain(|(row, _)| !stale.contains(row));
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::owner_team::{interrupted_hub_state, kept_texts, team_state};

    /// Only the message written in part while the draft was stashed is lost to its sender: one
    /// queued behind it waits for the session's hold to end, one queued before it was written.
    #[test]
    fn a_held_fragment_is_lost_only_to_its_own_sender() {
        assert_eq!(
            delivery_of(Drain::Fragmented(5), 6, AgentId::CODEX).unwrap(),
            Delivery::Queued
        );
        assert_eq!(
            delivery_of(Drain::Fragmented(5), 4, AgentId::CODEX).unwrap(),
            Delivery::Written
        );
        let lost = delivery_of(Drain::Fragmented(5), 5, AgentId::CODEX).unwrap_err();
        assert!(lost
            .downcast_ref::<CodedError>()
            .is_some_and(is_held_fragment));
    }

    /// A message the agent's own queue may have taken is a coded error naming the agent, which
    /// callers tell from an undelivered one; one queued behind it waits, one before it went in.
    #[test]
    fn an_uncertain_acceptance_is_coded_and_told_apart() {
        assert_eq!(
            delivery_of(Drain::Uncertain(5), 6, AgentId::CODEX).unwrap(),
            Delivery::Queued
        );
        assert_eq!(
            delivery_of(Drain::Uncertain(5), 4, AgentId::CODEX).unwrap(),
            Delivery::Written
        );
        let uncertain = delivery_of(Drain::Uncertain(5), 5, AgentId::CODEX).unwrap_err();
        assert!(acceptance_uncertain(&uncertain));
        let coded = uncertain.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::MESSAGE_ACCEPTANCE_UNCONFIRMED);
        assert_eq!(coded.params["agent"], "Codex");
        let lost = delivery_of(Drain::Lost, 5, AgentId::CODEX).unwrap_err();
        assert!(!acceptance_uncertain(&lost));
    }

    /// A message its agent refused outright is a coded error carrying the agent's reason.
    #[test]
    fn a_refused_message_is_a_coded_error_with_the_reason() {
        let error = refused_error(AgentId::CODEX, "input too long");
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::MESSAGE_REFUSED);
        assert_eq!(coded.params["agent"], "Codex");
        assert_eq!(coded.params["detail"], "input too long");
        assert!(!acceptance_uncertain(&error));
    }

    /// A message that would be an empty prompt once stripped is refused, not written.
    #[test]
    fn a_message_blank_once_stripped_is_refused() {
        let (state, _dir, live) = team_state("reporting-blank", &[]);
        let err = write_message(&state, "hub", "\u{2068}\u{2069} \u{1b}", WhenBlocked::Queue)
            .unwrap_err();
        let coded = err.downcast_ref::<CodedError>().expect("a coded refusal");
        assert_eq!(coded.code, error_code::FIELD_REQUIRED);
        assert_eq!(live.input_writes(), 0);
    }

    /// An owner archived after the caller read it is refused as archived, not as starting.
    #[test]
    fn an_owner_archived_since_it_was_read_is_refused_as_archived() {
        let (state, _dir) = interrupted_hub_state("reporting-owner-archived-since");
        let stale = state.session_record("hub").unwrap();
        let mut archived = stale.clone();
        archived.status = SessionStatus::Archived;
        state.store.update_session(&archived).unwrap();
        let worker = state.session_record("worker").unwrap();

        let err = write_to_owner(&state, &stale, &worker, "text", |kept| kept.to_string())
            .expect_err("refused");

        let coded = err.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::SESSION_ARCHIVED, "{err}");
        assert!(kept_texts(&state).is_empty());
    }
}
