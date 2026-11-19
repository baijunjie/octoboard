//! The scheduled tasks table: inserting, changing, turning on and off, deleting and reading the
//! tasks; moving a task's next run as the scheduler takes an occurrence; and the runs table, each
//! task's last runs.

use std::collections::HashMap;

use super::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};

use super::rows::{compatibility_from_text, compatibility_to_text};
use super::Store;
use crate::protocol::{
    AgentId, CodedError, Params, Schedule, ScheduledRunOutcome, ScheduledRunTrigger, ScheduledTask,
    ScheduledTaskLaunch, ScheduledTaskRun, SCHEDULED_TASK_RUNS_KEPT,
};

const SCHEDULED_TASK_COLUMNS: &str = "id, console_id, project_id, title, agent, account_id, \
     account_set, model, model_set, effort, effort_set, compatibility, bound_to, prompt, schedule, \
     enabled, created_at, updated_at, next_run_at, last_run_at";

impl Store {
    /// Stores a new task. Refused with `unknown_account` when its launch settings name an account
    /// that is no longer there (deleted while the settings were being checked).
    pub fn insert_scheduled_task(&self, task: &ScheduledTask) -> Result<()> {
        let launch = LaunchColumns::of(&task.launch);
        let conn = self.lock();
        require_named_account(&conn, &task.launch)?;
        conn.execute(
            &format!(
                "INSERT INTO scheduled_tasks ({SCHEDULED_TASK_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                         ?17, ?18, ?19, ?20)"
            ),
            params![
                task.id,
                task.console_id,
                task.project_id,
                launch.title,
                launch.agent,
                launch.account_id,
                launch.account_set,
                launch.model,
                launch.model_set,
                launch.effort,
                launch.effort_set,
                launch.compatibility,
                launch.bound_to,
                task.prompt,
                serde_json::to_string(&task.schedule)?,
                task.enabled,
                task.created_at,
                task.updated_at,
                task.next_run_at,
                task.last_run_at,
            ],
        )?;
        Ok(())
    }

    /// The task, or `None` for an id no task has, or one this build cannot read (see
    /// [`Store::list_scheduled_tasks`]).
    pub fn get_scheduled_task(&self, id: &str) -> Result<Option<ScheduledTask>> {
        let conn = self.lock();
        scheduled_task_in(&conn, id)
    }

    pub fn require_scheduled_task(&self, id: &str) -> Result<ScheduledTask> {
        self.get_scheduled_task(id)?
            .ok_or_else(|| CodedError::unknown_scheduled_task(id).into())
    }

    /// Every task, oldest first (`id` breaking a tie of `created_at`). A task naming an agent this
    /// build does not register, or a schedule of a kind it does not know, is kept in the store and
    /// left out.
    pub fn list_scheduled_tasks(&self) -> Result<Vec<ScheduledTask>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SCHEDULED_TASK_COLUMNS} FROM scheduled_tasks ORDER BY created_at, id"
        ))?;
        let tasks = stmt
            .query_map([], read_scheduled_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut tasks: Vec<ScheduledTask> = tasks.into_iter().flatten().collect();
        let mut sessions = run_sessions_by_task(&conn)?;
        for task in &mut tasks {
            task.run_sessions = sessions.remove(&task.id).unwrap_or_default();
        }
        Ok(tasks)
    }

    /// Writes what an edit of `task` changes, an edit made from `read`, the task as the editor
    /// read it: its target, launch settings, prompt, schedule and `updated_at`, leaving `enabled`,
    /// `created_at` and `last_run_at`, which only turning the task on or off, creating it and
    /// running it write (`task`'s own `enabled`, `next_run_at`, `created_at` and `last_run_at` are
    /// ignored). Nothing is written, and [`ScheduledTaskEdit::Changed`] answered, when the stored
    /// definition is no longer `read`'s (another edit landed since), so no edit's field is lost
    /// to one made from a stale read; the editor reads again and merges its changes anew. The
    /// next run is decided in the same transaction from the task as it is stored now: none while it
    /// is disabled, the stored one when the schedule is unchanged (so an occurrence due but not yet
    /// taken is not dropped, and one the scheduler took meanwhile is not given again), else
    /// `next_run` of the new schedule. `updated_at` is stamped `max(task.updated_at, previous + 1)`, so it
    /// orders the task's changes whatever the clock does. Refused with `unknown_account` when the launch settings
    /// name an account that is no longer there.
    pub fn edit_scheduled_task(
        &self,
        task: &ScheduledTask,
        read: &ScheduledTask,
        next_run: impl FnOnce(&Schedule) -> Option<i64>,
    ) -> Result<ScheduledTaskEdit> {
        let launch = LaunchColumns::of(&task.launch);
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let Some(current) = scheduled_task_in(&tx, &task.id)? else {
            return Ok(ScheduledTaskEdit::Gone);
        };
        if !same_definition(&current, read) {
            return Ok(ScheduledTaskEdit::Changed);
        }
        require_named_account(&tx, &task.launch)?;
        let next_run_at = if !current.enabled {
            None
        } else if current.schedule == task.schedule {
            current.next_run_at
        } else {
            next_run(&task.schedule)
        };
        tx.execute(
            "UPDATE scheduled_tasks SET console_id = ?2, project_id = ?3, title = ?4, agent = ?5,
                 account_id = ?6, account_set = ?7, model = ?8, model_set = ?9, effort = ?10,
                 effort_set = ?11, compatibility = ?12, bound_to = ?13, prompt = ?14,
                 schedule = ?15, updated_at = MAX(?16, updated_at + 1), next_run_at = ?17
             WHERE id = ?1",
            params![
                task.id,
                task.console_id,
                task.project_id,
                launch.title,
                launch.agent,
                launch.account_id,
                launch.account_set,
                launch.model,
                launch.model_set,
                launch.effort,
                launch.effort_set,
                launch.compatibility,
                launch.bound_to,
                task.prompt,
                serde_json::to_string(&task.schedule)?,
                task.updated_at,
                next_run_at,
            ],
        )?;
        let edited = scheduled_task_in(&tx, &task.id)?;
        tx.commit()?;
        Ok(edited.map_or(ScheduledTaskEdit::Gone, |task| {
            ScheduledTaskEdit::Edited(Box::new(task))
        }))
    }

    /// Turns the task `id` on or off at `now`, stamping `updated_at` `max(now, previous + 1)`.
    /// Turning it off clears its next run; turning it on takes `next_run` of its stored schedule. A task already in the state asked for is left
    /// untouched, its next run included. Returns the task as it now is, or `None` when it was not
    /// there.
    pub fn set_scheduled_task_enabled(
        &self,
        id: &str,
        enabled: bool,
        now: i64,
        next_run: impl FnOnce(&Schedule) -> Option<i64>,
    ) -> Result<Option<ScheduledTask>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let Some(current) = scheduled_task_in(&tx, id)? else {
            return Ok(None);
        };
        if current.enabled == enabled {
            return Ok(Some(current));
        }
        let next_run_at = enabled.then(|| next_run(&current.schedule)).flatten();
        tx.execute(
            "UPDATE scheduled_tasks
             SET enabled = ?2, next_run_at = ?3, updated_at = MAX(?4, updated_at + 1)
             WHERE id = ?1",
            params![id, enabled, next_run_at, now],
        )?;
        let changed = scheduled_task_in(&tx, id)?;
        tx.commit()?;
        Ok(changed)
    }

    /// Deletes the task, with its runs; returns whether it was there.
    pub fn delete_scheduled_task(&self, id: &str) -> Result<bool> {
        let deleted = self
            .lock()
            .execute("DELETE FROM scheduled_tasks WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    /// Moves the next run of the enabled task `id` from `due` to `next`, as the scheduler takes the
    /// occurrence `due` (or skips past it); returns the task as it now is, or `None` when it was not
    /// there, is disabled or no longer comes due at `due` (an edit, or another taker, came first).
    pub fn take_scheduled_occurrence(
        &self,
        id: &str,
        due: i64,
        next: Option<i64>,
    ) -> Result<Option<ScheduledTask>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE scheduled_tasks SET next_run_at = ?3
             WHERE id = ?1 AND enabled = 1 AND next_run_at = ?2",
            params![id, due, next],
        )?;
        let task = if changed > 0 {
            scheduled_task_in(&tx, id)?
        } else {
            None
        };
        tx.commit()?;
        Ok(task)
    }

    /// Gives the occurrence `due` back to the task `id`, which [`Store::take_scheduled_occurrence`]
    /// moved on to `next`, so a daemon that starts later makes it up; returns the task as it now
    /// is, or `None` when it has changed since (an edit, a delete) and is left alone.
    pub fn give_back_scheduled_occurrence(
        &self,
        id: &str,
        due: i64,
        next: Option<i64>,
    ) -> Result<Option<ScheduledTask>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE scheduled_tasks SET next_run_at = ?2
             WHERE id = ?1 AND enabled = 1 AND next_run_at IS ?3",
            params![id, due, next],
        )?;
        let task = if changed > 0 {
            scheduled_task_in(&tx, id)?
        } else {
            None
        };
        tx.commit()?;
        Ok(task)
    }

    /// Records `run`, keeps the last [`SCHEDULED_TASK_RUNS_KEPT`] runs of its task, and moves the
    /// task's `last_run_at` to the run's time unless a later run's is there already; returns the
    /// task as it now is, or `None`, recording nothing, when the task is gone.
    pub fn record_scheduled_task_run(
        &self,
        run: &ScheduledTaskRun,
    ) -> Result<Option<ScheduledTask>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE scheduled_tasks
             SET last_run_at = MAX(COALESCE(last_run_at, ?2), ?2)
             WHERE id = ?1",
            params![run.task_id, run.ran_at],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let (session_id, started_independent, failure) = match &run.outcome {
            ScheduledRunOutcome::Started {
                session,
                started_independent,
            } => (Some(session.as_str()), *started_independent, None),
            ScheduledRunOutcome::Failed {
                code,
                params,
                message,
            } => (
                None,
                false,
                Some((code, serde_json::to_string(params)?, message)),
            ),
        };
        tx.execute(
            "INSERT INTO scheduled_task_runs (id, task_id, ran_at, due_at, trigger, session_id,
                 started_independent, failure_code, failure_params, failure_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run.id,
                run.task_id,
                run.ran_at,
                run.due_at,
                trigger_name(run.trigger),
                session_id,
                started_independent,
                failure.as_ref().map(|(code, _, _)| code.as_str()),
                failure.as_ref().map(|(_, params, _)| params.as_str()),
                failure.as_ref().map(|(_, _, message)| message.as_str()),
            ],
        )?;
        tx.execute(
            "DELETE FROM scheduled_task_runs
             WHERE task_id = ?1 AND seq NOT IN (
                 SELECT seq FROM scheduled_task_runs WHERE task_id = ?1
                 ORDER BY seq DESC LIMIT ?2
             )",
            params![run.task_id, SCHEDULED_TASK_RUNS_KEPT as i64],
        )?;
        let task = scheduled_task_in(&tx, &run.task_id)?;
        tx.commit()?;
        Ok(task)
    }

    /// The kept runs of the task `task_id`, most recent first; none for a task that is not there.
    /// A run of a trigger this build does not know is left out.
    pub fn list_scheduled_task_runs(&self, task_id: &str) -> Result<Vec<ScheduledTaskRun>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, ran_at, due_at, trigger, session_id, started_independent,
                 failure_code, failure_params, failure_message
             FROM scheduled_task_runs WHERE task_id = ?1 ORDER BY seq DESC",
        )?;
        let runs = stmt
            .query_map(params![task_id], read_scheduled_task_run)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(runs.into_iter().flatten().collect())
    }
}

/// What [`Store::edit_scheduled_task`] did.
#[derive(Debug, PartialEq)]
pub enum ScheduledTaskEdit {
    /// Written; the task as it now is.
    Edited(Box<ScheduledTask>),
    /// The task was not there.
    Gone,
    /// Not written: the stored definition is not the one the edit was made from.
    Changed,
}

/// Whether `a` and `b` have the same definition: what an edit writes, as against what the
/// scheduler and turning a task on or off move.
fn same_definition(a: &ScheduledTask, b: &ScheduledTask) -> bool {
    a.console_id == b.console_id
        && a.project_id == b.project_id
        && a.launch == b.launch
        && a.prompt == b.prompt
        && a.schedule == b.schedule
}

/// The task `id` as `conn` reads it, with its `run_sessions`.
pub(super) fn scheduled_task_in(conn: &Connection, id: &str) -> Result<Option<ScheduledTask>> {
    let task = conn
        .query_row(
            &format!("SELECT {SCHEDULED_TASK_COLUMNS} FROM scheduled_tasks WHERE id = ?1"),
            params![id],
            read_scheduled_task,
        )
        .optional()?;
    let Some(mut task) = task.flatten() else {
        return Ok(None);
    };
    task.run_sessions = run_sessions_of(conn, id)?;
    Ok(Some(task))
}

/// Refuses, as `unknown_account`, `launch` naming an account the `accounts` table does not have.
/// `Store::delete_account` clears the accounts it deletes from the tasks, so a task that names one
/// that is gone was saved from a check made before the deletion.
fn require_named_account(conn: &Connection, launch: &ScheduledTaskLaunch) -> Result<()> {
    let Some(Some(account)) = &launch.account else {
        return Ok(());
    };
    let exists = conn
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(CodedError::unknown_account(account).into())
    }
}

/// The sessions the kept runs of the task `task_id` started: most recent run first, each once,
/// leaving out a session that is gone or archived. Uses the runs' task index.
fn run_sessions_of(conn: &Connection, task_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT r.session_id
         FROM scheduled_task_runs r JOIN sessions s ON s.id = r.session_id
         WHERE r.task_id = ?1 AND s.status != 'archived'
         ORDER BY r.seq DESC",
    )?;
    let rows = stmt
        .query_map(params![task_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(first_of_each(rows))
}

/// The sessions the kept runs started, for every task, as [`run_sessions_of`] reads them for one:
/// one query for the whole listing.
fn run_sessions_by_task(conn: &Connection) -> Result<HashMap<String, Vec<String>>> {
    let mut stmt = conn.prepare(
        "SELECT r.task_id, r.session_id
         FROM scheduled_task_runs r JOIN sessions s ON s.id = r.session_id
         WHERE s.status != 'archived'
         ORDER BY r.seq DESC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut by_task: HashMap<String, Vec<String>> = HashMap::new();
    for (task_id, session_id) in rows {
        by_task.entry(task_id).or_default().push(session_id);
    }
    Ok(by_task
        .into_iter()
        .map(|(task_id, sessions)| (task_id, first_of_each(sessions)))
        .collect())
}

/// `sessions` with each id kept at its first place only.
fn first_of_each(sessions: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    sessions
        .into_iter()
        .filter(|session| seen.insert(session.clone()))
        .collect()
}

fn trigger_name(trigger: ScheduledRunTrigger) -> &'static str {
    match trigger {
        ScheduledRunTrigger::Scheduled => "scheduled",
        ScheduledRunTrigger::CatchUp => "catch_up",
        ScheduledRunTrigger::RunNow => "run_now",
    }
}

fn trigger_of(name: &str) -> Option<ScheduledRunTrigger> {
    [
        ScheduledRunTrigger::Scheduled,
        ScheduledRunTrigger::CatchUp,
        ScheduledRunTrigger::RunNow,
    ]
    .into_iter()
    .find(|trigger| trigger_name(*trigger) == name)
}

/// A row of the runs table; `None` for a trigger this build does not know. A run with no session
/// is a failed one; failure columns that cannot be read are read as empty.
fn read_scheduled_task_run(row: &Row<'_>) -> rusqlite::Result<Option<ScheduledTaskRun>> {
    let Some(trigger) = trigger_of(&row.get::<_, String>(4)?) else {
        return Ok(None);
    };
    let outcome = match row.get::<_, Option<String>>(5)? {
        Some(session) => ScheduledRunOutcome::Started {
            session,
            started_independent: row.get(6)?,
        },
        None => ScheduledRunOutcome::Failed {
            code: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
            params: row
                .get::<_, Option<String>>(8)?
                .and_then(|text| serde_json::from_str::<Params>(&text).ok())
                .unwrap_or_default(),
            message: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
        },
    };
    Ok(Some(ScheduledTaskRun {
        id: row.get(0)?,
        task_id: row.get(1)?,
        ran_at: row.get(2)?,
        due_at: row.get(3)?,
        trigger,
        outcome,
    }))
}

/// The launch settings as their columns hold them: each tri-state value as a value and a flag
/// saying whether it is chosen.
struct LaunchColumns {
    title: Option<String>,
    agent: Option<&'static str>,
    account_id: Option<String>,
    account_set: bool,
    model: Option<String>,
    model_set: bool,
    effort: Option<String>,
    effort_set: bool,
    compatibility: Option<String>,
    bound_to: Option<String>,
}

impl LaunchColumns {
    fn of(launch: &ScheduledTaskLaunch) -> Self {
        Self {
            title: launch.title.clone(),
            agent: launch.agent.map(AgentId::wire_name),
            account_id: launch.account.clone().flatten(),
            account_set: launch.account.is_some(),
            model: launch.model.clone().flatten(),
            model_set: launch.model.is_some(),
            effort: launch.effort.clone().flatten(),
            effort_set: launch.effort.is_some(),
            compatibility: compatibility_to_text(&launch.compatibility),
            bound_to: launch.bound_to.clone(),
        }
    }
}

/// A row of `SCHEDULED_TASK_COLUMNS`; `None` for one naming an agent this build does not register
/// or holding a schedule it cannot read.
fn read_scheduled_task(row: &Row<'_>) -> rusqlite::Result<Option<ScheduledTask>> {
    let agent = match row.get::<_, Option<String>>(4)? {
        None => None,
        Some(id) => match AgentId::from_wire_name(&id) {
            Some(agent) => Some(agent),
            None => return Ok(None),
        },
    };
    let Ok(schedule) = serde_json::from_str::<Schedule>(&row.get::<_, String>(14)?) else {
        return Ok(None);
    };
    let chosen = |value: Option<String>, set: bool| set.then_some(value);
    Ok(Some(ScheduledTask {
        id: row.get(0)?,
        console_id: row.get(1)?,
        project_id: row.get(2)?,
        launch: ScheduledTaskLaunch {
            title: row.get(3)?,
            agent,
            account: chosen(row.get(5)?, row.get(6)?),
            model: chosen(row.get(7)?, row.get(8)?),
            effort: chosen(row.get(9)?, row.get(10)?),
            compatibility: compatibility_from_text(row.get(11)?),
            bound_to: row.get(12)?,
        },
        prompt: row.get(13)?,
        schedule,
        enabled: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        next_run_at: row.get(18)?,
        last_run_at: row.get(19)?,
        run_sessions: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{console, project, temp_db};
    use super::*;
    use crate::protocol::{TimeOfDay, Weekday};
    use std::collections::BTreeMap;

    fn task(id: &str, created_at: i64) -> ScheduledTask {
        ScheduledTask {
            id: id.to_string(),
            console_id: "console-1".to_string(),
            project_id: None,
            launch: ScheduledTaskLaunch::default(),
            prompt: "Check the build".to_string(),
            schedule: Schedule::Manual,
            enabled: true,
            created_at,
            updated_at: created_at,
            next_run_at: None,
            last_run_at: None,
            run_sessions: vec![],
        }
    }

    fn store_with_project(name: &str) -> (crate::test_support::ScratchFile, Store) {
        let path = temp_db(name);
        let store = Store::open(&path).expect("store");
        store.insert_console(&console(None, None, None)).unwrap();
        store.insert_project(&project(false)).unwrap();
        (path, store)
    }

    /// Every field comes back as written, each tri-state launch value telling "inherit" (absent)
    /// from "the default" (`null`) from a value.
    #[test]
    fn a_task_reads_back_as_written() {
        let (_path, store) = store_with_project("scheduled-roundtrip");
        let full = ScheduledTask {
            project_id: Some("project-1".to_string()),
            launch: ScheduledTaskLaunch {
                title: Some("Nightly".to_string()),
                agent: Some(AgentId::CODEX),
                account: Some(None),
                model: Some(Some("gpt-5".to_string())),
                effort: None,
                compatibility: BTreeMap::from([("setting".to_string(), true)]),
                bound_to: Some("session-9".to_string()),
            },
            schedule: Schedule::Weekly {
                time: TimeOfDay {
                    hour: 9,
                    minute: 30,
                },
                days: vec![Weekday::Monday, Weekday::Friday],
            },
            enabled: false,
            next_run_at: Some(5),
            last_run_at: Some(3),
            ..task("task-1", 1)
        };
        store.insert_scheduled_task(&full).unwrap();
        assert_eq!(store.get_scheduled_task("task-1").unwrap(), Some(full));

        let plain = task("task-2", 2);
        store.insert_scheduled_task(&plain).unwrap();
        assert_eq!(store.get_scheduled_task("task-2").unwrap(), Some(plain));
    }

    #[test]
    fn tasks_are_listed_oldest_first() {
        let (_path, store) = store_with_project("scheduled-order");
        for (id, at) in [("b", 2), ("c", 1), ("a", 2)] {
            store.insert_scheduled_task(&task(id, at)).unwrap();
        }
        let ids: Vec<String> = store
            .list_scheduled_tasks()
            .unwrap()
            .into_iter()
            .map(|task| task.id)
            .collect();
        assert_eq!(ids, ["c", "a", "b"]);
    }

    /// An edit writes the definition and leaves when the task was created and last ran, and
    /// whether it is enabled; a changed schedule takes the next run from the caller, an unchanged
    /// one keeps the stored next run, and a disabled task has none.
    #[test]
    fn an_edit_keeps_what_it_does_not_write_and_decides_the_next_run() {
        let (_path, store) = store_with_project("scheduled-update");
        store
            .insert_scheduled_task(&ScheduledTask {
                last_run_at: Some(7),
                next_run_at: Some(4),
                schedule: Schedule::Hourly { minute: 5 },
                ..task("task-1", 1)
            })
            .unwrap();
        let unchanged = ScheduledTask {
            prompt: "Another".to_string(),
            updated_at: 10,
            enabled: false,
            next_run_at: Some(99),
            created_at: 99,
            last_run_at: None,
            schedule: Schedule::Hourly { minute: 5 },
            ..task("task-1", 1)
        };
        let read = store.require_scheduled_task("task-1").unwrap();
        let ScheduledTaskEdit::Edited(stored) = store
            .edit_scheduled_task(&unchanged, &read, |_| panic!("the schedule is unchanged"))
            .unwrap()
        else {
            panic!("edited");
        };
        let stored = *stored;
        assert_eq!(
            stored,
            ScheduledTask {
                created_at: 1,
                last_run_at: Some(7),
                enabled: true,
                next_run_at: Some(4),
                ..unchanged.clone()
            }
        );

        let changed = ScheduledTask {
            schedule: Schedule::Hourly { minute: 6 },
            ..unchanged
        };
        let read = store.require_scheduled_task("task-1").unwrap();
        let ScheduledTaskEdit::Edited(stored) = store
            .edit_scheduled_task(&changed, &read, |_| Some(20))
            .unwrap()
        else {
            panic!("edited");
        };
        let stored = *stored;
        assert_eq!(stored.next_run_at, Some(20));

        store
            .set_scheduled_task_enabled("task-1", false, 30, |_| Some(40))
            .unwrap();
        let read = store.require_scheduled_task("task-1").unwrap();
        let ScheduledTaskEdit::Edited(stored) = store
            .edit_scheduled_task(
                &ScheduledTask {
                    schedule: Schedule::Hourly { minute: 6 },
                    ..task("task-1", 1)
                },
                &read,
                |_| Some(50),
            )
            .unwrap()
        else {
            panic!("edited");
        };
        let stored = *stored;
        assert!(!stored.enabled);
        assert_eq!(stored.next_run_at, None);
        assert_eq!(
            store
                .edit_scheduled_task(&task("missing", 1), &read, |_| None)
                .unwrap(),
            ScheduledTaskEdit::Gone
        );
    }

    /// An edit made from a read the stored definition has moved on from writes nothing; a move of
    /// the next run or of `enabled` is not such a move.
    #[test]
    fn an_edit_from_a_stale_read_writes_nothing() {
        let (_path, store) = store_with_project("scheduled-stale-edit");
        store.insert_scheduled_task(&task("task-1", 1)).unwrap();
        let read = store.require_scheduled_task("task-1").unwrap();
        store
            .set_scheduled_task_enabled("task-1", false, 5, |_| Some(6))
            .unwrap();
        store
            .set_scheduled_task_enabled("task-1", true, 7, |_| Some(8))
            .unwrap();
        let other = ScheduledTask {
            prompt: "Other".to_string(),
            ..task("task-1", 1)
        };
        assert!(matches!(
            store.edit_scheduled_task(&other, &read, |_| None).unwrap(),
            ScheduledTaskEdit::Edited(_)
        ));
        let mine = ScheduledTask {
            schedule: Schedule::Hourly { minute: 1 },
            ..task("task-1", 1)
        };
        assert_eq!(
            store.edit_scheduled_task(&mine, &read, |_| None).unwrap(),
            ScheduledTaskEdit::Changed
        );
        let stored = store.require_scheduled_task("task-1").unwrap();
        assert_eq!(stored.prompt, "Other");
        assert_eq!(stored.schedule, Schedule::Manual);
    }

    /// An occurrence the scheduler took between an edit's read and its write is not given again,
    /// and a task turned off in that gap stays off, with no next run.
    #[test]
    fn an_edit_keeps_what_the_scheduler_and_turning_off_did_since_its_read() {
        let (_path, store) = store_with_project("scheduled-edit-gap");
        let daily = Schedule::Daily {
            time: TimeOfDay { hour: 9, minute: 0 },
        };
        for id in ["taken", "off"] {
            store
                .insert_scheduled_task(&ScheduledTask {
                    schedule: daily.clone(),
                    next_run_at: Some(100),
                    ..task(id, 1)
                })
                .unwrap();
        }
        let edit_of = |id: &str| {
            let read = store.require_scheduled_task(id).unwrap();
            let edited = ScheduledTask {
                prompt: "Another".to_string(),
                updated_at: 50,
                ..read.clone()
            };
            (edited, read)
        };

        let (edited, read) = edit_of("taken");
        store
            .take_scheduled_occurrence("taken", 100, Some(200))
            .unwrap()
            .expect("taken");
        let ScheduledTaskEdit::Edited(stored) = store
            .edit_scheduled_task(&edited, &read, |_| panic!("the schedule is unchanged"))
            .unwrap()
        else {
            panic!("edited");
        };
        assert_eq!(
            (stored.prompt.as_str(), stored.next_run_at),
            ("Another", Some(200))
        );

        let (edited, read) = edit_of("off");
        store
            .set_scheduled_task_enabled("off", false, 60, |_| panic!("turned off"))
            .unwrap();
        let ScheduledTaskEdit::Edited(stored) = store
            .edit_scheduled_task(&edited, &read, |_| Some(300))
            .unwrap()
        else {
            panic!("edited");
        };
        assert_eq!(stored.prompt, "Another");
        assert!(!stored.enabled);
        assert_eq!(stored.next_run_at, None);
    }

    /// The scheduler takes an occurrence only from an enabled task that still comes due at it.
    #[test]
    fn an_occurrence_is_taken_only_while_it_is_the_tasks_next_run() {
        let (_path, store) = store_with_project("scheduled-take");
        let due = ScheduledTask {
            next_run_at: Some(100),
            ..task("task-1", 1)
        };
        store.insert_scheduled_task(&due).unwrap();

        assert_eq!(
            store
                .take_scheduled_occurrence("task-1", 99, Some(200))
                .unwrap(),
            None
        );
        let taken = store
            .take_scheduled_occurrence("task-1", 100, Some(200))
            .unwrap()
            .expect("taken");
        assert_eq!(taken.next_run_at, Some(200));
        // Taken once: a second taker of the same occurrence finds it gone.
        assert_eq!(
            store
                .take_scheduled_occurrence("task-1", 100, Some(300))
                .unwrap(),
            None
        );
        assert_eq!(
            store.require_scheduled_task("task-1").unwrap().next_run_at,
            Some(200)
        );

        store
            .set_scheduled_task_enabled("task-1", false, 5, |_| None)
            .unwrap();
        store
            .lock()
            .execute("UPDATE scheduled_tasks SET next_run_at = 200", [])
            .unwrap();
        assert_eq!(
            store
                .take_scheduled_occurrence("task-1", 200, None)
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .take_scheduled_occurrence("missing", 200, None)
                .unwrap(),
            None
        );
    }

    /// Turning on a task that is on, or off one that is off, changes nothing (its next run and
    /// `updated_at` included); turning it off clears the next run and turning it on again takes
    /// it from the stored schedule at the given time.
    #[test]
    fn turning_a_task_on_or_off_changes_it_only_when_its_state_changes() {
        let (_path, store) = store_with_project("scheduled-enabled");
        let on = ScheduledTask {
            schedule: Schedule::Hourly { minute: 5 },
            next_run_at: Some(4),
            ..task("task-1", 1)
        };
        store.insert_scheduled_task(&on).unwrap();

        let same = store
            .set_scheduled_task_enabled("task-1", true, 50, |_| panic!("already on"))
            .unwrap();
        assert_eq!(same, Some(on.clone()));
        assert_eq!(store.require_scheduled_task("task-1").unwrap(), on);

        let off = store
            .set_scheduled_task_enabled("task-1", false, 60, |_| panic!("no next run"))
            .unwrap()
            .expect("there");
        assert_eq!(
            off,
            ScheduledTask {
                enabled: false,
                next_run_at: None,
                updated_at: 60,
                ..on.clone()
            }
        );
        let same = store
            .set_scheduled_task_enabled("task-1", false, 70, |_| panic!("already off"))
            .unwrap();
        assert_eq!(same, Some(off.clone()));

        let again = store
            .set_scheduled_task_enabled("task-1", true, 80, |schedule| {
                assert_eq!(*schedule, Schedule::Hourly { minute: 5 });
                Some(90)
            })
            .unwrap()
            .expect("there");
        assert_eq!(
            again,
            ScheduledTask {
                enabled: true,
                next_run_at: Some(90),
                updated_at: 80,
                ..on
            }
        );
        assert_eq!(
            store
                .set_scheduled_task_enabled("missing", true, 1, |_| None)
                .unwrap(),
            None
        );
    }

    /// A task is read with the sessions its kept runs started, newest run first, each once, with
    /// a session that is archived or gone left out; a failed run starts none.
    #[test]
    fn a_task_is_read_with_the_sessions_its_runs_started() {
        use crate::protocol::{
            ScheduledRunOutcome, ScheduledRunTrigger, ScheduledTaskRun, SessionKind,
        };
        use crate::test_support::bare_session;

        let (_path, store) = store_with_project("scheduled-run-sessions");
        store.insert_scheduled_task(&task("task-1", 1)).unwrap();
        store.insert_scheduled_task(&task("task-2", 2)).unwrap();
        for id in ["s-1", "s-2", "s-3"] {
            store
                .insert_session(&bare_session(id, SessionKind::Console))
                .unwrap();
        }
        let run = |id: &str, task_id: &str, outcome: ScheduledRunOutcome| ScheduledTaskRun {
            id: id.to_string(),
            task_id: task_id.to_string(),
            ran_at: 10,
            due_at: None,
            trigger: ScheduledRunTrigger::RunNow,
            outcome,
        };
        let started = |session: &str| ScheduledRunOutcome::Started {
            session: session.to_string(),
            started_independent: false,
        };
        let failed = ScheduledRunOutcome::Failed {
            code: "x".to_string(),
            params: Default::default(),
            message: String::new(),
        };
        for (id, task_id, outcome) in [
            ("r-1", "task-1", started("s-1")),
            ("r-2", "task-1", failed),
            ("r-3", "task-1", started("s-2")),
            ("r-4", "task-1", started("s-1")),
            ("r-5", "task-1", started("s-3")),
            ("r-6", "task-1", started("deleted")),
            ("r-7", "task-2", started("s-2")),
        ] {
            let recorded = store
                .record_scheduled_task_run(&run(id, task_id, outcome))
                .unwrap()
                .expect("recorded");
            assert_eq!(
                recorded.run_sessions,
                store.require_scheduled_task(task_id).unwrap().run_sessions
            );
        }
        store.archive_session("s-3", 20, None).unwrap();

        let listed = store.list_scheduled_tasks().unwrap();
        assert_eq!(listed[0].run_sessions, ["s-1", "s-2"]);
        assert_eq!(listed[1].run_sessions, ["s-2"]);
        assert_eq!(
            store.require_scheduled_task("task-1").unwrap().run_sessions,
            ["s-1", "s-2"]
        );
    }

    /// An account deleted while the launch settings were being checked is refused at the write, as
    /// the check itself would have refused it, and nothing is stored.
    /// `updated_at` orders a task's stored changes: two changes stamped in the same millisecond, or
    /// with a clock that went back, still increase it, and a turn on or off to the state the task is
    /// in stamps nothing.
    #[test]
    fn updated_at_increases_with_every_stored_change() {
        let (_path, store) = store_with_project("scheduled-updated-at");
        store
            .insert_scheduled_task(&ScheduledTask {
                updated_at: 100,
                ..task("task-1", 100)
            })
            .unwrap();
        let read = store.require_scheduled_task("task-1").unwrap();
        let edit = |prompt: &str, now: i64, read: &ScheduledTask| {
            let ScheduledTaskEdit::Edited(edited) = store
                .edit_scheduled_task(
                    &ScheduledTask {
                        prompt: prompt.to_string(),
                        updated_at: now,
                        ..read.clone()
                    },
                    read,
                    |_| None,
                )
                .unwrap()
            else {
                panic!("edited");
            };
            *edited
        };
        let first = edit("One", 500, &read);
        assert_eq!(first.updated_at, 500);
        let second = edit("Two", 500, &first);
        assert_eq!(second.updated_at, 501, "the same millisecond");
        let off = store
            .set_scheduled_task_enabled("task-1", false, 500, |_| None)
            .unwrap()
            .expect("there");
        assert_eq!(off.updated_at, 502);
        let back = edit("Three", 10, &off);
        assert_eq!(back.updated_at, 503, "a clock that went back");
        let again = store
            .set_scheduled_task_enabled("task-1", false, 9_000, |_| None)
            .unwrap()
            .expect("there");
        assert_eq!(again.updated_at, 503, "already off: nothing stamped");
    }

    #[test]
    fn a_save_naming_an_account_that_is_gone_is_refused() {
        let (_path, store) = store_with_project("scheduled-account-gone");
        let named = ScheduledTask {
            launch: ScheduledTaskLaunch {
                account: Some(Some("deleted-meanwhile".to_string())),
                ..Default::default()
            },
            ..task("task-1", 1)
        };
        let refused = |err: super::super::StoreError| {
            let err = anyhow::Error::from(err);
            let coded = err.downcast_ref::<CodedError>().expect("coded").clone();
            assert_eq!(coded.code, crate::protocol::error_code::UNKNOWN_ACCOUNT);
            assert_eq!(coded.message, "unknown account deleted-meanwhile");
        };
        refused(store.insert_scheduled_task(&named).unwrap_err());
        assert!(store.list_scheduled_tasks().unwrap().is_empty());

        let stored = task("task-1", 1);
        store.insert_scheduled_task(&stored).unwrap();
        refused(
            store
                .edit_scheduled_task(&named, &stored, |_| None)
                .unwrap_err(),
        );
        assert_eq!(store.require_scheduled_task("task-1").unwrap(), stored);
    }

    #[test]
    fn an_unknown_task_is_refused_with_its_code() {
        let (_path, store) = store_with_project("scheduled-unknown");
        let err = anyhow::Error::from(store.require_scheduled_task("nope").unwrap_err());
        let coded = err.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(
            coded.code,
            crate::protocol::error_code::UNKNOWN_SCHEDULED_TASK
        );
        assert_eq!(coded.message, "unknown scheduled task nope");
    }

    #[test]
    fn deleting_a_task_removes_it_once() {
        let (_path, store) = store_with_project("scheduled-delete");
        store.insert_scheduled_task(&task("task-1", 1)).unwrap();
        assert!(store.delete_scheduled_task("task-1").unwrap());
        assert!(!store.delete_scheduled_task("task-1").unwrap());
        assert!(store.get_scheduled_task("task-1").unwrap().is_none());
    }

    /// Deleting the project takes its tasks, and deleting the console takes the rest.
    #[test]
    fn deleting_a_console_or_project_takes_its_tasks() {
        let (_path, store) = store_with_project("scheduled-cascade");
        store
            .insert_scheduled_task(&task("console-task", 1))
            .unwrap();
        store
            .insert_scheduled_task(&ScheduledTask {
                project_id: Some("project-1".to_string()),
                ..task("project-task", 2)
            })
            .unwrap();
        store.delete_project("project-1").unwrap();
        let ids: Vec<String> = store
            .list_scheduled_tasks()
            .unwrap()
            .into_iter()
            .map(|task| task.id)
            .collect();
        assert_eq!(ids, ["console-task"]);
        store.delete_console("console-1").unwrap();
        assert!(store.list_scheduled_tasks().unwrap().is_empty());
    }

    /// A task naming an agent this build does not register is kept, and left out of what is read.
    #[test]
    fn a_task_of_an_unregistered_agent_is_kept_and_left_out() {
        let (_path, store) = store_with_project("scheduled-unregistered");
        store.insert_scheduled_task(&task("task-1", 1)).unwrap();
        store
            .lock()
            .execute(
                "UPDATE scheduled_tasks SET agent = 'gone' WHERE id = 'task-1'",
                [],
            )
            .unwrap();
        assert!(store.list_scheduled_tasks().unwrap().is_empty());
        assert!(store.get_scheduled_task("task-1").unwrap().is_none());
        assert!(store.delete_scheduled_task("task-1").unwrap());
    }
}
