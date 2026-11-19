//! Scheduled tasks: a session Octoboard starts on a schedule, with a prompt as its opening task. The
//! record, its schedule and its launch settings, which the requests that create and change a task
//! carry too. See "Scheduled tasks" in `apps/daemon/PROTOCOL.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::agents::AgentId;
use super::fields::present_option;

/// A scheduled task as the daemon stores it. A console session of `console_id` is started when
/// `project_id` is null, else a project session of that project.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub console_id: String,
    pub project_id: Option<String>,
    /// What the session is started with, as the session dialog's New conversation offers it.
    pub launch: ScheduledTaskLaunch,
    /// The opening task the started session is handed; never blank.
    pub prompt: String,
    pub schedule: Schedule,
    /// A disabled task does not run on its schedule.
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// When the task next runs on its schedule; null for a manual-only task, a one-off whose time
    /// has passed, or a disabled task.
    pub next_run_at: Option<i64>,
    /// When the task last ran; null until it first does.
    pub last_run_at: Option<i64>,
    /// The sessions its kept runs started (runs whose outcome is `started`), most recent run first,
    /// each once, leaving out a session the daemon no longer holds or that is archived. Not
    /// stored: read from the runs and the sessions each time the task is sent, so every
    /// `scheduled_task_upserted` that follows a recorded run carries the session that run started.
    /// A session archived or deleted after the task was sent may still be listed until the task is
    /// next sent.
    pub run_sessions: Vec<String>,
}

/// The launch settings of a scheduled task's session, with the rules `open_session` gives the same
/// fields: each is checked as `open_session` checks it when the task is saved, and read again at
/// every run.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScheduledTaskLaunch {
    /// The started session's title; absent or blank names it as `open_session` does by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub title: Option<String>,
    /// Absent takes the project's default agent, else the console's, at each run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub agent: Option<AgentId>,
    /// The account, by id. Absent leaves the choice to the project's reference, else the
    /// console's, at each run; an explicit `null` chooses the default account.
    #[serde(
        default,
        deserialize_with = "present_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(test, ts(optional), schemars(transform = super::schema::nullable))]
    pub account: Option<Option<String>>,
    /// Absent inherits the project's (or console's) setting at each run; an explicit `null`
    /// leaves the choice to the agent; a value pins the model.
    #[serde(
        default,
        deserialize_with = "present_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(test, ts(optional), schemars(transform = super::schema::nullable))]
    pub model: Option<Option<String>>,
    /// The reasoning effort, with the rule of `model`.
    #[serde(
        default,
        deserialize_with = "present_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(test, ts(optional), schemars(transform = super::schema::nullable))]
    pub effort: Option<Option<String>>,
    /// The compatibility switches the session holds its own value for, by setting id; one left
    /// out follows the project's, else the console's, else the agent's value. Absent in a request
    /// holds none.
    #[serde(default)]
    pub compatibility: BTreeMap<String, bool>,
    /// The console session a project task's session reports to; absent for none. Ignored for a
    /// console task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub bound_to: Option<String>,
}

/// When a scheduled task runs, in the local time zone of the daemon's machine.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Once, at the instant `run_at`.
    Once { run_at: i64 },
    /// Every hour, at `minute` past it (0 to 59).
    Hourly { minute: u8 },
    /// Every day, at `time`.
    Daily { time: TimeOfDay },
    /// Monday to Friday, at `time`.
    Weekdays { time: TimeOfDay },
    /// On each of `days`, at `time`; `days` is not empty.
    Weekly { time: TimeOfDay, days: Vec<Weekday> },
    /// On day `day` of every month (1 to 31), at `time`. A month without that day runs on its last
    /// day, so 31 is the last day of every month.
    Monthly { day: u8, time: TimeOfDay },
    /// Only when run by hand.
    Manual,
}

/// A wall-clock time of day: `hour` 0 to 23, `minute` 0 to 59.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

/// One run of a scheduled task: what started it and what came of it. The daemon keeps the last
/// [`SCHEDULED_TASK_RUNS_KEPT`] of each task, and drops them with the task.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScheduledTaskRun {
    pub id: String,
    pub task_id: String,
    /// When the run started its session, or failed to.
    pub ran_at: i64,
    /// The occurrence of the schedule the run stands for; null for `run_now`. A catch-up's is the
    /// missed occurrence it makes up for, earlier than `ran_at`.
    pub due_at: Option<i64>,
    pub trigger: ScheduledRunTrigger,
    pub outcome: ScheduledRunOutcome,
}

/// How many runs of each task the daemon keeps, the most recent ones.
pub const SCHEDULED_TASK_RUNS_KEPT: usize = 50;

/// What started a run.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledRunTrigger {
    /// The schedule came due while the daemon was running.
    Scheduled,
    /// An occurrence missed while the daemon was not running (or the machine was asleep), made up
    /// once.
    CatchUp,
    /// `run_scheduled_task`.
    RunNow,
}

/// What came of a run.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduledRunOutcome {
    /// The session `session` was started and handed the prompt. `started_independent` is true
    /// when the task names a console session to report to that could not take sessions by then
    /// (archived, deleted or still starting), so the session was started reporting to none.
    Started {
        session: String,
        started_independent: bool,
    },
    /// No session was started: the refusal or launch failure, as an `error` reply would carry it
    /// (`code`, `params`, and `message`, its English account with the raw detail).
    Failed {
        code: String,
        params: super::Params,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Params, RequestBody};
    use serde_json::json;

    /// Every kind of schedule has the wire shape the clients write: its kind tag, and the fields that
    /// kind needs, with the days named in lower case.
    #[test]
    fn each_schedule_has_its_wire_shape() {
        let cases = [
            (
                Schedule::Once {
                    run_at: 1_800_000_000_000,
                },
                json!({"kind": "once", "run_at": 1_800_000_000_000_i64}),
            ),
            (
                Schedule::Hourly { minute: 15 },
                json!({"kind": "hourly", "minute": 15}),
            ),
            (
                Schedule::Daily {
                    time: TimeOfDay { hour: 9, minute: 5 },
                },
                json!({"kind": "daily", "time": {"hour": 9, "minute": 5}}),
            ),
            (
                Schedule::Weekdays {
                    time: TimeOfDay {
                        hour: 18,
                        minute: 0,
                    },
                },
                json!({"kind": "weekdays", "time": {"hour": 18, "minute": 0}}),
            ),
            (
                Schedule::Weekly {
                    time: TimeOfDay {
                        hour: 7,
                        minute: 30,
                    },
                    days: vec![Weekday::Monday, Weekday::Sunday],
                },
                json!({"kind": "weekly", "time": {"hour": 7, "minute": 30}, "days": ["monday", "sunday"]}),
            ),
            (
                Schedule::Monthly {
                    day: 31,
                    time: TimeOfDay { hour: 9, minute: 0 },
                },
                json!({"kind": "monthly", "day": 31, "time": {"hour": 9, "minute": 0}}),
            ),
            (Schedule::Manual, json!({"kind": "manual"})),
        ];
        for (schedule, wire) in cases {
            assert_eq!(
                serde_json::to_value(&schedule).unwrap(),
                wire,
                "{schedule:?}"
            );
            assert_eq!(serde_json::from_value::<Schedule>(wire).unwrap(), schedule);
        }
    }

    /// The days of the week sort in week order, Monday first, which the daemon stores a weekly
    /// schedule's days by.
    #[test]
    fn the_days_of_the_week_sort_monday_first() {
        let mut days = vec![
            Weekday::Sunday,
            Weekday::Wednesday,
            Weekday::Monday,
            Weekday::Saturday,
        ];
        days.sort();
        assert_eq!(
            days,
            [
                Weekday::Monday,
                Weekday::Wednesday,
                Weekday::Saturday,
                Weekday::Sunday
            ]
        );
    }

    /// A launch setting left out, set to `null` and set to a value are three different things on
    /// the wire, and each is written back as it was read.
    #[test]
    fn a_launch_tells_an_absent_setting_from_a_null_one() {
        let absent: ScheduledTaskLaunch = serde_json::from_value(json!({})).unwrap();
        assert_eq!(absent, ScheduledTaskLaunch::default());
        assert_eq!(
            serde_json::to_value(&absent).unwrap(),
            json!({"compatibility": {}})
        );

        let wire = json!({
            "title": "Nightly",
            "agent": "claude",
            "account": null,
            "model": "opus",
            "effort": null,
            "compatibility": {"skip_permissions": true},
            "bound_to": "hub",
        });
        let launch: ScheduledTaskLaunch = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(launch.title.as_deref(), Some("Nightly"));
        assert_eq!(launch.agent, Some(AgentId::CLAUDE));
        assert_eq!(launch.account, Some(None));
        assert_eq!(launch.model, Some(Some("opus".to_string())));
        assert_eq!(launch.effort, Some(None));
        assert_eq!(
            launch.compatibility,
            [("skip_permissions".to_string(), true)].into()
        );
        assert_eq!(launch.bound_to.as_deref(), Some("hub"));
        assert_eq!(serde_json::to_value(&launch).unwrap(), wire);

        let named: ScheduledTaskLaunch =
            serde_json::from_value(json!({"account": "work"})).unwrap();
        assert_eq!(named.account, Some(Some("work".to_string())));
    }

    /// The requests of the operations read what a client sends: a create without a project or
    /// launch settings, and an update that names only what it changes.
    #[test]
    fn the_requests_read_what_a_client_sends() {
        let create: RequestBody = serde_json::from_value(json!({
            "type": "create_scheduled_task",
            "console_id": "c-1",
            "prompt": "Check the build",
            "schedule": {"kind": "manual"},
        }))
        .unwrap();
        assert!(matches!(
            create,
            RequestBody::CreateScheduledTask { console_id, project_id: None, launch: None, prompt, schedule: Schedule::Manual, enabled: true }
                if console_id == "c-1" && prompt == "Check the build"
        ));

        let update: RequestBody = serde_json::from_value(json!({
            "type": "update_scheduled_task",
            "task": "t-1",
            "prompt": "Another",
        }))
        .unwrap();
        assert!(matches!(
            update,
            RequestBody::UpdateScheduledTask { task, console_id: None, project_id: None, launch: None, prompt: Some(prompt), schedule: None }
                if task == "t-1" && prompt == "Another"
        ));

        let enabled: RequestBody = serde_json::from_value(
            json!({"type": "set_scheduled_task_enabled", "task": "t-1", "enabled": false}),
        )
        .unwrap();
        assert!(matches!(
            enabled,
            RequestBody::SetScheduledTaskEnabled { enabled: false, .. }
        ));
        for kind in [
            "delete_scheduled_task",
            "run_scheduled_task",
            "list_scheduled_task_runs",
        ] {
            serde_json::from_value::<RequestBody>(json!({"type": kind, "task": "t-1"})).unwrap();
        }
        serde_json::from_value::<RequestBody>(json!({"type": "list_scheduled_tasks"})).unwrap();
    }

    /// An update's `project_id` tells a missing key (leave the project) from `null` (make a console
    /// task) from a project id, and a create can be asked to start disabled.
    #[test]
    fn an_updates_project_id_is_tri_state_and_a_create_can_start_disabled() {
        let project_id = |wire: serde_json::Value| {
            let mut request = json!({"type": "update_scheduled_task", "task": "t-1"});
            request
                .as_object_mut()
                .unwrap()
                .extend(wire.as_object().unwrap().clone());
            match serde_json::from_value::<RequestBody>(request).unwrap() {
                RequestBody::UpdateScheduledTask { project_id, .. } => project_id,
                other => panic!("not an update: {other:?}"),
            }
        };
        assert_eq!(project_id(json!({})), None);
        assert_eq!(project_id(json!({"project_id": null})), Some(None));
        assert_eq!(
            project_id(json!({"project_id": "p-1"})),
            Some(Some("p-1".to_string()))
        );

        let create: RequestBody = serde_json::from_value(json!({
            "type": "create_scheduled_task",
            "console_id": "c-1",
            "prompt": "Check the build",
            "schedule": {"kind": "manual"},
            "enabled": false,
        }))
        .unwrap();
        assert!(matches!(
            create,
            RequestBody::CreateScheduledTask { enabled: false, .. }
        ));
    }

    /// A run is written with its trigger and outcome tagged as the clients read them.
    #[test]
    fn a_run_has_its_wire_shape() {
        let started = ScheduledTaskRun {
            id: "r-1".to_string(),
            task_id: "t-1".to_string(),
            ran_at: 20,
            due_at: Some(10),
            trigger: ScheduledRunTrigger::CatchUp,
            outcome: ScheduledRunOutcome::Started {
                session: "s-1".to_string(),
                started_independent: true,
            },
        };
        assert_eq!(
            serde_json::to_value(&started).unwrap(),
            json!({
                "id": "r-1", "task_id": "t-1", "ran_at": 20, "due_at": 10, "trigger": "catch_up",
                "outcome": {"kind": "started", "session": "s-1", "started_independent": true},
            })
        );
        let failed = ScheduledTaskRun {
            due_at: None,
            trigger: ScheduledRunTrigger::RunNow,
            outcome: ScheduledRunOutcome::Failed {
                code: "account_disabled".to_string(),
                params: Params::from([("account".to_string(), "Work".to_string())]),
                message: "The account is disabled".to_string(),
            },
            ..started
        };
        assert_eq!(
            serde_json::to_value(&failed).unwrap(),
            json!({
                "id": "r-1", "task_id": "t-1", "ran_at": 20, "due_at": null, "trigger": "run_now",
                "outcome": {"kind": "failed", "code": "account_disabled", "params": {"account": "Work"}, "message": "The account is disabled"},
            })
        );
        assert_eq!(json!(ScheduledRunTrigger::Scheduled), json!("scheduled"));
    }
}
