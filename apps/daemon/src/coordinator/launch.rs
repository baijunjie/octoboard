//! Starting the agent process of a recorded session, a relaunch included, and stopping it, the
//! waits for it to go down, the launch settings a session starts with, and the checks of the
//! model and effort a session is opened, switched, reopened or changed to.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::agents::launch_settings::{normalize_session_launch_setting, LaunchCheck};
use crate::git_workflow::GitWorkflowSettings;
use crate::mcp;
use crate::messaging::delivery;
use crate::messaging::owner_notices;
use crate::protocol::{
    error_code, Agent, CodedError, Console, Launch, Project, Role, Session, SessionStatus,
    TerminalAppearance,
};
use crate::session::dialogs;
use crate::session::term;
use crate::session::trust;
use crate::session::LiveSession;
use crate::state::{session_archived, AppState};

use super::bindings::owner_of;
use super::resume::resumable_agent_session_id;
use super::settings::{CLAUDE_MODE, CODEX_REVIEWER};

/// The relaunch a resume and a switch of the session's account both end in: everything Octoboard
/// injects is reassembled and every launch refusal applies, whichever of the two asked. `claim` is
/// the caller's, taken before it read anything and dropped by it when it is done; `reopen` says
/// whether an archived session may be relaunched (a resume reopens one, a switch must not);
/// `shell_env` is a snapshot the caller already resolved something from (see
/// [`term::LaunchRequest::preresolved_shell_env`]). `launch` is `term::launch` outside the tests,
/// which cannot start an agent.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn relaunch_session_with(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    id: &str,
    instruction: Option<&str>,
    reopen: bool,
    shell_env: Option<HashMap<String, String>>,
    terminal_appearance: Option<TerminalAppearance>,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<()> {
    let mut session = state.session_record(id)?;
    if !session.status.has_no_process() {
        // The ordinary cause is a second click landing after the first relaunch already moved the
        // session, so it carries the same code as a launch that is already under way.
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_RUNNING,
            "this session is not interrupted or archived",
            &[("session", id)],
        ));
    }

    if !reopen && session.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }

    // A change of owner the session was not running to be told of is told now, ahead of the
    // instruction, from the binding as it stands: Claude Code and Grok replay what they were first
    // told of their owner on every resume. Read and built with the refusals, before anything is
    // written, and cleared only for the count seen here, so a change marked since stays owed.
    let owed = state.store.owner_notice_pending(id)?;
    // What was kept for the session while it was interrupted, read with the refusals as well,
    // less the waits for the user that are over by now.
    let held = delivery::current_kept_messages(state, id)?;
    let combined = if owed > 0 {
        let notice = owner_notices::owner_notice(state, &session)?;
        Some(match instruction {
            Some(instruction) => format!("{notice}\n\n{instruction}"),
            None => notice,
        })
    } else {
        instruction.map(str::to_string)
    };
    let instruction = combined.as_deref();

    let previous_status = session.status;
    let previous_ended_at = session.ended_at;
    let cwd = session_cwd(state, &session)?;
    let resume_id = resumable_agent_session_id(&session);
    // A relaunch carries no task of its own, so the agent comes up at its prompt rather than
    // working (a Claude Code instruction given as the launch prompt is reported by its hook). On
    // Codex that is also the only correct value available: its `SessionStart` hook does not fire
    // until the first prompt submission, so an optimistic `working` would never be corrected.
    session.status = SessionStatus::Idle;
    session.ended_at = None;
    // Marked only here, once the refusals are past and whether the agent is handed a prompt is
    // known; the session is announced again with the launch's outcome below.
    if instruction.is_some() {
        state.expect_prompt_on_launch(id);
        state.publish_session_by_id(id);
    }
    if state
        .store
        .update_session_lifecycle_if(id, previous_status, SessionStatus::Idle, None)?
        .is_none()
    {
        return Err(session_archived(id));
    }

    // Claude Code asks "Resume this conversation?" at a launch that resumes with no prompt on the
    // command line, and a message pasted into that dialog is discarded while its Enter picks a
    // row. With the instruction as the launch prompt the dialog is not shown, so it goes there;
    // every other launch has it queued instead.
    let (prompt, queued) = match instruction {
        Some(instruction) if session.agent == Agent::Claude && resume_id.is_some() => {
            (term::launch_prompt(instruction), None)
        }
        queued => (None, queued),
    };
    // The information is queued in the order it arrived. For a Claude Code resume the instruction
    // goes as the launch prompt, so the information is written after it; for the other agents it
    // is queued behind the information. Each row stays in the store until its message has been
    // written into the session (`AppState::flush_outbox`): a launch that fails, a process that
    // ends or a daemon that exits before then discards the queue and leaves the row for the next
    // resume.
    for (row, text) in &held {
        state.queue_kept_message(&session.id, text, *row);
    }
    // Queued with every refusal above already past, so a call that never launched leaves nothing
    // behind for a later launch to deliver.
    if let Some(instruction) = queued {
        state.queue_message(&session.id, instruction);
    }

    match start_process(
        state,
        claim,
        &session,
        &cwd,
        prompt.as_deref(),
        resume_id.as_deref(),
        terminal_appearance,
        // The snapshot the caller resolved something from is the one the launch must use.
        move |request| {
            launch(term::LaunchRequest {
                preresolved_shell_env: shell_env,
                ..request
            })
        },
    )
    .await
    {
        Ok(()) => {
            // What was queued for the relaunched session — the instruction above, say — is
            // released by its first hook (for Claude Code, once bracketed paste is on as well);
            // for Codex, which runs none until its first prompt, also by Octoboard's successful
            // press of its trust screen, or once its trust watch has ended with none sighted
            // (`trust::supervise`). Never before, since the agent may be sitting on its trust or
            // approval screen, where the paste's trailing Enter would answer it.
            let recorded = match state.session_record(&session.id) {
                Ok(recorded) => recorded,
                Err(err) => {
                    // The caller will not watch the process come up.
                    state.release_launch_watch(&session.id);
                    return Err(err);
                }
            };
            if recorded.status == SessionStatus::Archived {
                // The archive may have landed while the launch was off the runtime, before
                // there was a registered process for it to end.
                if let Some(live) = state.live_session(&session.id) {
                    stop_process(state, live);
                }
                state.revoke_mcp_tokens(&session.id);
                state.discard_outbox(&session.id);
                return Err(session_archived(&session.id));
            }
            // The notice went out with the launch: a failure to clear it only means it is told
            // once more at the next one.
            if owed > 0 {
                if let Err(err) = state.store.clear_owner_notice(&session.id, owed) {
                    tracing::warn!(session = %session.id, %err, "clearing a delivered owner notice failed");
                }
            }
            state.publish_session(&recorded);
            Ok(())
        }
        Err(err) => {
            // Back to where it was, archived included: a reopen that failed to launch must not
            // quietly move a session out of its project's Archive group. The recorded end time is
            // written back rather than left alone, since the relaunch above cleared it and
            // persisted that before launching; the archive is ordered and dated by it (see
            // "The archive view" in `docs/product/archive-view.md`), so a session this attempt did not
            // change would otherwise jump to the top with an incorrect archive date.
            // Restore only this attempt's optimistic status: a newer archive, including its
            // own end time, must survive a launch failure.
            if let Some(restored) = state.store.update_session_lifecycle_if(
                id,
                SessionStatus::Idle,
                previous_status,
                previous_ended_at,
            )? {
                state.publish_session(&restored);
            }
            state.revoke_mcp_tokens(&session.id);
            // Nothing will ever release what was queued for this relaunch, and leaving it would
            // deliver a stale instruction to whatever launch comes next.
            state.discard_outbox(&session.id);
            Err(err)
        }
    }
}

/// The model and reasoning effort a session of `project` launches `agent` with when it chose
/// neither: for each value independently, the project's own, else its console's project session
/// value, else `None`, the agent's own default. Each agent has its own pair, since each has its own
/// model names.
pub(crate) fn project_launch_settings(
    project: &Project,
    console: &Console,
    agent: Agent,
) -> Launch {
    let own = project.launch.get(agent);
    let console_launch = console.project_session_launch.get(agent);
    Launch {
        model: own.model.clone().or_else(|| console_launch.model.clone()),
        effort: own.effort.clone().or_else(|| console_launch.effort.clone()),
    }
}

/// The model and reasoning effort `session` is launched with, for its own agent: a console session
/// reads its console's console session pair, a project session its project's with its console's
/// project session pair behind it (see [`project_launch_settings`]), unless that field has a
/// session override. Inherited values are read at every launch, while an override (including
/// explicitly leaving the choice to the agent) survives resumes and account switches.
pub(crate) fn launch_settings(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<(Option<String>, Option<String>)> {
    let inherited = match &session.project_id {
        Some(project_id) => {
            let project = state.store.require_project(project_id)?;
            let console = state.store.require_console(&session.console_id)?;
            project_launch_settings(&project, &console, session.agent)
        }
        None => {
            let console = state.store.require_console(&session.console_id)?;
            // A console session's own pair: each agent has its own model names.
            console.launch.get(session.agent).clone()
        }
    };
    Ok((
        session.model.clone().unwrap_or(inherited.model),
        session.effort.clone().unwrap_or(inherited.effort),
    ))
}

/// Which of the values given to [`check_launch_settings`] are held against what the agent offers.
#[derive(Clone, Copy)]
pub(super) enum SettingsChecked<'a> {
    /// Every value, as for a session being opened or moved to another account.
    All,
    /// Only the values that differ from the session's own overrides `model` and `effort`: a value
    /// it already runs with is not refused because the agent has stopped offering it since.
    Changed {
        model: &'a Option<Option<String>>,
        effort: &'a Option<Option<String>>,
    },
}

/// Normalises a session's model and effort override and refuses one `agent` does not offer under
/// `account_id`, as the launch of a session does, among the values `checked` names.
#[allow(clippy::too_many_arguments)]
pub(super) async fn check_launch_settings(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<String>,
    project: Option<&Project>,
    console: &Console,
    model: Option<Option<String>>,
    effort: Option<Option<String>>,
    checked: SettingsChecked<'_>,
) -> Result<(Option<Option<String>>, Option<Option<String>>)> {
    let (model, effort) = (
        normalize_session_launch_setting(model),
        normalize_session_launch_setting(effort),
    );
    // A session that chose no model takes the project's, else the console's project session one,
    // or for a console session the console's console session one.
    let effective_model = match &model {
        Some(model) => model.clone(),
        None => match project {
            Some(project) => project_launch_settings(project, console, agent).model,
            None => console.launch.get(agent).model.clone(),
        },
    };
    let given = |value: &Option<Option<String>>, had: Option<&Option<Option<String>>>| {
        value.clone().filter(|_| had != Some(value)).flatten()
    };
    let (had_model, had_effort) = match checked {
        SettingsChecked::All => (None, None),
        SettingsChecked::Changed { model, effort } => (Some(model), Some(effort)),
    };
    LaunchCheck {
        agent,
        account_id,
        model: given(&model, had_model),
        effort: given(&effort, had_effort),
        effective_model,
    }
    .run(state)
    .await?;
    Ok((model, effort))
}

/// The project a session belongs to, if any, and its console, which its launch settings fall
/// back on.
pub(super) fn session_scope(
    state: &AppState,
    session: &Session,
) -> Result<(Option<Project>, Console)> {
    let project = match &session.project_id {
        Some(project_id) => Some(state.store.require_project(project_id)?),
        None => None,
    };
    let console = state.store.require_console(&session.console_id)?;
    Ok((project, console))
}

/// The model and reasoning effort a user changed for a switch or a reopen, each absent to leave the
/// session's own override as it is, `Some(None)` for the agent's own default.
#[derive(Default)]
pub(crate) struct LaunchEdit {
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
}

/// The model and effort overrides `session` would have with `edit` applied, each value left
/// unchanged keeping what the session has, refused for a value the agent does not offer under
/// `account_id`; `None` when there is nothing to change.
pub(super) async fn checked_launch_overrides(
    state: &Arc<AppState>,
    session: &Session,
    account_id: Option<String>,
    edit: LaunchEdit,
) -> Result<Option<(Option<Option<String>>, Option<Option<String>>)>> {
    if edit.model.is_none() && edit.effort.is_none() {
        return Ok(None);
    }
    let (project, console) = session_scope(state, session)?;
    let checked = check_launch_settings(
        state,
        session.agent,
        account_id,
        project.as_ref(),
        &console,
        edit.model.or_else(|| session.model.clone()),
        edit.effort.or_else(|| session.effort.clone()),
        SettingsChecked::All,
    )
    .await?;
    Ok(Some(checked))
}

/// The git workflow settings in force for a project session's project as this launch is assembled,
/// which shape the rule its role text carries; none for a console session, which is given no rule.
fn launch_git_workflow_settings(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<Option<GitWorkflowSettings>> {
    let Some(project_id) = session
        .project_id
        .as_ref()
        .filter(|_| session.role == Role::Project)
    else {
        return Ok(None);
    };
    let project = state.store.require_project(project_id)?;
    Ok(Some(crate::git_workflow::effective(
        &state.store,
        &project,
    )?))
}

/// Launches the agent for an already-stored session, through `launch` (`term::launch` outside the
/// tests, which cannot start an agent), and starts watching it.
#[allow(clippy::too_many_arguments)]
pub(super) async fn start_process(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    session: &Session,
    cwd: &Path,
    task: Option<&str>,
    resume_agent_session_id: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<()> {
    // Refresh the instruction file on every console launch, resume and account switch. A missing
    // directory is left to the launch, whose refusal names it, rather than failing here on the write.
    if session.role == Role::Console && cwd.is_dir() {
        mcp::console_instructions::refresh_console_session_instructions(
            &state.store,
            &session.console_id,
        )?;
    }
    let (model, effort) = launch_settings(state, session)?;
    // A sharing account's links are repaired before its agent starts, and the agent is told
    // where the shared directory is.
    let sharing = crate::accounts::shared_config::before_launch(state, session).await?;
    for notice in sharing.notices {
        state.broadcast(notice.about(&session.id));
    }
    // Reserved until the process is registered below, and given up if it never is.
    let pending_launch = state.process_ledger.begin_launch(&session.id);
    let request = term::LaunchRequest {
        session_id: session.id.clone(),
        agent: session.agent,
        role: session.role,
        owner: owner_of(state, session)?,
        git_workflow: launch_git_workflow_settings(state, session)?,
        ask_questions_as_choices: state.store.ask_questions_as_choices()?,
        user_language: state.store.interface_language()?,
        cwd: cwd.to_path_buf(),
        task: task.map(str::to_string),
        resume_agent_session_id: resume_agent_session_id.map(str::to_string),
        config_dir: session.config_dir.as_ref().map(PathBuf::from),
        approval_reviewer: CODEX_REVIEWER.for_session(state, session)?,
        approval_mode: CLAUDE_MODE.for_session(state, session)?,
        claude_compatibility: state.store.claude_compatibility_of(session.agent)?,
        shared_default_dir: sharing.shared_default_dir,
        model,
        effort,
        terminal_appearance,
        preresolved_shell_env: None,
        daemon_port: state.port,
        daemon_key: state.key.clone(),
        self_exe: state.self_exe.clone(),
        // Issued per launch: the previous process is gone, and a token outliving it would let a
        // stale child act on this session.
        mcp_token: state.issue_mcp_token(&session.id),
        process_tag: pending_launch.tag().clone(),
    };

    // Launching snapshots the user's shell environment and forks a process, so it goes off the
    // runtime rather than holding up the socket it was asked on.
    let launch = tokio::task::spawn_blocking(move || launch(request)).await??;
    // Octoboard's own account switch, or the restoring of a session after one, watches the
    // process come up and reports the outcome itself.
    if claim.watches_launch() {
        launch.session.watch_under_claim();
    }

    // The live map now says the session is running; `claim` stays with the caller, which drops it
    // when it is done with the launch.
    state.register_live(launch.session.clone());
    // The output of an earlier run is stale from here on: were this process to end without saving
    // (a daemon killed outright), it must not stand in for what this one printed. It goes after
    // `register_live`, so a client attaching meanwhile finds either the process or the saved
    // output, never neither (and a launch that fails leaves it to show). It also goes before
    // `watch_exit`, so the exit save of this process is never the one removed; `stop_all_sessions`
    // saves too, but only after `terminate` has returned, which waits on the process, long past
    // this point.
    state.forget_saved_output(&session.id);
    super::exit_watch::watch_exit(state, launch.session.clone());
    trust::supervise(state, &launch.session);
    dialogs::watch(state, &launch.session);
    if let Some(agent_session_id) = launch.agent_session_id {
        if let Err(err) = state.set_agent_session_id(&session.id, &agent_session_id) {
            // Registered already, and the caller will not watch it come up.
            state.release_launch_watch(&session.id);
            return Err(err);
        }
    }
    if let Some(notice) = launch.notice {
        state.broadcast(notice.about(&session.id));
    }
    Ok(())
}

/// Ends one session's process, waiting out the graceful period the agents' shutdown hooks depend
/// on, on a blocking thread. This is the whole of "ending a process" and nothing more: it decides
/// no status, archives nothing and reaches no other session. What archiving adds around it — the
/// status, and whatever later follows an archived session to the sessions bound to it — is
/// `archive_session`'s, which is why a switch of the session's account stops the process through
/// this and not through archiving. Answers whether it found the process running and stopped it
/// (see [`LiveSession::end`]), which the exit watcher reads to know the end was asked for.
///
/// The process ledger reads the table first, so that what the agent started a moment ago is
/// attributed to its launch while its chain through the agent still shows.
pub(crate) fn stop_process(
    state: &Arc<AppState>,
    live: Arc<LiveSession>,
) -> tokio::task::JoinHandle<bool> {
    let ledger = Arc::clone(&state.process_ledger);
    tokio::task::spawn_blocking(move || {
        ledger.scan_before_ending();
        live.end()
    })
}

/// How long an operation that restarts a session waits for a process to be seen gone and recorded
/// down, which the exit watcher does on its own schedule: an account switch, a model change and a
/// change of an account's sharing wait this long for a process they ended, and the switch and the
/// model change also for a relaunched one that did not come up. Far above that schedule plus the
/// graceful period: reaching it means the process would not die, and an operation that ended it
/// does not go on: it answers `session_did_not_stop`, or its own code (the model change's
/// `model_change_did_not_stop`, the sharing change's `sharing_session_did_not_stop`). For a
/// relaunched process the operation is failing already, so reaching it is only logged.
pub(crate) const SWITCH_STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// Waits for the exit watcher to have seen the session's process gone and recorded the session as
/// without a process — the state a relaunch starts from, and the one thing that must be true
/// before the watcher can no longer write over what the relaunch records.
pub(crate) async fn wait_until_down(state: &Arc<AppState>, id: &str) -> Result<()> {
    wait_until_down_within(state, id, SWITCH_STOP_TIMEOUT).await
}

/// [`wait_until_down`], giving up after `timeout`.
pub(crate) async fn wait_until_down_within(
    state: &Arc<AppState>,
    id: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while state.live_session(id).is_some() || !state.session_record(id)?.status.has_no_process() {
        if Instant::now() >= deadline {
            // The process may still be running, with nothing recorded and nothing copied.
            return Err(CodedError::raised(
                error_code::SESSION_DID_NOT_STOP,
                "the session's process did not end in time, so it was not switched",
                &[("session", id)],
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

/// Whether the relaunched process is still running after `settle`.
pub(super) async fn came_up(live: &LiveSession, settle: Duration) -> bool {
    let deadline = Instant::now() + settle;
    while Instant::now() < deadline {
        if live.poll_exit() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    !live.poll_exit()
}

fn session_cwd(state: &Arc<AppState>, session: &Session) -> Result<PathBuf> {
    match &session.project_id {
        Some(project_id) => {
            let project = state.store.require_project(project_id)?;
            Ok(PathBuf::from(project.path))
        }
        None => {
            let console = state.store.require_console(&session.console_id)?;
            Ok(PathBuf::from(console.workdir))
        }
    }
}

#[cfg(test)]
mod tests;
