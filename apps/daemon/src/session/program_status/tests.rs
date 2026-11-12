use std::time::Instant;

use super::*;
use crate::protocol::AgentId;
use crate::state::test_support::insert_record;
use crate::terminal::fan::{Delivery, Outgoing};
use crate::terminal::saved_screen::SavedScreen;
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
/// (the reports are taken out of what the clients receive) but the reports move no status.
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

/// What an agent whose hooks and signals give its status gets of the reports is the state of its
/// turn, in place of the glyphs its title drops: they move the title activity, and nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_with_its_own_status_has_its_turn_read_off_the_reports() {
    for agent in [AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK] {
        let (state, dir) = app_state("program-status-activity");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", agent, 80, 24, "sleep 30");
        state.register_live(live.clone());
        assert!(!live.title_activity().shows_running(Instant::now()));

        live.on_output(b"\x1b]7501;state=idle:app=claude-code\x1b\\");
        assert!(
            !live.title_activity().shows_running(Instant::now()),
            "{agent:?}"
        );
        assert_eq!(live.title_activity().stops(), 0, "{agent:?}");

        live.on_output(b"\x1b]7501;state=working:app=claude-code\x1b\\");
        assert!(
            live.title_activity().shows_running(Instant::now()),
            "{agent:?}"
        );

        live.on_output(b"\x1b]7501;state=idle:app=claude-code\x1b\\");
        assert!(
            !live.title_activity().shows_running(Instant::now()),
            "{agent:?}"
        );
        assert_eq!(live.title_activity().stops(), 1, "{agent:?}");

        // The title says nothing from the first report on, whichever glyph it carries.
        live.on_output("\x1b]0;\u{25D0} Claude Code\x07".as_bytes());
        assert!(
            !live.title_activity().shows_running(Instant::now()),
            "{agent:?}"
        );

        // Nothing of the session's own changed: no move was queued, no status given.
        {
            let moves = live.status_reports.moves();
            assert!(moves.pending.is_empty() && !moves.draining && moves.applied.is_none());
        }
        let record = state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::Idle, "{agent:?}");
        assert!(!record.has_conversation, "{agent:?}");
    }
}

/// Every state that is not `working`, and the record's going away, reads as the turn back at the
/// prompt, for an agent whose hooks give the status as for one whose reports do.
#[tokio::test(flavor = "multi_thread")]
async fn each_state_a_report_leaves_a_turn_for_reads_as_back_at_the_prompt() {
    for (report, name) in [
        (&b"state=idle"[..], "idle"),
        (b"state=done", "done"),
        (b"state=error", "error"),
        (b"state=blocked:kind=permission", "blocked"),
        (b"state=clear", "clear"),
    ] {
        for agent in [AgentId::CLAUDE, AgentId::TEST] {
            let (state, dir) = app_state("program-status-leaving");
            insert_record(&state, &dir, "s", SessionStatus::Idle);
            let live = fake_live_session("s", agent, 80, 24, "sleep 30");
            state.register_live(live.clone());
            live.on_output(b"\x1b]7501;state=working\x07");
            assert!(live.title_activity().shows_running(Instant::now()));
            live.on_output(&[&b"\x1b]7501;"[..], report, b"\x07"].concat());
            assert!(
                !live.title_activity().shows_running(Instant::now()),
                "{name} {agent:?}"
            );
            assert_eq!(live.title_activity().stops(), 1, "{name} {agent:?}");
        }
    }
}

/// A session whose status the reports give still has them move its turn's activity.
#[tokio::test(flavor = "multi_thread")]
async fn reports_move_the_turn_activity_of_an_agent_whose_status_they_give_too() {
    let (state, dir) = app_state("program-status-activity-and-status");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
    state.register_live(live.clone());

    live.on_output(b"\x1b]7501;state=working\x07");
    assert!(live.title_activity().shows_running(Instant::now()));
    until_status(&state, SessionStatus::Working).await;
    live.on_output(b"\x1b]7501;state=idle\x07");
    assert!(!live.title_activity().shows_running(Instant::now()));
    until_status(&state, SessionStatus::Idle).await;
}

/// Reports the program made at no change in its root record move nothing: a child's, one with an
/// unknown state, and the root's own state repeated.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_move_of_the_root_record_moves_the_turn_activity() {
    let (state, dir) = app_state("program-status-root-only");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
    state.register_live(live.clone());

    live.on_output(b"\x1b]7501;state=working:id=child\x07\x1b]7501;state=asleep\x07");
    let activity = live.title_activity();
    assert!(!activity.shows_running(Instant::now()));
    // No report of the root yet: its title still governs.
    live.on_output("\x1b]0;\u{25D0} Claude Code\x07".as_bytes());
    assert!(live.title_activity().shows_running(Instant::now()));

    live.on_output(b"\x1b]7501;state=working\x07");
    live.on_output(b"\x1b]7501;state=working:progress=40\x07");
    assert!(live.title_activity().shows_running(Instant::now()));
    assert_eq!(live.title_activity().stops(), 0);
}

/// A terminal adopted at a planned restart is restored with its program's root status record and
/// no move of it, so the record is taken as the program's last report: a turn running across the
/// restart reads as running, though the program reports it no more.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_running_across_a_restart_reads_as_running_once_the_restored_record_is_noted() {
    let (state, dir) = app_state("program-status-restored-working");
    insert_record(&state, &dir, "s", SessionStatus::Working);
    let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
    state.register_live(live.clone());
    live.restore_screen(&SavedScreen::of_bytes(
        24,
        80,
        b"\x1b]7501;state=working:app=claude-code\x1b\\",
    ));
    assert!(live.pty.take_program_status().is_empty());
    assert!(!live.title_activity().shows_running(Instant::now()));

    live.note_restored_program_status(&state);
    // Its hooks give Claude Code's session its status and its turns, so none is opened here.
    assert!(!state.turn_is_open("s"));
    let activity = live.title_activity();
    assert!(activity.shows_running(Instant::now() + std::time::Duration::from_secs(3600)));
    assert_eq!(activity.stops(), 0);
    // From now on the reports govern, as they do in a session that never restarted.
    live.on_output("\x1b]0;\u{2733} Claude Code\x07".as_bytes());
    assert!(live.title_activity().shows_running(Instant::now()));
    live.on_output(b"\x1b]7501;state=idle\x07");
    assert!(!live.title_activity().shows_running(Instant::now()));
    assert_eq!(live.title_activity().stops(), 1);
    // The session's status is the store's, kept across the restart.
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
}

/// A restored record that is not `working` is the program at its prompt, and it too has the
/// reports govern from then on; one the program never made leaves the title in charge.
#[tokio::test(flavor = "multi_thread")]
async fn a_restored_record_not_working_reads_as_the_prompt_and_none_leaves_the_title_in_charge() {
    for (saved, reports) in [
        (&b"\x1b]7501;state=idle\x07"[..], true),
        (b"\x1b]7501;state=done\x07", true),
        (b"\x1b]7501;state=blocked:kind=question\x07", true),
        (b"some output\r\n", false),
        (b"\x1b]7501;state=working:id=child\x07", false),
    ] {
        let (state, dir) = app_state("program-status-restored-other");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        live.restore_screen(&SavedScreen::of_bytes(24, 80, saved));
        live.note_restored_program_status(&state);
        assert!(
            !live.title_activity().shows_running(Instant::now()),
            "{saved:?}"
        );
        assert_eq!(live.title_activity().stops(), 0, "{saved:?}");

        live.on_output("\x1b]0;\u{25D0} Claude Code\x07".as_bytes());
        assert_eq!(
            live.title_activity().shows_running(Instant::now()),
            !reports,
            "{saved:?}"
        );
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

/// The move a program reports after a restart is applied from the root record the terminal was
/// restored with, not from nothing: a turn running across the restart, reported done after it, is
/// closed and reported as any other turn's end, rather than left open.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_running_across_a_restart_is_closed_by_the_report_that_ends_it() {
    let (state, dir) = app_state("program-status-restored-turn");
    insert_record(&state, &dir, "s", SessionStatus::Working);
    let ends = state.turn_ends("s");
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
    live.restore_screen(&SavedScreen::of_bytes(
        24,
        80,
        b"\x1b]7501;state=working\x1b\\",
    ));
    live.note_restored_program_status(&state);
    state.register_live(live.clone());

    assert!(
        state.turn_is_open("s"),
        "the turn that ran across the restart is open again"
    );
    // Held up within the turn, which opens no other.
    let opened = state.turn_opened("s");
    live.on_output(b"\x1b]7501;state=blocked\x1b\\");
    until_status(&state, SessionStatus::WaitingUser).await;
    live.on_output(b"\x1b]7501;state=working\x1b\\");
    until_status(&state, SessionStatus::Working).await;
    assert_eq!(state.turn_opened("s"), opened);
    live.on_output(b"\x1b]7501;state=done\x1b\\");
    until_status(&state, SessionStatus::Idle).await;
    live.settle_program_status().await;

    assert!(
        !state.turn_is_open("s"),
        "the turn the report ended is closed"
    );
    assert_ne!(state.turn_ends("s"), ends);
}
