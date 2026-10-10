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

use crate::outbox::Drain;
use crate::protocol::{error_code, CodedError, Role, Session, SessionStatus};
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
    /// Queue it. The right answer for an owner and for a report, neither of which has anyone to
    /// tell and neither of which may be dropped.
    Queue,
}

/// Writes a message into a running session, or queues it until the session can take one.
///
/// **The gate is the session's hook-reported state, never the terminal.** No agent signals its modal
/// state through terminal modes, and a write while a modal dialog is up has its trailing Enter
/// confirm whatever option is highlighted — at Claude Code's trust dialog that exits the session, at
/// Grok's approval modal it selects always-approve. Working and idle are both safe: every agent
/// queues a message written mid-turn and consumes it when the turn ends. The one thing that comes
/// before any hook is each agent's own trust screen, which the trailing Enter would answer — Codex
/// and Grok Build would take it as trusting the folder — so a session whose trust watch still holds
/// writes (`TrustState::holds_writes`) is blocked as well.
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
/// user to deal with.
pub fn deliver_report(
    state: &Arc<AppState>,
    session_id: &str,
    report: Report<'_>,
) -> Result<String> {
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
    let finished = report.status == ReportStatus::Done && report.open_items.is_empty();
    if finished {
        let unfinished: Vec<Session> = coordinator::bound_sessions(state, &session)?
            .into_iter()
            .filter(|bound| bound.status != SessionStatus::Archived)
            .collect();
        if !unfinished.is_empty() {
            let named = unfinished
                .iter()
                .map(|bound| format!("`{}` (\"{}\")", bound.id, bound.title))
                .collect::<Vec<_>>()
                .join(", ");
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
    let owner_kind = match owner.role {
        Role::Console => "console session",
        Role::Project => "project session",
    };

    let project = match &session.project_id {
        Some(id) => state.store.get_project(id)?.map(|project| project.name),
        None => None,
    };
    let message = render_report(&session, project.as_deref(), &report);
    // A lost message propagates rather than being treated as delivered: the session must not be
    // archived on the strength of a report its owner never got.
    let delivery = write_message(state, &owner.id, &message, WhenBlocked::Queue)?;

    if finished {
        coordinator::archive_session(state, session_id)?;
    }

    Ok(match (delivery, finished) {
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
    })
}

/// Reports to the owner for a session that stopped without reporting for itself. The
/// caller has already closed the turn and established that a report is owed.
///
/// Nothing is reported for a lead session while a session bound to it has a process: it has
/// stopped to wait for its sessions, not stopped working.
///
/// **Blocks** on writing into the owner.
pub fn synthesise_report(state: &Arc<AppState>, session_id: &str, turn: hooks::TurnEnd) {
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

/// The report as it is written into the owner. Plain prose with the structured fields spelled out:
/// the owner reads this as a user message, so it has to be readable rather than a payload, and the
/// session id has to be in it or the owner cannot follow up.
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
            has_conversation: true,
            bound_to: Some("console-session-1".to_string()),
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
            console_session_agent: Agent::Claude,
            default_agent: Agent::Claude,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
            icon: None,
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
            has_conversation: false,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
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
