use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::input_box::captures::*;
use super::*;
use crate::protocol::{Agent, CodedError, Session, SessionStatus};
use crate::reporting::{self, Delivery, WhenBlocked};
use crate::state::AppState;
use crate::test_support::{
    app_state, console, console_session, fake_live_session, watched_live_session, ScratchDir,
    StandIn, PATIENCE,
};

/// A running title of each agent, as it sends one a turn.
fn running_title(agent: Agent) -> &'static [u8] {
    match agent {
        Agent::Claude => "\x1b]0;\u{25D0} Claude Code\x07".as_bytes(),
        Agent::Codex => include_bytes!("../../testdata/codex_working_title.bin"),
        Agent::Grok => "\x1b]0;\u{280B} - Waiting for response\u{2026} - grok\x07".as_bytes(),
    }
}

const CLAUDE_AT_PROMPT_TITLE: &[u8] = "\x1b]0;\u{2733} Claude Code\x07".as_bytes();
const GROK_AT_PROMPT_TITLE: &[u8] = b"\x1b]0;NOTOKEN wait - grok\x07";

struct Fixture {
    state: Arc<AppState>,
    live: StandIn,
    keys: PathBuf,
    _dir: ScratchDir,
}

impl Fixture {
    fn live(&self) -> Arc<LiveSession> {
        (*self.live).clone()
    }

    /// What has been written into the session's input so far.
    fn keys(&self) -> Vec<u8> {
        std::fs::read(&self.keys).unwrap_or_default()
    }

    /// What was written into the session's input, once a marker written after the call has
    /// arrived, so an absence asserted in it is one of a reader that has caught up.
    fn keys_after_marker(&self) -> Vec<u8> {
        const MARKER: &[u8] = b"marker";
        self.live.write_input(MARKER).unwrap();
        let deadline = Instant::now() + PATIENCE;
        loop {
            let keys = self.keys();
            if keys.ends_with(MARKER) || Instant::now() >= deadline {
                assert!(keys.ends_with(MARKER), "{keys:?}");
                return keys[..keys.len() - MARKER.len()].to_vec();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// A session `s` of `agent` at `status`, with a stand-in on a PTY in raw mode that keeps every byte
/// written into its input, so a test can tell exactly what went in. `trust_watch` leaves its trust
/// watch looking for the agent's screen, as a freshly launched session's is.
fn fixture(name: &str, agent: Agent, status: SessionStatus, trust_watch: bool) -> Fixture {
    let (state, dir) = app_state(&format!("interrupt-{name}"));
    state.store.insert_console(&console(&dir)).unwrap();
    state
        .store
        .insert_session(&Session {
            agent,
            ..console_session("s", status)
        })
        .unwrap();
    let ready = dir.join("ready");
    let keys = dir.join("keys");
    let script = format!(
        "stty raw -echo && touch '{}' && exec cat > '{}'",
        ready.display(),
        keys.display()
    );
    let live = if trust_watch {
        watched_live_session("s", agent, 120, 40, &script, None)
    } else {
        fake_live_session("s", agent, 120, 40, &script)
    };
    let deadline = Instant::now() + PATIENCE;
    while !ready.exists() {
        assert!(Instant::now() < deadline, "the stand-in did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    state.register_live(live.clone());
    Fixture {
        state,
        live,
        keys,
        _dir: dir,
    }
}

/// [`fixture`] in a turn: working by its hooks, with its title showing the turn running.
fn working(name: &str, agent: Agent) -> Fixture {
    let fixture = fixture(name, agent, SessionStatus::Working, false);
    fixture.live.on_output(running_title(agent));
    fixture
}

/// Draws `screen` (rows of text) as the stand-in's whole screen.
fn draw(live: &LiveSession, screen: &str) {
    let mut bytes = b"\x1b[2J\x1b[H".to_vec();
    bytes.extend(screen.trim_end().replace('\n', "\r\n").into_bytes());
    live.on_output(&bytes);
}

/// A key the agent is to see written, and what it does about it.
type Step = (&'static [u8], Box<dyn FnOnce() + Send>);

/// Plays the agent: for each step, waits until exactly the keys given so far have been written into
/// the session, then does what the agent does about them.
fn respond(fixture: &Fixture, steps: Vec<Step>) -> std::thread::JoinHandle<()> {
    let keys = fixture.keys.clone();
    std::thread::spawn(move || {
        let mut expected = Vec::new();
        for (key, act) in steps {
            expected.extend_from_slice(key);
            let deadline = Instant::now() + PATIENCE;
            while std::fs::read(&keys).unwrap_or_default() != expected {
                if Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            act();
        }
    })
}

/// Moves the session out of `Working`, as the hook or watch that follows a cancel does.
fn back_at_prompt(state: &Arc<AppState>) -> Box<dyn FnOnce() + Send> {
    let state = state.clone();
    Box::new(move || {
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
    })
}

#[tokio::test]
async fn codex_is_sent_one_esc_and_told_of_the_messages_its_cancel_dropped() {
    let fixture = working("codex", Agent::Codex);
    fixture.state.turn_started("s", None);
    fixture.state.note_written_mid_turn("s");
    fixture.state.note_written_mid_turn("s");
    draw(&fixture.live, "›\n\n  100% context left");
    assert_eq!(
        input_box(Agent::Codex, &fixture.live.screen_text_undimmed()).as_deref(),
        Some("")
    );
    fixture.live.on_output(running_title(Agent::Codex));
    let live = fixture.live();
    let state = fixture.state.clone();
    let agent = respond(
        &fixture,
        vec![(
            ESC,
            Box::new(move || {
                draw(&live, "› preserved user draft\n\n  100% context left");
                state.apply_hook_status("s", SessionStatus::Idle).unwrap();
            }),
        )],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert_eq!(result.status, SessionStatus::Idle);
    assert!(
        result.note.contains("about 2 message(s)") && result.note.contains("Codex steers"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), ESC);
}

#[tokio::test]
async fn grok_is_sent_ctrl_u_then_ctrl_c() {
    let fixture = working("grok", Agent::Grok);
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(|| {})),
            (CTRL_C, back_at_prompt(&fixture.state)),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert_eq!(fixture.keys_after_marker(), [CTRL_U, CTRL_C].concat());
}

/// A message the agent had queued starts a turn of its own milliseconds after the cancel, so the
/// session reads `Working` again before anything polling its status could see it leave; the turn
/// that ended is what counts.
#[tokio::test]
async fn a_queued_message_running_right_after_the_cancel_does_not_hide_it() {
    let fixture = working("grok-requeued", Agent::Grok);
    fixture.state.turn_started("s", None);
    fixture.state.note_written_mid_turn("s");
    let state = fixture.state.clone();
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(|| {})),
            (
                CTRL_C,
                Box::new(move || {
                    // `StopCancelled`, then the queued message's `UserPromptSubmit`.
                    state.abandon_turn("s");
                    state.apply_hook_status("s", SessionStatus::Idle).unwrap();
                    state.turn_started("s", Some("the queued message"));
                    state
                        .apply_hook_status("s", SessionStatus::Working)
                        .unwrap();
                }),
            ),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert_eq!(result.status, SessionStatus::Working);
    assert!(
        result.note.contains("about 1 message(s)")
            && result.note.contains("Grok Build keeps them queued"),
        "{}",
        result.note
    );
}

/// An Esc before the model has shown anything has Claude Code put the prompt back in its input
/// box; it is cleared with one Ctrl-U a press until the screen shows the box empty.
#[tokio::test]
async fn claude_is_sent_one_esc_and_the_prompt_it_restores_is_cleared() {
    let fixture = working("claude-restored", Agent::Claude);
    draw(&fixture.live, CLEARED);
    let live = fixture.live();
    let (first, second) = (live.clone(), live.clone());
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![
            (
                ESC,
                Box::new(move || {
                    live.on_output(CLAUDE_AT_PROMPT_TITLE);
                    draw(&live, RESTORED_LINES);
                }),
            ),
            (CTRL_U, Box::new(move || draw(&first, ONE_CTRL_U))),
            (
                CTRL_U,
                Box::new(move || {
                    draw(&second, CLEARED);
                    back();
                }),
            ),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(!result.note.contains("input box"), "{}", result.note);
    assert_eq!(fixture.keys_after_marker(), [ESC, CTRL_U, CTRL_U].concat());
}

/// A faint placeholder in the box before the Esc is no text of the user's: the prompt Claude Code
/// puts back is cleared as from an empty box.
#[tokio::test]
async fn a_faint_placeholder_does_not_keep_the_restored_prompt() {
    let fixture = working("claude-placeholder", Agent::Claude);
    let placeholder = CLEARED.replacen(
        "\u{276F}\n",
        "\u{276F} \x1b[2mTry \"refactor <filepath>\"\x1b[22m\n",
        1,
    );
    assert_ne!(placeholder, CLEARED);
    draw(&fixture.live, &placeholder);
    let live = fixture.live();
    let cleared = live.clone();
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![
            (
                ESC,
                Box::new(move || {
                    live.on_output(CLAUDE_AT_PROMPT_TITLE);
                    draw(&live, RESTORED);
                }),
            ),
            (
                CTRL_U,
                Box::new(move || {
                    draw(&cleared, CLEARED);
                    back();
                }),
            ),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(!result.note.contains("input box"), "{}", result.note);
    assert_eq!(fixture.keys_after_marker(), [ESC, CTRL_U].concat());
}

/// A message sent while the restored prompt is being cleared waits until the clearing is over,
/// rather than joining the prompt.
#[tokio::test]
async fn a_message_sent_during_the_clearing_goes_in_after_it() {
    let fixture = working("claude-message-during-clear", Agent::Claude);
    draw(&fixture.live, CLEARED);
    let live = fixture.live();
    let (first, second) = (live.clone(), live.clone());
    let state = fixture.state.clone();
    let back = back_at_prompt(&fixture.state);
    let (sent, delivery) = std::sync::mpsc::channel();
    let agent = respond(
        &fixture,
        vec![
            (
                ESC,
                Box::new(move || {
                    live.on_output(CLAUDE_AT_PROMPT_TITLE);
                    draw(&live, RESTORED_LINES);
                }),
            ),
            (
                CTRL_U,
                Box::new(move || {
                    let written =
                        reporting::write_message(&state, "s", "sent meanwhile", WhenBlocked::Queue);
                    sent.send(written.unwrap()).unwrap();
                    draw(&first, ONE_CTRL_U);
                }),
            ),
            (
                CTRL_U,
                Box::new(move || {
                    draw(&second, CLEARED);
                    back();
                }),
            ),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert_eq!(delivery.recv().unwrap(), Delivery::Queued);
    let deadline = Instant::now() + PATIENCE;
    while !String::from_utf8_lossy(&fixture.keys()).contains("sent meanwhile") {
        assert!(Instant::now() < deadline, "the message never went in");
        std::thread::sleep(Duration::from_millis(10));
    }
    let keys = fixture.keys();
    let keys_first = [ESC, CTRL_U, CTRL_U].concat();
    assert!(keys.starts_with(&keys_first), "{keys:?}");
    assert!(String::from_utf8_lossy(&keys[keys_first.len()..]).contains("sent meanwhile"));
}

/// What was in the input box before the Esc is the user's, or a message left unsent: it stays, and
/// the caller is told it is there.
#[tokio::test]
async fn text_in_claude_s_input_box_before_the_esc_is_left_alone() {
    let fixture = working("claude-draft", Agent::Claude);
    draw(&fixture.live, DRAFT);
    let live = fixture.live();
    let state = fixture.state.clone();
    let agent = respond(
        &fixture,
        vec![(
            ESC,
            Box::new(move || {
                live.on_output(CLAUDE_AT_PROMPT_TITLE);
                // Long enough for a clear to have been tried.
                std::thread::sleep(Duration::from_secs(1));
                state.apply_hook_status("s", SessionStatus::Idle).unwrap();
            }),
        )],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(
        result
            .note
            .contains("Claude Code's input box still holds text"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), ESC);
}

/// Ctrl-C before Grok Build's first token has it put the prompt back in its composer; it is
/// cleared the same way as Claude Code's.
#[tokio::test]
async fn the_prompt_grok_restores_is_cleared() {
    let fixture = working("grok-restored", Agent::Grok);
    draw(&fixture.live, GROK_WORKING);
    let live = fixture.live();
    let (first, second) = (live.clone(), live.clone());
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(|| {})),
            (
                CTRL_C,
                Box::new(move || {
                    live.on_output(GROK_AT_PROMPT_TITLE);
                    draw(&live, GROK_RESTORED_LINES);
                }),
            ),
            (CTRL_U, Box::new(move || draw(&first, GROK_ONE_CTRL_U))),
            (
                CTRL_U,
                Box::new(move || {
                    draw(&second, GROK_CLEARED);
                    back();
                }),
            ),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(!result.note.contains("input box"), "{}", result.note);
    assert_eq!(
        fixture.keys_after_marker(),
        [CTRL_U, CTRL_C, CTRL_U, CTRL_U].concat()
    );
}

/// A one-line draft in Grok's composer is cleared, seen gone, before the Ctrl-C, and the prompt
/// Grok then puts back is cleared too.
#[tokio::test]
async fn a_draft_in_grok_s_composer_is_cleared_before_the_ctrl_c() {
    let fixture = working("grok-draft", Agent::Grok);
    draw(&fixture.live, GROK_DRAFT);
    let live = fixture.live();
    let (first, second) = (live.clone(), live.clone());
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(move || draw(&first, GROK_WORKING))),
            (
                CTRL_C,
                Box::new(move || {
                    live.on_output(GROK_AT_PROMPT_TITLE);
                    draw(&live, GROK_RESTORED);
                }),
            ),
            (
                CTRL_U,
                Box::new(move || {
                    draw(&second, GROK_CLEARED);
                    back();
                }),
            ),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(
        result
            .note
            .contains("composer held a draft, which was cleared first")
            && !result.note.contains("still holds text"),
        "{}",
        result.note
    );
    assert_eq!(
        fixture.keys_after_marker(),
        [CTRL_U, CTRL_C, CTRL_U].concat()
    );
}

/// A draft of several lines takes a press a line and one a line break; the Ctrl-C waits until the
/// screen shows the composer empty, or it would only clear the rest of the draft.
#[tokio::test]
async fn a_draft_of_several_lines_is_cleared_press_by_press() {
    let fixture = working("grok-draft-lines", Agent::Grok);
    draw(&fixture.live, GROK_RESTORED_LINES);
    let live = fixture.live();
    let (first, second) = (live.clone(), live.clone());
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(move || draw(&first, GROK_ONE_CTRL_U))),
            (CTRL_U, Box::new(|| {})),
            (CTRL_U, Box::new(move || draw(&second, GROK_WORKING))),
            (CTRL_C, back_at_prompt(&fixture.state)),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(
        result
            .note
            .contains("composer held a draft, which was cleared first"),
        "{}",
        result.note
    );
    assert_eq!(
        fixture.keys_after_marker(),
        [CTRL_U, CTRL_U, CTRL_U, CTRL_C].concat()
    );
}

/// A restored prompt the presses do not clear is said to be still there.
#[tokio::test]
async fn a_restored_prompt_that_will_not_clear_is_reported() {
    let fixture = working("grok-left", Agent::Grok);
    draw(&fixture.live, GROK_WORKING);
    let live = fixture.live();
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(|| {})),
            (
                CTRL_C,
                Box::new(move || {
                    live.on_output(GROK_AT_PROMPT_TITLE);
                    draw(&live, GROK_RESTORED);
                }),
            ),
            (CTRL_U, Box::new(|| {})),
            (CTRL_U, Box::new(|| {})),
            (CTRL_U, back_at_prompt(&fixture.state)),
        ],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(
        result
            .note
            .contains("Grok Build's input box still holds text")
            && result.note.contains("could not be cleared"),
        "{}",
        result.note
    );
    assert_eq!(
        fixture.keys_after_marker(),
        [CTRL_U, CTRL_C, CTRL_U, CTRL_U, CTRL_U].concat()
    );
}

/// A turn that ends between Grok's Ctrl-U and its Ctrl-C gets no Ctrl-C, and the caller is told
/// what did go in.
#[tokio::test]
async fn grok_s_ctrl_c_is_held_once_the_turn_has_ended() {
    let fixture = working("grok-ended-between", Agent::Grok);
    let agent = respond(&fixture, vec![(CTRL_U, back_at_prompt(&fixture.state))]);

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(!result.interrupted);
    assert!(
        result.note.contains("Only a Ctrl-U was sent")
            && result.note.contains("the turn ended first"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), CTRL_U);
}

/// Someone else writing into the terminal between two keys stops the next one.
#[tokio::test]
async fn a_write_by_someone_else_between_the_keys_stops_the_next_one() {
    let fixture = working("grok-touched", Agent::Grok);
    let live = fixture.live();
    let agent = respond(
        &fixture,
        vec![(CTRL_U, Box::new(move || live.write_input(b"x").unwrap()))],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(!result.interrupted);
    assert!(
        result
            .note
            .contains("something else wrote into its terminal first"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), [CTRL_U, b"x"].concat());
}

/// A key that goes in and leaves the session working is said to have done so.
#[tokio::test]
async fn a_session_still_working_after_the_key_is_reported() {
    let fixture = working("codex-still-working", Agent::Codex);

    let result = interrupt_session(&fixture.state, "s").await.unwrap();

    assert!(!result.interrupted);
    assert_eq!(result.status, SessionStatus::Working);
    assert!(
        result
            .note
            .contains("The interrupt key was sent, but the session still reads as working"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), ESC);
}

/// A session at its prompt has nothing to interrupt, and nothing is written into it.
#[tokio::test]
async fn an_idle_session_is_left_alone() {
    let fixture = fixture("idle", Agent::Codex, SessionStatus::Idle, false);
    fixture.live.on_output(running_title(Agent::Codex));

    let result = interrupt_session(&fixture.state, "s").await.unwrap();

    assert!(!result.interrupted);
    assert_eq!(result.status, SessionStatus::Idle);
    assert_eq!(result.note, NOTHING_TO_INTERRUPT);
    assert_eq!(fixture.keys_after_marker(), b"");
}

/// Working by its hooks while the title does not show the turn running gets no key, and the caller
/// is told why rather than that the session is awaiting instructions.
#[tokio::test]
async fn no_key_goes_in_unless_the_title_shows_the_turn_running() {
    let fixture = fixture(
        "title-at-prompt",
        Agent::Claude,
        SessionStatus::Working,
        false,
    );
    fixture.live.on_output(running_title(Agent::Claude));
    fixture.live.on_output(CLAUDE_AT_PROMPT_TITLE);

    let result = interrupt_session(&fixture.state, "s").await.unwrap();

    assert!(!result.interrupted);
    assert_eq!(result.note, NOT_SHOWN_RUNNING);
    assert_eq!(fixture.keys_after_marker(), b"");
}

#[tokio::test]
async fn a_second_interrupt_while_one_is_under_way_sends_nothing() {
    let fixture = working("claimed", Agent::Codex);
    let _first = fixture.state.claim_interrupt("s").unwrap();

    let result = interrupt_session(&fixture.state, "s").await.unwrap();

    assert!(!result.interrupted);
    assert!(result.note.contains("already under way"), "{}", result.note);
    assert_eq!(fixture.keys_after_marker(), b"");
}

/// The code of a refusal and its message.
async fn refused_with(state: &Arc<AppState>) -> (String, String) {
    let err = interrupt_session(state, "s").await.expect_err("refused");
    let code = err
        .downcast_ref::<CodedError>()
        .unwrap_or_else(|| panic!("not a coded error: {err}"))
        .code
        .to_string();
    (code, err.to_string())
}

/// Every state a key must not go into is refused with its own code and message, and nothing is
/// written.
#[tokio::test]
async fn sessions_that_cannot_take_an_interrupt_are_refused() {
    type Setup = fn(&Fixture) -> Option<Box<dyn std::any::Any>>;
    let not_running = (error_code::SESSION_NOT_RUNNING, "is not running");
    let starting = (error_code::SESSION_NOT_RUNNING, "still starting");
    let trust = (
        error_code::SESSION_TRUST_PENDING,
        "folder-trust confirmation",
    );
    let waiting = (error_code::SESSION_WAITING_FOR_USER, "waiting for the user");
    let busy = (
        error_code::SESSION_BUSY,
        "being switched to another account",
    );
    /// The state's name, the agent, its status, whether its trust watch still looks, what else is
    /// set up, and the code and part of the message it is refused with.
    type Case = (
        &'static str,
        Agent,
        SessionStatus,
        bool,
        Setup,
        (&'static str, &'static str),
    );
    let cases: [Case; 11] = [
        (
            "archived",
            Agent::Claude,
            SessionStatus::Archived,
            false,
            |_| None,
            not_running,
        ),
        (
            "interrupted",
            Agent::Claude,
            SessionStatus::Interrupted,
            false,
            |_| None,
            not_running,
        ),
        (
            "starting",
            Agent::Claude,
            SessionStatus::Working,
            true,
            |_| None,
            starting,
        ),
        (
            "trust",
            Agent::Claude,
            SessionStatus::Working,
            true,
            |f| {
                f.live
                    .on_output(include_bytes!("../../testdata/claude_trust_screen.bin"));
                None
            },
            trust,
        ),
        (
            "dialog",
            Agent::Claude,
            SessionStatus::Working,
            false,
            |f| {
                f.live.on_output(
                    "We're changing auto mode.\r\nEnter to continue \u{B7} Esc to cancel\r\n"
                        .as_bytes(),
                );
                None
            },
            waiting,
        ),
        (
            "codex-action",
            Agent::Codex,
            SessionStatus::Working,
            false,
            |f| {
                f.live
                    .on_output(include_bytes!("../../testdata/codex_approval_title.bin"));
                None
            },
            waiting,
        ),
        (
            "hook-hand",
            Agent::Claude,
            SessionStatus::WaitingUser,
            false,
            |_| None,
            waiting,
        ),
        (
            "request-hand",
            Agent::Claude,
            SessionStatus::Working,
            false,
            |f| {
                f.state.raise_request_hand("s").unwrap();
                None
            },
            waiting,
        ),
        (
            "switching",
            Agent::Claude,
            SessionStatus::Working,
            false,
            |f| Some(Box::new(f.state.begin_switch("s").unwrap())),
            busy,
        ),
        (
            "handoff",
            Agent::Claude,
            SessionStatus::Working,
            false,
            |f| Some(Box::new(f.state.hold_outbox("s"))),
            busy,
        ),
        (
            "sharing-change",
            Agent::Claude,
            SessionStatus::Working,
            false,
            |f| Some(Box::new(f.state.hold_outbox_for_sharing("s"))),
            busy,
        ),
    ];
    for (name, agent, status, trust_watch, setup, (code, message)) in cases {
        let fixture = fixture(&format!("refused-{name}"), agent, status, trust_watch);
        fixture.live.on_output(running_title(agent));
        let held = setup(&fixture);
        let (refused, said) = refused_with(&fixture.state).await;
        assert_eq!(refused, code, "{name}");
        assert!(said.contains(message), "{name}: {said}");
        drop(held);
        assert_eq!(fixture.keys_after_marker(), b"", "{name}");
    }
}

/// With no process, or only a launch under way, there is nothing to send a key to.
#[tokio::test]
async fn a_session_with_no_process_is_refused() {
    let (state, dir) = app_state("interrupt-no-process");
    state.store.insert_console(&console(&dir)).unwrap();
    state
        .store
        .insert_session(&console_session("s", SessionStatus::Working))
        .unwrap();
    let (code, message) = refused_with(&state).await;
    assert_eq!(code, error_code::SESSION_NOT_RUNNING);
    assert!(message.contains("is not running"), "{message}");
    let _launch = state.begin_launch("s").unwrap();
    let (code, message) = refused_with(&state).await;
    assert_eq!(code, error_code::SESSION_NOT_RUNNING);
    assert!(message.contains("still starting"), "{message}");
}
