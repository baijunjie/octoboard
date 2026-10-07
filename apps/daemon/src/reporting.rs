//! The channel between a console session and its project sessions: the brief a task is
//! handed over as, the message writing that carries both directions, the report that comes back,
//! and the report Octoboard synthesises when a session stops without having sent one.
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

use anyhow::{anyhow, bail, Result};

use crate::outbox::Drain;
use crate::protocol::{error_code, Agent, CodedError, Role, Session, SessionStatus};
use crate::state::AppState;
use crate::{coordinator, hooks, term};

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
    let mut prompt = format!("## Goal\n\n{}\n", goal.trim());
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
    /// Queue it. The right answer for the console session and for a report, neither of which has
    /// anyone to tell and neither of which may be dropped.
    Queue,
}

/// Writes a message into a running session, or queues it until the session can take one.
///
/// **The gate is the session's hook-reported state, never the terminal.** No agent signals its modal
/// state through terminal modes, and a write while a modal dialog is up has its trailing Enter
/// confirm whatever option is highlighted — at Claude Code's trust dialog that exits the session, at
/// Grok's approval modal it selects always-approve. Working and idle are both safe: every agent
/// queues a message written mid-turn and consumes it when the turn ends.
///
/// **Blocks** on the PTY write; callers on the runtime are responsible for keeping it off a worker.
pub fn write_message(
    state: &Arc<AppState>,
    id: &str,
    text: &str,
    when_blocked: WhenBlocked,
) -> Result<Delivery> {
    let session = state.session_record(id)?;
    // Nothing to write to and nothing to wait for: a resume starts the agent at its prompt rather
    // than replaying a queue, so queuing here would lose the message silently.
    if session.status.is_dormant() || state.live_session(id).is_none() {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_RUNNING,
            "this session is not running",
            &[("session", id)],
        ));
    }
    let writable = matches!(session.status, SessionStatus::Working | SessionStatus::Idle);
    if !writable && when_blocked == WhenBlocked::Refuse {
        return Err(CodedError::raised(
            error_code::SESSION_WAITING_FOR_USER,
            "this session is waiting for you — answer it in the terminal first",
            &[("session", id)],
        ));
    }

    // Queued even when the session looks ready, so messages cannot overtake one another.
    state.queue_message(id, text);
    match state.flush_outbox(id, session.status) {
        Drain::Clear => Ok(Delivery::Written),
        Drain::Pending => Ok(Delivery::Queued),
        Drain::Lost => Err(CodedError::raised(
            error_code::QUEUED_MESSAGES_LOST,
            lost_message(),
            &[],
        )),
    }
}

/// Releases whatever is queued for a session that has just been relaunched, where the agent will not
/// release it itself. Called on every relaunch, so most calls find an empty queue.
///
/// The instruction is queued rather than written because the session's status after a relaunch is
/// Octoboard's own doing, not something a hook reported — and the agent may be sitting on its own
/// trust or approval dialog, where the paste's trailing Enter confirms whatever option is
/// highlighted: at Claude Code's trust dialog that exits the session, at Grok's approval modal it
/// selects always-approve. Those two each report a status as they start, which releases it.
///
/// Codex does not: its `SessionStart` does not fire until the first prompt submission, because the
/// thread is created lazily then, so nothing it reports would release the queue before the user
/// typed. Writing to it unprompted is safe where it would not be for the other two — a paste at a
/// Codex modal changes nothing.
pub fn release_after_relaunch(state: &Arc<AppState>, session: &Session) {
    if session.agent == Agent::Codex {
        state.spawn_flush_outbox(&session.id, SessionStatus::Idle);
    }
}

/// A report from a project session, as the console session reads it.
pub struct Report<'a> {
    pub summary: &'a str,
    pub status: ReportStatus,
    pub open_items: &'a [String],
    /// True when Octoboard built this report from the turn's last assistant message rather than the
    /// session sending one, which the console session has to know: the structured fields are then a
    /// guess and only the prose is the session's own.
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

/// Delivers one project session's report to its console session, and wraps the session up when
/// the report says there is nothing left.
///
/// Archiving happens once the report has been *accepted* for the console session rather than once
/// the console session has read it: a console session that is merely busy still has the report
/// queued for it, and leaving a finished session alive until the console session gets round to it
/// would strand it. A console session that is not running at all is a different matter — the
/// report fails, and the session stays as it is for the user to deal with.
pub fn deliver_report(
    state: &Arc<AppState>,
    session_id: &str,
    report: Report<'_>,
) -> Result<String> {
    let session = state.session_record(session_id)?;
    if !session.include_in_hub {
        bail!(
            "this session is not part of the console session's orchestration, so there is nobody \
             to report to"
        );
    }
    // A session that reported `done` is archived by the time a second report could arrive, and its
    // process is on its way out. Refusing is what keeps that race from delivering the same round of
    // work to the console session twice.
    if session.status.is_dormant() {
        bail!("this session has already been wrapped up; there is nothing further to report");
    }

    let console_session = console_session_of(state, &session.console_id)?.ok_or_else(|| {
        anyhow!("this console has no console session, so there is nobody to report to")
    })?;

    let project = match &session.project_id {
        Some(id) => state.store.get_project(id)?.map(|project| project.name),
        None => None,
    };
    let message = render_report(&session, project.as_deref(), &report);
    // A lost message propagates rather than being treated as delivered: the project session must
    // not be archived on the strength of a report the console session never got.
    let delivery = write_message(state, &console_session.id, &message, WhenBlocked::Queue)?;

    let finished = report.status == ReportStatus::Done && report.open_items.is_empty();
    if finished {
        coordinator::archive_session(state, session_id)?;
    }

    Ok(match (delivery, finished) {
        (Delivery::Written, true) => {
            "Reported to the console session. This session is now archived.".into()
        }
        (Delivery::Written, false) => "Reported to the console session.".into(),
        (Delivery::Queued, true) => {
            "Report accepted; the console session will see it as soon as it can take a message. \
             This session is now archived."
                .into()
        }
        (Delivery::Queued, false) => {
            "Report accepted; the console session will see it as soon as it can take a message."
                .into()
        }
    })
}

/// Reports for the console session on a session that stopped without reporting for itself. The
/// caller has already closed the turn and established that a report is owed.
///
/// **Blocks** on writing into the console session.
pub fn synthesise_report(state: &Arc<AppState>, session_id: &str, turn: hooks::TurnEnd) {
    match state.store.get_session(session_id) {
        Ok(Some(session)) if session.include_in_hub => {}
        // A session outside the orchestration has nobody to report to, and a record that is gone is
        // nothing to report about.
        _ => return,
    }

    let summary = turn.last_assistant_message.unwrap_or_else(|| {
        "This session's turn ended and it said nothing. Check it with `get_session`.".to_string()
    });
    let report = Report {
        summary: &summary,
        // A turn that ended in an error failed; a turn that merely ended without a report is the
        // console session's to judge, which is what `needs_decision` asks it to do.
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

/// The console's console session, preferring one with a process behind it. A console has at most
/// one console session that is not archived, but an older archived one may still be on record.
pub(crate) fn console_session_of(
    state: &Arc<AppState>,
    console_id: &str,
) -> Result<Option<Session>> {
    let mut candidates: Vec<Session> = state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| session.console_id == console_id && session.role == Role::Console)
        .collect();
    candidates.sort_by_key(|session| (session.status.is_dormant(), -session.started_at));
    Ok(candidates.into_iter().next())
}

/// The report as it is written into the console session. Plain prose with the structured fields
/// spelled out: the console session reads this as a user message, so it has to be readable rather
/// than a payload, and the session id has to be in it or the console session cannot follow up.
fn render_report(session: &Session, project: Option<&str>, report: &Report<'_>) -> String {
    let origin = match project.filter(|project| *project != session.title) {
        Some(project) => format!("{} ({})", session.title, project),
        None => session.title.clone(),
    };
    let mut message = format!(
        "Report from session {} — {origin}\nStatus: {}",
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
    let mut message = format!("Report panel form submission — page {page_id}\n\n");
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
    use super::*;
    use crate::protocol::{Origin, SessionStatus};

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
            has_conversation: true,
            include_in_hub: true,
            config_dir: None,
            pinned: false,
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
