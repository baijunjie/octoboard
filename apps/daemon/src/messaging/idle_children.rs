//! Children left idle after their last report: the judgement of a session with a parent that sits
//! at its prompt with nothing running after its last report reached its owner, which the session
//! records give as `idle_after_report`, and the one notice its owner gets once that has lasted
//! [`NUDGE_AFTER`].
//!
//! A session is archived by itself only through its own `done` report with no open items. One that
//! finished without such a report — it never called `report`, or reported with open items —
//! stays up, and this is what keeps its owner from forgetting it. The owner is only told:
//! archiving stays its decision, or the user's.
//!
//! What it rests on is the session's turn bookkeeping, held in memory and dropped with its
//! process: a session that is resumed, or comes back after the daemon restarts, is not idle after
//! a report until a report of its new process reaches its owner.

use std::sync::Arc;
use std::time::Duration;

use crate::messaging::naming::one_line;
use crate::messaging::owner_notices::tell_owner;
use crate::messaging::texts::{session_notice_opening, IDLE_PHRASE};
use crate::protocol::{Role, Session, SessionStatus};
use crate::session_tools::catalogue;
use crate::state::{bindings, AppState, LastReport};
use crate::time::now_millis;

/// How long a child must have stayed idle after its last report before its owner is told.
pub(crate) const NUDGE_AFTER: Duration = Duration::from_secs(10 * 60);

/// How often a child's idleness is looked at while its owner may be owed the notice.
const LOOK_EVERY: Duration = Duration::from_secs(30);

/// The last report of `session` when the session is idle after it: it has a parent, the report
/// (its own or synthesised) was accepted for that parent since a turn of its own last opened (see
/// [`AppState::turn_opened`]), it is shown idle with no work left in the background, nothing is
/// queued for it, and no session bound to it has a process. `None` otherwise — working, waiting
/// for the user, dormant, with a message on its way, a turn of its own started since, or no
/// process.
pub(crate) fn idle_after_report(state: &Arc<AppState>, session: &Session) -> Option<LastReport> {
    idle_after_report_shown(state, session, state.shown_status(session))
}

/// [`idle_after_report`] for a caller that has read the status shown, `shown`, already, so that
/// what it describes of the session cannot disagree with it.
pub(crate) fn idle_after_report_shown(
    state: &Arc<AppState>,
    session: &Session,
    shown: SessionStatus,
) -> Option<LastReport> {
    session.bound_to.as_ref()?;
    let last = state.last_report(&session.id)?;
    if shown != SessionStatus::Idle
        || !state.shown_background_tasks(&session.id, shown).is_empty()
        || state.queued_count(&session.id) > 0
    {
        return None;
    }
    // Only a coordinator has sessions of its own; listing the table is left to it.
    if session.role == Role::Coordinator {
        let bound = bindings::bound_sessions(state, session).ok()?;
        if bound.iter().any(|bound| state.has_process(&bound.id)) {
            return None;
        }
    }
    Some(last)
}

/// Notes that the report `synthesised` or of `session_id`'s own was accepted for its owner, unless
/// a turn of the session has opened since `opened` was read (see
/// [`AppState::note_report_accepted`]), and, on a runtime, watches the session from then on for
/// its owner to be told once it has stayed idle after it for [`NUDGE_AFTER`]. The watch ends with
/// the report no longer the session's last — a turn of its own opened, another report came, its
/// process ended — or once the owner is told.
pub(crate) fn report_accepted(
    state: &Arc<AppState>,
    session_id: &str,
    synthesised: bool,
    opened: u64,
) {
    let Some(number) = state.note_report_accepted(session_id, synthesised, opened) else {
        return;
    };
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let (state, session_id) = (state.clone(), session_id.to_string());
    runtime.spawn(async move {
        let mut idle_since = None;
        loop {
            tokio::time::sleep(LOOK_EVERY).await;
            match look(&state, &session_id, number, &mut idle_since, now_millis()) {
                Look::Wait => {}
                Look::Stop => return,
                Look::Tell => {
                    let state = state.clone();
                    let session_id = session_id.clone();
                    // Writing into the owner blocks.
                    let _ = tokio::task::spawn_blocking(move || {
                        tell_owner_it_is_idle(&state, &session_id, number)
                    })
                    .await;
                    return;
                }
            }
        }
    });
}

/// What one look at a watched child decides.
#[derive(Debug, PartialEq, Eq)]
enum Look {
    /// Not yet: look again later.
    Wait,
    /// The report watched is no longer the session's last, or its owner was told already.
    Stop,
    /// The session has been idle after the report for [`NUDGE_AFTER`]: tell its owner.
    Tell,
}

/// One look at `session_id`, watched for its report `number`, at `now` (milliseconds):
/// `idle_since` is when it was first seen idle after it without a break, cleared when it is not.
fn look(
    state: &Arc<AppState>,
    session_id: &str,
    number: u64,
    idle_since: &mut Option<i64>,
    now: i64,
) -> Look {
    let session = match state.store.get_session(session_id) {
        Ok(Some(session)) if !session.status.has_no_process() => session,
        // Archived, or its process gone: its bookkeeping, and the report watched, go with it.
        _ => return Look::Stop,
    };
    match state.last_report(session_id) {
        Some(last) if last.number == number && !last.nudged => {}
        _ => return Look::Stop,
    }
    if idle_after_report(state, &session).is_none() {
        *idle_since = None;
        return Look::Wait;
    }
    let since = *idle_since.get_or_insert(now);
    if now.saturating_sub(since) >= NUDGE_AFTER.as_millis() as i64 {
        Look::Tell
    } else {
        Look::Wait
    }
}

/// Tells the owner of `session_id` that the session stayed idle after its report `number`, once
/// for that report and only while it still is, as [`tell_owner`] does.
///
/// **Blocks** on writing into the owner.
pub(crate) fn tell_owner_it_is_idle(state: &Arc<AppState>, session_id: &str, number: u64) {
    let Ok(Some(session)) = state.store.get_session(session_id) else {
        return;
    };
    match idle_after_report(state, &session) {
        Some(last) if last.number == number => {}
        _ => return,
    }
    if !state.claim_idle_nudge(session_id, number) {
        return;
    }
    let synthesised = state
        .last_report(session_id)
        .is_some_and(|last| last.synthesised);
    tell_owner(state, &session, |owner| {
        idle_notice(&session, synthesised, |name| {
            catalogue::qualified_tool_name(owner.agent, name)
        })
    });
}

/// The notice that `session` stayed idle after its last report, `synthesised` or its own, with
/// the owner's tools named by `tool`.
fn idle_notice(session: &Session, synthesised: bool, tool: impl Fn(&str) -> String) -> String {
    let which = if synthesised {
        "Octoboard synthesised that report, as it stopped without reporting"
    } else {
        "That report was its own, and did not archive it"
    };
    format!(
        "{opening}{title}{IDLE_PHRASE} for {minutes} minutes since its last report reached you, \
         with nothing running. {which}. If its work is over, check it with `{get_session}` and \
         archive it with `{archive_session}`; otherwise give it its next instruction with \
         `{instruct_session}`. You are not reminded again until it reports again.",
        opening = session_notice_opening(session),
        title = one_line(&session.title),
        minutes = NUDGE_AFTER.as_secs() / 60,
        get_session = tool("get_session"),
        archive_session = tool("archive_session"),
        instruct_session = tool("instruct_session"),
    )
}

#[cfg(test)]
mod tests;
