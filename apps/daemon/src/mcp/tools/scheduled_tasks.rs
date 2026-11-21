//! The tools with which a console session or a project coordinator reads and changes scheduled
//! tasks: `list_scheduled_tasks`, `list_scheduled_task_runs`, `create_scheduled_task`,
//! `update_scheduled_task`, `set_scheduled_task_enabled`, `delete_scheduled_task` and
//! `run_scheduled_task`. What each does and refuses is the coordinator's
//! (`crate::coordinator::scheduled_tasks`, the functions the control socket calls); what is here is
//! the caller's side: the tasks within its reach (a console session's console, a coordinator's own
//! project), the arguments read as the control requests carry them, and, for the five that write,
//! the user being asked first unless they chose to have scheduled task changes approved without
//! asking (`session_request`). Whatever the operation would refuse is refused before the user is
//! asked.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::coordinator::scheduled_tasks::{
    check_new_scheduled_task, check_run_scheduled_task, check_scheduled_task_changes,
    create_scheduled_task as create, delete_scheduled_task as delete,
    list_scheduled_task_runs as runs_of, list_scheduled_tasks as list, run_scheduled_task as run,
    set_scheduled_task_enabled as enable, update_scheduled_task as update, NewScheduledTask,
    ScheduledTaskChanges,
};
use crate::mcp::lookup::{
    optional_bool, optional_text, own_project, parse_agent, present_text, raw_text, required_str,
    resolve_account, resolve_project,
};
use crate::protocol::{
    AgentId, CodedError, Event, Schedule, ScheduledTask, ScheduledTaskAsked, ScheduledTaskLaunch,
    Session, SessionKind, SessionRequestAction,
};
use crate::session_request::{
    self, run_replacing, TaskChange, TaskPerform, TaskPlace, ANSWER_TIME_LIMIT,
};
use crate::state::AppState;

/// What a tool that writes returns. They are boxed in a function of their own because their
/// futures are large, and `dispatch` holds a temporary for each tool it can call: in a debug build
/// that put a long test of the tools past its stack.
type CallFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send + 'a>>;

/// The tasks a caller reaches: a console session's console, a coordinator's own project.
enum Scope {
    Console(String),
    Project { console: String, project: String },
}

impl Scope {
    fn of(caller: &Session) -> Result<Self> {
        match (caller.kind, caller.project_id.clone()) {
            (SessionKind::Console, _) => Ok(Self::Console(caller.console_id.clone())),
            (SessionKind::Project, Some(project)) => Ok(Self::Project {
                console: caller.console_id.clone(),
                project,
            }),
            (SessionKind::Project, None) => bail!("this session's project is no longer on record"),
        }
    }

    /// The task `id` for an approval carried out minutes after the call was made: within the
    /// caller's reach as its record and the task stand now. One that was deleted or moved out of
    /// reach meanwhile is the coded `unknown_scheduled_task`, as the coordinator refuses an unknown
    /// task, since the user is told of it.
    fn task_now(state: &Arc<AppState>, caller_id: &str, id: &str) -> Result<ScheduledTask> {
        let scope = Self::of(&state.session_record(caller_id)?)?;
        state
            .store
            .get_scheduled_task(id)?
            .filter(|task| scope.holds(task))
            .ok_or_else(|| CodedError::unknown_scheduled_task(id))
    }

    fn holds(&self, task: &ScheduledTask) -> bool {
        match self {
            Self::Console(console) => task.console_id == *console,
            Self::Project { console, project } => {
                task.console_id == *console && task.project_id.as_deref() == Some(project)
            }
        }
    }

    /// The task `id`, or the refusal that it is none of the caller's: an unknown task and one
    /// outside its reach are told alike.
    fn task(&self, state: &Arc<AppState>, id: &str) -> Result<ScheduledTask> {
        state
            .store
            .get_scheduled_task(id)?
            .filter(|task| self.holds(task))
            .ok_or_else(|| {
                anyhow!(
                    "there is no scheduled task `{id}` among {}; `list_scheduled_tasks` names them",
                    match self {
                        Self::Console(_) => "this console's tasks",
                        Self::Project { .. } => "this project's tasks",
                    }
                )
            })
    }

    /// The project a task is created for: for a console session the one `project` names (by id or
    /// by an unambiguous name), none when it is left out or `null`, which makes a console task;
    /// for a coordinator always its own, which it may name.
    fn project_to_create(
        &self,
        state: &Arc<AppState>,
        caller: &Session,
        arguments: &Map<String, Value>,
    ) -> Result<Option<String>> {
        match (self, arguments.get("project")) {
            (Self::Console(_), None | Some(Value::Null)) => Ok(None),
            (Self::Project { project, .. }, None) => Ok(Some(project.clone())),
            _ => self
                .named_project(state, caller, arguments)
                .map(|named| named.flatten()),
        }
    }

    /// What `project` names within the scope: `None` when it is left out, `Some(None)` for `null`,
    /// else the project's id. A coordinator names only its own project.
    fn named_project(
        &self,
        state: &Arc<AppState>,
        caller: &Session,
        arguments: &Map<String, Value>,
    ) -> Result<Option<Option<String>>> {
        let named = match arguments.get("project") {
            None => return Ok(None),
            Some(Value::Null) => None,
            Some(Value::String(named)) if !named.trim().is_empty() => Some(named.trim()),
            Some(_) => bail!("`project` must be a project's id or name, or null"),
        };
        match (self, named) {
            (Self::Console(console), Some(named)) => {
                Ok(Some(Some(resolve_project(state, console, named)?.id)))
            }
            (Self::Console(_), None) => Ok(Some(None)),
            (Self::Project { .. }, None) => bail!(
                "a project session works only on tasks of its own project, so a task cannot be \
                 made a console task"
            ),
            (Self::Project { .. }, Some(named)) => Ok(Some(Some(
                own_project(
                    state,
                    caller,
                    Some(named.to_string()),
                    "work only on the tasks of",
                )?
                .id,
            ))),
        }
    }
}

/// A task as the tools show it: the stored record, its launch settings as stored (a setting left
/// out follows the project's, else the console's, at each run).
fn describe_task(task: &ScheduledTask) -> Value {
    json!({
        "id": task.id,
        "console": task.console_id,
        "project": task.project_id,
        "launch": task.launch,
        "prompt": task.prompt,
        "schedule": task.schedule,
        "enabled": task.enabled,
        "next_run_at": task.next_run_at,
        "last_run_at": task.last_run_at,
        "created_at": task.created_at,
        "updated_at": task.updated_at,
        "run_sessions": task.run_sessions,
    })
}

/// What a saved task's note says of its next run.
fn next_run_note(task: &ScheduledTask) -> &'static str {
    match (task.enabled, task.next_run_at) {
        (false, _) => "It is off, so it does not run on its schedule until it is turned on.",
        (true, Some(_)) => "It is on and next runs at `next_run_at`.",
        (true, None) => {
            "It is on but has no next run, since its schedule is manual or its one-off time has \
             passed: it runs only when run by hand."
        }
    }
}

pub(in crate::mcp) fn list_scheduled_tasks(
    state: &Arc<AppState>,
    caller: &Session,
) -> Result<Value> {
    let scope = Scope::of(caller)?;
    let Some(Event::ScheduledTasksListed {
        tasks, time_zone, ..
    }) = list(state, None)?
    else {
        bail!("the scheduled tasks could not be listed");
    };
    let tasks: Vec<Value> = tasks
        .iter()
        .filter(|task| scope.holds(task))
        .map(describe_task)
        .collect();
    Ok(json!({ "tasks": tasks, "time_zone": time_zone }))
}

pub(in crate::mcp) fn list_scheduled_task_runs(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let task = Scope::of(caller)?.task(state, required_str(arguments, "task")?)?;
    let Some(Event::ScheduledTaskRunsListed { runs, .. }) = runs_of(state, None, &task.id)? else {
        bail!("the runs could not be listed");
    };
    Ok(json!({ "task": task.id, "runs": runs }))
}

/// The task's schedule, as the control requests carry it.
fn schedule_argument(arguments: &Map<String, Value>) -> Result<Schedule> {
    let wanted = arguments
        .get("schedule")
        .ok_or_else(|| anyhow!("`schedule` is required"))?;
    schedule_from(wanted)
}

fn schedule_from(wanted: &Value) -> Result<Schedule> {
    serde_json::from_value(wanted.clone()).map_err(|err| {
        anyhow!(
            "`schedule` is not a schedule ({err}): it is an object whose `kind` is one of `once`, \
             `hourly`, `daily`, `weekdays`, `weekly`, `monthly` or `manual`, with the fields that \
             kind needs"
        )
    })
}

/// The `launch` argument as the control requests carry it, with the account by id or name and the
/// model and effort as `open_session` takes them: blank is unset (the default model, the agent's
/// own effort).
fn launch_from(state: &Arc<AppState>, wanted: &Value) -> Result<ScheduledTaskLaunch> {
    let fields = match wanted {
        Value::Null => return Ok(ScheduledTaskLaunch::default()),
        Value::Object(fields) => fields,
        _ => bail!("`launch` must be an object"),
    };
    let agent = match fields.get("agent") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(parse_agent(name)?),
        Some(_) => bail!("`launch.agent` must be an agent or null"),
    };
    let account = match fields.get("account") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(wanted)) => Some(resolve_account(state, agent, wanted)?),
        Some(_) => bail!("`launch.account` must be an account's id or name, or null"),
    };
    // A blank model or effort is unset, as in `open_session`.
    let unset_if_blank = |value: Option<Option<String>>| {
        value.map(|text| text.filter(|text| !text.trim().is_empty()))
    };
    let compatibility = match fields.get("compatibility") {
        None | Some(Value::Null) => Default::default(),
        Some(Value::Object(entries)) => entries
            .iter()
            .map(|(setting, value)| match value {
                Value::Bool(value) => Ok((setting.clone(), *value)),
                _ => Err(anyhow!(
                    "`launch.compatibility.{setting}` must be true or false"
                )),
            })
            .collect::<Result<_>>()?,
        Some(_) => bail!("`launch.compatibility` must be an object of switch to true or false"),
    };
    Ok(ScheduledTaskLaunch {
        title: optional_text(fields, "title")?,
        agent,
        account,
        model: unset_if_blank(present_text(fields, "model")?),
        effort: unset_if_blank(present_text(fields, "effort")?),
        compatibility,
        bound_to: optional_text(fields, "bound_to")?,
    })
}

/// What a write is asked as: the tool, what the dialog shows of it and where the task is.
struct Write {
    tool: &'static str,
    action: SessionRequestAction,
    asked: ScheduledTaskAsked,
    prompt: Option<String>,
    place: TaskPlace,
}

impl Write {
    /// The place of a task whose target is `console` and `project` after the change, which names
    /// `launch_agent` or else is the caller's own agent.
    fn place(
        caller: &Session,
        launch_agent: Option<AgentId>,
        console: String,
        project: Option<String>,
    ) -> TaskPlace {
        TaskPlace {
            agent: launch_agent.unwrap_or(caller.agent),
            console,
            project,
        }
    }
}

/// Carries out `write`'s `operation`: at once when the user chose to have scheduled task changes
/// approved without asking (`automatic`, read once by the caller, which refused beforehand only
/// when it was not), else after the user approves it.
async fn submit(
    state: &Arc<AppState>,
    caller: &Session,
    automatic: bool,
    write: Write,
    operation: impl std::future::Future<Output = Result<Value>> + Send + 'static,
) -> Result<Value> {
    if automatic {
        return run_replacing(
            state,
            caller,
            write.tool,
            write.asked.task.as_deref(),
            operation,
        )
        .await;
    }
    session_request::ask_to_change_task(
        state,
        caller,
        write.place,
        TaskChange {
            tool: write.tool,
            action: write.action,
            asked: write.asked,
            prompt: write.prompt,
            perform: TaskPerform::new(move |()| operation),
        },
        ANSWER_TIME_LIMIT,
    )
    .await
}

/// The task a save answered with.
fn saved(event: Option<Event>) -> Result<ScheduledTask> {
    match event {
        Some(Event::ScheduledTaskSaved { task, .. }) => Ok(task),
        _ => bail!("the scheduled task was not saved"),
    }
}

pub(in crate::mcp) fn create_scheduled_task<'a>(
    state: &'a Arc<AppState>,
    caller: &'a Session,
    arguments: &'a Map<String, Value>,
) -> CallFuture<'a> {
    Box::pin(create_scheduled_task_to_the_end(state, caller, arguments))
}

async fn create_scheduled_task_to_the_end(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let scope = Scope::of(caller)?;
    let project_id = scope.project_to_create(state, caller, arguments)?;
    let launch = match arguments.get("launch") {
        Some(wanted) => launch_from(state, wanted)?,
        None => ScheduledTaskLaunch::default(),
    };
    let prompt = raw_text(arguments, "prompt")?.ok_or_else(|| anyhow!("`prompt` is required"))?;
    let schedule = schedule_argument(arguments)?;
    let enabled = optional_bool(arguments, "enabled")?.unwrap_or(true);
    let new = NewScheduledTask {
        console_id: caller.console_id.clone(),
        project_id: project_id.clone(),
        launch: Some(launch.clone()),
        prompt: prompt.clone(),
        schedule: schedule.clone(),
        enabled,
    };
    // Read once: the call acts on it.
    let automatic = state.store.auto_approve_scheduled_task_changes()?;
    // What the dialog shows is the launch as it would be saved.
    let launch = if automatic {
        launch
    } else {
        check_new_scheduled_task(state, &new).await?
    };
    let write = Write {
        tool: "create_scheduled_task",
        action: SessionRequestAction::CreateScheduledTask,
        place: Write::place(caller, launch.agent, caller.console_id.clone(), project_id),
        asked: ScheduledTaskAsked {
            task: None,
            launch: Some(launch),
            schedule: Some(schedule),
            enabled: Some(enabled),
        },
        prompt: Some(prompt),
    };
    let state_for_run = state.clone();
    submit(state, caller, automatic, write, async move {
        let task = saved(create(&state_for_run, None, new).await?)?;
        Ok(json!({
            "note": format!(
                "Created the scheduled task `{}`. {}",
                task.id,
                next_run_note(&task)
            ),
            "task": describe_task(&task),
        }))
    })
    .await
}

pub(in crate::mcp) fn update_scheduled_task<'a>(
    state: &'a Arc<AppState>,
    caller: &'a Session,
    arguments: &'a Map<String, Value>,
) -> CallFuture<'a> {
    Box::pin(update_scheduled_task_to_the_end(state, caller, arguments))
}

async fn update_scheduled_task_to_the_end(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let scope = Scope::of(caller)?;
    let task = scope.task(state, required_str(arguments, "task")?)?;
    let changes = ScheduledTaskChanges {
        console_id: None,
        project_id: scope.named_project(state, caller, arguments)?,
        launch: arguments
            .get("launch")
            .map(|wanted| launch_from(state, wanted))
            .transpose()?,
        prompt: raw_text(arguments, "prompt")?,
        schedule: arguments.get("schedule").map(schedule_from).transpose()?,
    };
    if changes.project_id.is_none()
        && changes.launch.is_none()
        && changes.prompt.is_none()
        && changes.schedule.is_none()
    {
        bail!("name what to change: at least one of `project`, `launch`, `prompt` or `schedule`");
    }
    let automatic = state.store.auto_approve_scheduled_task_changes()?;
    // What the dialog shows is the launch as it would be saved.
    let checked_launch = if automatic {
        None
    } else {
        Some(check_scheduled_task_changes(state, &task.id, &changes).await?)
    };
    let project = changes
        .project_id
        .clone()
        .unwrap_or_else(|| task.project_id.clone());
    let launch = changes
        .launch
        .clone()
        .map(|asked| checked_launch.unwrap_or(asked));
    let launch_agent = match &launch {
        Some(launch) => launch.agent,
        None => task.launch.agent,
    };
    let write = Write {
        tool: "update_scheduled_task",
        action: SessionRequestAction::UpdateScheduledTask,
        place: Write::place(caller, launch_agent, task.console_id.clone(), project),
        asked: ScheduledTaskAsked {
            task: Some(task.id.clone()),
            launch,
            schedule: changes.schedule.clone(),
            enabled: None,
        },
        prompt: changes.prompt.clone(),
    };
    let (state_for_run, caller_id, id) = (state.clone(), caller.id.clone(), task.id.clone());
    submit(state, caller, automatic, write, async move {
        // The approval can come minutes later: the task must still be within the caller's reach.
        Scope::task_now(&state_for_run, &caller_id, &id)?;
        let task = saved(update(&state_for_run, None, &id, changes).await?)?;
        Ok(json!({
            "note": format!("Changed the scheduled task `{}`. {}", task.id, next_run_note(&task)),
            "task": describe_task(&task),
        }))
    })
    .await
}

pub(in crate::mcp) fn set_scheduled_task_enabled<'a>(
    state: &'a Arc<AppState>,
    caller: &'a Session,
    arguments: &'a Map<String, Value>,
) -> CallFuture<'a> {
    Box::pin(set_scheduled_task_enabled_to_the_end(
        state, caller, arguments,
    ))
}

async fn set_scheduled_task_enabled_to_the_end(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let task = Scope::of(caller)?.task(state, required_str(arguments, "task")?)?;
    let enabled =
        optional_bool(arguments, "enabled")?.ok_or_else(|| anyhow!("`enabled` is required"))?;
    let automatic = state.store.auto_approve_scheduled_task_changes()?;
    let write = Write {
        tool: "set_scheduled_task_enabled",
        action: SessionRequestAction::SetScheduledTaskEnabled,
        place: Write::place(
            caller,
            task.launch.agent,
            task.console_id.clone(),
            task.project_id.clone(),
        ),
        asked: ScheduledTaskAsked {
            task: Some(task.id.clone()),
            enabled: Some(enabled),
            ..Default::default()
        },
        prompt: None,
    };
    let (state_for_run, caller_id, id) = (state.clone(), caller.id.clone(), task.id.clone());
    submit(state, caller, automatic, write, async move {
        Scope::task_now(&state_for_run, &caller_id, &id)?;
        let task = saved(enable(&state_for_run, None, &id, enabled)?)?;
        Ok(json!({
            "note": format!(
                "Turned the scheduled task `{}` {}. {}",
                task.id,
                if enabled { "on" } else { "off" },
                next_run_note(&task)
            ),
            "task": describe_task(&task),
        }))
    })
    .await
}

pub(in crate::mcp) fn delete_scheduled_task<'a>(
    state: &'a Arc<AppState>,
    caller: &'a Session,
    arguments: &'a Map<String, Value>,
) -> CallFuture<'a> {
    Box::pin(delete_scheduled_task_to_the_end(state, caller, arguments))
}

async fn delete_scheduled_task_to_the_end(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let task = Scope::of(caller)?.task(state, required_str(arguments, "task")?)?;
    let automatic = state.store.auto_approve_scheduled_task_changes()?;
    let write = Write {
        tool: "delete_scheduled_task",
        action: SessionRequestAction::DeleteScheduledTask,
        place: Write::place(
            caller,
            task.launch.agent,
            task.console_id.clone(),
            task.project_id.clone(),
        ),
        asked: ScheduledTaskAsked {
            task: Some(task.id.clone()),
            ..Default::default()
        },
        prompt: None,
    };
    let (state_for_run, caller_id, id) = (state.clone(), caller.id.clone(), task.id.clone());
    submit(state, caller, automatic, write, async move {
        Scope::task_now(&state_for_run, &caller_id, &id)?;
        delete(&state_for_run, &id)?;
        Ok(json!({
            "deleted": id,
            "note": format!(
                "Deleted the scheduled task `{id}` and its recorded runs; the sessions those runs \
                 started are not affected."
            ),
        }))
    })
    .await
}

pub(in crate::mcp) fn run_scheduled_task<'a>(
    state: &'a Arc<AppState>,
    caller: &'a Session,
    arguments: &'a Map<String, Value>,
) -> CallFuture<'a> {
    Box::pin(run_scheduled_task_to_the_end(state, caller, arguments))
}

async fn run_scheduled_task_to_the_end(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let task = Scope::of(caller)?.task(state, required_str(arguments, "task")?)?;
    let automatic = state.store.auto_approve_scheduled_task_changes()?;
    if !automatic {
        check_run_scheduled_task(state, &task.id)?;
    }
    let write = Write {
        tool: "run_scheduled_task",
        action: SessionRequestAction::RunScheduledTask,
        place: Write::place(
            caller,
            task.launch.agent,
            task.console_id.clone(),
            task.project_id.clone(),
        ),
        asked: ScheduledTaskAsked {
            task: Some(task.id.clone()),
            ..Default::default()
        },
        prompt: None,
    };
    let (state_for_run, caller_id, id) = (state.clone(), caller.id.clone(), task.id.clone());
    submit(state, caller, automatic, write, async move {
        Scope::task_now(&state_for_run, &caller_id, &id)?;
        run(&state_for_run, &id)?;
        Ok(json!({
            "task": id,
            "note": format!(
                "Started a run of the scheduled task `{id}` in the background, whatever its \
                 schedule and whether it is on; its schedule is left alone. \
                 `list_scheduled_task_runs` shows the run once its launch has settled, with the \
                 session it started or why none started."
            ),
        }))
    })
    .await
}

#[cfg(test)]
mod tests;
