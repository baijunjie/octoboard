use std::time::Instant;

use super::*;
use crate::protocol::AgentId;
use crate::state::test_support::insert_record;
use crate::terminal::fan::{Delivery, Outgoing};
use crate::test_support::{app_state, fake_live_session, PATIENCE};

#[test]
fn each_reported_state_maps_onto_its_wire_status() {
    use ReportedState::*;
    assert_eq!(wire_status(Some(Working)), SessionStatus::Working);
    assert_eq!(wire_status(Some(Blocked)), SessionStatus::WaitingUser);
    for root in [Some(Idle), Some(Done), Some(Error), None] {
        assert_eq!(wire_status(root), SessionStatus::Idle, "{root:?}");
    }
}

/// Working opens a turn and a conversation, blocked keeps the turn open with the hand up, and
/// leaving both closes it: error leaves the session paused, done does not.
#[tokio::test]
async fn reported_moves_set_the_status_turns_and_pause() {
    let (state, dir) = app_state("program-status-moves");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let status = |state: &Arc<AppState>| state.session_record("s").unwrap().status;

    apply(&state, "s", None, Some(ReportedState::Working));
    assert_eq!(status(&state), SessionStatus::Working);
    assert!(state.session_record("s").unwrap().has_conversation);
    // Turn numbers are drawn from a count every test shares.
    let first = state.turn_opened("s");
    assert_ne!(first, 0);

    apply(
        &state,
        "s",
        Some(ReportedState::Working),
        Some(ReportedState::Blocked),
    );
    assert_eq!(status(&state), SessionStatus::WaitingUser);
    apply(
        &state,
        "s",
        Some(ReportedState::Blocked),
        Some(ReportedState::Working),
    );
    assert_eq!(state.turn_opened("s"), first, "blocked is within the turn");

    apply(
        &state,
        "s",
        Some(ReportedState::Working),
        Some(ReportedState::Error),
    );
    assert_eq!(status(&state), SessionStatus::Idle);
    assert!(state.last_turn_abnormal("s"));

    apply(
        &state,
        "s",
        Some(ReportedState::Error),
        Some(ReportedState::Working),
    );
    assert!(state.turn_opened("s") > first);
    assert!(!state.last_turn_abnormal("s"));
    apply(
        &state,
        "s",
        Some(ReportedState::Working),
        Some(ReportedState::Done),
    );
    assert_eq!(status(&state), SessionStatus::Idle);
    assert!(!state.last_turn_abnormal("s"));
}

/// Waits until the session's recorded status is `expected`, failing after [`PATIENCE`].
async fn until_status(state: &Arc<AppState>, expected: SessionStatus) {
    let deadline = Instant::now() + PATIENCE;
    while state.session_record("s").unwrap().status != expected {
        assert!(Instant::now() < deadline, "never became {expected:?}");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// A program in the terminal of an agent with no status of its own moves its session through
/// working, waiting for the user and idle by what it reports.
#[tokio::test(flavor = "multi_thread")]
async fn a_programs_reports_move_a_session_with_no_other_status() {
    let (state, dir) = app_state("program-status-reports");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
    state.register_live(live.clone());

    live.on_output(b"\x1b]7501;state=working:progress=10\x07");
    until_status(&state, SessionStatus::Working).await;
    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x1b\\");
    until_status(&state, SessionStatus::WaitingUser).await;
    // An unknown state, and a report breaking a limit, are ignored.
    live.on_output(b"\x1b]7501;state=asleep\x07");
    live.on_output(b"\x1b]7501;state=idle:id=a//b\x07");
    live.on_output(b"\x1b]7501;state=idle\x07");
    until_status(&state, SessionStatus::Idle).await;
    assert!(!state.last_turn_abnormal("s"));
}

/// Reports made before the session is bound to the daemon's state are applied once it is.
#[tokio::test(flavor = "multi_thread")]
async fn reports_made_before_the_session_is_bound_are_applied_once_it_is() {
    let (state, dir) = app_state("program-status-early");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
    live.on_output(b"\x1b]7501;state=working\x07");
    state.register_live(live.clone());
    until_status(&state, SessionStatus::Working).await;
}

/// Where an agent's hooks and signals give the status, its terminal still supports the protocol
/// (the reports are taken out of what the clients receive) but the reports move nothing.
#[tokio::test(flavor = "multi_thread")]
async fn hooks_and_agent_signals_win_over_a_programs_reports() {
    for agent in [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK] {
        let (state, dir) = app_state("program-status-integration");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", agent, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let client = live.attach(0).answering.expect("a live client").client;

        live.on_output(b"\x1b]7501;?\x07\x1b]7501;state=working\x07\x1b]7501;state=blocked\x1b\\x");
        let sent: Vec<u8> = match live.take_output(client) {
            Delivery::Send(frames) => frames
                .iter()
                .flat_map(|frame| match frame {
                    Outgoing::Output(bytes) => bytes.to_vec(),
                    Outgoing::Paint { .. } => panic!("a paint in place of output"),
                })
                .collect(),
            Delivery::Wait | Delivery::Over => panic!("no output after the reports"),
        };
        assert_eq!(
            sent, b"x",
            "the support query, answered, and the reports are kept from the clients"
        );
        // Moves are queued on the reader's own call, so none was, nor is one being applied.
        {
            let moves = live.status_reports.moves();
            assert!(moves.pending.is_empty() && !moves.draining && moves.applied.is_none());
        }
        let record = state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::Idle);
        assert!(!record.has_conversation);
        assert_eq!(state.turn_opened("s"), 0);
    }
}

/// A report the program made before it exited, still being applied when the exit is seen, decides
/// how its turn ended before the exit is recorded.
#[tokio::test(flavor = "multi_thread")]
async fn a_report_still_being_applied_when_the_process_exits_decides_how_the_turn_ended() {
    let (state, dir) = app_state("program-status-exit");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "true");
    state.register_live(live.clone());
    live.on_output(b"\x1b]7501;state=working\x07");
    until_status(&state, SessionStatus::Working).await;

    // The program's last report, read and held back from being applied (by a thread of its own,
    // so no lock is held across this test's awaits).
    let (locked, release) = {
        let (locked_tx, locked) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let reports = live.status_reports.clone();
        std::thread::spawn(move || {
            let _held = reports.gate.lock().unwrap();
            locked_tx.send(()).unwrap();
            released.recv().ok();
        });
        (locked, release)
    };
    locked.recv().unwrap();
    live.on_output(b"\x1b]7501;state=done\x07");
    crate::coordinator::exit_watch::watch_exit(&state, live.clone());
    let deadline = Instant::now() + PATIENCE;
    while !live.poll_exit() {
        assert!(Instant::now() < deadline, "the stand-in never exited");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    // Past the exit watcher's poll that sees the process gone.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working,
        "the exit is not recorded while the report is being applied"
    );
    release.send(()).unwrap();

    let deadline = Instant::now() + PATIENCE;
    while state.live_session("s").is_some() {
        assert!(Instant::now() < deadline, "the exit was never recorded");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Interrupted
    );
    assert!(!state.last_turn_abnormal("s"), "the turn ended with done");
}

/// A shell prompt in the terminal ends what the program said it was doing.
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_ends_a_working_programs_turn() {
    let (state, dir) = app_state("program-status-prompt");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
    state.register_live(live.clone());
    live.on_output(b"\x1b]7501;state=working\x07");
    until_status(&state, SessionStatus::Working).await;
    live.on_output(b"\x1b]133;A\x07$ ");
    until_status(&state, SessionStatus::Idle).await;
    assert!(!state.last_turn_abnormal("s"));
}

/// A move that reaches a session whose process has ended opens no turn and moves nothing.
#[tokio::test]
async fn a_move_after_the_process_ended_changes_nothing() {
    let (state, dir) = app_state("program-status-late");
    insert_record(&state, &dir, "s", SessionStatus::Interrupted);
    apply(&state, "s", None, Some(ReportedState::Working));
    assert_eq!(state.turn_opened("s"), 0);
    let record = state.session_record("s").unwrap();
    assert_eq!(record.status, SessionStatus::Interrupted);
    assert!(!record.has_conversation);
}
