use super::*;
use crate::coordinator::lifecycle::{archive_session, delete_session};
use crate::coordinator::resume::resume_session_with;
use crate::coordinator::test_support::{
    open_request, resolve_bound_to, state_for_opening, stub_shell, wait_until,
};
use crate::protocol::{
    error_code, notice_code, CodedError, Event, Session, SessionKind, SessionStatus,
};
use crate::session::term;
use crate::session::LiveSession;
use crate::state::AppState;
use crate::test_support::code_of;
use crate::test_support::console_with_project_and_accounts;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

/// A launch that waits for `release` and then hands over `live` as the process it started.
fn launch_after(
    release: std::sync::mpsc::Receiver<()>,
    live: Arc<LiveSession>,
) -> impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static {
    move |_| {
        release.recv().expect("released");
        Ok(term::Launch {
            session: live,
            agent_session_id: None,
            notice: None,
        })
    }
}

/// [`open_detached_with`], with the launch started as soon as the session is returned.
async fn open_started(
    state: &Arc<AppState>,
    request: OpenRequest,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<Session> {
    let (session, followup) = open_detached_with(state, request, launch).await?;
    drop(followup);
    Ok(session)
}

/// The reply, and the broadcast, come while the agent is still being launched, with the session
/// `starting`; the launch settling then moves it to the status a fresh session has.
#[tokio::test]
async fn a_session_is_shown_as_starting_while_its_agent_is_still_being_launched() {
    let (state, _dir) = state_for_opening("coordinator-open-detached");
    let mut events = state.subscribe();
    // Not known until the session is: the stand-in is made once the launch is asked for it.
    let (release, released) = std::sync::mpsc::channel();
    let (live_tx, live_rx) = std::sync::mpsc::channel::<Arc<LiveSession>>();
    let opened = open_started(&state, open_request(), move |request| {
        launch_after(released, live_rx.recv().expect("a process"))(request)
    })
    .await
    .expect("opened");

    assert_eq!(opened.status, SessionStatus::Starting);
    let stored = state.store.get_session(&opened.id).unwrap().unwrap();
    assert_eq!(stored.status, SessionStatus::Starting);
    assert!(state.is_launching(&opened.id));
    assert!(state.live_session(&opened.id).is_none());
    let published = loop {
        match events.try_recv().expect("the session was published") {
            Event::SessionUpserted { session } => break session,
            _ => continue,
        }
    };
    assert_eq!(published.status, SessionStatus::Starting);

    let stand_in = crate::test_support::idle_stand_in(&opened.id);
    live_tx.send((*stand_in).clone()).unwrap();
    release.send(()).unwrap();
    wait_until(|| !state.is_launching(&opened.id)).await;
    assert_eq!(
        state.store.get_session(&opened.id).unwrap().unwrap().status,
        SessionStatus::Idle
    );
    assert!(state.live_session(&opened.id).is_some());
}

/// A launch that fails leaves nothing behind: the session is deleted again, and the user is told
/// why by a notice that names the session by its title, which no longer exists to be looked up.
/// The failure's own code and params travel in the notice, for a client to word the reason.
#[tokio::test]
async fn a_launch_that_fails_deletes_the_session_and_says_why() {
    let (state, _dir) = state_for_opening("coordinator-open-detached-fails");
    let mut events = state.subscribe();
    let opened = open_started(&state, open_request(), |_| {
        Err(CodedError::raised(
            error_code::BINARY_NOT_FOUND,
            "`claude` was not found".to_string(),
            &[("binary", "claude")],
        ))
    })
    .await
    .expect("replied before the launch");

    wait_until(|| state.store.get_session(&opened.id).unwrap().is_none()).await;
    assert!(!state.is_launching(&opened.id));
    let (mut deleted, mut notice) = (false, None);
    while let Ok(event) = events.try_recv() {
        match event {
            Event::SessionDeleted { session } if session == opened.id => deleted = true,
            Event::SessionNotice {
                session,
                code,
                params,
                ..
            } if session == opened.id => notice = Some((code, params)),
            _ => {}
        }
    }
    assert!(deleted, "clients drop the row they were shown");
    let (code, params) = notice.expect("a notice");
    assert_eq!(code, notice_code::LAUNCH_FAILED);
    assert_eq!(params["reason"], "`claude` was not found");
    assert_eq!(params["reason_code"], error_code::BINARY_NOT_FOUND);
    assert_eq!(params["binary"], "claude");
    assert_eq!(params["title"], opened.title);
}

/// The caller that waits for the launch gets its refusal as the error, as it always did, and no
/// notice besides: the error is how it is told.
#[tokio::test]
async fn a_session_opened_by_a_caller_that_waits_reports_a_failed_launch_as_its_error() {
    let (state, _dir) = state_for_opening("coordinator-open-waits");
    let mut events = state.subscribe();
    let starting = begin_open(&state, open_request()).await.expect("recorded");
    let id = starting.session.id.clone();
    let err = finish_open(&state, starting, |_| Err(anyhow::anyhow!("no binary")))
        .await
        .map_err(OpenError::into_error)
        .expect_err("refused");
    assert_eq!(format!("{err}"), "no binary");
    assert!(state.store.get_session(&id).unwrap().is_none());
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, Event::SessionNotice { .. }));
    }
}

/// Archiving a session that is still starting stands: when the process comes up it is ended
/// rather than left running under a session in the archive.
#[tokio::test]
async fn a_session_archived_while_starting_does_not_keep_the_process_it_launched() {
    let (state, _dir) = state_for_opening("coordinator-open-archived-meanwhile");
    let (release, released) = std::sync::mpsc::channel();
    let (live_tx, live_rx) = std::sync::mpsc::channel::<Arc<LiveSession>>();
    let (started_tx, started) = std::sync::mpsc::channel();
    let opened = open_started(&state, open_request(), move |request| {
        started_tx.send(()).unwrap();
        launch_after(released, live_rx.recv().expect("a process"))(request)
    })
    .await
    .expect("opened");
    wait_until(|| started.try_recv().is_ok()).await;

    archive_session(&state, &opened.id).expect("archived while starting");
    let stand_in = crate::test_support::idle_stand_in(&opened.id);
    live_tx.send((*stand_in).clone()).unwrap();
    release.send(()).unwrap();

    wait_until(|| !state.is_launching(&opened.id)).await;
    wait_until(|| stand_in.poll_exit()).await;
    wait_until(|| state.live_session(&opened.id).is_none()).await;
    assert_eq!(
        state.store.get_session(&opened.id).unwrap().unwrap().status,
        SessionStatus::Archived
    );
}

/// Archived before the launch has begun, a session is not launched at all.
#[tokio::test]
async fn a_session_archived_before_its_launch_begins_is_never_launched() {
    let (state, _dir) = state_for_opening("coordinator-open-archived-before");
    let launched = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let opened = open_started(&state, open_request(), {
        let launched = launched.clone();
        move |_| {
            launched.store(true, std::sync::atomic::Ordering::SeqCst);
            Err(anyhow::anyhow!("launched"))
        }
    })
    .await
    .expect("opened");
    archive_session(&state, &opened.id).expect("archived while starting");

    wait_until(|| !state.is_launching(&opened.id)).await;
    assert_eq!(
        state.store.get_session(&opened.id).unwrap().unwrap().status,
        SessionStatus::Archived
    );
    assert!(!launched.load(std::sync::atomic::Ordering::SeqCst));
    assert!(state.live_session(&opened.id).is_none());
}

/// A session opened to continue an outside conversation is launched with it, after the reply.
#[tokio::test]
async fn a_conversation_to_resume_reaches_the_launch_after_the_reply() {
    const ID: &str = "11111111-2222-3333-4444-555555555555";
    let (state, dir) = console_with_project_and_accounts("coordinator-resume-id-launched");
    let record = dir
        .join("claude-a/projects/slug")
        .join(format!("{ID}.jsonl"));
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "").unwrap();
    let (asked_tx, asked) = std::sync::mpsc::channel();
    let opened = open_started(
        &state,
        OpenRequest {
            project_id: Some("project-1".to_string()),
            account: Some(Some("claude-a".to_string())),
            resume_agent_session_id: Some(ID.to_string()),
            ..open_request()
        },
        move |request| {
            asked_tx.send(request.resume_agent_session_id).unwrap();
            Err(anyhow::anyhow!("no agent in the tests"))
        },
    )
    .await
    .expect("opened");

    assert_eq!(opened.status, SessionStatus::Starting);
    assert_eq!(opened.agent_session_id.as_deref(), Some(ID));
    assert!(opened.has_conversation);
    let mut resumed = None;
    wait_until(|| {
        resumed = asked.try_recv().ok();
        resumed.is_some()
    })
    .await;
    assert_eq!(resumed.flatten().as_deref(), Some(ID));
}

/// A session that is starting cannot be deleted or relaunched out from under its launch.
#[tokio::test]
async fn a_starting_session_is_not_deleted_or_resumed() {
    let (state, dir) = state_for_opening("coordinator-open-starting-refusals");
    let (release, released) = std::sync::mpsc::channel();
    let (live_tx, live_rx) = std::sync::mpsc::channel::<Arc<LiveSession>>();
    let opened = open_started(&state, open_request(), move |request| {
        launch_after(released, live_rx.recv().expect("a process"))(request)
    })
    .await
    .expect("opened");

    let err = delete_session(&state, &opened.id).expect_err("not while starting");
    assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE);
    let err = resume_session_with(&state, &opened.id, None, None, stub_shell(&dir))
        .await
        .expect_err("already being started");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);

    let stand_in = crate::test_support::idle_stand_in(&opened.id);
    live_tx.send((*stand_in).clone()).unwrap();
    release.send(()).unwrap();
    wait_until(|| !state.is_launching(&opened.id)).await;
}

/// The launch is the reply's follow-up: nothing is launched while it is held, so a launch that
/// fails at once cannot delete the session on a client before the reply has told it of it.
#[tokio::test]
async fn the_launch_waits_for_the_reply_to_be_queued() {
    let (state, _dir) = state_for_opening("coordinator-open-after-reply");
    let launched = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (opened, followup) = open_detached_with(&state, open_request(), {
        let launched = launched.clone();
        move |_| {
            launched.store(true, std::sync::atomic::Ordering::SeqCst);
            Err(anyhow::anyhow!("no binary"))
        }
    })
    .await
    .expect("opened");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!launched.load(std::sync::atomic::Ordering::SeqCst));
    assert!(state.store.get_session(&opened.id).unwrap().is_some());

    drop(followup);
    wait_until(|| state.store.get_session(&opened.id).unwrap().is_none()).await;
    assert!(launched.load(std::sync::atomic::Ordering::SeqCst));
}

/// A session still starting is refused as an owner: its launch may fail, which deletes it
/// whatever is bound to it.
#[tokio::test]
async fn a_starting_session_is_refused_as_an_owner() {
    let (state, _dir) = state_for_opening("coordinator-open-starting-owner");
    let (owner, followup) = open_detached_with(&state, open_request(), |_| {
        Err(anyhow::anyhow!("never launched"))
    })
    .await
    .expect("opened");
    let err = resolve_bound_to(
        &state,
        "console-1",
        Some("project-1"),
        SessionKind::Project,
        Some(owner.id.clone()),
    )
    .expect_err("starting");
    assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
    drop(followup);
    wait_until(|| !state.is_launching(&owner.id)).await;
}

/// A session opened with a task is shown as handing its agent a prompt from the record that
/// announces it until its launch is settled; one opened without a task never is.
#[tokio::test]
async fn a_session_opened_with_a_task_shows_as_prompting_until_it_is_launched() {
    let (state, _dir) = state_for_opening("coordinator-open-prompting");
    for (task, expected) in [(Some("do the work".to_string()), true), (None, false)] {
        let mut events = state.subscribe();
        let (live_tx, live_rx) = std::sync::mpsc::channel::<Arc<LiveSession>>();
        let (opened, followup) = open_detached_with(
            &state,
            OpenRequest {
                task,
                ..open_request()
            },
            move |_| {
                Ok(term::Launch {
                    session: live_rx.recv().expect("a process"),
                    agent_session_id: None,
                    notice: None,
                })
            },
        )
        .await
        .expect("opened");
        let announced = loop {
            match events.try_recv() {
                Ok(Event::SessionUpserted { session }) if session.id == opened.id => {
                    break session.prompt_on_launch;
                }
                Ok(_) => {}
                Err(err) => panic!("the session was never announced: {err}"),
            }
        };
        assert_eq!(announced, expected);
        assert_eq!(state.is_prompted_on_launch(&opened.id), expected);
        let mut after = state.subscribe();
        live_tx
            .send((*crate::test_support::idle_stand_in(&opened.id)).clone())
            .unwrap();
        drop(followup);
        wait_until(|| !state.is_launching(&opened.id)).await;
        assert!(!state.is_prompted_on_launch(&opened.id));
        // Releasing the claim announces the session with the flag cleared, so the last record a
        // client is sent of it no longer shows the launch as prompting.
        let mut last = None;
        while let Ok(event) = after.try_recv() {
            if let Event::SessionUpserted { session } = event {
                if session.id == opened.id {
                    last = Some(session.prompt_on_launch);
                }
            }
        }
        assert_eq!(last, Some(false));
    }
}

/// A refusal after the row is recorded, here the launch claim refused because the daemon is
/// shutting down, leaves no session row behind.
#[tokio::test]
async fn a_refused_launch_claim_leaves_no_session_row() {
    let (state, _dir) = state_for_opening("coordinator-open-claim-refused");
    state.stop_all_sessions().await;

    let err = match begin_open(&state, open_request()).await {
        Ok(_) => panic!("opened while shutting down"),
        Err(err) => err,
    };

    assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    assert!(state.store.list_sessions().unwrap().is_empty());
}

/// The role of a project session follows its parent: a coordinator under a console session, so it
/// can open sessions of its own, and a worker under a project coordinator, which keeps
/// orchestration two levels deep. A session with no parent is a coordinator too.
#[test]
fn a_session_opened_under_a_console_is_a_coordinator_and_one_under_a_coordinator_a_worker() {
    use crate::test_support::{console_session, project_session};

    let (state, _dir) = console_with_project_and_accounts("coordinator-open-roles");
    state
        .store
        .insert_session(&console_session("hub", SessionStatus::Idle))
        .unwrap();
    let request_under = |owner: Option<&str>| OpenRequest {
        project_id: Some("project-1".to_string()),
        bound_to: owner.map(str::to_string),
        ..open_request()
    };

    let under_console = resolve_open(&state, &request_under(Some("hub"))).unwrap();
    assert_eq!(under_console.role, Role::Coordinator);

    let coordinator = Session {
        bound_to: Some("hub".to_string()),
        role: under_console.role,
        ..project_session(
            "coordinator",
            crate::protocol::Agent::Claude,
            SessionStatus::Idle,
        )
    };
    state.store.insert_session(&coordinator).unwrap();
    let under_coordinator = resolve_open(&state, &request_under(Some("coordinator"))).unwrap();
    assert_eq!(under_coordinator.role, Role::Worker);

    let independent = Session {
        id: "independent".to_string(),
        bound_to: None,
        ..coordinator
    };
    state.store.insert_session(&independent).unwrap();
    let under_independent = resolve_open(&state, &request_under(Some("independent"))).unwrap();
    assert_eq!(under_independent.role, Role::Worker);
    assert_eq!(
        resolve_open(&state, &request_under(None)).unwrap().role,
        Role::Coordinator
    );
}
