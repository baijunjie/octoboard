//! The reporting channel between an owner — a console session, or a project session that started
//! sessions, unbound or a lead session — and the project sessions bound to it: the report a bound
//! session sends back, delivered into its owner and archiving the session when it says there is
//! nothing left, and the report Octoboard synthesises when a session stops without having sent
//! one. Writing into the owner, and keeping a report for an owner that is interrupted, is
//! [`crate::delivery`]'s.
//!
//! That is the one concern the project session's `report` tool, binding to an owner and the hook
//! callback all share, which is why it sits apart from the control-socket handling in
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

use crate::branch_cleanup;
use crate::delivery::{is_archived_refusal, rebound_owner, write_to_owner, Delivery};
use crate::protocol::{error_code, CodedError, Session, SessionStatus};
use crate::session_texts::REPORT_OPENING;
use crate::state::{AppState, TurnClose};
use crate::{coordinator, hooks};

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
/// finished session alive until the owner gets round to it would strand it. An interrupted owner
/// counts as accepting it too: the report is kept for it and written into it once it is resumed
/// (see [`write_to_owner`]), and the reporter, whose work is done, has nothing to stay up for. An
/// archived owner takes nothing until it is reopened, so the report fails, and the session stays as
/// it is. So does a report written into the owner but left unsent by its agent: the owner has not
/// read it and nothing guarantees it will, so the session is not archived.
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
    let session = state.session_record(session_id)?;
    let Some(bound_to) = &session.bound_to else {
        bail!("this session is unbound, so there is nobody to report to");
    };
    // A session that reported `done` is archived by the time a second report could arrive, and its
    // process is on its way out. Refusing is what keeps that race from delivering the same round of
    // work to the owner twice.
    if session.status.is_dormant() {
        // Archiving an owner archives the sessions bound to it first, so this is the reporter of an
        // archived owner too, and "wrapped up" would leave it wondering by whom.
        let owner_archived = session.status == SessionStatus::Archived
            && state
                .store
                .get_session(bound_to)
                .ok()
                .flatten()
                .is_some_and(|owner| owner.status == SessionStatus::Archived);
        if owner_archived {
            return Err(CodedError::raised(
                error_code::SESSION_ARCHIVED,
                format!(
                    "your owner `{bound_to}` is archived, and so is this session, whose process is \
                     on its way out: the report was not delivered, and there is nothing further to \
                     report"
                ),
                &[("session", bound_to.as_str())],
            ));
        }
        bail!("this session has already been wrapped up; there is nothing further to report");
    }
    let finished = report.status == ReportStatus::Done
        && report.open_items.is_empty()
        && unlocked_with_team(state, &session)?;
    if finished {
        let unfinished = unfinished_bound(state, &session)?;
        if !unfinished.is_empty() {
            let named = coordinator::lifecycle::session_labels(&unfinished);
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
    let mut owner = state
        .store
        .get_session(bound_to)?
        .ok_or_else(|| anyhow::anyhow!("this session's owner is no longer on record"))?;
    // Nothing is kept for an archived owner, as nothing is shared with one: nothing says it will
    // ever be reopened. Its sessions are archived with it, so this is a reporter left outside that
    // archive, and the user is the one left to read what it did, unless a handoff has moved the
    // reporter to the session that took the owner's place since the binding was read.
    if owner.status == SessionStatus::Archived {
        match rebound_owner(state, session_id, &owner.id)? {
            Some(new) => owner = new,
            None => return Err(archived_owner(&owner)),
        }
    }
    let mut owner_kind = format!("{} session", owner.role.kind());

    let project = match &session.project_id {
        Some(id) => state.store.get_project(id)?.map(|project| project.name),
        None => None,
    };
    let render = |kept: Option<&str>| render_report(&session, project.as_deref(), &report, kept);
    // A lost message propagates rather than being treated as delivered: the session must not be
    // archived on the strength of a report its owner never got.
    let deliver_to = |owner: &Session| {
        write_to_owner(state, owner, &session, &render(None), |kept| {
            render(Some(kept))
        })
        .map_err(|err| from_reporters_side(err, owner))
    };
    let mut delivered = deliver_to(&owner);
    // The wait for the owner's claim can outlast the owner: a handoff archives it after moving
    // this session to the session that took its place, so an archived owner may not be the
    // owner any more.
    if is_archived_refusal(&delivered) {
        if let Some(new) = rebound_owner(state, session_id, &owner.id)? {
            owner = new;
            owner_kind = format!("{} session", owner.role.kind());
            delivered = deliver_to(&owner);
        } else if let Some(now) = state
            .session_record(session_id)?
            .bound_to
            .filter(|now| *now != owner.id)
        {
            bail!(
                "your owner changed to `{now}` while the report waited for it; it was not \
                 delivered, so report again"
            );
        }
    }
    let delivery = delivered?;

    // A report the owner has not read archives nothing: the session would be gone before its
    // owner, or the user, ever saw why.
    let held_back = finished && delivery == Delivery::Unsent;
    let mut finished = finished && !held_back;
    let mut changed_meanwhile = false;
    let (left, archived) = if finished {
        // Taken only once the report is delivered, which can wait out the owner's claim and its
        // agent sending the message: held across that, it would refuse every change of a lock in
        // the daemon. The checks that decided the archive are made again under it.
        let _removal = state.claim_session_removal();
        let session = state.session_record(session_id)?;
        if session.status == SessionStatus::Archived {
            // Archived by someone else while the report was delivered.
            (Vec::new(), Vec::new())
        } else if unlocked_with_team(state, &session)?
            && unfinished_bound(state, &session)?.is_empty()
        {
            let left = branch_cleanup::survey::will_be_left(state, std::slice::from_ref(&session));
            (
                left,
                coordinator::lifecycle::archive_session(state, session_id)?,
            )
        } else {
            finished = false;
            changed_meanwhile = true;
            (Vec::new(), Vec::new())
        }
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
        (Delivery::Kept, true) => format!(
            "Report kept for the {owner_kind} `{}`, which is not running: it is delivered once \
             the {owner_kind} is resumed, so do not send it again. This session is now archived.",
            owner.id
        ),
        (Delivery::Kept, false) => format!(
            "Report kept for the {owner_kind} `{}`, which is not running: it is delivered once \
             the {owner_kind} is resumed, so do not send it again.",
            owner.id
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
    if changed_meanwhile {
        note.push_str(
            " This session was not archived: while the report was delivered, it or a session under \
             it was locked, or a session was bound to it that is not archived.",
        );
    }
    if !left.is_empty() {
        note.push_str(&format!(
            " These git branches and worktrees bound to this session will not be removed: {}.",
            branch_cleanup::report::lines(&left).join("; ")
        ));
    }
    Ok((note, archived, delivery))
}

/// Whether neither `session` nor any session under it is locked, which a `done` report needs to
/// archive it.
fn unlocked_with_team(state: &Arc<AppState>, session: &Session) -> Result<bool> {
    Ok(coordinator::lifecycle::ensure_unlocked(
        std::iter::once(session.clone()).chain(coordinator::bindings::team(state, session)?),
    )
    .is_ok())
}

/// The sessions bound to `session` that are not archived, which refuse its `done` report.
fn unfinished_bound(state: &Arc<AppState>, session: &Session) -> Result<Vec<Session>> {
    Ok(coordinator::bindings::bound_sessions(state, session)?
        .into_iter()
        .filter(|bound| bound.status != SessionStatus::Archived)
        .collect())
}

/// A failure to write into `owner`, worded from the reporter's side.
fn from_reporters_side(err: anyhow::Error, owner: &Session) -> anyhow::Error {
    let owner_kind = format!("{} session", owner.role.kind());
    match err.downcast_ref::<CodedError>() {
        // "This session is not running" would read as the reporter itself.
        Some(coded) if coded.code == error_code::SESSION_NOT_RUNNING => CodedError::raised(
            error_code::SESSION_NOT_RUNNING,
            format!(
                "your owner, the {owner_kind} `{}`, has no process to take the report right now: \
                 it is being started, resumed, switched or handed off, or having its owner \
                 changed. The report was not delivered, and this session was left as it was; \
                 report again once your owner is running",
                owner.id
            ),
            &[("session", owner.id.as_str())],
        ),
        // The same code reads as the reporter's own fate through `delivery::lost_message`, which
        // speaks of "this session"; the drain announced the loss to the user.
        Some(coded) if coded.code == error_code::QUEUED_MESSAGES_LOST => CodedError::raised(
            error_code::QUEUED_MESSAGES_LOST,
            format!(
                "a message queued for your owner, the {owner_kind} `{}`, could not be written in \
                 full, so it and everything queued behind it, this report included, were \
                 dropped; your owner's input line may hold a fragment of it. The report was not \
                 delivered and this session was left as it was; the user has been told, so tell \
                 them in this terminal what the report says rather than reporting again",
                owner.id
            ),
            &[("session", owner.id.as_str())],
        ),
        // The owner was archived after it was read.
        Some(coded) if coded.code == error_code::SESSION_ARCHIVED => archived_owner(owner),
        _ => err,
    }
}

/// The refusal of a report for an archived owner, worded from the reporter's side.
fn archived_owner(owner: &Session) -> anyhow::Error {
    CodedError::raised(
        error_code::SESSION_ARCHIVED,
        format!(
            "your owner, the {} session `{}`, is archived, so the report was not delivered and is \
             not kept for it: an archived session takes nothing until it is reopened. This session \
             was left as it was; tell the user in this terminal what the report says, since your \
             owner will not read it",
            owner.role.kind(),
            owner.id
        ),
        &[("session", owner.id.as_str())],
    )
}

/// Closes the session's open turn on `turn` ending it, and has a report synthesised for the owner
/// when the session reported nothing itself. What closing amounted to is returned, for the caller
/// to decide what else the ending says about the session.
///
/// A report owed for a failed turn waits while the automatic account switch judges that turn, and
/// is not sent when the switch moved the session to another account or resumed it on its own,
/// which its owner is told of instead: the owner would otherwise act on a failure already dealt
/// with, possibly before that notice reaches it.
pub fn close_turn_and_report(
    state: &Arc<AppState>,
    session_id: &str,
    turn: hooks::TurnEnd,
) -> TurnClose {
    let close = state.close_turn(session_id, turn.backstop);
    // The turn stopped on an error, which may be the plan's usage limit; whether it was, and
    // whether to do anything about it, is the switch's to judge.
    let judging = if turn.failed && matches!(close, TurnClose::OwesReport | TurnClose::Reported) {
        crate::auto_switch::after_failed_turn(state, session_id)
    } else {
        None
    };
    if close == TurnClose::OwesReport {
        let state = state.clone();
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            // A switch that failed, panicked or found nothing to do leaves the report owed.
            if let Some(judging) = judging {
                if judging.await.unwrap_or(false) {
                    return;
                }
            }
            // Delivery writes into the owner's PTY, which blocks, so it goes off the caller's path.
            let _ =
                tokio::task::spawn_blocking(move || synthesise_report(&state, &session_id, turn))
                    .await;
        });
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
    match coordinator::bindings::bound_sessions(state, &session) {
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

/// Delivers a report the session itself sent, for a caller on the runtime: what the `report` tool
/// does, and what a session binding itself to an owner does with its summary.
///
/// The session is credited with having reported before delivery, even if its owner cannot take the
/// message yet, so its stop does not also produce a synthesised report. The credit is given after
/// the session's queued hooks, whose turn start would otherwise reset it, and is taken back
/// when delivery definitely failed, so the owner gets a synthesised report instead of neither.
/// An uncertain native submission keeps its credit: synthesising another report could duplicate
/// one the native queue already accepted.
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
    let (note, archived, delivery) = delivered.inspect_err(|error| {
        if !error.is::<crate::codex_transport::UncertainSubmission>() {
            state.clear_reported(session_id);
        }
    })?;
    branch_cleanup::remove::remove_in_background(state, archived);
    Ok((note, delivery))
}

/// The report as it is written into the owner. Plain prose with the structured fields spelled out:
/// the owner reads this as a user message, so it has to be readable rather than a payload, and the
/// session id has to be in it or the owner cannot follow up. `kept` is the line that says it was
/// kept for an owner that was not running, among the header's lines rather than after the summary,
/// which is the session's own text.
fn render_report(
    session: &Session,
    project: Option<&str>,
    report: &Report<'_>,
    kept: Option<&str>,
) -> String {
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
    if let Some(kept) = kept {
        message.push('\n');
        message.push_str(kept);
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::protocol::{Agent, OrchestrationRole, Origin, Role};
    use crate::test_support::owner_team::{
        console, hub_heard, hub_heard_of, interrupted_hub_state, kept_texts, project_session,
        team_state,
    };

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
            None,
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
            None,
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
            None,
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
            None,
        );
        assert!(!own.contains("stopped without reporting"));
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

    /// Reporting fails, and leaves the session exactly as it was, when the reporting session is
    /// unbound — there is nobody its binding names to report to.
    #[test]
    fn reporting_fails_for_an_unbound_session() {
        let (state, _dir) = crate::test_support::app_state("reporting-unbound");
        state.store.insert_console(&console()).unwrap();
        let mut unbound = project_session("worker");
        unbound.bound_to = None;
        unbound.orchestration_role = OrchestrationRole::Coordinator;
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
        orphaned.orchestration_role = OrchestrationRole::Worker;
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
            orchestration_role: OrchestrationRole::Coordinator,
            context: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: false,
            bound_to: None,
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
        worker.orchestration_role = OrchestrationRole::Worker;
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
                orchestration_role: OrchestrationRole::Worker,
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
            orchestration_role: OrchestrationRole::Worker,
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

    /// The report owed for a failed turn waits while the automatic switch judges that turn: it is
    /// dropped when the switch moved the session on, and sent when the switch did not. Only the
    /// check made before either judgement ends is ordered; the later absence is best-effort.
    #[tokio::test]
    async fn a_failed_turns_report_waits_for_the_switch_and_is_dropped_once_it_moved_on() {
        let (state, _dir, hub) = team_state(
            "reporting-failed-turn-held",
            &[
                ("switch-moved", "hub", false, SessionStatus::Working),
                ("switch-stayed", "hub", false, SessionStatus::Working),
            ],
        );
        state.store.set_auto_switch_on_usage_limit(true).unwrap();
        let (moved, moved_judged) = tokio::sync::oneshot::channel();
        let (stayed, stayed_judged) = tokio::sync::oneshot::channel();
        crate::auto_switch::MOVES.lock().unwrap().extend([
            ("switch-moved".to_string(), moved_judged),
            ("switch-stayed".to_string(), stayed_judged),
        ]);
        let turn = |summary: &str| hooks::TurnEnd {
            last_assistant_message: Some(summary.to_string()),
            failed: true,
            backstop: false,
        };
        for id in ["switch-moved", "switch-stayed"] {
            state.turn_started(id, Some("work"));
        }

        close_turn_and_report(&state, "switch-moved", turn("summary-of-the-moved-turn"));
        close_turn_and_report(&state, "switch-stayed", turn("summary-of-the-stayed-turn"));
        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("summary-of-the-"), "{heard}");

        moved.send(true).unwrap();
        let heard = hub_heard(&state, &hub).await;
        assert!(!heard.contains("summary-of-the-moved-turn"), "{heard}");
        stayed.send(false).unwrap();
        let heard = hub_heard_of(&hub, "summary-of-the-stayed-turn").await;
        assert!(heard.contains("summary-of-the-stayed-turn"), "{heard}");
        assert!(!heard.contains("summary-of-the-moved-turn"), "{heard}");
    }

    /// A claim that outlasts the wait leaves the report refused with the owner named, from the
    /// reporter's side, and the reporter as it was.
    #[test]
    fn a_report_to_an_owner_under_a_claim_that_outlasts_the_wait_is_refused() {
        let (state, _dir) = interrupted_hub_state("reporting-report-under-claim");
        let _claim = state.begin_launch("hub").unwrap();
        state
            .store
            .update_session_lifecycle_if(
                "worker",
                SessionStatus::WaitingUser,
                SessionStatus::Idle,
                None,
            )
            .unwrap();

        let err = deliver_report(&state, "worker", done_report()).expect_err("refused");

        let coded = err.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::SESSION_NOT_RUNNING, "{err}");
        assert!(
            err.to_string()
                .contains("your owner, the console session `hub`"),
            "{err}"
        );
        assert!(err.to_string().contains("handed off"), "{err}");
        assert_eq!(
            state.session_record("worker").unwrap().status,
            SessionStatus::Idle
        );
        assert!(kept_texts(&state).is_empty());
    }

    /// A `done` report waiting for its owner's claim holds no claim on removals meanwhile, so the
    /// user can lock the reporter while it waits; the lock is seen once the report is delivered,
    /// and the reporter is left as it was, saying why.
    #[test]
    fn a_report_waiting_for_its_owner_leaves_the_locks_free_and_sees_a_lock_set_meanwhile() {
        let (state, _dir) = interrupted_hub_state("reporting-lock-while-waiting");
        state
            .store
            .update_session_lifecycle_if(
                "worker",
                SessionStatus::WaitingUser,
                SessionStatus::Idle,
                None,
            )
            .unwrap();
        let claim = state.begin_launch("hub").unwrap();
        let reporting = {
            let state = state.clone();
            std::thread::spawn(move || deliver_report(&state, "worker", done_report()))
        };

        std::thread::sleep(Duration::from_millis(300));
        assert!(!reporting.is_finished(), "the report waits for the claim");
        state.set_session_locked("worker", true).expect("not busy");
        drop(claim);
        let note = reporting.join().unwrap().expect("delivered");

        assert!(note.contains("was not archived"), "{note}");
        assert_eq!(kept_texts(&state).len(), 1);
        assert_eq!(
            state.session_record("worker").unwrap().status,
            SessionStatus::Idle
        );
    }

    /// A report that waits for its owner's claim while a handoff moves the reporter to the session
    /// that took the owner's place, and archives the owner, goes to the new owner.
    #[test]
    fn a_report_waiting_for_an_owner_that_is_replaced_goes_to_the_new_owner() {
        let (state, _dir) = interrupted_hub_state("reporting-owner-replaced");
        let heir = Session {
            id: "heir".to_string(),
            role: Role::Console,
            bound_to: None,
            status: SessionStatus::Interrupted,
            ..project_session("heir")
        };
        state.store.insert_session(&heir).unwrap();
        let claim = state.begin_launch("hub").unwrap();
        let handing_off = {
            let state = state.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                state.store.hand_over_binding("hub", "heir", None).unwrap();
                let mut hub = state.session_record("hub").unwrap();
                hub.status = SessionStatus::Archived;
                state.store.update_session(&hub).unwrap();
                drop(claim);
            })
        };

        deliver_report(&state, "worker", done_report()).expect("delivered to the heir");
        handing_off.join().unwrap();

        assert!(kept_texts(&state).is_empty());
        assert_eq!(state.store.kept_messages("heir").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn uncertain_native_report_keeps_credit_and_is_not_synthesised_again() {
        use tokio_tungstenite::tungstenite::{accept, Message};
        let (state, _dir) = crate::test_support::app_state("reporting-native-uncertain");
        state.store.insert_console(&console()).unwrap();
        let hub = Session {
            id: "hub".into(),
            role: Role::Console,
            bound_to: None,
            agent: Agent::Codex,
            ..project_session("hub")
        };
        state.store.insert_session(&hub).unwrap();
        let mut worker = project_session("worker");
        worker.id = "worker".into();
        worker.bound_to = Some("hub".into());
        worker.orchestration_role = OrchestrationRole::Worker;
        state.store.insert_session(&worker).unwrap();
        let (transport, listener) = crate::codex_transport::test_transport();
        let live = crate::test_support::fake_live_session("hub", Agent::Codex, 120, 40, "sleep 30")
            .with_codex_transport(transport);
        state.register_live(live.clone());
        state.turn_started("worker", Some("work"));
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket = accept(stream).unwrap();
            loop {
                let request: serde_json::Value =
                    serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
                match request["method"].as_str().unwrap() {
                    "initialize" => socket
                        .send(Message::Text(
                            serde_json::json!({
                                "id":request["id"], "result":{},
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                    "initialized" => {}
                    "thread/loaded/list" => socket
                        .send(Message::Text(
                            serde_json::json!({
                                "id":request["id"], "result":{"data":["thread"],"nextCursor":null},
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                    "thread/queue/add" => {
                        assert!(request["params"]["input"][0]["text"]
                            .as_str()
                            .unwrap()
                            .contains("summary"));
                        socket.close(None).unwrap();
                        break;
                    }
                    other => panic!("unexpected request: {other}"),
                }
            }
        });
        let error = deliver_own_report(
            &state,
            "worker",
            "summary".into(),
            ReportStatus::Done,
            vec![],
        )
        .await
        .unwrap_err();
        assert!(
            error.is::<crate::codex_transport::UncertainSubmission>(),
            "{error:#}"
        );
        assert_eq!(
            state.close_turn("worker", false),
            crate::state::TurnClose::Reported
        );
        assert_eq!(
            state.store.get_session("worker").unwrap().unwrap().status,
            SessionStatus::Idle
        );
        assert_eq!(
            state.write_hold("hub"),
            Some(crate::state::WriteHold::SubmissionUncertain)
        );
        assert_eq!(state.queued_count("hub"), 0);
        server.join().unwrap();
    }
}
