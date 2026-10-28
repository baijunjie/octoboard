use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::*;
use crate::branch_origin::{self, repo_key, repository_of};
use crate::protocol::{
    Console, Origin, Project, ProjectAccount, ProjectSource, SessionRequestAction, SessionStatus,
};
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    app_state, branch_made_by, idle_stand_in, output_containing, repo_with, running, ScratchDir,
    PATIENCE,
};
use serde_json::json;

const CONSOLE: &str = "console-1";
const OTHER_CONSOLE: &str = "console-2";

fn project_session(id: &str) -> Session {
    Session {
        id: id.to_string(),
        agent: Agent::Claude,
        agent_session_id: None,
        console_id: CONSOLE.to_string(),
        project_id: Some("project-1".to_string()),
        host_id: LOCAL_HOST_ID.to_string(),
        role: Role::Project,
        origin: Origin::User,
        title: format!("Work {id}"),
        status: SessionStatus::Idle,
        terminal_title: None,
        record_titles: Default::default(),
        background_tasks: Vec::new(),
        switching_account: false,
        default_effort: None,
        orchestration_role: None,
        context: None,
        observed_model: None,
        observed_effort: None,
        model: None,
        effort: None,
        has_conversation: true,
        bound_to: None,
        lead: false,
        colour: None,
        ordinal: None,
        account_id: None,
        config_dir: None,
        pinned: false,
        locked: false,
        started_at: 0,
        ended_at: None,
    }
}

fn console(id: &str, dir: &ScratchDir) -> Console {
    Console {
        id: id.to_string(),
        name: format!("Console {id}"),
        workdir: dir.join(id).to_string_lossy().into_owned(),
        default_agent: Agent::Claude,
        project_session_agent: None,
        claude_project_session_model: None,
        claude_project_session_effort: None,
        codex_project_session_model: None,
        codex_project_session_effort: None,
        grok_project_session_model: None,
        grok_project_session_effort: None,
        claude_account_id: None,
        codex_account_id: None,
        grok_account_id: None,
        claude_console_session_model: None,
        claude_console_session_effort: None,
        codex_console_session_model: None,
        codex_console_session_effort: None,
        grok_console_session_model: None,
        grok_console_session_effort: None,
        icon: None,
        console_session_prompt: None,
        created_at: 0,
    }
}

fn project(id: &str, console_id: &str, name: &str) -> Project {
    Project {
        id: id.to_string(),
        console_id: console_id.to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
        name: name.to_string(),
        path: format!("/tmp/{id}"),
        default_agent: None,
        source: ProjectSource::Local,
        remote_url: None,
        claude_account_id: ProjectAccount::Inherit,
        codex_account_id: ProjectAccount::Inherit,
        grok_account_id: ProjectAccount::Inherit,
        claude_session_model: None,
        claude_session_effort: None,
        codex_session_model: None,
        codex_session_effort: None,
        grok_session_model: None,
        grok_session_effort: None,
        trust_consent: false,
        pinned: false,
        tags: Vec::new(),
        description: None,
        git_workflow: Default::default(),
    }
}

/// Two consoles, a project in each, and an unbound project session in the first one's, `asker`,
/// not running.
fn fixture(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = app_state(&format!("session-request-{name}"));
    for id in [CONSOLE, OTHER_CONSOLE] {
        state.store.insert_console(&console(id, &dir)).unwrap();
    }
    state
        .store
        .insert_project(&project("project-1", CONSOLE, "Shop"))
        .unwrap();
    state
        .store
        .insert_project(&project("project-2", OTHER_CONSOLE, "Payments"))
        .unwrap();
    state
        .store
        .insert_session(&project_session("asker"))
        .unwrap();
    (state, dir)
}

const GOAL: &str = "Serve the refund endpoint the shop now calls.";

/// A request for a project session in `project` of `console`, or a console session of `console`
/// for none.
fn asked(console: &str, project: Option<&str>) -> OpenRequest {
    OpenRequest {
        console_id: console.to_string(),
        project_id: project.map(str::to_string),
        agent: None,
        account: None,
        model: None,
        effort: None,
        task: Some(format!("## Goal\n\n{GOAL}\n")),
        title: Some("Serve refunds".to_string()),
        origin: Origin::Console,
        bound_to: None,
        resume_agent_session_id: None,
        session_id: None,
        terminal_appearance: None,
    }
}

/// Starts `caller`'s call, waiting `limit`, and returns it with the request's event once it is
/// waiting.
async fn ask_in_background(
    state: &Arc<AppState>,
    caller: &str,
    asked: OpenRequest,
    limit: Duration,
) -> (tokio::task::JoinHandle<Result<Value>>, Event) {
    ask_handing_over(state, caller, asked, None, limit).await
}

/// [`ask_in_background`] for a request that hands `handover` over.
async fn ask_handing_over(
    state: &Arc<AppState>,
    caller: &str,
    asked: OpenRequest,
    handover: Option<Handover>,
    limit: Duration,
) -> (tokio::task::JoinHandle<Result<Value>>, Event) {
    let caller_id = caller.to_string();
    let caller = state.session_record(caller).unwrap();
    let task = {
        let state = state.clone();
        tokio::spawn(async move {
            ask_for_session(&state, &caller, asked, handover, false, limit).await
        })
    };
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        let waiting = pending_events(state).into_iter().find(
            |event| matches!(event, Event::SessionRequest { session, .. } if *session == caller_id),
        );
        if let Some(event) = waiting {
            return (task, event);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the request never waited"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// [`ask_in_background`] for a project session in the asker's own project, returning the
/// request's id.
async fn ask_here(
    state: &Arc<AppState>,
    caller: &str,
    limit: Duration,
) -> (tokio::task::JoinHandle<Result<Value>>, String) {
    let (call, event) =
        ask_in_background(state, caller, asked(CONSOLE, Some("project-1")), limit).await;
    (call, request_id_of(&event))
}

fn request_id_of(event: &Event) -> String {
    match event {
        Event::SessionRequest { request_id, .. } => request_id.clone(),
        other => panic!(
            "not a session request: {}",
            serde_json::to_string(other).unwrap()
        ),
    }
}

/// Stands in for `coordinator::open_session`, which would launch a real agent: records the
/// session the request describes, with a stand-in process, and counts the launches.
struct Launcher {
    state: Arc<AppState>,
    launched: AtomicUsize,
}

impl Launcher {
    fn new(state: &Arc<AppState>) -> Arc<Self> {
        Arc::new(Self {
            state: state.clone(),
            launched: AtomicUsize::new(0),
        })
    }

    async fn open(self: Arc<Self>, request: OpenRequest) -> Result<Session> {
        // Nobody's session, whatever the caller is.
        assert!(request.bound_to.is_none());
        let n = self.launched.fetch_add(1, Ordering::SeqCst);
        let session = Session {
            id: format!("started-{n}"),
            console_id: request.console_id.clone(),
            project_id: request.project_id.clone(),
            title: request.title.clone().unwrap(),
            origin: request.origin,
            ..project_session("started")
        };
        let session = match request.project_id {
            Some(_) => {
                self.state.store.insert_session(&session)?;
                session
            }
            None => self.state.store.insert_console_session(
                Session {
                    role: Role::Console,
                    ..session
                },
                request.title,
            )?,
        };
        running(&self.state, &session.id);
        Ok(session)
    }
}

/// Stands in for `coordinator::resume_session` where a request that starts a session is answered:
/// nothing there brings a session back.
async fn never_resumed(_id: String) -> Result<()> {
    bail!("no session is brought back here")
}

async fn answer_by(
    launcher: &Arc<Launcher>,
    request_id: &str,
    approve: bool,
) -> Result<Option<Session>> {
    let open = |request| launcher.clone().open(request);
    answer_with_resume(&launcher.state, request_id, approve, open, never_resumed).await
}

/// A call's outcome, waited for no longer than `PATIENCE`.
async fn outcome_of(call: tokio::task::JoinHandle<Result<Value>>) -> Result<Value> {
    tokio::time::timeout(PATIENCE, call)
        .await
        .expect("the call returns in time")
        .unwrap()
}

fn code_of(err: &anyhow::Error) -> Option<&'static str> {
    err.downcast_ref::<CodedError>().map(|coded| coded.code)
}

/// A session may ask for one in a project of another console: the dialog is told where, and an
/// approval starts it there with nobody as its owner, leaves the caller as it was, and writes the
/// outcome into the caller's session as well as returning it.
#[tokio::test]
async fn an_approval_starts_a_session_with_no_owner_in_any_project() {
    let (state, _dir) = fixture("approve-elsewhere");
    let asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let mut events = state.subscribe();
    let (call, event) = ask_in_background(
        &state,
        "asker",
        asked(OTHER_CONSOLE, Some("project-2")),
        ANSWER_TIME_LIMIT,
    )
    .await;
    let Event::SessionRequest {
        role,
        console,
        project,
        agent,
        ..
    } = &event
    else {
        unreachable!()
    };
    assert_eq!(
        (*role, console.as_str(), project.as_deref(), *agent),
        (
            Role::Project,
            OTHER_CONSOLE,
            Some("project-2"),
            Agent::Claude
        )
    );
    assert!(matches!(&event, Event::SessionRequest { branches, .. } if branches.is_empty()));

    let started = answer_by(&launcher, &request_id_of(&event), true)
        .await
        .unwrap()
        .unwrap();
    let result = outcome_of(call).await.unwrap();
    assert_eq!(result["session"], started.id);
    assert_eq!(result["project"], "project-2");
    let note = result["note"].as_str().unwrap();
    assert!(note.contains("reports to the user"), "{note}");
    assert!(note.contains("mcp__octoboard__send_message"), "{note}");
    // Not in the caller's console, so there is no binding to offer.
    assert!(!note.contains("bind_to_session"), "{note}");
    assert!(!note.contains("anded over"), "{note}");
    assert!(state
        .session_record(&started.id)
        .unwrap()
        .bound_to
        .is_none());
    assert!(state.session_record("asker").unwrap().bound_to.is_none());
    let message = output_containing(&asker, "mcp__octoboard__request_session");
    assert!(message.contains("Message from Octoboard"), "{message}");

    let mut closed = None;
    while let Ok(event) = events.try_recv() {
        if let Event::SessionRequestClosed { reason, .. } = event {
            closed = Some(reason);
        }
    }
    assert_eq!(closed, Some(SessionRequestEnding::Approved));
}

/// A console session can be asked for too; an unbound project session of that console is then
/// told it can bind itself to it, which is what having a console session to work under takes.
#[tokio::test]
async fn an_approval_can_start_a_console_session() {
    let (state, _dir) = fixture("approve-console");
    let _asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let (call, event) =
        ask_in_background(&state, "asker", asked(CONSOLE, None), ANSWER_TIME_LIMIT).await;
    assert!(matches!(
        event,
        Event::SessionRequest {
            role: Role::Console,
            project: None,
            ..
        }
    ));

    let started = answer_by(&launcher, &request_id_of(&event), true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(started.role, Role::Console);
    let result = outcome_of(call).await.unwrap();
    let note = result["note"].as_str().unwrap();
    assert!(note.contains("console session"), "{note}");
    assert!(
        note.contains(&format!(
            "`mcp__octoboard__bind_to_session` with `owner` set to `{}`",
            started.id
        )),
        "{note}"
    );
}

/// A refusal is a tool error in fixed prose, and nothing is started.
#[tokio::test]
async fn a_refusal_changes_nothing() {
    let (state, _dir) = fixture("refuse");
    let launcher = Launcher::new(&state);
    let (call, request_id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;

    answer_by(&launcher, &request_id, false).await.unwrap();
    let err = outcome_of(call).await.expect_err("refused");
    assert!(err.to_string().contains("the user refused"), "{err}");
    assert!(err.to_string().contains("Do not ask again"), "{err}");
    assert_eq!(launcher.launched.load(Ordering::SeqCst), 0);
    assert!(pending_events(&state).is_empty());
}

/// While a session's approval is being carried out it has no request waiting, and a call it
/// makes then is refused rather than putting a second session in front of the user.
#[tokio::test]
async fn a_call_made_while_its_approval_is_carried_out_is_refused() {
    let (state, _dir) = fixture("approving");
    let _asker = running(&state, "asker");
    let (_call, request_id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;
    let caller = state.session_record("asker").unwrap();

    // Held in the launch, which is where an approval spends its time.
    let (entered, entered_rx) = oneshot::channel();
    let (release, released) = oneshot::channel::<()>();
    let launch = |_request| async move {
        let _ = entered.send(());
        let _ = released.await;
        Err(anyhow!("not launched"))
    };
    let approval = tokio::spawn({
        let state = state.clone();
        async move { answer_with_resume(&state, &request_id, true, launch, never_resumed).await }
    });
    entered_rx.await.unwrap();

    let err = ask_for_session(
        &state,
        &caller,
        asked(CONSOLE, Some("project-1")),
        None,
        false,
        ANSWER_TIME_LIMIT,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("being carried out"), "{err}");
    assert!(pending_events(&state).is_empty());
    release.send(()).unwrap();
    approval.await.unwrap().unwrap_err();
}

/// With no answer within the limit the request is dropped and the call says the user did not
/// respond; an approval after that starts nothing and says the session is no longer waiting.
#[tokio::test]
async fn no_answer_within_the_limit_drops_the_request() {
    let (state, _dir) = fixture("time-limit");
    let launcher = Launcher::new(&state);
    let (call, request_id) = ask_here(&state, "asker", Duration::from_millis(50)).await;

    let err = outcome_of(call).await.expect_err("timed out");
    assert!(err.to_string().contains("did not respond"), "{err}");
    assert!(pending_events(&state).is_empty());
    let late = answer_by(&launcher, &request_id, true).await.unwrap_err();
    assert_eq!(
        code_of(&late),
        Some(error_code::SESSION_REQUEST_NOT_WAITING)
    );
    assert_eq!(launcher.launched.load(Ordering::SeqCst), 0);
}

/// Posts one hook payload of Claude Code's to `/hook/:session`, and returns once the daemon has
/// applied what it says: the route answers before that, so the hook queue is waited out.
async fn post_hook(state: &AppState, port: u16, session: &str, name: &str, mut payload: Value) {
    payload["hook_event_name"] = json!(name);
    payload["session_id"] = json!("claude-session");
    let body = serde_json::to_vec(&payload).unwrap();
    let path = format!("/hook/{session}");
    tokio::task::spawn_blocking(move || crate::loopback::post(port, &path, &body, PATIENCE))
        .await
        .unwrap()
        .unwrap();
    let deadline = std::time::Instant::now() + PATIENCE;
    while state.hook_queues.is_busy() {
        assert!(
            std::time::Instant::now() < deadline,
            "the hook was never applied"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn serve_hooks(state: &Arc<AppState>) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = crate::server::router(state.clone());
    tokio::spawn(async move { axum::serve(listener, router).await });
    port
}

fn status_of(state: &AppState, id: &str) -> SessionStatus {
    state.session_record(id).unwrap().status
}

/// A session whose request waits for the user is waiting for the user, and its owner is told so.
#[tokio::test]
async fn an_owner_is_told_that_its_session_waits_on_a_request() {
    let (state, _dir) = fixture("owner-told");
    state
        .store
        .insert_session(&Session {
            role: Role::Console,
            project_id: None,
            ..project_session("hub")
        })
        .unwrap();
    state
        .store
        .insert_session(&Session {
            bound_to: Some("hub".to_string()),
            ..project_session("worker")
        })
        .unwrap();
    let hub = running(&state, "hub");
    let _worker = running(&state, "worker");
    let launcher = Launcher::new(&state);
    let (call, request_id) = ask_here(&state, "worker", ANSWER_TIME_LIMIT).await;

    let heard =
        tokio::task::spawn_blocking(move || output_containing(&hub, "is waiting for the user"))
            .await
            .unwrap();
    assert!(heard.contains("is waiting for the user"), "{heard}");

    answer_by(&launcher, &request_id, false).await.unwrap();
    let _ = outcome_of(call).await;
}

/// The hook events Claude Code 2.1.295 sent while a call waited for the user (the order and
/// payloads of the hook log that showed the bug): it moves the call to the background, ends
/// its turn with the call still running, and reports itself idle. None of them lowers the
/// hand, the agent's own status stays what a message may be written on, and once the request
/// is answered the session reads what its agent's events left it at.
#[tokio::test]
async fn a_waiting_request_holds_the_hand_up_whatever_the_agent_reports() {
    let (state, _dir) = fixture("hand-held");
    let _asker = running(&state, "asker");
    let port = serve_hooks(&state).await;
    let launcher = Launcher::new(&state);
    let (call, request_id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;
    assert_eq!(status_of(&state, "asker"), SessionStatus::WaitingUser);

    post_hook(&state, port, "asker", "PostToolUse", json!({})).await;
    post_hook(&state, port, "asker", "PostToolBatch", json!({})).await;
    let task = json!({
        "id": "kdn46xv43",
        "type": "MCP task",
        "status": "running",
        "description": "octoboard/request_session",
        "server": "octoboard",
        "tool": "request_session",
    });
    post_hook(
        &state,
        port,
        "asker",
        "Stop",
        json!({ "background_tasks": [task] }),
    )
    .await;
    assert_eq!(status_of(&state, "asker"), SessionStatus::WaitingUser);
    post_hook(
        &state,
        port,
        "asker",
        "Notification",
        json!({"notification_type": "idle_prompt"}),
    )
    .await;
    assert_eq!(status_of(&state, "asker"), SessionStatus::WaitingUser);
    assert_eq!(pending_events(&state).len(), 1);

    // Answering the dialog is not the terminal's business: a write is not refused for it.
    let written = {
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            reporting::write_message(&state, "asker", "hello", WhenBlocked::Refuse)
        })
        .await
        .unwrap()
    };
    assert!(written.is_ok(), "{:?}", written.err());

    answer_by(&launcher, &request_id, false).await.unwrap();
    assert_eq!(status_of(&state, "asker"), SessionStatus::Idle);
    post_hook(&state, port, "asker", "PreToolUse", json!({})).await;
    assert_eq!(status_of(&state, "asker"), SessionStatus::Working);
    let _ = outcome_of(call).await;
}

/// An agent can give up on the call without any signal reaching Octoboard, so when the request
/// times out no event follows to correct the session: it goes back to what the agent's last
/// event said, not to idle and not left waiting.
#[tokio::test]
async fn a_request_that_times_out_returns_the_session_to_its_agents_status() {
    // The timeout stays short, so a stalled machine can let it fire before the hook lands: then
    // nothing was tested, and the whole scenario runs again.
    let deadline = std::time::Instant::now() + PATIENCE;
    for attempt in 0.. {
        assert!(
            std::time::Instant::now() < deadline,
            "the hook never landed inside the request's time limit"
        );
        let (state, _dir) = fixture(&format!("hand-timeout-{attempt}"));
        let _asker = running(&state, "asker");
        let port = serve_hooks(&state).await;
        let (call, _request_id) = ask_here(&state, "asker", Duration::from_millis(300)).await;
        post_hook(&state, port, "asker", "PostToolUse", json!({})).await;
        if !state.session_requests.is_waiting_for("asker")
            || status_of(&state, "asker") != SessionStatus::WaitingUser
        {
            let _ = outcome_of(call).await;
            continue;
        }

        outcome_of(call).await.expect_err("timed out");
        assert_eq!(status_of(&state, "asker"), SessionStatus::Working);
        return;
    }
}

/// The caller's process ending withdraws its request; an approval after that starts nothing
/// and is refused with the code the dialog's toast is shown for.
#[tokio::test]
async fn the_callers_process_ending_withdraws_its_request() {
    let (state, _dir) = fixture("process-ends");
    let asker = idle_stand_in("asker");
    state.register_live(asker.clone());
    state.watch_exit(asker.clone());
    let launcher = Launcher::new(&state);
    let (call, request_id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;

    asker.terminate();
    let err = outcome_of(call).await.expect_err("withdrawn");
    assert!(err.to_string().contains("withdrawn"), "{err}");
    let late = answer_by(&launcher, &request_id, true).await.unwrap_err();
    assert_eq!(
        code_of(&late),
        Some(error_code::SESSION_REQUEST_NOT_WAITING)
    );
    assert_eq!(launcher.launched.load(Ordering::SeqCst), 0);
}

/// Two clients answering the same request at once: the first answer taken is carried out,
/// once, and the other is refused as already answered.
#[tokio::test]
async fn the_first_answer_wins() {
    let (state, _dir) = fixture("first-answer");
    let _asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let (call, request_id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;

    // Polled in this order, so the approval is the one taken first.
    let (approval, refusal) = tokio::join!(
        answer_by(&launcher, &request_id, true),
        answer_by(&launcher, &request_id, false),
    );
    approval.unwrap();
    let lost = refusal.unwrap_err();
    assert_eq!(code_of(&lost), Some(error_code::SESSION_REQUEST_ANSWERED));
    assert!(outcome_of(call).await.is_ok());
    assert_eq!(launcher.launched.load(Ordering::SeqCst), 1);
}

/// A session asking again has stopped waiting for its earlier call, which its agent may have
/// given up on without a word: the new request takes the earlier one's place, and answering the
/// earlier one now is a late answer.
#[tokio::test]
async fn a_second_call_takes_the_place_of_the_first() {
    let (state, _dir) = fixture("second-call");
    let launcher = Launcher::new(&state);
    let (first, first_id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;
    let caller = state.session_record("asker").unwrap();
    let _second = {
        let state = state.clone();
        tokio::spawn(async move {
            ask_for_session(
                &state,
                &caller,
                asked(CONSOLE, None),
                None,
                false,
                ANSWER_TIME_LIMIT,
            )
            .await
        })
    };

    let err = outcome_of(first).await.expect_err("withdrawn");
    assert!(err.to_string().contains("withdrawn"), "{err}");
    let deadline = std::time::Instant::now() + PATIENCE;
    let waiting = loop {
        let waiting: Vec<String> = pending_events(&state).iter().map(request_id_of).collect();
        if !waiting.is_empty() || std::time::Instant::now() >= deadline {
            break waiting;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert_eq!(waiting.len(), 1);
    assert_ne!(waiting[0], first_id);
    let late = answer_by(&launcher, &first_id, true).await.unwrap_err();
    assert_eq!(
        code_of(&late),
        Some(error_code::SESSION_REQUEST_NOT_WAITING)
    );
}

/// The next event about the requests themselves; the caller's own status changes with them
/// are not what these tests look at.
async fn next_event(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Event {
    loop {
        let event = tokio::time::timeout(PATIENCE, events.recv())
            .await
            .expect("an event in time")
            .unwrap();
        if !matches!(event, Event::SessionUpserted { .. }) {
            return event;
        }
    }
}

/// Over the real router: a call whose connection is dropped withdraws its request, and the
/// requests still waiting are sent to a client right after its snapshot, oldest first.
#[tokio::test]
async fn a_dropped_call_is_withdrawn_and_waiting_requests_follow_the_snapshot() {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let (state, _dir) = fixture("router");
    for id in ["second", "dropped"] {
        state.store.insert_session(&project_session(id)).unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = crate::server::router(state.clone());
    tokio::spawn(async move { axum::serve(listener, router).await });

    // Withdrawn when its connection goes.
    let mut events = state.subscribe();
    let token = state.issue_mcp_token("dropped");
    let arguments = json!({ "kind": "project", "brief": { "goal": GOAL } });
    let body = json!({ "tool": "request_session", "arguments": arguments });
    let body = body.to_string();
    let mut call = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    call.write_all(
        format!(
            "POST /mcp/{token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    assert!(matches!(
        next_event(&mut events).await,
        Event::SessionRequest { .. }
    ));
    drop(call);
    assert!(matches!(
        next_event(&mut events).await,
        Event::SessionRequestClosed {
            reason: SessionRequestEnding::Withdrawn,
            ..
        }
    ));

    // Sent again after a snapshot, oldest first.
    let (_first, first_id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;
    let (_second, second_id) = ask_here(&state, "second", ANSWER_TIME_LIMIT).await;
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws/control"))
        .await
        .unwrap();
    let (_sink, mut stream) = socket.split();
    let mut types = Vec::new();
    let mut requests = Vec::new();
    while requests.len() < 2 {
        let frame = tokio::time::timeout(PATIENCE, stream.next())
            .await
            .expect("a frame in time")
            .unwrap()
            .unwrap();
        let tokio_tungstenite::tungstenite::Message::Text(text) = frame else {
            continue;
        };
        let event: Value = serde_json::from_str(&text).unwrap();
        types.push(event["type"].as_str().unwrap().to_string());
        if event["type"] == "session_request" {
            requests.push(event["request_id"].as_str().unwrap().to_string());
        }
    }
    assert_eq!(types[0], "snapshot");
    assert_eq!(requests, [first_id, second_id]);
}

/// A repository with a branch `asker` is responsible for under each of `branches`, the handover
/// of them, and the directory the repository lives in, which the handover is only good for as long
/// as it is kept.
fn handover_of(state: &Arc<AppState>, branches: &[&str]) -> (Handover, ScratchDir) {
    let repo = repo_with("session-request-repo", &[(b"a", b"a")]);
    for branch in branches {
        branch_made_by(state, &repo, branch, "asker");
    }
    let handover = Handover {
        repository: repository_of(&repo).unwrap(),
        branches: branches.iter().map(|branch| branch.to_string()).collect(),
        targets: Default::default(),
    };
    (handover, repo)
}

/// Who is responsible for each of the repository's branches, by branch name.
fn responsible_for(state: &Arc<AppState>, handover: &Handover) -> Vec<(String, String)> {
    state
        .store
        .branch_origins(&repo_key(&handover.repository))
        .unwrap()
        .into_iter()
        .map(|row| (row.branch, row.responsible_session))
        .collect()
}

/// The branches a request names are in its event, and an approval hands them to the session it
/// started, which the caller is told in the result and in the message, and the records take the new
/// session's title.
#[tokio::test]
async fn an_approval_hands_the_branches_over_to_the_session_it_started() {
    let (state, _dir) = fixture("handover-approve");
    let asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let (handover, _repo) = handover_of(&state, &["alpha", "beta"]);
    let (call, event) = ask_handing_over(
        &state,
        "asker",
        asked(CONSOLE, Some("project-1")),
        Some(handover.clone()),
        ANSWER_TIME_LIMIT,
    )
    .await;
    assert!(
        matches!(&event, Event::SessionRequest { branches, .. } if branches == &["alpha", "beta"])
    );
    // Asking moves nothing.
    assert!(responsible_for(&state, &handover)
        .iter()
        .all(|(_, session)| session == "asker"));

    let started = answer_by(&launcher, &request_id_of(&event), true)
        .await
        .unwrap()
        .unwrap();

    let result = outcome_of(call).await.unwrap();
    let note = result["note"].as_str().unwrap();
    assert!(
        note.contains("Handed over to it: `alpha`, `beta`."),
        "{note}"
    );
    assert!(!note.contains("Not handed over"), "{note}");
    assert_eq!(
        responsible_for(&state, &handover),
        [
            ("alpha".to_string(), started.id.clone()),
            ("beta".to_string(), started.id.clone())
        ]
    );
    let rows = state
        .store
        .branch_origins(&repo_key(&handover.repository))
        .unwrap();
    assert!(rows
        .iter()
        .all(|row| row.responsible_title == started.title));
    let message = output_containing(&asker, "Handed over to it");
    assert!(message.contains("`alpha`, `beta`"), "{message}");
}

/// A branch whose responsible session changed while the user was deciding is reported in the
/// outcome and left alone; the others are handed over.
#[tokio::test]
async fn a_branch_that_changed_hands_meanwhile_is_reported_not_moved() {
    let (state, _dir) = fixture("handover-partial");
    let _asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let (handover, _repo) = handover_of(&state, &["kept", "taken"]);
    let (call, event) = ask_handing_over(
        &state,
        "asker",
        asked(CONSOLE, Some("project-1")),
        Some(handover.clone()),
        ANSWER_TIME_LIMIT,
    )
    .await;
    branch_origin::hand_over(
        &state,
        &handover.repository,
        &["taken".to_string()],
        "asker",
        "elsewhere",
        "Elsewhere",
    )
    .unwrap();

    let started = answer_by(&launcher, &request_id_of(&event), true)
        .await
        .unwrap()
        .unwrap();

    let result = outcome_of(call).await.unwrap();
    let note = result["note"].as_str().unwrap();
    assert!(note.contains("Handed over to it: `kept`."), "{note}");
    assert!(note.contains("Not handed over"), "{note}");
    assert!(note.contains("`taken`. It stays as it was."), "{note}");
    assert_eq!(
        responsible_for(&state, &handover),
        [
            ("kept".to_string(), started.id),
            ("taken".to_string(), "elsewhere".to_string())
        ]
    );
}

/// A move the store cannot record is not an error of the approval: the session still starts, and
/// the caller is told it is still responsible for the branches.
#[tokio::test]
async fn a_handover_the_store_cannot_record_is_told_to_the_caller() {
    let (state, _dir) = fixture("handover-failed");
    let _asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let (handover, _repo) = handover_of(&state, &["alpha"]);
    let (call, event) = ask_handing_over(
        &state,
        "asker",
        asked(CONSOLE, Some("project-1")),
        Some(handover),
        ANSWER_TIME_LIMIT,
    )
    .await;
    state.store.drop_branch_origins().unwrap();

    answer_by(&launcher, &request_id_of(&event), true)
        .await
        .unwrap()
        .unwrap();

    let result = outcome_of(call).await.unwrap();
    let note = result["note"].as_str().unwrap();
    assert!(
        note.contains("could not record the handover of `alpha`, so you are still responsible"),
        "{note}"
    );
    assert!(!note.contains("Handed over"), "{note}");
}

/// A request that ends any way but an approval moves nothing.
#[tokio::test]
async fn a_request_that_is_not_approved_moves_nothing() {
    for ending in ["refused", "timed out", "withdrawn"] {
        let (state, _dir) = fixture(&format!("handover-{}", ending.replace(' ', "-")));
        let asker = idle_stand_in("asker");
        state.register_live(asker.clone());
        state.watch_exit(asker.clone());
        let launcher = Launcher::new(&state);
        let (handover, _repo) = handover_of(&state, &["topic"]);
        let limit = match ending {
            "timed out" => Duration::from_millis(50),
            _ => ANSWER_TIME_LIMIT,
        };
        let (call, event) = ask_handing_over(
            &state,
            "asker",
            asked(CONSOLE, Some("project-1")),
            Some(handover.clone()),
            limit,
        )
        .await;
        match ending {
            "refused" => {
                answer_by(&launcher, &request_id_of(&event), false)
                    .await
                    .unwrap();
            }
            "withdrawn" => asker.terminate(),
            _ => {}
        }
        outcome_of(call).await.expect_err(ending);
        // Too late to answer, whichever way it ended.
        let _ = answer_by(&launcher, &request_id_of(&event), true).await;

        assert_eq!(launcher.launched.load(Ordering::SeqCst), 0, "{ending}");
        assert_eq!(
            responsible_for(&state, &handover),
            [("topic".to_string(), "asker".to_string())],
            "{ending}"
        );
    }
}

/// Starts `caller`'s call with the setting on, through the stand-in launcher.
async fn ask_automatically(
    launcher: &Arc<Launcher>,
    caller: &str,
    asked: OpenRequest,
) -> Result<Value> {
    let caller = launcher.state.session_record(caller).unwrap();
    let launcher_for_launch = launcher.clone();
    let open = move |request| launcher_for_launch.open(request);
    ask_with(
        &launcher.state,
        &caller,
        Action::Start {
            open: asked,
            handover: None,
        },
        true,
        ANSWER_TIME_LIMIT,
        open,
        never_resumed,
    )
    .await
}

/// With the setting on, a request that passed its checks is approved at once: nothing is
/// broadcast or held and no hand goes up, the session is started with no owner, and the note and
/// the message to the caller say Octoboard approved it rather than the user.
#[tokio::test]
async fn an_automatic_approval_starts_the_session_without_asking() {
    let (state, _dir) = fixture("automatic");
    let asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let mut events = state.subscribe();

    let result = tokio::time::timeout(
        PATIENCE,
        ask_automatically(&launcher, "asker", asked(CONSOLE, Some("project-1"))),
    )
    .await
    .expect("the call returns without an answer")
    .unwrap();

    assert_eq!(launcher.launched.load(Ordering::SeqCst), 1);
    assert_eq!(result["session"], "started-0");
    let note = result["note"].as_str().unwrap();
    assert!(
        note.starts_with("Octoboard approved the request automatically"),
        "{note}"
    );
    assert!(
        note.contains("session requests approved without asking"),
        "{note}"
    );
    assert!(!note.contains("The user approved"), "{note}");
    assert!(note.contains("reports to the user"), "{note}");
    assert!(pending_events(&state).is_empty());
    assert!(!state.session_requests.is_waiting_for("asker"));
    assert!(state
        .session_record("started-0")
        .unwrap()
        .bound_to
        .is_none());
    assert!(!state.session_requests.lock().approving.contains("asker"));
    let message = output_containing(&asker, "mcp__octoboard__request_session");
    assert!(
        message.contains("approved the request automatically"),
        "{message}"
    );
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(
                event,
                Event::SessionRequest { .. } | Event::SessionRequestClosed { .. }
            ),
            "no dialog is opened or closed"
        );
    }
    // Nothing is left marking the session as being approved.
    ask_automatically(&launcher, "asker", asked(CONSOLE, None))
        .await
        .unwrap();
}

/// An automatic request takes the place of one its session has waiting in a dialog, which is
/// closed everywhere, and is refused while an approval of the session's earlier request is being
/// carried out.
#[tokio::test]
async fn an_automatic_approval_replaces_a_waiting_request_and_respects_an_approval_under_way() {
    let (state, _dir) = fixture("automatic-replaces");
    let _asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let (waiting, _id) = ask_here(&state, "asker", ANSWER_TIME_LIMIT).await;

    ask_automatically(&launcher, "asker", asked(CONSOLE, None))
        .await
        .unwrap();
    let err = outcome_of(waiting).await.expect_err("withdrawn");
    assert!(err.to_string().contains("withdrawn"), "{err}");
    assert!(pending_events(&state).is_empty());

    let (entered, entered_rx) = oneshot::channel();
    let (release, released) = oneshot::channel::<()>();
    let caller = state.session_record("asker").unwrap();
    let held = tokio::spawn({
        let state = state.clone();
        let caller = caller.clone();
        async move {
            let launch = |_request| async move {
                let _ = entered.send(());
                let _ = released.await;
                Err(anyhow!("not launched"))
            };
            ask_with(
                &state,
                &caller,
                Action::Start {
                    open: asked(CONSOLE, None),
                    handover: None,
                },
                true,
                ANSWER_TIME_LIMIT,
                launch,
                never_resumed,
            )
            .await
        }
    });
    entered_rx.await.unwrap();
    let err = ask_automatically(&launcher, "asker", asked(CONSOLE, None))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("being carried out"), "{err}");
    release.send(()).unwrap();
    let err = held.await.unwrap().unwrap_err();
    assert!(
        err.to_string()
            .contains("approved automatically, but it could not be carried out"),
        "{err}"
    );
}

/// A call cancelled while its session is launching (the agent cancels, or the connection drops)
/// does not interrupt the approval: the session is started, the outcome reaches the caller as a
/// message, and the session is no longer marked as being approved.
#[tokio::test]
async fn an_automatic_approval_carries_on_when_the_call_is_dropped() {
    let (state, _dir) = fixture("automatic-dropped");
    let asker = running(&state, "asker");
    let launcher = Launcher::new(&state);
    let caller = state.session_record("asker").unwrap();

    let (entered, entered_rx) = oneshot::channel();
    let (release, released) = oneshot::channel::<()>();
    let launch = {
        let launcher = launcher.clone();
        move |request| async move {
            let _ = entered.send(());
            let _ = released.await;
            launcher.open(request).await
        }
    };
    let call = tokio::spawn({
        let state = state.clone();
        async move {
            ask_with(
                &state,
                &caller,
                Action::Start {
                    open: asked(CONSOLE, Some("project-1")),
                    handover: None,
                },
                true,
                ANSWER_TIME_LIMIT,
                launch,
                never_resumed,
            )
            .await
        }
    });
    entered_rx.await.unwrap();
    call.abort();
    let _ = call.await;
    release.send(()).unwrap();

    // The task carrying the approval out shares this test's thread, so wait for it by yielding:
    // it releases its claim on the session once the outcome is written.
    let deadline = std::time::Instant::now() + PATIENCE;
    while state.session_requests.lock().approving.contains("asker") {
        assert!(std::time::Instant::now() < deadline, "the claim was kept");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let message = output_containing(&asker, "mcp__octoboard__request_session");
    assert!(
        message.contains("approved the request automatically"),
        "{message}"
    );
    assert_eq!(launcher.launched.load(Ordering::SeqCst), 1);
}

const CONVERSATION: &str = "11111111-2222-3333-4444-555555555555";

/// A dormant session of `project-1` that holds a conversation, reporting to `bound_to`.
fn holder(id: &str, status: SessionStatus, bound_to: Option<&str>) -> Session {
    Session {
        status,
        title: "Old work".to_string(),
        bound_to: bound_to.map(str::to_string),
        agent_session_id: Some(CONVERSATION.to_string()),
        ended_at: Some(1),
        ..project_session(id)
    }
}

/// Stands in for `coordinator::resume_session`, which would launch a real agent: moves the session
/// out of its dormant status, and counts the calls.
struct Resumer {
    state: Arc<AppState>,
    resumed: Mutex<Vec<String>>,
}

impl Resumer {
    fn new(state: &Arc<AppState>) -> Arc<Self> {
        Arc::new(Self {
            state: state.clone(),
            resumed: Mutex::new(Vec::new()),
        })
    }

    async fn resume(self: Arc<Self>, id: String) -> Result<()> {
        let before = self.state.session_record(&id)?.status;
        self.resumed.lock().unwrap().push(id.clone());
        self.state
            .store
            .update_session_lifecycle_if(&id, before, SessionStatus::Idle, None)?;
        Ok(())
    }

    fn resumed(&self) -> Vec<String> {
        self.resumed.lock().unwrap().clone()
    }
}

fn resuming(target: &str, status: SessionStatus, rename: Option<&str>) -> Action {
    Action::Resume {
        target: target.to_string(),
        archived: status == SessionStatus::Archived,
        owner: None,
        reopened_owners: Vec::new(),
        rename: rename.map(str::to_string),
    }
}

/// Makes the call for `action` as `caller` with the stand-ins, waits for its dialog if it asks,
/// and returns the call with the request's event.
async fn ask_to_resume(
    launcher: &Arc<Launcher>,
    resumer: &Arc<Resumer>,
    caller: &str,
    action: Action,
    automatic: bool,
) -> tokio::task::JoinHandle<Result<Value>> {
    let state = launcher.state.clone();
    let caller = state.session_record(caller).unwrap();
    let (launcher, resumer) = (launcher.clone(), resumer.clone());
    tokio::spawn(async move {
        ask_with(
            &state,
            &caller,
            action,
            automatic,
            ANSWER_TIME_LIMIT,
            move |request| launcher.open(request),
            move |id| resumer.resume(id),
        )
        .await
    })
}

async fn event_of(state: &Arc<AppState>, caller: &str) -> Event {
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        if let Some(event) = pending_events(state).into_iter().find(
            |event| matches!(event, Event::SessionRequest { session, .. } if session == caller),
        ) {
            return event;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the request never waited"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn answer_resuming(
    launcher: &Arc<Launcher>,
    resumer: &Arc<Resumer>,
    request_id: &str,
    approve: bool,
) -> Result<Option<Session>> {
    answer_with_resume(
        &launcher.state,
        request_id,
        approve,
        |request| launcher.clone().open(request),
        |id| resumer.clone().resume(id),
    )
    .await
}

/// The dialog is told which session an approval brings back, whom it reports to and which owners
/// come back with it; an approval reopens it, renames it, leaves its owner as it was, and tells the
/// caller, by the name of the tool it called. Refusing changes nothing.
#[tokio::test]
async fn an_approval_reopens_the_session_that_holds_the_conversation() {
    let (state, _dir) = fixture("resume-reopen");
    state
        .store
        .insert_session(&holder("boss", SessionStatus::Archived, None))
        .unwrap();
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, Some("boss")))
        .unwrap();
    let asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));
    let action = Action::Resume {
        target: "held".to_string(),
        archived: true,
        owner: Some("boss".to_string()),
        reopened_owners: vec!["boss".to_string()],
        rename: Some("Refund flow".to_string()),
    };

    let refused = ask_to_resume(&launcher, &resumer, "asker", action, false).await;
    let event = event_of(&state, "asker").await;
    let Event::SessionRequest {
        action,
        role,
        project,
        target,
        owner,
        reopened_owners,
        account,
        conversation,
        ..
    } = &event
    else {
        unreachable!()
    };
    assert_eq!(
        (
            *action,
            *role,
            project.as_deref(),
            target.as_deref(),
            owner.as_deref()
        ),
        (
            SessionRequestAction::Reopen,
            Role::Project,
            Some("project-1"),
            Some("held"),
            Some("boss")
        )
    );
    assert_eq!(reopened_owners, &["boss".to_string()]);
    assert_eq!((account, conversation), (&None, &None));
    answer_resuming(&launcher, &resumer, &request_id_of(&event), false)
        .await
        .unwrap();
    let err = outcome_of(refused).await.expect_err("refused");
    assert!(err.to_string().contains("nothing was resumed"), "{err}");
    assert!(resumer.resumed().is_empty());
    assert_eq!(status_of(&state, "held"), SessionStatus::Archived);

    let action = Action::Resume {
        target: "held".to_string(),
        archived: true,
        owner: Some("boss".to_string()),
        reopened_owners: vec!["boss".to_string()],
        rename: Some("Refund flow".to_string()),
    };
    let call = ask_to_resume(&launcher, &resumer, "asker", action, false).await;
    let event = event_of(&state, "asker").await;
    let session = answer_resuming(&launcher, &resumer, &request_id_of(&event), true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.id, "held");
    let result = outcome_of(call).await.unwrap();
    assert_eq!(
        (
            result["action"].as_str(),
            result["session"].as_str(),
            result["title"].as_str()
        ),
        (Some("reopened"), Some("held"), Some("Refund flow"))
    );
    assert_eq!(
        (&result["owner"], &result["renamed"]),
        (&json!("boss"), &json!(true))
    );
    let note = result["note"].as_str().unwrap();
    assert!(note.contains("reports to its owner `boss`"), "{note}");
    assert!(note.contains("mcp__octoboard__send_message"), "{note}");
    assert_eq!(resumer.resumed(), ["held"]);
    let held = state.session_record("held").unwrap();
    assert_eq!(
        (held.status, held.bound_to.as_deref(), held.title.as_str()),
        (SessionStatus::Idle, Some("boss"), "Refund flow")
    );
    let message = output_containing(&asker, "mcp__octoboard__request_resume_session");
    assert!(message.contains("Message from Octoboard"), "{message}");
}

/// An interrupted session is resumed, not reopened, and keeps being the user's own.
#[tokio::test]
async fn an_approval_resumes_an_interrupted_holder() {
    let (state, _dir) = fixture("resume-interrupted");
    state
        .store
        .insert_session(&holder("held", SessionStatus::Interrupted, None))
        .unwrap();
    let _asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));

    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Interrupted, None),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    assert!(matches!(
        event,
        Event::SessionRequest {
            action: SessionRequestAction::Resume,
            ..
        }
    ));
    answer_resuming(&launcher, &resumer, &request_id_of(&event), true)
        .await
        .unwrap();
    let result = outcome_of(call).await.unwrap();
    assert_eq!(result["action"], "resumed");
    assert!(result["owner"].is_null());
    assert!(result.get("renamed").is_none());
    assert!(result["note"]
        .as_str()
        .unwrap()
        .contains("reports to the user, as before"));
}

/// A session that is running by the time of the approval is left alone: nothing is resumed or
/// renamed, and the result says so, whether it was seen running or the resume found it so.
#[tokio::test]
async fn an_approval_finding_the_session_running_does_nothing() {
    let (state, _dir) = fixture("resume-already-running");
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, None))
        .unwrap();
    let _asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));

    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, Some("Refund flow")),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    // Brought back by the user meanwhile.
    resumer.clone().resume("held".to_string()).await.unwrap();
    resumer.resumed.lock().unwrap().clear();
    answer_resuming(&launcher, &resumer, &request_id_of(&event), true)
        .await
        .unwrap();
    let result = outcome_of(call).await.unwrap();
    assert_eq!(result["action"], "already_running");
    assert_eq!(result["renamed"], false);
    let note = result["note"].as_str().unwrap();
    assert!(note.contains("did nothing"), "{note}");
    assert!(note.contains("could not be renamed"), "{note}");
    assert!(resumer.resumed().is_empty());
    assert_eq!(state.session_record("held").unwrap().title, "Old work");

    // The resume itself finding it running, as a click of the user's own would.
    state
        .store
        .update_session_lifecycle_if(
            "held",
            SessionStatus::Idle,
            SessionStatus::Interrupted,
            None,
        )
        .unwrap();
    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Interrupted, None),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    let answered = answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |request| launcher.clone().open(request),
        |id| async move {
            Err(CodedError::raised(
                error_code::SESSION_ALREADY_RUNNING,
                "running",
                &[("session", &id)],
            ))
        },
    )
    .await;
    assert!(answered.unwrap().is_some());
    assert_eq!(outcome_of(call).await.unwrap()["action"], "already_running");
}

/// A refusal for the launch claim of an archived owner is not the session running: the approval
/// could not be carried out and says so, unless the session itself came back meanwhile.
#[tokio::test]
async fn an_owners_launch_claim_is_not_the_session_running() {
    let (state, _dir) = fixture("resume-owner-claim");
    state
        .store
        .insert_session(&holder("boss", SessionStatus::Archived, None))
        .unwrap();
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, Some("boss")))
        .unwrap();
    let _asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));
    let claimed = || {
        CodedError::raised(
            error_code::SESSION_ALREADY_STARTING,
            "starting",
            &[("session", "boss")],
        )
    };

    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, None),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    let err = answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |request| launcher.clone().open(request),
        |_| async { Err(claimed()) },
    )
    .await
    .expect_err("not carried out");
    assert!(err.to_string().contains("starting"), "{err}");
    let told = outcome_of(call).await.expect_err("not carried out");
    assert!(
        told.to_string().contains("could not be carried out"),
        "{told}"
    );
    assert_eq!(status_of(&state, "held"), SessionStatus::Archived);

    // The session itself came back while the owner's claim was refused.
    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, None),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    let came_back = resumer.clone();
    answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |request| launcher.clone().open(request),
        |id| async move {
            came_back.resume(id).await?;
            Err(claimed())
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome_of(call).await.unwrap()["action"], "already_running");
}

/// A refusal for the claim of an account switch, a handoff or a binding change on the session
/// itself is not a launch under way: the session is still dormant, so the approval could not be
/// carried out. A launch claim of the user's own resume still counts as done.
#[tokio::test]
async fn a_switch_holding_the_session_is_not_a_launch_of_it() {
    let (state, _dir) = fixture("resume-switch-claim");
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, None))
        .unwrap();
    let _asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));
    let refused_by = |state: Arc<AppState>| {
        move |id: String| async move {
            // What a resume is told by the claim, whoever holds it.
            Err::<(), _>(state.begin_launch(&id).map(drop).unwrap_err())
        }
    };

    let switch = state.begin_switch("held").unwrap();
    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, None),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    let err = answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |request| launcher.clone().open(request),
        refused_by(state.clone()),
    )
    .await
    .expect_err("not carried out");
    assert_eq!(code_of(&err), Some(error_code::SESSION_ALREADY_STARTING));
    let told = outcome_of(call).await.expect_err("not carried out");
    assert!(
        told.to_string().contains("could not be carried out"),
        "{told}"
    );
    assert_eq!(status_of(&state, "held"), SessionStatus::Archived);
    drop(switch);

    // A launch of the session, as the user's own resume of it.
    let launch = state.begin_launch("held").unwrap();
    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, None),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |request| launcher.clone().open(request),
        refused_by(state.clone()),
    )
    .await
    .unwrap();
    assert_eq!(outcome_of(call).await.unwrap()["action"], "already_running");
    drop(launch);
}

/// A refusal for a switch claim that is gone by the time it is judged is not read as a launch of
/// the target: with the session still dormant, the approval could not be carried out.
#[tokio::test]
async fn a_switch_claim_gone_meanwhile_is_not_a_launch_of_the_target() {
    let (state, _dir) = fixture("resume-switch-claim-gone");
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, None))
        .unwrap();
    let switch = state.begin_switch("held").unwrap();
    let refusal = state.begin_launch("held").map(drop).unwrap_err();
    drop(switch);
    assert!(!refused_as_running(&state, &refusal, "held").unwrap());

    let launch = state.begin_launch("held").unwrap();
    assert!(refused_as_running(&state, &refusal, "held").unwrap());
    drop(launch);
}

/// The archived owners a resume brings back are told, by id, in the result and the note.
#[tokio::test]
async fn an_approval_that_brings_the_owner_back_says_so() {
    let (state, _dir) = fixture("resume-owners-back");
    state
        .store
        .insert_session(&holder("boss", SessionStatus::Archived, None))
        .unwrap();
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, Some("boss")))
        .unwrap();
    let _asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));

    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, None),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    let stand_in = resumer.clone();
    let owner_state = state.clone();
    answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |request| launcher.clone().open(request),
        |id| async move {
            // As `resume_session` does: the owner first, then the session.
            owner_state.store.update_session_lifecycle_if(
                "boss",
                SessionStatus::Archived,
                SessionStatus::Idle,
                None,
            )?;
            stand_in.resume(id).await
        },
    )
    .await
    .unwrap();
    let result = outcome_of(call).await.unwrap();
    assert_eq!(result["reopened_owners"], json!(["boss"]));
    let note = result["note"].as_str().unwrap();
    assert!(
        note.contains("archived owners were reopened with it: `boss`"),
        "{note}"
    );
}

/// A resume the agent refuses is the failure of an approved request, which the caller is told, with
/// the session as it was.
#[tokio::test]
async fn a_resume_that_fails_is_told_to_the_caller() {
    let (state, _dir) = fixture("resume-fails");
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, None))
        .unwrap();
    let _asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));

    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, Some("Refund flow")),
        false,
    )
    .await;
    let event = event_of(&state, "asker").await;
    let err = answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |request| launcher.clone().open(request),
        |_| async { Err(anyhow!("the agent is gone")) },
    )
    .await
    .expect_err("fails");
    assert!(err.to_string().contains("the agent is gone"), "{err}");
    let told = outcome_of(call).await.expect_err("fails");
    assert!(
        told.to_string()
            .contains("approved this request for a session, but it could not be carried out"),
        "{told}"
    );
    let held = state.session_record("held").unwrap();
    assert_eq!(
        (held.status, held.title.as_str()),
        (SessionStatus::Archived, "Old work")
    );
}

/// A conversation only an agent's record holds opens an unbound session on that record: the dialog
/// is told the account and the conversation, and a session that came to hold the conversation
/// meanwhile makes the approval fail rather than a second session start.
#[tokio::test]
async fn an_approval_opens_a_session_on_the_record() {
    let (state, _dir) = fixture("resume-record");
    let asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));
    let record = || Action::ResumeRecord {
        open: OpenRequest {
            task: None,
            origin: Origin::User,
            title: Some("Refunds".to_string()),
            resume_agent_session_id: Some(CONVERSATION.to_string()),
            ..asked(CONSOLE, Some("project-1"))
        },
        conversation: CONVERSATION.to_string(),
    };

    let call = ask_to_resume(&launcher, &resumer, "asker", record(), false).await;
    let event = event_of(&state, "asker").await;
    assert!(matches!(
        &event,
        Event::SessionRequest {
            action: SessionRequestAction::ResumeRecord,
            target: None,
            account: None,
            conversation: Some(conversation),
            project: Some(project),
            ..
        } if conversation == CONVERSATION && project == "project-1"
    ));
    let started = answer_resuming(&launcher, &resumer, &request_id_of(&event), true)
        .await
        .unwrap()
        .unwrap();
    let result = outcome_of(call).await.unwrap();
    assert_eq!(
        (result["action"].as_str(), result["session"].as_str()),
        (Some("opened"), Some(started.id.as_str()))
    );
    assert!(result["owner"].is_null());
    let note = result["note"].as_str().unwrap();
    assert!(note.contains("It has no owner"), "{note}");
    assert!(state
        .session_record(&started.id)
        .unwrap()
        .bound_to
        .is_none());
    output_containing(&asker, "mcp__octoboard__request_resume_session");

    // Another session came to hold the conversation before the approval: the launch refuses.
    let call = ask_to_resume(&launcher, &resumer, "asker", record(), false).await;
    let event = event_of(&state, "asker").await;
    let err = answer_with_resume(
        &state,
        &request_id_of(&event),
        true,
        |_| async {
            Err(CodedError::raised(
                error_code::CONVERSATION_IN_SESSION,
                "already held",
                &[],
            ))
        },
        |id| resumer.clone().resume(id),
    )
    .await
    .expect_err("fails");
    assert!(err.to_string().contains("already held"), "{err}");
    outcome_of(call).await.expect_err("fails");
}

/// With the setting on, a resume is approved at once as a start is: no dialog, the same outcome.
#[tokio::test]
async fn an_automatic_approval_resumes_without_asking() {
    let (state, _dir) = fixture("resume-automatic");
    state
        .store
        .insert_session(&holder("held", SessionStatus::Archived, None))
        .unwrap();
    let asker = running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));
    let mut events = state.subscribe();

    let call = ask_to_resume(
        &launcher,
        &resumer,
        "asker",
        resuming("held", SessionStatus::Archived, None),
        true,
    )
    .await;
    let result = outcome_of(call).await.unwrap();
    assert_eq!(result["action"], "reopened");
    let note = result["note"].as_str().unwrap();
    assert!(
        note.starts_with("Octoboard approved the request automatically"),
        "{note}"
    );
    assert!(
        note.contains("resume requests approved without asking"),
        "{note}"
    );
    assert!(pending_events(&state).is_empty());
    assert!(!state.session_requests.lock().approving.contains("asker"));
    let message = output_containing(&asker, "mcp__octoboard__request_resume_session");
    assert!(
        message.contains("approved the request automatically"),
        "{message}"
    );
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(
                event,
                Event::SessionRequest { .. } | Event::SessionRequestClosed { .. }
            ),
            "no dialog is opened or closed"
        );
    }
}

/// With the setting on, a record's session opens at once too, and the note names the resume
/// setting, as for a session brought back.
#[tokio::test]
async fn an_automatic_approval_opens_a_session_on_the_record() {
    let (state, _dir) = fixture("resume-record-automatic");
    running(&state, "asker");
    let (launcher, resumer) = (Launcher::new(&state), Resumer::new(&state));
    let record = Action::ResumeRecord {
        open: OpenRequest {
            task: None,
            origin: Origin::User,
            title: Some("Refunds".to_string()),
            resume_agent_session_id: Some(CONVERSATION.to_string()),
            ..asked(CONSOLE, Some("project-1"))
        },
        conversation: CONVERSATION.to_string(),
    };

    let call = ask_to_resume(&launcher, &resumer, "asker", record, true).await;
    let result = outcome_of(call).await.unwrap();
    assert_eq!(result["action"], "opened");
    let note = result["note"].as_str().unwrap();
    assert!(
        note.contains("resume requests approved without asking"),
        "{note}"
    );
    assert!(pending_events(&state).is_empty());
}

/// A switch nobody answers within the limit ends with an error saying so and that nothing was
/// changed, and runs nothing.
#[tokio::test]
async fn a_switch_nobody_answers_times_out_without_running() {
    let (state, _dir) = fixture("switch-timeout");
    let _asker = running(&state, "asker");
    state
        .store
        .insert_session(&project_session("owned"))
        .unwrap();
    let ran = Arc::new(AtomicUsize::new(0));
    let action = Change::Switch {
        target: "owned".to_string(),
        account: None,
        perform: Perform::new({
            let ran = ran.clone();
            move |_| async move {
                ran.fetch_add(1, Ordering::SeqCst);
                bail!("not to be run")
            }
        }),
    };
    let caller = state.session_record("asker").unwrap();
    let target = state.session_record("owned").unwrap();

    let err = ask_to_change(&state, &caller, &target, action, Duration::from_millis(50))
        .await
        .expect_err("timed out");

    assert!(
        err.to_string()
            .contains("the user did not respond to this switch of the session's account"),
        "{err}"
    );
    assert!(err.to_string().contains("nothing was changed"), "{err}");
    assert_eq!(ran.load(Ordering::SeqCst), 0);
    assert!(!state.session_requests.is_waiting_for("asker"));
}
