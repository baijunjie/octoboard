use std::time::{Duration, Instant};

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

/// Waits until `holds`, failing after [`PATIENCE`] with `what` never having come about.
async fn until(what: &str, holds: impl Fn() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !holds() {
        assert!(Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// A program in the terminal of an agent with no status of its own moves its session through
/// working, waiting for the user and its prompt by what it reports; a turn that ends on `done`
/// leaves it not paused.
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
    live.on_output(b"\x1b]7501;state=done\x07");
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
                    Outgoing::Size(_) => panic!("a size change in place of output"),
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
    live.on_output("Ready\x1b]0;\u{25D0} Claude Code\x07".as_bytes());
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
    // The exit watcher has no signal for reaching the report gate before it can record the exit.
    const EXIT_WATCH_OBSERVATION: Duration = std::time::Duration::from_secs(1);
    tokio::time::sleep(EXIT_WATCH_OBSERVATION).await;
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

/// A turn that ends on a `done` or an `error` report is reported to the session's owner with the
/// report's `msg` as what the turn said: the reply, or the failure's text.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_ended_by_a_done_or_error_report_carries_the_reports_msg() {
    use crate::test_support::owner_team::{hub_heard_of, team_state};
    for (name, ended, said) in [
        (
            "program-status-done-msg",
            ReportedState::Done,
            "the answer is 42",
        ),
        (
            "program-status-error-msg",
            ReportedState::Error,
            "the model was unreachable",
        ),
    ] {
        let (state, _dir, hub) = team_state(name, &[("s", "hub", false, SessionStatus::Idle)]);
        apply(&state, "s", None, Some(ReportedState::Working));
        apply(
            &state,
            "s",
            Some(ReportedState::Working),
            RootMove {
                state: Some(ended),
                kind: None,
                msg: Some(said.to_string()),
            },
        );
        let heard = hub_heard_of(&hub, said).await;
        assert!(heard.contains(said), "{name}: {heard}");
    }
}

/// A `msg` is the turn's final message only on the report that ends the turn as done or failed,
/// and only when it says something: a turn whose record went away is reported with none, as is
/// one whose `done` carries a blank `msg`, like a program whose reports carry none.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_ended_otherwise_or_with_a_blank_msg_carries_no_message() {
    use crate::test_support::owner_team::{hub_heard_of, team_state};
    for (name, ended, msg) in [
        ("program-status-gone-msg", None, Some("left over")),
        (
            "program-status-blank-msg",
            Some(ReportedState::Done),
            Some("  "),
        ),
        ("program-status-no-msg", Some(ReportedState::Done), None),
    ] {
        let (state, _dir, hub) = team_state(name, &[("s", "hub", false, SessionStatus::Idle)]);
        apply(&state, "s", None, Some(ReportedState::Working));
        apply(
            &state,
            "s",
            Some(ReportedState::Working),
            RootMove {
                state: ended,
                kind: None,
                msg: msg.map(str::to_string),
            },
        );
        // The report of a turn with no message says so, which is what is waited for.
        let heard = hub_heard_of(&hub, "it said nothing").await;
        assert!(heard.contains("it said nothing"), "{name}: {heard}");
        assert!(!heard.contains("left over"), "{name}: {heard}");
    }
}

/// A session of the test-only provider bound to the owner `hub`, at its prompt with a process whose
/// reports give it its status, and everything published about it from now on.
fn reporting_child(
    name: &str,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    crate::test_support::StandIn,
    tokio::sync::broadcast::Receiver<crate::protocol::Event>,
) {
    let (state, dir, hub) = crate::test_support::owner_team::team_state(
        name,
        &[("s", "hub", false, SessionStatus::Idle)],
    );
    let mut record = state.session_record("s").unwrap();
    record.agent = AgentId::TEST;
    record.has_conversation = false;
    state.store.update_session(&record).unwrap();
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
    state.register_live(live.clone());
    let events = state.subscribe();
    (state, dir, hub, live, events)
}

/// Whether `events` hold a publication of the session as waiting for the user.
fn published_waiting(
    events: &mut tokio::sync::broadcast::Receiver<crate::protocol::Event>,
) -> bool {
    let mut waiting = false;
    while let Ok(event) = events.try_recv() {
        if let crate::protocol::Event::SessionUpserted { session } = event {
            waiting |= session.id == "s" && session.status == SessionStatus::WaitingUser;
        }
    }
    waiting
}

/// A `blocked` report of kind `auth` that the next report replaces within the settle is passed
/// over whole: the session is never published as waiting for the user, no turn is opened for it,
/// so nothing is reported at its end either, and its owner is told nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_a_sign_in_replaced_within_the_settle_is_passed_over() {
    let (state, _dir, hub, live, mut events) = reporting_child("program-status-auth-passing");

    live.on_output(b"\x1b]7501;state=blocked:kind=auth\x07");
    live.on_output(b"\x1b]7501;state=idle\x07");
    live.settle_program_status().await;

    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );
    assert!(!published_waiting(&mut events));
    assert!(
        !state.session_record("s").unwrap().has_conversation,
        "no turn was opened"
    );
    let heard = crate::test_support::owner_team::hub_heard(&state, &hub).await;
    assert!(!heard.contains("only they can clear it"), "{heard}");
    assert!(!heard.contains("it said nothing"), "{heard}");
}

/// A `blocked` report of kind `auth` that stands for the settle is a sign-in the user has to make:
/// the session is then published as waiting for the user and its owner told so, once.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_a_sign_in_that_stays_is_shown_and_told_once_the_settle_has_passed() {
    /// Short, as a test proving a production window runs out keeps it.
    const SETTLE: Duration = Duration::from_millis(50);
    let (state, _dir, hub, live, _events) = reporting_child("program-status-auth-stays");

    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(Some(SETTLE)));
    live.on_output(b"\x1b]7501;state=blocked:kind=auth\x07");
    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(None));

    until_status(&state, SessionStatus::WaitingUser).await;
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "only they can clear it").await;
    assert_eq!(
        heard.matches("only they can clear it").count(),
        1,
        "{heard}"
    );
}

/// A `blocked` report of kind `permission` or `question`, or one that names no kind, is a wait on
/// a person from the moment it is reported: the session is published as waiting at once, with the
/// settle for a sign-in left at its longest, and an approval that takes it back to `working`
/// leaves it in the same turn.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_a_permission_or_a_question_is_shown_at_once() {
    for (name, report) in [
        (
            "program-status-permission",
            &b"\x1b]7501;state=blocked:kind=permission\x07"[..],
        ),
        (
            "program-status-question",
            b"\x1b]7501;state=blocked:kind=question\x07",
        ),
        ("program-status-no-kind", b"\x1b]7501;state=blocked\x07"),
    ] {
        let (state, _dir, _hub, live, _events) = reporting_child(name);
        live.on_output(b"\x1b]7501;state=working\x07");
        until_status(&state, SessionStatus::Working).await;
        let turn = state.turn_opened("s");

        live.on_output(report);
        until_status(&state, SessionStatus::WaitingUser).await;
        live.on_output(b"\x1b]7501;state=working\x07");
        until_status(&state, SessionStatus::Working).await;
        assert_eq!(state.turn_opened("s"), turn, "{name}: the same turn");
    }
}

/// A wait on a permission reported while a wait on a sign-in is still held replaces it: the
/// session is published as waiting at once, with the settle left far longer than the test waits,
/// and its owner told once. Answered, the session goes back to its prompt with no turn opened for
/// either wait, so none is left open and nothing is reported at an end.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_a_permission_within_a_sign_ins_settle_is_shown_at_once() {
    let (state, _dir, hub, live, _events) = reporting_child("program-status-auth-permission");

    // Only the report that replaces the held one can publish the wait inside the test's patience.
    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(Some(PATIENCE * 10)));
    live.on_output(b"\x1b]7501;state=blocked:kind=auth\x07");
    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(None));
    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x07");

    until_status(&state, SessionStatus::WaitingUser).await;
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "only they can clear it").await;
    assert_eq!(
        heard.matches("only they can clear it").count(),
        1,
        "{heard}"
    );
    assert!(
        !state.turn_is_open("s"),
        "a wait before any turn opens none"
    );

    live.on_output(b"\x1b]7501;state=idle\x07");
    until_status(&state, SessionStatus::Idle).await;
    live.settle_program_status().await;
    assert!(!state.turn_is_open("s"));
    assert!(!state.session_record("s").unwrap().has_conversation);
    let heard = crate::test_support::owner_team::hub_heard(&state, &hub).await;
    assert_eq!(
        heard.matches("only they can clear it").count(),
        1,
        "{heard}"
    );
    assert!(!heard.contains("it said nothing"), "{heard}");
}

/// A wait outside a turn that the program answers by going to work opens the turn there, and the
/// turn's end is reported as any turn's is.
#[tokio::test(flavor = "multi_thread")]
async fn working_after_a_wait_outside_a_turn_opens_the_turn() {
    let (state, _dir, hub, live, _events) = reporting_child("program-status-wait-then-work");

    live.on_output(b"\x1b]7501;state=blocked:kind=question\x07");
    until_status(&state, SessionStatus::WaitingUser).await;
    assert!(!state.turn_is_open("s"));
    live.on_output(b"\x1b]7501;state=working\x07");
    until_status(&state, SessionStatus::Working).await;
    assert!(state.turn_is_open("s"));
    assert!(state.session_record("s").unwrap().has_conversation);

    live.on_output(b"\x1b]7501;state=done\x07");
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "it said nothing").await;
    assert!(heard.contains("it said nothing"), "{heard}");
}

/// A wait on a sign-in still held when the process ends is dropped with it: the end is recorded
/// without waiting the settle out, the session is never published as waiting for the user, and
/// its owner is told of no wait.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_a_sign_in_held_when_the_process_ends_is_dropped() {
    let (state, _dir, hub, _live, mut events) = reporting_child("program-status-auth-exit");
    // A process of the session's that ends by itself, in place of the fixture's.
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "true");
    state.register_live(live.clone());

    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(Some(PATIENCE * 10)));
    live.on_output(b"\x1b]7501;state=blocked:kind=auth\x07");
    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(None));
    crate::coordinator::exit_watch::watch_exit(&state, live.clone());

    let deadline = Instant::now() + PATIENCE;
    while state.live_session("s").is_some() {
        assert!(Instant::now() < deadline, "the exit was never recorded");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(state.session_record("s").unwrap().status.has_no_process());
    assert!(!published_waiting(&mut events));
    let heard = crate::test_support::owner_team::hub_heard(&state, &hub).await;
    assert!(!heard.contains("only they can clear it"), "{heard}");
}

/// The root record each restored terminal was rebuilt with, and whether it reopens a turn: one
/// that says a turn runs does (`working`, and `blocked` on a permission or a question, which a
/// program asks within a turn); a `blocked` that names no kind, one on a sign-in, and a record
/// back at the prompt do not.
#[tokio::test(flavor = "multi_thread")]
async fn a_restored_record_reopens_a_turn_only_where_it_says_one_runs() {
    for (saved, reopens) in [
        (&b"\x1b]7501;state=working\x07"[..], true),
        (b"\x1b]7501;state=blocked:kind=permission\x07", true),
        (b"\x1b]7501;state=blocked:kind=question\x07", true),
        (b"\x1b]7501;state=blocked\x07", false),
        (b"\x1b]7501;state=blocked:kind=auth\x07", false),
        (b"\x1b]7501;state=idle\x07", false),
        (b"\x1b]7501;state=done\x07", false),
        (b"\x1b]7501;state=error\x07", false),
    ] {
        let (state, dir) = app_state("program-status-restored-roots");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
        live.restore_screen(&SavedScreen::of_bytes(24, 80, saved));
        live.note_restored_program_status(&state);
        assert_eq!(state.turn_is_open("s"), reopens, "{saved:?}");
    }
}

/// A restored `blocked` on a sign-in is held for its settle again, as when it was reported: the
/// report that replaces it passes it over, with the session never shown as waiting and no turn
/// ended; one that stands is shown once the settle has passed.
#[tokio::test(flavor = "multi_thread")]
async fn a_restored_wait_on_a_sign_in_is_held_for_its_settle_again() {
    let restored = |name: &str, settle: Option<Duration>| {
        let (state, dir, hub, _first, events) = reporting_child(name);
        let live = fake_live_session("s", AgentId::TEST, 80, 24, "sleep 30");
        live.restore_screen(&SavedScreen::of_bytes(
            24,
            80,
            b"\x1b]7501;state=blocked:kind=auth\x07",
        ));
        live.note_restored_program_status(&state);
        assert!(!state.turn_is_open("s"));
        // Registering has the queued move taken up, on this thread, which is where the settle
        // is read.
        AUTH_SETTLE_OVERRIDE.with(|held| held.set(settle));
        state.register_live(live.clone());
        AUTH_SETTLE_OVERRIDE.with(|held| held.set(None));
        (state, dir, hub, live, events)
    };

    let (state, _dir, hub, live, mut events) =
        restored("program-status-restored-auth-passing", None);
    live.on_output(b"\x1b]7501;state=idle\x07");
    live.settle_program_status().await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );
    assert!(!published_waiting(&mut events));
    let heard = crate::test_support::owner_team::hub_heard(&state, &hub).await;
    assert!(!heard.contains("only they can clear it"), "{heard}");
    assert!(!heard.contains("it said nothing"), "{heard}");

    let (state, _dir, _hub, _live, _events) = restored(
        "program-status-restored-auth-stays",
        Some(Duration::from_millis(50)),
    );
    until_status(&state, SessionStatus::WaitingUser).await;
}

/// A move held for its settle does not count as unsettled, so nothing that waits for a session's
/// reports to be applied waits the settle out; and a daemon that begins a planned stop drops the
/// held move, applying nothing of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_wait_on_a_sign_in_is_settled_and_dropped_by_a_planned_stop() {
    let (state, _dir, _hub, live, mut events) = reporting_child("program-status-auth-stop");
    // Longer than anything below waits, so only the stop can end the hold.
    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(Some(PATIENCE * 10)));
    live.on_output(b"\x1b]7501;state=blocked:kind=auth\x07");
    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(None));

    let reports = live.status_reports.clone();
    let deadline = Instant::now() + PATIENCE;
    while !reports.moves().holding {
        assert!(Instant::now() < deadline, "the move was never held");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(reports.moves().settled(), "a held move is not waited for");
    // Returns at once, where the bound it is given is `PATIENCE` under test.
    live.settle_program_status().await;
    assert!(reports.moves().holding, "still held, not applied");

    let _ = state.settle_for_planned_stop().await;
    let deadline = Instant::now() + PATIENCE;
    while reports.moves().draining {
        assert!(Instant::now() < deadline, "the stop never ended the hold");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(reports.moves().applied.is_none(), "dropped, not applied");
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );
    assert!(!published_waiting(&mut events));
}

// Antigravity CLI 1.24.0, one turn each with the OSC 7501 reports as it wrote them
// (`testdata/antigravity-captures.md`), replayed into a session of the test-only provider, whose
// status the reports give as they give Antigravity CLI's.
/// working; idle (Esc).
const CANCELLED_TURN: &[u8] = include_bytes!("../../../testdata/antigravity_turn_cancel_esc.bin");
/// working; blocked on a permission; idle (declined on "No, cancel").
const DECLINED_TURN: &[u8] =
    include_bytes!("../../../testdata/antigravity_turn_permission_declined.bin");
/// working; done, with the reply as its `msg`.
const ANSWERED_TURN: &[u8] = include_bytes!("../../../testdata/antigravity_turn_answer.bin");

/// What a synthesised report says of a turn that ended without the session reporting.
const NOT_REPORTED: &str = "stopped without reporting";

/// A turn the reports open with `working` and close with `idle`, with no `done` or `error`
/// between, was interrupted: the session is left paused with its turn closed, and its owner is
/// not sent a report saying it stopped without reporting, as after a cancel a hook reports.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_leaves_working_for_idle_was_interrupted() {
    for (name, capture) in [
        ("program-status-interrupted", CANCELLED_TURN),
        ("program-status-declined", DECLINED_TURN),
    ] {
        let (state, _dir, hub, live, _events) = reporting_child(name);
        live.on_output(capture);
        live.settle_program_status().await;

        let record = state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::Idle, "{name}");
        assert!(record.has_conversation, "{name}: the turn ran");
        assert!(state.last_turn_abnormal("s"), "{name}: paused");
        assert!(!state.turn_is_open("s"), "{name}: the turn is closed");
        let heard = crate::test_support::owner_team::hub_heard(&state, &hub).await;
        assert!(!heard.contains(NOT_REPORTED), "{name}: {heard}");
        assert!(!heard.contains("it said nothing"), "{name}: {heard}");
    }
}

/// A turn that ends on `done` ends as before: reported with the report's `msg`, not paused.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_ends_on_done_is_reported_and_not_paused() {
    let (state, _dir, hub, live, _events) = reporting_child("program-status-answered");
    live.on_output(ANSWERED_TURN);
    live.settle_program_status().await;
    assert!(!state.last_turn_abnormal("s"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "Stub: hello.").await;
    assert!(heard.contains(NOT_REPORTED), "{heard}");
}

/// One turn of two model calls as Qwen Code 0.28.3 reports it (`docs/agent-cli-reference.md`,
/// "Qwen Code"): `working`, `done` for the gap between the calls, `working` at the next call, and
/// the `done` that ends the turn, here with the reply.
fn two_model_calls() -> Vec<u8> {
    let mut reports = b"\x1b]7501;state=working\x07".to_vec();
    reports.extend(done_saying("between the calls"));
    reports.extend(b"\x1b]7501;state=working\x07");
    reports.extend(done_saying("the reply"));
    reports
}

/// A gap a provider declares; under test the window it opens ends on the test's word
/// (`LiveSession::end_reported_done_gaps`), whatever the figure.
const GAP: Duration = Duration::from_secs(1);

/// A provider that declares its `done` may be a gap inside a turn has the `done` that `working`
/// follows within the bound passed over: one turn is opened and closed, reported once with the
/// reply, and nothing of the gap is told. The `done` that ends the turn is applied once its
/// window has ended, and settling waits for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_done_between_two_model_calls_is_passed_over_where_the_provider_declares_the_gap() {
    let _gap = crate::test_support::test_provider::declaring_reported_done_gap(GAP);
    let (state, _dir, hub, live, _events) = reporting_child("program-status-done-gap");
    live.on_output(&two_model_calls());
    // Every report is taken in, so the `working` after the gap is pending when its `done` is
    // examined; the turn's own `done` is applied once its window ends.
    live.end_reported_done_gaps();
    live.settle_program_status().await;

    // `turn_opened` is a number drawn from a counter every session shares, not a count: a second
    // turn would have ended the first, so the count of ends says how many turns there were.
    assert_eq!(state.turn_ends("s"), 1, "one turn, ended once");
    assert!(!state.turn_is_open("s"));
    assert!(!state.last_turn_abnormal("s"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the reply").await;
    assert_eq!(heard.matches("the reply").count(), 1, "{heard}");
    assert!(!heard.contains("between the calls"), "{heard}");
}

/// Without the declaration every `done` ends the turn, as before: the same reports open and close
/// two turns.
#[tokio::test(flavor = "multi_thread")]
async fn a_done_between_two_model_calls_ends_a_turn_where_no_gap_is_declared() {
    let (state, _dir, hub, live, _events) = reporting_child("program-status-done-no-gap");
    live.on_output(&two_model_calls());
    live.settle_program_status().await;

    assert_eq!(state.turn_ends("s"), 2, "two turns, each ended");
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the reply").await;
    assert!(heard.contains("between the calls"), "{heard}");
}

/// A held `done` that a report other than `working` follows is applied first, as the end of the
/// turn it says: an `idle` after it does not make the turn an interrupted one.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_done_followed_by_another_report_ends_the_turn_as_it_says() {
    let _gap = crate::test_support::test_provider::declaring_reported_done_gap(GAP);
    let (state, _dir, hub, live, _events) = reporting_child("program-status-done-gap-idle");
    let mut reports = b"\x1b]7501;state=working\x07".to_vec();
    reports.extend(done_saying("the reply"));
    reports.extend(b"\x1b]7501;state=idle\x07");
    live.on_output(&reports);
    live.settle_program_status().await;

    assert_eq!(state.turn_ends("s"), 1);
    assert!(!state.turn_is_open("s"));
    assert!(
        !state.last_turn_abnormal("s"),
        "ended by its done, not cancelled"
    );
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the reply").await;
    assert!(heard.contains("the reply"), "{heard}");
}

/// A held `done` stays held, the turn open and the session working, for as long as its window is
/// open, and is applied as the end of the turn the moment the window ends: what a provider's
/// test relies on to hold the window for the gaps it feeds and end it for the turn's last `done`.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_done_waits_while_its_window_is_open_and_ends_the_turn_once_it_ends() {
    let _gap = crate::test_support::test_provider::declaring_reported_done_gap(GAP);
    let (state, _dir, hub, live, _events) = reporting_child("program-status-done-gap-window");
    let mut reports = b"\x1b]7501;state=working\x07".to_vec();
    reports.extend(done_saying("the reply"));
    live.on_output(&reports);
    // The `done` is taken and held: nothing is pending, the task applying the moves still runs,
    // and what it applied last is the `working`.
    until("the done held", || {
        let moves = live.status_reports.moves();
        moves.pending.is_empty() && moves.draining && moves.applied == Some(ReportedState::Working)
    })
    .await;
    assert!(state.turn_is_open("s"));
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
    assert!(
        !live.status_reports.moves().settled(),
        "settling waits for it"
    );

    live.end_reported_done_gaps();
    until_status(&state, SessionStatus::Idle).await;
    assert_eq!(state.turn_ends("s"), 1);
    assert!(!state.last_turn_abnormal("s"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the reply").await;
    assert!(heard.contains("the reply"), "{heard}");
}

/// A held `done` is the last word on the turn when the process ends inside the bound: it is
/// applied before the exit is recorded, so the turn ended with it rather than cut short.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_done_is_applied_when_the_process_ends_within_the_bound() {
    let _gap = crate::test_support::test_provider::declaring_reported_done_gap(GAP);
    let (state, dir) = app_state("program-status-done-gap-exit");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = fake_live_session("s", AgentId::TEST, 80, 24, "true");
    state.register_live(live.clone());
    live.on_output(b"\x1b]7501;state=working\x07");
    until_status(&state, SessionStatus::Working).await;
    live.on_output(b"\x1b]7501;state=done\x07");
    crate::coordinator::exit_watch::watch_exit(&state, live.clone());

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

/// The sequence Pi 1.5.2 reports for a launch with a task that is then cancelled
/// (`docs/agent-cli-reference.md`, "Pi": `idle` once started, `working` at the prompt, `idle` on
/// Esc). The `idle` before the first turn opens and closes nothing and tells nobody anything; the
/// one after `working` is the interrupted turn.
#[tokio::test(flavor = "multi_thread")]
async fn the_idle_a_launch_reports_before_its_first_turn_ends_nothing() {
    let (state, _dir, hub, live, _events) = reporting_child("program-status-first-idle");
    live.on_output(b"\x1b]7501;state=idle:app=pi\x1b\\");
    live.settle_program_status().await;
    let record = state.session_record("s").unwrap();
    assert_eq!(record.status, SessionStatus::Idle);
    assert!(!record.has_conversation);
    assert_eq!(state.turn_opened("s"), 0, "no turn was opened");
    assert_eq!(state.turn_ends("s"), 0, "and none closed");
    assert!(!state.last_turn_abnormal("s"));

    live.on_output(b"\x1b]7501;state=working:app=pi\x1b\\");
    live.settle_program_status().await;
    assert!(state.turn_is_open("s"));
    live.on_output(b"\x1b]7501;state=idle:app=pi\x1b\\");
    live.settle_program_status().await;
    assert!(!state.turn_is_open("s"));
    assert!(state.last_turn_abnormal("s"));

    let heard = crate::test_support::owner_team::hub_heard(&state, &hub).await;
    assert!(!heard.contains(NOT_REPORTED), "{heard}");
    assert!(!heard.contains("waiting for the user"), "{heard}");
}

/// An `idle` after a wait outside any turn (a screen the program shows as it starts) is no
/// interrupted turn: nothing is closed and the session is not paused.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_after_a_wait_outside_a_turn_is_no_interrupted_turn() {
    let (state, _dir, _hub, live, _events) = reporting_child("program-status-idle-after-wait");
    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x07");
    until_status(&state, SessionStatus::WaitingUser).await;
    live.on_output(b"\x1b]7501;state=idle\x07");
    until_status(&state, SessionStatus::Idle).await;
    live.settle_program_status().await;
    assert!(!state.last_turn_abnormal("s"));
    assert_eq!(state.turn_ends("s"), 0);
}

/// An interpreter that declares it supplies its turns' messages and gives none by an event: the
/// tests hand the session the message themselves.
struct SupplyingHooks;

impl crate::hooks::HookInterpreter for SupplyingHooks {
    fn past_startup(&self, _event: &str, _payload: &serde_json::Value) -> bool {
        true
    }

    fn supplies_turn_message(&self) -> bool {
        true
    }
}

static SUPPLYING_HOOKS: SupplyingHooks = SupplyingHooks;

/// [`reporting_child`] of a provider that declares it supplies its turns' messages.
fn supplied_child(
    name: &str,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    crate::test_support::StandIn,
) {
    let _declared = crate::test_support::test_provider::declaring_hooks(&SUPPLYING_HOOKS);
    let (state, dir, hub, live, _events) = reporting_child(name);
    (state, dir, hub, live)
}

fn said(message: &str) -> crate::hooks::TurnMessage {
    crate::hooks::TurnMessage::Said(Some(message.to_string()))
}

/// `done` with `msg` as its message.
fn done_saying(msg: &str) -> Vec<u8> {
    use base64::Engine;
    let msg = base64::engine::general_purpose::STANDARD.encode(msg);
    format!("\x1b]7501;state=done:msg={msg}\x07").into_bytes()
}

/// A message the provider read for the turn before the report that ends it replaces the report's
/// `msg`, and the turn is closed once.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_supplied_before_the_report_replaces_its_msg() {
    let (state, _dir, hub, live) = supplied_child("program-status-message-before");
    live.on_output(b"\x1b]7501;state=working\x07");
    live.settle_program_status().await;
    let ends = state.turn_ends("s");
    live.supply_turn_message(said("the whole reply, read elsewhere"));
    live.on_output(&done_saying("the reply, cut"));

    let heard =
        crate::test_support::owner_team::hub_heard_of(&hub, "the whole reply, read elsewhere")
            .await;
    assert!(!heard.contains("the reply, cut"), "{heard}");
    live.settle_program_status().await;
    assert_eq!(state.turn_ends("s"), ends + 1, "closed once");
}

/// A message that comes after the report, within the bound, is still the turn's: the turn end
/// waits for it, the session reading as working meanwhile.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_supplied_after_the_report_within_the_bound_replaces_its_msg() {
    let (state, _dir, hub, live) = supplied_child("program-status-message-after");
    live.on_output(b"\x1b]7501;state=working\x07");
    live.settle_program_status().await;
    live.on_output(&done_saying("the reply, cut"));
    until("the turn end waits for the message", || {
        live.turn_messages.waiters() == 1
    })
    .await;
    assert!(
        state.turn_is_open("s"),
        "the turn end waits for the message"
    );
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );

    live.supply_turn_message(said("the whole reply, read elsewhere"));
    let heard =
        crate::test_support::owner_team::hub_heard_of(&hub, "the whole reply, read elsewhere")
            .await;
    assert!(!heard.contains("the reply, cut"), "{heard}");
    until_status(&state, SessionStatus::Idle).await;
}

/// With no message by the bound the report's `msg` is what the owner gets, and one that comes
/// later is dropped: it is not given to the next turn.
#[tokio::test(flavor = "multi_thread")]
async fn without_a_message_by_the_bound_the_reports_msg_stands() {
    let (_state, _dir, hub, live) = supplied_child("program-status-message-never");
    let _bound = crate::session::shorten_turn_message_bound(Duration::from_millis(50));
    live.on_output(b"\x1b]7501;state=working\x07");
    live.on_output(&done_saying("the first reply, cut"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the first reply, cut").await;
    assert!(heard.contains("the first reply, cut"), "{heard}");
    live.settle_program_status().await;

    live.supply_turn_message(said("the first reply, whole and late"));
    live.on_output(b"\x1b]7501;state=working\x07");
    live.on_output(&done_saying("the second reply, cut"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the second reply, cut").await;
    assert!(!heard.contains("whole and late"), "{heard}");
}

/// A provider that says its turn had no message ends the wait at once, and a blank one is none:
/// the report's `msg` stands.
#[tokio::test(flavor = "multi_thread")]
async fn a_supplied_message_that_is_none_or_blank_leaves_the_reports_msg() {
    for (name, supplied) in [
        ("program-status-message-none", None),
        ("program-status-message-blank", Some("  ".to_string())),
    ] {
        let (state, _dir, hub, live) = supplied_child(name);
        live.on_output(b"\x1b]7501;state=working\x07");
        live.settle_program_status().await;
        live.supply_turn_message(crate::hooks::TurnMessage::Said(supplied));
        let started = Instant::now();
        live.on_output(&done_saying("what the report said"));
        until_status(&state, SessionStatus::Idle).await;
        assert!(started.elapsed() < PATIENCE / 2, "{name}: it did not wait");
        let heard =
            crate::test_support::owner_team::hub_heard_of(&hub, "what the report said").await;
        assert!(heard.contains("what the report said"), "{name}: {heard}");
    }
}

/// A turn end of a provider that declares no message of its own waits for none, and a message
/// handed to a session whose provider's hooks give its status is not kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_provider_that_supplies_no_message_has_its_turn_ends_wait_for_none() {
    let (state, _dir, hub, live, _events) = reporting_child("program-status-message-undeclared");
    live.on_output(b"\x1b]7501;state=working\x07");
    let started = Instant::now();
    live.on_output(&done_saying("what the report said"));
    live.settle_program_status().await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );
    assert!(started.elapsed() < PATIENCE / 2, "it did not wait");
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "what the report said").await;
    assert!(heard.contains("what the report said"), "{heard}");
}

/// An interrupted turn takes no message, and one supplied for it is not given to the next turn.
#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_turn_drops_the_message_supplied_for_it() {
    let (state, _dir, hub, live) = supplied_child("program-status-message-interrupted");
    let _bound = crate::session::shorten_turn_message_bound(Duration::from_millis(50));
    live.on_output(b"\x1b]7501;state=working\x07");
    live.settle_program_status().await;
    live.supply_turn_message(said("said before the cancel"));
    live.on_output(b"\x1b]7501;state=idle\x07");
    live.settle_program_status().await;
    assert!(state.last_turn_abnormal("s"));

    live.on_output(b"\x1b]7501;state=working\x07");
    live.on_output(&done_saying("the next reply"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the next reply").await;
    assert!(!heard.contains("said before the cancel"), "{heard}");
}

// Qwen Code 0.28.3, one turn each with the OSC 7501 reports and the titles as it wrote them
// (`testdata/qwen-captures.md`), replayed into a session of the test-only provider declaring a
// status integration of its own, as Qwen Code has one.
/// working; idle (Esc).
const HOOKED_CANCEL: &[u8] = include_bytes!("../../../testdata/qwen_turn_cancel_esc.bin");
/// working; done; working; blocked on a permission; working; done.
const HOOKED_PERMISSION: &[u8] =
    include_bytes!("../../../testdata/qwen_turn_permission_approved.bin");

const WAIT_AND_CANCEL: ReportedSignals = ReportedSignals {
    wait_on_user: true,
    cancel: true,
};

/// [`reporting_child`] of a provider with a status integration of its own that declares its
/// reports say `signals` beside it, in the turn its hooks opened: working, by them.
fn hooked_child(
    name: &str,
    signals: ReportedSignals,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    crate::test_support::StandIn,
    tokio::sync::broadcast::Receiver<crate::protocol::Event>,
) {
    let _declared = crate::test_support::test_provider::declaring_reported_signals(signals);
    let (state, dir, hub, live, events) = reporting_child(name);
    state.turn_started("s", None);
    state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    (state, dir, hub, live, events)
}

/// The status the session reads as its agent's.
fn agent_status(state: &Arc<AppState>) -> SessionStatus {
    state.agent_status(&state.session_record("s").unwrap())
}

/// A declared `blocked` raises the hand at once and tells the owner, a hook that reports the
/// turn working meanwhile does not lower it, and the report that ends the wait does; the record's
/// own status stays the hooks'.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_wait_raises_the_hand_over_what_the_hooks_say_and_its_end_lowers_it() {
    let (state, _dir, hub, live, mut events) =
        hooked_child("program-status-declared-wait", WAIT_AND_CANCEL);
    live.on_output(b"\x1b]7501;state=working\x07");
    assert_eq!(agent_status(&state), SessionStatus::Working);

    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x07");
    assert_eq!(agent_status(&state), SessionStatus::WaitingUser);
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "only they can clear it").await;
    assert!(heard.contains("is waiting for the user"), "{heard}");
    assert!(published_waiting(&mut events));

    // The hooks disagree: another call of the turn ended, and they say working.
    state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    assert_eq!(agent_status(&state), SessionStatus::WaitingUser);
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working,
        "the record's status is the hooks'"
    );

    live.on_output(b"\x1b]7501;state=working\x07");
    assert_eq!(agent_status(&state), SessionStatus::Working);
    assert!(state.turn_is_open("s"), "the wait was within the turn");
}

/// A whole turn with a permission prompt as the program reported it: the hand is up exactly while
/// the root record reads `blocked`, and the `done` between two model calls closes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_replayed_turn_with_a_permission_prompt_waits_only_while_blocked() {
    let (state, _dir, _hub, live, _events) =
        hooked_child("program-status-declared-replay", WAIT_AND_CANCEL);
    let blocked = HOOKED_PERMISSION
        .windows(13)
        .position(|window| window == b"state=blocked")
        .expect("the capture holds the blocked report");
    // To the end of the blocked report.
    let answered = blocked
        + HOOKED_PERMISSION[blocked..]
            .iter()
            .position(|byte| *byte == 0x07)
            .expect("the report ends")
        + 1;
    live.on_output(&HOOKED_PERMISSION[..answered]);
    assert!(live.needs_user_action());
    assert_eq!(agent_status(&state), SessionStatus::WaitingUser);

    live.on_output(&HOOKED_PERMISSION[answered..]);
    assert!(!live.needs_user_action());
    // Decided as the reports were taken in: a `done` sets nothing going where the hooks give
    // the status.
    assert_eq!(live.signals.cancels_started(), 0);
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working,
        "a `done` is the hooks' to act on"
    );
    assert!(state.turn_is_open("s"));
}

/// The report that ends the wait arrives while the hooks still hold their own hand up: the
/// report's hand comes down, and the hooks' stays until they lower it.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_waits_end_leaves_a_hand_the_hooks_raised() {
    let (state, _dir, _hub, live, _events) =
        hooked_child("program-status-declared-late-hook", WAIT_AND_CANCEL);
    live.on_output(b"\x1b]7501;state=blocked:kind=question\x07");
    state
        .apply_hook_status("s", SessionStatus::WaitingUser)
        .unwrap();
    live.on_output(b"\x1b]7501;state=working\x07");
    assert!(!live.needs_user_action());
    assert_eq!(agent_status(&state), SessionStatus::WaitingUser);
    state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    assert_eq!(agent_status(&state), SessionStatus::Working);
}

/// A declared `blocked` of kind `auth` counts only once it has stood for its settle, as where the
/// reports give the status: one the next report replaces is passed over, one that stays is shown.
/// The settle is a timer of the runtime's, so the test moves its clock.
#[tokio::test]
async fn a_declared_wait_on_a_sign_in_counts_once_it_has_stood() {
    let (_state, _dir, _hub, live, _events) =
        hooked_child("program-status-declared-auth-passing", WAIT_AND_CANCEL);
    AUTH_SETTLE_OVERRIDE.with(|settle| settle.set(Some(AUTH_SETTLE)));
    let past_the_settle = AUTH_SETTLE + Duration::from_millis(1);
    live.on_output(b"\x1b]7501;state=blocked:kind=auth\x07");
    assert!(!live.needs_user_action());
    live.on_output(b"\x1b]7501;state=idle\x07");
    crate::test_support::runtime_advanced(past_the_settle).await;
    assert!(!live.needs_user_action(), "replaced within the settle");

    live.on_output(b"\x1b]7501;state=blocked:kind=auth\x07");
    assert!(!live.needs_user_action(), "not before it has stood");
    crate::test_support::runtime_advanced(past_the_settle).await;
    assert!(live.needs_user_action(), "a sign-in that stays is a wait");
}

/// Before the program's first report its title says whether only the user can act, as today;
/// from the first report on the reports do, and the title says nothing of it.
#[tokio::test(flavor = "multi_thread")]
async fn the_title_says_the_wait_until_the_first_report_and_not_after() {
    let title = format!(
        "\x1b]0;{} now\x07",
        crate::adapter::test_provider::NEEDS_USER_TITLE
    );
    let (_state, _dir, _hub, live, _events) =
        hooked_child("program-status-declared-title", WAIT_AND_CANCEL);
    assert!(!live.needs_user_action());
    live.on_output(b"Ready");
    live.on_output(title.as_bytes());
    assert!(live.needs_user_action(), "no report yet: the title stands");

    live.on_output(b"\x1b]7501;state=working\x07");
    assert!(!live.needs_user_action(), "the reports took over");
    live.on_output(title.as_bytes());
    assert!(!live.needs_user_action(), "the title says nothing now");
    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x07");
    assert!(live.needs_user_action());

    // A provider that declares no wait keeps its title, whatever it reports.
    let (_state, _dir, _hub, live, _events) = hooked_child(
        "program-status-undeclared-title",
        ReportedSignals {
            wait_on_user: false,
            cancel: true,
        },
    );
    live.on_output(b"Ready\x1b]7501;state=working\x07");
    live.on_output(title.as_bytes());
    assert!(live.needs_user_action());
    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x07\x1b]0;plain\x07");
    assert!(!live.needs_user_action());
}

/// A declared cancel, `idle` straight after `working`, puts the session at its prompt, paused,
/// and closes its turn with no report, without any title watch; a turn end of the agent's own
/// that still comes is its real end.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_cancel_closes_the_turn_at_once() {
    let (state, _dir, hub, live, _events) =
        hooked_child("program-status-declared-cancel", WAIT_AND_CANCEL);
    live.on_output(HOOKED_CANCEL);
    until_status(&state, SessionStatus::Idle).await;
    assert!(state.last_turn_abnormal("s"), "paused");
    assert!(!state.turn_is_open("s"));
    let heard = crate::test_support::owner_team::hub_heard(&state, &hub).await;
    assert!(!heard.contains(NOT_REPORTED), "{heard}");

    // A `Stop` whose hook command started slowly still closes the turn as its own end.
    assert_eq!(
        state.close_turn("s", false),
        crate::state::TurnClose::OwesReport
    );
}

/// A hook event posted before the cancel is applied first, and what it says of the session wins:
/// one that moved the session on leaves the cancel with nothing to close. The wait for the queue
/// is the runtime's, so the test lets the runtime run idle to see the cancel held back.
#[tokio::test]
async fn a_declared_cancel_waits_for_the_hooks_and_leaves_a_session_they_moved_on() {
    let (state, _dir, _hub, live, _events) =
        hooked_child("program-status-declared-cancel-late-hook", WAIT_AND_CANCEL);
    state
        .hook_queues
        .push("s", bytes::Bytes::from_static(b"{}"));
    live.on_output(HOOKED_CANCEL);
    crate::test_support::runtime_settled().await;
    assert_eq!(
        live.signals.cancels_settled(),
        0,
        "the cancel waits while an event is queued"
    );
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );

    // What the queued event said: the agent waits on the user.
    state
        .apply_hook_status("s", SessionStatus::WaitingUser)
        .unwrap();
    state.hook_queues.complete("s");
    until("the cancel is acted on", || {
        live.signals.cancels_settled() == 1
    })
    .await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::WaitingUser
    );
    assert!(!state.last_turn_abnormal("s"));
    assert!(state.turn_is_open("s"));
}

/// The reports show a turn running again before the cancel is acted on (a queued message started
/// the next turn at once): the next turn is not closed.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_cancel_does_not_close_a_turn_the_reports_show_running_again() {
    let (state, _dir, _hub, live, _events) =
        hooked_child("program-status-declared-cancel-next-turn", WAIT_AND_CANCEL);
    state
        .hook_queues
        .push("s", bytes::Bytes::from_static(b"{}"));
    live.on_output(HOOKED_CANCEL);
    live.on_output(b"\x1b]7501;state=working\x07");
    state.hook_queues.complete("s");
    until("the cancel is acted on", || {
        live.signals.cancels_settled() == 1
    })
    .await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
    assert!(state.turn_is_open("s"));
}

/// What a provider with a status integration does not declare, its reports do not say: a
/// `blocked` raises no hand and `idle` after `working` closes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn undeclared_signals_leave_the_hooks_alone() {
    let (state, _dir, _hub, live, _events) =
        hooked_child("program-status-undeclared-signals", ReportedSignals::NONE);
    live.on_output(b"\x1b]7501;state=working\x07\x1b]7501;state=blocked:kind=permission\x07");
    assert!(!live.needs_user_action());
    live.on_output(b"\x1b]7501;state=working\x07\x1b]7501;state=idle\x07");
    // Decided as the reports were taken in: with nothing declared they set nothing going.
    assert_eq!(live.signals.cancels_started(), 0);
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
    assert!(state.turn_is_open("s"));
}

/// A wait the program reported before a daemon restart is one it is still in: the hand, which
/// the daemon before kept in memory, is raised again from the restored record.
#[tokio::test(flavor = "multi_thread")]
async fn a_restored_blocked_record_raises_a_declared_wait_again() {
    let (state, _dir, _hub, live, _events) =
        hooked_child("program-status-declared-restored", WAIT_AND_CANCEL);
    live.restore_screen(&SavedScreen::of_bytes(
        24,
        80,
        b"\x1b]7501;state=blocked:kind=question\x07",
    ));
    assert!(!live.needs_user_action());
    live.note_restored_program_status(&state);
    assert!(live.needs_user_action());
    assert_eq!(agent_status(&state), SessionStatus::WaitingUser);
}

/// The pause an interrupted turn left stands through a move to the prompt that ends no turn (a
/// `done` with none open, the record going away), and is cleared by the next turn's ordinary end.
#[tokio::test(flavor = "multi_thread")]
async fn a_move_to_the_prompt_that_ends_no_turn_leaves_the_pause() {
    let (state, _dir, _hub, live, _events) = reporting_child("program-status-pause-kept");
    live.on_output(CANCELLED_TURN);
    live.settle_program_status().await;
    assert!(state.last_turn_abnormal("s"));

    live.on_output(b"\x1b]7501;state=done\x07");
    live.settle_program_status().await;
    assert!(state.last_turn_abnormal("s"), "a `done` with no turn open");
    live.on_output(b"\x1b]7501;state=clear\x07");
    live.settle_program_status().await;
    assert!(
        state.last_turn_abnormal("s"),
        "the record gone, no turn open"
    );
    live.on_output(b"\x1b]7501;state=idle\x07");
    live.settle_program_status().await;
    assert!(
        state.last_turn_abnormal("s"),
        "the `idle` of a program starting over"
    );

    live.on_output(ANSWERED_TURN);
    live.settle_program_status().await;
    assert!(!state.last_turn_abnormal("s"), "a turn that ended as usual");
}

/// A read of the provider's source that never returns is given up at the bound, where the turn
/// ends with the report's `msg`, and it is the only reader the session ever leaves behind: while
/// it runs, a later read is dropped unrun.
#[tokio::test(flavor = "multi_thread")]
async fn a_read_that_never_returns_ends_at_the_bound_and_is_the_only_one_left() {
    let (_state, _dir, hub, live) = supplied_child("program-status-message-stalled-read");
    let _bound = crate::session::shorten_turn_message_bound(Duration::from_millis(50));
    // Held open until the test is over, so the read stays stuck throughout.
    let (release, held) = std::sync::mpsc::channel::<()>();
    live.on_output(b"\x1b]7501;state=working\x07");
    live.settle_program_status().await;
    live.supply_turn_message(crate::hooks::TurnMessage::Read(Box::new(move || {
        let _ = held.recv();
        None
    })));
    live.on_output(&done_saying("the first reply, cut"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the first reply, cut").await;
    assert!(heard.contains("the first reply, cut"), "{heard}");
    assert!(
        live.turn_messages.reading(),
        "the first read is still stuck"
    );

    live.on_output(b"\x1b]7501;state=working\x07");
    live.settle_program_status().await;
    let second = Arc::new(());
    let held_by_the_read = second.clone();
    live.supply_turn_message(crate::hooks::TurnMessage::Read(Box::new(move || {
        let _held = held_by_the_read;
        Some("read by a second reader".to_string())
    })));
    assert_eq!(Arc::strong_count(&second), 1, "dropped unrun");
    live.on_output(&done_saying("the second reply, cut"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the second reply, cut").await;
    assert!(!heard.contains("read by a second reader"), "{heard}");

    // Once the stuck read returns, the session reads again.
    drop(release);
    until("the stuck read ended", || !live.turn_messages.reading()).await;
}

/// [`reporting_child`] as a launch that gave the agent a prompt on its command line leaves it:
/// working from its launch, the turn of that prompt still to start.
fn launched_with_a_prompt(
    name: &str,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    crate::test_support::StandIn,
    tokio::sync::broadcast::Receiver<crate::protocol::Event>,
) {
    let (state, dir, hub, live, _events) = reporting_child(name);
    let mut record = state.session_record("s").unwrap();
    record.status = SessionStatus::Working;
    state.store.update_session(&record).unwrap();
    live.await_launch_turn();
    let events = state.subscribe();
    (state, dir, hub, live, events)
}

/// Whether `events` hold a publication of the session at its prompt.
fn published_idle(events: &mut tokio::sync::broadcast::Receiver<crate::protocol::Event>) -> bool {
    let mut idle = false;
    while let Ok(event) = events.try_recv() {
        if let crate::protocol::Event::SessionUpserted { session } = event {
            idle |= session.id == "s" && session.status == SessionStatus::Idle;
        }
    }
    idle
}

/// The sequence Pi 1.5.2 reports for a launch with a task (`docs/agent-cli-reference.md`, "Pi":
/// `idle` once started, `working` about 0.3 s later). The session read working from its launch,
/// and the `idle` the `working` replaces within the settle is passed over whole: the session is
/// never published at its prompt. The `idle` after `working` is then the interrupted turn, applied
/// at once.
#[tokio::test(flavor = "multi_thread")]
async fn the_idle_before_the_turn_of_a_launchs_prompt_is_passed_over() {
    let (state, _dir, _hub, live, mut events) =
        launched_with_a_prompt("program-status-launch-idle-passing");
    live.on_output(b"\x1b]7501;state=idle:app=pi\x1b\\");
    live.on_output(b"\x1b]7501;state=working:app=pi\x1b\\");
    live.settle_program_status().await;

    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
    assert!(state.turn_is_open("s"));
    assert!(!published_idle(&mut events), "never shown at its prompt");
    assert_eq!(state.turn_ends("s"), 0);

    // Not held: the settle is at its longest, and the move is applied by the time they settle.
    live.on_output(b"\x1b]7501;state=idle:app=pi\x1b\\");
    live.settle_program_status().await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle
    );
    assert!(state.last_turn_abnormal("s"), "the interrupted turn");
}

/// An `idle` that stands for the settle is an agent that did not take the launch's prompt: the
/// session is then shown at its prompt, with no turn opened or closed, and an `idle` after that is
/// applied at once.
#[tokio::test(flavor = "multi_thread")]
async fn the_idle_of_a_launch_whose_prompt_was_not_taken_is_applied_once_it_has_stood() {
    /// Short, as a test proving a production window runs out keeps it.
    const SETTLE: Duration = Duration::from_millis(50);
    let (state, _dir, _hub, live, _events) =
        launched_with_a_prompt("program-status-launch-idle-stays");

    LAUNCH_IDLE_SETTLE_OVERRIDE.with(|settle| settle.set(Some(SETTLE)));
    live.on_output(b"\x1b]7501;state=idle\x07");
    LAUNCH_IDLE_SETTLE_OVERRIDE.with(|settle| settle.set(None));
    until_status(&state, SessionStatus::Idle).await;
    assert_eq!(state.turn_opened("s"), 0);
    assert_eq!(state.turn_ends("s"), 0);
    assert!(!state.last_turn_abnormal("s"));

    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x07");
    until_status(&state, SessionStatus::WaitingUser).await;
    live.on_output(b"\x1b]7501;state=idle\x07");
    live.settle_program_status().await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle,
        "applied at once, the settle at its longest"
    );
}

/// A screen the program shows before it takes the launch's prompt does not end the wait for that
/// prompt's turn: the `idle` after the screen is still passed over when `working` follows.
#[tokio::test(flavor = "multi_thread")]
async fn a_screen_before_a_launchs_turn_leaves_the_idle_after_it_passed_over() {
    let (state, _dir, _hub, live, mut events) =
        launched_with_a_prompt("program-status-launch-idle-screen");
    live.on_output(b"\x1b]7501;state=blocked:kind=permission\x07");
    until_status(&state, SessionStatus::WaitingUser).await;
    live.on_output(b"\x1b]7501;state=idle\x07");
    live.on_output(b"\x1b]7501;state=working\x07");
    until_status(&state, SessionStatus::Working).await;
    live.settle_program_status().await;
    assert!(!published_idle(&mut events));
    assert!(state.turn_is_open("s"));
}

/// An interpreter of a provider that supplies its turns' messages and whose program reports
/// something else as its `msg`, the session's name say.
struct NamingHooks;

impl crate::hooks::HookInterpreter for NamingHooks {
    fn past_startup(&self, _event: &str, _payload: &serde_json::Value) -> bool {
        true
    }

    fn supplies_turn_message(&self) -> bool {
        true
    }

    fn report_msg_is_turn_message(&self) -> bool {
        false
    }
}

static NAMING_HOOKS: NamingHooks = NamingHooks;

/// [`reporting_child`] of a provider whose report's `msg` is not its turn's message.
fn named_child(
    name: &str,
) -> (
    Arc<AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
    crate::test_support::StandIn,
) {
    let _declared = crate::test_support::test_provider::declaring_hooks(&NAMING_HOOKS);
    let (state, dir, hub, live, _events) = reporting_child(name);
    (state, dir, hub, live)
}

/// Where the provider says its report's `msg` is not the turn's message, a turn it gave no
/// message for ends with none, whichever way it gave none: it said the turn had no message, or it
/// said nothing by the bound (a hook that failed, an end no hook reports). The `msg`, the
/// session's name here, is never what the owner is told the turn said.
#[tokio::test(flavor = "multi_thread")]
async fn a_msg_that_is_not_the_turns_message_never_stands_in_for_one() {
    for (name, supplied) in [
        ("program-status-named-said-nothing", true),
        ("program-status-named-gave-nothing", false),
    ] {
        let (state, _dir, hub, live) = named_child(name);
        let bound = (!supplied)
            .then(|| crate::session::shorten_turn_message_bound(Duration::from_millis(50)));
        live.on_output(b"\x1b]7501;state=working\x07");
        live.settle_program_status().await;
        if supplied {
            live.supply_turn_message(crate::hooks::TurnMessage::Said(None));
        }
        live.on_output(&done_saying("my session's name"));
        drop(bound);
        let heard = crate::test_support::owner_team::hub_heard_of(&hub, "it said nothing").await;
        assert!(!heard.contains("my session's name"), "{name}: {heard}");
        until_status(&state, SessionStatus::Idle).await;
    }
}

/// The message such a provider does supply is the turn's, as for any provider that supplies one.
#[tokio::test(flavor = "multi_thread")]
async fn a_supplied_message_stands_where_the_msg_is_not_the_turns_message() {
    let (_state, _dir, hub, live) = named_child("program-status-named-supplied");
    live.on_output(b"\x1b]7501;state=working\x07");
    live.settle_program_status().await;
    live.supply_turn_message(said("what the turn said"));
    live.on_output(&done_saying("my session's name"));
    let heard = crate::test_support::owner_team::hub_heard_of(&hub, "what the turn said").await;
    assert!(!heard.contains("my session's name"), "{heard}");
}

/// A launch that goes without its provider's status integration takes its whole status from its
/// reports, turns opened and closed included, though the provider declares an integration and
/// what its reports say beside it; a launch that has the integration leaves the status to it.
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_without_its_providers_status_integration_takes_its_status_from_its_reports() {
    let _declared = crate::test_support::test_provider::declaring_reported_signals(WAIT_AND_CANCEL);
    {
        let _without =
            crate::test_support::test_provider::declaring_launch_without_status_integration();
        let (state, _dir, hub, live, _events) = reporting_child("program-status-launch-reports");
        live.on_output(b"\x1b]7501;state=working\x07");
        until_status(&state, SessionStatus::Working).await;
        assert!(state.turn_is_open("s"), "the report opened the turn");
        live.on_output(&done_saying("the reply"));
        let heard = crate::test_support::owner_team::hub_heard_of(&hub, "the reply").await;
        assert!(heard.contains("the reply"), "{heard}");
        until_status(&state, SessionStatus::Idle).await;
        assert!(!state.turn_is_open("s"));
        assert_eq!(
            live.reported_cancels_started(),
            0,
            "no declared signal is read"
        );
    }
    let (state, _dir, _hub, live, _events) = reporting_child("program-status-launch-integration");
    live.on_output(b"\x1b]7501;state=working\x07");
    live.settle_program_status().await;
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Idle,
        "the integration gives the status"
    );
    assert!(!state.turn_is_open("s"));
}

/// The count of cancels taken up is what says a report started none: a `done` after `working`
/// starts none, an `idle` after `working` starts one, counted as the report is taken in.
#[tokio::test(flavor = "multi_thread")]
async fn the_cancels_a_sessions_reports_started_are_counted_as_they_are_taken_in() {
    let (_state, _dir, _hub, live, _events) =
        hooked_child("program-status-cancels-counted", WAIT_AND_CANCEL);
    live.on_output(b"\x1b]7501;state=working\x07\x1b]7501;state=done\x07");
    assert_eq!(live.reported_cancels_started(), 0);
    live.on_output(b"\x1b]7501;state=working\x07\x1b]7501;state=idle\x07");
    assert_eq!(live.reported_cancels_started(), 1);
}

/// The bound a turn end waits for its message is shortened by the crate-wide seam a provider's
/// test reaches too, only while its guard lives: dropped, it is the test value again.
#[test]
fn a_shortened_turn_message_bound_lasts_as_long_as_its_guard() {
    assert_eq!(turn_message::bound(), PATIENCE);
    let shortened = crate::session::shorten_turn_message_bound(Duration::from_millis(50));
    assert_eq!(turn_message::bound(), Duration::from_millis(50));
    drop(shortened);
    assert_eq!(turn_message::bound(), PATIENCE);
}
