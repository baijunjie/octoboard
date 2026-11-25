//! The scheduler's passes, running a task and recording its runs, on a controlled clock: each pass
//! is given the time it runs at, and the schedules are read in UTC.

use super::runs::{run_scheduled_task_now, Launcher};
use super::scheduler::{run_due, Pass, CATCH_UP_WINDOW_MILLIS, ON_TIME_MILLIS};
use super::*;
use crate::coordinator::test_support::{project_of, state_for_opening};
use crate::protocol::{
    error_code, Account, ScheduledRunOutcome, ScheduledRunTrigger, ScheduledTaskRun, SessionStatus,
    TimeOfDay, SCHEDULED_TASK_RUNS_KEPT,
};
use crate::test_support::{code_of, stand_in_launch, Launched, StandIn, PATIENCE};
use jiff::tz::TimeZone;
use std::sync::Mutex;

const PROMPT: &str = "Check the build";

/// Milliseconds of a UTC timestamp such as `2026-11-19T09:00:00Z`.
fn at(text: &str) -> i64 {
    text.parse::<jiff::Timestamp>().unwrap().as_millisecond()
}

const HOUR: i64 = 60 * 60 * 1000;

fn task(id: &str, schedule: Schedule, next_run_at: Option<i64>, enabled: bool) -> ScheduledTask {
    ScheduledTask {
        id: id.to_string(),
        console_id: "console-1".to_string(),
        project_id: None,
        launch: Default::default(),
        prompt: PROMPT.to_string(),
        schedule,
        enabled,
        created_at: 0,
        updated_at: 0,
        next_run_at,
        last_run_at: None,
        run_sessions: vec![],
    }
}

fn daily_at_nine() -> Schedule {
    Schedule::Daily {
        time: TimeOfDay { hour: 9, minute: 0 },
    }
}

/// A launcher that starts a stand-in for every launch, keeping it in `started` and the request in
/// `launches`.
struct StandIns {
    started: Arc<Mutex<Vec<StandIn>>>,
    launches: Arc<Mutex<Vec<Launched>>>,
}

impl StandIns {
    fn new() -> Self {
        Self {
            started: Arc::new(Mutex::new(Vec::new())),
            launches: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn launcher(&self) -> Launcher {
        let (started, launches) = (self.started.clone(), self.launches.clone());
        Arc::new(move |request| (stand_in_launch(&started, &launches, || {}))(request))
    }

    fn launched(&self) -> Vec<Launched> {
        self.launches.lock().unwrap().clone()
    }
}

fn failing_launcher(err: fn() -> anyhow::Error) -> Launcher {
    Arc::new(move |_| Err(err()))
}

/// Waits for the next run recorded, past whatever else is broadcast.
async fn next_run_added(events: &mut tokio::sync::broadcast::Receiver<Event>) -> ScheduledTaskRun {
    tokio::time::timeout(PATIENCE, async {
        loop {
            if let Event::ScheduledTaskRunAdded { run } = events.recv().await.unwrap() {
                return run;
            }
        }
    })
    .await
    .expect("a run was recorded")
}

fn stored(state: &Arc<AppState>, id: &str) -> ScheduledTask {
    state.store.require_scheduled_task(id).unwrap()
}

fn pass(state: &Arc<AppState>, now: i64, pass: Pass, launch: &Launcher) -> Option<i64> {
    run_due(state, now, &TimeZone::UTC, pass, launch)
}

/// A task that comes due starts its session with the prompt as the opening task, records the run as
/// on schedule with the session it started, and moves its next run to the next occurrence.
#[tokio::test]
async fn a_due_task_starts_its_session_with_the_prompt_and_records_the_run() {
    let (state, _dir) = state_for_opening("scheduled-run-due");
    let due = at("2026-11-19T09:00:00Z");
    state
        .store
        .insert_scheduled_task(&task("t", daily_at_nine(), Some(due), true))
        .unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    let earliest = pass(&state, due + 30_000, Pass::Running, &stand_ins.launcher());
    let run = next_run_added(&mut events).await;

    let tomorrow = at("2026-11-20T09:00:00Z");
    assert_eq!(earliest, Some(tomorrow));
    let launched = stand_ins.launched();
    assert_eq!(launched.len(), 1);
    assert_eq!(launched[0].2.as_deref(), Some(PROMPT));
    assert_eq!(run.trigger, ScheduledRunTrigger::Scheduled);
    assert_eq!(run.due_at, Some(due));
    assert_eq!(
        run.outcome,
        ScheduledRunOutcome::Started {
            session: launched[0].0.clone(),
            started_independent: false,
        }
    );
    assert_eq!(
        state.session_record(&launched[0].0).unwrap().status,
        SessionStatus::Working
    );
    let task = stored(&state, "t");
    assert_eq!(task.next_run_at, Some(tomorrow));
    assert_eq!(task.last_run_at, Some(run.ran_at));
    assert_eq!(state.store.list_scheduled_task_runs("t").unwrap(), [run]);
}

/// A disabled task and a manual-only one do not run on a pass, whatever their stored next run.
#[tokio::test]
async fn disabled_and_manual_tasks_do_not_run() {
    let (state, _dir) = state_for_opening("scheduled-run-disabled");
    let due = at("2026-11-19T09:00:00Z");
    let off = task("off", daily_at_nine(), Some(due), false);
    let manual = task("manual", Schedule::Manual, None, true);
    state.store.insert_scheduled_task(&off).unwrap();
    state.store.insert_scheduled_task(&manual).unwrap();
    let stand_ins = StandIns::new();

    assert_eq!(
        pass(&state, due + 1000, Pass::Running, &stand_ins.launcher()),
        None
    );
    assert_eq!(
        pass(&state, due + 1000, Pass::Start, &stand_ins.launcher()),
        None
    );

    assert!(stand_ins.launched().is_empty());
    assert_eq!(stored(&state, "off"), off);
    assert_eq!(stored(&state, "manual"), manual);
}

/// A one-off task runs once and then has no next run, so a later pass leaves it alone.
#[tokio::test]
async fn a_one_off_task_runs_once() {
    let (state, _dir) = state_for_opening("scheduled-run-once");
    let run_at = at("2026-11-19T15:30:00Z");
    state
        .store
        .insert_scheduled_task(&task("t", Schedule::Once { run_at }, Some(run_at), true))
        .unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    pass(&state, run_at + 5_000, Pass::Running, &stand_ins.launcher());
    let run = next_run_added(&mut events).await;
    assert_eq!(run.trigger, ScheduledRunTrigger::Scheduled);
    assert_eq!(stored(&state, "t").next_run_at, None);

    pass(&state, run_at + HOUR, Pass::Running, &stand_ins.launcher());
    assert_eq!(stand_ins.launched().len(), 1);
}

/// At the daemon's start, several missed occurrences of a task make one catch-up run, of the most
/// recent, and the next run moves past the present.
#[tokio::test]
async fn catch_up_at_start_runs_the_latest_missed_occurrence_once() {
    let (state, _dir) = state_for_opening("scheduled-run-catch-up");
    let now = at("2026-11-19T09:30:00Z");
    state
        .store
        .insert_scheduled_task(&task(
            "t",
            Schedule::Hourly { minute: 0 },
            Some(at("2026-11-18T23:00:00Z")),
            true,
        ))
        .unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    pass(&state, now, Pass::Start, &stand_ins.launcher());
    let run = next_run_added(&mut events).await;

    assert_eq!(run.trigger, ScheduledRunTrigger::CatchUp);
    assert_eq!(run.due_at, Some(at("2026-11-19T09:00:00Z")));
    assert_eq!(stand_ins.launched().len(), 1);
    assert_eq!(
        stored(&state, "t").next_run_at,
        Some(at("2026-11-19T10:00:00Z"))
    );
}

/// A missed occurrence older than the catch-up window is skipped, and the task's next run still
/// moves past the present (none for a one-off); one just inside the window is made up.
#[tokio::test]
async fn a_missed_occurrence_older_than_a_day_is_skipped() {
    let (state, _dir) = state_for_opening("scheduled-run-too-old");
    let now = at("2026-11-19T09:30:00Z");
    let too_old = now - CATCH_UP_WINDOW_MILLIS - 1;
    let just_in = now - CATCH_UP_WINDOW_MILLIS;
    // The skipped task is listed, and so handled, first: a run wrongly made for it is then the first
    // one recorded, which the assertions below would see.
    state
        .store
        .insert_scheduled_task(&ScheduledTask {
            created_at: 1,
            ..task(
                "old",
                Schedule::Once { run_at: too_old },
                Some(too_old),
                true,
            )
        })
        .unwrap();
    state
        .store
        .insert_scheduled_task(&ScheduledTask {
            created_at: 2,
            ..task(
                "in",
                Schedule::Once { run_at: just_in },
                Some(just_in),
                true,
            )
        })
        .unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    pass(&state, now, Pass::Start, &stand_ins.launcher());
    let run = next_run_added(&mut events).await;

    assert_eq!(run.task_id, "in");
    assert_eq!(run.trigger, ScheduledRunTrigger::CatchUp);
    assert_eq!(stand_ins.launched().len(), 1);
    assert_eq!(stored(&state, "old").next_run_at, None);
    assert!(state
        .store
        .list_scheduled_task_runs("old")
        .unwrap()
        .is_empty());
}

/// On a running pass, an occurrence reached later than the on-time allowance (the machine slept
/// through it) is a catch-up, not a scheduled run.
#[tokio::test]
async fn a_pass_woken_late_counts_as_a_catch_up() {
    let (state, _dir) = state_for_opening("scheduled-run-late");
    let due = at("2026-11-19T09:00:00Z");
    state
        .store
        .insert_scheduled_task(&task("t", daily_at_nine(), Some(due), true))
        .unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    pass(
        &state,
        due + ON_TIME_MILLIS + 1,
        Pass::Running,
        &stand_ins.launcher(),
    );

    let run = next_run_added(&mut events).await;
    assert_eq!(run.trigger, ScheduledRunTrigger::CatchUp);
    assert_eq!(run.due_at, Some(due));
}

/// Run now runs a task that is disabled and manual-only, and leaves its schedule as it was; an
/// unknown task is refused.
#[tokio::test]
async fn run_now_runs_any_task_and_leaves_its_schedule() {
    let (state, _dir) = state_for_opening("scheduled-run-now");
    let manual = task("t", Schedule::Manual, None, false);
    state.store.insert_scheduled_task(&manual).unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
    let run = next_run_added(&mut events).await;

    assert_eq!(run.trigger, ScheduledRunTrigger::RunNow);
    assert_eq!(run.due_at, None);
    assert!(matches!(run.outcome, ScheduledRunOutcome::Started { .. }));
    assert_eq!(stand_ins.launched()[0].2.as_deref(), Some(PROMPT));
    let after = stored(&state, "t");
    assert_eq!((after.enabled, after.next_run_at), (false, None));
    assert_eq!(after.last_run_at, Some(run.ran_at));

    let err = run_scheduled_task_now(&state, "nope", stand_ins.launcher()).unwrap_err();
    assert_eq!(code_of(&err), error_code::UNKNOWN_SCHEDULED_TASK);
    assert!(err.to_string().contains("unknown scheduled task nope"));
}

/// In a clean-up run, Run now is refused before anything is run: no run is recorded, failed or
/// otherwise, the task is left as it was and nothing is launched.
#[tokio::test]
async fn run_now_is_refused_in_a_clean_up_run_and_leaves_no_run() {
    let (state, _dir) = state_for_opening("scheduled-run-now-clean-up");
    let manual = task("t", Schedule::Manual, None, false);
    state.store.insert_scheduled_task(&manual).unwrap();
    state.set_clean_up_run();
    let stand_ins = StandIns::new();

    let err = run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap_err();

    assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    assert!(
        format!("{err:#}").contains("so it starts no agent"),
        "{err:#}"
    );
    assert!(state
        .store
        .list_scheduled_task_runs("t")
        .unwrap()
        .is_empty());
    assert_eq!(stored(&state, "t").last_run_at, None);
    assert!(stand_ins.launched().is_empty());
}

/// A launch that fails leaves a failed run with the failure's code and account, and no session.
#[tokio::test]
async fn a_failed_launch_is_recorded_as_a_failed_run() {
    let (state, _dir) = state_for_opening("scheduled-run-failed");
    state
        .store
        .insert_scheduled_task(&task("t", Schedule::Manual, None, true))
        .unwrap();
    let mut events = state.subscribe();

    run_scheduled_task_now(
        &state,
        "t",
        failing_launcher(|| anyhow::anyhow!("the agent would not start")),
    )
    .unwrap();
    let run = next_run_added(&mut events).await;

    match &run.outcome {
        ScheduledRunOutcome::Failed { code, message, .. } => {
            assert_eq!(code, error_code::INTERNAL_ERROR.as_str());
            assert!(message.contains("the agent would not start"), "{message}");
        }
        other => panic!("not a failed run: {other:?}"),
    }
    assert!(state.store.list_sessions().unwrap().is_empty());
    assert_eq!(state.store.list_scheduled_task_runs("t").unwrap(), [run]);
}

/// A task stored with a prompt of nothing but control and invisible characters, as one saved
/// before such a prompt was refused can be, starts no session: its run is recorded as failed for
/// the missing prompt, and nothing is launched.
#[tokio::test]
async fn a_stored_task_whose_prompt_is_blank_fails_its_run_for_the_missing_prompt() {
    let (state, _dir) = state_for_opening("scheduled-run-blank-prompt");
    state
        .store
        .insert_scheduled_task(&ScheduledTask {
            prompt: "\u{1}\u{2068}\u{2069} ".to_string(),
            ..task("t", Schedule::Manual, None, true)
        })
        .unwrap();
    let mut events = state.subscribe();

    run_scheduled_task_now(
        &state,
        "t",
        failing_launcher(|| anyhow::anyhow!("nothing is to be launched")),
    )
    .unwrap();
    let run = next_run_added(&mut events).await;

    match &run.outcome {
        ScheduledRunOutcome::Failed {
            code,
            params,
            message,
        } => {
            assert_eq!(code, error_code::FIELD_REQUIRED.as_str());
            assert_eq!(params.get("field").map(String::as_str), Some("prompt"));
            assert_eq!(message, "`prompt` is required");
        }
        other => panic!("not a failed run: {other:?}"),
    }
    assert!(state.store.list_sessions().unwrap().is_empty());
}

/// A run the daemon's stop refuses is not recorded, and the task gets back the occurrence it was
/// waiting for, for the next daemon to make up.
#[tokio::test]
async fn a_run_the_stop_refuses_gives_its_occurrence_back() {
    let (state, _dir) = state_for_opening("scheduled-run-stopping");
    let due = at("2026-11-19T09:00:00Z");
    state
        .store
        .insert_scheduled_task(&task("t", daily_at_nine(), Some(due), true))
        .unwrap();
    let mut events = state.subscribe();

    pass(
        &state,
        due + 1000,
        Pass::Running,
        &failing_launcher(crate::state::shutting_down_refusal),
    );

    tokio::time::timeout(PATIENCE, async {
        loop {
            match events.recv().await.unwrap() {
                Event::ScheduledTaskUpserted { task } if task.next_run_at == Some(due) => return,
                Event::ScheduledTaskRunAdded { run } => panic!("recorded {run:?}"),
                _ => {}
            }
        }
    })
    .await
    .expect("the occurrence was given back");
    assert_eq!(stored(&state, "t").next_run_at, Some(due));
    assert!(state
        .store
        .list_scheduled_task_runs("t")
        .unwrap()
        .is_empty());
}

/// A project task whose console session to report to is gone starts its session independent, and
/// the run says so.
#[tokio::test]
async fn a_task_whose_console_session_is_gone_starts_independent() {
    let (state, dir) = state_for_opening("scheduled-run-owner-gone");
    let mut project = project_of("console-1", "project-1");
    project.path = dir.to_string_lossy().into_owned();
    state.store.insert_project(&project).unwrap();
    let mut gone = task("t", Schedule::Manual, None, true);
    gone.project_id = Some("project-1".to_string());
    gone.launch.bound_to = Some("gone-console-session".to_string());
    state.store.insert_scheduled_task(&gone).unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
    let run = next_run_added(&mut events).await;

    let ScheduledRunOutcome::Started {
        session,
        started_independent,
    } = run.outcome
    else {
        panic!("not started: {run:?}");
    };
    assert!(started_independent);
    assert_eq!(state.session_record(&session).unwrap().bound_to, None);
}

/// A task's account that is turned off gives way to the automatic pick while automatic account
/// selection is on; with it off, the run fails with that refusal.
#[tokio::test]
async fn a_turned_off_account_gives_way_only_with_automatic_selection() {
    let (state, dir) = state_for_opening("scheduled-run-account");
    state
        .store
        .insert_account(&Account {
            id: "off".to_string(),
            agent: crate::protocol::AgentId::CLAUDE,
            name: "Off".to_string(),
            config_dir: dir.join("off").to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: true,
            versions: Default::default(),
        })
        .unwrap();
    let mut on_off = task("t", Schedule::Manual, None, true);
    on_off.launch.account = Some(Some("off".to_string()));
    state.store.insert_scheduled_task(&on_off).unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    state.store.set_auto_select_account(false).unwrap();
    run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
    match next_run_added(&mut events).await.outcome {
        ScheduledRunOutcome::Failed { code, .. } => {
            assert_eq!(code, error_code::ACCOUNT_DISABLED.as_str())
        }
        other => panic!("not refused: {other:?}"),
    }

    state.store.set_auto_select_account(true).unwrap();
    run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
    let ScheduledRunOutcome::Started { session, .. } = next_run_added(&mut events).await.outcome
    else {
        panic!("not started");
    };
    assert_ne!(
        state
            .session_record(&session)
            .unwrap()
            .account_id
            .as_deref(),
        Some("off")
    );
}

/// The store keeps the last runs of a task, most recent first, and drops them with the task.
#[tokio::test]
async fn the_last_runs_of_a_task_are_kept_and_go_with_it() {
    let (state, _dir) = state_for_opening("scheduled-runs-kept");
    state
        .store
        .insert_scheduled_task(&task("t", Schedule::Manual, None, true))
        .unwrap();
    let total = SCHEDULED_TASK_RUNS_KEPT + 3;
    for n in 0..total {
        let recorded = state
            .store
            .record_scheduled_task_run(&ScheduledTaskRun {
                id: format!("run-{n}"),
                task_id: "t".to_string(),
                ran_at: 1000,
                due_at: None,
                trigger: ScheduledRunTrigger::RunNow,
                outcome: ScheduledRunOutcome::Failed {
                    code: "internal_error".to_string(),
                    params: [("detail".to_string(), format!("{n}"))].into(),
                    message: format!("{n}"),
                },
            })
            .unwrap();
        assert_eq!(recorded.unwrap().last_run_at, Some(1000));
    }

    let runs = state.store.list_scheduled_task_runs("t").unwrap();
    let ids: Vec<String> = runs.iter().map(|run| run.id.clone()).collect();
    let expected: Vec<String> = (3..total).rev().map(|n| format!("run-{n}")).collect();
    assert_eq!(ids, expected);
    assert!(matches!(
        &runs[0].outcome,
        ScheduledRunOutcome::Failed { params, .. } if params["detail"] == format!("{}", total - 1)
    ));

    assert!(state.store.delete_scheduled_task("t").unwrap());
    assert!(state
        .store
        .list_scheduled_task_runs("t")
        .unwrap()
        .is_empty());
    assert!(state
        .store
        .record_scheduled_task_run(&runs[0])
        .unwrap()
        .is_none());
}

/// The scheduler's first pass catches up what passed while the daemon was down, and a change to the
/// tasks wakes it long before its wait is over, to run what has come due on schedule.
#[tokio::test]
async fn the_scheduler_catches_up_at_start_and_wakes_on_a_change() {
    let (state, _dir) = state_for_opening("scheduled-loop");
    let missed = crate::time::now_millis() - 1000;
    state
        .store
        .insert_scheduled_task(&task(
            "missed",
            Schedule::Once { run_at: missed },
            Some(missed),
            true,
        ))
        .unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    let scheduler = super::scheduler::spawn_with(
        &state,
        std::time::Duration::from_secs(24 * 60 * 60),
        stand_ins.launcher(),
    );
    let caught_up = next_run_added(&mut events).await;
    assert_eq!(
        (caught_up.task_id.as_str(), caught_up.trigger),
        ("missed", ScheduledRunTrigger::CatchUp)
    );

    let due = crate::time::now_millis();
    state
        .store
        .insert_scheduled_task(&task(
            "due",
            Schedule::Once { run_at: due },
            Some(due),
            true,
        ))
        .unwrap();
    state.scheduler_wake.wake();
    let scheduled = next_run_added(&mut events).await;
    scheduler.abort();
    assert_eq!(
        (scheduled.task_id.as_str(), scheduled.trigger),
        ("due", ScheduledRunTrigger::Scheduled)
    );
}

/// A run due while the session of the previous run is still working starts another session, and
/// so does a second Run now: overlapping runs are not skipped.
#[tokio::test]
async fn overlapping_runs_each_start_their_own_session() {
    let (state, _dir) = state_for_opening("scheduled-run-overlap");
    let due = at("2026-11-19T09:00:00Z");
    let tomorrow = at("2026-11-20T09:00:00Z");
    state
        .store
        .insert_scheduled_task(&task("t", daily_at_nine(), Some(due), true))
        .unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();
    let started = |run: ScheduledTaskRun| match run.outcome {
        ScheduledRunOutcome::Started { session, .. } => session,
        other => panic!("not started: {other:?}"),
    };

    pass(&state, due + 1000, Pass::Running, &stand_ins.launcher());
    let first = started(next_run_added(&mut events).await);
    pass(
        &state,
        tomorrow + 1000,
        Pass::Running,
        &stand_ins.launcher(),
    );
    let second = started(next_run_added(&mut events).await);
    run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
    let third = started(next_run_added(&mut events).await);

    let sessions = [&first, &second, &third];
    for (n, session) in sessions.iter().enumerate() {
        assert!(
            sessions[..n].iter().all(|earlier| earlier != session),
            "{session} repeats an earlier session"
        );
        assert_eq!(
            state.session_record(session).unwrap().status,
            SessionStatus::Working,
            "{session}"
        );
    }
    assert_eq!(stand_ins.launched().len(), 3);
    assert_eq!(state.store.list_scheduled_task_runs("t").unwrap().len(), 3);
}

/// Of several missed occurrences the most recent runs once when it is within a day, however long
/// ago the task's stored next run was; one whose latest occurrence is older than a day is skipped
/// and its next run moves to the future; a task not yet due is left alone.
#[tokio::test]
async fn catch_up_judges_the_most_recent_occurrence_not_the_stored_one() {
    let (state, _dir) = state_for_opening("scheduled-catch-up-latest");
    // 2026-11-19 is a Thursday.
    let now = at("2026-11-19T10:00:00Z");
    let mut weekly_old = task(
        "weekly-old",
        Schedule::Weekly {
            time: TimeOfDay { hour: 9, minute: 0 },
            days: vec![crate::protocol::Weekday::Tuesday],
        },
        // Last Tuesday's occurrence was missed too; this week's was on Tuesday, 2 days ago.
        Some(at("2026-11-10T09:00:00Z")),
        true,
    );
    weekly_old.created_at = 1;
    let mut daily_recent = task(
        "daily-recent",
        daily_at_nine(),
        Some(at("2026-11-16T09:00:00Z")),
        true,
    );
    daily_recent.created_at = 2;
    let mut not_due = task(
        "not-due",
        daily_at_nine(),
        Some(at("2026-11-20T09:00:00Z")),
        true,
    );
    not_due.created_at = 3;
    for task in [&weekly_old, &daily_recent, &not_due] {
        state.store.insert_scheduled_task(task).unwrap();
    }
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    let earliest = pass(&state, now, Pass::Start, &stand_ins.launcher());
    let run = next_run_added(&mut events).await;

    assert_eq!(run.task_id, "daily-recent");
    assert_eq!(run.trigger, ScheduledRunTrigger::CatchUp);
    assert_eq!(run.due_at, Some(at("2026-11-19T09:00:00Z")));
    assert_eq!(stand_ins.launched().len(), 1);
    assert_eq!(
        stored(&state, "daily-recent").next_run_at,
        Some(at("2026-11-20T09:00:00Z"))
    );
    assert_eq!(
        stored(&state, "weekly-old").next_run_at,
        Some(at("2026-11-24T09:00:00Z"))
    );
    assert!(state
        .store
        .list_scheduled_task_runs("weekly-old")
        .unwrap()
        .is_empty());
    assert_eq!(stored(&state, "not-due"), not_due);
    assert_eq!(earliest, Some(at("2026-11-20T09:00:00Z")));
}

/// A project task reports to its console session while that session is there, and starts
/// independent, saying so, when it has been archived.
#[tokio::test]
async fn a_project_task_reports_to_its_console_session_unless_it_is_archived() {
    let (state, dir) = state_for_opening("scheduled-run-owner-archived");
    let mut project = project_of("console-1", "project-1");
    project.path = dir.to_string_lossy().into_owned();
    state.store.insert_project(&project).unwrap();
    state
        .store
        .insert_session(&crate::test_support::bare_session(
            "hub",
            crate::protocol::SessionKind::Console,
        ))
        .unwrap();
    let mut reporting = task("t", Schedule::Manual, None, true);
    reporting.project_id = Some("project-1".to_string());
    reporting.launch.bound_to = Some("hub".to_string());
    state.store.insert_scheduled_task(&reporting).unwrap();
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
    let ScheduledRunOutcome::Started {
        session,
        started_independent,
    } = next_run_added(&mut events).await.outcome
    else {
        panic!("not started");
    };
    assert!(!started_independent);
    assert_eq!(
        state.session_record(&session).unwrap().bound_to.as_deref(),
        Some("hub")
    );

    state.store.archive_session("hub", 1, None).unwrap();
    run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
    let ScheduledRunOutcome::Started {
        session,
        started_independent,
    } = next_run_added(&mut events).await.outcome
    else {
        panic!("not started");
    };
    assert!(started_independent);
    assert_eq!(state.session_record(&session).unwrap().bound_to, None);
}

/// An account whose quota is used up, or that is not logged in, gives way to the automatic pick
/// only while automatic account selection is on; with it off the run fails with that refusal.
#[tokio::test]
async fn a_used_up_or_signed_out_account_gives_way_only_with_automatic_selection() {
    use crate::protocol::{
        quota_unavailable_code, AccountQuota, QuotaUnavailable, QuotaWindow, QuotaWindowKind,
    };

    let (state, dir) = state_for_opening("scheduled-run-unusable");
    let config_dir = dir.join("acct").to_string_lossy().into_owned();
    state
        .store
        .insert_account(&Account {
            id: "acct".to_string(),
            agent: crate::protocol::AgentId::CLAUDE,
            name: "Acct".to_string(),
            config_dir: config_dir.clone(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        })
        .unwrap();
    let mut pinned = task("t", Schedule::Manual, None, true);
    pinned.launch.account = Some(Some("acct".to_string()));
    state.store.insert_scheduled_task(&pinned).unwrap();
    let reading = |signed_out: bool| AccountQuota {
        reset_credits: None,
        agent: crate::protocol::AgentId::CLAUDE,
        account: Some("acct".to_string()),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: if signed_out {
            Vec::new()
        } else {
            vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: 100.0,
                resets_at: Some(crate::time::now_millis() + 60_000),
                window_minutes: None,
            }]
        },
        unavailable: signed_out.then(|| {
            QuotaUnavailable::new(
                quota_unavailable_code::NOT_SIGNED_IN,
                "x",
                &[("agent", "Claude Code")],
            )
        }),
    };
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    for (signed_out, refusal) in [
        (false, error_code::ACCOUNT_QUOTA_EXHAUSTED),
        (true, error_code::ACCOUNT_NOT_SIGNED_IN),
    ] {
        state
            .account_quotas
            .hold_in_dir_for_test(&reading(signed_out), &config_dir);

        state.store.set_auto_select_account(false).unwrap();
        run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
        match next_run_added(&mut events).await.outcome {
            ScheduledRunOutcome::Failed { code, .. } => {
                assert_eq!(code, refusal.as_str(), "signed out: {signed_out}")
            }
            other => panic!("not refused (signed out: {signed_out}): {other:?}"),
        }

        state.store.set_auto_select_account(true).unwrap();
        run_scheduled_task_now(&state, "t", stand_ins.launcher()).unwrap();
        let ScheduledRunOutcome::Started { session, .. } =
            next_run_added(&mut events).await.outcome
        else {
            panic!("not started (signed out: {signed_out})");
        };
        assert_ne!(
            state
                .session_record(&session)
                .unwrap()
                .account_id
                .as_deref(),
            Some("acct"),
            "signed out: {signed_out}"
        );
    }
}

/// Once the daemon is asked to stop, a pass takes no occurrence and starts nothing, leaving what is
/// due for the daemon that starts next; and the scheduler ends at once.
#[tokio::test]
async fn nothing_is_run_once_the_daemon_is_asked_to_stop() {
    let (state, _dir) = state_for_opening("scheduled-stopping");
    let due = at("2026-11-19T09:00:00Z");
    let waiting = task("t", daily_at_nine(), Some(due), true);
    state.store.insert_scheduled_task(&waiting).unwrap();
    let stand_ins = StandIns::new();

    state.request_planned_stop();
    pass(&state, due + 1000, Pass::Running, &stand_ins.launcher());
    pass(&state, due + 1000, Pass::Start, &stand_ins.launcher());
    assert!(stand_ins.launched().is_empty());
    assert_eq!(stored(&state, "t"), waiting);

    let scheduler = super::scheduler::spawn_with(
        &state,
        std::time::Duration::from_secs(24 * 60 * 60),
        stand_ins.launcher(),
    );
    state.scheduler_wake.wake();
    tokio::time::timeout(PATIENCE, scheduler)
        .await
        .expect("the scheduler ended")
        .unwrap();
    assert!(stand_ins.launched().is_empty());
    assert_eq!(stored(&state, "t"), waiting);
}

/// Which occurrence a catch-up makes up, read where it is decided: the most recent one no older
/// than a day, whatever the stored one was, and none when even the most recent is older (a run
/// that was wrongly made for it could only be told apart from a skipped one by a window of
/// waiting, which is why this is checked on the decision itself).
#[test]
fn the_occurrence_to_make_up_is_the_most_recent_within_a_day() {
    use super::scheduler::latest_occurrence;
    let zone = TimeZone::UTC;
    let now = at("2026-11-19T10:00:00Z");
    let hourly = Schedule::Hourly { minute: 0 };
    // Several missed occurrences: the latest of them, however far back the stored one is.
    assert_eq!(
        latest_occurrence(&hourly, at("2026-11-10T00:00:00Z"), now, &zone),
        Some(at("2026-11-19T10:00:00Z"))
    );
    assert_eq!(
        latest_occurrence(&daily_at_nine(), at("2026-11-16T09:00:00Z"), now, &zone),
        Some(at("2026-11-19T09:00:00Z"))
    );
    // A single missed occurrence is made up up to a day old, to the millisecond, and not past it.
    let once = |run_at| Schedule::Once { run_at };
    assert_eq!(
        latest_occurrence(
            &once(now - CATCH_UP_WINDOW_MILLIS),
            now - CATCH_UP_WINDOW_MILLIS,
            now,
            &zone
        ),
        Some(now - CATCH_UP_WINDOW_MILLIS)
    );
    assert_eq!(
        latest_occurrence(
            &once(now - CATCH_UP_WINDOW_MILLIS - 1),
            now - CATCH_UP_WINDOW_MILLIS - 1,
            now,
            &zone
        ),
        None
    );
    // The most recent occurrence older than a day is skipped, though the stored one is older still.
    let weekly = Schedule::Weekly {
        time: TimeOfDay { hour: 9, minute: 0 },
        days: vec![crate::protocol::Weekday::Tuesday],
    };
    assert_eq!(
        latest_occurrence(&weekly, at("2026-11-10T09:00:00Z"), now, &zone),
        None
    );
    // A monthly occurrence missed within a day is made up, and one a day or more old is not.
    let monthly = Schedule::Monthly {
        day: 19,
        time: TimeOfDay { hour: 9, minute: 0 },
    };
    assert_eq!(
        latest_occurrence(&monthly, at("2026-10-19T09:00:00Z"), now, &zone),
        Some(at("2026-11-19T09:00:00Z"))
    );
    assert_eq!(
        latest_occurrence(
            &monthly,
            at("2026-10-19T09:00:00Z"),
            at("2026-11-20T10:00:00Z"),
            &zone
        ),
        None
    );
    // Nothing is due before the stored occurrence.
    assert_eq!(
        latest_occurrence(&daily_at_nine(), now + HOUR, now, &zone),
        None
    );
}

/// A monthly occurrence in a month without its day falls on that month's last day, and that is
/// the one made up: day 31 in November is 30 November, and day 30 in February is 28 February.
#[test]
fn the_occurrence_to_make_up_for_a_monthly_task_follows_the_last_day_rule() {
    use super::scheduler::latest_occurrence;
    let zone = TimeZone::UTC;
    let monthly = |day| Schedule::Monthly {
        day,
        time: TimeOfDay { hour: 9, minute: 0 },
    };
    assert_eq!(
        latest_occurrence(
            &monthly(31),
            at("2026-10-31T09:00:00Z"),
            at("2026-11-30T10:00:00Z"),
            &zone
        ),
        Some(at("2026-11-30T09:00:00Z"))
    );
    assert_eq!(
        latest_occurrence(
            &monthly(30),
            at("2027-01-30T09:00:00Z"),
            at("2027-02-28T10:00:00Z"),
            &zone
        ),
        Some(at("2027-02-28T09:00:00Z"))
    );
    // Before the last day's time nothing is due yet.
    assert_eq!(
        latest_occurrence(
            &monthly(31),
            at("2026-11-30T09:00:00Z"),
            at("2026-11-30T08:00:00Z"),
            &zone
        ),
        None
    );
}

/// A project task whose console session to report to cannot take sessions (still starting,
/// archived, or a session of another console) starts independent and says so, not failed; one
/// whose console session is working keeps reporting to it.
#[tokio::test]
async fn a_console_session_that_cannot_take_sessions_leaves_the_run_independent() {
    use crate::protocol::{Console, SessionKind};
    use crate::test_support::bare_session;

    let (state, dir) = state_for_opening("scheduled-run-owner-states");
    let mut project = project_of("console-1", "project-1");
    project.path = dir.to_string_lossy().into_owned();
    state.store.insert_project(&project).unwrap();
    state
        .store
        .insert_console(&Console {
            id: "console-2".to_string(),
            workdir: dir.join("other").to_string_lossy().into_owned(),
            ..crate::test_support::console(&dir)
        })
        .unwrap();
    let owners = [
        ("working", SessionStatus::Working, "console-1", false),
        ("starting", SessionStatus::Starting, "console-1", true),
        ("archived", SessionStatus::Archived, "console-1", true),
        ("of-another-console", SessionStatus::Idle, "console-2", true),
    ];
    for (id, status, console_id, _) in owners {
        let mut owner = bare_session(id, SessionKind::Console);
        owner.status = status;
        owner.console_id = console_id.to_string();
        state.store.insert_session(&owner).unwrap();
        let mut reporting = task(id, Schedule::Manual, None, true);
        reporting.project_id = Some("project-1".to_string());
        reporting.launch.bound_to = Some(id.to_string());
        state.store.insert_scheduled_task(&reporting).unwrap();
    }
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    for (id, _, _, independent) in owners {
        run_scheduled_task_now(&state, id, stand_ins.launcher()).unwrap();
        let run = next_run_added(&mut events).await;
        assert_eq!(run.task_id, id);
        let ScheduledRunOutcome::Started {
            session,
            started_independent,
        } = run.outcome
        else {
            panic!("{id}: not started: {run:?}");
        };
        assert_eq!(started_independent, independent, "{id}");
        assert_eq!(
            state.session_record(&session).unwrap().bound_to,
            (!independent).then(|| id.to_string()),
            "{id}"
        );
    }
}

/// An edit that leaves the schedule alone, or sends the one the task has, does not drop a run that
/// is due and not yet taken: the next scheduler pass still runs it, with the edited prompt.
#[tokio::test]
async fn an_edit_does_not_drop_a_run_that_is_due() {
    let (state, _dir) = state_for_opening("scheduled-run-edit-due");
    let due = at("2026-11-19T09:00:00Z");
    for id in ["prompt-only", "same-schedule"] {
        state
            .store
            .insert_scheduled_task(&task(id, daily_at_nine(), Some(due), true))
            .unwrap();
    }
    let edit = |id: &'static str, schedule: Option<Schedule>| {
        let state = state.clone();
        async move {
            update_scheduled_task(
                &state,
                None,
                id,
                ScheduledTaskChanges {
                    prompt: Some("Edited".to_string()),
                    schedule,
                    console_id: None,
                    project_id: None,
                    launch: None,
                },
            )
            .await
            .unwrap()
        }
    };
    edit("prompt-only", None).await;
    edit("same-schedule", Some(daily_at_nine())).await;
    for id in ["prompt-only", "same-schedule"] {
        assert_eq!(stored(&state, id).next_run_at, Some(due), "{id}");
    }
    let mut events = state.subscribe();
    let stand_ins = StandIns::new();

    pass(&state, due + 30_000, Pass::Running, &stand_ins.launcher());
    let mut ran = vec![
        next_run_added(&mut events).await.task_id,
        next_run_added(&mut events).await.task_id,
    ];
    ran.sort();

    assert_eq!(ran, ["prompt-only", "same-schedule"]);
    assert!(stand_ins
        .launched()
        .iter()
        .all(|launched| launched.2.as_deref() == Some("Edited")));
}

/// Deleting an account that tasks named gives those tasks their target's account again, announced
/// once per changed task; a task pinned to the default account and one that inherits are left
/// alone, and a later run of a changed task starts rather than failing with `unknown_account`.
#[tokio::test]
async fn a_task_whose_account_was_deleted_runs_on_its_targets_account() {
    use crate::protocol::{AgentId, RequestBody};

    let (state, dir) = state_for_opening("scheduled-run-deleted-account");
    for id in ["gone", "kept"] {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent: AgentId::CLAUDE,
                name: id.to_string(),
                config_dir: dir.join(id).to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();
    }
    let with_account = |id: &str, account: Option<Option<&str>>| {
        let mut task = task(id, Schedule::Manual, None, true);
        task.launch.account = account.map(|account| account.map(str::to_string));
        task
    };
    let tasks = [
        with_account("names-gone", Some(Some("gone"))),
        with_account("pinned-default", Some(None)),
        with_account("inherits", None),
        with_account("names-kept", Some(Some("kept"))),
    ];
    for task in &tasks {
        state.store.insert_scheduled_task(task).unwrap();
    }
    let mut events = state.subscribe();

    crate::coordinator::handle(
        &state,
        None,
        RequestBody::DeleteAccount {
            account: "gone".to_string(),
            trash_directory: false,
        },
    )
    .await
    .expect("deleted");

    let mut announced = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::ScheduledTaskUpserted { task } = event {
            announced.push(task);
        }
    }
    let mut cleared = tasks[0].clone();
    cleared.launch.account = None;
    let [announced] = announced.as_slice() else {
        panic!("one task is announced, not {announced:?}");
    };
    // Clearing the account is a stored change of the task, so its stamp moves past the one it had.
    assert!(announced.updated_at > cleared.updated_at);
    cleared.updated_at = announced.updated_at;
    assert_eq!(*announced, cleared);
    assert_eq!(stored(&state, "names-gone"), cleared);
    for unchanged in &tasks[1..] {
        assert_eq!(stored(&state, &unchanged.id), *unchanged);
    }

    let stand_ins = StandIns::new();
    run_scheduled_task_now(&state, "names-gone", stand_ins.launcher()).unwrap();
    let run = next_run_added(&mut events).await;
    assert!(
        matches!(run.outcome, ScheduledRunOutcome::Started { .. }),
        "{run:?}"
    );
}
