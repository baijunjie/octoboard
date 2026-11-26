//! The status a launch without hooks takes from its title where its program reports nothing,
//! replayed from real Grok Build title captures (`testdata/grok-activity-titles.md`) through a bare
//! launch, whose hooks are absent from its start. The title's stamps are the wall clock's, so the
//! settle is zeroed or held open through the title cancel watch's hooks rather than waited out.

use std::sync::Arc;

use super::*;
use crate::protocol::{AgentId, Session};
use crate::session::screen::ScreenReader;
use crate::session::title_activity::{spinner_activity, Shown};
use crate::session::title_interrupt::{polls_settled, with_settle_held, without_settle};
use crate::state::TurnClose;
use crate::test_support::{
    app_state, bare_stand_in_of, console, console_session, within, ScratchDir, StandIn,
};

/// Captures from Grok Build 1.0.70; see `testdata/grok-activity-titles.md`.
const GROK_CANCEL_TITLES: &[u8] =
    include_bytes!("../../../testdata/grok_title_cancel_before_output.bin");
const GROK_TURN_END_TITLES: &[u8] = include_bytes!("../../../testdata/grok_title_turn_end.bin");
const GROK_TOOL_TITLES: &[u8] = include_bytes!("../../../testdata/grok_title_tool.bin");
/// A Grok Build 1.0.89 shell permission card, as a 120x40 terminal drew it
/// (`testdata/grok-interaction-captures.md`).
const GROK_SHELL_CARD: &[u8] = include_bytes!("../../../testdata/grok_interaction_shell_card.bin");
/// Grok Build 1.0.70's screen at its prompt, its composer empty, as a 120x40 terminal rendered it
/// (`testdata/grok-composer.md`).
const GROK_AT_PROMPT: &str = include_str!("../../../testdata/grok_composer_cleared.txt");

/// Redraws the screen as Grok Build draws it at its prompt, its composer drawn: its reader reads
/// the composer wherever it is at its prompt (`ScreenReader::reads_input_box`), so a turn the title
/// opened closes only with it on the screen.
fn draw_prompt(live: &LiveSession) {
    let screen = format!("\x1b[2J\x1b[H{}", GROK_AT_PROMPT.replace('\n', "\r\n"));
    live.on_output(screen.as_bytes());
    assert!(
        live.input_box_rows(&live.box_screen()).is_some(),
        "the composer is read off the redrawn screen"
    );
}

struct Fixture {
    live: StandIn,
    state: Arc<AppState>,
    _dir: ScratchDir,
}

/// A bare launch of `agent` for session `s`, at its prompt, registered, on a 120x40 terminal.
fn bare(name: &str, agent: AgentId) -> Fixture {
    let (state, dir) = app_state(&format!("title-status-{name}"));
    state.store.insert_console(&console(&dir)).expect("console");
    state
        .store
        .insert_session(&Session {
            agent,
            ..console_session("s", SessionStatus::Idle)
        })
        .expect("session");
    let live = bare_stand_in_of("s", agent, 120, 40);
    assert!(live.hooks_absent(), "a bare launch is without hooks");
    assert!(live.status_from_reports());
    state.register_live(live.clone());
    Fixture {
        live,
        state,
        _dir: dir,
    }
}

/// Feeds `titles` one byte at a time, so each title completes in its own chunk as it did on the
/// PTY.
fn replay(live: &LiveSession, titles: &[u8]) {
    for byte in titles {
        live.on_output(std::slice::from_ref(byte));
    }
}

fn status(state: &AppState) -> Option<SessionStatus> {
    state
        .store
        .get_session("s")
        .ok()
        .flatten()
        .map(|session| session.status)
}

async fn reaches(state: &Arc<AppState>, status: SessionStatus) -> bool {
    let state = state.clone();
    within(crate::test_support::PATIENCE, move || {
        self::status(&state) == Some(status)
    })
    .await
}

/// A turn whose title keeps its spinner through a tool call reads working throughout, as one turn
/// opened by the title with no prompt attributed to it.
#[tokio::test]
async fn a_turn_running_a_tool_reads_working() {
    let fixture = bare("tool", AgentId::GROK);
    let _settle = without_settle();
    replay(&fixture.live, GROK_TOOL_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    assert!(fixture.state.turn_is_open("s"), "the title opened the turn");
    polls_settled().await;
    assert_eq!(status(&fixture.state), Some(SessionStatus::Working));
    assert!(fixture.state.turn_is_open("s"));
    assert_eq!(fixture.state.turn_ends("s"), 0);
}

/// The title back at the prompt for a moment between two steps of one turn, the settle not run
/// out, leaves the turn open and the session working.
#[tokio::test]
async fn a_gap_between_two_steps_keeps_the_turn_open() {
    let fixture = bare("gap", AgentId::GROK);
    let _settle = with_settle_held();
    replay(&fixture.live, GROK_TURN_END_TITLES);
    replay(&fixture.live, GROK_TOOL_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    polls_settled().await;
    assert_eq!(status(&fixture.state), Some(SessionStatus::Working));
    assert!(fixture.state.turn_is_open("s"));
    assert_eq!(fixture.state.turn_ends("s"), 0, "one turn, never closed");
}

/// A turn that ended reads idle once its title has settled at the prompt: the turn is closed with
/// no report synthesised and the session not paused; an end of the agent's own that still came
/// would be its real end.
#[tokio::test]
async fn a_turn_end_reads_idle_with_no_report_and_no_pause() {
    let fixture = bare("turn-end", AgentId::GROK);
    let _settle = without_settle();
    draw_prompt(&fixture.live);
    replay(&fixture.live, GROK_TURN_END_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Idle).await);
    assert!(!fixture.state.turn_is_open("s"));
    assert_eq!(fixture.state.turn_ends("s"), 1);
    assert!(
        !fixture.state.last_turn_abnormal("s"),
        "a title never marks the session paused"
    );
    assert_eq!(
        fixture.state.close_turn("s", true),
        TurnClose::NoTurn,
        "nothing is owed to a backstop"
    );
    assert_eq!(
        fixture.state.close_turn("s", false),
        TurnClose::OwesReport,
        "no report was made for the turn: an end of the agent's own still owes it"
    );
}

/// Grok's composer not on the screen, as when another view is drawn over it, keeps a turn the title
/// opened open after its title has settled at the prompt; drawn, it lets the turn close.
#[tokio::test]
async fn a_grok_title_at_the_prompt_closes_the_turn_only_with_its_composer_drawn() {
    let fixture = bare("no-composer", AgentId::GROK);
    let _settle = without_settle();
    replay(&fixture.live, GROK_TURN_END_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    polls_settled().await;
    assert_eq!(status(&fixture.state), Some(SessionStatus::Working));
    assert!(fixture.state.turn_is_open("s"));
    draw_prompt(&fixture.live);
    assert!(reaches(&fixture.state, SessionStatus::Idle).await);
}

/// A cancel reads idle the same way, not paused: the title cannot tell it from an end.
#[tokio::test]
async fn a_cancel_reads_idle_and_is_not_paused() {
    let fixture = bare("cancel", AgentId::GROK);
    let _settle = without_settle();
    draw_prompt(&fixture.live);
    replay(&fixture.live, GROK_CANCEL_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Idle).await);
    assert!(!fixture.state.turn_is_open("s"));
    assert!(!fixture.state.last_turn_abnormal("s"));
}

/// The next running title after a closed turn opens another.
#[tokio::test]
async fn a_running_title_after_a_closed_turn_opens_another() {
    let fixture = bare("again", AgentId::GROK);
    let _settle = without_settle();
    draw_prompt(&fixture.live);
    replay(&fixture.live, GROK_TURN_END_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Idle).await);
    replay(&fixture.live, GROK_TOOL_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    assert!(fixture.state.turn_is_open("s"));
}

/// A prompt of the agent's on the screen keeps the turn open whatever the title says, and the turn
/// closes once it is gone.
#[tokio::test]
async fn a_prompt_on_the_screen_keeps_the_turn_open() {
    let fixture = bare("prompt", AgentId::GROK);
    let _settle = without_settle();
    // In one chunk: only its last title, which says nothing of a turn, is taken.
    fixture.live.on_output(GROK_SHELL_CARD);
    replay(&fixture.live, GROK_CANCEL_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    polls_settled().await;
    assert_eq!(status(&fixture.state), Some(SessionStatus::Working));
    assert!(fixture.state.turn_is_open("s"));

    draw_prompt(&fixture.live);
    assert!(reaches(&fixture.state, SessionStatus::Idle).await);
    assert!(!fixture.state.last_turn_abnormal("s"));
}

/// The program's first status report takes over from the title for the rest of the launch, from
/// the turn the title opened.
#[tokio::test]
async fn the_first_status_report_takes_over() {
    let fixture = bare("report", AgentId::GROK);
    let _settle = without_settle();
    replay(&fixture.live, GROK_TOOL_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    fixture.live.on_output(b"\x1b]7501;state=done\x07");
    assert!(reaches(&fixture.state, SessionStatus::Idle).await);
    assert!(!fixture.live.title_gives_status());
    assert!(
        !fixture.state.turn_is_open("s"),
        "the report closed the turn"
    );
    replay(&fixture.live, GROK_TOOL_TITLES);
    polls_settled().await;
    assert_eq!(
        status(&fixture.state),
        Some(SessionStatus::Idle),
        "a title moves nothing once the program reports"
    );
}

/// A hook moving the launch to present ends the source at once: a turn the title opened is left to
/// the hooks, and no title opens another.
#[tokio::test]
async fn a_hook_takes_over() {
    let fixture = bare("hook", AgentId::GROK);
    let _settle = without_settle();
    replay(&fixture.live, GROK_TOOL_TITLES);
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    crate::session::hook_absence::on_hook(&fixture.state, &fixture.live);
    assert!(!fixture.live.title_gives_status());
    replay(&fixture.live, GROK_CANCEL_TITLES);
    polls_settled().await;
    assert_eq!(status(&fixture.state), Some(SessionStatus::Working));
    assert!(fixture.state.turn_is_open("s"));
}

/// A launch whose hooks are not absent takes nothing from its title.
#[tokio::test]
async fn a_launch_with_hooks_takes_nothing_from_its_title() {
    let (state, dir) = app_state("title-status-hooks");
    state.store.insert_console(&console(&dir)).expect("console");
    state
        .store
        .insert_session(&Session {
            agent: AgentId::GROK,
            ..console_session("s", SessionStatus::Idle)
        })
        .expect("session");
    let live = crate::test_support::idle_stand_in_of("s", AgentId::GROK);
    assert!(!live.hooks_absent());
    state.register_live(live.clone());
    replay(&live, GROK_TOOL_TITLES);
    polls_settled().await;
    assert_eq!(status(&state), Some(SessionStatus::Idle));
    assert!(!state.turn_is_open("s"));
}

/// A reader that reads activity off a braille spinner and an input box drawn as `INPUT>`, and
/// declares that the box is drawn wherever its agent is at its prompt.
struct BoxedTitles;

impl ScreenReader for BoxedTitles {
    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        screen.lines().find_map(|line| {
            line.trim_start()
                .strip_prefix("INPUT>")
                .map(|text| vec![text.trim().to_string()])
        })
    }

    fn reads_input_box(&self) -> bool {
        true
    }

    fn title_activity(&self, glyph: Option<char>) -> Option<Shown> {
        spinner_activity(glyph)
    }
}

static BOXED_TITLES: BoxedTitles = BoxedTitles;

/// Where the provider's reader reads its input box, a title back at the prompt closes the turn only
/// once the box is drawn.
#[tokio::test]
async fn a_reader_of_the_input_box_closes_the_turn_only_with_the_box_drawn() {
    let _screen = crate::test_support::test_provider::declaring_screen(&BOXED_TITLES);
    let fixture = bare("input-box", AgentId::TEST);
    let _settle = without_settle();
    replay(&fixture.live, "\x1b]0;\u{280B} thinking\x07".as_bytes());
    assert!(reaches(&fixture.state, SessionStatus::Working).await);
    replay(&fixture.live, b"\x1b]0;at the prompt\x07");
    polls_settled().await;
    assert_eq!(
        status(&fixture.state),
        Some(SessionStatus::Working),
        "no input box on the screen"
    );
    fixture.live.on_output(b"\x1b[2J\x1b[HINPUT> \r\n");
    assert!(reaches(&fixture.state, SessionStatus::Idle).await);
    assert!(!fixture.state.turn_is_open("s"));
}
