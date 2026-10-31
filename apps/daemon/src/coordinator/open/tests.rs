use super::*;
use crate::coordinator::bindings::resolve_bound_to;
use crate::coordinator::lifecycle::{archive_session, delete_session};
use crate::coordinator::resume::resume_session_with;
use crate::coordinator::test_support::{
    code_of, open_request, state_for_opening, stub_shell, wait_until,
};
use crate::protocol::{error_code, notice_code, CodedError, Event, Role, Session, SessionStatus};
use crate::session::LiveSession;
use crate::state::AppState;
use crate::term;
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
        Role::Project,
        Some(owner.id.clone()),
    )
    .expect_err("starting");
    assert_eq!(code_of(&err), error_code::UNKNOWN_SESSION);
    drop(followup);
    wait_until(|| !state.is_launching(&owner.id)).await;
}
