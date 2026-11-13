//! The reading of a session's turn: what each consumer takes from the title, the program's
//! reports, the dialog sighting and the hooks' status, and where they differ on purpose.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::{AgentId, SessionStatus};
use crate::session::title_activity::Shown;
use crate::session::LiveSession;
use crate::state::test_support::insert_record;
use crate::state::AppState;
use crate::terminal::program_status::ReportedState;
use crate::test_support::{app_state, fake_live_session, ScratchDir, StandIn};

/// How long ago a running title was sent, for one the agent stopped sending: far past any
/// freshness window.
const STALE: Duration = Duration::from_secs(3600);

/// A title Claude Code sends while a turn runs and the one it sends back at its prompt.
const RUNNING_TITLE: &[u8] = "\x1b]0;\u{25D0} Claude Code\x07".as_bytes();
const PROMPT_TITLE: &[u8] = "\x1b]0;\u{2733} Claude Code\x07".as_bytes();

/// What a program reports of its turn through OSC 7501.
const REPORTS_WORKING: &[u8] = b"\x1b]7501;state=working:app=claude-code\x1b\\";
const REPORTS_IDLE: &[u8] = b"\x1b]7501;state=idle:app=claude-code\x1b\\";

/// A dialog of Claude Code's that only a person can answer.
const DIALOG: &[u8] =
    "We're changing auto mode.\r\nEnter to continue \u{B7} Esc to cancel\r\n".as_bytes();

/// The title Codex sends while it waits on the user.
const CODEX_ACTION_TITLE: &[u8] = b"\x1b]0;[ ! ] Action Required | octoboard\x07";

/// A session `s` registered with a state that holds its record at some status. Declared in the
/// order the pieces are dropped in: the process before the directory its record lives in.
struct Fixture {
    live: StandIn,
    state: Arc<AppState>,
    _dir: ScratchDir,
}

fn bound(name: &str, agent: AgentId, status: SessionStatus) -> Fixture {
    let (state, dir) = app_state(&format!("turn-reading-{name}"));
    insert_record(&state, &dir, "s", status);
    let live = fake_live_session("s", agent, 140, 45, "sleep 30");
    state.register_live(live.clone());
    Fixture {
        live,
        state,
        _dir: dir,
    }
}

/// The instant `ago` before now.
fn before_now(ago: Duration) -> Instant {
    Instant::now()
        .checked_sub(ago)
        .expect("a clock that reaches back that far")
}

/// Has the title show a turn running, sent `ago` before now.
fn running_title_sent(live: &LiveSession, ago: Duration) {
    live.title
        .lock()
        .unwrap()
        .activity
        .note(Some(Shown::Running), before_now(ago));
}

/// Has the program's report of a turn running reach the session `ago` before now.
fn working_reported(live: &LiveSession, ago: Duration) {
    live.title
        .lock()
        .unwrap()
        .activity
        .note_report(Some(ReportedState::Working), before_now(ago));
}

/// The interrupt key goes in only on a turn the agent is still sending, with the record and the
/// agent's own status both at work and nothing up that the key would answer.
#[test]
fn an_interrupt_may_press_only_on_a_title_still_being_sent_and_both_statuses_working() {
    use SessionStatus::*;
    for (label, status, title_sent, expected) in [
        ("fresh title, working", Working, Some(Duration::ZERO), true),
        ("fresh title, idle", Idle, Some(Duration::ZERO), false),
        (
            "fresh title, waiting for the user",
            WaitingUser,
            Some(Duration::ZERO),
            false,
        ),
        ("stale title, working", Working, Some(STALE), false),
        ("no title, working", Working, None, false),
    ] {
        let fixture = bound("press", AgentId::CLAUDE, status);
        if let Some(ago) = title_sent {
            running_title_sent(&fixture.live, ago);
        }
        assert_eq!(
            fixture.live.turn_reading().interrupt_may_press(),
            expected,
            "{label}"
        );
    }
}

/// A hand raised for a request leaves the record waiting while the agent's own status stays at
/// work: the key waits for both to read working, and the hooks' status is the agent's.
#[test]
fn an_interrupt_waits_for_the_record_as_well_as_the_agent_s_status() {
    let fixture = bound("hand", AgentId::CLAUDE, SessionStatus::Working);
    running_title_sent(&fixture.live, Duration::ZERO);
    assert!(fixture.live.turn_reading().interrupt_may_press());

    fixture.state.raise_request_hand("s").unwrap();
    assert_eq!(
        fixture.state.session_record("s").unwrap().status,
        SessionStatus::WaitingUser
    );
    let reading = fixture.live.turn_reading();
    assert_eq!(reading.hooks(), Some(SessionStatus::Working));
    assert!(reading.hooks_working());
    assert!(!reading.interrupt_may_press());
}

/// What the hooks' status reads as is the status of the session's record as the agent has it, and
/// nothing of a session bound to no state.
#[test]
fn the_hooks_status_is_the_agent_s_status_of_the_record() {
    for status in [
        SessionStatus::Working,
        SessionStatus::Idle,
        SessionStatus::WaitingUser,
    ] {
        let fixture = bound("hooks", AgentId::CLAUDE, status);
        let reading = fixture.live.turn_reading();
        assert_eq!(reading.hooks(), Some(status));
        assert_eq!(reading.hooks_working(), status == SessionStatus::Working);
    }
}

/// A session bound to no state has no hooks' status, and the terminal alone says whether an Esc
/// is typed mid-turn; no interrupt goes into it, whatever its title shows.
#[test]
fn a_session_bound_to_no_state_has_no_hooks_status() {
    let live = fake_live_session("s", AgentId::CLAUDE, 140, 45, "sleep 30");
    let reading = live.turn_reading();
    assert_eq!(reading.hooks(), None);
    assert!(!reading.hooks_working());
    assert!(!reading.escape_mid_turn());
    assert!(!reading.interrupt_may_press());

    running_title_sent(&live, Duration::ZERO);
    let reading = live.turn_reading();
    assert_eq!(reading.hooks(), None);
    assert!(!reading.hooks_working());
    assert!(reading.escape_mid_turn(), "the title shows a turn running");
    assert!(!reading.interrupt_may_press(), "no record says it works");
}

/// An Esc is typed mid-turn where the last title or report that said anything showed a turn
/// running, however long ago, or where the hooks say one runs.
#[test]
fn an_escape_is_mid_turn_on_a_running_title_of_any_age_or_on_the_hooks_reporting_one() {
    use SessionStatus::*;
    let cases: [(&str, SessionStatus, fn(&LiveSession), bool); 7] = [
        ("nothing said, hooks idle", Idle, |_| {}, false),
        (
            "fresh running title, hooks idle",
            Idle,
            |live| running_title_sent(live, Duration::ZERO),
            true,
        ),
        (
            "stale running title, hooks idle",
            Idle,
            |live| running_title_sent(live, STALE),
            true,
        ),
        ("nothing said, hooks working", Working, |_| {}, true),
        (
            "title back at the prompt, hooks idle",
            Idle,
            |live| {
                running_title_sent(live, STALE);
                live.on_output(PROMPT_TITLE);
            },
            false,
        ),
        (
            "title back at the prompt, hooks working",
            Working,
            |live| {
                running_title_sent(live, STALE);
                live.on_output(PROMPT_TITLE);
            },
            true,
        ),
        (
            "title back at the prompt, hooks waiting for the user",
            WaitingUser,
            |live| {
                running_title_sent(live, STALE);
                live.on_output(PROMPT_TITLE);
            },
            false,
        ),
    ];
    for (label, status, said, expected) in cases {
        let fixture = bound("escape", AgentId::CLAUDE, status);
        said(&fixture.live);
        assert_eq!(
            fixture.live.turn_reading().escape_mid_turn(),
            expected,
            "{label}"
        );
    }
}

/// The deliberate difference: with an agent whose title alone shows its turn, a running title sent
/// longer ago than the window it counts as fresh in is a turn an Esc is typed in the middle of, and
/// no turn an interrupt key goes into; once the program reports `working`, which it does not
/// repeat, both read it running whatever the age.
#[test]
fn a_stale_running_title_is_mid_turn_for_an_escape_and_no_turn_to_interrupt() {
    for (agent, glyph_title) in [
        (AgentId::CLAUDE, RUNNING_TITLE),
        (
            AgentId::GROK,
            "\x1b]0;\u{280B} - Waiting for response\u{2026} - grok\x07".as_bytes(),
        ),
    ] {
        // Hooks idle for the Esc, working for the interrupt: the title alone is what differs.
        let idle = bound("stale-escape", agent, SessionStatus::Idle);
        let working = bound("stale-press", agent, SessionStatus::Working);

        for fixture in [&idle, &working] {
            fixture.live.on_output(glyph_title);
        }
        assert!(
            idle.live.turn_reading().escape_mid_turn(),
            "{agent:?} fresh"
        );
        assert!(
            working.live.turn_reading().interrupt_may_press(),
            "{agent:?} fresh"
        );

        for fixture in [&idle, &working] {
            running_title_sent(&fixture.live, STALE);
        }
        assert!(
            idle.live.turn_reading().escape_mid_turn(),
            "{agent:?} stale"
        );
        assert!(
            !working.live.turn_reading().interrupt_may_press(),
            "{agent:?} stale"
        );
        let reading = working.live.turn_reading();
        assert!(!reading.terminal_running_fresh(), "{agent:?} stale");
        assert!(reading.terminal_last_said_running(), "{agent:?} stale");
        assert!(!reading.reported(), "{agent:?} stale");
        drop(reading);

        for fixture in [&idle, &working] {
            working_reported(&fixture.live, STALE);
        }
        assert!(
            idle.live.turn_reading().escape_mid_turn(),
            "{agent:?} reported"
        );
        let reading = working.live.turn_reading();
        assert!(reading.reported(), "{agent:?} reported");
        assert!(reading.terminal_running_fresh(), "{agent:?} reported");
        assert!(reading.terminal_last_said_running(), "{agent:?} reported");
        assert!(reading.interrupt_may_press(), "{agent:?} reported");
    }
}

/// A program's own reports, fed through its output, move the reading the way its title would: a
/// `working` report is a turn running and any other state is back at the prompt.
#[test]
fn a_programs_reports_say_whether_the_turn_runs() {
    let working = bound("report-press", AgentId::CLAUDE, SessionStatus::Working);
    assert!(!working.live.turn_reading().interrupt_may_press());
    working.live.on_output(REPORTS_WORKING);
    let reading = working.live.turn_reading();
    assert!(reading.reported());
    assert!(reading.interrupt_may_press());
    drop(reading);
    working.live.on_output(REPORTS_IDLE);
    assert!(!working.live.turn_reading().interrupt_may_press());

    // The hooks idle, so that only the reports can say an Esc is typed mid-turn.
    let idle = bound("report-escape", AgentId::CLAUDE, SessionStatus::Idle);
    assert!(!idle.live.turn_reading().escape_mid_turn());
    idle.live.on_output(REPORTS_WORKING);
    assert!(idle.live.turn_reading().escape_mid_turn());
    idle.live.on_output(REPORTS_IDLE);
    assert!(!idle.live.turn_reading().escape_mid_turn());
}

/// A dialog only a person can answer is up: no interrupt key goes in, though the turn shows
/// running, and the title is not read as settled at the prompt.
#[test]
fn a_dialog_stops_an_interrupt_and_a_settled_prompt() {
    let fixture = bound("dialog", AgentId::CLAUDE, SessionStatus::Working);
    running_title_sent(&fixture.live, Duration::ZERO);
    let reading = fixture.live.turn_reading();
    assert!(!reading.dialog_up());
    assert!(reading.interrupt_may_press());
    drop(reading);

    fixture.live.on_output(DIALOG);
    fixture.live.look_for_dialog();
    assert!(
        fixture.live.agent_dialog().is_some(),
        "the fixture's dialog is recognised"
    );
    let reading = fixture.live.turn_reading();
    assert!(reading.dialog_up());
    assert!(reading.terminal_running_fresh());
    assert!(!reading.interrupt_may_press());

    // The title went back to the prompt, as it does when a turn ends at a dialog.
    fixture.live.on_output(PROMPT_TITLE);
    assert!(!fixture
        .live
        .turn_reading()
        .settled_at_prompt(0, Duration::ZERO));
}

/// Codex's title waiting on the user keeps the interrupt key out, though a report says its turn
/// runs: the agent's status reads as waiting for the user.
#[test]
fn codex_s_action_required_title_stops_an_interrupt() {
    let fixture = bound("action", AgentId::CODEX, SessionStatus::Working);
    fixture.live.on_output(REPORTS_WORKING);
    assert!(fixture.live.turn_reading().interrupt_may_press());

    fixture.live.on_output(CODEX_ACTION_TITLE);
    assert!(fixture.live.needs_user_action(), "the title is recognised");
    let reading = fixture.live.turn_reading();
    assert!(reading.terminal_running_fresh());
    assert_eq!(reading.hooks(), Some(SessionStatus::WaitingUser));
    assert!(!reading.interrupt_may_press());
    assert!(reading.escape_mid_turn(), "the reports say a turn runs");
}

/// The title has settled at the prompt once it went back there after the stops counted when the
/// watch was armed, and stayed for the settle, with no dialog up.
#[test]
fn a_title_back_at_the_prompt_is_settled_after_the_wait_with_no_dialog_up() {
    let fixture = bound("settled", AgentId::CLAUDE, SessionStatus::Working);
    {
        let mut observer = fixture.live.title.lock().unwrap();
        observer
            .activity
            .note(Some(Shown::Running), before_now(Duration::from_secs(20)));
        observer
            .activity
            .note(Some(Shown::AtPrompt), before_now(Duration::from_secs(10)));
    }
    let settled =
        |armed, settle: Duration| fixture.live.turn_reading().settled_at_prompt(armed, settle);
    assert!(settled(0, Duration::from_secs(5)));
    assert!(
        !settled(0, Duration::from_secs(30)),
        "not for that long yet"
    );
    assert!(!settled(1, Duration::ZERO), "no stop since the first");

    fixture.live.on_output(DIALOG);
    fixture.live.look_for_dialog();
    assert!(fixture.live.agent_dialog().is_some());
    assert!(!settled(0, Duration::from_secs(5)), "a dialog is up");
}

/// A turn running again after it settled is no longer settled, and the titles an agent sends are
/// what the watch reads.
#[test]
fn a_turn_running_again_is_not_settled_at_the_prompt() {
    let live = fake_live_session("s", AgentId::CLAUDE, 140, 45, "sleep 30");
    assert!(!live.turn_reading().settled_at_prompt(0, Duration::ZERO));
    live.on_output(RUNNING_TITLE);
    assert!(!live.turn_reading().settled_at_prompt(0, Duration::ZERO));
    live.on_output(PROMPT_TITLE);
    assert!(live.turn_reading().settled_at_prompt(0, Duration::ZERO));
    assert!(!live.turn_reading().settled_at_prompt(1, Duration::ZERO));
    live.on_output(RUNNING_TITLE);
    assert!(!live.turn_reading().settled_at_prompt(0, Duration::ZERO));
}
