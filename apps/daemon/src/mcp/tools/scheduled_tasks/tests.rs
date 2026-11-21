use super::*;
use crate::mcp::dispatch::call;
use crate::mcp::test_support::{arguments, shared_console_state, wait_for_request};
use crate::protocol::error_code;
use crate::protocol::{
    ScheduledRunOutcome, ScheduledRunTrigger, ScheduledTaskRun, SessionRequestEnding, TimeOfDay,
};
use crate::session_request;
use crate::session_request::Edits;
use crate::test_support::{code_of, offer_models, running, PATIENCE};
use crate::time::now_millis;
use serde_json::json;

fn daily() -> Schedule {
    Schedule::Daily {
        time: TimeOfDay { hour: 9, minute: 0 },
    }
}

/// A stored task of `console` and `project`, whose prompt is `prompt`.
fn stored(state: &Arc<AppState>, id: &str, console: &str, project: Option<&str>, prompt: &str) {
    let now = now_millis();
    state
        .store
        .insert_scheduled_task(&ScheduledTask {
            id: id.to_string(),
            console_id: console.to_string(),
            project_id: project.map(str::to_string),
            launch: ScheduledTaskLaunch::default(),
            prompt: prompt.to_string(),
            schedule: daily(),
            enabled: true,
            created_at: now,
            updated_at: now,
            next_run_at: None,
            last_run_at: None,
            run_sessions: Vec::new(),
        })
        .unwrap();
}

/// The tasks of console 1 (a console task and a task of its project) and of console 2.
fn three_tasks(state: &Arc<AppState>) {
    stored(state, "t-console", "console-1", None, "console work");
    stored(
        state,
        "t-project",
        "console-1",
        Some("project-1"),
        "project work",
    );
    stored(
        state,
        "t-foreign",
        "console-2",
        Some("project-2"),
        "foreign work",
    );
}

fn ids(listed: &Value) -> Vec<&str> {
    listed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap())
        .collect()
}

/// The requests of `session` that are waiting, oldest first.
fn waiting_of(state: &Arc<AppState>, session: &str) -> Vec<Event> {
    session_request::pending_events(state)
        .into_iter()
        .filter(|event| matches!(event, Event::SessionRequest { session: s, .. } if s == session))
        .collect()
}

async fn nobody_waits(state: &Arc<AppState>, session: &str) {
    tokio::task::yield_now().await;
    assert!(waiting_of(state, session).is_empty());
}

/// A worker is offered none of the tools, and the daemon holds to it.
#[tokio::test]
async fn a_worker_has_none_of_the_scheduled_task_tools() {
    let (state, _dir) = shared_console_state("scheduled-worker");
    three_tasks(&state);
    for tool in [
        "list_scheduled_tasks",
        "list_scheduled_task_runs",
        "create_scheduled_task",
        "update_scheduled_task",
        "set_scheduled_task_enabled",
        "delete_scheduled_task",
        "run_scheduled_task",
    ] {
        let err = call(
            &state,
            "of-a",
            tool,
            &arguments(json!({ "task": "t-project" })),
        )
        .await
        .expect_err("not offered");
        assert!(err.to_string().contains("is not a tool"), "{tool}: {err}");
    }
}

/// A console session reads the tasks of its console, console tasks and its projects' alike, and a
/// coordinator only those of its own project; another console's are in neither.
#[tokio::test]
async fn the_reads_show_only_the_tasks_within_the_callers_reach() {
    let (state, _dir) = shared_console_state("scheduled-reads");
    three_tasks(&state);

    let console = call(&state, "a", "list_scheduled_tasks", &arguments(json!({})))
        .await
        .unwrap();
    let mut listed = ids(&console);
    listed.sort_unstable();
    assert_eq!(listed, ["t-console", "t-project"]);
    assert!(console["time_zone"].is_string() || console["time_zone"].is_null());
    let task = &console["tasks"][0];
    for field in [
        "id",
        "console",
        "project",
        "launch",
        "prompt",
        "schedule",
        "enabled",
        "next_run_at",
        "last_run_at",
        "created_at",
        "updated_at",
        "run_sessions",
    ] {
        assert!(task.get(field).is_some(), "{field}");
    }

    let coordinator = call(
        &state,
        "loose",
        "list_scheduled_tasks",
        &arguments(json!({})),
    )
    .await
    .unwrap();
    assert_eq!(ids(&coordinator), ["t-project"]);

    let runs = call(
        &state,
        "loose",
        "list_scheduled_task_runs",
        &arguments(json!({ "task": "t-project" })),
    )
    .await
    .unwrap();
    assert_eq!(runs["task"], "t-project");
    assert_eq!(runs["runs"], json!([]));
    for (caller, task) in [("loose", "t-console"), ("a", "t-foreign")] {
        let err = call(
            &state,
            caller,
            "list_scheduled_task_runs",
            &arguments(json!({ "task": task })),
        )
        .await
        .expect_err("out of reach");
        assert!(err.to_string().contains("no scheduled task"), "{err}");
    }
}

/// A task outside the caller's reach is refused alike whether it exists or not, by every tool,
/// and a coordinator can neither create a console task nor move a task out of its project: none of
/// it asks the user.
#[tokio::test]
async fn a_task_outside_the_callers_reach_is_refused_without_asking() {
    let (state, _dir) = shared_console_state("scheduled-scope");
    three_tasks(&state);
    running(&state, "a");
    running(&state, "loose");

    for (caller, task) in [
        ("loose", "t-console"),
        ("loose", "t-foreign"),
        ("a", "t-foreign"),
        ("a", "no-such-task"),
    ] {
        for (tool, extra) in [
            ("update_scheduled_task", json!({ "prompt": "x" })),
            ("set_scheduled_task_enabled", json!({ "enabled": false })),
            ("delete_scheduled_task", json!({})),
            ("run_scheduled_task", json!({})),
        ] {
            let mut input = extra;
            input["task"] = json!(task);
            let err = call(&state, caller, tool, &arguments(input))
                .await
                .expect_err("out of reach");
            assert!(
                err.to_string().contains("no scheduled task"),
                "{caller} {tool} {task}: {err}"
            );
            nobody_waits(&state, caller).await;
        }
    }

    let create = |extra: Value| {
        let mut input = json!({ "prompt": "x", "schedule": { "kind": "manual" } });
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        arguments(input)
    };
    for input in [
        create(json!({ "project": null })),
        create(json!({ "project": "project-2" })),
    ] {
        let err = call(&state, "loose", "create_scheduled_task", &input)
            .await
            .expect_err("not its project");
        assert!(err.to_string().contains("own project"), "{err}");
    }
    // A console session's project is found among its own console's.
    let err = call(
        &state,
        "a",
        "create_scheduled_task",
        &create(json!({ "project": "project-2" })),
    )
    .await
    .expect_err("another console's project");
    assert!(err.to_string().contains("no project"), "{err}");
    for project in [json!(null), json!("project-2")] {
        let err = call(
            &state,
            "loose",
            "update_scheduled_task",
            &arguments(json!({ "task": "t-project", "project": project })),
        )
        .await
        .expect_err("cannot be moved");
        assert!(err.to_string().contains("own project"), "{err}");
    }
    nobody_waits(&state, "loose").await;
    nobody_waits(&state, "a").await;
    assert_eq!(state.store.list_scheduled_tasks().unwrap().len(), 3);
}

/// What the operation would refuse is refused before the user is asked, and nothing is stored.
#[tokio::test]
async fn what_could_never_be_carried_out_is_refused_before_the_user_is_asked() {
    let (state, _dir) = shared_console_state("scheduled-pre-ask");
    three_tasks(&state);
    running(&state, "a");
    let past = now_millis() - 60_000;

    for (tool, input, reason) in [
        (
            "create_scheduled_task",
            json!({ "prompt": "  ", "schedule": { "kind": "manual" } }),
            "prompt",
        ),
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "once", "run_at": past } }),
            "not in the future",
        ),
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "hourly", "minute": 60 } }),
            "out of range",
        ),
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "weekly", "time": { "hour": 9, "minute": 0 }, "days": [] } }),
            "days",
        ),
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "sometimes" } }),
            "not a schedule",
        ),
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "manual" }, "launch": { "bound_to": "of-a" }, "project": "project-1" }),
            "of-a",
        ),
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "manual" }, "launch": { "account": "nobody" } }),
            "nobody",
        ),
        (
            "update_scheduled_task",
            json!({ "task": "t-project", "prompt": " " }),
            "prompt",
        ),
        (
            "update_scheduled_task",
            json!({ "task": "t-project", "schedule": { "kind": "once", "run_at": past } }),
            "not in the future",
        ),
        (
            "update_scheduled_task",
            json!({ "task": "t-project" }),
            "name what to change",
        ),
    ] {
        let err = call(&state, "a", tool, &arguments(input.clone()))
            .await
            .expect_err("refused");
        assert!(err.to_string().contains(reason), "{input}: {err}");
        nobody_waits(&state, "a").await;
    }
    assert_eq!(state.store.list_scheduled_tasks().unwrap().len(), 3);
    assert_eq!(
        state
            .store
            .require_scheduled_task("t-project")
            .unwrap()
            .prompt,
        "project work"
    );
}

/// With the setting off a create waits as a request telling the user the whole task, and an
/// approval stores it and returns it as stored.
#[tokio::test]
async fn a_create_waits_for_the_user_and_an_approval_stores_the_task() {
    let (state, _dir) = shared_console_state("scheduled-create-asked");
    running(&state, "a");
    let waiting = {
        let state = state.clone();
        tokio::spawn(async move {
            call(
                &state,
                "a",
                "create_scheduled_task",
                &arguments(json!({
                    "prompt": "  Check the build  ",
                    "schedule": { "kind": "daily", "time": { "hour": 7, "minute": 30 } },
                    "project": "project-1",
                    "enabled": false,
                    "launch": { "title": "Nightly", "agent": "codex", "model": "" },
                })),
            )
            .await
        })
    };
    let Event::SessionRequest {
        request_id,
        session,
        action,
        kind,
        console,
        project,
        agent,
        target,
        scheduled_task,
        prompt,
        account,
        branches,
        ..
    } = wait_for_request(&state, "a").await
    else {
        panic!("a session request");
    };
    assert_eq!(session, "a");
    assert_eq!(action, SessionRequestAction::CreateScheduledTask);
    assert_eq!(kind, SessionKind::Project);
    assert_eq!(console, "console-1");
    assert_eq!(project.as_deref(), Some("project-1"));
    // The agent the task names, not the requester's.
    assert_eq!(agent, AgentId::CODEX);
    assert_eq!((target, account), (None, None));
    assert!(branches.is_empty());
    // The prompt is the agent's text as written, in the event's own field.
    assert_eq!(prompt.as_deref(), Some("  Check the build  "));
    let asked = scheduled_task.expect("what is asked");
    assert_eq!(asked.task, None);
    assert_eq!(asked.enabled, Some(false));
    assert_eq!(
        asked.schedule,
        Some(Schedule::Daily {
            time: TimeOfDay {
                hour: 7,
                minute: 30
            }
        })
    );
    let launch = asked.launch.expect("launch");
    assert_eq!(launch.title.as_deref(), Some("Nightly"));
    assert_eq!(launch.agent, Some(AgentId::CODEX));
    assert_eq!(launch.model, Some(None));
    assert!(state.store.list_scheduled_tasks().unwrap().is_empty());
    assert!(state.session_requests.is_waiting_for("a"));

    let answered = session_request::answer(&state, &request_id, true, Edits::default())
        .await
        .unwrap();
    assert!(answered.is_none(), "no session is started or moved");
    let result = waiting.await.unwrap().unwrap();
    let stored = state.store.list_scheduled_tasks().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].prompt, "Check the build");
    assert!(!stored[0].enabled);
    assert_eq!(result["task"]["id"], stored[0].id);
    assert_eq!(result["task"]["project"], "project-1");
    assert!(result["note"].as_str().unwrap().contains(&stored[0].id));
}

/// A refusal changes nothing and the call is told so.
#[tokio::test]
async fn a_refused_request_changes_nothing() {
    let (state, _dir) = shared_console_state("scheduled-refused");
    three_tasks(&state);
    running(&state, "a");
    let waiting = {
        let state = state.clone();
        tokio::spawn(async move {
            call(
                &state,
                "a",
                "delete_scheduled_task",
                &arguments(json!({ "task": "t-project" })),
            )
            .await
        })
    };
    let Event::SessionRequest {
        request_id,
        action,
        scheduled_task,
        prompt,
        project,
        ..
    } = wait_for_request(&state, "a").await
    else {
        panic!("a session request");
    };
    assert_eq!(action, SessionRequestAction::DeleteScheduledTask);
    assert_eq!(project.as_deref(), Some("project-1"));
    assert_eq!(prompt, None);
    assert_eq!(scheduled_task.unwrap().task.as_deref(), Some("t-project"));
    session_request::answer(&state, &request_id, false, Edits::default())
        .await
        .unwrap();
    let err = waiting.await.unwrap().expect_err("refused");
    assert!(err.to_string().contains("the user refused"), "{err}");
    assert!(err.to_string().contains("nothing was changed"), "{err}");
    assert!(state
        .store
        .get_scheduled_task("t-project")
        .unwrap()
        .is_some());
}

/// An update's request carries the task and only what the call changes, and its new prompt when
/// it changes one; turning a task off and running one carry the task (and `enabled`).
#[tokio::test]
async fn each_write_tells_the_user_what_it_changes() {
    let (state, _dir) = shared_console_state("scheduled-events");
    three_tasks(&state);
    running(&state, "a");
    let ask = |tool: &'static str, input: Value| {
        let state = state.clone();
        tokio::spawn(async move { call(&state, "a", tool, &arguments(input)).await })
    };
    let first_request = |state: &Arc<AppState>| waiting_of(state, "a").into_iter().next();

    let update = ask(
        "update_scheduled_task",
        json!({ "task": "t-project", "schedule": { "kind": "manual" }, "project": null }),
    );
    let Event::SessionRequest {
        request_id,
        action,
        scheduled_task,
        prompt,
        project,
        kind,
        ..
    } = wait_for_request(&state, "a").await
    else {
        panic!("a session request");
    };
    assert_eq!(action, SessionRequestAction::UpdateScheduledTask);
    let asked = scheduled_task.unwrap();
    assert_eq!(asked.task.as_deref(), Some("t-project"));
    assert_eq!(asked.schedule, Some(Schedule::Manual));
    assert_eq!((asked.launch, asked.enabled), (None, None));
    // No new prompt, so none is shown; the task's target is the one after the change.
    assert_eq!(prompt, None);
    assert_eq!((project, kind), (None, SessionKind::Console));
    session_request::answer(&state, &request_id, false, Edits::default())
        .await
        .unwrap();
    update.await.unwrap().expect_err("refused");

    let update = ask(
        "update_scheduled_task",
        json!({ "task": "t-project", "prompt": "new work" }),
    );
    let Event::SessionRequest {
        prompt, project, ..
    } = wait_for_request(&state, "a").await
    else {
        panic!("a session request");
    };
    assert_eq!(prompt.as_deref(), Some("new work"));
    assert_eq!(project.as_deref(), Some("project-1"));
    update.abort();

    let enable = ask(
        "set_scheduled_task_enabled",
        json!({ "task": "t-console", "enabled": false }),
    );
    let Some(Event::SessionRequest {
        action,
        scheduled_task,
        ..
    }) = poll(|| {
        first_request(&state).filter(|event| {
            matches!(
                event,
                Event::SessionRequest {
                    action: SessionRequestAction::SetScheduledTaskEnabled,
                    ..
                }
            )
        })
    })
    .await
    else {
        panic!("a request");
    };
    assert_eq!(action, SessionRequestAction::SetScheduledTaskEnabled);
    let asked = scheduled_task.unwrap();
    assert_eq!(asked.task.as_deref(), Some("t-console"));
    assert_eq!(asked.enabled, Some(false));
    enable.abort();
}

/// `found`'s answer once it has one, polled for no longer than `PATIENCE`.
async fn poll<T>(mut found: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        if let Some(found) = found() {
            return Some(found);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

/// A session asking again of the same tool for the same task takes the place of its earlier
/// request, while another task's change, and another tool's for the same task, wait beside it.
#[tokio::test]
async fn a_request_replaces_only_the_same_tool_for_the_same_task() {
    let (state, _dir) = shared_console_state("scheduled-replace");
    three_tasks(&state);
    running(&state, "a");
    let ask = |tool: &'static str, input: Value| {
        let state = state.clone();
        tokio::spawn(async move { call(&state, "a", tool, &arguments(input)).await })
    };
    let waiting = |count: usize| {
        let state = state.clone();
        async move { poll(|| (waiting_of(&state, "a").len() == count).then_some(())).await }
    };

    let first = ask(
        "update_scheduled_task",
        json!({ "task": "t-project", "prompt": "one" }),
    );
    assert!(waiting(1).await.is_some());
    let other_task = ask(
        "update_scheduled_task",
        json!({ "task": "t-console", "prompt": "two" }),
    );
    assert!(waiting(2).await.is_some());
    let other_tool = ask("delete_scheduled_task", json!({ "task": "t-project" }));
    assert!(waiting(3).await.is_some());

    let again = ask(
        "update_scheduled_task",
        json!({ "task": "t-project", "prompt": "three" }),
    );
    let err = first.await.unwrap().expect_err("replaced");
    assert!(err.to_string().contains("withdrawn"), "{err}");
    assert!(waiting(3).await.is_some());
    let prompts: Vec<_> = waiting_of(&state, "a")
        .into_iter()
        .map(|event| match event {
            Event::SessionRequest { prompt, action, .. } => (action, prompt),
            _ => unreachable!(),
        })
        .collect();
    assert!(prompts.contains(&(
        SessionRequestAction::UpdateScheduledTask,
        Some("three".to_string())
    )));
    assert!(!prompts.contains(&(
        SessionRequestAction::UpdateScheduledTask,
        Some("one".to_string())
    )));
    for call in [again, other_task, other_tool] {
        call.abort();
    }
}

/// With the user's standing approval the operation runs at once: no request, and a waiting one of
/// the same tool for the same task is withdrawn.
#[tokio::test]
async fn with_the_setting_on_the_operation_runs_at_once() {
    let (state, _dir) = shared_console_state("scheduled-automatic");
    three_tasks(&state);
    running(&state, "loose");

    let waiting = {
        let state = state.clone();
        tokio::spawn(async move {
            call(
                &state,
                "loose",
                "set_scheduled_task_enabled",
                &arguments(json!({ "task": "t-project", "enabled": false })),
            )
            .await
        })
    };
    wait_for_request(&state, "loose").await;
    state
        .store
        .set_auto_approve_scheduled_task_changes(true)
        .unwrap();
    let mut closed = state.subscribe();

    let off = call(
        &state,
        "loose",
        "set_scheduled_task_enabled",
        &arguments(json!({ "task": "t-project", "enabled": false })),
    )
    .await
    .unwrap();
    assert_eq!(off["task"]["enabled"], false);
    assert!(
        !state
            .store
            .require_scheduled_task("t-project")
            .unwrap()
            .enabled
    );
    let err = waiting.await.unwrap().expect_err("replaced");
    assert!(err.to_string().contains("withdrawn"), "{err}");
    assert!(waiting_of(&state, "loose").is_empty());
    let mut ending = None;
    while let Ok(event) = closed.try_recv() {
        if let Event::SessionRequestClosed { reason, .. } = event {
            ending = Some(reason);
        }
    }
    assert_eq!(ending, Some(SessionRequestEnding::Withdrawn));

    let created = call(
        &state,
        "loose",
        "create_scheduled_task",
        &arguments(json!({ "prompt": "own", "schedule": { "kind": "manual" } })),
    )
    .await
    .unwrap();
    assert_eq!(created["task"]["project"], "project-1");
    assert!(waiting_of(&state, "loose").is_empty());

    let deleted = call(
        &state,
        "loose",
        "delete_scheduled_task",
        &arguments(json!({ "task": "t-project" })),
    )
    .await
    .unwrap();
    assert_eq!(deleted["deleted"], "t-project");
    assert!(state
        .store
        .get_scheduled_task("t-project")
        .unwrap()
        .is_none());

    // The scope holds with the setting on.
    let err = call(
        &state,
        "loose",
        "delete_scheduled_task",
        &arguments(json!({ "task": "t-console" })),
    )
    .await
    .expect_err("out of reach");
    assert!(err.to_string().contains("no scheduled task"), "{err}");
    assert!(state
        .store
        .get_scheduled_task("t-console")
        .unwrap()
        .is_some());
}

/// The operation is checked again when it is carried out: a task deleted while the user was
/// deciding is not an approval's to change.
#[tokio::test]
async fn an_approval_checks_again_when_it_is_carried_out() {
    let (state, _dir) = shared_console_state("scheduled-recheck");
    three_tasks(&state);
    running(&state, "a");
    let waiting = {
        let state = state.clone();
        tokio::spawn(async move {
            call(
                &state,
                "a",
                "update_scheduled_task",
                &arguments(json!({ "task": "t-console", "prompt": "later" })),
            )
            .await
        })
    };
    let Event::SessionRequest { request_id, .. } = wait_for_request(&state, "a").await else {
        panic!("a session request");
    };
    state.store.delete_scheduled_task("t-console").unwrap();
    let err = session_request::answer(&state, &request_id, true, Edits::default())
        .await
        .expect_err("gone");
    assert!(err.to_string().contains("t-console"), "{err}");
    let err = waiting
        .await
        .unwrap()
        .expect_err("could not be carried out");
    assert!(
        err.to_string().contains("could not be carried out"),
        "{err}"
    );
}

/// A launch names its account by id or by name, among the agent's accounts, and a name that fits
/// accounts of several agents is refused unless the agent is named.
#[tokio::test]
async fn a_launch_account_is_read_by_id_or_name() {
    let (state, _dir) = shared_console_state("scheduled-account");
    for (id, agent) in [("c-1", AgentId::CLAUDE), ("x-1", AgentId::CODEX)] {
        state
            .store
            .insert_account(&crate::mcp::test_support::account(
                id,
                agent,
                "Work",
                format!("/tmp/{id}"),
            ))
            .unwrap();
    }
    state
        .store
        .insert_account(&crate::mcp::test_support::account(
            "c-2",
            AgentId::CLAUDE,
            "Solo",
            "/tmp/c-2",
        ))
        .unwrap();
    let account = |launch: Value| launch_from(&state, &launch).map(|launch| launch.account);
    assert_eq!(
        account(json!({ "account": "Solo" })).unwrap(),
        Some(Some("c-2".into()))
    );
    assert_eq!(
        account(json!({ "account": "x-1" })).unwrap(),
        Some(Some("x-1".into()))
    );
    assert_eq!(
        account(json!({ "agent": "codex", "account": "work" })).unwrap(),
        Some(Some("x-1".into()))
    );
    assert_eq!(
        account(json!({ "account": "default" })).unwrap(),
        Some(None)
    );
    assert_eq!(account(json!({ "account": null })).unwrap(), Some(None));
    assert_eq!(account(json!({})).unwrap(), None);
    let err = account(json!({ "account": "Work" })).expect_err("ambiguous");
    assert!(err.to_string().contains("several agents"), "{err}");
    assert!(account(json!({ "agent": "claude", "account": "x-1" })).is_err());
}

/// `tool` called by `caller` with `input`, on a task of its own.
fn spawn_call(
    state: &Arc<AppState>,
    caller: &'static str,
    tool: &'static str,
    input: Value,
) -> tokio::task::JoinHandle<Result<Value>> {
    let state = state.clone();
    tokio::spawn(async move { call(&state, caller, tool, &arguments(input)).await })
}

/// Approves the request `caller` is waiting with, and gives the session the approval started or
/// moved.
async fn approve(state: &Arc<AppState>, caller: &str) -> Option<Session> {
    let Event::SessionRequest { request_id, .. } = wait_for_request(state, caller).await else {
        panic!("a session request");
    };
    session_request::answer(state, &request_id, true, Edits::default())
        .await
        .unwrap()
}

/// What an opened session would refuse in the launch settings of a task is refused before the
/// user is asked, whether the task is being created or changed, and nothing is stored.
#[tokio::test]
async fn launch_settings_an_opened_session_would_refuse_are_refused_before_the_user_is_asked() {
    let (state, _dir) = shared_console_state("scheduled-launch-refusals");
    three_tasks(&state);
    running(&state, "a");
    for (id, agent, name) in [
        ("c-1", AgentId::CLAUDE, "Work"),
        ("c-off", AgentId::CLAUDE, "Off"),
        ("x-1", AgentId::CODEX, "Other"),
    ] {
        state
            .store
            .insert_account(&crate::mcp::test_support::account(
                id,
                agent,
                name,
                format!("/tmp/{id}"),
            ))
            .unwrap();
    }
    state.store.set_account_disabled("c-off", true).unwrap();
    offer_models(&state, AgentId::CLAUDE, &[("sonnet", &["low", "high"])]);
    // A console session of another console, which no task of console-1 may report to.
    let mut elsewhere = crate::mcp::test_support::console_session("elsewhere");
    elsewhere.console_id = "console-2".to_string();
    state.store.insert_session(&elsewhere).unwrap();
    let tasks_before = state.store.list_scheduled_tasks().unwrap();

    let create = |launch: Value| {
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "manual" }, "project": "project-1", "launch": launch }),
        )
    };
    let update = |launch: Value| {
        (
            "update_scheduled_task",
            json!({ "task": "t-project", "launch": launch }),
        )
    };
    let cases = [
        (
            create(json!({ "agent": "nobody" })),
            None,
            Some("`nobody` is not one of"),
        ),
        (
            create(json!({ "agent": "claude", "account": "x-1" })),
            Some(error_code::UNKNOWN_ACCOUNT),
            None,
        ),
        (
            create(json!({ "agent": "claude", "account": "c-off" })),
            Some(error_code::ACCOUNT_DISABLED),
            None,
        ),
        (
            create(json!({ "agent": "claude", "model": "no-such-model" })),
            None,
            Some("no-such-model"),
        ),
        (
            create(json!({ "agent": "claude", "model": "sonnet", "effort": "extreme" })),
            None,
            Some("extreme"),
        ),
        (
            create(json!({ "agent": "claude", "compatibility": { "no_such_switch": true } })),
            None,
            Some("no_such_switch"),
        ),
        (create(json!({ "bound_to": "of-a" })), None, Some("of-a")),
        (
            create(json!({ "bound_to": "elsewhere" })),
            None,
            Some("elsewhere"),
        ),
        (create(json!({ "bound_to": "gone" })), None, Some("gone")),
        (
            update(json!({ "agent": "claude", "account": "x-1" })),
            Some(error_code::UNKNOWN_ACCOUNT),
            None,
        ),
        (
            update(json!({ "agent": "claude", "account": "c-off" })),
            Some(error_code::ACCOUNT_DISABLED),
            None,
        ),
        (
            update(json!({ "agent": "claude", "model": "no-such-model" })),
            None,
            Some("no-such-model"),
        ),
        (
            update(json!({ "agent": "claude", "compatibility": { "no_such_switch": false } })),
            None,
            Some("no_such_switch"),
        ),
        (
            update(json!({ "bound_to": "elsewhere" })),
            None,
            Some("elsewhere"),
        ),
    ];
    for ((tool, input), code, text) in cases {
        let err = call(&state, "a", tool, &arguments(input.clone()))
            .await
            .expect_err("refused");
        if let Some(code) = code {
            assert_eq!(code_of(&err), code, "{input}: {err:#}");
        }
        if let Some(text) = text {
            assert!(err.to_string().contains(text), "{input}: {err:#}");
        }
        nobody_waits(&state, "a").await;
    }
    assert_eq!(state.store.list_scheduled_tasks().unwrap(), tasks_before);
}

/// A schedule out of range is refused before the user is asked: an hour or minute past its end
/// whichever kind carries it, a day of the month that does not exist, a weekly schedule with no
/// day, in a create and in an update alike.
#[tokio::test]
async fn a_schedule_out_of_range_is_refused_before_the_user_is_asked() {
    let (state, _dir) = shared_console_state("scheduled-schedule-range");
    three_tasks(&state);
    running(&state, "a");
    let time = |hour: u32, minute: u32| json!({ "hour": hour, "minute": minute });
    let schedules = [
        (json!({ "kind": "hourly", "minute": 60 }), "minute"),
        (json!({ "kind": "daily", "time": time(24, 0) }), "hour"),
        (json!({ "kind": "daily", "time": time(9, 60) }), "minute"),
        (json!({ "kind": "weekdays", "time": time(9, 99) }), "minute"),
        (
            json!({ "kind": "weekly", "time": time(25, 0), "days": ["monday"] }),
            "hour",
        ),
        (
            json!({ "kind": "weekly", "time": time(9, 0), "days": [] }),
            "days",
        ),
        (
            json!({ "kind": "monthly", "day": 0, "time": time(9, 0) }),
            "day",
        ),
        (
            json!({ "kind": "monthly", "day": 32, "time": time(9, 0) }),
            "day",
        ),
        (
            json!({ "kind": "monthly", "day": 15, "time": time(9, 60) }),
            "minute",
        ),
    ];
    for (schedule, reason) in schedules {
        for (tool, input) in [
            (
                "create_scheduled_task",
                json!({ "prompt": "x", "schedule": schedule }),
            ),
            (
                "update_scheduled_task",
                json!({ "task": "t-console", "schedule": schedule }),
            ),
        ] {
            let err = call(&state, "a", tool, &arguments(input))
                .await
                .expect_err("refused");
            assert!(err.to_string().contains(reason), "{schedule}: {err:#}");
            nobody_waits(&state, "a").await;
        }
    }
    assert_eq!(state.store.list_scheduled_tasks().unwrap().len(), 3);
}

/// An approval of each write carries the operation out through the coordinator and returns what
/// the tool returns: the task as stored with a `note` for an update and for turning one off and
/// on, a confirmation for a delete, and, for a run, the task and a note, the run being started in
/// the background. Nothing is changed before the approval, and no session is started or moved by
/// it.
#[tokio::test]
async fn an_approval_of_each_write_returns_what_the_tool_returns() {
    let (state, _dir) = shared_console_state("scheduled-approved");
    three_tasks(&state);
    for agent in AgentId::ALL {
        offer_models(&state, agent, &[("m", &["low"])]);
    }
    running(&state, "a");
    let described = |task: &str| describe_task(&state.store.require_scheduled_task(task).unwrap());

    let update = spawn_call(
        &state,
        "a",
        "update_scheduled_task",
        json!({ "task": "t-project", "prompt": "  new work  ", "schedule": { "kind": "manual" } }),
    );
    wait_for_request(&state, "a").await;
    assert_eq!(
        state
            .store
            .require_scheduled_task("t-project")
            .unwrap()
            .prompt,
        "project work"
    );
    assert!(approve(&state, "a").await.is_none());
    let result = update.await.unwrap().unwrap();
    assert_eq!(result["task"], described("t-project"));
    assert_eq!(result["task"]["prompt"], "new work");
    assert_eq!(result["task"]["schedule"]["kind"], "manual");
    assert_eq!(result["task"]["project"], "project-1");
    assert!(result["note"].as_str().unwrap().contains("t-project"));

    for enabled in [false, true] {
        let enable = spawn_call(
            &state,
            "a",
            "set_scheduled_task_enabled",
            json!({ "task": "t-console", "enabled": enabled }),
        );
        assert!(approve(&state, "a").await.is_none());
        let result = enable.await.unwrap().unwrap();
        assert_eq!(result["task"]["enabled"], enabled);
        assert_eq!(result["task"], described("t-console"));
        assert!(result["note"].as_str().unwrap().contains("t-console"));
    }

    let mut events = state.subscribe();
    let run = spawn_call(
        &state,
        "a",
        "run_scheduled_task",
        json!({ "task": "t-console" }),
    );
    wait_for_request(&state, "a").await;
    assert!(
        state
            .store
            .list_scheduled_task_runs("t-console")
            .unwrap()
            .is_empty(),
        "nothing runs before the approval"
    );
    assert!(approve(&state, "a").await.is_none());
    let result = run.await.unwrap().unwrap();
    assert_eq!(result["task"], "t-console");
    assert!(result["note"].as_str().unwrap().contains("t-console"));
    let ran = tokio::time::timeout(PATIENCE, async {
        loop {
            if let Ok(Event::ScheduledTaskRunAdded { run }) = events.recv().await {
                return run;
            }
        }
    })
    .await
    .expect("the run is recorded");
    assert_eq!(ran.task_id, "t-console");
    assert_eq!(ran.trigger, crate::protocol::ScheduledRunTrigger::RunNow);
    // A run leaves the task's schedule and switch alone.
    assert!(
        state
            .store
            .require_scheduled_task("t-console")
            .unwrap()
            .enabled
    );

    let delete = spawn_call(
        &state,
        "a",
        "delete_scheduled_task",
        json!({ "task": "t-project" }),
    );
    wait_for_request(&state, "a").await;
    assert!(state
        .store
        .get_scheduled_task("t-project")
        .unwrap()
        .is_some());
    assert!(approve(&state, "a").await.is_none());
    let result = delete.await.unwrap().unwrap();
    assert_eq!(result["deleted"], "t-project");
    assert!(result["note"].as_str().unwrap().contains("t-project"));
    assert!(state
        .store
        .get_scheduled_task("t-project")
        .unwrap()
        .is_none());
}

/// A refused, withdrawn or answered-by-nobody request of any write changes nothing: a refused
/// update, turning off, delete and run each leave the task as it was.
#[tokio::test]
async fn a_refused_write_of_each_kind_leaves_the_task_as_it_was() {
    let (state, _dir) = shared_console_state("scheduled-refused-each");
    three_tasks(&state);
    running(&state, "a");
    let before = state.store.list_scheduled_tasks().unwrap();
    let mut events = state.subscribe();
    for (tool, input) in [
        (
            "update_scheduled_task",
            json!({ "task": "t-project", "prompt": "other" }),
        ),
        (
            "set_scheduled_task_enabled",
            json!({ "task": "t-project", "enabled": false }),
        ),
        ("delete_scheduled_task", json!({ "task": "t-project" })),
        ("run_scheduled_task", json!({ "task": "t-project" })),
        (
            "create_scheduled_task",
            json!({ "prompt": "x", "schedule": { "kind": "manual" } }),
        ),
    ] {
        let waiting = spawn_call(&state, "a", tool, input);
        let Event::SessionRequest { request_id, .. } = wait_for_request(&state, "a").await else {
            panic!("a session request");
        };
        session_request::answer(&state, &request_id, false, Edits::default())
            .await
            .unwrap();
        let err = waiting.await.unwrap().expect_err("refused");
        assert!(
            err.to_string().contains("the user refused"),
            "{tool}: {err}"
        );
        assert!(
            err.to_string().contains(if tool == "run_scheduled_task" {
                "the task was not run"
            } else {
                "nothing was changed"
            }),
            "{tool}: {err}"
        );
        assert_eq!(
            state.store.list_scheduled_tasks().unwrap(),
            before,
            "{tool}"
        );
        assert!(
            state
                .store
                .list_scheduled_task_runs("t-project")
                .unwrap()
                .is_empty(),
            "{tool}"
        );
    }
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(
                event,
                Event::ScheduledTaskUpserted { .. }
                    | Event::ScheduledTaskDeleted { .. }
                    | Event::ScheduledTaskRunAdded { .. }
            ),
            "a refusal announced a change: {event:?}"
        );
    }
}

/// The setting is read once, when the call is made: a request already waiting stays waiting, and
/// can be answered, when the user then turns it on; a call made after that runs at once.
#[tokio::test]
async fn a_request_already_waiting_stays_waiting_when_the_setting_is_turned_on() {
    let (state, _dir) = shared_console_state("scheduled-setting-later");
    three_tasks(&state);
    running(&state, "a");
    let waiting = spawn_call(
        &state,
        "a",
        "update_scheduled_task",
        json!({ "task": "t-project", "prompt": "asked first" }),
    );
    wait_for_request(&state, "a").await;

    state
        .store
        .set_auto_approve_scheduled_task_changes(true)
        .unwrap();
    assert!(state.session_requests.is_waiting_for("a"));
    assert_eq!(waiting_of(&state, "a").len(), 1);
    assert_eq!(
        state
            .store
            .require_scheduled_task("t-project")
            .unwrap()
            .prompt,
        "project work",
        "the setting does not carry out what was already asked"
    );

    // A call about another task runs at once, beside the waiting one.
    let at_once = call(
        &state,
        "a",
        "update_scheduled_task",
        &arguments(json!({ "task": "t-console", "prompt": "at once" })),
    )
    .await
    .unwrap();
    assert_eq!(at_once["task"]["prompt"], "at once");
    assert_eq!(waiting_of(&state, "a").len(), 1);

    assert!(approve(&state, "a").await.is_none());
    let result = waiting.await.unwrap().unwrap();
    assert_eq!(result["task"]["prompt"], "asked first");
    assert_eq!(
        state
            .store
            .require_scheduled_task("t-project")
            .unwrap()
            .prompt,
        "asked first"
    );
}

/// With the setting on, every write runs at once and no request is broadcast or waits, and none of
/// them raises the session's hand: update, turning off, run and delete as well as create.
#[tokio::test]
async fn with_the_setting_on_no_write_asks_or_raises_a_hand() {
    let (state, _dir) = shared_console_state("scheduled-automatic-all");
    three_tasks(&state);
    for agent in AgentId::ALL {
        offer_models(&state, agent, &[("m", &["low"])]);
    }
    running(&state, "a");
    state
        .store
        .set_auto_approve_scheduled_task_changes(true)
        .unwrap();
    let mut events = state.subscribe();

    let updated = call(
        &state,
        "a",
        "update_scheduled_task",
        &arguments(json!({ "task": "t-project", "prompt": "changed" })),
    )
    .await
    .unwrap();
    assert_eq!(updated["task"]["prompt"], "changed");
    let off = call(
        &state,
        "a",
        "set_scheduled_task_enabled",
        &arguments(json!({ "task": "t-project", "enabled": false })),
    )
    .await
    .unwrap();
    assert_eq!(off["task"]["enabled"], false);
    let created = call(
        &state,
        "a",
        "create_scheduled_task",
        &arguments(json!({ "prompt": "new", "schedule": { "kind": "manual" } })),
    )
    .await
    .unwrap();
    assert!(created["task"]["project"].is_null());
    let ran = call(
        &state,
        "a",
        "run_scheduled_task",
        &arguments(json!({ "task": "t-project" })),
    )
    .await
    .unwrap();
    assert_eq!(ran["task"], "t-project");
    let deleted = call(
        &state,
        "a",
        "delete_scheduled_task",
        &arguments(json!({ "task": "t-project" })),
    )
    .await
    .unwrap();
    assert_eq!(deleted["deleted"], "t-project");

    assert!(!state.session_requests.is_waiting_for("a"));
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, Event::SessionRequest { .. }),
            "a request was broadcast: {event:?}"
        );
    }
    // What the tools refuse stays refused: the scope, and a prompt left blank.
    let err = call(
        &state,
        "a",
        "create_scheduled_task",
        &arguments(json!({ "prompt": " ", "schedule": { "kind": "manual" } })),
    )
    .await
    .expect_err("blank");
    assert!(err.to_string().contains("prompt"), "{err}");
}

/// A request tells the dialog the task's target after the change and, for a delete and a run, the
/// task alone; the prompt is only text the call wrote, never an empty string. A console session's
/// create without a project is for its console, a coordinator's for its project.
#[tokio::test]
async fn a_delete_a_run_and_a_create_each_tell_the_dialog_only_what_they_carry() {
    let (state, _dir) = shared_console_state("scheduled-event-fields");
    three_tasks(&state);
    running(&state, "a");
    running(&state, "loose");

    let mut asked = Vec::new();
    for (caller, tool, input) in [
        ("a", "delete_scheduled_task", json!({ "task": "t-project" })),
        ("a", "run_scheduled_task", json!({ "task": "t-console" })),
        (
            "a",
            "create_scheduled_task",
            json!({ "prompt": "Own", "schedule": { "kind": "manual" } }),
        ),
        (
            "loose",
            "create_scheduled_task",
            json!({ "prompt": "Mine", "schedule": { "kind": "manual" } }),
        ),
        (
            "loose",
            "create_scheduled_task",
            json!({ "prompt": "Named", "schedule": { "kind": "manual" }, "project": "project-1" }),
        ),
    ] {
        let waiting = spawn_call(&state, caller, tool, input);
        asked.push(wait_for_request(&state, caller).await);
        waiting.abort();
        let _ = waiting.await;
        let ended = poll(|| waiting_of(&state, caller).is_empty().then_some(())).await;
        assert!(
            ended.is_some(),
            "{tool}: the request is withdrawn with its call"
        );
    }
    let field = |at: usize| match &asked[at] {
        Event::SessionRequest {
            action,
            kind,
            console,
            project,
            scheduled_task,
            prompt,
            ..
        } => (
            *action,
            *kind,
            console.clone(),
            project.clone(),
            scheduled_task.clone().expect("what is asked"),
            prompt.clone(),
        ),
        other => panic!("not a session request: {other:?}"),
    };

    let (action, kind, console, project, task, prompt) = field(0);
    assert_eq!(action, SessionRequestAction::DeleteScheduledTask);
    assert_eq!(
        (kind, console.as_str()),
        (SessionKind::Project, "console-1")
    );
    assert_eq!(project.as_deref(), Some("project-1"));
    assert_eq!(task.task.as_deref(), Some("t-project"));
    assert_eq!(
        (task.launch, task.schedule, task.enabled),
        (None, None, None)
    );
    assert_eq!(prompt, None);

    let (action, kind, _, project, task, prompt) = field(1);
    assert_eq!(action, SessionRequestAction::RunScheduledTask);
    assert_eq!((kind, project), (SessionKind::Console, None));
    assert_eq!(task.task.as_deref(), Some("t-console"));
    assert_eq!(
        (task.launch, task.schedule, task.enabled),
        (None, None, None)
    );
    assert_eq!(prompt, None);

    let (action, kind, console, project, task, prompt) = field(2);
    assert_eq!(action, SessionRequestAction::CreateScheduledTask);
    assert_eq!(
        (kind, console.as_str(), project),
        (SessionKind::Console, "console-1", None)
    );
    assert_eq!(task.task, None);
    assert_eq!(task.schedule, Some(Schedule::Manual));
    assert_eq!((task.enabled, prompt.as_deref()), (Some(true), Some("Own")));
    assert_eq!(task.launch, Some(ScheduledTaskLaunch::default()));

    for (at, text) in [(3, "Mine"), (4, "Named")] {
        let (action, kind, console, project, task, prompt) = field(at);
        assert_eq!(action, SessionRequestAction::CreateScheduledTask);
        assert_eq!(
            (kind, console.as_str(), project.as_deref()),
            (SessionKind::Project, "console-1", Some("project-1")),
            "{text}"
        );
        assert_eq!((task.task, prompt.as_deref()), (None, Some(text)));
    }
}

/// A coordinator may name its own project (by id) when it updates a task, which then stays where
/// it is, but no other and not `null`; a console session may move a task between its console's
/// projects and to the console, never to another console's project.
#[tokio::test]
async fn a_task_moves_only_within_its_callers_reach() {
    let (state, _dir) = shared_console_state("scheduled-move");
    three_tasks(&state);
    running(&state, "a");
    running(&state, "loose");
    state
        .store
        .set_auto_approve_scheduled_task_changes(true)
        .unwrap();

    let own = call(
        &state,
        "loose",
        "update_scheduled_task",
        &arguments(json!({ "task": "t-project", "project": "project-1", "prompt": "same place" })),
    )
    .await
    .unwrap();
    assert_eq!(own["task"]["project"], "project-1");
    assert_eq!(own["task"]["prompt"], "same place");

    let to_console = call(
        &state,
        "a",
        "update_scheduled_task",
        &arguments(json!({ "task": "t-project", "project": null })),
    )
    .await
    .unwrap();
    assert!(to_console["task"]["project"].is_null());
    // It is a console task now, so the coordinator no longer reaches it.
    let err = call(
        &state,
        "loose",
        "update_scheduled_task",
        &arguments(json!({ "task": "t-project", "prompt": "x" })),
    )
    .await
    .expect_err("out of reach");
    assert!(err.to_string().contains("no scheduled task"), "{err}");

    let to_project = call(
        &state,
        "a",
        "update_scheduled_task",
        &arguments(json!({ "task": "t-project", "project": "project-1" })),
    )
    .await
    .unwrap();
    assert_eq!(to_project["task"]["project"], "project-1");
    let err = call(
        &state,
        "a",
        "update_scheduled_task",
        &arguments(json!({ "task": "t-project", "project": "project-2" })),
    )
    .await
    .expect_err("another console's project");
    assert!(err.to_string().contains("no project"), "{err}");
    assert_eq!(
        state
            .store
            .require_scheduled_task("t-project")
            .unwrap()
            .project_id
            .as_deref(),
        Some("project-1")
    );
}

/// What each tool's description says it returns is what it returns: every field it names is
/// there, and every field there is named.
#[tokio::test]
async fn the_descriptions_name_every_field_the_tools_return() {
    let (state, _dir) = shared_console_state("scheduled-descriptions");
    three_tasks(&state);
    for agent in AgentId::ALL {
        offer_models(&state, agent, &[("m", &["low"])]);
    }
    running(&state, "a");
    running(&state, "loose");
    state
        .store
        .set_auto_approve_scheduled_task_changes(true)
        .unwrap();
    // Two kept runs of the task, one that started a session and one that failed to; a task put
    // back after a delete has none, so each caller's turn records them again.
    let record_runs = |turn: &str| {
        for (at, outcome) in [
            ScheduledRunOutcome::Started {
                session: "of-a".to_string(),
                started_independent: false,
            },
            ScheduledRunOutcome::Failed {
                code: "unknown_session".to_string(),
                params: Default::default(),
                message: "unknown session".to_string(),
            },
        ]
        .into_iter()
        .enumerate()
        {
            state
                .store
                .record_scheduled_task_run(&ScheduledTaskRun {
                    id: format!("run-{turn}-{at}"),
                    task_id: "t-project".to_string(),
                    ran_at: 1_000 + at as i64,
                    due_at: None,
                    trigger: ScheduledRunTrigger::Scheduled,
                    outcome,
                })
                .unwrap();
        }
    };

    let task_fields = [
        "id",
        "console",
        "project",
        "launch",
        "prompt",
        "schedule",
        "enabled",
        "next_run_at",
        "last_run_at",
        "created_at",
        "updated_at",
        "run_sessions",
    ];
    let named = |description: &str, field: &str| description.contains(&format!("`{field}`"));
    for (kind, caller) in [(SessionKind::Console, "a"), (SessionKind::Project, "loose")] {
        record_runs(caller);
        let tool = |name: &str| {
            crate::session_tools::catalogue::tool_by_name(kind, false, name)
                .unwrap_or_else(|| panic!("{name} is offered to {kind:?}"))
                .description
        };
        let keys = |value: &Value| -> Vec<String> {
            let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        };
        let sorted = |fields: &[&str]| -> Vec<String> {
            let mut fields: Vec<String> = fields.iter().map(|field| field.to_string()).collect();
            fields.sort();
            fields
        };

        let listed = call(
            &state,
            caller,
            "list_scheduled_tasks",
            &arguments(json!({})),
        )
        .await
        .unwrap();
        let description = tool("list_scheduled_tasks");
        assert_eq!(keys(&listed), sorted(&["tasks", "time_zone"]));
        for field in ["tasks", "time_zone"] {
            assert!(named(description, field), "{kind:?} list: {field}");
        }
        let first = &listed["tasks"][0];
        assert_eq!(keys(first), sorted(&task_fields));
        for field in task_fields {
            assert!(named(description, field), "{kind:?} list: {field}");
        }

        let runs = call(
            &state,
            caller,
            "list_scheduled_task_runs",
            &arguments(json!({ "task": "t-project" })),
        )
        .await
        .unwrap();
        let description = tool("list_scheduled_task_runs");
        assert_eq!(keys(&runs), sorted(&["task", "runs"]));
        let run_fields = ["id", "task_id", "ran_at", "due_at", "trigger", "outcome"];
        let outcomes = [
            ("started", vec!["kind", "session", "started_independent"]),
            ("failed", vec!["kind", "code", "params", "message"]),
        ];
        let listed_runs = runs["runs"].as_array().unwrap();
        assert_eq!(listed_runs.len(), 2);
        for run in listed_runs {
            assert_eq!(keys(run), sorted(&run_fields));
            let kind = run["outcome"]["kind"].as_str().unwrap();
            let (_, fields) = outcomes.iter().find(|(name, _)| *name == kind).unwrap();
            assert_eq!(keys(&run["outcome"]), sorted(fields), "{kind}");
        }
        for field in ["task", "runs"].into_iter().chain(run_fields) {
            assert!(named(description, field), "{kind:?} runs: {field}");
        }
        // The outcome's fields are listed inside its shape, not each in its own span.
        for field in [
            "session",
            "started_independent",
            "code",
            "params",
            "message",
        ] {
            assert!(description.contains(field), "{kind:?} runs: {field}");
        }

        let written = [
            (
                "create_scheduled_task",
                json!({ "prompt": "new", "schedule": { "kind": "manual" } }),
                vec!["task", "note"],
            ),
            (
                "update_scheduled_task",
                json!({ "task": "t-project", "prompt": "changed" }),
                vec!["task", "note"],
            ),
            (
                "set_scheduled_task_enabled",
                json!({ "task": "t-project", "enabled": false }),
                vec!["task", "note"],
            ),
            (
                "delete_scheduled_task",
                json!({ "task": "t-project" }),
                vec!["deleted", "note"],
            ),
            (
                "run_scheduled_task",
                json!({ "task": "t-console" }),
                vec!["task", "note"],
            ),
        ];
        for (name, input, fields) in written {
            // A coordinator reaches the tasks of its project only.
            let input = if kind == SessionKind::Project && name == "run_scheduled_task" {
                json!({ "task": "t-project" })
            } else {
                input
            };
            let result = call(&state, caller, name, &arguments(input.clone()))
                .await
                .unwrap_or_else(|err| panic!("{kind:?} {name}: {err:#}"));
            assert_eq!(keys(&result), sorted(&fields), "{kind:?} {name}");
            let description = tool(name);
            for field in &fields {
                assert!(named(description, field), "{kind:?} {name}: {field}");
            }
            if let Some(task) = result.get("task").filter(|task| task.is_object()) {
                assert_eq!(keys(task), sorted(&task_fields), "{kind:?} {name}");
                for field in task_fields {
                    assert!(named(description, field), "{kind:?} {name}: {field}");
                }
            }
            if name == "delete_scheduled_task" {
                // Put the task back for the next caller.
                stored(
                    &state,
                    "t-project",
                    "console-1",
                    Some("project-1"),
                    "project work",
                );
            }
        }
    }
}

/// A write is approved minutes after it was asked, and the task may be out of the caller's reach
/// by then: a coordinator's waiting delete and update, approved after the tasks became console
/// tasks, are refused, and nothing changes.
#[tokio::test]
async fn an_approval_is_refused_when_the_task_has_left_the_callers_reach() {
    let (state, _dir) = shared_console_state("scheduled-left-reach");
    three_tasks(&state);
    stored(
        &state,
        "t-second",
        "console-1",
        Some("project-1"),
        "second work",
    );
    running(&state, "loose");

    let delete = spawn_call(
        &state,
        "loose",
        "delete_scheduled_task",
        json!({ "task": "t-project" }),
    );
    let update = spawn_call(
        &state,
        "loose",
        "update_scheduled_task",
        json!({ "task": "t-second", "prompt": "changed" }),
    );
    let enable = spawn_call(
        &state,
        "loose",
        "set_scheduled_task_enabled",
        json!({ "task": "t-second", "enabled": false }),
    );
    let run = spawn_call(
        &state,
        "loose",
        "run_scheduled_task",
        json!({ "task": "t-second" }),
    );
    poll(|| (waiting_of(&state, "loose").len() == 4).then_some(()))
        .await
        .expect("four requests wait");

    // Both tasks are moved out to the console while the user decides.
    for (id, prompt) in [("t-project", "project work"), ("t-second", "second work")] {
        state.store.delete_scheduled_task(id).unwrap();
        stored(&state, id, "console-1", None, prompt);
    }
    for request in waiting_of(&state, "loose") {
        let Event::SessionRequest { request_id, .. } = request else {
            unreachable!()
        };
        let err = session_request::answer(&state, &request_id, true, Edits::default())
            .await
            .expect_err("out of reach by now");
        // The user is told of it with a code, not as an internal error.
        assert_eq!(
            crate::test_support::code_of(&err),
            crate::protocol::error_code::UNKNOWN_SCHEDULED_TASK,
            "{err:#}"
        );
    }
    for call in [delete, update, enable, run] {
        let err = call.await.unwrap().expect_err("refused at approval");
        assert!(
            err.to_string().contains("could not be carried out"),
            "{err}"
        );
        assert!(err.to_string().contains("unknown scheduled task"), "{err}");
    }
    let first = state.store.require_scheduled_task("t-project").unwrap();
    let second = state.store.require_scheduled_task("t-second").unwrap();
    assert_eq!(first.project_id, None);
    assert_eq!(
        (second.prompt.as_str(), second.enabled),
        ("second work", true)
    );
}

/// The dialog is shown the launch as it would be saved: the title trimmed, and no report-to for a
/// console task.
#[tokio::test]
async fn the_request_shows_the_launch_as_it_would_be_saved() {
    let (state, _dir) = shared_console_state("scheduled-checked-launch");
    running(&state, "a");
    let waiting = spawn_call(
        &state,
        "a",
        "create_scheduled_task",
        json!({
            "prompt": "x",
            "schedule": { "kind": "manual" },
            "launch": { "title": "  Nightly  ", "bound_to": "a" },
        }),
    );
    let Event::SessionRequest {
        scheduled_task,
        request_id,
        ..
    } = wait_for_request(&state, "a").await
    else {
        panic!("a session request");
    };
    let launch = scheduled_task.unwrap().launch.unwrap();
    assert_eq!(launch.title.as_deref(), Some("Nightly"));
    assert_eq!(launch.bound_to, None);
    session_request::answer(&state, &request_id, true, Edits::default())
        .await
        .unwrap();
    let result = waiting.await.unwrap().unwrap();
    assert_eq!(result["task"]["launch"]["title"], "Nightly");
    assert!(result["task"]["launch"].get("bound_to").is_none());
}

/// Two creates are two requests: both wait, and each is answered on its own. A create run on the
/// user's standing approval withdraws neither.
#[tokio::test]
async fn creates_replace_nothing_and_are_answered_apart() {
    let (state, _dir) = shared_console_state("scheduled-creates");
    running(&state, "a");
    let create = |prompt: &str| {
        spawn_call(
            &state,
            "a",
            "create_scheduled_task",
            json!({ "prompt": prompt, "schedule": { "kind": "manual" } }),
        )
    };
    let (one, two) = (create("one"), create("two"));
    poll(|| (waiting_of(&state, "a").len() == 2).then_some(()))
        .await
        .expect("both wait");

    state
        .store
        .set_auto_approve_scheduled_task_changes(true)
        .unwrap();
    call(
        &state,
        "a",
        "create_scheduled_task",
        &arguments(json!({ "prompt": "three", "schedule": { "kind": "manual" } })),
    )
    .await
    .unwrap();
    assert_eq!(waiting_of(&state, "a").len(), 2);

    let requests = waiting_of(&state, "a");
    // The second asked is answered first, then the first: neither blocks the other.
    for request in requests.iter().rev() {
        let Event::SessionRequest { request_id, .. } = request else {
            unreachable!()
        };
        session_request::answer(&state, request_id, true, Edits::default())
            .await
            .unwrap();
    }
    one.await.unwrap().unwrap();
    two.await.unwrap().unwrap();
    let mut prompts: Vec<String> = state
        .store
        .list_scheduled_tasks()
        .unwrap()
        .into_iter()
        .map(|task| task.prompt)
        .collect();
    prompts.sort_unstable();
    assert_eq!(prompts, ["one", "three", "two"]);
}
