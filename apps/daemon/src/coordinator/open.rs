//! Opening a session: resolving what a request opens, recording it as starting, settling its
//! launch, and refusing beforehand a session that could not be opened.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::env_shell;
use crate::protocol::{
    error_parts, notice_code, now_millis, Agent, CodedError, Console, Notice, OrchestrationRole,
    Origin, Project, Role, Session, SessionStatus, TerminalAppearance,
};
use crate::session::term;
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;

use super::account_choice::{
    automatic_open_choice, refuse_if_account_not_signed_in, refuse_if_account_quota_exhausted,
    refuse_if_agent_unavailable, refuse_unless_agent_added, session_account, session_agent,
};
use super::accounts::refuse_if_account_disabled;
use super::bindings::resolve_bound_to;
use super::launch::{check_launch_settings, start_process, stop_process, SettingsChecked};
use super::resume::{
    checked_resume_id, locate_conversation_to_resume, refuse_if_conversation_recorded,
};
use super::Followup;

/// One session to open. A struct rather than a parameter list because the two callers differ in
/// more than one field — the user opening a session by hand, and the console session dispatching
/// one — and the fields that differ are all optional strings that would otherwise be positional.
#[derive(Clone)]
pub struct OpenRequest {
    pub console_id: String,
    pub project_id: Option<String>,
    pub agent: Option<Agent>,
    /// The account chosen for this session: `None` leaves it to the project's reference for the
    /// session's agent, else the console's; `Some(None)` chooses the default account outright. See
    /// [`session_account`].
    pub account: Option<Option<String>>,
    /// A model or effort override, preserving absent versus explicitly unset.
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    pub task: Option<String>,
    pub title: Option<String>,
    pub origin: Origin,
    /// The session this (project) session should report to — a console session, or an unbound
    /// project session of the same project — or `None` for one outside the orchestration. Ignored
    /// when a console session itself is being opened, which is never bound.
    pub bound_to: Option<String>,
    /// The agent's own id of a conversation started outside Octoboard, which this (project)
    /// session continues instead of starting one. What is refused is in [`checked_resume_id`] and
    /// [`locate_conversation_to_resume`].
    pub resume_agent_session_id: Option<String>,
    /// The id to record the session under, for a caller that needs it before the session exists
    /// (a handoff names its package by it); `None` makes a new one.
    pub session_id: Option<String>,
    /// The appearance of the terminal this launch will be shown in. Not recorded on the session.
    /// See `protocol::TerminalAppearance`.
    pub terminal_appearance: Option<TerminalAppearance>,
}

/// A session that is recorded and published as `starting`, whose agent process is still to be
/// launched. The launch claim is held from here until [`finish_open`] has settled it, so nothing
/// else launches, switches or deletes the session meanwhile.
struct StartingSession {
    claim: crate::state::LaunchClaim,
    session: Session,
    cwd: PathBuf,
    task: Option<String>,
    /// The outside conversation the session continues, if it was opened with one.
    resume_agent_session_id: Option<String>,
    /// The login-shell snapshot that conversation's record was found with, which the launch must
    /// use too (see [`term::LaunchRequest::preresolved_shell_env`]).
    resume_shell_env: Option<HashMap<String, String>>,
    /// See [`OpenRequest::terminal_appearance`].
    terminal_appearance: Option<TerminalAppearance>,
}

/// Opens a session and returns it once it has launched: running, or with the launch's refusal
/// as the error and nothing left behind. For a caller that has nothing to show before the agent
/// is up — an agent's `new_session` tool, or the approval of a session request — which
/// would otherwise be told of a session that may yet vanish. The user's own request is
/// [`open_session_detached`].
pub async fn open_session(state: &Arc<AppState>, request: OpenRequest) -> Result<Session> {
    open_session_with(state, request, term::launch).await
}

/// [`open_session`], launching through `launch`: `term::launch` outside the tests, which cannot
/// start an agent.
pub(crate) async fn open_session_with(
    state: &Arc<AppState>,
    request: OpenRequest,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<Session> {
    let starting = begin_open(state, request).await?;
    finish_open(state, starting, launch)
        .await
        .map_err(OpenError::into_error)
}

/// Opens a session and returns it as soon as it is recorded and published as `starting`, so a
/// client can show it at once: launching snapshots the user's shell environment and forks the
/// agent, which can take seconds on a busy machine. The launch is the returned [`Followup`]'s, to
/// run once the session has been handed to the client; the session's `session_upserted` then says
/// it is up, and one that could not be launched is deleted again and reported with a
/// `launch_failed` `session_notice`.
pub async fn open_session_detached(
    state: &Arc<AppState>,
    request: OpenRequest,
) -> Result<(Session, Followup)> {
    open_detached_with(state, request, term::launch).await
}

/// [`open_session_detached`], launching through `launch`: `term::launch` outside the tests, which
/// cannot start an agent.
async fn open_detached_with(
    state: &Arc<AppState>,
    request: OpenRequest,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<(Session, Followup)> {
    let starting = begin_open(state, request).await?;
    let opened = starting.session.clone();
    let state = state.clone();
    let runtime = tokio::runtime::Handle::current();
    let followup = Followup(Some(Box::new(move || {
        runtime.spawn(async move {
            let session = starting.session.clone();
            match finish_open(&state, starting, launch).await {
                Ok(_) => {}
                Err(OpenError::Launch(err)) => report_failed_launch(&state, &session, &err),
                // The process is up and the session stays; only settling it after the launch failed.
                Err(OpenError::Settling(err)) => {
                    tracing::warn!(session = %session.id, %err, "settling a launched session failed");
                }
            }
        });
    })));
    Ok((opened, followup))
}

/// Tells every client that the launch of `session` failed with `err`. The session is normally
/// already deleted; if it could not be deleted it is left interrupted, and the notice is sent all
/// the same. The notice carries the failure's own code as `reason_code` and its params beside
/// `title` and `reason` (the English account), so a client words the reason itself.
pub(crate) fn report_failed_launch(state: &Arc<AppState>, session: &Session, err: &anyhow::Error) {
    let (code, coded, reason) = error_parts(err);
    let mut pairs: Vec<(&str, &str)> = coded
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    // After the failure's own, which a later pair of the same name replaces.
    pairs.extend([
        ("title", session.title.as_str()),
        ("reason", reason.as_str()),
        ("reason_code", code.as_str()),
    ]);
    state.broadcast(
        Notice::new(
            notice_code::LAUNCH_FAILED,
            format!("could not start {}: {reason}", session.title),
            &pairs,
        )
        .about(&session.id),
    );
}

/// Why [`finish_open`] did not leave a running session.
enum OpenError {
    /// The agent could not be launched, and the session is deleted again.
    Launch(anyhow::Error),
    /// The agent is up, but recording that failed.
    Settling(anyhow::Error),
}

impl OpenError {
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Launch(err) | Self::Settling(err) => err,
        }
    }
}

/// What opening a session settles from its request before anything is awaited, launched or
/// recorded, and refuses for what cannot be opened.
pub(super) struct ResolvedOpen {
    role: Role,
    cwd: PathBuf,
    project: Option<Project>,
    console: Console,
    default_title: String,
    resume_agent_session_id: Option<String>,
    bound_to: Option<String>,
    pub(super) agent: Agent,
    pub(super) account_id: Option<String>,
    config_dir: Option<String>,
}

/// Resolves what [`open_session`] opens from `request`, refusing an unknown console, project or
/// owner, an agent that is not available or not added, an account of another agent, a disabled
/// account, and an account whose plan quota is used up.
pub(super) fn resolve_open(state: &Arc<AppState>, request: &OpenRequest) -> Result<ResolvedOpen> {
    resolve_open_with(state, request, true)
}

/// [`resolve_open`], refusing an account whose quota is used up only when `enforce_quota`.
pub(super) fn resolve_open_with(
    state: &Arc<AppState>,
    request: &OpenRequest,
    enforce_quota: bool,
) -> Result<ResolvedOpen> {
    let console = state.store.require_console(&request.console_id)?;

    let (role, cwd, project, default_title) = match &request.project_id {
        Some(project_id) => {
            let project = state.store.require_project(project_id)?;
            let cwd = PathBuf::from(&project.path);
            let title = project.name.clone();
            (Role::Project, cwd, Some(project), title)
        }
        None => {
            let workdir = PathBuf::from(&console.workdir);
            // The store reads the current console name when inserting the session.
            (Role::Console, workdir, None, String::new())
        }
    };

    // Before `bound_to` is resolved, which would drop a console session's request for one.
    let resume_agent_session_id = checked_resume_id(
        state,
        role,
        request.project_id.as_deref(),
        request.bound_to.as_deref(),
        request.resume_agent_session_id.clone(),
    )?;
    let bound_to = resolve_bound_to(
        state,
        &request.console_id,
        request.project_id.as_deref(),
        role,
        request.bound_to.clone(),
    )?;

    let preferred = session_agent(request.agent, project.as_ref(), &console);
    let automatic = request.account.is_none()
        && request.resume_agent_session_id.is_none()
        && state.store.auto_select_account()?;
    let picked = if automatic {
        automatic_open_choice(
            state,
            preferred,
            &console,
            project.as_ref(),
            None,
            request.agent.is_none(),
        )?
    } else {
        None
    };
    let agent = picked.as_ref().map_or(preferred, |(agent, _)| *agent);
    refuse_if_agent_unavailable(state, agent)?;
    // After the availability check, as in the UI, where "not installed" wins over "not added".
    refuse_unless_agent_added(state, agent)?;

    let (account_id, config_dir) = session_account(
        state,
        agent,
        &console,
        project.as_ref(),
        picked
            .map(|(_, account)| account)
            .or_else(|| request.account.clone()),
    )?;
    refuse_if_account_disabled(state, agent, account_id.as_deref())?;
    if enforce_quota {
        refuse_if_account_quota_exhausted(
            state,
            agent,
            account_id.as_deref(),
            config_dir.as_deref(),
        )?;
        refuse_if_account_not_signed_in(
            state,
            agent,
            account_id.as_deref(),
            config_dir.as_deref(),
        )?;
    }
    // Asked again by the launch under its claim; asked here so that nothing is recorded.
    crate::accounts::shared_config::refuse_if_changing(state, account_id.as_deref())?;
    Ok(ResolvedOpen {
        role,
        cwd,
        project,
        console,
        default_title,
        resume_agent_session_id,
        bound_to,
        agent,
        account_id,
        config_dir,
    })
}

/// Everything of opening a session that precedes launching its agent: the refusals, the record
/// (`starting`) and its publication.
async fn begin_open(state: &Arc<AppState>, request: OpenRequest) -> Result<StartingSession> {
    let ResolvedOpen {
        role,
        cwd,
        project,
        console,
        default_title,
        resume_agent_session_id,
        bound_to,
        agent,
        account_id,
        config_dir,
    } = resolve_open(state, &request)?;
    let OpenRequest {
        console_id,
        project_id,
        model,
        effort,
        task,
        title,
        origin,
        session_id,
        terminal_appearance,
        ..
    } = request;
    let resume_shell_env = match resume_agent_session_id.clone() {
        Some(id) => {
            let dir = config_dir.clone();
            tokio::task::spawn_blocking(move || {
                locate_conversation_to_resume(
                    agent,
                    dir.as_deref(),
                    &id,
                    env_shell::launch_snapshot,
                )
            })
            .await??
        }
        None => None,
    };
    let (model, effort) = check_launch_settings(
        state,
        agent,
        account_id.clone(),
        project.as_ref(),
        &console,
        model,
        effort,
        SettingsChecked::All,
    )
    .await?;
    let title = title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty());

    let session = Session {
        id: session_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        agent,
        // Recorded with `has_conversation`, so an interrupt before the first prompt does not drop
        // the id on a relaunch (see [`resumable_agent_session_id`]).
        agent_session_id: resume_agent_session_id.clone(),
        console_id,
        project_id,
        host_id: LOCAL_HOST_ID.to_string(),
        role,
        origin,
        title: title.clone().unwrap_or_else(|| default_title.clone()),
        // Until the process is up; `finish_open` moves it to what the launch implies.
        status: SessionStatus::Starting,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        default_effort: None,
        context: None,
        observed_model: None,
        observed_effort: None,
        has_conversation: resume_agent_session_id.is_some(),
        // Resolved now and kept: a resume must find the transcript where the first launch put it,
        // whatever the console's reference says by then.
        account_id,
        config_dir,
        model,
        effort,
        // A session is opened bound or unbound; only binding it afterwards makes a coordinator
        // of one under a console session.
        orchestration_role: if bound_to.is_some() {
            OrchestrationRole::Worker
        } else {
            OrchestrationRole::Coordinator
        },
        bound_to,
        // Assigned by `insert_console_session` below for a console session; a project session
        // carries no colour or ordinal of its own.
        colour: None,
        ordinal: None,
        pinned: false,
        locked: false,
        started_at: now_millis(),
        ended_at: None,
    };
    let session = if role == Role::Console {
        state.store.insert_console_session(session, title)?
    } else {
        // `bound_to` was resolved before the launch check above awaited, so the owner may since
        // have been deleted, or have bound itself to a project session or been handed to one, and
        // so can no longer own sessions: a binding that lands in between must not leave a third
        // level. The store checks the owner in the insert itself, and once this row exists the
        // owner can neither be deleted nor change either way. Nothing has been launched yet.
        match &session.bound_to {
            Some(owner) => {
                if !state.store.insert_session_under_owner(&session, owner)? {
                    return Err(match state.store.get_session(owner)? {
                        None => CodedError::unknown_session(owner),
                        Some(_) => anyhow::anyhow!(
                            "`{owner}` can no longer own sessions: it was bound to a project \
                             session while this one was being opened"
                        ),
                    });
                }
            }
            None => state.store.insert_session(&session)?,
        }
        // The duplicate check ran before the awaits above, so another request may have recorded
        // the same conversation since. Nothing has been launched yet.
        if let Some(id) = &resume_agent_session_id {
            if let Err(err) = refuse_if_conversation_recorded(
                state,
                id,
                session.project_id.as_deref(),
                Some(&session.id),
            ) {
                state.store.delete_session(&session.id)?;
                return Err(err);
            }
        }
        session
    };
    let claim = state.begin_launch(&session.id)?;
    state.publish_session(&session);
    Ok(StartingSession {
        claim,
        session,
        cwd,
        task,
        resume_agent_session_id,
        resume_shell_env,
        terminal_appearance,
    })
}

/// Launches the agent of a session [`begin_open`] recorded, through `launch`, and publishes it
/// running. Without a process the session is deleted again: nothing ran, so there is nothing to
/// resume, and a session in the tree that never existed would only mislead.
async fn finish_open(
    state: &Arc<AppState>,
    starting: StartingSession,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<Session, OpenError> {
    let StartingSession {
        claim,
        session,
        cwd,
        task,
        resume_agent_session_id,
        resume_shell_env,
        terminal_appearance,
    } = starting;
    // Read inside the launch's own result: a failure here is one before anything ran, which is
    // the same as a launch that failed, and must delete the session the same way.
    let started = async {
        let recorded = state.session_record(&session.id)?;
        // Archived in the moment before the launch began: the user has already said they do not
        // want it, and there is nothing to end.
        if recorded.status != SessionStatus::Starting {
            return Ok(Some(recorded));
        }
        start_process(
            state,
            &claim,
            &session,
            &cwd,
            task.as_deref(),
            resume_agent_session_id.as_deref(),
            terminal_appearance,
            // The snapshot the conversation to resume was found with is the one the launch must use.
            move |request| {
                launch(term::LaunchRequest {
                    preresolved_shell_env: resume_shell_env,
                    ..request
                })
            },
        )
        .await
        .map(|()| None)
    }
    .await;
    match started {
        Ok(Some(archived)) => Ok(archived),
        Ok(None) => settle_started(state, &session, task.is_some()).map_err(OpenError::Settling),
        Err(err) => {
            state.revoke_mcp_tokens(&session.id);
            state.discard_outbox(&session.id);
            // The claim is held through the deletion, so nothing acts on the session half-gone.
            if let Err(discard_err) = state.discard_session(&session.id) {
                tracing::warn!(session = %session.id, err = %discard_err, "deleting a session whose launch failed failed");
                // The row stays, so it must not stay `starting`: nothing would ever move it, and
                // `starting` sessions refuse deletion and resume.
                mark_interrupted(state, &session.id);
            }
            drop(claim);
            Err(OpenError::Launch(err))
        }
    }
}

/// Moves a session that has no process out of `starting`, to the status a click resumes from.
fn mark_interrupted(state: &Arc<AppState>, id: &str) {
    if let Err(err) = state.interrupt_session_record(id) {
        tracing::warn!(session = %id, %err, "marking a session that failed to launch interrupted failed");
    }
}

/// Moves a session whose process is up from `starting` to what the launch implies, and returns its
/// record.
fn settle_started(state: &Arc<AppState>, session: &Session, has_task: bool) -> Result<Session> {
    // A session opened without a task is sitting at its prompt, not working. This also matters for
    // Codex specifically: its `SessionStart` hook does not fire until the first prompt submission,
    // so an optimistic `working` here would stay until the user typed. Conditional on it still
    // starting: the agent's own first hook may have moved it on already, and an archive that landed
    // while the process was being forked stands.
    let up = if has_task {
        SessionStatus::Working
    } else {
        SessionStatus::Idle
    };
    if !state.apply_reported_status_if(&session.id, SessionStatus::Starting, up)? {
        let record = state.session_record(&session.id)?;
        if record.status == SessionStatus::Archived {
            // Archived while the process was being forked, when there was none to end.
            if let Some(live) = state.live_session(&session.id) {
                stop_process(state, live);
            }
        }
        return Ok(record);
    }
    // Re-read rather than return the record we built: launching may have learned the agent's own
    // session id, and the stale copy has none.
    state.session_record(&session.id)
}

/// Refuses a session that [`open_session`] would refuse for what `request` asks, a console session
/// included, before anything is launched or recorded: an unknown console, project or owner, an
/// agent that is not available or not added, an account of another agent, a disabled account, an
/// account whose plan quota is used up or that the last quota reading found not logged in, a model
/// or effort the agent does not offer. For a session whose opening waits on the user's answer, so
/// that a request which could never be carried out is refused to the asker rather than put to the
/// user. Returns the agent the session would be of.
pub async fn check_open(state: &Arc<AppState>, request: &OpenRequest) -> Result<Agent> {
    Ok(check_open_with(state, request, true).await?.0)
}

/// [`check_open`] for a session whose opening waits on the user's choice of account, which is not
/// refused for an account whose quota is used up or that is not logged in (the user can pick
/// another; the refusal comes at approval). Returns the account the session would open on.
pub async fn check_open_asking(
    state: &Arc<AppState>,
    request: &OpenRequest,
) -> Result<Option<String>> {
    Ok(check_open_with(state, request, false).await?.1)
}

/// The checks of [`check_open`], refusing an account whose quota is used up or that is not logged
/// in only when `enforce_quota`; returns the agent and the account the session would open on.
async fn check_open_with(
    state: &Arc<AppState>,
    request: &OpenRequest,
    enforce_quota: bool,
) -> Result<(Agent, Option<String>)> {
    let ResolvedOpen {
        project,
        console,
        agent,
        account_id,
        ..
    } = resolve_open_with(state, request, enforce_quota)?;
    check_launch_settings(
        state,
        agent,
        account_id.clone(),
        project.as_ref(),
        &console,
        request.model.clone(),
        request.effort.clone(),
        SettingsChecked::All,
    )
    .await?;
    Ok((agent, account_id))
}

#[cfg(test)]
mod tests;
