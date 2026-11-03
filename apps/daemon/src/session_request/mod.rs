//! A session's request for a session the user has to approve: the `request_session` call, which
//! waits while the user is asked, the requests the daemon holds meanwhile, and what the user's
//! answer does. `request_resume_session` is the same request with another thing to approve: the
//! [`Action`] a request carries says whether an approval starts a new session, brings back the
//! session that holds a conversation, or opens a session on an agent's own record of one, and
//! everything else (the queue, the hand, the waiver, the withdrawal, the answer) is shared. So are
//! an owner's `switch_session_account` and `handoff_session` calls while the user has not chosen
//! to have them run without asking: their approval is a form, whose values the user may change
//! (`Edits`), and what it runs is the tool's own operation, handed over as a [`Perform`].
//!
//! **The confirmation is the enforcement.** An approved request puts a session with no owner in
//! any project, or a console session that dispatches into every project of its console, or a
//! conversation the user did not name back into a session, on the word of a model, so nothing
//! here acts on a call by itself: the request is broadcast for a dialog, the call waits, and only
//! an approval starts anything. A rule in a prompt could be ignored by the model; this cannot. The
//! one exception is the user's own standing approval, the setting described next, which only the
//! user's client can change.
//!
//! **The user can waive the dialog.** While the application setting for the kind of request is on
//! (`auto_approve_session_requests` for `request_session`, `auto_approve_resume_requests` for
//! `request_resume_session`, independent of each other), a request that passed every pre-check
//! is approved at once, through the same [`carry_out`] an approval from the dialog takes: nothing
//! is broadcast, no hand goes up, and the session is started or brought back, the branches handed
//! over and the outcome told exactly as for an approval. That runs in a task of its own, so a call
//! cancelled meanwhile does not interrupt it. The setting is read when the call is made, so a
//! request already waiting stays in its dialog. The settings of the switches
//! (`auto_approve_account_switches`) and of the handoffs (`auto_approve_handoffs`) are read by the
//! tools themselves, which run the operation outright rather than ask (`change::run_unasked`).
//!
//! **A request lives until it is answered, times out or is withdrawn.** It is held in memory only
//! ([`SessionRequests`] on `AppState`) and sent again to a client after each `snapshot`, like a
//! trust prompt. Nothing needs persisting: every agent process ends with the daemon, and the call
//! is served inside it, so a restart leaves no caller waiting. A request is withdrawn on the
//! signals the daemon sees for every agent alike — the caller's process ends
//! ([`withdraw_for_session`], from the live-session bookkeeping), or the call's connection to the
//! daemon is dropped, which drops the waiting call ([`wait_for_answer`]) and with it its guard. An
//! agent that cancels the call over MCP reaches the second: its stdio server closes that
//! connection (`mcp::stdio`). A session that asks again of the same kind (and, for a switch or a
//! handoff, about the same session) has stopped waiting for its earlier call, so its new request
//! takes the earlier one's place ([`Request::replaces`]).
//!
//! **An agent can also give up on the call by its own time limit without any signal reaching
//! Octoboard,** and nothing here tries to tell when. So an approval is always carried out the same
//! way, and its outcome is written into the caller's session as a message as well as returned as
//! the call's result: when the result was lost, the message is how the caller learns what became
//! of its request. The turn that message starts is credited as reported, since it only reads an
//! outcome the caller already has. The turn Claude Code starts on the result of a backgrounded
//! `request_session` call is not: there the caller's round really continues, and its end is a
//! report its owner is owed. The same holds for `request_resume_session`.

mod carry_out;
mod change;
mod outcome;
mod registry;
mod request;
#[cfg(test)]
mod tests;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::branch_origin::handover::Handover;
use crate::coordinator::{self, open::OpenRequest};
use crate::messaging::delivery::{self, WhenBlocked};
use crate::messaging::texts;
use crate::protocol::{error_code, Agent, CodedError, Event, Session, SessionRequestEnding};
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::AppState;

use carry_out::carry_out;
pub(crate) use change::{
    ask_to_change, run_to_the_end, run_unasked, Change, Edits, Perform, Performed,
};
pub(crate) use outcome::branch_outcome;
pub use registry::SessionRequests;
use registry::Waiting;
use request::Request;
pub(crate) use request::{Action, Ask};

/// How long a call waits for the user's answer. Octoboard's own value, for every agent alike, and
/// kept below `session_tools::CALL_TIMEOUT` with room left for an approval given at the last
/// moment to be carried out — launching the session included — before that forwarding gives up.
/// An agent's own limit is not obeyed; the launch sets it above that forwarding limit instead,
/// whichever side the agent's own default falls on (apps/daemon/PROTOCOL.md's
/// "`POST /mcp/:token` — Octoboard's MCP tools" section has the ordering).
pub const ANSWER_TIME_LIMIT: Duration = Duration::from_secs(8 * 60);

const _: () = assert!(ANSWER_TIME_LIMIT.as_secs() < crate::session_tools::CALL_TIMEOUT.as_secs());

/// The requests to put to a client that has just been sent a snapshot, oldest first. A request is
/// broadcast once, so a client that missed it — it was not connected, or a snapshot replaced what
/// it had — is told here, and one that already has it ignores the repeat.
pub fn pending_events(state: &AppState) -> Vec<Event> {
    state
        .session_requests
        .lock()
        .waiting
        .iter()
        .map(|waiting| waiting.request.event())
        .collect()
}

/// Stops the requests `matches` picks out from waiting and tells every client to close their
/// dialogs. Returns them, so an answer can carry its outcome to the waiting call.
fn close(
    state: &Arc<AppState>,
    matches: impl Fn(&Request) -> bool,
    how: SessionRequestEnding,
) -> Vec<Waiting> {
    let taken = state.session_requests.take(matches, how);
    announce_closed(state, &taken, how);
    taken
}

/// The one place every ending of a request passes through. Besides telling the clients, it lowers
/// the hand the request raised: the session goes back to what its agent's own events say, which
/// matters most when the agent gave up on the call without any signal reaching Octoboard, as no
/// event follows then to correct it.
fn announce_closed(state: &Arc<AppState>, closed: &[Waiting], how: SessionRequestEnding) {
    for waiting in closed {
        state.broadcast(Event::SessionRequestClosed {
            request_id: waiting.request.id.clone(),
            reason: how,
        });
        state.lower_request_hand(&waiting.request.session);
    }
}

/// Closes the dialogs of the requests a new request of their session took the place of, and wakes
/// the calls they were by dropping their answers.
fn withdraw_replaced(state: &Arc<AppState>, replaced: Vec<Waiting>) {
    announce_closed(state, &replaced, SessionRequestEnding::Withdrawn);
    drop(replaced);
}

/// Withdraws whatever the session has waiting, because its process has ended: its call has nobody
/// left to return to. Called wherever a live session is dropped.
pub fn withdraw_for_session(state: &Arc<AppState>, session_id: &str) {
    close(
        state,
        |request| request.session == session_id,
        SessionRequestEnding::Withdrawn,
    );
}

/// Withdraws a request when the call waiting on it is dropped: the call's connection to the daemon
/// was dropped, so its result has nowhere to go. A request that is no longer waiting — answered,
/// timed out, already withdrawn — is left alone, so dropping this after the call has its outcome
/// changes nothing.
struct WithdrawOnDrop<'a> {
    state: &'a Arc<AppState>,
    request_id: String,
}

impl Drop for WithdrawOnDrop<'_> {
    fn drop(&mut self) {
        close(
            self.state,
            |request| request.id == self.request_id,
            SessionRequestEnding::Withdrawn,
        );
    }
}

/// `request_session`: asks the user to start a session with no owner — a project session in any
/// project, or a console session — and waits up to `limit` for the answer, which is what the call
/// returns. `open` must be unbound: a session with no owner is the point of the request, and the
/// caller binding it is for `new_session`, or for the caller binding itself afterwards. What the
/// session could never be started for is refused before the user is asked. The `Err` case is what
/// the model is shown as the reason, so it is written for it. `handover` is moved to the session
/// on approval and only then. `automatic` is whether the user's setting to approve requests
/// without asking was on when the call was made, which the caller also words the session's
/// opening prompt by: the request is then approved at once, without asking.
///
/// A cancelled `request_session` is withdrawn by dropping this future (see `mcp::stdio`), so it
/// can stop at any `.await`: work that must finish once started belongs on a task of its own.
pub async fn ask_for_session(
    state: &Arc<AppState>,
    caller: &Session,
    open: OpenRequest,
    handover: Option<Handover>,
    automatic: bool,
    limit: Duration,
) -> Result<Value> {
    ask_to_approve(
        state,
        caller,
        Action::Start {
            open,
            handover: handover.map(Box::new),
        },
        automatic,
        limit,
    )
    .await
}

/// The ask both `request_session` and `request_resume_session` come to: asks the user to approve
/// `action` and waits up to `limit` for the answer, as [`ask_for_session`] does. What it could
/// never be carried out for is refused before the user is asked: here for a session to start and
/// for the session a record would open, and by the caller for a session that holds the
/// conversation (`mcp::tools::resume_request` judges it).
pub(crate) async fn ask_to_approve(
    state: &Arc<AppState>,
    caller: &Session,
    action: Action,
    automatic: bool,
    limit: Duration,
) -> Result<Value> {
    let opener = state.clone();
    let resumer = state.clone();
    ask_with(
        state,
        caller,
        action,
        automatic,
        limit,
        move |request| async move { coordinator::open::open_session(&opener, request).await },
        move |id| resume_through_coordinator(resumer, id),
    )
    .await
}

/// Brings a session back as the user's own resume does: no instruction, and no window appearance
/// to match, since an approval is not a window's.
async fn resume_through_coordinator(state: Arc<AppState>, id: String) -> Result<()> {
    coordinator::resume::resume_session(&state, &id, None, None).await
}

/// [`ask_to_approve`], starting a session through `launch` and bringing one back through
/// `resume`: `coordinator::open::open_session` and `coordinator::resume::resume_session` outside
/// the tests, which cannot launch an agent.
pub(crate) async fn ask_with<F, Fut, R, RFut>(
    state: &Arc<AppState>,
    caller: &Session,
    action: Action,
    automatic: bool,
    limit: Duration,
    launch: F,
    resume: R,
) -> Result<Value>
where
    F: FnOnce(OpenRequest) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Session>> + Send + 'static,
    R: FnOnce(String) -> RFut + Send + 'static,
    RFut: Future<Output = Result<()>> + Send + 'static,
{
    let (agent, console, project) = match &action {
        Action::Start { open, .. } | Action::ResumeRecord { open, .. } => (
            coordinator::open::check_open(state, open).await?,
            open.console_id.clone(),
            open.project_id.clone(),
        ),
        Action::Resume { target, .. } => {
            let target = state.session_record(target)?;
            (target.agent, target.console_id, target.project_id)
        }
    };
    let request = Request::new(
        caller,
        agent,
        console,
        project,
        Ask::Session(action),
        automatic,
    );
    if automatic {
        return approve_automatically(state, request, launch, resume).await;
    }
    wait_for_answer(state, caller, request, limit).await
}

/// Carries out `request` without asking, as the approval of a dialog would be. The request takes
/// the place of one its session has waiting, and blocks a further one until it is carried out.
///
/// The carrying out is a task of its own: the call is cancelled by dropping it (see
/// `mcp::stdio`), and a launch or a handover interrupted halfway would leave a session unsettled
/// and the caller never told. The call only waits for the task, and dropping it cancels nothing.
async fn approve_automatically<F, Fut, R, RFut>(
    state: &Arc<AppState>,
    request: Request,
    launch: F,
    resume: R,
) -> Result<Value>
where
    F: FnOnce(OpenRequest) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Session>> + Send + 'static,
    R: FnOnce(String) -> RFut + Send + 'static,
    RFut: Future<Output = Result<()>> + Send + 'static,
{
    let (replaced, approving) = state.session_requests.approve_directly(state, &request)?;
    withdraw_replaced(state, replaced);
    let state = state.clone();
    let carrying_out = tokio::spawn(async move {
        let _approving = approving;
        let approved = carry_out(&state, &request, Edits::default(), launch, resume)
            .await
            .map_err(|err| {
                anyhow!(
                    "this request for a session was approved automatically, but it could not be \
                     carried out: {err:#}"
                )
            })?;
        tell_the_caller(&state, &approved.caller_id, approved.message).await;
        Ok(approved.result)
    });
    carrying_out.await.unwrap_or_else(|err| {
        Err(anyhow!(
            "this request for a session was approved automatically, but carrying it out did not \
             finish: {err}"
        ))
    })
}

/// Holds `request`, has the clients ask the user, and waits up to `limit` for the answer.
async fn wait_for_answer(
    state: &Arc<AppState>,
    caller: &Session,
    request: Request,
    limit: Duration,
) -> Result<Value> {
    let request_id = request.id.clone();
    let (untouched, what, ask_again) = (
        request.untouched(),
        request.what(),
        request.do_not_ask_again(),
    );
    let event = request.event();
    let (answer, mut answered) = oneshot::channel();
    let replaced = state.session_requests.hold(request, answer)?;
    withdraw_replaced(state, replaced);
    state.broadcast(event);
    let _withdraw = WithdrawOnDrop {
        state,
        request_id: request_id.clone(),
    };
    // A session waiting on a dialog is waiting for the user, whatever its agent's events say
    // meanwhile.
    if let Err(err) = state.raise_request_hand(&caller.id) {
        tracing::debug!(session = %caller.id, %err, "raising the session request's hand failed");
    } else {
        crate::messaging::owner_notices::spawn_tell_owner_it_waits(state, &caller.id);
    }
    // The request was answerable from `hold` on, so it may have ended before the hand went up, and
    // then nothing else would lower it. Costs nothing while the request still waits.
    state.lower_request_hand(&caller.id);

    let outcome = tokio::select! {
        outcome = &mut answered => outcome,
        () = tokio::time::sleep(limit) => {
            let timed_out = close(
                state,
                |request| request.id == request_id,
                SessionRequestEnding::TimedOut,
            );
            if !timed_out.is_empty() {
                bail!(
                    "the user did not respond to this {what} within {} minutes, so it was \
                     dropped: {untouched}. {ask_again}",
                    limit.as_secs() / 60,
                );
            }
            // An answer took the request first and is being carried out: its outcome is this
            // call's.
            (&mut answered).await
        }
    };
    match outcome {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(reason)) => Err(anyhow!(reason)),
        Err(_) => bail!("this {what} was withdrawn before the user answered it: {untouched}"),
    }
}

/// The user's answer to a request, from the dialog its broadcast opened. The first answer wins:
/// the request stops waiting the moment it is taken, so an answer for one already answered, timed
/// out or withdrawn changes nothing and is refused with a code saying which.
///
/// An approval returns the session it started or brought back, or, for a switch or a handoff, the
/// session it moved or the one that took its place. `edits` are the values the user changed in the
/// dialog of a switch or a handoff.
pub async fn answer(
    state: &Arc<AppState>,
    request_id: &str,
    approve: bool,
    edits: Edits,
) -> Result<Option<Session>> {
    answer_with(
        state,
        request_id,
        approve,
        edits,
        |request| coordinator::open::open_session(state, request),
        |id| resume_through_coordinator(state.clone(), id),
    )
    .await
}

/// [`answer`], starting a session through `launch` and bringing one back through `resume`, for a
/// request that has no edits.
#[cfg(test)]
pub(crate) async fn answer_with_resume<F, Fut, R, RFut>(
    state: &Arc<AppState>,
    request_id: &str,
    approve: bool,
    launch: F,
    resume: R,
) -> Result<Option<Session>>
where
    F: FnOnce(OpenRequest) -> Fut,
    Fut: Future<Output = Result<Session>>,
    R: FnOnce(String) -> RFut,
    RFut: Future<Output = Result<()>>,
{
    answer_with(state, request_id, approve, Edits::default(), launch, resume).await
}

/// [`answer`], starting a session through `launch` and bringing one back through `resume`.
pub(crate) async fn answer_with<F, Fut, R, RFut>(
    state: &Arc<AppState>,
    request_id: &str,
    approve: bool,
    edits: Edits,
    launch: F,
    resume: R,
) -> Result<Option<Session>>
where
    F: FnOnce(OpenRequest) -> Fut,
    Fut: Future<Output = Result<Session>>,
    R: FnOnce(String) -> RFut,
    RFut: Future<Output = Result<()>>,
{
    let how = if approve {
        SessionRequestEnding::Approved
    } else {
        SessionRequestEnding::Refused
    };
    let Some((waiting, _approving)) = state.session_requests.take_answered(state, request_id, how)
    else {
        return Err(match state.session_requests.ending(request_id) {
            Some((session, SessionRequestEnding::TimedOut | SessionRequestEnding::Withdrawn)) => {
                not_waiting(&session)
            }
            _ => already_answered(),
        });
    };
    announce_closed(state, std::slice::from_ref(&waiting), how);
    if !approve {
        let _ = waiting.answer.send(Err(format!(
            "the user refused this {}: {}. {}",
            waiting.request.what(),
            waiting.request.untouched(),
            waiting.request.do_not_ask_again(),
        )));
        return Ok(None);
    }
    match carry_out(state, &waiting.request, edits, launch, resume).await {
        Ok(approved) => {
            // The call may be gone by now, so the outcome is also written into the session, after
            // the result so that a call still waiting has it first.
            let _ = waiting.answer.send(Ok(approved.result));
            tell_the_caller(state, &approved.caller_id, approved.message).await;
            Ok(Some(approved.started))
        }
        Err(err) => {
            let _ = waiting.answer.send(Err(format!(
                "the user approved this {}, but it could not be carried out: {err:#}",
                waiting.request.what()
            )));
            Err(err)
        }
    }
}

/// How an approval's outcome written into the caller's session opens, which is what tells the turn
/// it starts from any other.
const OUTCOME_OPENING: &str = concat!(texts::notice_prefix!(), " about your");

/// The message that tells a caller's session what became of its call of `tool`, the note being the
/// call's own, for a caller whose agent gave up on the call without any signal reaching Octoboard.
pub(crate) fn outcome_message(agent: Agent, tool: &str, note: &str) -> String {
    format!(
        "{OUTCOME_OPENING} `{}` call. {note} If that call already returned this, there is nothing \
         more to do about it.",
        qualified_tool_name(agent, tool)
    )
}

/// Writes an approval's outcome into the caller's session through the delivery every message takes.
/// The turn the message starts is credited as reported: it only reads an outcome the caller already
/// has from the call's result, and would otherwise reach its owner as a synthesised report.
pub(crate) async fn tell_the_caller(state: &Arc<AppState>, caller_id: &str, message: String) {
    state.credit_turn_opened_by(caller_id, OUTCOME_OPENING.to_string());
    if let Err(err) =
        delivery::write_message_off_runtime(state, caller_id, message, WhenBlocked::Queue).await
    {
        state.clear_turn_credit(caller_id, OUTCOME_OPENING);
        tracing::debug!(
            session = %caller_id,
            %err,
            "writing the approved request's outcome into the session failed"
        );
    }
}

fn not_waiting(session_id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_REQUEST_NOT_WAITING,
        "this session is no longer waiting for an answer, so its request was not carried out",
        &[("session", session_id)],
    )
}

/// The refusal of an answer to something that is not a waiting request: one already answered, or
/// one the daemon does not know.
fn already_answered() -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_REQUEST_ANSWERED,
        "this is not a waiting session request: it was answered already",
        &[],
    )
}
