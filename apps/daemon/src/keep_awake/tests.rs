use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::broadcast::Receiver;

use super::*;
use crate::coordinator::settings::{update_settings, SettingsChanges};
use crate::protocol::BackgroundTask;
use crate::test_support::{app_state, project_session, PATIENCE};

/// The holds a test's stand-in has asked for and the ones still standing; the stand-in refuses
/// while `refusing` is set.
#[derive(Default)]
struct Holds {
    taken: AtomicUsize,
    standing: AtomicUsize,
    refusing: AtomicBool,
}

struct StandIn(Arc<Holds>);

impl Drop for StandIn {
    fn drop(&mut self) {
        self.0.standing.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Starts the task over a stand-in for the host, which can hold as `available` says and refuses
/// while `refuse` is set.
fn start_on(state: &Arc<AppState>, available: bool, refuse: bool) -> (Arc<Holds>, Receiver<Event>) {
    let holds = Arc::new(Holds::default());
    holds.refusing.store(refuse, Ordering::SeqCst);
    let own = holds.clone();
    let events = state.subscribe();
    spawn_with(
        state,
        move || available,
        move || {
            own.taken.fetch_add(1, Ordering::SeqCst);
            if own.refusing.load(Ordering::SeqCst) {
                anyhow::bail!("the host refuses");
            }
            own.standing.fetch_add(1, Ordering::SeqCst);
            Ok(StandIn(own.clone()))
        },
    );
    (holds, events)
}

fn start(state: &Arc<AppState>, refuse: bool) -> (Arc<Holds>, Receiver<Event>) {
    start_on(state, true, refuse)
}

fn set(state: &Arc<AppState>, keep_awake: bool, when: KeepAwakeWhen) {
    update_settings(
        state,
        SettingsChanges {
            keep_awake: Some(keep_awake),
            keep_awake_when: Some(when),
            ..Default::default()
        },
    )
    .unwrap();
}

/// Waits for the next `keep_awake_held_updated` and gives what it says.
async fn next_held(events: &mut Receiver<Event>) -> bool {
    tokio::time::timeout(PATIENCE, async {
        loop {
            if let Event::KeepAwakeHeldUpdated { keep_awake_held } = events.recv().await.unwrap() {
                return keep_awake_held;
            }
        }
    })
    .await
    .expect("the hold was announced")
}

/// Waits until `condition` holds.
async fn until(condition: impl Fn() -> bool) {
    tokio::time::timeout(PATIENCE, async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the condition came true");
}

/// Gives the task time to see everything sent so far, for a check that it did nothing, which has
/// no event to wait for.
async fn settled() {
    tokio::time::sleep(Duration::from_millis(200)).await;
}

fn working_session(id: &str, status: SessionStatus) -> Session {
    project_session(id, crate::protocol::AgentId::CLAUDE, status)
}

fn publish(state: &Arc<AppState>, session: &Session) {
    state.broadcast(Event::SessionUpserted {
        session: session.clone(),
    });
}

#[test]
fn a_session_is_working_as_the_ui_reads_it() {
    let at = |status| working_session("s", status);
    for working in [
        SessionStatus::Starting,
        SessionStatus::Working,
        SessionStatus::Dormant,
    ] {
        assert!(session_is_working(&at(working)), "{working:?}");
    }
    for idle in [
        SessionStatus::Idle,
        SessionStatus::WaitingUser,
        SessionStatus::Interrupted,
        SessionStatus::Archived,
    ] {
        assert!(!session_is_working(&at(idle)), "{idle:?}");
    }
    let mut background = at(SessionStatus::Idle);
    background.background_tasks = vec![BackgroundTask {
        id: "t".to_string(),
        kind: None,
        description: None,
    }];
    assert!(session_is_working(&background));
    background.status = SessionStatus::WaitingUser;
    assert!(!session_is_working(&background));
}

#[tokio::test]
async fn nothing_is_held_while_the_setting_is_off() {
    let (state, _dir) = app_state("keep-awake-off");
    let (holds, _events) = start(&state, false);
    set(&state, false, KeepAwakeWhen::Always);
    settled().await;
    assert_eq!(holds.taken.load(Ordering::SeqCst), 0);
    assert!(!state.keep_awake_held());
}

#[tokio::test]
async fn always_holds_from_the_start_and_releases_when_turned_off() {
    let (state, _dir) = app_state("keep-awake-always");
    state.store.set_keep_awake(true).unwrap();
    state
        .store
        .set_keep_awake_when(KeepAwakeWhen::Always)
        .unwrap();
    let (holds, mut events) = start(&state, false);

    assert!(
        next_held(&mut events).await,
        "held at the start, with no client"
    );
    assert!(state.keep_awake_held());
    assert_eq!(holds.standing.load(Ordering::SeqCst), 1);

    set(&state, false, KeepAwakeWhen::Always);
    assert!(!next_held(&mut events).await);
    assert!(!state.keep_awake_held());
    assert_eq!(holds.standing.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn working_holds_only_while_a_session_works() {
    let (state, _dir) = app_state("keep-awake-working");
    let (holds, mut events) = start(&state, false);
    set(&state, true, KeepAwakeWhen::Working);
    settled().await;
    assert_eq!(
        holds.taken.load(Ordering::SeqCst),
        0,
        "no session is working"
    );

    publish(&state, &working_session("s", SessionStatus::Working));
    assert!(next_held(&mut events).await);
    assert_eq!(holds.standing.load(Ordering::SeqCst), 1);

    // A second working session changes nothing, and one waiting for the user is not at work; the
    // last working session being deleted releases the hold, and only then.
    publish(&state, &working_session("t", SessionStatus::Starting));
    publish(&state, &working_session("s", SessionStatus::WaitingUser));
    state.broadcast(Event::SessionDeleted {
        session: "t".to_string(),
    });
    assert!(!next_held(&mut events).await);
    assert_eq!(holds.taken.load(Ordering::SeqCst), 1);
    assert_eq!(holds.standing.load(Ordering::SeqCst), 0);

    // Switching to `always` holds without any session working.
    set(&state, true, KeepAwakeWhen::Always);
    assert!(next_held(&mut events).await);
}

#[tokio::test]
async fn deleting_a_project_releases_the_hold_of_its_sessions() {
    let (state, _dir) = app_state("keep-awake-project-deleted");
    let (holds, mut events) = start(&state, false);
    set(&state, true, KeepAwakeWhen::Working);
    publish(&state, &working_session("s", SessionStatus::Working));
    assert!(next_held(&mut events).await);

    // The sessions of a deleted project go with it with no `session_deleted` each, and the store
    // has none of them left.
    state.broadcast(Event::ProjectDeleted {
        project: "project-1".to_string(),
    });
    assert!(!next_held(&mut events).await);
    assert_eq!(holds.standing.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn deleting_a_console_releases_the_hold_of_its_sessions() {
    let (state, _dir) = app_state("keep-awake-console-deleted");
    let (holds, mut events) = start(&state, false);
    set(&state, true, KeepAwakeWhen::Working);
    publish(&state, &working_session("s", SessionStatus::Working));
    assert!(next_held(&mut events).await);

    state.broadcast(Event::ConsoleDeleted {
        console: "console-1".to_string(),
    });
    assert!(!next_held(&mut events).await);
    assert_eq!(holds.standing.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_host_that_cannot_hold_is_never_asked() {
    let (state, _dir) = app_state("keep-awake-unavailable");
    let (holds, _events) = start_on(&state, false, false);
    set(&state, true, KeepAwakeWhen::Always);
    settled().await;
    assert_eq!(holds.taken.load(Ordering::SeqCst), 0);
    assert!(!state.keep_awake_held());
}

#[tokio::test]
async fn a_refused_hold_is_reported_not_held_and_is_not_asked_for_again_meanwhile() {
    let (state, _dir) = app_state("keep-awake-refused");
    let (holds, mut events) = start(&state, true);
    set(&state, true, KeepAwakeWhen::Always);
    until(|| holds.taken.load(Ordering::SeqCst) == 1).await;
    assert!(!state.keep_awake_held());

    // Nothing that leaves the answer at true asks again.
    publish(&state, &working_session("s", SessionStatus::Working));
    state.broadcast(Event::SessionDeleted {
        session: "s".to_string(),
    });
    set(&state, true, KeepAwakeWhen::Always);
    settled().await;
    assert_eq!(holds.taken.load(Ordering::SeqCst), 1);

    // Once the answer has gone false and true again, it does, and this time the host allows it.
    set(&state, false, KeepAwakeWhen::Always);
    settled().await;
    holds.refusing.store(false, Ordering::SeqCst);
    set(&state, true, KeepAwakeWhen::Always);
    assert!(next_held(&mut events).await);
    assert_eq!(holds.taken.load(Ordering::SeqCst), 2);
}
