//! Starting the agent process of a recorded session, a relaunch included, and stopping it, the
//! waits for it to go down, the launch settings a session starts with, and the checks of the
//! launch values a session is opened, switched, resumed or changed to.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::agents::launch_settings::{
    self, apply_compatibility, check_compatibility, normalize_session_launch_setting, LaunchCheck,
};
use crate::messaging::delivery;
use crate::messaging::owner_notices;
use crate::processes::ledger::PendingLaunch;
use crate::protocol::{
    error_code, AgentId, CodedError, Console, GitWorkflowSettings, Launch, ProcessTree, Project,
    Session, SessionKind, SessionStatus, TerminalAppearance,
};
use crate::providers::ProviderSettings;
use crate::session::dialogs;
use crate::session::term;
use crate::session::trust;
use crate::session::LiveSession;
use crate::state::{session_archived, AppState};

use super::bindings::owner_of;
use super::resume::resumable_agent_session_id;

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

    // A provider that declares `resume_instruction_as_launch_prompt` shows a resume dialog at a
    // launch that resumes with no prompt on the command line — Claude Code asks "Resume this
    // conversation?" — and a message pasted into that dialog is discarded while its Enter picks a
    // row. With the instruction as the launch prompt the dialog is not shown, so it goes there;
    // every other launch has it queued instead.
    let (prompt, queued) = match instruction {
        Some(instruction)
            if resume_id.is_some()
                && session
                    .agent
                    .descriptor()
                    .capabilities
                    .delivery
                    .resume_instruction_as_launch_prompt =>
        {
            (term::launch_prompt(instruction), None)
        }
        queued => (None, queued),
    };
    // The information is queued in the order it arrived. For a Claude Code resume the instruction
    // goes as the launch prompt, so the information is written after it; for the other agents it
    // is queued behind the information, except under a restart's hold, where it is put ahead of
    // the information and of what the restart carried over. Each row stays in the store until its message has been
    // written into the session (`AppState::flush_outbox`): a launch that fails, a process that
    // ends or a daemon that exits before then discards the queue and leaves the row for the next
    // resume.
    for (row, text) in &held {
        state.queue_kept_message(&session.id, text, *row);
    }
    // Queued with every refusal above already past, so a call that never launched leaves nothing
    // behind for a later launch to deliver. After a restart it goes ahead of what the restart
    // carried over.
    if let Some(instruction) = queued {
        state.queue_relaunch_instruction(&session.id, instruction);
    }

    match start_process(
        state,
        claim,
        &session,
        &cwd,
        prompt.as_deref(),
        resume_id.as_deref(),
        terminal_appearance,
        shell_env.clone(),
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
            // A hook of the relaunched agent may have tried the queue while the restart still
            // blocked it, and would not try again until its next event.
            if state.relaunched_after_restart(&session.id) {
                state.retry_pending_outbox(&session.id);
            }
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
            // deliver a stale instruction to whatever launch comes next. What a restart carried
            // over is its hold's to keep for a restore, all but this relaunch's instruction.
            if !state.relaunch_failed_after_restart(&session.id) {
                state.discard_outbox(&session.id);
            }
            Err(err)
        }
    }
}

/// A stored model as a launch reads it: a spelling the provider has retired (Claude Code's
/// `best` alias) is no model, so it reads as unset wherever it is stored.
fn readable_model(agent: AgentId, model: &Option<String>) -> Option<String> {
    let retired = agent.descriptor().model_display.retired;
    model
        .clone()
        .filter(|model| !retired.contains(&model.as_str()))
}

/// The model, reasoning effort and compatibility switches a session of `project` launches `agent`
/// with when it chose none: for each value independently, the project's own, else its console's
/// project session value, else `None`, unset (a switch then follows the agent's own value): the
/// effort is then the agent's own default, and the model
/// [`launch_settings::launch_default_model`]'s (for Claude Code, the best one the account may use
/// unless Claude Code's own configuration names a model; see [`start_process`]). Each agent has its
/// own launch settings, since each has its own models, efforts and compatibility switches.
pub(crate) fn project_launch_settings(
    project: &Project,
    console: &Console,
    agent: AgentId,
) -> Launch {
    let own = project.launch.get(agent);
    let console_launch = console.project_session_launch.get(agent);
    let mut compatibility = console_launch.compatibility.clone();
    compatibility.extend(own.compatibility.clone());
    Launch {
        model: readable_model(agent, &own.model)
            .or_else(|| readable_model(agent, &console_launch.model)),
        effort: own.effort.clone().or_else(|| console_launch.effort.clone()),
        compatibility,
    }
}

/// The model, reasoning effort and held compatibility switches a session of `agent` inherits:
/// a console session (`project` none) reads its console's console session settings, a project
/// session its project's with its console's project session settings behind them (see
/// [`project_launch_settings`]).
fn inherited_launch(project: Option<&Project>, console: &Console, agent: AgentId) -> Launch {
    match project {
        Some(project) => project_launch_settings(project, console, agent),
        None => {
            let launch = console.launch.get(agent);
            Launch {
                model: readable_model(agent, &launch.model),
                effort: launch.effort.clone(),
                compatibility: launch.compatibility.clone(),
            }
        }
    }
}

/// The model and reasoning effort `session` is launched with, for its own agent: a console session
/// reads its console's console session pair, a project session its project's with its console's
/// project session pair behind it (see [`project_launch_settings`]), unless that field has a
/// session override. Inherited values are read at every launch, while an override (including
/// explicitly leaving it unset, so that the default model applies) survives resumes and account
/// switches.
pub(crate) fn launch_settings(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<(Option<String>, Option<String>)> {
    let (project, console) = session_scope(state, session)?;
    let inherited = inherited_launch(project.as_ref(), &console, session.agent);
    Ok((
        match &session.model {
            Some(own) => readable_model(session.agent, own),
            None => inherited.model,
        },
        session.effort.clone().unwrap_or(inherited.effort),
    ))
}

/// The compatibility switches `session` is launched with that some level holds a value for: its
/// own, else its project's, else its console's set for its kind of session (see
/// [`launch_settings`]); a switch no level holds is the agent's own value, which
/// [`launch_provider_settings`] adds.
pub(crate) fn launch_compatibility(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<BTreeMap<String, bool>> {
    let (project, console) = session_scope(state, session)?;
    let mut held = inherited_launch(project.as_ref(), &console, session.agent).compatibility;
    held.extend(session.compatibility.clone());
    Ok(held)
}

/// The value each compatibility switch of `session`'s agent has when it is launched, a switch no
/// level holds taking the agent's own value: what [`launch_provider_settings`] gives the launch,
/// for a tool to report.
pub(crate) fn effective_compatibility(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<BTreeMap<String, bool>> {
    let settings = launch_provider_settings(state, session)?;
    Ok(switches_of(session.agent, &settings))
}

/// The value in `settings` of each compatibility switch `agent` declares.
pub(crate) fn switches_of(agent: AgentId, settings: &ProviderSettings) -> BTreeMap<String, bool> {
    agent
        .descriptor()
        .overridable_settings()
        .map(|setting| (setting.id.to_string(), settings.switch(setting.id)))
        .collect()
}

/// The settings of `session`'s provider as its launch reads them: the application-wide values,
/// with the compatibility switches some level of the session's chain holds taking their place.
/// The held values are combined with application-wide values by
/// [`provider_settings_with_compatibility`].
pub(crate) fn launch_provider_settings(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<ProviderSettings> {
    provider_settings_with_compatibility(
        state,
        session.agent,
        &launch_compatibility(state, session)?,
    )
}

/// Combines the provider's application-wide settings with the switches held by a launch chain.
pub(crate) fn provider_settings_with_compatibility(
    state: &AppState,
    agent: AgentId,
    held: &BTreeMap<String, bool>,
) -> Result<ProviderSettings> {
    Ok(state.store.provider_settings_of(agent)?.with_switches(held))
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
    agent: AgentId,
    account_id: Option<String>,
    project: Option<&Project>,
    console: &Console,
    model: Option<Option<String>>,
    effort: Option<Option<String>>,
    checked: SettingsChecked<'_>,
) -> Result<(Option<Option<String>>, Option<Option<String>>)> {
    // A stored `best` is no model, so it is no change when the model is edited back to unset.
    let model = normalize_session_launch_setting(model).map(|model| readable_model(agent, &model));
    let effort = normalize_session_launch_setting(effort);
    // A session that chose no model takes the project's, else the console's project session one,
    // or for a console session the console's console session one.
    let effective_model = match &model {
        Some(model) => model.clone(),
        None => inherited_launch(project, console, agent).model,
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

/// The model, reasoning effort and compatibility switches a user changed for a switch or a resume,
/// each absent to leave the session's own override as it is, `Some(None)` for unset (the default
/// model, see [`launch_settings::launch_default_model`], with Claude Code's own configuration
/// winning); a switch in `compatibility` is held with `Some(value)` and goes back to following
/// with `None`.
#[derive(Default)]
pub(crate) struct LaunchEdit {
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    pub compatibility: BTreeMap<String, Option<bool>>,
}

/// The overrides a session has once a [`LaunchEdit`] is applied to it.
pub(crate) struct LaunchOverrides {
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    pub compatibility: BTreeMap<String, bool>,
}

impl LaunchOverrides {
    /// Records the overrides as the session's own and tells every client.
    pub(crate) fn record(&self, state: &AppState, id: &str) -> Result<()> {
        state.set_session_launch(id, &self.model, &self.effort, &self.compatibility)
    }
}

/// The overrides `session` would have with `edit` applied, each value left unchanged keeping what
/// the session has, refused for a value the agent does not offer under `account_id` or a switch
/// the agent does not declare; `None` when there is nothing to change.
pub(super) async fn checked_launch_overrides(
    state: &Arc<AppState>,
    session: &Session,
    account_id: Option<String>,
    edit: LaunchEdit,
) -> Result<Option<LaunchOverrides>> {
    if edit.model.is_none() && edit.effort.is_none() && edit.compatibility.is_empty() {
        return Ok(None);
    }
    check_compatibility(session.agent, &edit.compatibility)?;
    let mut compatibility = session.compatibility.clone();
    apply_compatibility(&mut compatibility, &edit.compatibility);
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
    Ok(Some(LaunchOverrides {
        model: checked.0,
        effort: checked.1,
        compatibility,
    }))
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
        .filter(|_| session.kind == SessionKind::Project)
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
/// tests, which cannot start an agent), and starts watching it. The launch outlives this call if
/// it is dropped before the holder has answered: see [`run_launch`].
#[allow(clippy::too_many_arguments)]
pub(super) async fn start_process(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    session: &Session,
    cwd: &Path,
    task: Option<&str>,
    resume_agent_session_id: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    shell_env: Option<HashMap<String, String>>,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<()> {
    // Counted while a planned stop waits, so that it waits for the process to be registered: the
    // claim the caller holds was refused once the stop raised its handover barrier. Refused once
    // the stop has stopped waiting, as a launch claimed before the barrier and started after the
    // wait would start a terminal the stop never lists.
    let _tracked = state.track_change_unless_passed()?;
    // Stamped now, not when the terminal is built after the shell snapshot and the spawn, so a
    // display a client reports in the meantime is the newer of the two.
    let display = crate::terminal::display::DisplaySeed::new(
        state.display_fallback.clone(),
        terminal_appearance,
    );
    // Refresh the instruction file on every console launch, resume and account switch. A missing
    // directory is left to the launch, whose refusal names it, rather than failing here on the write.
    if session.kind == SessionKind::Console && cwd.is_dir() {
        super::console_workdir::refresh_console_session_instructions(
            &state.store,
            &session.console_id,
        )?;
    }
    let (model, effort) = launch_settings(state, session)?;
    // Where nothing up the chain names a model, Claude Code gets the best one the account may use
    // rather than choosing for itself, unless its own configuration names one: `shell_env` is the
    // snapshot the launch runs in (the one the caller resolved something from, else the launch's).
    let model = match model {
        Some(model) => Some(model),
        None => {
            launch_settings::launch_default_model(
                state,
                session.agent,
                session.config_dir.clone(),
                cwd,
                shell_env,
            )
            .await?
        }
    };
    // A sharing account's links are repaired before its agent starts, and the agent is told
    // where the shared directory is.
    let sharing = crate::accounts::shared_config::before_launch(state, session).await?;
    for notice in sharing.notices {
        state.broadcast(notice.about(&session.id));
    }
    // Reserved until the process is registered (see `run_launch`), and given up if it never is.
    let pending_launch = state
        .process_ledger
        .begin_launch(&session.id)
        .context("creating the launch's process containment")?;
    // The guard fails open: a list that cannot be built launches the session without it.
    let guard = match crate::cross_project_guard::for_session(state, session).await {
        Ok(guard) => guard,
        Err(err) => {
            tracing::warn!(session = %session.id, error = ?err, "cross-project guard: launching without it");
            None
        }
    };
    let request = term::LaunchRequest {
        session_id: session.id.clone(),
        agent: session.agent,
        kind: session.kind,
        owner: owner_of(state, session)?,
        git_workflow: launch_git_workflow_settings(state, session)?,
        ask_questions_as_choices: state.store.ask_questions_as_choices()?,
        user_language: state.store.interface_language()?,
        cwd: cwd.to_path_buf(),
        task: task.map(str::to_string),
        resume_agent_session_id: resume_agent_session_id.map(str::to_string),
        config_dir: session.config_dir.as_ref().map(PathBuf::from),
        settings: launch_provider_settings(state, session)?,
        shared_default_dir: sharing.shared_default_dir,
        model,
        effort,
        terminal_appearance,
        display,
        preresolved_shell_env: None,
        daemon_port: state.port,
        daemon_key: state.key.clone(),
        // A copy of the running build where the daemon updates itself: an update replaces its own
        // path with the new build, which helpers would run against this one.
        self_exe: crate::self_update::helpers::helper_exe(state).await,
        install_exe: state.install_exe.clone(),
        // Issued per launch: the previous process is gone, and a token outliving it would let a
        // stale child act on this session.
        mcp_token: state.issue_mcp_token(&session.id),
        guard,
        process_tag: pending_launch.tag().clone(),
    };

    // The rest runs as a task of its own, not as part of the request this was asked in. A holder
    // that has not confirmed the start may have the program running, and may answer long after
    // that request has been dropped: until it does, the task keeps the launch's claim and its
    // place in the ledger, so the session stays claimed and unavailable, and registers what
    // started whether or not anyone is still waiting for it.
    let (report, outcome) = tokio::sync::oneshot::channel();
    tokio::spawn(run_launch(
        state.clone(),
        claim.share(),
        pending_launch,
        request,
        launch,
        report,
    ));
    outcome
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("the launch ended without an outcome")))
}

/// Starts the program of `request` through `launch`, registers it as the session's live process
/// and starts watching it, then hands the outcome to `report`. Holds `claim` and the
/// ledger's reservation until it is done, whoever is left to hear the outcome.
async fn run_launch(
    state: Arc<AppState>,
    claim: crate::state::LaunchClaim,
    _pending_launch: PendingLaunch,
    request: term::LaunchRequest,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
    report: tokio::sync::oneshot::Sender<Result<()>>,
) {
    let session_id = request.session_id.clone();
    // Launching snapshots the user's shell environment and forks a process, so it goes off the
    // runtime rather than holding up the socket it was asked on.
    let launched = match tokio::task::spawn_blocking(move || launch(request)).await {
        Ok(launched) => launched,
        Err(err) => Err(err.into()),
    };
    match launched {
        Ok(launched) => {
            let registered = register_launched(&state, &claim, &session_id, launched);
            report.send(registered).ok();
        }
        Err(err) => {
            let Err(Err(err)) = report.send(Err(err)) else {
                return;
            };
            // The request that asked for the launch is gone, and with it what it would have done
            // about the failure: nothing is left to move the session out of `starting`.
            tracing::warn!(session = %session_id, %err, "a launch nobody waited for failed");
            state.revoke_mcp_tokens(&session_id);
            state.discard_outbox(&session_id);
            if let Err(err) = state.interrupt_session_record(&session_id) {
                tracing::warn!(session = %session_id, %err, "marking a session whose launch failed interrupted failed");
            }
        }
    }
}

/// Makes `launched` the live process of session `session_id`, under `claim`, and starts watching
/// it.
fn register_launched(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    session_id: &str,
    launched: term::Launch,
) -> Result<()> {
    // Octoboard's own account switch, or the restoring of a session after one, watches the
    // process come up and reports the outcome itself.
    if claim.watches_launch() {
        launched.session.watch_under_claim();
    }

    // The live map now says the session is running; the claim stays with `run_launch`, which
    // drops it once it is done with the launch.
    state.register_live(launched.session.clone());
    // The output of an earlier run is stale from here on: were this process to end without saving
    // (a daemon killed outright), it must not stand in for what this one printed. It goes after
    // `register_live`, so a client attaching meanwhile finds either the process or the saved
    // output, never neither (and a launch that fails leaves it to show). It also goes before
    // `watch_exit`, so the exit save of this process is never the one removed; `stop_all_sessions`
    // saves too, but only after `terminate` has returned, which waits on the process, long past
    // this point.
    state.forget_saved_output(session_id);
    super::exit_watch::watch_exit(state, launched.session.clone());
    trust::supervise(state, &launched.session);
    dialogs::watch(state, &launched.session);
    if let Some(agent_session_id) = launched.agent_session_id {
        if let Err(err) = state.set_agent_session_id(session_id, &agent_session_id) {
            // Registered already, and the caller will not watch it come up.
            state.release_launch_watch(session_id);
            return Err(err);
        }
    }
    if let Some(notice) = launched.notice {
        state.broadcast(notice.about(session_id));
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
///
/// Counted by the handover barrier until the process has ended, from this call on rather than
/// from when the blocking thread starts, so that a planned stop waits for an end an archive or a
/// deletion began and returned before; never refused, since what began it was admitted.
pub(crate) fn stop_process(
    state: &Arc<AppState>,
    live: Arc<LiveSession>,
) -> tokio::task::JoinHandle<bool> {
    let ledger = Arc::clone(&state.process_ledger);
    let tracked = state.track_change();
    tokio::task::spawn_blocking(move || {
        let _tracked = tracked;
        ledger.scan_before_ending();
        live.end()
    })
}

/// [`stop_process`], answering with the trees of the session's launches that were running before
/// it was stopped as well, so a restart can tell what it ended. The listing reads the table, and
/// the read before the end is made again right after it, to keep what the agent started meanwhile
/// attributed while its chain through the agent still shows. Counted by the handover barrier as
/// [`stop_process`] is.
pub(crate) fn stop_process_listing_trees(
    state: &Arc<AppState>,
    live: Arc<LiveSession>,
) -> tokio::task::JoinHandle<(bool, Vec<ProcessTree>)> {
    let ledger = Arc::clone(&state.process_ledger);
    let tracked = state.track_change();
    tokio::task::spawn_blocking(move || {
        let _tracked = tracked;
        let running = ledger.trees_of_sessions(std::slice::from_ref(&live.id), Duration::ZERO);
        ledger.scan_before_ending();
        (live.end(), running)
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
