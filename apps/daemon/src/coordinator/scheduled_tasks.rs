//! Scheduled tasks: the requests that list, create, change, turn on and off and delete them. A
//! task's launch settings are checked as `open_session` checks them, through
//! [`check_saved_open`], so a task is refused for whatever would refuse the session it opens; its
//! next run is computed by [`next_run`] in the daemon machine's time zone. The [`scheduler`] runs
//! a task when it comes due and [`runs`] runs it once and records the run; every change of a task
//! here wakes the scheduler. The agents' scheduled task tools (`mcp::tools::scheduled_tasks`) call
//! the same functions, and the `check_*` ones to refuse before the user is asked whatever the
//! operation would refuse.

mod next_run;
#[cfg(test)]
mod run_tests;
mod runs;
pub mod scheduler;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::protocol::{
    error_code, CodedError, Event, Origin, Schedule, ScheduledTask, ScheduledTaskLaunch,
    SessionKind,
};
use crate::state::AppState;
use crate::store::ScheduledTaskEdit;
use crate::text::trimmed_or_none;
use crate::time::now_millis;

use super::open::{check_saved_open, OpenRequest};
use super::validate::{field_required, required_text};
use next_run::zone_name;
pub(crate) use next_run::{local_zone, next_run};
pub(crate) use runs::list_scheduled_task_runs;

/// `run_scheduled_task`: see [`runs::run_scheduled_task_now`].
pub(crate) fn run_scheduled_task(state: &Arc<AppState>, task_id: &str) -> Result<()> {
    runs::run_scheduled_task_now(state, task_id, runs::agent_launcher())
}

/// What `run_scheduled_task` refuses before it runs the task: an unknown task, and a run while the
/// daemon is stopping or makes a clean-up run, which launches no agent: refused here, the run is
/// not recorded as one that failed. Runs nothing, and gives the task.
pub(crate) fn check_run_scheduled_task(
    state: &Arc<AppState>,
    task_id: &str,
) -> Result<ScheduledTask> {
    let task = state.store.require_scheduled_task(task_id)?;
    state.refuse_while_shutting_down()?;
    state.admit_launch()?;
    Ok(task)
}

/// What a `create_scheduled_task` asks for.
pub(crate) struct NewScheduledTask {
    pub console_id: String,
    pub project_id: Option<String>,
    pub launch: Option<ScheduledTaskLaunch>,
    pub prompt: String,
    pub schedule: Schedule,
    pub enabled: bool,
}

/// What an `update_scheduled_task` changes; `None` leaves a field alone, and for `project_id`
/// `Some(None)` makes a console task.
pub(crate) struct ScheduledTaskChanges {
    pub console_id: Option<String>,
    pub project_id: Option<Option<String>>,
    pub launch: Option<ScheduledTaskLaunch>,
    pub prompt: Option<String>,
    pub schedule: Option<Schedule>,
}

pub(crate) fn list_scheduled_tasks(
    state: &Arc<AppState>,
    request_id: Option<String>,
) -> Result<Option<Event>> {
    Ok(Some(Event::ScheduledTasksListed {
        id: request_id,
        tasks: state.store.list_scheduled_tasks()?,
        time_zone: zone_name(&local_zone()),
    }))
}

/// `new` as it would be stored, or the refusal: [`checked_definition`] of what it asks for.
async fn checked_new(state: &Arc<AppState>, new: &NewScheduledTask) -> Result<Definition> {
    checked_definition(
        state,
        Definition {
            console_id: new.console_id.clone(),
            project_id: new.project_id.clone(),
            launch: new.launch.clone().unwrap_or_default(),
            prompt: new.prompt.clone(),
            schedule: new.schedule.clone(),
        },
        true,
        now_millis(),
    )
    .await
}

/// What `create_scheduled_task` would refuse `new` for, checked as it checks it and saving
/// nothing. Gives the launch settings as the task would be saved with them: the title trimmed, the
/// model and effort normalised, and no `bound_to` for a console task.
pub(crate) async fn check_new_scheduled_task(
    state: &Arc<AppState>,
    new: &NewScheduledTask,
) -> Result<ScheduledTaskLaunch> {
    checked_new(state, new).await.map(|checked| checked.launch)
}

pub(crate) async fn create_scheduled_task(
    state: &Arc<AppState>,
    request_id: Option<String>,
    new: NewScheduledTask,
) -> Result<Option<Event>> {
    let definition = checked_new(state, &new).await?;
    let enabled = new.enabled;
    let now = now_millis();
    let task = ScheduledTask {
        id: Uuid::new_v4().to_string(),
        next_run_at: enabled
            .then(|| next_run(&definition.schedule, now, &local_zone()))
            .flatten(),
        console_id: definition.console_id,
        project_id: definition.project_id,
        launch: definition.launch,
        prompt: definition.prompt,
        schedule: definition.schedule,
        enabled,
        created_at: now,
        updated_at: now,
        last_run_at: None,
        run_sessions: vec![],
    };
    state.store.insert_scheduled_task(&task)?;
    Ok(Some(saved(state, request_id, task)))
}

/// How many times an edit is made again from a fresh read when another edit lands first.
const EDIT_ATTEMPTS: usize = 3;

#[cfg(test)]
thread_local! {
    /// A test's hold on the next edit between its read and its write: the edit signals `reached`
    /// and waits for `resume`. Taken on the calling thread before the edit's first await.
    static EDIT_PAUSE: std::cell::RefCell<Option<EditPause>> = const { std::cell::RefCell::new(None) };
    /// How many times the edits on this thread have read the task to make an attempt.
    static EDIT_ATTEMPTS_MADE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The two signals of a held edit; see `EDIT_PAUSE`.
#[cfg(test)]
struct EditPause {
    reached: Arc<tokio::sync::Notify>,
    resume: Arc<tokio::sync::Notify>,
}

/// Changes a task. Every field of the result is checked, not only the changed ones: the launch
/// settings depend on the target, and a project or console session they name may be gone since.
/// A one-off time is refused as passed only when the schedule is changed. `project_id` absent keeps
/// the task's project, which moving to another console refuses (`unknown_project`) unless it
/// belongs to that console; `null` makes a console task. `enabled` is not written, and the next run
/// is decided by the store from the task as it is when the edit lands (see
/// [`crate::store::Store::edit_scheduled_task`]), as checking the launch settings can take
/// seconds. The changes are merged onto the task as read, and the store writes only if the task's
/// definition is still that one: when another edit landed during the check, this one is made again
/// from a fresh read (up to [`EDIT_ATTEMPTS`] times), so neither edit's fields are lost.
pub(crate) async fn update_scheduled_task(
    state: &Arc<AppState>,
    request_id: Option<String>,
    task_id: &str,
    changes: ScheduledTaskChanges,
) -> Result<Option<Event>> {
    let schedule_changed = changes.schedule.is_some();
    #[cfg(test)]
    let mut pause = EDIT_PAUSE.with(|slot| slot.borrow_mut().take());
    for _ in 0..EDIT_ATTEMPTS {
        let task = state.store.require_scheduled_task(task_id)?;
        #[cfg(test)]
        EDIT_ATTEMPTS_MADE.with(|made| made.set(made.get() + 1));
        let definition = checked_definition(
            state,
            changed_definition(&task, &changes),
            schedule_changed,
            now_millis(),
        )
        .await?;
        #[cfg(test)]
        if let Some(pause) = pause.take() {
            pause.reached.notify_one();
            pause.resume.notified().await;
        }
        let now = now_millis();
        let edited = ScheduledTask {
            console_id: definition.console_id,
            project_id: definition.project_id,
            launch: definition.launch,
            prompt: definition.prompt,
            schedule: definition.schedule,
            updated_at: now,
            ..task.clone()
        };
        match state
            .store
            .edit_scheduled_task(&edited, &task, |schedule| {
                next_run(schedule, now, &local_zone())
            })? {
            ScheduledTaskEdit::Edited(updated) => {
                return Ok(Some(saved(state, request_id, *updated)))
            }
            ScheduledTaskEdit::Gone => return Err(CodedError::unknown_scheduled_task(task_id)),
            ScheduledTaskEdit::Changed => {}
        }
    }
    Err(anyhow::anyhow!(
        "scheduled task {task_id} kept changing while it was being edited"
    ))
}

/// `task` with `changes` merged onto it, not yet checked.
fn changed_definition(task: &ScheduledTask, changes: &ScheduledTaskChanges) -> Definition {
    let (console_id, project_id) = match (&changes.console_id, &changes.project_id) {
        (Some(console_id), Some(project_id)) => (console_id.clone(), project_id.clone()),
        (Some(console_id), None) => (console_id.clone(), task.project_id.clone()),
        (None, Some(project_id)) => (task.console_id.clone(), project_id.clone()),
        (None, None) => (task.console_id.clone(), task.project_id.clone()),
    };
    Definition {
        console_id,
        project_id,
        launch: changes
            .launch
            .clone()
            .unwrap_or_else(|| task.launch.clone()),
        prompt: changes
            .prompt
            .clone()
            .unwrap_or_else(|| task.prompt.clone()),
        schedule: changes
            .schedule
            .clone()
            .unwrap_or_else(|| task.schedule.clone()),
    }
}

/// What `update_scheduled_task` would refuse `changes` to the task for, checked as it checks
/// them, once, and saving nothing. Gives the launch settings the task would be saved with, as
/// [`check_new_scheduled_task`] does.
pub(crate) async fn check_scheduled_task_changes(
    state: &Arc<AppState>,
    task_id: &str,
    changes: &ScheduledTaskChanges,
) -> Result<ScheduledTaskLaunch> {
    let task = state.store.require_scheduled_task(task_id)?;
    checked_definition(
        state,
        changed_definition(&task, changes),
        changes.schedule.is_some(),
        now_millis(),
    )
    .await
    .map(|checked| checked.launch)
}

/// Turns a task on or off. A disabled task has no next run; enabling one computes it from now,
/// and turning on one that is already on leaves its next run alone.
pub(crate) fn set_scheduled_task_enabled(
    state: &Arc<AppState>,
    request_id: Option<String>,
    task_id: &str,
    enabled: bool,
) -> Result<Option<Event>> {
    let now = now_millis();
    let Some(updated) =
        state
            .store
            .set_scheduled_task_enabled(task_id, enabled, now, |schedule| {
                next_run(schedule, now, &local_zone())
            })?
    else {
        return Err(CodedError::unknown_scheduled_task(task_id));
    };
    Ok(Some(saved(state, request_id, updated)))
}

pub(crate) fn delete_scheduled_task(state: &Arc<AppState>, task_id: &str) -> Result<()> {
    if !state.store.delete_scheduled_task(task_id)? {
        return Err(CodedError::unknown_scheduled_task(task_id));
    }
    state.scheduler_wake.wake();
    state.broadcast(Event::ScheduledTaskDeleted {
        task: task_id.to_string(),
    });
    Ok(())
}

/// Tells every client of the stored `task` and answers the request with it. Wakes the scheduler,
/// whose earliest next run may have changed.
fn saved(state: &Arc<AppState>, request_id: Option<String>, task: ScheduledTask) -> Event {
    state.scheduler_wake.wake();
    state.broadcast(Event::ScheduledTaskUpserted { task: task.clone() });
    Event::ScheduledTaskSaved {
        id: request_id,
        task,
    }
}

/// What a task starts and with what, before and after [`checked_definition`].
struct Definition {
    console_id: String,
    project_id: Option<String>,
    launch: ScheduledTaskLaunch,
    prompt: String,
    schedule: Schedule,
}

/// `definition` as it is stored, or the refusal: a blank prompt, a schedule that is not well
/// formed (a one-off time not after `now` only when `check_once`), a project of another console,
/// and whatever `open_session` would refuse the launch settings for. The prompt and title are
/// trimmed, a weekly schedule's days sorted with each named once, the model and effort normalised
/// as a session records them, and a console task's `bound_to` dropped, as `open_session` ignores
/// it for a console session. A `bound_to` has to name a console session of the task's console that
/// the daemon holds, whatever its status: whether it can take a session is decided when the task
/// runs.
async fn checked_definition(
    state: &Arc<AppState>,
    definition: Definition,
    check_once: bool,
    now: i64,
) -> Result<Definition> {
    let Definition {
        console_id,
        project_id,
        mut launch,
        prompt,
        schedule,
    } = definition;
    let prompt = required_text("prompt", &prompt)?;
    let schedule = checked_schedule(schedule, check_once, now)?;
    if let Some(project_id) = &project_id {
        let project = state.store.require_project(project_id)?;
        if project.console_id != console_id {
            return Err(CodedError::unknown_project(project_id));
        }
    }
    launch.title = trimmed_or_none(launch.title.as_deref());
    if project_id.is_none() {
        launch.bound_to = None;
    }
    if let Some(owner) = &launch.bound_to {
        // `open_session` also takes an unbound project session as an owner; a task reports only
        // to a console session, which outlives any one run. Its status is not checked here (the
        // opening below is not given the owner): a session that is archived or still starting
        // when the task runs leaves the run independent.
        let is_console_session = state.store.get_session(owner)?.is_some_and(|session| {
            session.kind == SessionKind::Console && session.console_id == console_id
        });
        if !is_console_session {
            return Err(CodedError::unknown_session(owner));
        }
    }
    let checked = check_saved_open(
        state,
        &OpenRequest {
            console_id: console_id.clone(),
            project_id: project_id.clone(),
            agent: launch.agent,
            account: launch.account.clone(),
            model: launch.model.clone(),
            effort: launch.effort.clone(),
            compatibility: launch
                .compatibility
                .iter()
                .map(|(id, value)| (id.clone(), Some(*value)))
                .collect::<BTreeMap<_, _>>(),
            task: Some(prompt.clone()),
            title: launch.title.clone(),
            origin: Origin::User,
            bound_to: None,
            resume_agent_session_id: None,
            session_id: None,
            terminal_appearance: None,
        },
    )
    .await?;
    launch.model = checked.model;
    launch.effort = checked.effort;
    Ok(Definition {
        console_id,
        project_id,
        launch,
        prompt,
        schedule,
    })
}

/// `schedule` as stored, or the refusal: an hour, minute or day of the month out of range
/// (`unreadable_request`, a shape no client sends), a weekly schedule with no day
/// (`field_required`, `days`), and, when `check_once`, a one-off time not after `now`
/// (`scheduled_time_passed`).
fn checked_schedule(schedule: Schedule, check_once: bool, now: i64) -> Result<Schedule> {
    let out_of_range = |what: &str| {
        CodedError::raised(
            error_code::UNREADABLE_REQUEST,
            format!("the schedule's {what} is out of range"),
            &[],
        )
    };
    let check_time = |time: &crate::protocol::TimeOfDay| {
        if time.hour > 23 {
            return Err(out_of_range("hour"));
        }
        if time.minute > 59 {
            return Err(out_of_range("minute"));
        }
        Ok(())
    };
    match schedule {
        Schedule::Once { run_at } if check_once && run_at <= now => Err(CodedError::raised(
            error_code::SCHEDULED_TIME_PASSED,
            "the one-off time is not in the future",
            &[("run_at", &run_at.to_string())],
        )),
        Schedule::Hourly { minute } if minute > 59 => Err(out_of_range("minute")),
        Schedule::Daily { time } | Schedule::Weekdays { time } => {
            check_time(&time)?;
            Ok(schedule)
        }
        Schedule::Monthly { day, time } => {
            if !(1..=31).contains(&day) {
                return Err(out_of_range("day"));
            }
            check_time(&time)?;
            Ok(schedule)
        }
        Schedule::Weekly { time, mut days } => {
            check_time(&time)?;
            if days.is_empty() {
                return Err(field_required("days"));
            }
            days.sort();
            days.dedup();
            Ok(Schedule::Weekly { time, days })
        }
        other => Ok(other),
    }
}
