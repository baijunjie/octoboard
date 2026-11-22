//! Archiving and deleting sessions together with what is bound to them, the inspections a client
//! makes before either, and a session's title, pin, lock and colour.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::branch_cleanup;
use crate::hostfs::trash::TrashFailure;
use crate::protocol::{
    error_code, CodedError, ConsoleSessionColour, Event, Session, SessionBranchItem,
    SessionBranchTarget, SessionKind, SessionStatus,
};
use crate::records;
use crate::state::bindings::{bound_sessions, team};
use crate::state::refusals::ensure_unlocked;
use crate::state::{AppState, ChangeAdmission, Deletion};

use super::launch::stop_process;
use super::validate::required_text;

mod finished;
mod record_trash;
pub(crate) use finished::{
    archivable_self, archive_finished, unfinished, Finish, Finished, Unfinished,
};
pub(super) use record_trash::host_trash;
use record_trash::trash_agent_records_of;

/// Ends the session's process and archives it. What archiving means beyond ending the process
/// belongs here and not in [`stop_process`], which a switch of the session's account shares.
///
/// Archiving a session that has sessions bound to it — a console session, or a project session
/// that started some — is a group operation along the binding, at both levels: a console
/// session's bound sessions include its bound coordinators, and theirs come along. It is decided by
/// whether a process is running and not by status: refused, with nothing changed, while any
/// session in the group has one (working, awaiting instructions or waiting for the user alike, at
/// either level; the refusal counts and names them), and otherwise archiving every session in the
/// group that is not archived yet, before the owner itself. Nothing bound to it is left outside the
/// archive. The order is deliberate: the deepest level first and the owner last, so a failure part
/// way leaves only a state the user could have produced by hand, never an archived owner with a
/// session bound to it still outside the archive. A session with nothing bound to it — one with
/// the role of a worker, or an owner that started none — never reaches another session: the
/// owner's tool, a `done` report with no open items, a `delete_project` that stops sessions.
/// The user's control-socket request instead explicitly ends running descendants too.
///
/// Returns the sessions it archived, as they were before, for the removal of the branches bound
/// to them (`branch_cleanup`), which is the caller's: a project's deletion stops its
/// sessions this way and removes none.
///
/// Refused with `daemon_shutting_down`, with nothing changed, once a planned stop has raised its
/// handover barrier (`AppState::admit_change`): the terminals are the next daemon's.
pub fn archive_session(state: &Arc<AppState>, id: &str) -> Result<Vec<Session>> {
    let admission = state.admit_change()?;
    archive_admitted(state, &admission, id)
}

/// [`archive_session`] for an operation admitted past the handover barrier already, which holds
/// `_admission` until it is done: a step of it, which no barrier raised meanwhile cuts off.
pub(crate) fn archive_admitted(
    state: &Arc<AppState>,
    _admission: &ChangeAdmission,
    id: &str,
) -> Result<Vec<Session>> {
    archive_session_group(state, id, false)
}

/// Archives `session` and what is bound to it as an `archive_session` request asks: the process
/// trees in `keep_process_ids` are kept from being stopped with it, and its branches are removed
/// once it is down, the locked worktrees in `force_remove` included and those in `keep` left.
/// Refused, with nothing kept or archived, when one of those trees cannot be kept because a
/// process of it is on its agent's own console or in the process group of a command the agent
/// ends as it exits ([`super::processes::keep`]).
pub(super) async fn archive_requested(
    state: &Arc<AppState>,
    session: String,
    force_remove: Vec<SessionBranchTarget>,
    keep: Vec<SessionBranchTarget>,
    keep_process_ids: Vec<String>,
) -> Result<()> {
    let _admission = state.admit_change()?;
    // Before the archive ends the processes, which the exit path then stops; kept after
    // the record is known so an unknown session keeps nothing.
    if !keep_process_ids.is_empty() {
        state.session_record(&session)?;
        super::processes::keep_chosen(state, &session, keep_process_ids).await?;
    }
    let archived = archive_session_group(state, &session, true)?;
    remove_branches_once_stopped(state, archived, force_remove, keep);
    Ok(())
}

/// Refuses, with nothing changed, a session or a session bound to it that is locked, as an archive
/// of `id` would: for an operation that must refuse before it changes anything of its own ahead
/// of the archive ([`archive_keeping_branches`]). The archive checks again under its own claim.
pub(super) fn ensure_group_unlocked(state: &Arc<AppState>, id: &str) -> Result<()> {
    let session = state.session_record(id)?;
    let descendants = team(state, &session)?;
    ensure_unlocked(std::iter::once(session).chain(descendants))
}

/// Archives `id` and what is bound to it as a user's `archive_session` does, running descendants
/// included, but removes no branch or worktree, which is what opening a session's conversation in
/// its agent's application needs: the application works in the session's directory. A locked
/// session or descendant refuses it, with nothing changed. The processes are ended as an archive
/// ends them, not awaited: the caller waits for the one it needs gone.
pub(super) fn archive_keeping_branches(
    state: &Arc<AppState>,
    _admission: &ChangeAdmission,
    id: &str,
) -> Result<()> {
    archive_session_group(state, id, true).map(drop)
}

/// A user-confirmed archive may end every process in the group; agent tools retain the
/// unfinished-work safeguard. Validate all locks before changing any session. Returns the sessions
/// archived, as [`archive_session`] does.
fn archive_session_group(
    state: &Arc<AppState>,
    id: &str,
    stop_descendants: bool,
) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    let session = state.session_record(id)?;
    let descendants = team(state, &session)?;
    ensure_unlocked(std::iter::once(session.clone()).chain(descendants.clone()))?;
    let mut archived = Vec::new();
    if stop_descendants {
        for bound in descendants {
            if bound.status != SessionStatus::Archived {
                archive_record(state, &bound.id)?;
                archived.push(bound);
            }
        }
        archive_record(state, id)?;
        archived.push(session);
        return Ok(archived);
    }
    // "Not running" is taken to mean interrupted by exclusion: a status that is neither archived
    // nor backed by a process would be archived here without anyone having decided so. One gap is
    // left open: `begin_open` inserts the session record, `bound_to` included, before it takes
    // the launch claim, so in between a bound session reads as neither archived nor
    // process-backed. Nothing awaits in that gap, so no archive can land in it, and it is not
    // closed.
    let (running, no_process): (Vec<Session>, Vec<Session>) = descendants
        .into_iter()
        .filter(|bound| bound.status != SessionStatus::Archived)
        .partition(|bound| state.has_process(&bound.id));
    if !running.is_empty() {
        return Err(CodedError::raised(
            error_code::SESSION_HAS_RUNNING_SESSIONS,
            format!(
                "this session cannot be archived while sessions under it have a process \
                 running: {}; archive them or wait for them to finish first",
                session_titles(&running)
            ),
            &[
                ("count", &running.len().to_string()),
                ("sessions", &session_titles(&running)),
                (
                    "session_ids",
                    &serde_json::to_string(
                        &running
                            .iter()
                            .map(|session| &session.id)
                            .collect::<Vec<_>>(),
                    )?,
                ),
            ],
        ));
    }
    for bound in no_process {
        archive_record(state, &bound.id)?;
        archived.push(bound);
    }
    archive_record(state, id)?;
    archived.push(session);
    Ok(archived)
}

/// Removes the branches and worktrees bound to `sessions` once their processes have ended, off
/// the request's path, forcing what `forced` names; every client is told of what is still there
/// and was not kept on purpose (`session_branches_not_removed`, see
/// `branch_cleanup::report::to_report`). The archive or deletion it follows stands either way.
fn remove_branches_once_stopped(
    state: &Arc<AppState>,
    sessions: Vec<Session>,
    forced: Vec<SessionBranchTarget>,
    keep: Vec<SessionBranchTarget>,
) {
    if sessions.is_empty() {
        return;
    }
    let state = Arc::clone(state);
    // Counted from here, while the request still holds its admission, so that a planned stop
    // waits for the removal and the removal does not start once it has stopped waiting.
    let covered = state.track_change();
    tokio::spawn(async move {
        let left = branch_cleanup::remove::remove_once_stopped(
            state.clone(),
            sessions,
            forced.clone(),
            &covered,
        )
        .await;
        report_not_removed(
            &state,
            &branch_cleanup::report::to_report(left, &forced, &keep),
        );
    });
}

/// Tells every client of the branches and worktrees an archive, a deletion or a project's removal
/// was to take and that are still there (`session_branches_not_removed`); nothing when there are
/// none.
pub(super) fn report_not_removed(state: &AppState, missed: &[SessionBranchItem]) {
    if missed.is_empty() {
        return;
    }
    let failure = CodedError::raised(
        error_code::SESSION_BRANCHES_NOT_REMOVED,
        format!(
            "these were to be removed and are still there: {}",
            branch_cleanup::report::lines(missed).join("; ")
        ),
        &[
            ("count", &missed.len().to_string()),
            ("items", &branch_cleanup::report::for_client(missed)),
        ],
    );
    state.broadcast(Event::from_error(None, &failure));
}

/// Answers an `inspect_session_branches` request: what archiving or deleting `sessions` would
/// remove of their branches and worktrees, and with `processes` their long-running process trees.
pub(super) async fn inspect_session_branches(
    state: &Arc<AppState>,
    request_id: Option<String>,
    sessions: Vec<String>,
    processes: bool,
) -> Result<Option<Event>> {
    let ids = processes.then(|| sessions.clone());
    let inspected = Arc::clone(state);
    let items = tokio::task::spawn_blocking(move || {
        let mut found = Vec::new();
        for id in &sessions {
            found.extend(inspected.store.get_session(id)?);
        }
        branch_cleanup::survey::inspect(&inspected, &found, crate::host_tools::Pace::Person)
    })
    .await??;
    let processes = match ids {
        Some(ids) => {
            super::processes::long_running(state, ids, super::processes::ask_after()).await?
        }
        None => Vec::new(),
    };
    Ok(Some(Event::SessionBranchesInspected {
        id: request_id,
        items,
        processes,
    }))
}

/// English fallback titles; clients use the structured IDs to word and render the list.
fn session_titles(sessions: &[Session]) -> String {
    sessions
        .iter()
        .map(|session| session.title.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Marks one session archived and ends its process, if it has one.
pub(super) fn archive_record(state: &Arc<AppState>, id: &str) -> Result<()> {
    ensure_unlocked([state.session_record(id)?])?;
    state.archive_session_record(id)?;
    state.forget_left_for_limit(id);
    if let Some(live) = state.live_session(id) {
        // Ending the process waits out the graceful period, which the caller should not.
        stop_process(state, live);
    }
    // Messages kept for it to take at a resume are not for it once archived: a later reopen must
    // not be handed them.
    if let Err(err) = state.store.clear_kept_messages(id) {
        tracing::warn!(
            session = %id, %err,
            "dropping the messages kept for an archived session failed"
        );
    }
    Ok(())
}

/// The ids of the stored sessions that match, for a deletion that drops them by cascade.
pub(super) fn session_ids_where(
    state: &Arc<AppState>,
    matches: impl Fn(&Session) -> bool,
) -> Result<Vec<String>> {
    Ok(state
        .store
        .list_sessions()?
        .into_iter()
        .filter(|session| matches(session))
        .map(|session| session.id)
        .collect())
}

/// Removes Octoboard's record of one session: an archived one, and with an archived owner (a
/// console session, or a project session that started some) the archived sessions bound to it, at
/// both levels: left behind, reopening one would have no owner to come back with. Deleting an
/// archived bound session on its own leaves its owner alone.
///
/// A session that is not archived is deleted only while it is unused: no turn yet, with none
/// bound to it, and bound to no owner or opened by the user. That is decided in the same step as
/// the delete (see `Store::delete_session_if_unused`), so a turn racing the request wins and the
/// request is refused. Its process is ended after the record is gone, and not before, so a
/// refused delete never costs the session its process.
///
/// The record takes its saved terminal output with it. Nothing else of Octoboard's hangs off a
/// session: its queue, MCP token and turn bookkeeping go when its process does, and so does its
/// scratch directory (clearing the run directory at startup is only the fallback). The project is
/// never touched. The caller hands every session deleted, the archived sessions bound to it
/// included, to [`trash_agent_records_with`], which decides whose native records move to the
/// Trash ([`may_move_agent_records`]).
pub(super) fn delete_session(state: &Arc<AppState>, id: &str) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    let session = state.session_record(id)?;
    ensure_unlocked(std::iter::once(session.clone()).chain(team(state, &session)?))?;
    let mut gone = Vec::new();
    let deleted = if session.status == SessionStatus::Archived {
        delete_archived_group(state, &session, &mut gone)?
    } else {
        match state.delete_if_unused(id)? {
            Deletion::Deleted { process, session } => {
                if let Some(live) = process {
                    stop_process(state, live);
                }
                gone.push(*session);
                true
            }
            Deletion::Refused => false,
        }
    };
    if !deleted {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_DELETABLE,
            "only an archived session, or one that has never been used, can be deleted, and not \
             while it is starting",
            &[("session", id)],
        ));
    }
    Ok(gone)
}

pub(super) fn inspect_session_records(
    state: &AppState,
    ids: &[String],
    mut locator: records::Locator,
) -> Result<crate::protocol::SessionRecordPresence> {
    use crate::protocol::SessionRecordPresence::{Absent, Present, Unknown};
    let accounts = state.store.list_accounts()?;
    let mut presence = Absent;
    for id in ids {
        let Some(session) = state.store.get_session(id)? else {
            presence = Unknown;
            continue;
        };
        match records::presence(&state.record_hints, &mut locator, &accounts, &session) {
            Present => return Ok(Present),
            Unknown => presence = Unknown,
            Absent => {}
        }
    }
    Ok(presence)
}

fn delete_unused_session(state: &Arc<AppState>, id: &str) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    ensure_unlocked([state.session_record(id)?])?;
    if let Deletion::Deleted { process, session } = state.delete_if_unused(id)? {
        if let Some(live) = process {
            stop_process(state, live);
        }
        return Ok(vec![*session]);
    }
    Err(CodedError::raised(
        error_code::SESSION_NOT_DELETABLE,
        "the session is no longer unused",
        &[("session", id)],
    ))
}

/// What the user said, in the deletion they confirmed, about the agent's own records of the
/// sessions it removes. Only a request of the user's can carry [`RecordsConsent::Ticked`]: a
/// deletion nobody confirmed has no tick to pass. Private to this module, with
/// [`trash_agent_records_with`], so no other path can pass a tick of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordsConsent {
    /// Nobody ticked the cleanup option of a deletion of one session and what goes with it: left
    /// unticked, or a deletion that asks nothing.
    Unticked,
    /// Nobody ticked the cleanup option of a deletion of every archived session in a scope.
    UntickedBulk,
    /// The user ticked the confirmation's option to move the agent's own records too.
    Ticked,
}

impl RecordsConsent {
    /// From a `delete_session` request's `trash_agent_records`.
    fn of_request(trash_agent_records: bool) -> Self {
        if trash_agent_records {
            Self::Ticked
        } else {
            Self::Unticked
        }
    }

    /// From a `delete_archived_sessions` request's `trash_agent_records`.
    fn of_bulk_request(trash_agent_records: bool) -> Self {
        if trash_agent_records {
            Self::Ticked
        } else {
            Self::UntickedBulk
        }
    }
}

/// Whether deleting `session` may move its agent's own records to the Trash: the one place that
/// decides it, for every path that deletes a session. The records of a session that never had a
/// conversation are what Octoboard's own launch made the agent write, so they go with it; those of
/// a session that had one hold the user's conversation, and move only on the user's tick. The
/// deletion of every archived session in a scope is stricter: left unticked it moves nothing, a
/// never-used session's records included, since it removes many sessions on one confirmation. A
/// session let through without the tick counts as never used only by Octoboard's own record of it,
/// which rests on the agent's report of the first prompt, so [`trash_agent_records_with`] goes on
/// to ask the agent's records themselves ([`borne_out_as_unused`]).
fn may_move_agent_records(session: &Session, consent: RecordsConsent) -> bool {
    match consent {
        RecordsConsent::Ticked => true,
        RecordsConsent::Unticked => !session.has_conversation,
        RecordsConsent::UntickedBulk => false,
    }
}

/// Of `unused`, sessions whose records may move only because they count as never having had a
/// conversation, those whose records bear that out: none of them holds a prompt the user sent
/// (`records::user_prompt`). One whose record holds such a prompt had a conversation the agent
/// never reported, and one whose records cannot be read through, hold a line that was skipped or
/// cannot be placed, or cannot be told a prompt from, may have: the records of both stay where
/// they are. Reads the records, so the caller is on a
/// blocking task and holds no lock.
fn borne_out_as_unused(
    state: &AppState,
    unused: Vec<Session>,
    locator: &mut records::Locator,
) -> Vec<Session> {
    let accounts = match state.store.list_accounts() {
        Ok(accounts) => accounts,
        Err(err) => {
            tracing::warn!(%err, "reading the accounts before trashing the agent's records failed");
            return Vec::new();
        }
    };
    unused
        .into_iter()
        .filter(|session| {
            let found =
                records::user_prompt::of_session(&state.record_hints, locator, &accounts, session);
            let unused = found == records::user_prompt::UserPrompt::NotHeld;
            if !unused {
                tracing::info!(
                    session = %session.id, ?found,
                    "leaving the agent's records of a session that counted as unused"
                );
            }
            unused
        })
        .collect()
}

/// [`trash_agent_records_with`] for a deletion nobody confirmed, such as the automatic deletion of
/// archived sessions whose conversation is gone: with no tick, it moves only the records of the
/// sessions that never had a conversation, once their records bear that out
/// ([`borne_out_as_unused`]), and leaves a used session's where they are.
pub(super) async fn trash_unconfirmed_agent_records_with(
    state: &Arc<AppState>,
    deleted: Vec<Session>,
    locator: records::Locator,
    trash: impl Fn(&Path) -> Result<(), TrashFailure> + Send + 'static,
) {
    trash_agent_records_with(state, deleted, RecordsConsent::Unticked, locator, trash).await;
}

/// [`trash_agent_records_of`], for those of `deleted` whose records may move under `consent`
/// ([`may_move_agent_records`]) and, without the user's tick, whose records bear out that they
/// were never used ([`borne_out_as_unused`]), on a blocking task: a move across volumes copies,
/// and the accounts' records are listed and read. The delete has happened already, so nothing here
/// fails the request: a failure is logged.
async fn trash_agent_records_with(
    state: &Arc<AppState>,
    deleted: Vec<Session>,
    consent: RecordsConsent,
    locator: records::Locator,
    trash: impl Fn(&Path) -> Result<(), TrashFailure> + Send + 'static,
) {
    let deleted: Vec<Session> = deleted
        .into_iter()
        .filter(|session| may_move_agent_records(session, consent))
        .collect();
    if deleted.is_empty() {
        return;
    }
    let state = Arc::clone(state);
    let moved = tokio::task::spawn_blocking(move || {
        let mut locator = locator;
        let deleted = match consent {
            RecordsConsent::Ticked | RecordsConsent::UntickedBulk => deleted,
            RecordsConsent::Unticked => borne_out_as_unused(&state, deleted, &mut locator),
        };
        trash_agent_records_of(&state, &deleted, locator, trash)
    })
    .await;
    if let Err(err) = moved {
        tracing::warn!(%err, "moving the agent's session records to the Trash did not finish");
    }
}

/// Deletes `session` if it is archived and says whether it did; the archived sessions bound to it
/// go along, each with its own archived bound sessions, so an archived console session takes its
/// archived bound coordinators' archived sessions too. Deleting the owner first means a refusal,
/// which only that first step can give, deletes nothing. It does not make the whole atomic: if a
/// later step is skipped (a session being launched right now) or fails, the owner is already gone.
/// That is left, because a bound session can only be mid-launch through a resume, which reopens the
/// owner first, so the owner was no longer archived and the delete was refused. One that stopped
/// being archived meanwhile is skipped. Each session deleted is added to `deleted`, also when a
/// later step fails. The automatic cleanup meets none of the skips: it checks under the removal
/// claim that no member is launching or has a process.
pub(crate) fn delete_archived_group(
    state: &Arc<AppState>,
    session: &Session,
    deleted: &mut Vec<Session>,
) -> Result<bool> {
    if !state.delete_if_archived(&session.id)? {
        return Ok(false);
    }
    deleted.push(session.clone());
    for bound in bound_sessions(state, session)? {
        // One that is not archived keeps its own bound sessions, which still have an owner.
        delete_archived_group(state, &bound, deleted)?;
    }
    Ok(true)
}

/// Deletes every archived session of `project`, or every archived session bound to the console
/// session `console_session_id`, or with neither every archived console session of `console` along
/// with the archived sessions bound to them; an archived bound coordinator in either of the last
/// two takes its own archived sessions along. One that stopped being archived meanwhile (a resume
/// got there first) is skipped. Returns every session deleted.
fn delete_archived_sessions(
    state: &Arc<AppState>,
    console_id: &str,
    project_id: Option<&str>,
    console_session_id: Option<&str>,
) -> Result<Vec<Session>> {
    let _removal = state.claim_session_removal();
    state.store.require_console(console_id)?;
    if project_id.is_some() && console_session_id.is_some() {
        return Err(CodedError::raised(
            error_code::CONFLICTING_FIELDS,
            "`project` and `console_session` cannot both be given",
            &[("first", "project"), ("second", "console_session")],
        ));
    }
    if let Some(project_id) = project_id {
        let project = state.store.require_project(project_id)?;
        if project.console_id != console_id {
            return Err(CodedError::unknown_project(project_id));
        }
    }
    if let Some(console_session_id) = console_session_id {
        let owner = state.session_record(console_session_id)?;
        if owner.console_id != console_id || owner.kind != SessionKind::Console {
            return Err(CodedError::unknown_session(console_session_id));
        }
    }
    let mut deleted = Vec::new();
    let mut selected = Vec::new();
    for session in state.store.list_sessions()? {
        let in_scope = session.console_id == console_id
            && match (project_id, console_session_id) {
                (Some(project_id), _) => session.project_id.as_deref() == Some(project_id),
                (None, Some(owner)) => session.bound_to.as_deref() == Some(owner),
                (None, None) => session.kind == SessionKind::Console,
            };
        if in_scope && session.status == SessionStatus::Archived {
            ensure_unlocked(std::iter::once(session.clone()).chain(team(state, &session)?))?;
            selected.push(session);
        }
    }
    for session in selected {
        delete_archived_group(state, &session, &mut deleted)?;
    }
    Ok(deleted)
}

/// Deletes `session` and what goes with it as a `delete_session` request asks, only if it is unused
/// with `only_if_unused`. Its branches are removed once it is down, and its agent records go to
/// the Trash with `trash_agent_records`, or when it has no conversation to keep and its records
/// bear that out: without the tick, the records of a session that counts as unused stay where
/// they are when one of them holds a prompt the user sent or cannot be read through
/// ([`borne_out_as_unused`]).
pub(super) async fn delete_requested(
    state: &Arc<AppState>,
    session: String,
    trash_agent_records: bool,
    only_if_unused: bool,
    force_remove: Vec<SessionBranchTarget>,
    keep: Vec<SessionBranchTarget>,
) -> Result<()> {
    delete_requested_with(
        state,
        session,
        RecordsConsent::of_request(trash_agent_records),
        only_if_unused,
        (force_remove, keep),
        records::Locator::default(),
        host_trash(state),
    )
    .await
}

/// [`delete_requested`], finding the agent's records through `locator` and moving them through
/// `trash`: the Trash proper and the default accounts for a request, a test's own otherwise.
async fn delete_requested_with(
    state: &Arc<AppState>,
    session: String,
    consent: RecordsConsent,
    only_if_unused: bool,
    (force_remove, keep): (Vec<SessionBranchTarget>, Vec<SessionBranchTarget>),
    locator: records::Locator,
    trash: impl Fn(&Path) -> Result<(), TrashFailure> + Send + 'static,
) -> Result<()> {
    let _admission = state.admit_change()?;
    let deleted = if only_if_unused {
        delete_unused_session(state, &session)?
    } else {
        delete_session(state, &session)?
    };
    remove_branches_once_stopped(state, deleted.clone(), force_remove, keep);
    trash_agent_records_with(state, deleted, consent, locator, trash).await;
    Ok(())
}

/// Deletes the archived sessions a `delete_archived_sessions` request selects, removing their
/// branches once they are down and, with `trash_agent_records`, moving their agent records to the
/// Trash. Without it nothing moves, a never-used session's records included: the bulk deletion
/// moves records only on the user's tick ([`RecordsConsent::UntickedBulk`]).
pub(super) async fn delete_archived_requested(
    state: &Arc<AppState>,
    console: String,
    project: Option<String>,
    console_session: Option<String>,
    trash_agent_records: bool,
    force_remove: Vec<SessionBranchTarget>,
    keep: Vec<SessionBranchTarget>,
) -> Result<()> {
    let _admission = state.admit_change()?;
    let deleted = delete_archived_sessions(
        state,
        &console,
        project.as_deref(),
        console_session.as_deref(),
    )?;
    remove_branches_once_stopped(state, deleted.clone(), force_remove, keep);
    trash_agent_records_with(
        state,
        deleted,
        RecordsConsent::of_bulk_request(trash_agent_records),
        records::Locator::default(),
        host_trash(state),
    )
    .await;
    Ok(())
}

/// Renames a session and broadcasts it. The title is stored trimmed and cannot be blank.
pub fn rename_session(state: &Arc<AppState>, id: &str, title: &str) -> Result<Session> {
    state.set_session_title(id, &required_text("title", title)?)
}

/// Pins or unpins a session and broadcasts it.
pub fn set_session_pinned(state: &Arc<AppState>, id: &str, pinned: bool) -> Result<Session> {
    state.store.set_session_pinned(id, pinned)?;
    let session = state.session_record(id)?;
    state.publish_session(&session);
    Ok(session)
}

pub fn set_session_locked(state: &Arc<AppState>, id: &str, locked: bool) -> Result<Session> {
    state.set_session_locked(id, locked)?;
    let session = state.session_record(id)?;
    state.publish_session(&session);
    Ok(session)
}

/// Sets a console session's badge colour and broadcasts it. A project session carries none.
pub fn set_session_colour(
    state: &Arc<AppState>,
    id: &str,
    colour: ConsoleSessionColour,
) -> Result<Session> {
    if state.session_record(id)?.kind != SessionKind::Console {
        anyhow::bail!("only a console session has a colour");
    }
    state.store.set_session_colour(id, colour)?;
    let session = state.session_record(id)?;
    state.publish_session(&session);
    Ok(session)
}

#[cfg(test)]
mod tests;
