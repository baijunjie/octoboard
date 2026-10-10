//! A project session's request for a console session: the `request_console_session` call, which
//! waits while the user is asked, the requests the daemon holds meanwhile, and what the user's
//! answer does.
//!
//! **The confirmation is the enforcement.** An approved request hands a session the user opened by
//! hand a route to the whole console, through a console session that dispatches into every project
//! of it, so nothing here acts on the call by itself: the request is broadcast for a dialog, the
//! call waits, and only an approval starts anything. A rule in a prompt could be ignored by the
//! model; this cannot.
//!
//! **A request lives until it is answered, times out or is withdrawn.** It is held in memory only
//! ([`ConsoleRequests`] on `AppState`) and sent again to a client after each `snapshot`, like a
//! trust prompt. Nothing needs persisting: every agent process ends with the daemon, and the call
//! is served inside it, so a restart leaves no caller waiting. A request is withdrawn on the
//! signals the daemon sees for every agent alike — the caller's process ends
//! ([`withdraw_for_session`], from the live-session bookkeeping), or the call's connection to the
//! daemon is dropped, which drops the waiting call ([`ask`]) and with it its guard. An agent that
//! cancels the call over MCP reaches the second: its stdio server closes that connection
//! (`mcp::stdio`). A session that asks again has stopped waiting for its earlier call, so its new
//! request takes the earlier one's place.
//!
//! **An agent can also give up on the call by its own time limit without any signal reaching
//! Octoboard,** and nothing here tries to tell when. So an approval is always carried out the same
//! way, and its outcome is written into the caller's session as a message as well as returned as
//! the call's result: when the result was lost, the message is how the caller learns it is bound.

use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::coordinator::{self, OpenRequest};
use crate::hooks;
use crate::mcp::{qualified_tool_name, SERVER_KEY};
use crate::protocol::{
    error_code, now_millis, Agent, CodedError, ConsoleRequestEnding, Event, Origin, Session,
};
use crate::reporting::{self, Report, ReportStatus, WhenBlocked};
use crate::state::AppState;

/// How long a call waits for the user's answer. Octoboard's own value, for every agent alike, and
/// kept below the ten minutes the per-session stdio server gives each call it forwards to the
/// daemon (`mcp::stdio`'s `CALL_TIMEOUT`), with room left for an approval given at the last moment
/// to be carried out — launching the console session included — before that forwarding gives up.
/// No agent's own limit on a tool call is measured or followed.
pub const ANSWER_TIME_LIMIT: Duration = Duration::from_secs(8 * 60);

/// How many ended requests are remembered, for telling a late answer what became of its request.
/// Far more than could be on screen at once; the rest are forgotten oldest first.
const REMEMBERED_ENDINGS: usize = 64;

/// What the waiting call is handed when its request is answered: the tool's result, or the reason
/// it failed, written for the model.
type Outcome = std::result::Result<Value, String>;

/// The requests waiting for the user's answer, and how the last few that stopped waiting ended.
#[derive(Default)]
pub struct ConsoleRequests {
    inner: Mutex<Requests>,
}

#[derive(Default)]
struct Requests {
    /// Oldest first.
    waiting: Vec<Waiting>,
    ended: VecDeque<Ended>,
    /// The sessions whose request has been approved and is being carried out. Such a session has
    /// no request waiting, and asking again meanwhile would have two console sessions started for
    /// it.
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
    how: ConsoleRequestEnding,
}

/// What a session hands over with `request_console_session`, shaped like a report: what it needs
/// done, what it has found out, and the separate pieces of work, if any.
pub struct Asked {
    pub request: String,
    pub summary: String,
    pub open_items: Vec<String>,
}

/// One request, as the dialog shows it.
#[derive(Clone)]
struct Request {
    id: String,
    session: String,
    console: String,
    project: Option<String>,
    request: String,
    summary: String,
    open_items: Vec<String>,
    requested_at: i64,
}

impl Request {
    fn event(&self) -> Event {
        Event::ConsoleSessionRequest {
            request_id: self.id.clone(),
            session: self.session.clone(),
            console: self.console.clone(),
            project: self.project.clone(),
            request: self.request.clone(),
            summary: self.summary.clone(),
            open_items: self.open_items.clone(),
            requested_at: self.requested_at,
        }
    }
}

impl Requests {
    /// Takes the requests `matches` picks out of the waiting ones, recording how they ended.
    fn take(
        &mut self,
        matches: impl Fn(&Request) -> bool,
        how: ConsoleRequestEnding,
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

impl ConsoleRequests {
    fn lock(&self) -> std::sync::MutexGuard<'_, Requests> {
        self.inner.lock().expect("console requests lock poisoned")
    }

    /// Holds a new request, in place of one its session already has waiting, which is withdrawn
    /// and returned: a session asking again has stopped waiting for its earlier call, which its
    /// agent may have given up on by a time limit of its own without a word to Octoboard.
    /// Refused while an approval of its session's earlier request is being carried out.
    fn hold(&self, request: Request, answer: oneshot::Sender<Outcome>) -> Result<Vec<Waiting>> {
        let mut requests = self.lock();
        if requests.approving.contains(&request.session) {
            bail!(
                "the user has just approved this session's earlier request for a console session, \
                 and it is being carried out; its outcome arrives in this terminal as a message"
            );
        }
        let replaced = requests.take(
            |waiting| waiting.session == request.session,
            ConsoleRequestEnding::Withdrawn,
        );
        requests.waiting.push(Waiting { request, answer });
        Ok(replaced)
    }

    fn take(&self, matches: impl Fn(&Request) -> bool, how: ConsoleRequestEnding) -> Vec<Waiting> {
        self.lock().take(matches, how)
    }

    /// Takes one request for the user's answer; an approval also marks its session as being
    /// approved, in the same step, until the returned guard is dropped.
    fn take_answered(
        &self,
        request_id: &str,
        how: ConsoleRequestEnding,
    ) -> Option<(Waiting, Option<Approving<'_>>)> {
        let mut requests = self.lock();
        let waiting = requests
            .take(|request| request.id == request_id, how)
            .pop()?;
        let approving = (how == ConsoleRequestEnding::Approved).then(|| {
            requests.approving.insert(waiting.request.session.clone());
            Approving {
                requests: self,
                session: waiting.request.session.clone(),
            }
        });
        Some((waiting, approving))
    }

    /// How a request that is no longer waiting ended, and whose it was, if it is still remembered.
    fn ending(&self, request_id: &str) -> Option<(String, ConsoleRequestEnding)> {
        self.lock()
            .ended
            .iter()
            .find(|ended| ended.request_id == request_id)
            .map(|ended| (ended.session.clone(), ended.how))
    }
}

/// A session's approval being carried out, released on drop however carrying it out ends.
struct Approving<'a> {
    requests: &'a ConsoleRequests,
    session: String,
}

impl Drop for Approving<'_> {
    fn drop(&mut self) {
        self.requests.lock().approving.remove(&self.session);
    }
}

/// The requests to put to a client that has just been sent a snapshot, oldest first. A request is
/// broadcast once, so a client that missed it — it was not connected, or a snapshot replaced what
/// it had — is told here, and one that already has it ignores the repeat.
pub fn pending_events(state: &AppState) -> Vec<Event> {
    state
        .console_requests
        .lock()
        .waiting
        .iter()
        .map(|waiting| waiting.request.event())
        .collect()
}

/// Stops the requests `matches` picks out from waiting and tells every client to close their
/// dialogs. Returns them, so an answer can carry its outcome to the waiting call.
fn close(
    state: &AppState,
    matches: impl Fn(&Request) -> bool,
    how: ConsoleRequestEnding,
) -> Vec<Waiting> {
    let taken = state.console_requests.take(matches, how);
    announce_closed(state, &taken, how);
    taken
}

fn announce_closed(state: &AppState, closed: &[Waiting], how: ConsoleRequestEnding) {
    for waiting in closed {
        state.broadcast(Event::ConsoleSessionRequestClosed {
            request_id: waiting.request.id.clone(),
            reason: how,
        });
    }
}

/// Withdraws whatever the session has waiting, because its process has ended: its call has nobody
/// left to return to. Called wherever a live session is dropped.
pub fn withdraw_for_session(state: &AppState, session_id: &str) {
    close(
        state,
        |request| request.session == session_id,
        ConsoleRequestEnding::Withdrawn,
    );
}

/// Withdraws a request when the call waiting on it is dropped: the call's connection to the daemon
/// was dropped, so its result has nowhere to go. A request that is no longer waiting — answered,
/// timed out, already withdrawn — is left alone, so dropping this after the call has its outcome
/// changes nothing.
struct WithdrawOnDrop<'a> {
    state: &'a AppState,
    request_id: String,
}

impl Drop for WithdrawOnDrop<'_> {
    fn drop(&mut self) {
        close(
            self.state,
            |request| request.id == self.request_id,
            ConsoleRequestEnding::Withdrawn,
        );
    }
}

/// Credits as reported the turn that will deliver the result of a `request_console_session` call
/// the agent has moved to the background, when this hook event of `session_id`'s says it has. An
/// approval's result would otherwise reach the console session as a synthesised report, for the
/// same reason as the outcome message's turn (see [`tell_the_lead_session`]).
///
/// Armed whether or not the request is still waiting — an approval may be under way — and whatever
/// the answer turns out to be: a session left unbound owes no report anyway, and the credit names
/// that one call's task, so it cannot cover any other turn.
pub fn note_backgrounded(
    state: &AppState,
    session_id: &str,
    agent: Agent,
    event: &str,
    payload: &Value,
) {
    if let Some(task) =
        hooks::backgrounded_mcp_call(agent, event, payload, SERVER_KEY, "request_console_session")
    {
        state.credit_turn_opened_by(session_id, hooks::task_notification_opening(task));
    }
}

/// `request_console_session`: asks the user for a console session on the caller's behalf, and waits
/// up to `limit` for the answer, which is what the call returns. The `Err` case is what the model
/// is shown as the reason, so it is written for it.
pub async fn ask(
    state: &Arc<AppState>,
    caller: &Session,
    asked: Asked,
    limit: Duration,
) -> Result<Value> {
    if let Some(owner) = &caller.bound_to {
        // Only a lead session can get here: a session opened bound is not offered the tool.
        bail!(
            "this session is already bound to the console session `{owner}` as its lead \
             session, and a session becomes bound only once. Report to it with `{report}`, and \
             ask it there for what you need done in other projects",
            report = qualified_tool_name(caller.agent, "report"),
        );
    }
    let request = Request {
        id: Uuid::new_v4().to_string(),
        session: caller.id.clone(),
        console: caller.console_id.clone(),
        project: caller.project_id.clone(),
        request: asked.request,
        summary: asked.summary,
        open_items: asked.open_items,
        requested_at: now_millis(),
    };
    let request_id = request.id.clone();
    let event = request.event();
    let (answer, mut answered) = oneshot::channel();
    let replaced = state.console_requests.hold(request, answer)?;
    announce_closed(state, &replaced, ConsoleRequestEnding::Withdrawn);
    // Dropping their answers is what wakes the calls they were.
    drop(replaced);
    state.broadcast(event);
    let _withdraw = WithdrawOnDrop {
        state: state.as_ref(),
        request_id: request_id.clone(),
    };

    let outcome = tokio::select! {
        outcome = &mut answered => outcome,
        () = tokio::time::sleep(limit) => {
            let timed_out = close(
                state,
                |request| request.id == request_id,
                ConsoleRequestEnding::TimedOut,
            );
            if !timed_out.is_empty() {
                bail!(
                    "the user did not respond to this request for a console session within {} \
                     minutes, so it was dropped: nothing was started, and this session is still \
                     unbound. {DO_NOT_ASK_AGAIN}",
                    limit.as_secs() / 60
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
            "this request for a console session was withdrawn before the user answered it: \
             nothing was started"
        ),
    }
}

/// What a call that changed nothing tells the model to do next: stopping there is the expected
/// outcome, and asking again would put the same question in front of the user straight away.
const DO_NOT_ASK_AGAIN: &str = "Do not ask again unless the user tells you to: carry on with what \
                                this project allows, and tell the user in this terminal what is \
                                left that needs other projects";

/// The user's answer to a request, from the dialog its broadcast opened. The first answer wins:
/// the request stops waiting the moment it is taken, so an answer for one already answered, timed
/// out or withdrawn changes nothing and is refused with a code saying which.
///
/// An approval returns the console session it started, which is what the client that approved
/// goes on to show.
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

/// [`answer`], starting the console session through `launch`: `coordinator::open_session` outside
/// the tests, which cannot launch an agent.
async fn answer_with<F, Fut>(
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
        ConsoleRequestEnding::Approved
    } else {
        ConsoleRequestEnding::Refused
    };
    let Some((waiting, _approving)) = state.console_requests.take_answered(request_id, how) else {
        return Err(match state.console_requests.ending(request_id) {
            Some((session, ConsoleRequestEnding::TimedOut | ConsoleRequestEnding::Withdrawn)) => {
                not_waiting(&session)
            }
            _ => CodedError::raised(
                error_code::CONSOLE_REQUEST_ANSWERED,
                "this request for a console session has already been answered",
                &[],
            ),
        });
    };
    announce_closed(state, std::slice::from_ref(&waiting), how);
    if !approve {
        let _ = waiting.answer.send(Err(format!(
            "the user refused this request for a console session: nothing was started, and this \
             session is still unbound. {DO_NOT_ASK_AGAIN}"
        )));
        return Ok(None);
    }
    match carry_out(state, &waiting.request, launch).await {
        Ok(approved) => {
            // The call may be gone by now, so the outcome is also written into the session, after
            // the result so that a call still waiting has it first.
            let _ = waiting.answer.send(Ok(approved.result));
            tell_the_lead_session(state, &approved.lead_id, approved.message).await;
            Ok(Some(approved.console_session))
        }
        Err(err) => {
            let _ = waiting.answer.send(Err(format!(
                "the user approved this request for a console session, but it could not be \
                 carried out: {err:#}"
            )));
            Err(err)
        }
    }
}

/// An approval carried out: the call's result, and the same outcome as a message for the session
/// that is now a lead session.
struct Approved {
    result: Value,
    lead_id: String,
    console_session: Session,
    message: String,
}

/// How an approval's outcome written into the lead session opens, which is what tells the turn it
/// starts from any other.
const OUTCOME_OPENING: &str = "Message from Octoboard about your";

/// Writes an approval's outcome into the lead session through the delivery every message takes.
/// The turn the message starts is credited as reported: the session's request was its report, and
/// a turn spent reading that it already knows this would otherwise reach the console session as a
/// synthesised report.
async fn tell_the_lead_session(state: &Arc<AppState>, lead_id: &str, message: String) {
    state.credit_turn_opened_by(lead_id, OUTCOME_OPENING.to_string());
    let written = {
        let state = state.clone();
        let lead_id = lead_id.to_string();
        // Writing into the session's PTY blocks.
        tokio::task::spawn_blocking(move || {
            reporting::write_message(&state, &lead_id, &message, WhenBlocked::Queue)
        })
        .await
    };
    if let Err(err) = written
        .map_err(anyhow::Error::from)
        .and_then(|written| written)
    {
        state.clear_turn_credit(lead_id, OUTCOME_OPENING);
        tracing::debug!(
            session = %lead_id,
            %err,
            "writing the approved request's outcome into the session failed"
        );
    }
}

/// Carries out an approval: starts a console session in the caller's console as the user opening
/// one by hand would, binds the caller to it as its lead session, and delivers what the caller
/// handed over as its first report.
async fn carry_out<F, Fut>(state: &Arc<AppState>, request: &Request, launch: F) -> Result<Approved>
where
    F: FnOnce(OpenRequest) -> Fut,
    Fut: Future<Output = Result<Session>>,
{
    let caller = state.session_record(&request.session)?;
    // Withdrawn when its process ended, but that can land while the answer is on its way.
    if !running(state, &caller) {
        return Err(not_waiting(&caller.id));
    }
    if caller.bound_to.is_some() {
        bail!("the requesting session is already bound to a session");
    }

    let console_session = launch(OpenRequest {
        console_id: caller.console_id.clone(),
        project_id: None,
        agent: None,
        account: None,
        task: None,
        title: None,
        origin: Origin::User,
        bound_to: None,
    })
    .await?;
    // The caller's process can also end while the console session launches, once nothing withdraws
    // its request any more.
    let bound = match state.session_record(&caller.id) {
        Ok(now) if running(state, &now) => {
            coordinator::bind_to_console_session(state, &caller.id, &console_session.id)
        }
        Ok(_) => Err(not_waiting(&caller.id)),
        Err(err) => Err(err),
    };
    let lead = match bound {
        Ok(lead) => lead,
        Err(err) => {
            // Started for this binding alone, so it does not outlive a binding that failed.
            if let Err(archive_err) = coordinator::archive_session(state, &console_session.id) {
                tracing::warn!(
                    session = %console_session.id,
                    %archive_err,
                    "archiving an unused console session failed"
                );
            }
            return Err(err);
        }
    };

    // The request is this round's report, so the round ending without another is not reported on
    // a second time.
    state.mark_reported(&lead.id);
    let delivered = {
        let state = state.clone();
        let lead_id = lead.id.clone();
        let summary = first_report_summary(request);
        let open_items = request.open_items.clone();
        // Delivery writes into the console session's PTY, which blocks.
        tokio::task::spawn_blocking(move || {
            reporting::deliver_report(
                &state,
                &lead_id,
                Report {
                    summary: &summary,
                    status: ReportStatus::NeedsDecision,
                    open_items: &open_items,
                    synthesised: false,
                },
            )
        })
        .await
    };
    // Nothing after the binding fails the approval: the caller is bound by now, so a first report
    // that was not delivered, for whatever reason, is the caller's to send.
    let first_report = match delivered.map_err(anyhow::Error::from).and_then(|sent| sent) {
        Ok(_) => "Your request was sent to it as your first report.".to_string(),
        Err(err) => {
            state.clear_reported(&lead.id);
            format!(
                "Your request could not be delivered to it ({err:#}), so send it to it yourself \
                 with `{}`.",
                qualified_tool_name(lead.agent, "report")
            )
        }
    };

    let note = outcome_note(&lead, &console_session, &first_report);
    let message = format!(
        "{OUTCOME_OPENING} `{}` call. {note} If that call already returned this, there is nothing \
         more to do about it.",
        qualified_tool_name(lead.agent, "request_console_session")
    );
    Ok(Approved {
        result: json!({
            "console_session": console_session.id,
            "title": console_session.title,
            "note": note,
        }),
        lead_id: lead.id,
        console_session,
        message,
    })
}

/// Whether the session's process is still there to be bound and told.
fn running(state: &AppState, session: &Session) -> bool {
    !session.status.is_dormant() && state.has_process(&session.id)
}

fn not_waiting(session_id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::CONSOLE_REQUEST_NOT_WAITING,
        "this session is no longer waiting for an answer, so no console session was started",
        &[("session", session_id)],
    )
}

/// What a bound session is told of its new binding, in the call's result and in the message alike,
/// with `first_report` saying what became of its request. Its role description, recorded when it
/// was launched unbound, tells it not to call `report`; this is what tells it that it now must, so
/// it says outright that the instruction no longer holds.
fn outcome_note(lead: &Session, console_session: &Session, first_report: &str) -> String {
    let tool = |name: &str| qualified_tool_name(lead.agent, name);
    format!(
        "The user approved the request: Octoboard started the console session `{id}` (\"{title}\") \
         in this console and bound this session to it, as its lead session. {first_report} From \
         now on you report to that console session with `{report}`. This replaces the \
         instruction in your role description not to call `{report}`, which held only while this \
         session was unbound. Report once per round of work: `done` with no open items archives \
         this session, and is refused while a session you started is not archived; \
         `needs_decision` or open items leave it running. The console session dispatches the \
         work in the other projects and instructs you as one of its sessions; you keep the \
         sessions you started in this project and go on driving them with `{start_session}` and \
         the tools that go with it. A session becomes bound only once, so do not call \
         `{request}` again.",
        id = console_session.id,
        title = console_session.title,
        report = tool("report"),
        start_session = tool("start_session"),
        request = tool("request_console_session"),
    )
}

/// The first report's prose: the console session was started with no prompt of its own, so this
/// is also what tells it how the session reporting came to be its own.
fn first_report_summary(request: &Request) -> String {
    format!(
        "This project session asked for a console session because its work needs sessions in \
         other projects, and the user approved it: this console session was started for it, and \
         it is now bound to you as a lead session. It keeps the sessions it started in its own \
         project and drives them itself; you can read those but not drive them. Drive it like \
         any session of yours, and dispatch the work in the other projects.\n\
         \n\
         What it needs done:\n\
         {}\n\
         \n\
         What it has found out:\n\
         {}",
        request.request, request.summary,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::protocol::{Console, Project, ProjectSource, Role, SessionStatus};
    use crate::state::TurnClose;
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{app_state, idle_stand_in, ScratchDir, StandIn, PATIENCE};

    const CONSOLE: &str = "console-1";

    fn project_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: CONSOLE.to_string(),
            project_id: Some("project-1".to_string()),
            host_id: LOCAL_HOST_ID.to_string(),
            role: Role::Project,
            origin: Origin::User,
            title: format!("Work {id}"),
            status: SessionStatus::Idle,
            has_conversation: true,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// A console with one project and an unbound project session in it, `asker`, not running.
    fn fixture(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = app_state(&format!("console-request-{name}"));
        state
            .store
            .insert_console(&Console {
                id: CONSOLE.to_string(),
                name: "Console".to_string(),
                workdir: dir.join("console").to_string_lossy().into_owned(),
                console_session_agent: Agent::Claude,
                default_agent: Agent::Claude,
                claude_account_id: None,
                codex_account_id: None,
                grok_account_id: None,
                icon: None,
                created_at: 0,
            })
            .unwrap();
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: CONSOLE.to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Shop".to_string(),
                path: "/tmp/shop".to_string(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                trust_consent: false,
                pinned: false,
                tags: Vec::new(),
            })
            .unwrap();
        state
            .store
            .insert_session(&project_session("asker"))
            .unwrap();
        (state, dir)
    }

    /// Registers a stand-in process for `id` whose output a test can read.
    fn running(state: &Arc<AppState>, id: &str) -> StandIn {
        let live = idle_stand_in(id);
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        live
    }

    fn output_containing(live: &crate::session::LiveSession, needle: &str) -> String {
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let output = String::from_utf8_lossy(&live.recent_output(8 * 1024)).into_owned();
            if output.contains(needle) || std::time::Instant::now() >= deadline {
                return output;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    const REQUEST: &str = "Add the refund endpoint the shop now calls to the payments service.";
    const SUMMARY: &str = "The shop's checkout calls /refunds, which payments does not serve yet.";
    const OPEN_ITEM: &str = "payments: serve /refunds";

    fn asked() -> Asked {
        Asked {
            request: REQUEST.to_string(),
            summary: SUMMARY.to_string(),
            open_items: vec![OPEN_ITEM.to_string()],
        }
    }

    /// Starts `asker`'s call, waiting `limit`, and returns it once its request is waiting.
    async fn ask_in_background(
        state: &Arc<AppState>,
        caller: &str,
        limit: Duration,
    ) -> (tokio::task::JoinHandle<Result<Value>>, String) {
        let caller_id = caller.to_string();
        let caller = state.session_record(caller).unwrap();
        let task = {
            let state = state.clone();
            tokio::spawn(async move { ask(&state, &caller, asked(), limit).await })
        };
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let waiting = pending_events(state)
                .into_iter()
                .find_map(|event| match event {
                    Event::ConsoleSessionRequest {
                        request_id,
                        session,
                        ..
                    } if session == caller_id => Some(request_id),
                    _ => None,
                });
            if let Some(request_id) = waiting {
                return (task, request_id);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the request never waited"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Stands in for `coordinator::open_session`, which would launch a real agent: inserts a
    /// console session as one opened by hand with a stand-in process, and counts the launches.
    struct Launcher {
        state: Arc<AppState>,
        launched: AtomicUsize,
        hubs: Mutex<Vec<StandIn>>,
    }

    impl Launcher {
        fn new(state: &Arc<AppState>) -> Arc<Self> {
            Arc::new(Self {
                state: state.clone(),
                launched: AtomicUsize::new(0),
                hubs: Mutex::new(Vec::new()),
            })
        }

        async fn open(self: Arc<Self>, request: OpenRequest) -> Result<Session> {
            // Exactly the console session the user opens by hand: no project, no prompt, no owner.
            assert!(request.project_id.is_none() && request.task.is_none());
            assert!(request.bound_to.is_none() && request.origin == Origin::User);
            let n = self.launched.fetch_add(1, Ordering::SeqCst);
            let session = self.state.store.insert_console_session(
                Session {
                    id: format!("hub-{n}"),
                    role: Role::Console,
                    project_id: None,
                    ..project_session("hub")
                },
                None,
            )?;
            let live = running(&self.state, &session.id);
            self.hubs.lock().unwrap().push(live);
            Ok(session)
        }

        fn hub_output(&self, needle: &str) -> String {
            output_containing(&self.hubs.lock().unwrap()[0], needle)
        }
    }

    async fn answer_by(
        launcher: &Arc<Launcher>,
        request_id: &str,
        approve: bool,
    ) -> Result<Option<Session>> {
        let open = |request| launcher.clone().open(request);
        answer_with(&launcher.state, request_id, approve, open).await
    }

    /// A call's outcome, waited for no longer than `PATIENCE`.
    async fn outcome_of(call: tokio::task::JoinHandle<Result<Value>>) -> Result<Value> {
        tokio::time::timeout(PATIENCE, call)
            .await
            .expect("the call returns in time")
            .unwrap()
    }

    fn code_of(err: &anyhow::Error) -> Option<&'static str> {
        err.downcast_ref::<CodedError>().map(|coded| coded.code)
    }

    /// Approval starts a console session as the user would open one, binds the caller to it as its
    /// lead session, delivers the request as its first report, and tells the caller in its result
    /// that it now reports there with `report`.
    #[tokio::test]
    async fn an_approval_starts_a_console_session_and_binds_the_caller_to_it() {
        let (state, _dir) = fixture("approve");
        let _asker = running(&state, "asker");
        let launcher = Launcher::new(&state);
        let mut events = state.subscribe();
        let (call, request_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;

        // The console session is returned too, for the client that approved to show it.
        let started = answer_by(&launcher, &request_id, true).await.unwrap();
        assert_eq!(started.map(|session| session.id).as_deref(), Some("hub-0"));
        let result = outcome_of(call).await.unwrap();
        assert_eq!(result["console_session"], "hub-0");
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains("report to that console session with `mcp__octoboard__report`"),
            "{note}"
        );
        assert!(note.contains("replaces the instruction"), "{note}");

        let lead = state.session_record("asker").unwrap();
        assert_eq!(lead.bound_to.as_deref(), Some("hub-0"));
        assert!(lead.lead);
        let report = launcher.hub_output(OPEN_ITEM);
        assert!(report.contains("Report from session asker"), "{report}");
        assert!(report.contains("Status: needs_decision"), "{report}");
        assert!(report.contains("refund endpoint"), "{report}");

        let mut closed = None;
        while let Ok(event) = events.try_recv() {
            if let Event::ConsoleSessionRequestClosed { reason, .. } = event {
                closed = Some(reason);
            }
        }
        assert_eq!(closed, Some(ConsoleRequestEnding::Approved));
    }

    /// The call may have ended on the agent's side without any signal reaching Octoboard, so an
    /// approval also writes its outcome into the caller's session, whatever became of the result.
    #[tokio::test]
    async fn an_approval_also_writes_its_outcome_into_the_callers_session() {
        let (state, _dir) = fixture("approve-message");
        let asker = running(&state, "asker");
        let launcher = Launcher::new(&state);
        let (_unread_call, request_id) =
            ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;

        answer_by(&launcher, &request_id, true).await.unwrap();
        // Its beginning: the stand-in's terminal is in canonical mode, which echoes no more than a
        // line's worth of what is written into it.
        let message = output_containing(&asker, "mcp__octoboard__report");
        assert!(message.contains("Message from Octoboard"), "{message}");
        assert!(message.contains("`hub-0`"), "{message}");
    }

    /// A call Claude Code moved to the background has its result delivered as a turn of its own,
    /// which is credited as the outcome message's is. Payload and prompt are the shapes Claude Code
    /// 2.1.295 produced.
    #[test]
    fn the_turn_that_delivers_a_backgrounded_calls_result_is_credited() {
        let (state, _dir) = app_state("console-request-backgrounded");
        let paused = json!({"background_tasks": [{
            "id": "kdn46xv43",
            "type": "MCP task",
            "status": "running",
            "description": "octoboard/request_console_session",
            "server": "octoboard",
            "tool": "request_console_session",
        }]});
        note_backgrounded(&state, "asker", Agent::Claude, "Stop", &paused);

        let notification = json!({"prompt": "<task-notification>\n<task-id>kdn46xv43</task-id>\n\
            <status>completed</status>\n<summary>MCP task kdn46xv43 \
            (octoboard/request_console_session) completed.</summary>"});
        let prompt = hooks::submitted_prompt(Agent::Claude, &notification);
        state.turn_started("asker", prompt);
        assert_eq!(state.close_turn("asker", false), TurnClose::Reported);
    }

    /// A refusal is a tool error in fixed prose, and nothing is started or bound.
    #[tokio::test]
    async fn a_refusal_changes_nothing() {
        let (state, _dir) = fixture("refuse");
        let launcher = Launcher::new(&state);
        let (call, request_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;

        answer_by(&launcher, &request_id, false).await.unwrap();
        let err = outcome_of(call).await.expect_err("refused");
        assert!(err.to_string().contains("the user refused"), "{err}");
        assert_eq!(launcher.launched.load(Ordering::SeqCst), 0);
        assert!(state.session_record("asker").unwrap().bound_to.is_none());
        assert!(pending_events(&state).is_empty());
    }

    /// A binding that fails after the console session was launched leaves the session unbound, and
    /// the console session, started for that binding alone, is archived rather than left behind.
    #[tokio::test]
    async fn a_failed_binding_archives_the_console_session_it_started() {
        let (state, _dir) = fixture("bind-fails");
        let _asker = running(&state, "asker");
        let (call, request_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;

        // A console session with no process, which a session cannot be bound to.
        let insert = |_request| {
            let session = Session {
                id: "hub-0".to_string(),
                role: Role::Console,
                project_id: None,
                ..project_session("hub")
            };
            let state = state.clone();
            async move { state.store.insert_console_session(session, None) }
        };
        let err = answer_with(&state, &request_id, true, insert)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not running"), "{err}");
        let reason = outcome_of(call).await.expect_err("not carried out");
        assert!(
            reason.to_string().contains("could not be carried out"),
            "{reason}"
        );
        assert!(state.session_record("asker").unwrap().bound_to.is_none());
        let hub = state.session_record("hub-0").unwrap();
        assert_eq!(hub.status, SessionStatus::Archived);
    }

    /// While a session's approval is being carried out it has no request waiting, and a call it
    /// makes then is refused rather than putting a second console session in front of the user.
    #[tokio::test]
    async fn a_call_made_while_its_approval_is_carried_out_is_refused() {
        let (state, _dir) = fixture("approving");
        let _asker = running(&state, "asker");
        let (_call, request_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;
        let caller = state.session_record("asker").unwrap();

        // Held in the launch, which is where an approval spends its time.
        let (entered, entered_rx) = oneshot::channel();
        let (release, released) = oneshot::channel::<()>();
        let launch = |_request| async move {
            let _ = entered.send(());
            let _ = released.await;
            Err(anyhow!("not launched"))
        };
        let approval = tokio::spawn({
            let state = state.clone();
            async move { answer_with(&state, &request_id, true, launch).await }
        });
        entered_rx.await.unwrap();

        let err = ask(&state, &caller, asked(), ANSWER_TIME_LIMIT)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("being carried out"), "{err}");
        assert!(pending_events(&state).is_empty());
        release.send(()).unwrap();
        approval.await.unwrap().unwrap_err();
    }

    /// With no answer within the limit the request is dropped and the call says the user did not
    /// respond; an approval after that starts nothing and says the session is no longer waiting.
    #[tokio::test]
    async fn no_answer_within_the_limit_drops_the_request() {
        let (state, _dir) = fixture("time-limit");
        let launcher = Launcher::new(&state);
        let (call, request_id) =
            ask_in_background(&state, "asker", Duration::from_millis(50)).await;

        let err = outcome_of(call).await.expect_err("timed out");
        assert!(err.to_string().contains("did not respond"), "{err}");
        assert!(pending_events(&state).is_empty());
        let late = answer_by(&launcher, &request_id, true).await.unwrap_err();
        assert_eq!(
            code_of(&late),
            Some(error_code::CONSOLE_REQUEST_NOT_WAITING)
        );
        assert_eq!(launcher.launched.load(Ordering::SeqCst), 0);
    }

    /// The caller's process ending withdraws its request; an approval after that starts nothing
    /// and is refused with the code the dialog's toast is shown for.
    #[tokio::test]
    async fn the_callers_process_ending_withdraws_its_request() {
        let (state, _dir) = fixture("process-ends");
        let asker = idle_stand_in("asker");
        state.register_live(asker.clone());
        state.watch_exit(asker.clone());
        let launcher = Launcher::new(&state);
        let (call, request_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;

        asker.terminate();
        let err = outcome_of(call).await.expect_err("withdrawn");
        assert!(err.to_string().contains("withdrawn"), "{err}");
        let late = answer_by(&launcher, &request_id, true).await.unwrap_err();
        assert_eq!(
            code_of(&late),
            Some(error_code::CONSOLE_REQUEST_NOT_WAITING)
        );
        assert_eq!(launcher.launched.load(Ordering::SeqCst), 0);
        assert!(state.session_record("asker").unwrap().bound_to.is_none());
    }

    /// Two clients answering the same request at once: the first answer taken is carried out,
    /// once, and the other is refused as already answered.
    #[tokio::test]
    async fn the_first_answer_wins() {
        let (state, _dir) = fixture("first-answer");
        let _asker = running(&state, "asker");
        let launcher = Launcher::new(&state);
        let (call, request_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;

        // Polled in this order, so the approval is the one taken first.
        let (approval, refusal) = tokio::join!(
            answer_by(&launcher, &request_id, true),
            answer_by(&launcher, &request_id, false),
        );
        approval.unwrap();
        let lost = refusal.unwrap_err();
        assert_eq!(code_of(&lost), Some(error_code::CONSOLE_REQUEST_ANSWERED));
        assert!(outcome_of(call).await.is_ok());
        assert_eq!(launcher.launched.load(Ordering::SeqCst), 1);
    }

    /// A session asking again has stopped waiting for its earlier call, which its agent may have
    /// given up on without a word: the new request takes the earlier one's place.
    #[tokio::test]
    async fn a_second_call_takes_the_place_of_the_first() {
        let (state, _dir) = fixture("second-call");
        let (first, first_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;
        let caller = state.session_record("asker").unwrap();
        let _second = {
            let state = state.clone();
            tokio::spawn(async move { ask(&state, &caller, asked(), ANSWER_TIME_LIMIT).await })
        };

        let err = outcome_of(first).await.expect_err("withdrawn");
        assert!(err.to_string().contains("withdrawn"), "{err}");
        let waiting: Vec<String> = pending_events(&state)
            .into_iter()
            .filter_map(|event| match event {
                Event::ConsoleSessionRequest { request_id, .. } => Some(request_id),
                _ => None,
            })
            .collect();
        assert_eq!(waiting.len(), 1);
        assert_ne!(waiting[0], first_id);
    }

    async fn next_event(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Event {
        tokio::time::timeout(PATIENCE, events.recv())
            .await
            .expect("an event in time")
            .unwrap()
    }

    /// Over the real router: a call whose connection is dropped withdraws its request, and the
    /// requests still waiting are sent to a client right after its snapshot, oldest first.
    #[tokio::test]
    async fn a_dropped_call_is_withdrawn_and_waiting_requests_follow_the_snapshot() {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let (state, _dir) = fixture("router");
        for id in ["second", "dropped"] {
            state.store.insert_session(&project_session(id)).unwrap();
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let router = crate::server::router(state.clone());
        tokio::spawn(async move { axum::serve(listener, router).await });

        // Withdrawn when its connection goes.
        let mut events = state.subscribe();
        let token = state.issue_mcp_token("dropped");
        let arguments =
            json!({ "request": REQUEST, "summary": SUMMARY, "open_items": [OPEN_ITEM] });
        let body = json!({ "tool": "request_console_session", "arguments": arguments });
        let body = body.to_string();
        let mut call = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        call.write_all(
            format!(
                "POST /mcp/{token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        assert!(matches!(
            next_event(&mut events).await,
            Event::ConsoleSessionRequest { .. }
        ));
        drop(call);
        assert!(matches!(
            next_event(&mut events).await,
            Event::ConsoleSessionRequestClosed {
                reason: ConsoleRequestEnding::Withdrawn,
                ..
            }
        ));

        // Sent again after a snapshot, oldest first.
        let (_first, first_id) = ask_in_background(&state, "asker", ANSWER_TIME_LIMIT).await;
        let (_second, second_id) = ask_in_background(&state, "second", ANSWER_TIME_LIMIT).await;
        let (socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control"))
                .await
                .unwrap();
        let (_sink, mut stream) = socket.split();
        let mut types = Vec::new();
        let mut requests = Vec::new();
        while requests.len() < 2 {
            let frame = tokio::time::timeout(PATIENCE, stream.next())
                .await
                .expect("a frame in time")
                .unwrap()
                .unwrap();
            let tokio_tungstenite::tungstenite::Message::Text(text) = frame else {
                continue;
            };
            let event: Value = serde_json::from_str(&text).unwrap();
            types.push(event["type"].as_str().unwrap().to_string());
            if event["type"] == "console_session_request" {
                requests.push(event["request_id"].as_str().unwrap().to_string());
            }
        }
        assert_eq!(types[0], "snapshot");
        assert_eq!(requests, [first_id, second_id]);
    }
}
