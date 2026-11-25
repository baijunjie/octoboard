//! Running a scheduled task once: starting its session through the same opening path as
//! `open_session`, with the prompt as the opening task, waiting for the launch to settle, and
//! recording the run, started or failed; and the requests that run a task now and list its runs.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::protocol::{
    error_code, CodedError, Event, Origin, ScheduledRunOutcome, ScheduledRunTrigger, ScheduledTask,
    ScheduledTaskRun, Session, SessionKind, SessionStatus,
};
use crate::session::term;
use crate::state::AppState;
use crate::time::now_millis;

use super::super::accounts::AccountGate;
use super::super::open::{open_session_with, resolve_open_with, OpenRequest};
use super::super::validate::field_required;

/// What starts a run's agent: `term::launch` outside the tests, which cannot start an agent.
pub(crate) type Launcher =
    Arc<dyn Fn(term::LaunchRequest) -> Result<term::Launch> + Send + Sync + 'static>;

pub(crate) fn agent_launcher() -> Launcher {
    Arc::new(term::launch)
}

/// Why [`run_task`] recorded no run.
#[derive(Debug, PartialEq)]
pub(super) enum NotRecorded {
    /// The daemon is stopping, which refused the launch: a scheduled run or a catch-up is left for
    /// the daemon that starts next.
    Stopping,
    /// The task is gone (deleted while its session was being started).
    TaskGone,
    /// The store failed to record the run (logged); the session, if one started, is left running.
    StoreFailed,
}

/// Runs `task` once for `trigger`, standing for the occurrence `due_at`, and records the run:
/// started with its session, or failed with the refusal or launch failure. Every client is told of
/// the run (`scheduled_task_run_added`) and of the task with its new `last_run_at`. A launch the
/// daemon's stop refused is not recorded for a scheduled run or a catch-up, only for a run now,
/// which the user asked for.
pub(super) async fn run_task(
    state: &Arc<AppState>,
    task: &ScheduledTask,
    trigger: ScheduledRunTrigger,
    due_at: Option<i64>,
    launch: &Launcher,
) -> Result<ScheduledTaskRun, NotRecorded> {
    let outcome = match start_session(state, task, launch).await {
        Ok((session, started_independent)) => ScheduledRunOutcome::Started {
            session: session.id,
            started_independent,
        },
        Err(err) => {
            let failure = CodedError::of(&err);
            if failure.code == error_code::DAEMON_SHUTTING_DOWN
                && trigger != ScheduledRunTrigger::RunNow
            {
                return Err(NotRecorded::Stopping);
            }
            tracing::info!(task = %task.id, %err, "a scheduled task's run started no session");
            ScheduledRunOutcome::Failed {
                code: failure.code.as_str().to_string(),
                params: failure.params,
                message: failure.message,
            }
        }
    };
    let run = ScheduledTaskRun {
        id: Uuid::new_v4().to_string(),
        task_id: task.id.clone(),
        ran_at: now_millis(),
        due_at,
        trigger,
        outcome,
    };
    match state.store.record_scheduled_task_run(&run) {
        Ok(Some(updated)) => {
            state.broadcast(Event::ScheduledTaskUpserted { task: updated });
            state.broadcast(Event::ScheduledTaskRunAdded { run: run.clone() });
            Ok(run)
        }
        Ok(None) => Err(NotRecorded::TaskGone),
        Err(err) => {
            tracing::warn!(task = %task.id, %err, "recording a scheduled task's run failed");
            Err(NotRecorded::StoreFailed)
        }
    }
}

/// Starts `task`'s session and returns it once launched, with whether it was started independent
/// although the task names a console session to report to. That console session unable to take
/// sessions at that moment (archived, deleted or still starting) leaves the session independent.
/// An account of the task's that is turned off, used up or not logged in gives way, while
/// automatic account selection is on, to the pick a new session gets (`open_session`'s, the
/// account left to the daemon); otherwise, or with no usable account to pick, the run fails with
/// that refusal.
async fn start_session(
    state: &Arc<AppState>,
    task: &ScheduledTask,
    launch: &Launcher,
) -> Result<(Session, bool)> {
    // A task saved before a prompt of nothing but control and invisible characters was refused
    // can still hold one. Opening its session would hand the agent no task, so the run fails with
    // what saving such a prompt is refused with instead of starting a session that sits idle.
    if term::is_blank_once_stripped(&task.prompt) {
        return Err(field_required("prompt"));
    }
    let mut request = open_request(task);
    let mut independent = false;
    if let Some(owner) = &request.bound_to {
        if !owner_can_take_sessions(state, &task.console_id, owner)? {
            request.bound_to = None;
            independent = true;
        }
    }
    if let Err(err) = resolve_open_with(state, &request, AccountGate::Full) {
        let gives_way = request.account.is_some()
            && is_unusable_account(&err)
            && state.store.auto_select_account()?;
        if !gives_way {
            return Err(err);
        }
        request.account = None;
        resolve_open_with(state, &request, AccountGate::Full)?;
    }
    let opened = {
        let launch = launch.clone();
        open_session_with(state, request.clone(), move |asked| launch(asked)).await
    };
    match opened {
        Ok(session) => Ok((session, independent)),
        // The console session was archived or deleted while the opening checked the launch.
        Err(err)
            if request.bound_to.as_deref().is_some_and(|owner| {
                CodedError::of(&err).code == error_code::UNKNOWN_SESSION
                    && !owner_can_take_sessions(state, &task.console_id, owner).unwrap_or(true)
            }) =>
        {
            request.bound_to = None;
            let launch = launch.clone();
            let session = open_session_with(state, request, move |asked| launch(asked)).await?;
            Ok((session, true))
        }
        Err(err) => Err(err),
    }
}

/// What opening `task`'s session asks for: its target and launch settings, its prompt as the
/// opening task, and no window's request.
fn open_request(task: &ScheduledTask) -> OpenRequest {
    let launch = &task.launch;
    OpenRequest {
        console_id: task.console_id.clone(),
        project_id: task.project_id.clone(),
        agent: launch.agent,
        account: launch.account.clone(),
        model: launch.model.clone(),
        effort: launch.effort.clone(),
        compatibility: launch
            .compatibility
            .iter()
            .map(|(id, value)| (id.clone(), Some(*value)))
            .collect::<BTreeMap<_, _>>(),
        task: Some(task.prompt.clone()),
        title: launch.title.clone(),
        origin: Origin::User,
        bound_to: task.project_id.as_ref().and(launch.bound_to.clone()),
        resume_agent_session_id: None,
        session_id: None,
        terminal_appearance: None,
    }
}

/// Whether the console session `owner` can be reported to at this moment, by the filter of
/// `resolve_owner` for a console session: recorded, of the task's console `console_id`, and neither
/// archived nor still starting (its launch may fail, and a failed launch deletes the session).
fn owner_can_take_sessions(state: &Arc<AppState>, console_id: &str, owner: &str) -> Result<bool> {
    Ok(state.store.get_session(owner)?.is_some_and(|session| {
        session.kind == SessionKind::Console
            && session.console_id == console_id
            && !matches!(
                session.status,
                SessionStatus::Starting | SessionStatus::Archived
            )
    }))
}

/// Whether `err` refuses the account for a state it is in at the moment: turned off, used up or
/// not logged in.
fn is_unusable_account(err: &anyhow::Error) -> bool {
    let code = CodedError::of(err).code;
    [
        error_code::ACCOUNT_DISABLED,
        error_code::ACCOUNT_QUOTA_EXHAUSTED,
        error_code::ACCOUNT_NOT_SIGNED_IN,
    ]
    .contains(&code)
}

/// `run_scheduled_task`: refuses an unknown task, and a run while the daemon is stopping;
/// otherwise runs it in the background, whatever its schedule and whether it is enabled, and
/// leaves its schedule alone. The run is announced once its launch has settled.
pub(super) fn run_scheduled_task_now(
    state: &Arc<AppState>,
    task_id: &str,
    launch: Launcher,
) -> Result<()> {
    let task = super::check_run_scheduled_task(state, task_id)?;
    let state = state.clone();
    tokio::spawn(async move {
        let _ = run_task(&state, &task, ScheduledRunTrigger::RunNow, None, &launch).await;
    });
    Ok(())
}

/// `list_scheduled_task_runs`: the task's kept runs, most recent first.
pub(crate) fn list_scheduled_task_runs(
    state: &Arc<AppState>,
    request_id: Option<String>,
    task_id: &str,
) -> Result<Option<Event>> {
    state.store.require_scheduled_task(task_id)?;
    Ok(Some(Event::ScheduledTaskRunsListed {
        id: request_id,
        task: task_id.to_string(),
        runs: state.store.list_scheduled_task_runs(task_id)?,
    }))
}
