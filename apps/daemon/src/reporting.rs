//! The channel between an owner — a console session, or a project session that started sessions,
//! unbound or a lead session — and the project sessions bound to it: the brief a task is handed
//! over as, the message writing that carries both directions, the report that comes back, and the
//! report Octoboard synthesises when a session stops without having sent one.
//!
//! That is the one concern the console session's tools, the project session's `report` tool and
//! the hook callback all share, which is why it sits apart from the control-socket handling in
//! `coordinator`.
//!
//! **Reporting is never forced.** Gating the stop through the `Stop` hook was measured to work, but
//! every gated turn shows the user an error-styled line the agent will not suppress, and the model
//! reads the injected demand as prompt injection often enough to matter. So the `report` tool is
//! encouraged and nothing is blocked; a session that stops without it has the turn's last assistant
//! message read as its report instead, with the structured fields marked as Octoboard's guess. The
//! conditions that qualify that synthesis are in the "Claude Code" and "Grok Build" sections of
//! `docs/agent-cli-reference.md` and encoded in `crate::hooks`.

use std::sync::Arc;

use anyhow::{bail, Result};

use crate::branch_origin::cleanup;
use crate::outbox::Drain;
use crate::protocol::{error_code, CodedError, Session, SessionStatus};
use crate::state::{AppState, TurnClose};
use crate::{coordinator, hooks, mcp, sharing, term};

/// How every notice Octoboard writes into a session begins, worded after it at several places. A
/// macro so that a constant can be built on it with `concat!`.
macro_rules! notice_prefix {
    () => {
        "Message from Octoboard"
    };
}
pub(crate) use notice_prefix;

/// [`notice_prefix!`] as a value.
pub(crate) const NOTICE_PREFIX: &str = notice_prefix!();

/// How [`render_brief`] begins.
pub(crate) const BRIEF_OPENING: &str = "## Goal";

/// How [`render_report`] begins.
pub(crate) const REPORT_OPENING: &str = "Report from session ";

/// How [`render_page_submission`] begins.
pub(crate) const PAGE_SUBMISSION_OPENING: &str = "Report panel form submission";

/// What a session is handed as its opening prompt, rendered from the console session's `brief`.
///
/// A fixed template rather than something the console session composes, so what a session is
/// handed does not vary with the console session's mood; a field the console session left out is
/// omitted entirely rather than sent as an empty heading, which would tell the session there was
/// something to say and then say nothing.
pub fn render_brief(
    goal: &str,
    context: Option<&str>,
    acceptance: Option<&str>,
    constraints: Option<&str>,
) -> String {
    let mut prompt = format!("{BRIEF_OPENING}\n\n{}\n", goal.trim());
    for (heading, body) in [
        ("Context", context),
        ("Acceptance", acceptance),
        ("Constraints", constraints),
    ] {
        if let Some(body) = body.map(str::trim).filter(|body| !body.is_empty()) {
            prompt.push_str(&format!("\n## {heading}\n\n{body}\n"));
        }
    }
    prompt
}

/// What became of a message handed to [`write_message`].
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
/// **Blocks** on the PTY write and, for an idle Claude Code session, on the few seconds' wait for
/// its agent to send it (twice, for a message a dialog discarded and that is written again);
/// callers on the runtime are responsible for keeping it off a worker.
pub fn write_message(
    state: &Arc<AppState>,
    id: &str,
    text: &str,
    when_blocked: WhenBlocked,
) -> Result<Delivery> {
    // Written, it would be an empty prompt the agent does not send, and nobody would learn why.
    if term::is_blank_once_stripped(text) {
        return Err(CodedError::raised(
            error_code::FIELD_REQUIRED,
            "the message is empty once its control and invisible characters are removed",
            &[("field", "text")],
        ));
    }
    let session = state.session_record(id)?;
    // Held from the claim on until the resume, so queued rather than refused whatever state its
    // process is in meanwhile: the account's hold keeps the queue and the resumed process gets it,
    // or its drop reports the queue as dropped when the session is not relaunched.
    if state.queue_if_restarting(id, text).is_some() {
        return Ok(Delivery::Queued);
    }
    // Nothing to write to and nothing to wait for: a resume starts the agent at its prompt rather
    // than replaying a queue, so queuing here would lose the message silently.
    if session.status.is_dormant() || state.live_session(id).is_none() {
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
    let ticket = state.queue_message(id, text);
    match state.flush_outbox(id, agent_status) {
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
        Drain::Lost => Err(CodedError::raised(
            error_code::QUEUED_MESSAGES_LOST,
            lost_message(),
            &[],
        )),
    }
}

/// A report from a bound project session, as its owner reads it.
pub struct Report<'a> {
    pub summary: &'a str,
    pub status: ReportStatus,
    pub open_items: &'a [String],
    /// True when Octoboard built this report from the turn's last assistant message rather than the
    /// session sending one, which the owner has to know: the structured fields are then a guess
    /// and only the prose is the session's own.
    pub synthesised: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportStatus {
    Done,
    Failed,
    NeedsDecision,
}

impl ReportStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "needs_decision" => Some(Self::NeedsDecision),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Failed => "failed",
            Self::NeedsDecision => "needs_decision",
        }
    }
}

/// Delivers one project session's report to the session it is bound to — a console session, or the
/// project session that started it — and wraps the session up when the report says there is
/// nothing left.
///
/// A lead session's `done` report with no open items is refused while any session bound to it is
/// not archived: its sessions archive themselves when they finish, so one that is not archived has
/// not finished, and neither has the lead session. Its other reports go through as any bound
/// session's do.
///
/// Archiving happens once the report has been *accepted* for the owner rather than once the owner
/// has read it: an owner that is merely busy still has the report queued for it, and leaving a
/// finished session alive until the owner gets round to it would strand it. An owner that is not
/// running at all is a different matter — the report fails, and the session stays as it is for the
/// user to deal with. So is a report written into the owner but left unsent by its agent: the owner
/// has not read it and nothing guarantees it will, so the session is not archived.
pub fn deliver_report(
    state: &Arc<AppState>,
    session_id: &str,
    report: Report<'_>,
) -> Result<String> {
    // What it archived is dropped, its branches with it: the one caller that archives in earnest,
    // a session's own `done` report, goes through `deliver_own_report`, which removes them, and a
    // synthesised report is never `done`.
    deliver_and_archive(state, session_id, report).map(|(note, ..)| note)
}

/// [`deliver_report`], also returning what became of the message and the sessions it archived,
/// for the removal of the branches bound to them once their processes have ended, which is the
/// caller's. The answer names the bound branches and worktrees that removal will leave, judged
/// before the archive, since it has to reach the session before its process is ended.
fn deliver_and_archive(
    state: &Arc<AppState>,
    session_id: &str,
    report: Report<'_>,
) -> Result<(String, Vec<Session>, Delivery)> {
    let _removal = state.claim_session_removal();
    let session = state.session_record(session_id)?;
    let Some(bound_to) = &session.bound_to else {
        bail!("this session is unbound, so there is nobody to report to");
    };
    // A session that reported `done` is archived by the time a second report could arrive, and its
    // process is on its way out. Refusing is what keeps that race from delivering the same round of
    // work to the owner twice.
    if session.status.is_dormant() {
        bail!("this session has already been wrapped up; there is nothing further to report");
    }
    let finished = report.status == ReportStatus::Done
        && report.open_items.is_empty()
        && coordinator::ensure_unlocked(
            std::iter::once(session.clone()).chain(coordinator::team(state, &session)?),
        )
        .is_ok();
    if finished {
        let unfinished: Vec<Session> = coordinator::bound_sessions(state, &session)?
            .into_iter()
            .filter(|bound| bound.status != SessionStatus::Archived)
            .collect();
        if !unfinished.is_empty() {
            let named = coordinator::session_labels(&unfinished);
            bail!(
                "a `done` report with no open items is refused while sessions bound to you are \
                 not archived: {named}. Each archives itself when it reports it is done; wait for \
                 them, archive them, or report with open items instead"
            );
        }
    }

    // The binding names the owner directly, so this is a plain lookup by id rather than a search
    // for "the" console session of the console — a console may hold any number of them, each with
    // its own sessions.
    let owner = state
        .store
        .get_session(bound_to)?
        .ok_or_else(|| anyhow::anyhow!("this session's owner is no longer on record"))?;
    let owner_kind = format!("{} session", owner.role.kind());

    let project = match &session.project_id {
        Some(id) => state.store.get_project(id)?.map(|project| project.name),
        None => None,
    };
    let message = render_report(&session, project.as_deref(), &report);
    // A lost message propagates rather than being treated as delivered: the session must not be
    // archived on the strength of a report its owner never got.
    let delivery = write_message(state, &owner.id, &message, WhenBlocked::Queue)?;

    // A report the owner has not read archives nothing: the session would be gone before its
    // owner, or the user, ever saw why.
    let held_back = finished && delivery == Delivery::Unsent;
    let finished = finished && !held_back;
    let (left, archived) = if finished {
        let left = cleanup::will_be_left(state, std::slice::from_ref(&session));
        (left, coordinator::archive_session(state, session_id)?)
    } else {
        (Vec::new(), Vec::new())
    };

    let mut note = match (delivery, finished) {
        (Delivery::Written, true) => {
            format!("Reported to the {owner_kind}. This session is now archived.")
        }
        (Delivery::Written, false) => format!("Reported to the {owner_kind}."),
        (Delivery::Queued, true) => format!(
            "Report accepted; the {owner_kind} will see it as soon as it can take a message. \
             This session is now archived."
        ),
        (Delivery::Queued, false) => format!(
            "Report accepted; the {owner_kind} will see it as soon as it can take a message."
        ),
        (Delivery::Unsent, _) => format!(
            "Your report was written into the {owner_kind}'s input, but its agent did not send it \
             to the conversation, so the {owner_kind} has not read it; the user has been told. Do \
             not send it again.{}",
            if held_back {
                " This session was not archived, because its owner has not read the report."
            } else {
                ""
            }
        ),
    };
    if !left.is_empty() {
        note.push_str(&format!(
            " These git branches and worktrees bound to this session will not be removed: {}.",
            cleanup::lines(&left).join("; ")
        ));
    }
    Ok((note, archived, delivery))
}

/// Closes the session's open turn on `turn` ending it, and has a report synthesised for the owner
/// when the session reported nothing itself. What closing amounted to is returned, for the caller
/// to decide what else the ending says about the session.
pub fn close_turn_and_report(
    state: &Arc<AppState>,
    session_id: &str,
    turn: hooks::TurnEnd,
) -> TurnClose {
    let close = state.close_turn(session_id, turn.backstop);
    // The turn stopped on an error, which may be the plan's usage limit; whether it was, and
    // whether to do anything about it, is the switch's to judge.
    if turn.failed && matches!(close, TurnClose::OwesReport | TurnClose::Reported) {
        crate::auto_switch::after_failed_turn(state, session_id);
    }
    if close == TurnClose::OwesReport {
        let state = state.clone();
        let session_id = session_id.to_string();
        // Delivery writes into the owner's PTY, which blocks, so it goes off the caller's path.
        tokio::task::spawn_blocking(move || synthesise_report(&state, &session_id, turn));
    }
    close
}

/// Reports to the owner for a session that stopped without reporting for itself. The
/// caller has already closed the turn and established that a report is owed.
///
/// Nothing is reported for a lead session while a session bound to it has a process: it has
/// stopped to wait for its sessions, not stopped working.
///
/// **Blocks** on writing into the owner.
fn synthesise_report(state: &Arc<AppState>, session_id: &str, turn: hooks::TurnEnd) {
    let session = match state.store.get_session(session_id) {
        Ok(Some(session)) if session.bound_to.is_some() => session,
        // An unbound session has nobody to report to, and a record that is gone is nothing to
        // report about.
        _ => return,
    };
    match coordinator::bound_sessions(state, &session) {
        Ok(bound) if bound.iter().all(|bound| !state.has_process(&bound.id)) => {}
        Ok(_) => return,
        Err(err) => {
            tracing::debug!(
                session = %session_id,
                %err,
                "reading the sessions bound to a session failed"
            );
            return;
        }
    }

    let summary = turn.last_assistant_message.unwrap_or_else(|| {
        "This session's turn ended and it said nothing. Check it with `get_session`.".to_string()
    });
    let report = Report {
        summary: &summary,
        // A turn that ended in an error failed; a turn that merely ended without a report is the
        // owner's to judge, which is what `needs_decision` asks it to do.
        status: if turn.failed {
            ReportStatus::Failed
        } else {
            ReportStatus::NeedsDecision
        },
        open_items: &[],
        synthesised: true,
    };
    if let Err(err) = deliver_report(state, session_id, report) {
        tracing::debug!(session = %session_id, %err, "delivering a synthesised report failed");
    }
}

/// Tells the owner of `session` that its process ended without anyone asking it to, so that the
/// work it was doing is not waited for. Nothing is written when it has no owner or the owner has
/// no process to be told in.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_it_ended(state: &Arc<AppState>, session: &Session) {
    tell_owner(state, session, |owner| {
        format!(
            "{NOTICE_PREFIX}: {session} ended on its own: its process exited although nobody had \
             archived it, interrupted it or moved it to another account, whether it crashed or the \
             user closed it in its terminal. It is recorded as interrupted, and the work it was \
             doing stopped with it. Look at it with `{get_session}`, and relaunch it with \
             `{reopen_session}` if the work should go on.",
            session = sharing::named_session(session),
            get_session = mcp::qualified_tool_name(owner.agent, "get_session"),
            reopen_session = mcp::qualified_tool_name(owner.agent, "reopen_session"),
        )
    });
}

/// [`tell_owner_it_waits`] on a blocking thread, for a caller on the runtime that has just seen
/// the session wait for the user by a route other than a hook: a prompt on its terminal, a
/// dialog of its agent, or a request of its own.
pub fn spawn_tell_owner_it_waits(state: &Arc<AppState>, id: &str) {
    let (state, id) = (state.clone(), id.to_string());
    tokio::task::spawn_blocking(move || tell_owner_it_waits(&state, &id));
}

/// Tells the owner of `session` that Octoboard's own switch of it to another account failed after
/// its process was ended, and that bringing it back up on the account it had failed too, so that it
/// is interrupted and the work it was doing stopped. Nothing is written when it has no owner or the
/// owner has no process to be told in.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_the_switch_failed(state: &Arc<AppState>, session: &Session, reason: &str) {
    tell_owner(state, session, |owner| {
        format!(
            "{NOTICE_PREFIX}: {session} reached the usage limit of its account, and Octoboard \
             could neither switch it to another account nor bring it back up on the one it had \
             ({reason}). It is interrupted, and the work it was doing stopped with it. Look at it \
             with `{get_session}`, and relaunch it with `{reopen_session}` if the work should go \
             on.",
            session = sharing::named_session(session),
            get_session = mcp::qualified_tool_name(owner.agent, "get_session"),
            reopen_session = mcp::qualified_tool_name(owner.agent, "reopen_session"),
        )
    });
}

/// Tells the owner of the session `id` that it waits for the user, which no report says: the
/// fallback report at the end of a turn is held back while the session waits, and a report it sent
/// earlier in the turn does not say it then stopped for a person. Once a turn (see
/// [`AppState::claim_waiting_notice`]). Nothing when the session is not shown as waiting by now,
/// by whatever route.
///
/// **Blocks** on writing into the owner.
pub fn tell_owner_it_waits(state: &Arc<AppState>, id: &str) {
    let session = match state.store.get_session(id) {
        Ok(Some(session))
            if session.bound_to.is_some()
                && state.shown_status(&session) == SessionStatus::WaitingUser =>
        {
            session
        }
        _ => return,
    };
    if !state.claim_waiting_notice(id) {
        return;
    }
    tell_owner(state, &session, |_| {
        format!(
            "{NOTICE_PREFIX}: {session} is waiting for the user: it is at a permission prompt or \
             has asked the user something, and only they can clear it. Do not chase it or send it \
             anything to get it going: a message to it is held until they are done. Carry on with \
             the rest of your work.",
            session = sharing::named_session(&session),
        )
    });
}

/// Writes the notice `text` builds, for the owner it is given, into the owner of `session`. A
/// failure is logged and goes no further: an owner that cannot take it now is not owed it later.
fn tell_owner(state: &Arc<AppState>, session: &Session, text: impl FnOnce(&Session) -> String) {
    let Some(owner_id) = &session.bound_to else {
        return;
    };
    let owner = match state.store.get_session(owner_id) {
        Ok(Some(owner)) => owner,
        _ => return,
    };
    if let Err(err) = write_message(state, &owner.id, &text(&owner), WhenBlocked::Queue) {
        tracing::debug!(session = %session.id, owner = %owner.id, %err, "telling the owner failed");
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

/// Delivers a report the session itself sent, for a caller on the runtime: what the `report` tool
/// does, and what a session binding itself to an owner does with its summary.
///
/// The session is credited with having reported before delivery, even if its owner cannot take the
/// message yet, so its stop does not also produce a synthesised report. The credit is given after
/// the session's queued hooks, whose turn start would otherwise reset it, and is taken back
/// whenever delivery failed, so the owner gets a synthesised report instead of neither. A failure
/// after the owner already has the report would make that a duplicate, which is the better way
/// round.
pub async fn deliver_own_report(
    state: &Arc<AppState>,
    session_id: &str,
    summary: String,
    status: ReportStatus,
    open_items: Vec<String>,
) -> Result<(String, Delivery)> {
    state.hook_queues.settled(session_id).await;
    state.mark_reported(session_id);
    let delivered = {
        let state = state.clone();
        let session_id = session_id.to_string();
        // Delivery writes into the owner's PTY, which blocks.
        tokio::task::spawn_blocking(move || {
            deliver_and_archive(
                &state,
                &session_id,
                Report {
                    summary: &summary,
                    status,
                    open_items: &open_items,
                    synthesised: false,
                },
            )
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|delivered| delivered)
    };
    let (note, archived, delivery) = delivered.inspect_err(|_| state.clear_reported(session_id))?;
    cleanup::remove_in_background(state, archived);
    Ok((note, delivery))
}

/// The report as it is written into the owner. Plain prose with the structured fields spelled out:
/// the owner reads this as a user message, so it has to be readable rather than a payload, and the
/// session id has to be in it or the owner cannot follow up.
fn render_report(session: &Session, project: Option<&str>, report: &Report<'_>) -> String {
    let origin = match project.filter(|project| *project != session.title) {
        Some(project) => format!("{} ({})", session.title, project),
        None => session.title.clone(),
    };
    let mut message = format!(
        "{REPORT_OPENING}{} — {origin}\nStatus: {}",
        session.id,
        report.status.label()
    );
    if report.synthesised {
        message.push_str(
            "\nThis session stopped without reporting, so the text below is its last message and \
             the status is Octoboard's guess, not its own.",
        );
    }
    if !report.open_items.is_empty() {
        message.push_str("\nOpen items:");
        for item in report.open_items {
            message.push_str(&format!("\n- {item}"));
        }
    }
    message.push_str("\n\n");
    message.push_str(report.summary.trim());
    message
}

/// What a report panel form submission is written into the console session as. Plain prose, like
/// [`render_report`]: a header line naming the page, then the submitted data.
///
/// `data` is arbitrary JSON from the form, so it is rendered readably only in the shape a form
/// normally takes — an object of scalar values, one `key: value` line each — and falls back to
/// pretty-printed JSON for anything else, rather than guessing at a layout for nested data.
pub(crate) fn render_page_submission(page_id: &str, data: &serde_json::Value) -> String {
    let mut message = format!("{PAGE_SUBMISSION_OPENING} — page {page_id}\n\n");
    match data.as_object().filter(|fields| {
        !fields.is_empty()
            && fields
                .values()
                .all(|value| !value.is_object() && !value.is_array())
    }) {
        Some(fields) => {
            for (key, value) in fields {
                let shown = match value {
                    serde_json::Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                message.push_str(&format!("{key}: {shown}\n"));
            }
        }
        None => {
            message
                .push_str(&serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string()));
        }
    }
    message.trim_end().to_string()
}

/// The session title a dispatched brief earns, taken from its goal. Several sessions dispatched
/// into one project are otherwise all named for the project and indistinguishable in the menu.
pub fn title_from_goal(goal: &str) -> String {
    const MAX: usize = 48;
    let first_line = goal
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first_line.chars().count() <= MAX {
        return first_line.to_string();
    }
    // Cut on a character boundary, and on a word where there is one close enough to the limit.
    let truncated: String = first_line.chars().take(MAX).collect();
    match truncated.rfind(' ') {
        Some(at) if at >= MAX / 2 => format!("{}…", &truncated[..at]),
        _ => format!("{truncated}…"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::protocol::{Agent, Origin, Role, SessionStatus};

    fn project_session(title: &str) -> Session {
        Session {
            id: "session-7".to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: "local".to_string(),
            role: Role::Project,
            origin: Origin::Console,
            title: title.to_string(),
            status: SessionStatus::Idle,
            terminal_title: None,
            record_titles: Default::default(),
            background_tasks: Vec::new(),
            switching_account: false,
            default_effort: None,
            context: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: true,
            bound_to: Some("console-session-1".to_string()),
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            locked: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// A field the console session left out is omitted entirely: an empty heading tells the
    /// session there was something to say and then says nothing.
    #[test]
    fn a_brief_omits_the_sections_it_was_given_nothing_for() {
        let full = render_brief(
            "ship it",
            Some("background"),
            Some("tests pass"),
            Some("do not push"),
        );
        for heading in ["## Goal", "## Context", "## Acceptance", "## Constraints"] {
            assert!(full.contains(heading), "{heading} missing from {full}");
        }

        let bare = render_brief("ship it", None, Some("   "), None);
        assert!(bare.contains("## Goal"));
        assert!(!bare.contains("## Context"));
        // Whitespace is nothing to say either.
        assert!(!bare.contains("## Acceptance"));
    }

    /// The console session follows a session up by id, so the id has to be in the message it reads.
    #[test]
    fn a_report_names_the_session_its_status_and_what_is_open() {
        let rendered = render_report(
            &project_session("api"),
            Some("backend"),
            &Report {
                summary: "done the easy half",
                status: ReportStatus::NeedsDecision,
                open_items: &["pick a cache".to_string()],
                synthesised: false,
            },
        );
        assert!(rendered.contains("session-7"));
        assert!(rendered.contains("api (backend)"));
        assert!(rendered.contains("needs_decision"));
        assert!(rendered.contains("- pick a cache"));
        assert!(rendered.contains("done the easy half"));
    }

    /// A manually opened session is named for its project, so naming both would read `api (api)`.
    #[test]
    fn a_report_does_not_name_the_project_twice() {
        let rendered = render_report(
            &project_session("api"),
            Some("api"),
            &Report {
                summary: "done",
                status: ReportStatus::Done,
                open_items: &[],
                synthesised: false,
            },
        );
        assert!(rendered.contains("— api\n"));
        assert!(!rendered.contains("api (api)"));
    }

    /// The console session has to be able to tell the session's own report from Octoboard's guess,
    /// or it will treat a status nobody chose as the session's word.
    #[test]
    fn a_synthesised_report_says_so() {
        let synthesised = render_report(
            &project_session("api"),
            None,
            &Report {
                summary: "last thing it said",
                status: ReportStatus::NeedsDecision,
                open_items: &[],
                synthesised: true,
            },
        );
        assert!(synthesised.contains("stopped without reporting"));

        let own = render_report(
            &project_session("api"),
            None,
            &Report {
                summary: "last thing it said",
                status: ReportStatus::NeedsDecision,
                open_items: &[],
                synthesised: false,
            },
        );
        assert!(!own.contains("stopped without reporting"));
    }

    /// The normal case: a form submits an object of scalar fields, rendered as readable
    /// `key: value` lines.
    #[test]
    fn a_submission_of_scalar_fields_renders_as_key_value_lines() {
        let rendered = render_page_submission(
            "page-7",
            &serde_json::json!({ "name": "Ada", "age": 30, "subscribe": true }),
        );
        assert!(rendered.contains("page page-7"));
        assert!(rendered.contains("name: Ada"));
        assert!(rendered.contains("age: 30"));
        assert!(rendered.contains("subscribe: true"));
    }

    /// A form's field order is the author's reading order, not alphabetical — `serde_json` is
    /// built with `preserve_order` for exactly this, so a submission's fields must come out in the
    /// order they were sent, not in whatever order a `BTreeMap` would impose.
    #[test]
    fn a_submission_keeps_the_fields_in_their_sent_order() {
        let rendered = render_page_submission(
            "page-1",
            &serde_json::json!({ "who": "Ada", "page": "page-1" }),
        );
        let who_at = rendered
            .find("who: Ada")
            .expect("the who field is rendered");
        let page_at = rendered
            .find("page: page-1")
            .expect("the page field is rendered");
        assert!(
            who_at < page_at,
            "`who` was sent before `page` and must render before it: {rendered}"
        );
    }

    /// Anything that is not an object of scalars — nested data, an array, a bare value — falls back
    /// to pretty-printed JSON rather than a guessed layout.
    #[test]
    fn a_submission_of_nested_data_falls_back_to_pretty_json() {
        let rendered =
            render_page_submission("page-7", &serde_json::json!({ "answers": ["a", "b"] }));
        assert!(rendered.contains('{'));
        assert!(rendered.contains("\"answers\""));
        assert!(rendered.contains("\"a\""));
    }

    #[test]
    fn only_the_three_report_statuses_parse() {
        assert_eq!(ReportStatus::parse("done"), Some(ReportStatus::Done));
        assert_eq!(ReportStatus::parse("failed"), Some(ReportStatus::Failed));
        assert_eq!(
            ReportStatus::parse("needs_decision"),
            Some(ReportStatus::NeedsDecision)
        );
        assert_eq!(ReportStatus::parse("Done"), None);
        assert_eq!(ReportStatus::parse("finished"), None);
    }

    fn done_report() -> Report<'static> {
        Report {
            summary: "all good",
            status: ReportStatus::Done,
            open_items: &[],
            synthesised: false,
        }
    }

    /// A session's `console_id` is a foreign key, so every test below needs this on record first.
    fn console() -> crate::protocol::Console {
        crate::protocol::Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
            default_agent: Agent::Claude,
            project_session_agent: None,
            claude_project_session_model: None,
            claude_project_session_effort: None,
            codex_project_session_model: None,
            codex_project_session_effort: None,
            grok_project_session_model: None,
            grok_project_session_effort: None,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
            claude_console_session_model: None,
            claude_console_session_effort: None,
            codex_console_session_model: None,
            codex_console_session_effort: None,
            grok_console_session_model: None,
            grok_console_session_effort: None,
            icon: None,
            console_session_prompt: None,
            created_at: 0,
        }
    }

    /// Reporting fails, and leaves the session exactly as it was, when the reporting session is
    /// unbound — there is nobody its binding names to report to.
    #[test]
    fn reporting_fails_for_an_unbound_session() {
        let (state, _dir) = crate::test_support::app_state("reporting-unbound");
        state.store.insert_console(&console()).unwrap();
        let mut unbound = project_session("worker");
        unbound.bound_to = None;
        state.store.insert_session(&unbound).unwrap();

        let err = deliver_report(&state, &unbound.id, done_report()).expect_err("unbound");
        assert!(err.to_string().contains("unbound"), "{err}");
        // Left exactly as it was: still idle, not archived by a report that never went anywhere.
        assert_eq!(
            state
                .store
                .get_session(&unbound.id)
                .unwrap()
                .unwrap()
                .status,
            SessionStatus::Idle
        );
    }

    /// Reporting fails when the session's own binding names a console session that is no longer
    /// on record — deleted along with its console, say.
    #[test]
    fn reporting_fails_when_the_bound_console_session_is_gone() {
        let (state, _dir) = crate::test_support::app_state("reporting-owner-gone");
        state.store.insert_console(&console()).unwrap();
        let mut orphaned = project_session("worker");
        orphaned.bound_to = Some("no-such-session".to_string());
        state.store.insert_session(&orphaned).unwrap();

        let err = deliver_report(&state, &orphaned.id, done_report()).expect_err("owner gone");
        assert!(err.to_string().contains("no longer on record"), "{err}");
    }

    /// Reporting fails for a session that has already been wrapped up: a `done` report with no
    /// open items archives the session, and a second report from the same turn must not be
    /// delivered again.
    #[test]
    fn reporting_fails_for_a_session_already_wrapped_up() {
        let (state, _dir) = crate::test_support::app_state("reporting-wrapped-up");
        state.store.insert_console(&console()).unwrap();
        let mut done = project_session("worker");
        done.status = SessionStatus::Archived;
        state.store.insert_session(&done).unwrap();

        let err = deliver_report(&state, &done.id, done_report()).expect_err("already archived");
        assert!(err.to_string().contains("wrapped up"), "{err}");
    }

    /// A project session bound to one console session has its report delivered to that one, and
    /// not to another live console session in the same console: reports are routed by the
    /// binding, never by guessing which console session a project session's reports belong to.
    #[test]
    fn a_report_reaches_only_the_console_session_it_is_bound_to() {
        let (state, _dir) = crate::test_support::app_state("reporting-routes-by-binding");
        state.store.insert_console(&console()).unwrap();

        let owner = |id: &str| Session {
            id: id.to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: "local".to_string(),
            role: Role::Console,
            origin: Origin::User,
            title: id.to_string(),
            status: SessionStatus::Idle,
            terminal_title: None,
            record_titles: Default::default(),
            background_tasks: Vec::new(),
            switching_account: false,
            default_effort: None,
            context: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: false,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            locked: false,
            started_at: 0,
            ended_at: None,
        };
        state.store.insert_session(&owner("owner-a")).unwrap();
        state.store.insert_session(&owner("owner-b")).unwrap();
        let live_a = crate::test_support::idle_stand_in("owner-a");
        let live_b = crate::test_support::idle_stand_in("owner-b");
        state.register_live(live_a.clone());
        state.register_live(live_b.clone());
        // What actually fills each session's output ring buffer from its PTY — `register_live`
        // alone does not start that pump, see `term::launch`.
        crate::session::spawn_reader_thread(live_a.clone(), 8 * 1024);
        crate::session::spawn_reader_thread(live_b.clone(), 8 * 1024);

        let mut worker = project_session("worker");
        worker.bound_to = Some("owner-a".to_string());
        state.store.insert_session(&worker).unwrap();

        deliver_report(&state, &worker.id, done_report()).expect("delivered");

        let wait_for_output = |live: &crate::session::LiveSession| -> Vec<u8> {
            let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
            loop {
                let output = live.recent_output(8 * 1024);
                if !output.is_empty() || std::time::Instant::now() >= deadline {
                    return output;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let a_output = String::from_utf8_lossy(&wait_for_output(&live_a)).into_owned();
        assert!(
            a_output.contains(&worker.id),
            "owner-a should see the report: {a_output}"
        );
        // Read once, right after owner-a's report has landed, rather than waiting out the full
        // deadline again: the pump has had exactly as long to deliver to owner-b as it had to
        // deliver to owner-a, so an empty read here already distinguishes "nothing was delivered"
        // from "the pump produced nothing yet" — waiting longer would not change which is true.
        let b_output = String::from_utf8_lossy(&live_b.recent_output(8 * 1024)).into_owned();
        assert!(
            !b_output.contains(&worker.id),
            "owner-b must not see a report bound to owner-a: {b_output}"
        );
    }

    /// A `done` report with no open items archives the reporting session and nothing else: not its
    /// console session, which keeps running, and not a sibling bound to the same console session.
    #[test]
    fn a_done_report_archives_only_the_reporting_session() {
        let (state, _dir) = crate::test_support::app_state("reporting-archives-reporter");
        state.store.insert_console(&console()).unwrap();
        let hub = Session {
            id: "hub".to_string(),
            role: Role::Console,
            bound_to: None,
            ..project_session("hub")
        };
        state.store.insert_session(&hub).unwrap();
        let live = crate::test_support::idle_stand_in("hub");
        state.register_live(live.clone());
        for id in ["worker", "sibling"] {
            let session = Session {
                id: id.to_string(),
                bound_to: Some("hub".to_string()),
                ..project_session(id)
            };
            state.store.insert_session(&session).unwrap();
        }

        deliver_report(&state, "worker", done_report()).expect("delivered");

        let status = |id: &str| state.store.get_session(id).unwrap().unwrap().status;
        assert_eq!(status("worker"), SessionStatus::Archived);
        assert_eq!(status("hub"), SessionStatus::Idle);
        assert_eq!(status("sibling"), SessionStatus::Idle);
    }

    /// A `done` report its owner's agent leaves unsent archives nothing: the reporter is told the
    /// owner has not read it, and stays as it is.
    #[test]
    fn a_done_report_left_unsent_archives_nothing() {
        let (state, _dir, _live) = team_state(
            "reporting-unsent-done",
            &[("worker", "hub", false, SessionStatus::Idle)],
        );
        *state.send_wait.lock().unwrap() = Some(std::time::Duration::from_millis(100));

        let note = deliver_report(&state, "worker", done_report()).expect("written");

        assert!(note.contains("has not read it"), "{note}");
        assert!(note.contains("not archived"), "{note}");
        let status = state.store.get_session("worker").unwrap().unwrap().status;
        assert_eq!(status, SessionStatus::Idle);
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

    /// A session bound to an unbound project session reports there, as one bound to a console
    /// session does: the report reaches the starter's terminal, and a `done` one archives the
    /// reporter and not the starter.
    #[test]
    fn a_report_reaches_the_project_session_that_started_the_reporter() {
        let (state, _dir) = crate::test_support::app_state("reporting-project-session-owner");
        state.store.insert_console(&console()).unwrap();
        let starter = Session {
            id: "starter".to_string(),
            bound_to: None,
            ..project_session("starter")
        };
        state.store.insert_session(&starter).unwrap();
        let live = crate::test_support::idle_stand_in("starter");
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        let worker = Session {
            id: "worker".to_string(),
            bound_to: Some("starter".to_string()),
            ..project_session("worker")
        };
        state.store.insert_session(&worker).unwrap();

        let note = deliver_report(&state, "worker", done_report()).expect("delivered");

        assert!(note.contains("project session"), "{note}");
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        let output = loop {
            let output = String::from_utf8_lossy(&live.recent_output(8 * 1024)).into_owned();
            if output.contains("worker") || std::time::Instant::now() >= deadline {
                break output;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(output.contains("Report from session worker"), "{output}");
        let status = |id: &str| state.store.get_session(id).unwrap().unwrap().status;
        assert_eq!(status("worker"), SessionStatus::Archived);
        assert_eq!(status("starter"), SessionStatus::Idle);
    }

    /// A console session `hub` with a stand-in process, and the sessions given, each `(id, owner,
    /// lead, status)`, on record. The stand-in's output is pumped into its ring buffer, so what
    /// reaches it can be read back.
    fn team_state(
        label: &str,
        sessions: &[(&str, &str, bool, SessionStatus)],
    ) -> (
        Arc<AppState>,
        crate::test_support::ScratchDir,
        crate::test_support::StandIn,
    ) {
        let (state, dir) = crate::test_support::app_state(label);
        state.store.insert_console(&console()).unwrap();
        let hub = Session {
            id: "hub".to_string(),
            role: Role::Console,
            bound_to: None,
            ..project_session("hub")
        };
        state.store.insert_session(&hub).unwrap();
        let live = crate::test_support::idle_stand_in("hub");
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        for (id, owner, lead, status) in sessions {
            let session = Session {
                id: id.to_string(),
                bound_to: Some(owner.to_string()),
                lead: *lead,
                status: *status,
                ..project_session(id)
            };
            state.store.insert_session(&session).unwrap();
        }
        (state, dir, live)
    }

    #[test]
    fn a_locked_sessions_done_report_is_delivered_without_archiving() {
        let (state, _dir, _live) = team_state(
            "locked-report",
            &[("worker", "hub", false, SessionStatus::Idle)],
        );
        state.store.set_session_locked("worker", true).unwrap();
        let result = deliver_report(&state, "worker", done_report()).unwrap();
        assert!(!result.contains("now archived"));
        assert_eq!(
            state.session_record("worker").unwrap().status,
            SessionStatus::Idle
        );
        state.store.set_session_locked("worker", false).unwrap();
        deliver_report(&state, "worker", done_report()).unwrap();
        assert_eq!(
            state.session_record("worker").unwrap().status,
            SessionStatus::Archived
        );
    }

    /// A lead session's `done` report with no open items is refused, naming them, while a session
    /// bound to it is not archived, and leaves it as it was; a report with open items goes through
    /// meanwhile and archives nothing; once its sessions are all archived the `done` report is
    /// delivered and archives it.
    #[test]
    fn a_lead_sessions_done_report_waits_until_its_sessions_are_archived() {
        use SessionStatus::{Archived, Idle};
        let (state, _dir, _hub) = team_state(
            "reporting-lead-done",
            &[
                ("lead", "hub", true, Idle),
                ("busy", "lead", false, Idle),
                ("finished", "lead", false, Archived),
            ],
        );
        let status = |id: &str| state.store.get_session(id).unwrap().unwrap().status;

        let err = deliver_report(&state, "lead", done_report()).expect_err("busy is not archived");
        assert!(err.to_string().contains("`busy`"), "{err}");
        assert!(!err.to_string().contains("finished"), "{err}");
        assert_eq!(status("lead"), Idle);

        let open = ["the rest".to_string()];
        deliver_report(
            &state,
            "lead",
            Report {
                open_items: &open,
                ..done_report()
            },
        )
        .expect("a report with open items goes through");
        assert_eq!(status("lead"), Idle);

        let mut busy = state.store.get_session("busy").unwrap().unwrap();
        busy.status = Archived;
        state.store.update_session(&busy).unwrap();
        let note = deliver_report(&state, "lead", done_report()).expect("delivered");
        assert!(note.contains("console session"), "{note}");
        assert_eq!(status("lead"), Archived);
    }

    /// No report is synthesised for a lead session's turn while a session bound to it has a
    /// process; one whose sessions have none is reported on as any bound session is. Both are
    /// synthesised in that order into the same console session, so the second report arriving
    /// without the first shows the first was withheld rather than not yet written.
    #[test]
    fn no_report_is_synthesised_for_a_lead_session_while_its_sessions_run() {
        use SessionStatus::{Idle, Interrupted, Working};
        let (state, _dir, hub) = team_state(
            "reporting-lead-synthesis",
            &[
                ("waiting-lead", "hub", true, Idle),
                ("running", "waiting-lead", false, Working),
                ("idle-lead", "hub", true, Idle),
                ("stopped", "idle-lead", false, Interrupted),
            ],
        );
        let running = crate::test_support::idle_stand_in("running");
        state.register_live(running.clone());
        let turn = || hooks::TurnEnd {
            last_assistant_message: Some("waiting on my sessions".to_string()),
            failed: false,
            backstop: false,
        };

        synthesise_report(&state, "waiting-lead", turn());
        synthesise_report(&state, "idle-lead", turn());

        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        let output = loop {
            let output = String::from_utf8_lossy(&hub.recent_output(8 * 1024)).into_owned();
            if output.contains("idle-lead") || std::time::Instant::now() >= deadline {
                break output;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(output.contains("Report from session idle-lead"), "{output}");
        assert!(!output.contains("waiting-lead"), "{output}");
    }

    /// A stand-in process for the session `worker` of `team_state`, registered and watched for its
    /// exit as a launched one is.
    fn watched_worker(state: &Arc<AppState>) -> crate::test_support::StandIn {
        let live = crate::test_support::idle_stand_in("worker");
        state.register_live(live.clone());
        state.watch_exit(live.clone());
        live
    }

    /// Waits until the exit watcher has recorded the end of the session `id`.
    async fn recorded_ended(state: &Arc<AppState>, id: &str) {
        let (state, id) = (state.clone(), id.to_string());
        let ended = crate::test_support::within(crate::test_support::PATIENCE, move || {
            state
                .store
                .get_session(&id)
                .unwrap()
                .is_some_and(|session| session.ended_at.is_some())
        })
        .await;
        assert!(ended, "the exit was recorded");
    }

    /// What the hub has been written once a marker has reached it, so an absence asserted in it is
    /// one of a reader that has caught up.
    async fn hub_heard(state: &Arc<AppState>, hub: &crate::test_support::StandIn) -> String {
        let (state, hub) = (state.clone(), (**hub).clone());
        tokio::task::spawn_blocking(move || crate::test_support::heard_after_marker(&state, &hub))
            .await
            .unwrap()
    }

    /// A process that ends by itself, with nobody having archived the session, is told to its
    /// owner as an end nobody asked for, naming the session and the tools to look at it and
    /// relaunch it by.
    #[tokio::test]
    async fn an_owner_is_told_when_its_session_ends_on_its_own() {
        let (state, _dir, hub) = team_state(
            "reporting-unexpected-exit",
            &[("worker", "hub", false, SessionStatus::Working)],
        );
        let worker = watched_worker(&state);

        worker.terminate();
        recorded_ended(&state, "worker").await;
        // The notice is written off the watcher after the record, so a marker could overtake it.
        hub_heard_of(&hub, "ended on its own").await;

        let heard = hub_heard(&state, &hub).await;
        assert!(heard.contains("ended on its own"), "{heard}");
        assert!(heard.contains("`worker`"), "{heard}");
        assert!(
            heard.contains("`mcp__octoboard__reopen_session`"),
            "{heard}"
        );
        assert_eq!(heard.matches("ended on its own").count(), 1, "{heard}");
        assert_eq!(
            state.session_record("worker").unwrap().status,
            SessionStatus::Interrupted
        );
    }

    /// An end somebody asked for is not told: an archive, a daemon going down (which has marked
    /// the session interrupted already) and an operation that stops the process itself, as an
    /// account switch does. A session that reports to nobody has no one to tell.
    #[tokio::test]
    async fn an_owner_is_not_told_of_an_end_somebody_asked_for() {
        let (state, _dir, hub) = team_state(
            "reporting-deliberate-exit",
            &[
                ("worker", "hub", false, SessionStatus::Working),
                ("claimed", "hub", false, SessionStatus::Working),
                ("going-down", "hub", false, SessionStatus::Working),
            ],
        );
        let alone = Session {
            id: "alone".to_string(),
            bound_to: None,
            status: SessionStatus::Working,
            ..project_session("alone")
        };
        state.store.insert_session(&alone).unwrap();
        let mut ending = Vec::new();
        for id in ["worker", "claimed", "going-down", "alone"] {
            let live = crate::test_support::idle_stand_in(id);
            state.register_live(live.clone());
            state.watch_exit(live.clone());
            ending.push(live);
        }
        let mut archived = state.session_record("worker").unwrap();
        archived.status = SessionStatus::Archived;
        state.store.update_session(&archived).unwrap();
        let _claim = state.begin_account_switch("claimed", false).unwrap();
        let mut interrupted = state.session_record("going-down").unwrap();
        interrupted.status = SessionStatus::Interrupted;
        state.store.update_session(&interrupted).unwrap();

        for live in &ending {
            // What an account switch's stop does; archive and shutdown mark the record first.
            live.end();
        }
        for id in ["worker", "claimed", "going-down", "alone"] {
            recorded_ended(&state, id).await;
        }
        // The notice, if there were one, is written off the watcher right after the record.
        tokio::time::sleep(Duration::from_millis(300)).await;

        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("ended on its own"), "{heard}");
    }

    /// A turn that ended in an error asks the automatic switch to look at its session, while the
    /// setting is on and the end belongs to the turn that is open; an ordinary end, a stale one
    /// and a setting that is off ask nothing.
    #[tokio::test]
    async fn a_failed_turn_asks_the_automatic_switch_to_look_at_its_session() {
        let (state, _dir, _hub) = team_state(
            "reporting-failed-turn-switch",
            &[
                ("failed", "hub", false, SessionStatus::Working),
                ("ordinary", "hub", false, SessionStatus::Working),
                ("stale", "hub", false, SessionStatus::Working),
                ("off", "hub", false, SessionStatus::Working),
            ],
        );
        let asked = |id: &str| {
            crate::auto_switch::ASKED
                .lock()
                .unwrap()
                .contains(&id.to_string())
        };
        let turn = |failed| hooks::TurnEnd {
            last_assistant_message: None,
            failed,
            backstop: false,
        };
        state.store.set_auto_switch_on_usage_limit(true).unwrap();
        for id in ["failed", "ordinary", "stale"] {
            state.turn_started(id, Some("work"));
        }

        close_turn_and_report(&state, "ordinary", turn(false));
        close_turn_and_report(&state, "failed", turn(true));
        // A clock-attributed end that cannot belong to the turn open now.
        close_turn_and_report(
            &state,
            "stale",
            hooks::TurnEnd {
                failed: true,
                backstop: true,
                ..turn(true)
            },
        );
        state.store.set_auto_switch_on_usage_limit(false).unwrap();
        state.turn_started("off", Some("work"));
        close_turn_and_report(&state, "off", turn(true));

        assert!(asked("failed"));
        assert!(!asked("ordinary"));
        assert!(!asked("stale"));
        assert!(!asked("off"));
    }

    /// A claim on the session says nothing of how its process ended: one that exits by itself
    /// while an operation holds the claim, including one that stops no process (a handover of
    /// branches, a binding change), is told to the owner as an end nobody asked for, and so is a
    /// process the account switch found already gone when it stopped it.
    #[tokio::test]
    async fn an_owner_is_told_of_a_process_that_ended_by_itself_under_a_claim() {
        let (state, _dir, hub) = team_state(
            "reporting-exit-under-claim",
            &[("worker", "hub", false, SessionStatus::Working)],
        );
        let worker = watched_worker(&state);
        let _claim = state.begin_switch("worker").unwrap();

        worker.terminate();
        recorded_ended(&state, "worker").await;

        // The notice is written off the watcher after the record, so a marker could overtake it.
        let heard = hub_heard_of(&hub, "ended on its own").await;
        assert!(heard.contains("ended on its own"), "{heard}");
        assert!(!worker.end(), "the process was gone before the stop");
        assert!(!worker.was_ended_on_purpose());
    }

    /// An owner is told once a turn that its session waits for the user, and told it is not to
    /// chase it; the next turn tells it again.
    #[tokio::test]
    async fn an_owner_is_told_once_a_turn_that_its_session_waits_for_the_user() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting",
            &[("worker", "hub", false, SessionStatus::WaitingUser)],
        );

        tell_owner_it_waits(&state, "worker");
        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert!(heard.contains("is waiting for the user"), "{heard}");
        assert!(heard.contains("Do not chase it"), "{heard}");
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            1,
            "{heard}"
        );

        state.turn_started("worker", Some("go on"));
        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            2,
            "{heard}"
        );
    }

    /// A session that reported earlier in its turn and then stops at a permission prompt is told
    /// to its owner as waiting: its report did not say so. A wait before the first turn, a trust
    /// prompt say, does not use up the notice of the turn that follows.
    #[tokio::test]
    async fn a_wait_after_a_report_or_before_the_first_turn_is_told() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting-after-report",
            &[("worker", "hub", false, SessionStatus::WaitingUser)],
        );

        tell_owner_it_waits(&state, "worker");
        state.turn_started("worker", Some("go on"));
        state.mark_reported("worker");
        tell_owner_it_waits(&state, "worker");

        let heard = hub_heard(&state, &hub).await;
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            2,
            "{heard}"
        );
    }

    /// A wait the terminal shows (a dialog of the agent), with the record still at its prompt, is
    /// told to the owner as well, and the notice is not repeated for the same turn.
    #[tokio::test]
    async fn an_owner_is_told_of_a_wait_only_the_screen_shows() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting-screen",
            &[("worker", "hub", false, SessionStatus::Idle)],
        );
        let worker = crate::test_support::running(&state, "worker");

        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("is waiting for the user"), "{heard}");

        worker.on_output(
            "\n  Resume this conversation?\n  This conversation has been inactive for 1h and is \
             42k tokens long.\n  \u{276f} Resume\n    Start a new conversation\n\n  Enter to \
             confirm \u{b7} Esc to dismiss\n"
                .as_bytes(),
        );
        assert!(worker.look_for_dialog().current.is_some());
        tell_owner_it_waits(&state, "worker");
        tell_owner_it_waits(&state, "worker");
        let heard = hub_heard(&state, &hub).await;
        assert_eq!(
            heard.matches("is waiting for the user").count(),
            1,
            "{heard}"
        );
    }

    /// The routes by which a session comes to wait for the user without a hook saying so each tell
    /// its owner: a dialog the agent puts on the screen, and Codex's "Action Required" title.
    #[tokio::test]
    async fn an_owner_is_told_when_a_dialog_or_an_action_title_makes_its_session_wait() {
        let (state, _dir, hub) = team_state("reporting-waiting-routes", &[]);
        for (id, agent) in [("dialog", Agent::Claude), ("title", Agent::Codex)] {
            state
                .store
                .insert_session(&Session {
                    id: id.to_string(),
                    agent,
                    bound_to: Some("hub".to_string()),
                    status: SessionStatus::Idle,
                    ..project_session(id)
                })
                .unwrap();
        }
        let dialog = crate::test_support::running(&state, "dialog");
        let title =
            crate::test_support::fake_live_session("title", Agent::Codex, 80, 24, "sleep 30");
        state.register_live(title.clone());
        crate::session::spawn_reader_thread(title.clone(), 8 * 1024);

        crate::agent_dialogs::watch(&state, &dialog);
        dialog.on_output(
            "\n  Resume this conversation?\n  This conversation has been inactive for 1h and is \
             42k tokens long.\n  \u{276f} Resume\n    Start a new conversation\n\n  Enter to \
             confirm \u{b7} Esc to dismiss\n"
                .as_bytes(),
        );
        let heard = hub_heard_of(&hub, "`dialog`").await;
        assert!(heard.contains("`dialog`"), "{heard}");

        title.on_output(b"\x1b]0;[ ! ] Action Required | repo\x07");
        let heard = hub_heard_of(&hub, "`title`").await;
        assert!(heard.contains("`title`"), "{heard}");
    }

    /// What the hub hears until `needle` is in it.
    async fn hub_heard_of(hub: &crate::test_support::StandIn, needle: &'static str) -> String {
        let hub = (**hub).clone();
        tokio::task::spawn_blocking(move || crate::test_support::output_containing(&hub, needle))
            .await
            .unwrap()
    }

    /// Nothing is said of a wait when the session has no owner or no process to wait in.
    #[tokio::test]
    async fn an_owner_is_not_told_of_a_wait_that_is_not_one() {
        let (state, _dir, hub) = team_state(
            "reporting-waiting-not-told",
            &[("down", "hub", false, SessionStatus::Interrupted)],
        );
        let alone = Session {
            id: "alone".to_string(),
            bound_to: None,
            status: SessionStatus::WaitingUser,
            ..project_session("alone")
        };
        state.store.insert_session(&alone).unwrap();

        for id in ["alone", "down"] {
            tell_owner_it_waits(&state, id);
        }

        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("is waiting for the user"), "{heard}");
    }

    /// Several sessions in one project have to be told apart in the menu, and the goal is the only
    /// thing that distinguishes them.
    #[test]
    fn a_title_from_a_goal_stays_short_and_readable() {
        assert_eq!(
            title_from_goal("Fix the login retry"),
            "Fix the login retry"
        );
        assert_eq!(title_from_goal("\n\n  Fix login  \nmore"), "Fix login");

        let long = title_from_goal(
            "Replace the hand-rolled retry logic in the authentication client with something sane",
        );
        assert!(long.ends_with('…'));
        assert!(long.chars().count() <= 49);
        // Cut on a word, not mid-word.
        assert!(!long.contains("som…"));

        // A single long word still has to be cut, and on a character boundary.
        let unbroken = title_from_goal(&"日".repeat(80));
        assert_eq!(unbroken.chars().count(), 49);
    }
}
