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

mod change;
mod outcome;
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

use crate::branch_origin::handover::{move_branches, Handover};
use crate::coordinator::{self, OpenRequest};
use crate::mcp::qualified_tool_name;
use crate::protocol::{
    error_code, now_millis, Agent, CodedError, Event, Role, Session, SessionRequestAction,
    SessionRequestEnding, SessionStatus,
};
use crate::reporting::{self, WhenBlocked};
use crate::state::AppState;

pub(crate) use change::{ask_to_change, run_unasked, Change, Edits, Perform, Performed};
pub(crate) use outcome::branch_outcome;
use outcome::{
    approval_wording, outcome_note, record_outcome_note, resume_outcome_note, resume_result,
};

/// How long a call waits for the user's answer. Octoboard's own value, for every agent alike, and
/// kept below `mcp::stdio`'s `CALL_TIMEOUT` with room left for an approval given at the last
/// moment to be carried out — launching the session included — before that forwarding gives up.
/// An agent's own limit is not obeyed; the launch sets it above that forwarding limit instead,
/// whichever side the agent's own default falls on (apps/daemon/PROTOCOL.md's
/// "`POST /mcp/:token` — Octoboard's MCP tools" section has the ordering).
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

/// [`DO_NOT_ASK_AGAIN`] for a switch or a handoff, which has no new session to want.
const DO_NOT_ASK_AGAIN_TO_CHANGE: &str = "Do not ask again unless the user tells you to. Tell the \
                                          user in this terminal why you wanted it, and follow \
                                          their direction on what to do instead";

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
    /// The requests that have been approved and are being carried out, by `Request::approval_slot`.
    /// Such a request is not waiting, and asking again meanwhile would have it carried out twice.
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

/// What an approval does, as the request was made.
pub(crate) enum Action {
    /// Starts a session, unbound, from `open`, and moves `handover`'s branches to it.
    Start {
        open: OpenRequest,
        handover: Option<Handover>,
    },
    /// Brings back the archived or interrupted session `target`, which holds the conversation;
    /// `archived` is which it was when the request was made, which is what the dialog words the
    /// question by. Its owner stays as it is, and an archived owner comes back with it
    /// (`reopened_owners`, as they stood when the request was made, which is what the dialog
    /// lists). `rename` is the name to give an archived session once it is back.
    Resume {
        target: String,
        archived: bool,
        owner: Option<String>,
        reopened_owners: Vec<String>,
        rename: Option<String>,
    },
    /// Opens a session, unbound, from `open`, which continues the agent's own record of the
    /// conversation `conversation` that no session holds.
    ResumeRecord {
        open: OpenRequest,
        conversation: String,
    },
}

/// What a request asks the user to approve: a session to start or bring back, or a change to a
/// session the requester owns (`change`).
pub(crate) enum Ask {
    Session(Action),
    Change(Change),
}

/// One request: what the dialog is told of it, and what an approval does.
struct Request {
    id: String,
    /// The requesting session.
    session: String,
    requested_at: i64,
    /// The agent the session would run, settled when the request was made; for a resume, the
    /// agent of the target or of the record; for a switch, the target's, and for a handoff, the
    /// receiving agent.
    agent: Agent,
    /// Where the session is, or would be.
    console: String,
    project: Option<String>,
    ask: Ask,
    /// Whether the approval is Octoboard's own, under the user's setting, rather than the user's
    /// answer to the dialog. Only the wording of the outcome differs.
    automatic: bool,
}

impl Request {
    fn new(
        caller: &Session,
        agent: Agent,
        console: String,
        project: Option<String>,
        ask: Ask,
        automatic: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session: caller.id.clone(),
            requested_at: now_millis(),
            agent,
            console,
            project,
            ask,
            automatic,
        }
    }

    /// Whether the session asked for is a console session, which is opened without a project.
    fn role(&self) -> Role {
        match self.project {
            Some(_) => Role::Project,
            None => Role::Console,
        }
    }

    /// The tool the request was made with, as the outcome message names it.
    fn tool(&self) -> &'static str {
        match &self.ask {
            Ask::Session(Action::Start { .. }) => "request_session",
            Ask::Session(Action::Resume { .. } | Action::ResumeRecord { .. }) => {
                "request_resume_session"
            }
            Ask::Change(change) => change.tool(),
        }
    }

    /// What the request is called in what the call is told of how it ended.
    fn what(&self) -> &'static str {
        match &self.ask {
            Ask::Session(_) => "request for a session",
            Ask::Change(change) => change.what(),
        }
    }

    /// What a call that changed nothing is told to do next.
    fn do_not_ask_again(&self) -> &'static str {
        match &self.ask {
            Ask::Session(_) => DO_NOT_ASK_AGAIN,
            Ask::Change(_) => DO_NOT_ASK_AGAIN_TO_CHANGE,
        }
    }

    /// How the user's approval of `action` is worded in the outcome, by the request's kind.
    fn approval(&self, action: &Action) -> &'static str {
        approval_wording(self.automatic, action)
    }

    /// What a request that came to nothing says was left undone.
    fn untouched(&self) -> &'static str {
        match &self.ask {
            Ask::Session(Action::Start { .. }) => "nothing was started",
            Ask::Session(Action::Resume { .. } | Action::ResumeRecord { .. }) => {
                "nothing was resumed"
            }
            Ask::Change(_) => "nothing was changed",
        }
    }

    /// Whether this request takes the place of `other`, which the same session has waiting: a
    /// session asking again has stopped waiting for its earlier call. A switch or a handoff
    /// replaces only an earlier one of the same kind for the same session, so an owner can have
    /// several sessions' changes waiting at once.
    fn replaces(&self, other: &Request) -> bool {
        self.session == other.session
            && match (&self.ask, &other.ask) {
                (Ask::Session(_), Ask::Session(_)) => true,
                (Ask::Change(this), Ask::Change(that)) => this.same_change(that),
                _ => false,
            }
    }

    /// What the session's approval being carried out is marked by: the session itself for a
    /// request for a session, and the change for a switch or a handoff, so that one change being
    /// carried out does not hold another back.
    fn approval_slot(&self) -> String {
        match &self.ask {
            Ask::Session(_) => self.session.clone(),
            Ask::Change(change) => format!("{}\0{}", self.session, change.slot()),
        }
    }

    fn event(&self) -> Event {
        let mut target = None;
        let mut owner = None;
        let mut reopened_owners = Vec::new();
        let mut account = None;
        let mut conversation = None;
        let mut branches = Vec::new();
        let mut model = None;
        let mut effort = None;
        let action = match &self.ask {
            Ask::Change(change) => {
                let described = change.describe();
                target = Some(described.target);
                account = described.account;
                model = described.model;
                effort = described.effort;
                described.action
            }
            Ask::Session(Action::Start { handover, .. }) => {
                branches = handover
                    .as_ref()
                    .map(|handover| handover.branches.clone())
                    .unwrap_or_default();
                SessionRequestAction::Start
            }
            Ask::Session(Action::Resume {
                target: session,
                archived,
                owner: reports_to,
                reopened_owners: owners,
                ..
            }) => {
                target = Some(session.clone());
                owner = reports_to.clone();
                reopened_owners = owners.clone();
                if *archived {
                    SessionRequestAction::Reopen
                } else {
                    SessionRequestAction::Resume
                }
            }
            Ask::Session(Action::ResumeRecord {
                open,
                conversation: id,
            }) => {
                account = open.account.clone().flatten();
                conversation = Some(id.clone());
                SessionRequestAction::ResumeRecord
            }
        };
        Event::SessionRequest {
            request_id: self.id.clone(),
            session: self.session.clone(),
            action,
            role: self.role(),
            console: self.console.clone(),
            project: self.project.clone(),
            agent: self.agent,
            target,
            owner,
            reopened_owners,
            account,
            conversation,
            branches,
            model,
            effort,
            requested_at: self.requested_at,
        }
    }
}

impl Requests {
    /// Makes room for `request`: refused while an approval of its earlier one is being carried
    /// out, else the request it takes the place of (`Request::replaces`) is withdrawn and
    /// returned, since a session asking again has stopped waiting for its earlier call.
    fn make_room_for(&mut self, request: &Request) -> Result<Vec<Waiting>> {
        if self.approving.contains(&request.approval_slot()) {
            bail!(
                "this session's earlier request has just been approved, and it is being carried \
                 out; its outcome arrives in this terminal as a message"
            );
        }
        Ok(self.take(
            |waiting| request.replaces(waiting),
            SessionRequestEnding::Withdrawn,
        ))
    }

    /// Marks `request` as being approved until the returned guard is dropped.
    fn start_approving(&mut self, state: &Arc<AppState>, request: &Request) -> Approving {
        let slot = request.approval_slot();
        self.approving.insert(slot.clone());
        Approving {
            state: state.clone(),
            slot,
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

    /// Holds a new request, in place of the one it replaces, which is withdrawn and returned: a
    /// session asking again has stopped waiting for its earlier call, which its agent may have
    /// given up on by a time limit of its own without a word to Octoboard.
    /// Refused while an approval of its session's earlier request is being carried out.
    fn hold(&self, request: Request, answer: oneshot::Sender<Outcome>) -> Result<Vec<Waiting>> {
        let mut requests = self.lock();
        let replaced = requests.make_room_for(&request)?;
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
        let replaced = requests.make_room_for(request)?;
        let approving = requests.start_approving(state, request);
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
            .then(|| requests.start_approving(state, &waiting.request));
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

/// An approval being carried out, released on drop however carrying it out ends.
struct Approving {
    state: Arc<AppState>,
    slot: String,
}

impl Drop for Approving {
    fn drop(&mut self) {
        self.state
            .session_requests
            .lock()
            .approving
            .remove(&self.slot);
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
    ask_to_approve(
        state,
        caller,
        Action::Start { open, handover },
        automatic,
        limit,
    )
    .await
}

/// The ask both `request_session` and `request_resume_session` come to: asks the user to approve
/// `action` and waits up to `limit` for the answer, as [`ask_for_session`] does. What it could
/// never be carried out for is refused before the user is asked: here for a session to start and
/// for the session a record would open, and by the caller for a session that holds the
/// conversation (`mcp::resume_request` judges it).
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
        move |request| async move { coordinator::open_session(&opener, request).await },
        move |id| resume_through_coordinator(resumer, id),
    )
    .await
}

/// Brings a session back as the user's own resume does: no instruction, and no window appearance
/// to match, since an approval is not a window's.
async fn resume_through_coordinator(state: Arc<AppState>, id: String) -> Result<()> {
    coordinator::resume_session(&state, &id, None, None).await
}

/// [`ask_to_approve`], starting a session through `launch` and bringing one back through
/// `resume`: `coordinator::open_session` and `coordinator::resume_session` outside the tests,
/// which cannot launch an agent.
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
            coordinator::check_open(state, open).await?,
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
        |request| coordinator::open_session(state, request),
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

/// An approval carried out: the call's result, and the same outcome as a message for the session
/// that asked.
struct Approved {
    result: Value,
    caller_id: String,
    /// The session the approval started or brought back, or found running.
    started: Session,
    message: String,
}

/// Carries out an approval. A start opens the session the caller described, unbound, then hands
/// it the branches the request names. A resume brings back the session that holds the
/// conversation, or opens one on the record of it. A switch or a handoff runs the operation of its
/// tool with the values the user ended the dialog on. The caller keeps running, and does not own
/// what it asked for, except for the session a switch or a handoff leaves in its care.
async fn carry_out<F, Fut, R, RFut>(
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
    if caller.status.is_dormant() || !state.has_process(&caller.id) {
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
            let result = resume_result("opened", &started, &[], None, &note);
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
        SessionStatus::Archived => "reopened",
        status if status.is_dormant() => "resumed",
        _ => "already_running",
    };
    let owners = if action == "already_running" {
        Vec::new()
    } else {
        coordinator::archived_owners(state, target)?
    };
    if action != "already_running" {
        if let Err(err) = resume(target.to_string()).await {
            if refused_as_running(state, &err, target)? {
                action = "already_running";
            } else {
                return Err(err);
            }
        }
    }
    let mut renamed = None;
    let mut rename_failure = None;
    if let Some(title) = rename {
        if action != "reopened" {
            renamed = Some(false);
            rename_failure = Some(format!(
                "only a session reopened from the archive is renamed, and this one was {}",
                if action == "resumed" {
                    "not archived"
                } else {
                    "already running"
                }
            ));
        } else if state.session_record(target)?.title != title {
            match coordinator::rename_session(state, target, title) {
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
    let reopened_owners: Vec<String> = if action == "already_running" {
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
/// dormant status by now; otherwise the approval could not be carried out.
fn refused_as_running(state: &Arc<AppState>, err: &anyhow::Error, target: &str) -> Result<bool> {
    let Some(coded) = err.downcast_ref::<CodedError>() else {
        return Ok(false);
    };
    let for_target = coded.params.get("session").map(String::as_str) == Some(target);
    let launching = match coded.code {
        error_code::SESSION_ALREADY_RUNNING => for_target,
        // A switch's claim raises the same code as a launch's. A claim gone by now falls through
        // to the dormant re-check.
        error_code::SESSION_ALREADY_STARTING => for_target && state.is_held_by_launch(target),
        _ => return Ok(false),
    };
    Ok(launching || !state.session_record(target)?.status.is_dormant())
}

/// How an approval's outcome written into the caller's session opens, which is what tells the turn
/// it starts from any other.
const OUTCOME_OPENING: &str = concat!(reporting::notice_prefix!(), " about your");

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
