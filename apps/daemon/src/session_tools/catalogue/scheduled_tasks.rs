//! The tools that read and change scheduled tasks, for the sessions that may: console sessions
//! (their console's tasks) and project coordinators (their own project's).

use serde_json::{json, Value};

use super::schemas::{agent_enum_or_null, object_schema, NAMING_LANGUAGE};
use super::ToolDef;

/// What every one of these tools says of what a task is and whose it is.
macro_rules! about_tasks {
    () => {
        "A scheduled task starts a session on a schedule and hands it the task's `prompt` as its \
         opening task; schedules are read in the daemon machine's local time zone. The sessions \
         a task starts are the user's own: they report to the user, not to you, and you and they \
         can only exchange information (`inform_session`) — unless the task reports to a console \
         session (`launch.bound_to`), which then owns them. Tasks belong to the user: create, \
         change, turn on or off, delete or run one only when the user asked you to."
    };
}

/// What every tool that writes says of the user's approval.
macro_rules! asks_first {
    () => {
        "The user is asked first, and the call waits for their answer, for several minutes at \
         most — unless the user has chosen to have scheduled task changes approved \
         automatically, in which case the call does not wait. A refusal, or no answer in time, \
         comes back as an error and changes nothing. What could never be carried out is refused \
         before the user is asked."
    };
}

/// What a task is shown as, in the reads and in the writes' `task`.
macro_rules! task_fields {
    () => {
        "a task is shown with its `id`, `console`, `project` (null for a console task), `launch` \
         (the launch settings as stored: `title`, `agent`, `account` as an account's id, `model`, \
         `effort`, `compatibility` and `bound_to`; one left out follows the project's, else the \
         console's, setting at each run, and `null` for `account`, `model` or `effort` is the \
         default account, the default model and the agent's own effort), `prompt`, `schedule`, \
         `enabled`, `next_run_at` (epoch milliseconds; null for a manual-only task, a one-off \
         whose time has passed or a task that is off), `last_run_at` (null until it first runs), \
         `created_at`, `updated_at` and `run_sessions` (the sessions its kept runs started, most \
         recent run first, leaving out archived ones)"
    };
}

/// The schema of a task's `launch` argument.
fn launch_schema() -> Value {
    json!({
        "type": "object",
        "description": "The launch settings of the sessions the task starts, as `open_session` \
                        takes them (`list_agent_models` lists the models and efforts an agent \
                        offers); each is checked as `open_session` checks it when the task is \
                        saved. A setting left out follows the project's, else the console's, \
                        setting at each run. On an update it replaces the task's launch settings \
                        whole, so name every setting to keep.",
        "properties": {
            "title": {
                "type": "string",
                "description": format!(
                    "The started session's name in Octoboard's session list. Left out or blank, \
                     it is named as `open_session` names one by default. {NAMING_LANGUAGE}"
                ),
            },
            "agent": {
                "type": ["string", "null"],
                "enum": agent_enum_or_null(),
                "description": "The agent the session runs. Left out or null, it is the \
                                project's default agent, else the console's, at each run. Pass \
                                one only when the user asked for it.",
            },
            "account": {
                "type": ["string", "null"],
                "description": "The account the session runs on: an account's id or name (see \
                                `get_settings`), or `default` or null for the agent's default \
                                account. It must belong to `agent`; without an `agent`, a name \
                                that fits accounts of several agents is refused. Left out, the \
                                project's reference decides, else the console's, at each run. \
                                Pass one only when the user asked for it.",
            },
            "model": {
                "type": ["string", "null"],
                "description": "The model of the session; blank or null is the default model \
                                (`list_agent_models` names it, and lists the ones the agent \
                                offers). Left out, it follows the project's setting, else the \
                                console's.",
            },
            "effort": {
                "type": ["string", "null"],
                "description": "The reasoning effort of the session; blank or null is the \
                                agent's own default. `list_agent_models` lists the levels each \
                                model offers. Left out, it follows the project's setting, else \
                                the console's.",
            },
            "compatibility": {
                "type": "object",
                "description": "The compatibility switches the session holds its own value for, \
                                by setting id (`get_settings` lists each agent's switches): \
                                `true` or `false` holds that value; one left out follows the \
                                project's, else the console's, else the application-wide value.",
                "additionalProperties": { "type": "boolean" },
            },
            "bound_to": {
                "type": "string",
                "description": "For a project task, the id of a console session of the task's \
                                console that the started sessions report to, which then owns \
                                them. Left out, they report to the user. Ignored for a console \
                                task.",
            },
        },
    })
}

/// The schema of a task's `schedule` argument.
fn schedule_schema() -> Value {
    let time = json!({
        "type": "object",
        "properties": {
            "hour": { "type": "integer", "minimum": 0, "maximum": 23 },
            "minute": { "type": "integer", "minimum": 0, "maximum": 59 },
        },
        "required": ["hour", "minute"],
    });
    json!({
        "type": "object",
        "description": "When the task runs, in the daemon machine's local time zone. `kind` is \
                        `once` (at `run_at`, epoch milliseconds, which must be in the future), \
                        `hourly` (every hour, at `minute` past it), `daily` (every day at \
                        `time`), `weekdays` (Monday to Friday at `time`), `weekly` (on each of \
                        `days` at `time`; `days` is not empty), `monthly` (on day `day` of every \
                        month, 1 to 31, at `time`; a month without that day runs on its last day) \
                        or `manual` (only when run by hand). `time` is `{hour, minute}`, an \
                        hour 0 to 23 and a minute 0 to 59.",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["once", "hourly", "daily", "weekdays", "weekly", "monthly", "manual"],
            },
            "run_at": { "type": "integer" },
            "minute": { "type": "integer", "minimum": 0, "maximum": 59 },
            "time": time,
            "days": {
                "type": "array",
                "items": {
                    "type": "string",
                    "enum": [
                        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday",
                        "sunday",
                    ],
                },
            },
            "day": { "type": "integer", "minimum": 1, "maximum": 31 },
        },
        "required": ["kind"],
    })
}

/// The `project` argument of the tools of a console session, which names one among its console's.
fn console_project(what: &str) -> Value {
    json!({
        "type": ["string", "null"],
        "description": what,
    })
}

/// The `task` argument every tool about one task takes.
fn task_schema() -> Value {
    json!({
        "type": "string",
        "description": "The task's id, as `list_scheduled_tasks` gives it.",
    })
}

/// `list_scheduled_tasks`, for a console session and for a project coordinator alike: what it
/// returns is the tasks within the caller's reach.
pub(super) const LIST_SCHEDULED_TASKS: ToolDef = ToolDef {
    name: "list_scheduled_tasks",
    description: concat!(
        "List the scheduled tasks within your reach: for a console session, the tasks of this \
         console, console tasks and those of its projects; for a project session, the tasks of \
         your own project. ",
        about_tasks!(),
        " Returns `tasks`, oldest first, where ",
        task_fields!(),
        "; and `time_zone`, the IANA name of the zone the schedules are read in (null when it has \
         none). `list_scheduled_task_runs` gives a task's runs."
    ),
    schema: || object_schema(json!({}), &[]),
};

pub(super) const LIST_SCHEDULED_TASK_RUNS: ToolDef = ToolDef {
    name: "list_scheduled_task_runs",
    description: "List a scheduled task's kept runs, most recent first, for a task within your \
                  reach (`list_scheduled_tasks`). Returns `task` and `runs`, each with its `id`, \
                  `task_id`, `ran_at` (epoch milliseconds), `due_at` (the occurrence of the \
                  schedule it stands for; null for a run by hand), `trigger` (`scheduled`, \
                  `catch_up` for an occurrence missed while the daemon was not running, made up \
                  once, or `run_now`) and `outcome`: `{kind: \"started\", session, \
                  started_independent}` for a run that started the session `session` \
                  (`started_independent` is true when the task names a console session to report \
                  to that could not take sessions by then, so the session reports to none), or \
                  `{kind: \"failed\", code, params, message}` for one that started none.",
    schema: || object_schema(json!({ "task": task_schema() }), &["task"]),
};

/// `create_scheduled_task` as a console session has it: a console task, or a task of any of the
/// console's projects.
pub(super) const CONSOLE_CREATE_SCHEDULED_TASK: ToolDef = ToolDef {
    name: "create_scheduled_task",
    description: concat!(
        "Create a scheduled task in this console: a console task, which starts a console session, \
         or a task of one of this console's projects, which starts a project session. ",
        about_tasks!(),
        " ",
        asks_first!(),
        " Returns `task` (",
        task_fields!(),
        ") and a `note`."
    ),
    schema: || create_schema(true),
};

/// `create_scheduled_task` as a project coordinator has it: a task of its own project.
pub(super) const PROJECT_CREATE_SCHEDULED_TASK: ToolDef = ToolDef {
    name: "create_scheduled_task",
    description: concat!(
        "Create a scheduled task of this project, which starts a project session of it. You \
         cannot create a task for another project or for the console. ",
        about_tasks!(),
        " ",
        asks_first!(),
        " Returns `task` (",
        task_fields!(),
        ") and a `note`."
    ),
    schema: || create_schema(false),
};

fn create_schema(with_project: bool) -> Value {
    let mut properties = json!({
        "prompt": {
            "type": "string",
            "description": "The opening task the started session is handed; not blank.",
        },
        "schedule": schedule_schema(),
        "launch": launch_schema(),
        "enabled": {
            "type": "boolean",
            "description": "Whether the task runs on its schedule. Left out, true. A task \
                            created off has no next run until it is turned on.",
        },
    });
    if with_project {
        properties["project"] = console_project(
            "The project's id, or its name when that is unambiguous, for a project task. Left \
             out or null, it is a console task, which starts a console session.",
        );
    }
    object_schema(properties, &["prompt", "schedule"])
}

/// `update_scheduled_task` as a console session has it.
pub(super) const CONSOLE_UPDATE_SCHEDULED_TASK: ToolDef = ToolDef {
    name: "update_scheduled_task",
    description: concat!(
        "Change a scheduled task of this console. Name only what changes: `project` moves the \
         task to another project of this console, or to the console with null; `launch` replaces \
         its launch settings whole; `prompt` and `schedule` replace theirs. Every field of the \
         result is checked, not only the changed ones. Turning the task on or off is \
         `set_scheduled_task_enabled`. ",
        about_tasks!(),
        " ",
        asks_first!(),
        " Returns `task` (",
        task_fields!(),
        ") and a `note`."
    ),
    schema: || update_schema(true),
};

/// `update_scheduled_task` as a project coordinator has it: a task of its own project, which stays
/// of that project.
pub(super) const PROJECT_UPDATE_SCHEDULED_TASK: ToolDef = ToolDef {
    name: "update_scheduled_task",
    description: concat!(
        "Change a scheduled task of this project. Name only what changes: `launch` replaces its \
         launch settings whole; `prompt` and `schedule` replace theirs. The task stays a task of \
         this project: you cannot move it to another project or to the console. Every field of \
         the result is checked, not only the changed ones. Turning the task on or off is \
         `set_scheduled_task_enabled`. ",
        about_tasks!(),
        " ",
        asks_first!(),
        " Returns `task` (",
        task_fields!(),
        ") and a `note`."
    ),
    schema: || update_schema(false),
};

fn update_schema(with_project: bool) -> Value {
    let mut properties = json!({
        "task": task_schema(),
        "prompt": {
            "type": "string",
            "description": "The new opening task; not blank. Left out, it stays.",
        },
        "schedule": schedule_schema(),
        "launch": launch_schema(),
    });
    if with_project {
        properties["project"] = console_project(
            "The project's id, or its name when that is unambiguous, to move the task to; null \
             makes it a console task. Left out, the task stays where it is.",
        );
    }
    object_schema(properties, &["task"])
}

pub(super) const SET_SCHEDULED_TASK_ENABLED: ToolDef = ToolDef {
    name: "set_scheduled_task_enabled",
    description: concat!(
        "Turn a scheduled task within your reach on or off. An off task does not run on its \
         schedule and has no next run; turning one on computes its next run from now, and \
         turning on one that is already on leaves its next run alone. ",
        about_tasks!(),
        " ",
        asks_first!(),
        " Returns `task` (",
        task_fields!(),
        ") and a `note`."
    ),
    schema: || {
        object_schema(
            json!({
                "task": task_schema(),
                "enabled": { "type": "boolean", "description": "True to turn it on, false off." },
            }),
            &["task", "enabled"],
        )
    },
};

pub(super) const DELETE_SCHEDULED_TASK: ToolDef = ToolDef {
    name: "delete_scheduled_task",
    description: concat!(
        "Delete a scheduled task within your reach, with its recorded runs; the sessions those \
         runs started are not affected. ",
        about_tasks!(),
        " ",
        asks_first!(),
        " Returns `deleted`, the task's id, and a `note`."
    ),
    schema: || object_schema(json!({ "task": task_schema() }), &["task"]),
};

pub(super) const RUN_SCHEDULED_TASK: ToolDef = ToolDef {
    name: "run_scheduled_task",
    description: concat!(
        "Run a scheduled task within your reach once now, whatever its schedule and whether it is \
         on; its schedule is left alone. The run happens in the background: the call returns \
         once it has been started, and `list_scheduled_task_runs` shows the run when its launch \
         has settled, with the session it started or why none started. ",
        about_tasks!(),
        " ",
        asks_first!(),
        " Returns `task`, the task's id, and a `note`."
    ),
    schema: || object_schema(json!({ "task": task_schema() }), &["task"]),
};
