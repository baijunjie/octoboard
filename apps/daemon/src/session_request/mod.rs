//! A session's request for a session the user has to approve: the `request_session` call, which
//! waits while the user is asked, the requests the daemon holds meanwhile, and what the user's
//! answer does.
//!
//! **The confirmation is the enforcement.** An approved request puts a session with no owner in
//! any project, or a console session that dispatches into every project of its console, on the
//! word of a model, so nothing here acts on a call by itself: the request is broadcast for a
//! dialog, the call waits, and only an approval starts anything. A rule in a prompt could be
//! ignored by the model; this cannot. The one exception is the user's own standing approval, the
//! setting described next, which only the user's client can change.
//!
//! **The user can waive the dialog.** While the application setting `auto_approve_session_requests`
//! is on, a request that passed every pre-check is approved at once, through the same
//! [`carry_out`] an approval from the dialog takes: nothing is broadcast, no hand goes up, and the
//! session is started, the branches handed over and the outcome told exactly as for an approval.
//! That runs in a task of its own, so a call cancelled meanwhile does not interrupt it. The
//! setting is read when the call is made, so a request already waiting stays in its dialog.
//!
//! **A request lives until it is answered, times out or is withdrawn.** It is held in memory only
//! ([`SessionRequests`] on `AppState`) and sent again to a client after each `snapshot`, like a
//! trust prompt. Nothing needs persisting: every agent process ends with the daemon, and the call
//! is served inside it, so a restart leaves no caller waiting. A request is withdrawn on the
//! signals the daemon sees for every agent alike — the caller's process ends
//! ([`withdraw_for_session`], from the live-session bookkeeping), or the call's connection to the
//! daemon is dropped, which drops the waiting call ([`wait_for_answer`]) and with it its guard. An
//! agent that cancels the call over MCP reaches the second: its stdio server closes that
//! connection (`mcp::stdio`). A session that asks again has stopped waiting for its earlier call,
//! so its new request takes the earlier one's place.
//!
//! **An agent can also give up on the call by its own time limit without any signal reaching
//! Octoboard,** and nothing here tries to tell when. So an approval is always carried out the same
//! way, and its outcome is written into the caller's session as a message as well as returned as
//! the call's result: when the result was lost, the message is how the caller learns what became
//! of its request. The turn that message starts is credited as reported, since it only reads an
//! outcome the caller already has. The turn Claude Code starts on the result of a backgrounded
//! `request_session` call is not: there the caller's round really continues, and its end is a
//! report its owner is owed.

#[cfg(test)]
mod tests;

use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::branch_origin::handover::{
    branch_noun_and_pronoun, move_branches, named_branches, Handed, Handover,
};
use crate::coordinator::{self, OpenRequest};
use crate::mcp::{qualified_tool_name, tool_by_name};
use crate::protocol::{
    error_code, now_millis, Agent, CodedError, Event, Role, Session, SessionRequestEnding,
};
use crate::reporting::{self, WhenBlocked};
use crate::sharing::{one_line, place_of};
use crate::state::AppState;

/// How long a call waits for the user's answer. Octoboard's own value, for every agent alike, and
/// kept below `mcp::stdio`'s `CALL_TIMEOUT` with room left for an approval given at the last
/// moment to be carried out — launching the session included — before that forwarding gives up.
/// An agent's own limit is not obeyed; the launch sets it above that forwarding limit instead,
/// whichever side the agent's own default falls on (apps/daemon/PROTOCOL.md's
/// "`POST /mcp/:token`" section has the ordering).
pub const ANSWER_TIME_LIMIT: Duration = Duration::from_secs(8 * 60);

const _: () = assert!(ANSWER_TIME_LIMIT.as_secs() < crate::mcp::stdio::CALL_TIMEOUT.as_secs());

/// How many ended requests are remembered, for telling a late answer what became of its request.
/// Far more than could be on screen at once; the rest are forgotten oldest first.
const REMEMBERED_ENDINGS: usize = 64;

/// What a call that changed nothing tells the model to do next: stopping there is the expected
/// outcome, and asking again would put the same question in front of the user straight away.
const DO_NOT_ASK_AGAIN: &str = "Do not ask again unless the user tells you to. Tell the user in \
                                this terminal what you wanted the session for, and follow their \
                                direction on whether to continue your own work or leave handover \
                                notes and end";

/// What the waiting call is handed when its request is answered: the tool's result, or the reason
/// it failed, written for the model.
type Outcome = std::result::Result<Value, String>;

/// The requests waiting for the user's answer, and how the last few that stopped waiting ended.
#[derive(Default)]
pub struct SessionRequests {
    inner: Mutex<Requests>,
}

#[derive(Default)]
struct Requests {
    /// Oldest first.
    waiting: Vec<Waiting>,
    ended: VecDeque<Ended>,
    /// The sessions whose request has been approved and is being carried out. Such a session has
    /// no request waiting, and asking again meanwhile would have a second session started for it.
    approving: HashSet<String>,
}

struct Waiting {
    request: Request,
    /// Dropped without a value when the request is withdrawn, which is what wakes the call.
    answer: oneshot::Sender<Outcome>,
}

struct Ended {
    request_id: String,
    session: String,
    how: SessionRequestEnding,
}

/// One request: what the dialog is told of it, and the session an approval opens.
struct Request {
    id: String,
    /// The requesting session.
    session: String,
    requested_at: i64,
    /// The session to open, unbound.
    open: OpenRequest,
    /// The agent the session would run, settled when the request was made.
    agent: Agent,
    /// The branches an approval hands to the session, if the request names any.
    handover: Option<Handover>,
    /// Whether the approval is Octoboard's own, under the user's setting, rather than the user's
    /// answer to the dialog. Only the wording of the outcome differs.
    automatic: bool,
}

impl Request {
    /// Whether the session asked for is a console session, which is opened without a project.
    fn role(&self) -> Role {
        match self.open.project_id {
            Some(_) => Role::Project,
            None => Role::Console,
        }
    }

    fn event(&self) -> Event {
        Event::SessionRequest {
            request_id: self.id.clone(),
            session: self.session.clone(),
            role: self.role(),
            console: self.open.console_id.clone(),
            project: self.open.project_id.clone(),
            agent: self.agent,
            branches: self
                .handover
                .as_ref()
                .map(|handover| handover.branches.clone())
                .unwrap_or_default(),
            requested_at: self.requested_at,
        }
    }
}

impl Requests {
    /// Makes room for a new request of `session`: refused while an approval of its earlier one is
    /// being carried out, else the request it has waiting is withdrawn and returned, since a
    /// session asking again has stopped waiting for its earlier call.
    fn make_room_for(&mut self, session: &str) -> Result<Vec<Waiting>> {
        if self.approving.contains(session) {
            bail!(
                "this session's earlier request has just been approved, and it is being carried \
                 out; its outcome arrives in this terminal as a message"
            );
        }
        Ok(self.take(
            |waiting| waiting.session == session,
            SessionRequestEnding::Withdrawn,
        ))
    }

    /// Marks `session` as being approved until the returned guard is dropped.
    fn start_approving(&mut self, state: &Arc<AppState>, session: &str) -> Approving {
        self.approving.insert(session.to_string());
        Approving {
            state: state.clone(),
            session: session.to_string(),
        }
    }

    /// Takes the requests `matches` picks out of the waiting ones, recording how they ended.
    fn take(
        &mut self,
        matches: impl Fn(&Request) -> bool,
        how: SessionRequestEnding,
    ) -> Vec<Waiting> {
        let (taken, kept) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|waiting| matches(&waiting.request));
        self.waiting = kept;
        for waiting in &taken {
            if self.ended.len() == REMEMBERED_ENDINGS {
                self.ended.pop_front();
            }
            self.ended.push_back(Ended {
                request_id: waiting.request.id.clone(),
                session: waiting.request.session.clone(),
                how,
            });
        }
        taken
    }
}

impl SessionRequests {
    fn lock(&self) -> std::sync::MutexGuard<'_, Requests> {
        self.inner.lock().expect("session requests lock poisoned")
    }

    /// Holds a new request, in place of one its session already has waiting, which is withdrawn
    /// and returned: a session asking again has stopped waiting for its earlier call, which its
    /// agent may have given up on by a time limit of its own without a word to Octoboard.
    /// Refused while an approval of its session's earlier request is being carried out.
    fn hold(&self, request: Request, answer: oneshot::Sender<Outcome>) -> Result<Vec<Waiting>> {
        let mut requests = self.lock();
        let replaced = requests.make_room_for(&request.session)?;
        requests.waiting.push(Waiting { request, answer });
        Ok(replaced)
    }

    /// Marks a request approved without ever waiting: the session whose request it is takes the
    /// place of one it already has waiting, which is withdrawn and returned, and is marked as being
    /// approved until the returned guard is dropped. Refused as [`Self::hold`] refuses.
    fn approve_directly(
        &self,
        state: &Arc<AppState>,
        request: &Request,
    ) -> Result<(Vec<Waiting>, Approving)> {
        let mut requests = self.lock();
        let replaced = requests.make_room_for(&request.session)?;
        let approving = requests.start_approving(state, &request.session);
        Ok((replaced, approving))
    }

    fn take(&self, matches: impl Fn(&Request) -> bool, how: SessionRequestEnding) -> Vec<Waiting> {
        self.lock().take(matches, how)
    }

    /// Whether the session has a request waiting.
    pub fn is_waiting_for(&self, session_id: &str) -> bool {
        self.lock()
            .waiting
            .iter()
            .any(|waiting| waiting.request.session == session_id)
    }

    /// Takes one request for the user's answer. An approval also marks the request's session as
    /// being approved, in the same step, until the returned guard is dropped.
    fn take_answered(
        &self,
        state: &Arc<AppState>,
        request_id: &str,
        how: SessionRequestEnding,
    ) -> Option<(Waiting, Option<Approving>)> {
        let mut requests = self.lock();
        let waiting = requests
            .take(|request| request.id == request_id, how)
            .pop()?;
        let approving = (how == SessionRequestEnding::Approved)
            .then(|| requests.start_approving(state, &waiting.request.session));
        Some((waiting, approving))
    }

    /// How a request that is no longer waiting ended and whose it was, if it is still remembered.
    fn ending(&self, request_id: &str) -> Option<(String, SessionRequestEnding)> {
        self.lock()
            .ended
            .iter()
            .find(|ended| ended.request_id == request_id)
            .map(|ended| (ended.session.clone(), ended.how))
    }
}

/// A session's approval being carried out, released on drop however carrying it out ends.
struct Approving {
    state: Arc<AppState>,
    session: String,
}

impl Drop for Approving {
    fn drop(&mut self) {
        self.state
            .session_requests
            .lock()
            .approving
            .remove(&self.session);
    }
}

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
    let opener = state.clone();
    ask_for_session_with(
        state,
        caller,
        open,
        handover,
        automatic,
        limit,
        move |request| async move { coordinator::open_session(&opener, request).await },
    )
    .await
}

/// [`ask_for_session`], starting the session through `launch`: `coordinator::open_session`
/// outside the tests, which cannot launch an agent.
pub(crate) async fn ask_for_session_with<F, Fut>(
    state: &Arc<AppState>,
    caller: &Session,
    open: OpenRequest,
    handover: Option<Handover>,
    automatic: bool,
    limit: Duration,
    launch: F,
) -> Result<Value>
where
    F: FnOnce(OpenRequest) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Session>> + Send + 'static,
{
    let agent = coordinator::check_open(state, &open).await?;
    let request = Request {
        id: Uuid::new_v4().to_string(),
        session: caller.id.clone(),
        requested_at: now_millis(),
        open,
        agent,
        handover,
        automatic,
    };
    if automatic {
        return approve_automatically(state, request, launch).await;
    }
    wait_for_answer(state, caller, request, limit).await
}

/// Carries out `request` without asking, as the approval of a dialog would be. The request takes
/// the place of one its session has waiting, and blocks a further one until it is carried out.
///
/// The carrying out is a task of its own: the call is cancelled by dropping it (see
/// `mcp::stdio`), and a launch or a handover interrupted halfway would leave a session unsettled
/// and the caller never told. The call only waits for the task, and dropping it cancels nothing.
async fn approve_automatically<F, Fut>(
    state: &Arc<AppState>,
    request: Request,
    launch: F,
) -> Result<Value>
where
    F: FnOnce(OpenRequest) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Session>> + Send + 'static,
{
    let (replaced, approving) = state.session_requests.approve_directly(state, &request)?;
    withdraw_replaced(state, replaced);
    let state = state.clone();
    let carrying_out = tokio::spawn(async move {
        let _approving = approving;
        let approved = carry_out(&state, &request, launch).await.map_err(|err| {
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
                    "the user did not respond to this request for a session within {} minutes, \
                     so it was dropped: nothing was started. {DO_NOT_ASK_AGAIN}",
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
        Err(_) => bail!(
            "this request for a session was withdrawn before the user answered it: nothing was \
             started"
        ),
    }
}

/// The user's answer to a request, from the dialog its broadcast opened. The first answer wins:
/// the request stops waiting the moment it is taken, so an answer for one already answered, timed
/// out or withdrawn changes nothing and is refused with a code saying which.
///
/// An approval returns the session it started.
pub async fn answer(
    state: &Arc<AppState>,
    request_id: &str,
    approve: bool,
) -> Result<Option<Session>> {
    answer_with(state, request_id, approve, |request| {
        coordinator::open_session(state, request)
    })
    .await
}

/// [`answer`], starting the session through `launch`: `coordinator::open_session` outside the
/// tests, which cannot launch an agent.
pub(crate) async fn answer_with<F, Fut>(
    state: &Arc<AppState>,
    request_id: &str,
    approve: bool,
    launch: F,
) -> Result<Option<Session>>
where
    F: FnOnce(OpenRequest) -> Fut,
    Fut: Future<Output = Result<Session>>,
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
            "the user refused this request for a session: nothing was started. {DO_NOT_ASK_AGAIN}"
        )));
        return Ok(None);
    }
    match carry_out(state, &waiting.request, launch).await {
        Ok(approved) => {
            // The call may be gone by now, so the outcome is also written into the session, after
            // the result so that a call still waiting has it first.
            let _ = waiting.answer.send(Ok(approved.result));
            tell_the_caller(state, &approved.caller_id, approved.message).await;
            Ok(Some(approved.started))
        }
        Err(err) => {
            let _ = waiting.answer.send(Err(format!(
                "the user approved this request for a session, but it could not be carried out: \
                 {err:#}"
            )));
            Err(err)
        }
    }
}

/// An approval carried out: the call's result, and the same outcome as a message for the session
/// that asked.
struct Approved {
    result: Value,
    caller_id: String,
    started: Session,
    message: String,
}

/// Carries out an approval: starts the session the caller described, unbound, then hands it the
/// branches the request names. The caller keeps running, and does not own what it asked for.
async fn carry_out<F, Fut>(state: &Arc<AppState>, request: &Request, launch: F) -> Result<Approved>
where
    F: FnOnce(OpenRequest) -> Fut,
    Fut: Future<Output = Result<Session>>,
{
    let caller = state.session_record(&request.session)?;
    // Withdrawn when its process ended, but that can land while the answer is on its way.
    if caller.status.is_dormant() || !state.has_process(&caller.id) {
        return Err(not_waiting(&caller.id));
    }
    let started = launch(request.open.clone()).await?;
    let handed = hand_over_branches(state, request, &caller, &started).await;
    let note = outcome_note(state, &caller, &started, handed.as_ref(), request.automatic);
    let message = format!(
        "{OUTCOME_OPENING} `{}` call. {note} If that call already returned this, there is nothing \
         more to do about it.",
        qualified_tool_name(caller.agent, "request_session")
    );
    Ok(Approved {
        result: json!({
            "session": started.id,
            "role": started.role,
            "agent": started.agent,
            "title": started.title,
            "console": started.console_id,
            "project": started.project_id,
            "note": note,
        }),
        caller_id: caller.id,
        started,
        message,
    })
}

/// Moves the branches the request names to the session it started, each only if the caller is
/// still its responsible session. The session is running by now, so a failure to move is not one
/// of the approval: it is told to the caller instead.
async fn hand_over_branches(
    state: &Arc<AppState>,
    request: &Request,
    caller: &Session,
    started: &Session,
) -> Option<Handed> {
    let handover = request.handover.as_ref()?;
    Some(move_branches(state, handover, caller, started).await)
}

/// What a session is told of the session it asked for, in the call's result and in the message
/// alike: where it was started, that it reports to the user, what became of the branches it asked
/// to hand over, and what that leaves the caller able to do with the session — share information,
/// read it where the caller can read sessions, and bind itself to it where the caller could.
fn outcome_note(
    state: &Arc<AppState>,
    caller: &Session,
    started: &Session,
    handed: Option<&Handed>,
    automatic: bool,
) -> String {
    let tool = |name: &str| qualified_tool_name(caller.agent, name);
    let has = |name: &str| tool_by_name(caller.role, caller.has_bound_role(), name).is_some();
    let place = place_of(state, started);
    let kind = started.role.kind();
    let reads = if has("get_session") {
        format!(", and `{}` reads it", tool("get_session"))
    } else {
        String::new()
    };
    let can_bind =
        has("bind_to_session") && caller.bound_to.is_none() && can_own(state, caller, started);
    let bind = if can_bind {
        format!(
            " Should you want it to drive your work instead, `{}` with `owner` set to `{}` makes \
             you its session; that call's result tells you what then changes.",
            tool("bind_to_session"),
            started.id
        )
    } else {
        String::new()
    };
    let branches = handed.map(branch_outcome).unwrap_or_default();
    let approval = if automatic {
        "Octoboard approved the request automatically, as the user has chosen to have session \
         requests approved without asking"
    } else {
        "The user approved the request"
    };
    format!(
        "{approval}: Octoboard started the {kind} session `{id}` (\"{title}\", \
         {agent}){place}, with your brief as its opening prompt. It has no owner: it reports to \
         the user, not to you. It is not yours to drive — you cannot instruct, archive or reopen \
         it. Follow the user's direction on whether to continue your own work or end after the \
         handover.{branches} `{share_info}` tells it something that touches its work{reads}.{bind}",
        id = started.id,
        title = one_line(&started.title),
        agent = started.agent.label(),
        share_info = tool("share_info"),
    )
}

/// What became of the branches a request handed over, as a sentence or two for the outcome note,
/// each opening with a space.
pub(crate) fn branch_outcome(handed: &Handed) -> String {
    let mut text = String::new();
    match handed {
        Handed::Moved(handed) => {
            if !handed.moved.is_empty() {
                let (_, them) = branch_noun_and_pronoun(&handed.moved);
                text.push_str(&format!(
                    " Handed over to it: {}. It is now the session responsible for {them} and you \
                     no longer are.",
                    named_branches(&handed.moved)
                ));
            }
            if !handed.unmoved.is_empty() {
                let (them, they, were, stay) = match handed.unmoved.as_slice() {
                    [_] => ("it", "It", "it was", "stays as it was"),
                    _ => ("them", "They", "they were", "stay as they were"),
                };
                text.push_str(&format!(
                    " Not handed over, because by then you were no longer responsible for {them} \
                     or {were} gone: {}. {they} {stay}.",
                    named_branches(&handed.unmoved)
                ));
            }
        }
        Handed::Failed(branches) => {
            let (_, them) = branch_noun_and_pronoun(branches);
            text.push_str(&format!(
                " Octoboard could not record the handover of {}, so you are still responsible for \
                 {them}.",
                named_branches(branches)
            ));
        }
    }
    text
}

/// Whether `owner` could own `session` right now, by the rules a binding keeps.
fn can_own(state: &Arc<AppState>, session: &Session, owner: &Session) -> bool {
    matches!(coordinator::owner_refusal(state, session, owner), Ok(None))
}

/// How an approval's outcome written into the caller's session opens, which is what tells the turn
/// it starts from any other.
const OUTCOME_OPENING: &str = "Message from Octoboard about your";

/// Writes an approval's outcome into the caller's session through the delivery every message takes.
/// The turn the message starts is credited as reported: it only reads an outcome the caller already
/// has from the call's result, and would otherwise reach its owner as a synthesised report.
async fn tell_the_caller(state: &Arc<AppState>, caller_id: &str, message: String) {
    state.credit_turn_opened_by(caller_id, OUTCOME_OPENING.to_string());
    if let Err(err) =
        reporting::write_message_off_runtime(state, caller_id, message, WhenBlocked::Queue).await
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
        "this session is no longer waiting for an answer, so the session it asked for was not \
         started",
        &[("session", session_id)],
    )
}

/// The refusal of an answer to something that is not a waiting request: one already answered, or
/// one the daemon does not know.
fn already_answered() -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_REQUEST_ANSWERED,
        "this is not a waiting request to start a session: it was answered already",
        &[],
    )
}
