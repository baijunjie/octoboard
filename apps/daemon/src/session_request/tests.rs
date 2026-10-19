use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::branch_origin::{self, repo_key, repository_of};
use crate::protocol::{Console, Origin, Project, ProjectAccount, ProjectSource, SessionStatus};
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
        usage: None,
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
        console_session_agent: Agent::Claude,
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
        tokio::spawn(async move { ask_for_session(&state, &caller, asked, handover, limit).await })
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

async fn answer_by(
    launcher: &Arc<Launcher>,
    request_id: &str,
    approve: bool,
) -> Result<Option<Session>> {
    let open = |request| launcher.clone().open(request);
    answer_with(&launcher.state, request_id, approve, open).await
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
    assert!(note.contains("mcp__octoboard__share_info"), "{note}");
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
        async move { answer_with(&state, &request_id, true, launch).await }
    });
    entered_rx.await.unwrap();

    let err = ask_for_session(
        &state,
        &caller,
        asked(CONSOLE, Some("project-1")),
        None,
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
