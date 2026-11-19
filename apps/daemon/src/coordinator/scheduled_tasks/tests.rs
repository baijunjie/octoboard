use super::*;
use crate::protocol::{error_code, AgentId, Code, SessionKind, SessionStatus, TimeOfDay, Weekday};
use crate::test_support::{bare_session, code_of, console_with_project_and_accounts};
use std::sync::Arc;

fn new_task(prompt: &str, schedule: Schedule) -> NewScheduledTask {
    NewScheduledTask {
        console_id: "console-1".to_string(),
        project_id: None,
        launch: None,
        prompt: prompt.to_string(),
        schedule,
        enabled: true,
    }
}

fn daily() -> Schedule {
    Schedule::Daily {
        time: TimeOfDay { hour: 9, minute: 0 },
    }
}

fn no_changes() -> ScheduledTaskChanges {
    ScheduledTaskChanges {
        console_id: None,
        project_id: None,
        launch: None,
        prompt: None,
        schedule: None,
    }
}

async fn create(state: &Arc<AppState>, new: NewScheduledTask) -> Result<ScheduledTask> {
    match create_scheduled_task(state, Some("r1".to_string()), new).await? {
        Some(Event::ScheduledTaskSaved { id, task }) => {
            assert_eq!(id.as_deref(), Some("r1"));
            Ok(task)
        }
        other => panic!("not a saved task: {other:?}"),
    }
}

fn saved_task(reply: Option<Event>) -> ScheduledTask {
    match reply {
        Some(Event::ScheduledTaskSaved { task, .. }) => task,
        other => panic!("not a saved task: {other:?}"),
    }
}

/// The events about scheduled tasks broadcast since `events` subscribed.
fn task_events(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<Event> {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter(|event| {
            matches!(
                event,
                Event::ScheduledTaskUpserted { .. } | Event::ScheduledTaskDeleted { .. }
            )
        })
        .collect()
}

fn assert_refused(err: &anyhow::Error, code: Code, message: &str) {
    assert_eq!(code_of(err), code, "{err:#}");
    assert!(err.to_string().contains(message), "{err:#}");
}

/// A created task is stored enabled with its text trimmed and its next run computed from now, and
/// every client is told of it; listing returns it.
#[tokio::test]
async fn a_created_task_is_stored_enabled_and_announced() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-create");
    let mut events = state.subscribe();
    let task = create(
        &state,
        NewScheduledTask {
            launch: Some(ScheduledTaskLaunch {
                title: Some("  Nightly  ".to_string()),
                bound_to: Some("ignored-for-a-console-task".to_string()),
                ..Default::default()
            }),
            ..new_task("  Check the build  ", daily())
        },
    )
    .await
    .expect("created");

    assert!(task.enabled);
    assert_eq!(task.prompt, "Check the build");
    assert_eq!(task.launch.title.as_deref(), Some("Nightly"));
    assert_eq!(task.launch.bound_to, None);
    assert_eq!(task.created_at, task.updated_at);
    assert_eq!(
        task.next_run_at,
        next_run(&daily(), task.created_at, &local_zone())
    );
    assert!(task.next_run_at.is_some());
    assert_eq!(task.last_run_at, None);
    assert_eq!(
        state.store.get_scheduled_task(&task.id).unwrap(),
        Some(task.clone())
    );
    assert!(matches!(
        task_events(&mut events).as_slice(),
        [Event::ScheduledTaskUpserted { task: announced }] if *announced == task
    ));
    match list_scheduled_tasks(&state, Some("r2".to_string())).unwrap() {
        Some(Event::ScheduledTasksListed {
            id,
            tasks,
            time_zone,
        }) => {
            assert_eq!(id.as_deref(), Some("r2"));
            assert_eq!(tasks, [task]);
            // The zone schedules are read in, by its IANA name.
            assert_eq!(time_zone, zone_name(&local_zone()));
        }
        other => panic!("not a list: {other:?}"),
    }
}

/// A manual-only task has no next run, and a weekly one keeps its days sorted, each once.
#[tokio::test]
async fn a_task_is_stored_with_its_schedule_normalised() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-normalised");
    let manual = create(&state, new_task("Run", Schedule::Manual))
        .await
        .unwrap();
    assert_eq!(manual.next_run_at, None);
    let weekly = create(
        &state,
        new_task(
            "Run",
            Schedule::Weekly {
                time: TimeOfDay { hour: 7, minute: 0 },
                days: vec![Weekday::Friday, Weekday::Monday, Weekday::Friday],
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        weekly.schedule,
        Schedule::Weekly {
            time: TimeOfDay { hour: 7, minute: 0 },
            days: vec![Weekday::Monday, Weekday::Friday],
        }
    );
}

/// A monthly schedule is stored as given, with a next run on its day of the month.
#[tokio::test]
async fn a_monthly_task_is_stored_with_its_next_run() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-monthly");
    let schedule = Schedule::Monthly {
        day: 31,
        time: TimeOfDay { hour: 9, minute: 0 },
    };
    let task = create(&state, new_task("Run", schedule.clone()))
        .await
        .unwrap();
    assert_eq!(task.schedule, schedule);
    let next = task.next_run_at.expect("a next run");
    assert!(next > now_millis());
    assert_eq!(
        state.store.get_scheduled_task(&task.id).unwrap(),
        Some(task)
    );
}

/// A project task is checked against its project, and keeps the console session it reports to.
#[tokio::test]
async fn a_project_task_reports_to_a_console_session() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-project");
    state
        .store
        .insert_session(&bare_session("hub", SessionKind::Console))
        .unwrap();
    let task = create(
        &state,
        NewScheduledTask {
            project_id: Some("project-1".to_string()),
            launch: Some(ScheduledTaskLaunch {
                agent: Some(AgentId::CODEX),
                account: Some(Some("codex-a".to_string())),
                bound_to: Some("hub".to_string()),
                ..Default::default()
            }),
            ..new_task("Run", daily())
        },
    )
    .await
    .expect("created");
    assert_eq!(task.project_id.as_deref(), Some("project-1"));
    assert_eq!(task.launch.bound_to.as_deref(), Some("hub"));
    assert_eq!(task.launch.account, Some(Some("codex-a".to_string())));
}

/// Each refusal of a create, with its code and message, leaving nothing stored.
#[tokio::test]
async fn a_create_is_refused_for_each_ill_formed_or_unopenable_task() {
    let (state, dir) = console_with_project_and_accounts("scheduled-refusals");
    state
        .store
        .insert_console(&crate::protocol::Console {
            id: "console-2".to_string(),
            workdir: dir.join("other").to_string_lossy().into_owned(),
            ..crate::test_support::console(&dir)
        })
        .unwrap();
    let mut project_session = bare_session("worker", SessionKind::Project);
    project_session.project_id = Some("project-1".to_string());
    state.store.insert_session(&project_session).unwrap();
    let mut other_console_session = bare_session("elsewhere", SessionKind::Console);
    other_console_session.console_id = "console-2".to_string();
    state.store.insert_session(&other_console_session).unwrap();
    let launch = |launch: ScheduledTaskLaunch| Some(launch);
    let past = now_millis() - 1;
    let cases: Vec<(NewScheduledTask, Code, &str)> = vec![
        (
            new_task("   ", daily()),
            error_code::FIELD_REQUIRED,
            "`prompt` is required",
        ),
        (
            new_task(
                "Run",
                Schedule::Weekly {
                    time: TimeOfDay { hour: 7, minute: 0 },
                    days: vec![],
                },
            ),
            error_code::FIELD_REQUIRED,
            "`days` is required",
        ),
        (
            new_task("Run", Schedule::Hourly { minute: 60 }),
            error_code::UNREADABLE_REQUEST,
            "the schedule's minute is out of range",
        ),
        (
            new_task(
                "Run",
                Schedule::Monthly {
                    day: 0,
                    time: TimeOfDay { hour: 9, minute: 0 },
                },
            ),
            error_code::UNREADABLE_REQUEST,
            "the schedule's day is out of range",
        ),
        (
            new_task(
                "Run",
                Schedule::Monthly {
                    day: 32,
                    time: TimeOfDay { hour: 9, minute: 0 },
                },
            ),
            error_code::UNREADABLE_REQUEST,
            "the schedule's day is out of range",
        ),
        (
            new_task(
                "Run",
                Schedule::Monthly {
                    day: 15,
                    time: TimeOfDay {
                        hour: 9,
                        minute: 60,
                    },
                },
            ),
            error_code::UNREADABLE_REQUEST,
            "the schedule's minute is out of range",
        ),
        (
            new_task(
                "Run",
                Schedule::Daily {
                    time: TimeOfDay {
                        hour: 24,
                        minute: 0,
                    },
                },
            ),
            error_code::UNREADABLE_REQUEST,
            "the schedule's hour is out of range",
        ),
        (
            new_task("Run", Schedule::Once { run_at: past }),
            error_code::SCHEDULED_TIME_PASSED,
            "the one-off time is not in the future",
        ),
        (
            NewScheduledTask {
                console_id: "nope".to_string(),
                ..new_task("Run", daily())
            },
            error_code::UNKNOWN_CONSOLE,
            "unknown console nope",
        ),
        (
            NewScheduledTask {
                project_id: Some("missing".to_string()),
                ..new_task("Run", daily())
            },
            error_code::UNKNOWN_PROJECT,
            "unknown project missing",
        ),
        (
            NewScheduledTask {
                console_id: "console-2".to_string(),
                project_id: Some("project-1".to_string()),
                ..new_task("Run", daily())
            },
            error_code::UNKNOWN_PROJECT,
            "unknown project project-1",
        ),
        (
            NewScheduledTask {
                launch: launch(ScheduledTaskLaunch {
                    agent: Some(AgentId::CLAUDE),
                    account: Some(Some("codex-a".to_string())),
                    ..Default::default()
                }),
                ..new_task("Run", daily())
            },
            error_code::UNKNOWN_ACCOUNT,
            "codex-a",
        ),
        (
            NewScheduledTask {
                launch: launch(ScheduledTaskLaunch {
                    agent: Some(AgentId::CLAUDE),
                    compatibility: [("no_such_switch".to_string(), true)].into(),
                    ..Default::default()
                }),
                ..new_task("Run", daily())
            },
            error_code::UNREADABLE_REQUEST,
            "`no_such_switch` is not a compatibility switch",
        ),
        (
            NewScheduledTask {
                project_id: Some("project-1".to_string()),
                launch: launch(ScheduledTaskLaunch {
                    bound_to: Some("worker".to_string()),
                    ..Default::default()
                }),
                ..new_task("Run", daily())
            },
            error_code::UNKNOWN_SESSION,
            "unknown session worker",
        ),
        (
            NewScheduledTask {
                project_id: Some("project-1".to_string()),
                launch: launch(ScheduledTaskLaunch {
                    bound_to: Some("gone".to_string()),
                    ..Default::default()
                }),
                ..new_task("Run", daily())
            },
            error_code::UNKNOWN_SESSION,
            "unknown session gone",
        ),
        (
            NewScheduledTask {
                project_id: Some("project-1".to_string()),
                launch: launch(ScheduledTaskLaunch {
                    bound_to: Some("elsewhere".to_string()),
                    ..Default::default()
                }),
                ..new_task("Run", daily())
            },
            error_code::UNKNOWN_SESSION,
            "unknown session elsewhere",
        ),
    ];
    for (new, code, message) in cases {
        let err = create(&state, new).await.expect_err(message);
        assert_refused(&err, code, message);
    }
    assert!(state.store.list_scheduled_tasks().unwrap().is_empty());
}

/// A turned-off account is refused as `open_session` refuses it.
#[tokio::test]
async fn a_create_naming_a_turned_off_account_is_refused() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-disabled-account");
    assert!(state.store.set_account_disabled("claude-a", true).unwrap());
    let new = NewScheduledTask {
        launch: Some(ScheduledTaskLaunch {
            agent: Some(AgentId::CLAUDE),
            account: Some(Some("claude-a".to_string())),
            ..Default::default()
        }),
        ..new_task("Run", daily())
    };
    let err = create(&state, new).await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::ACCOUNT_DISABLED, "{err:#}");
    assert!(state.store.list_scheduled_tasks().unwrap().is_empty());
}

/// An update changes what it names and keeps the rest; the result is announced.
#[tokio::test]
async fn an_update_changes_only_what_it_names() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-update");
    let task = create(&state, new_task("Run", daily())).await.unwrap();
    let mut events = state.subscribe();
    let updated = saved_task(
        update_scheduled_task(
            &state,
            None,
            &task.id,
            ScheduledTaskChanges {
                prompt: Some(" Another ".to_string()),
                schedule: Some(Schedule::Manual),
                ..no_changes()
            },
        )
        .await
        .expect("updated"),
    );
    assert_eq!(updated.prompt, "Another");
    assert_eq!(updated.schedule, Schedule::Manual);
    assert_eq!(updated.next_run_at, None);
    assert_eq!(updated.console_id, task.console_id);
    assert_eq!(updated.launch, task.launch);
    assert_eq!(updated.created_at, task.created_at);
    assert_eq!(
        state.store.get_scheduled_task(&task.id).unwrap(),
        Some(updated.clone())
    );
    assert!(matches!(
        task_events(&mut events).as_slice(),
        [Event::ScheduledTaskUpserted { task: announced }] if *announced == updated
    ));

    // Moved to the project, with `project_id` read beside `console_id`.
    let moved = saved_task(
        update_scheduled_task(
            &state,
            None,
            &task.id,
            ScheduledTaskChanges {
                console_id: Some("console-1".to_string()),
                project_id: Some(Some("project-1".to_string())),
                ..no_changes()
            },
        )
        .await
        .expect("moved"),
    );
    assert_eq!(moved.project_id.as_deref(), Some("project-1"));
}

/// A one-off whose time has passed may be edited without changing its schedule, but a schedule
/// changed to a passed time is refused; so is a project without its console, and an unknown task.
#[tokio::test]
async fn an_update_is_refused_for_a_changed_passed_time_and_unknown_tasks() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-update-refusals");
    let past = now_millis() - 1;
    state
        .store
        .insert_scheduled_task(&ScheduledTask {
            id: "ran".to_string(),
            console_id: "console-1".to_string(),
            project_id: None,
            launch: ScheduledTaskLaunch::default(),
            prompt: "Run".to_string(),
            schedule: Schedule::Once { run_at: past },
            enabled: true,
            created_at: 1,
            updated_at: 1,
            next_run_at: None,
            last_run_at: Some(past),
            run_sessions: vec![],
        })
        .unwrap();

    let renamed = saved_task(
        update_scheduled_task(
            &state,
            None,
            "ran",
            ScheduledTaskChanges {
                prompt: Some("Renamed".to_string()),
                ..no_changes()
            },
        )
        .await
        .expect("an edit leaving the schedule alone"),
    );
    assert_eq!(renamed.next_run_at, None);
    assert_eq!(renamed.last_run_at, Some(past));

    let err = update_scheduled_task(
        &state,
        None,
        "ran",
        ScheduledTaskChanges {
            schedule: Some(Schedule::Once { run_at: past }),
            ..no_changes()
        },
    )
    .await
    .expect_err("a passed time");
    assert_refused(
        &err,
        error_code::SCHEDULED_TIME_PASSED,
        "the one-off time is not in the future",
    );

    let err = update_scheduled_task(
        &state,
        None,
        "ran",
        ScheduledTaskChanges {
            project_id: Some(Some("missing".to_string())),
            ..no_changes()
        },
    )
    .await
    .expect_err("an unknown project");
    assert_eq!(code_of(&err), error_code::UNKNOWN_PROJECT, "{err:#}");

    let err = update_scheduled_task(&state, None, "nope", no_changes())
        .await
        .expect_err("unknown");
    assert_refused(
        &err,
        error_code::UNKNOWN_SCHEDULED_TASK,
        "unknown scheduled task nope",
    );
    assert_eq!(
        state.store.require_scheduled_task("ran").unwrap().prompt,
        "Renamed"
    );
}

/// Turning a task off clears its next run and keeps it off through an edit; turning it on computes
/// it again from now. Each change is announced.
#[tokio::test]
async fn turning_a_task_off_and_on_clears_and_recomputes_its_next_run() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-enabled");
    let task = create(&state, new_task("Run", daily())).await.unwrap();
    let mut events = state.subscribe();

    let off = saved_task(set_scheduled_task_enabled(&state, None, &task.id, false).unwrap());
    assert!(!off.enabled);
    assert_eq!(off.next_run_at, None);
    let edited = saved_task(
        update_scheduled_task(
            &state,
            None,
            &task.id,
            ScheduledTaskChanges {
                prompt: Some("Edited".to_string()),
                ..no_changes()
            },
        )
        .await
        .unwrap(),
    );
    assert!(!edited.enabled);
    assert_eq!(edited.next_run_at, None);

    let on = saved_task(set_scheduled_task_enabled(&state, None, &task.id, true).unwrap());
    assert!(on.enabled);
    assert_eq!(
        on.next_run_at,
        next_run(&daily(), on.updated_at, &local_zone())
    );
    assert_eq!(state.store.require_scheduled_task(&task.id).unwrap(), on);
    assert_eq!(task_events(&mut events).len(), 3);

    let err = set_scheduled_task_enabled(&state, None, "nope", true).expect_err("unknown");
    assert_refused(
        &err,
        error_code::UNKNOWN_SCHEDULED_TASK,
        "unknown scheduled task nope",
    );
}

/// Deleting a task announces it gone; a second delete is refused.
#[tokio::test]
async fn deleting_a_task_announces_it_and_refuses_it_twice() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-delete");
    let task = create(&state, new_task("Run", daily())).await.unwrap();
    let mut events = state.subscribe();
    delete_scheduled_task(&state, &task.id).expect("deleted");
    assert!(state.store.get_scheduled_task(&task.id).unwrap().is_none());
    assert!(matches!(
        task_events(&mut events).as_slice(),
        [Event::ScheduledTaskDeleted { task: id }] if *id == task.id
    ));
    let err = delete_scheduled_task(&state, &task.id).expect_err("gone");
    assert_refused(
        &err,
        error_code::UNKNOWN_SCHEDULED_TASK,
        &format!("unknown scheduled task {}", task.id),
    );
    assert!(task_events(&mut events).is_empty());
}

/// A one-off in the future is stored with its own time as the next run.
#[tokio::test]
async fn a_one_off_in_the_future_is_due_at_its_time() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-once-future");
    let run_at = now_millis() + 60 * 60 * 1000;
    let task = create(&state, new_task("Run", Schedule::Once { run_at }))
        .await
        .expect("created");
    assert_eq!(task.schedule, Schedule::Once { run_at });
    assert_eq!(task.next_run_at, Some(run_at));
}

/// An account whose quota is used up or that is not logged in is not refused when the task is
/// saved: whether it can open a session is a matter of the moment of the run.
#[tokio::test]
async fn a_used_up_or_signed_out_account_is_not_refused_at_save_time() {
    use crate::protocol::{
        quota_unavailable_code, AccountQuota, QuotaUnavailable, QuotaWindow, QuotaWindowKind,
    };

    let (state, _dir) = console_with_project_and_accounts("scheduled-unusable-account");
    let config_dir = state
        .store
        .list_accounts()
        .unwrap()
        .into_iter()
        .find(|account| account.id == "claude-a")
        .expect("the claude account")
        .config_dir;
    let reading = |unavailable| AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: Some("claude-a".to_string()),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent: 100.0,
            resets_at: Some(now_millis() + 60_000),
            window_minutes: None,
        }],
        unavailable,
    };
    let new = || NewScheduledTask {
        launch: Some(ScheduledTaskLaunch {
            agent: Some(AgentId::CLAUDE),
            account: Some(Some("claude-a".to_string())),
            ..Default::default()
        }),
        ..new_task("Run", daily())
    };

    state
        .account_quotas
        .hold_in_dir_for_test(&reading(None), &config_dir);
    let used_up = create(&state, new()).await.expect("a used-up account");
    assert_eq!(used_up.launch.account, Some(Some("claude-a".to_string())));

    state.account_quotas.hold_in_dir_for_test(
        &reading(Some(QuotaUnavailable::new(
            quota_unavailable_code::NOT_SIGNED_IN,
            "x",
            &[("agent", "Claude Code")],
        ))),
        &config_dir,
    );
    let signed_out = create(&state, new()).await.expect("a signed-out account");
    assert_eq!(
        signed_out.launch.account,
        Some(Some("claude-a".to_string()))
    );
}

/// A changed schedule moves the next run to the new schedule's, from now; the refusals of a create
/// apply to an update too (a blank prompt, launch settings `open_session` would refuse).
#[tokio::test]
async fn an_update_recomputes_the_next_run_and_is_refused_as_a_create_is() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-update-checks");
    let task = create(&state, new_task("Run", daily())).await.unwrap();

    let hourly = Schedule::Hourly { minute: 30 };
    let updated = saved_task(
        update_scheduled_task(
            &state,
            None,
            &task.id,
            ScheduledTaskChanges {
                schedule: Some(hourly.clone()),
                ..no_changes()
            },
        )
        .await
        .expect("updated"),
    );
    assert_eq!(updated.schedule, hourly);
    assert_eq!(
        updated.next_run_at,
        next_run(&hourly, updated.updated_at, &local_zone())
    );
    assert!(updated.next_run_at.is_some());

    let blank = update_scheduled_task(
        &state,
        None,
        &task.id,
        ScheduledTaskChanges {
            prompt: Some("  ".to_string()),
            ..no_changes()
        },
    )
    .await
    .expect_err("a blank prompt");
    assert_refused(&blank, error_code::FIELD_REQUIRED, "`prompt` is required");

    let unknown_account = update_scheduled_task(
        &state,
        None,
        &task.id,
        ScheduledTaskChanges {
            launch: Some(ScheduledTaskLaunch {
                agent: Some(AgentId::CLAUDE),
                account: Some(Some("codex-a".to_string())),
                ..Default::default()
            }),
            ..no_changes()
        },
    )
    .await
    .expect_err("an account of another agent");
    assert_refused(&unknown_account, error_code::UNKNOWN_ACCOUNT, "codex-a");

    let stored = state.store.require_scheduled_task(&task.id).unwrap();
    assert_eq!(stored, updated);
}

/// Moving a project task to a console drops the console session it reported to, which only a
/// project task has.
#[tokio::test]
async fn moving_a_task_to_a_console_drops_its_report_to() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-move-to-console");
    state
        .store
        .insert_session(&bare_session("hub", SessionKind::Console))
        .unwrap();
    let task = create(
        &state,
        NewScheduledTask {
            project_id: Some("project-1".to_string()),
            launch: Some(ScheduledTaskLaunch {
                bound_to: Some("hub".to_string()),
                ..Default::default()
            }),
            ..new_task("Run", daily())
        },
    )
    .await
    .unwrap();
    assert_eq!(task.launch.bound_to.as_deref(), Some("hub"));

    let moved = saved_task(
        update_scheduled_task(
            &state,
            None,
            &task.id,
            ScheduledTaskChanges {
                console_id: Some("console-1".to_string()),
                project_id: Some(None),
                ..no_changes()
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(moved.project_id, None);
    assert_eq!(moved.launch.bound_to, None);
}

/// A task created with `enabled` false is stored and announced disabled, with no next run, which
/// turning it on then computes from now; one created without a choice is enabled (see the create
/// test above).
#[tokio::test]
async fn a_task_created_disabled_has_no_next_run_until_it_is_turned_on() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-create-disabled");
    let mut events = state.subscribe();
    let task = create(
        &state,
        NewScheduledTask {
            enabled: false,
            ..new_task("Run", daily())
        },
    )
    .await
    .expect("created");

    assert!(!task.enabled);
    assert_eq!(task.next_run_at, None);
    assert_eq!(task.schedule, daily());
    assert_eq!(
        state.store.get_scheduled_task(&task.id).unwrap(),
        Some(task.clone())
    );
    assert!(matches!(
        task_events(&mut events).as_slice(),
        [Event::ScheduledTaskUpserted { task: announced }] if *announced == task
    ));

    let on = saved_task(set_scheduled_task_enabled(&state, None, &task.id, true).unwrap());
    assert!(on.enabled);
    assert!(on.next_run_at.is_some());
}

/// `project_id` of an update is tri-state: absent keeps the project, `null` makes a console task
/// (dropping the console session it reported to), a project id makes a project task, and each
/// result is checked against the console it ends up in.
#[tokio::test]
async fn an_updates_project_id_keeps_clears_or_sets_the_project() {
    let (state, dir) = console_with_project_and_accounts("scheduled-update-project");
    state
        .store
        .insert_console(&crate::protocol::Console {
            id: "console-2".to_string(),
            workdir: dir.join("other").to_string_lossy().into_owned(),
            ..crate::test_support::console(&dir)
        })
        .unwrap();
    state
        .store
        .insert_session(&bare_session("hub", SessionKind::Console))
        .unwrap();
    let task = create(
        &state,
        NewScheduledTask {
            project_id: Some("project-1".to_string()),
            launch: Some(ScheduledTaskLaunch {
                bound_to: Some("hub".to_string()),
                ..Default::default()
            }),
            ..new_task("Run", daily())
        },
    )
    .await
    .unwrap();
    let update = |changes: ScheduledTaskChanges| {
        let (state, id) = (state.clone(), task.id.clone());
        async move {
            update_scheduled_task(&state, None, &id, changes)
                .await
                .map(saved_task)
        }
    };

    // Absent keeps the project, and what the task reports to.
    let kept = update(ScheduledTaskChanges {
        prompt: Some("Another".to_string()),
        ..no_changes()
    })
    .await
    .unwrap();
    assert_eq!(kept.project_id.as_deref(), Some("project-1"));
    assert_eq!(kept.launch.bound_to.as_deref(), Some("hub"));

    // Another console alone cannot keep a project that is not its own.
    let err = update(ScheduledTaskChanges {
        console_id: Some("console-2".to_string()),
        ..no_changes()
    })
    .await
    .expect_err("the project belongs to console-1");
    assert_refused(
        &err,
        error_code::UNKNOWN_PROJECT,
        "unknown project project-1",
    );
    let stored = state.store.require_scheduled_task(&task.id).unwrap();
    assert_eq!(stored.console_id, "console-1");
    assert_eq!(stored.project_id.as_deref(), Some("project-1"));

    // `null` makes a console task of the same console, with no console session to report to.
    let console_task = update(ScheduledTaskChanges {
        project_id: Some(None),
        ..no_changes()
    })
    .await
    .unwrap();
    assert_eq!(console_task.project_id, None);
    assert_eq!(console_task.launch.bound_to, None);
    assert_eq!(console_task.console_id, "console-1");

    // A project id turns the console task back into a project task, `console_id` left out.
    let project_task = update(ScheduledTaskChanges {
        project_id: Some(Some("project-1".to_string())),
        ..no_changes()
    })
    .await
    .unwrap();
    assert_eq!(project_task.project_id.as_deref(), Some("project-1"));
    assert_eq!(project_task.console_id, "console-1");

    // An unknown project is refused, and a project of another console than the result's.
    let err = update(ScheduledTaskChanges {
        project_id: Some(Some("missing".to_string())),
        ..no_changes()
    })
    .await
    .expect_err("an unknown project");
    assert_refused(&err, error_code::UNKNOWN_PROJECT, "unknown project missing");
    let err = update(ScheduledTaskChanges {
        console_id: Some("console-2".to_string()),
        project_id: Some(Some("project-1".to_string())),
        ..no_changes()
    })
    .await
    .expect_err("the project belongs to console-1");
    assert_refused(
        &err,
        error_code::UNKNOWN_PROJECT,
        "unknown project project-1",
    );
    assert_eq!(
        state.store.require_scheduled_task(&task.id).unwrap(),
        project_task
    );
}

/// A task and an edit of its prompt held between the edit's read and its write (see `EDIT_PAUSE`):
/// the edit has read the task and checked its launch settings, and waits for `resume`. The test
/// acts, then wakes the edit with `resume.notify_one()`.
async fn task_with_a_held_edit(
    state: &Arc<AppState>,
) -> (
    ScheduledTask,
    tokio::task::JoinHandle<Result<Option<Event>>>,
    Arc<tokio::sync::Notify>,
) {
    let task = create(state, new_task("Run", daily()))
        .await
        .expect("created");
    let reached = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    EDIT_PAUSE.with(|slot| {
        slot.replace(Some(EditPause {
            reached: reached.clone(),
            resume: resume.clone(),
        }))
    });
    EDIT_ATTEMPTS_MADE.with(|made| made.set(0));
    let mut edit = tokio::spawn({
        let (state, id) = (state.clone(), task.id.clone());
        async move {
            update_scheduled_task(
                &state,
                None,
                &id,
                ScheduledTaskChanges {
                    prompt: Some("Edited".to_string()),
                    ..no_changes()
                },
            )
            .await
        }
    });
    tokio::select! {
        _ = reached.notified() => {}
        ended = &mut edit => panic!("the edit ended before it was held: {ended:?}"),
    }
    (task, edit, resume)
}

/// How many attempts the held edit made.
fn edit_attempts() -> usize {
    EDIT_ATTEMPTS_MADE.with(std::cell::Cell::get)
}

/// An edit never gives an occurrence twice: the scheduler takes the occurrence while the edit
/// checks the launch settings, and the edit leaves the next run the scheduler moved on to.
#[tokio::test]
async fn an_edit_keeps_the_next_run_a_scheduler_take_moved_on_to() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-edit-take");
    let (task, edit, resume) = task_with_a_held_edit(&state).await;
    let due = task.next_run_at.expect("a next run");
    let next = due + 24 * 60 * 60 * 1000;
    let taken = state
        .store
        .take_scheduled_occurrence(&task.id, due, Some(next))
        .unwrap();
    assert!(taken.is_some(), "the occurrence was taken");

    resume.notify_one();
    let edited = saved_task(edit.await.unwrap().expect("edited"));

    assert_eq!(edit_attempts(), 1);
    assert_eq!(edited.prompt, "Edited");
    assert_eq!(edited.next_run_at, Some(next));
    assert_eq!(
        state
            .store
            .require_scheduled_task(&task.id)
            .unwrap()
            .next_run_at,
        Some(next)
    );
}

/// An edit never changes `enabled`: a task another request turned off while the edit checked the
/// launch settings stays off, with no next run, once the edit lands.
#[tokio::test]
async fn an_edit_leaves_a_task_turned_off_meanwhile_off() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-edit-disable");
    let (task, edit, resume) = task_with_a_held_edit(&state).await;
    let off = saved_task(set_scheduled_task_enabled(&state, None, &task.id, false).unwrap());
    assert!(!off.enabled);

    resume.notify_one();
    let edited = saved_task(edit.await.unwrap().expect("edited"));

    assert_eq!(edit_attempts(), 1);
    assert_eq!(edited.prompt, "Edited");
    assert!(!edited.enabled);
    assert_eq!(edited.next_run_at, None);
    let stored = state.store.require_scheduled_task(&task.id).unwrap();
    assert!(!stored.enabled);
    assert_eq!(stored.next_run_at, None);
}

/// Edits of different fields that overlap both land: while an edit of the prompt checks the launch
/// settings, an edit of the schedule lands; the first is made again from a fresh read, so the
/// stored task has both, the broadcast carries the merged task and the next run follows the new
/// schedule.
#[tokio::test]
async fn overlapping_edits_of_different_fields_both_land() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-edit-overlap");
    let (task, edit, resume) = task_with_a_held_edit(&state).await;
    let mut events = state.subscribe();
    let hourly = Schedule::Hourly { minute: 30 };
    // The other edit, as `update_scheduled_task` makes it, landing through the store.
    let read = state.store.require_scheduled_task(&task.id).unwrap();
    let now = now_millis();
    let other = ScheduledTask {
        schedule: hourly.clone(),
        updated_at: now,
        ..read.clone()
    };
    let landed = state
        .store
        .edit_scheduled_task(&other, &read, |schedule| {
            next_run(schedule, now, &local_zone())
        })
        .unwrap();
    assert!(matches!(landed, ScheduledTaskEdit::Edited(_)));

    resume.notify_one();
    let edited = saved_task(edit.await.unwrap().expect("edited"));

    assert_eq!(
        edit_attempts(),
        2,
        "the edit was made again from a fresh read"
    );
    assert_eq!(edited.prompt, "Edited");
    assert_eq!(edited.schedule, hourly);
    assert_eq!(edited.launch, task.launch);
    let expected_next = state
        .store
        .require_scheduled_task(&task.id)
        .unwrap()
        .next_run_at;
    assert_eq!(edited.next_run_at, expected_next);
    assert_eq!(edited.next_run_at, next_run(&hourly, now, &local_zone()));
    assert_eq!(
        state.store.require_scheduled_task(&task.id).unwrap(),
        edited
    );
    assert!(matches!(
        task_events(&mut events).as_slice(),
        [Event::ScheduledTaskUpserted { task: announced }] if *announced == edited
    ));
}

/// The console session a task reports to only has to be one of its console that the daemon holds:
/// a task naming one that is archived or still starting is saved, and its other fields stay
/// editable, since whether it can take a session is decided when the task runs.
#[tokio::test]
async fn a_report_to_session_may_be_archived_or_starting_when_saved() {
    let (state, _dir) = console_with_project_and_accounts("scheduled-report-to-status");
    for (id, status) in [
        ("archived-hub", SessionStatus::Archived),
        ("starting-hub", SessionStatus::Starting),
    ] {
        let mut hub = bare_session(id, SessionKind::Console);
        hub.status = status;
        state.store.insert_session(&hub).unwrap();
        let task = create(
            &state,
            NewScheduledTask {
                project_id: Some("project-1".to_string()),
                launch: Some(ScheduledTaskLaunch {
                    bound_to: Some(id.to_string()),
                    ..Default::default()
                }),
                ..new_task("Run", daily())
            },
        )
        .await
        .unwrap_or_else(|err| panic!("{id}: {err:#}"));
        assert_eq!(task.launch.bound_to.as_deref(), Some(id));
        let edited = saved_task(
            update_scheduled_task(
                &state,
                None,
                &task.id,
                ScheduledTaskChanges {
                    prompt: Some("Edited".to_string()),
                    ..no_changes()
                },
            )
            .await
            .unwrap_or_else(|err| panic!("{id}: {err:#}")),
        );
        assert_eq!(edited.prompt, "Edited");
        assert_eq!(edited.launch.bound_to.as_deref(), Some(id));
    }
}
