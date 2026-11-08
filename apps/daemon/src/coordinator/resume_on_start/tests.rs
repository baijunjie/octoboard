use super::*;
use crate::protocol::{AgentId, SessionStatus};
use crate::test_support::{console_with_project_and_accounts, project_session};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

fn bound(id: &str, owner: &str) -> Session {
    Session {
        bound_to: Some(owner.to_string()),
        ..project_session(id, AgentId::CLAUDE, SessionStatus::Idle)
    }
}

/// An owner is resumed before the sessions bound to it, however the records were ordered.
#[test]
fn an_owner_comes_back_before_the_sessions_under_it() {
    let sessions = vec![
        bound("grandchild", "child"),
        bound("child", "owner"),
        project_session("loner", AgentId::CLAUDE, SessionStatus::Idle),
        project_session("owner", AgentId::CLAUDE, SessionStatus::Idle),
        bound("orphan", "not-in-the-set"),
    ];
    assert_eq!(
        in_resume_order(sessions),
        ["loner", "owner", "orphan", "child", "grandchild"]
    );
}

/// A binding that loops back on itself still ends.
#[test]
fn a_cycle_of_bindings_does_not_hang_the_ordering() {
    let sessions = vec![bound("a", "b"), bound("b", "a")];
    assert_eq!(in_resume_order(sessions).len(), 2);
}

/// Only sessions a run left live are picked: the ones still running when it ended, and the ones
/// its shutdown marked; never an archived one, and never one interrupted before it ended. The
/// mark does not outlive the start that read it.
#[test]
fn only_the_sessions_a_run_left_live_are_picked() {
    let (state, _dir) = console_with_project_and_accounts("resume-on-start-pick");
    let insert = |id: &str, status| {
        state
            .store
            .insert_session(&project_session(id, AgentId::CLAUDE, status))
            .unwrap();
    };
    insert("running", SessionStatus::Working);
    insert("prompt", SessionStatus::Idle);
    insert("shut-down", SessionStatus::Working);
    insert("interrupted-before", SessionStatus::Interrupted);
    insert("archived", SessionStatus::Archived);
    // A clean shutdown: the mark first, then the interruption.
    state
        .store
        .mark_left_live(&["shut-down".to_string(), "archived".to_string()])
        .unwrap();
    state.store.interrupt_session("shut-down", 5, None).unwrap();

    let picked: Vec<String> = state
        .store
        .take_left_live_sessions()
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect();
    assert_eq!(picked, ["running", "prompt", "shut-down"]);

    // Taken once: after the start has interrupted what was running, nothing is left to pick.
    state.store.mark_live_sessions_interrupted().unwrap();
    assert!(state.store.take_left_live_sessions().unwrap().is_empty());
}

/// A daemon that stops its sessions leaves them marked, so the next start still knows they were
/// running although each now reads as interrupted.
#[tokio::test]
async fn stopping_the_sessions_marks_the_running_ones() {
    let (state, dir) = crate::test_support::app_state("resume-on-start-stop");
    crate::state::test_support::insert_record(&state, &dir, "running", SessionStatus::Working);
    state
        .store
        .insert_session(&crate::state::test_support::session_record(
            "before",
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let live =
        crate::test_support::fake_live_session("running", AgentId::CLAUDE, 80, 24, "sleep 30");
    state.register_live(live.clone());

    state.stop_all_sessions().await;

    let picked: Vec<String> = state
        .store
        .take_left_live_sessions()
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect();
    assert_eq!(picked, ["running"]);
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// Every session is resumed once, a failure does not stop the rest, and no more than `WIDTH` run
/// at the same time.
#[tokio::test]
async fn the_resumes_are_limited_in_width_and_a_failure_does_not_stop_the_rest() {
    let (state, _dir) = console_with_project_and_accounts("resume-on-start-run");
    let in_flight = Arc::new(AtomicUsize::new(0));
    let widest = Arc::new(AtomicUsize::new(0));
    let resumed = Arc::new(Mutex::new(Vec::new()));
    let names = ["a", "b", "c", "d", "e", "f", "g"];

    run(&state, ids(&names), Duration::ZERO, |_, id| {
        let (in_flight, widest, resumed) = (in_flight.clone(), widest.clone(), resumed.clone());
        async move {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            widest.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(30)).await;
            in_flight.fetch_sub(1, Ordering::SeqCst);
            resumed.lock().unwrap().push(id.clone());
            if id == "b" {
                anyhow::bail!("the conversation is gone");
            }
            Ok(())
        }
    })
    .await;

    let mut done = resumed.lock().unwrap().clone();
    done.sort();
    assert_eq!(done, names);
    assert!(widest.load(Ordering::SeqCst) <= WIDTH);
    assert!(
        widest.load(Ordering::SeqCst) > 1,
        "they do run side by side"
    );
}

/// Nothing new is started once the daemon is shutting down.
#[tokio::test]
async fn no_resume_starts_once_the_daemon_is_shutting_down() {
    let (state, _dir) = console_with_project_and_accounts("resume-on-start-shutdown");
    state.stop_all_sessions().await;
    let started = Arc::new(AtomicUsize::new(0));
    let counted = started.clone();
    run(&state, ids(&["a", "b"]), Duration::ZERO, move |_, _| {
        let counted = counted.clone();
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    })
    .await;
    assert_eq!(started.load(Ordering::SeqCst), 0);
}

/// Shutting down in the middle of the burst stops new resumes but waits for the running ones, so
/// none is cancelled between its launch and its registration.
#[tokio::test]
async fn a_shutdown_mid_burst_waits_for_the_resumes_running() {
    let (state, _dir) = console_with_project_and_accounts("resume-on-start-mid-burst");
    let finished = Arc::new(AtomicUsize::new(0));
    let counted = finished.clone();
    let stopper = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        stopper.stop_all_sessions().await;
    });
    run(
        &state,
        ids(&["a", "b"]),
        Duration::from_millis(60),
        move |_, _| {
            let counted = counted.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
    )
    .await;
    assert_eq!(finished.load(Ordering::SeqCst), 1);
}

/// A resume that found its session being started already is tried once more, and no more than once.
#[tokio::test]
async fn a_session_being_started_already_is_retried_once() {
    let (state, _dir) = console_with_project_and_accounts("resume-on-start-retry");
    let tries = Arc::new(Mutex::new(Vec::new()));
    let recorded = tries.clone();
    run(&state, ids(&["a", "b"]), Duration::ZERO, move |_, id| {
        let recorded = recorded.clone();
        async move {
            let mut tried = recorded.lock().unwrap();
            tried.push(id.clone());
            // "a" never gets through; "b" does on its second try.
            if id == "a" || tried.iter().filter(|tried| *tried == "b").count() < 2 {
                return Err(CodedError::raised(
                    error_code::SESSION_ALREADY_STARTING,
                    "starting",
                    &[],
                ));
            }
            Ok(())
        }
    })
    .await;
    let mut tried = tries.lock().unwrap().clone();
    tried.sort();
    assert_eq!(tried, ["a", "a", "b", "b"]);
}

/// A session the user archived or resumed since the start is left as it is.
#[tokio::test]
async fn a_session_no_longer_interrupted_is_not_resumed() {
    let (state, _dir) = console_with_project_and_accounts("resume-on-start-recheck");
    for (id, status) in [
        ("archived", SessionStatus::Archived),
        ("working", SessionStatus::Working),
    ] {
        state
            .store
            .insert_session(&project_session(id, AgentId::CLAUDE, status))
            .unwrap();
        resume_if_still_interrupted(&state, id).await.unwrap();
        assert_eq!(state.session_record(id).unwrap().status, status);
    }
}

/// With the setting off, nothing is resumed and nothing is spawned.
#[tokio::test]
async fn the_setting_off_resumes_nothing() {
    let (state, _dir) = console_with_project_and_accounts("resume-on-start-off");
    let called = Arc::new(AtomicUsize::new(0));
    let counted = called.clone();
    let left = vec![project_session(
        "s",
        AgentId::CLAUDE,
        SessionStatus::Working,
    )];
    spawn_with(&state, left, move |_, _| {
        let counted = counted.clone();
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(called.load(Ordering::SeqCst), 0);
}
