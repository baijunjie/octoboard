//! Archiving a session whose work is finished, which the session does itself: an owner archiving
//! itself, or a project session archived on its `done` report. What each requires of the sessions
//! under it is a [`Finished`] rule, and what keeps one from being archived is [`Unfinished`].

use std::sync::Arc;

use anyhow::Result;

use crate::messaging::naming::session_labels;
use crate::protocol::{AgentId, Session, SessionStatus};
use crate::session_tools::catalogue::qualified_tool_name;
use crate::state::bindings::{bound_sessions, team};
use crate::state::AppState;

use super::archive_session;

/// What a session's own archive requires of the sessions under it, as well as that none is locked.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Finished {
    /// An owner that reports to nobody archiving itself: every session below it, along both
    /// levels of its bindings, is archived.
    Team,
    /// A session archived on its `done` report: every session bound to it directly is archived.
    Direct,
}

/// What keeps a session from being archived as finished.
pub(crate) enum Unfinished {
    /// It reports to an owner, so it does not archive itself: the tool is offered to a coordinator
    /// with a parent too, and its `done` report is what archives it.
    ReportsToOwner,
    /// It is locked.
    Locked,
    /// Sessions under it are locked.
    TeamLocked(Vec<Session>),
    /// Sessions under it are not archived.
    NotArchived(Vec<Session>),
}

/// What became of [`archive_finished`].
pub(crate) enum Finish {
    /// Archived, with the sessions it archived as they were before (see [`archive_session`]).
    Archived(Vec<Session>),
    /// Archived by someone else already; nothing was changed.
    AlreadyArchived,
    /// Not archived: something keeps it from being archived as finished.
    Refused(Unfinished),
}

/// Why `session` cannot be archived as finished under `rule` as things stand, or `None`.
pub(crate) fn unfinished(
    state: &Arc<AppState>,
    session: &Session,
    rule: Finished,
) -> Result<Option<Unfinished>> {
    if rule == Finished::Team && session.bound_to.is_some() {
        return Ok(Some(Unfinished::ReportsToOwner));
    }
    if session.locked {
        return Ok(Some(Unfinished::Locked));
    }
    let team = team(state, session)?;
    let locked: Vec<Session> = team.iter().filter(|bound| bound.locked).cloned().collect();
    if !locked.is_empty() {
        return Ok(Some(Unfinished::TeamLocked(locked)));
    }
    let under = match rule {
        Finished::Team => team,
        Finished::Direct => bound_sessions(state, session)?,
    };
    let unarchived: Vec<Session> = under
        .into_iter()
        .filter(|bound| bound.status != SessionStatus::Archived)
        .collect();
    Ok((!unarchived.is_empty()).then_some(Unfinished::NotArchived(unarchived)))
}

/// The caller's own record, once it is a session that may archive itself now (`Finished::Team`);
/// otherwise a refusal worded for it.
pub(crate) fn archivable_self(state: &Arc<AppState>, caller: &Session) -> Result<Session> {
    let caller = state.session_record(&caller.id)?;
    match unfinished(state, &caller, Finished::Team)? {
        Some(why) => anyhow::bail!(why.refusal(&caller)),
        None => Ok(caller),
    }
}

/// Archives the session `id` and what is bound to it as [`archive_session`] does, unless it is
/// unfinished under `rule`. The checks are made under the claim that keeps a lock from changing
/// meanwhile, taken only now: held across a wait on the owner or on a survey of the session's
/// branches, it would refuse every change of a lock in the daemon for as long as that took.
pub(crate) fn archive_finished(state: &Arc<AppState>, id: &str, rule: Finished) -> Result<Finish> {
    let _removal = state.claim_session_removal();
    let session = state.session_record(id)?;
    if session.status == SessionStatus::Archived {
        return Ok(Finish::AlreadyArchived);
    }
    if let Some(why) = unfinished(state, &session, rule)? {
        return Ok(Finish::Refused(why));
    }
    Ok(Finish::Archived(archive_session(state, id)?))
}

impl Unfinished {
    /// What refuses the session `caller` its own archive, worded to it.
    pub(crate) fn refusal(&self, caller: &Session) -> String {
        match self {
            Unfinished::ReportsToOwner => format!(
                "this session reports to an owner, so it does not archive itself: call `{}` with \
                 status `done` and no open items, which archives it once the report is accepted",
                qualified_tool_name(caller.agent, "report")
            ),
            Unfinished::Locked => {
                "this session is locked, so it cannot be archived; the user has to unlock it first"
                    .to_string()
            }
            Unfinished::TeamLocked(locked) => format!(
                "this session cannot archive itself while sessions under it are locked: {}. The \
                 user has to unlock them first",
                session_labels(locked)
            ),
            Unfinished::NotArchived(unarchived) => {
                unarchived_refusal(caller.agent, &caller.id, unarchived)
            }
        }
    }
}

/// What refuses a session its own archive while sessions under it are not archived. A session
/// bound to a project coordinator is not the caller's to archive, so it is named under its
/// coordinator with different advice from the caller's direct sessions.
fn unarchived_refusal(caller: AgentId, caller_id: &str, unarchived: &[Session]) -> String {
    let archive_session = qualified_tool_name(caller, "archive_session");
    let (direct, under_coordinators): (Vec<&Session>, Vec<&Session>) = unarchived
        .iter()
        .partition(|session| session.bound_to.as_deref() == Some(caller_id));
    let mut advice = Vec::new();
    if !direct.is_empty() {
        advice.push(format!(
            "{}: an owner archives its sessions once their tasks are done, so an unarchived one \
             means its task is not finished; finish it and archive it with `{archive_session}` first",
            session_labels(direct)
        ));
    }
    let mut coordinators: Vec<&str> = under_coordinators
        .iter()
        .filter_map(|session| session.bound_to.as_deref())
        .collect();
    coordinators.sort_unstable();
    coordinators.dedup();
    for coordinator in coordinators {
        let named = session_labels(
            under_coordinators
                .iter()
                .copied()
                .filter(|session| session.bound_to.as_deref() == Some(coordinator)),
        );
        advice.push(format!(
            "{named} under the project coordinator `{coordinator}`: not yours to archive; wait for the \
             coordinator to finish them (its `done` report archives it), or archive `{coordinator}` with \
             `{archive_session}`, which takes along its sessions that have no process running and \
             is refused while any has one"
        ));
    }
    format!(
        "this session cannot archive itself while sessions under it are not archived. {}. Then \
         archive yourself",
        advice.join(". ")
    )
}
