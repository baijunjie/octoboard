use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::messaging::delivery::{self, Delivery, WhenBlocked};
use crate::protocol::{AgentId, CodedError, Session, SessionStatus};
use crate::session::input_box::captures::*;
use crate::session::input_box::input_box;
use crate::state::AppState;
use crate::test_support::{
    app_state, console, console_session, fake_live_session, watched_live_session, ScratchDir,
    StandIn, PATIENCE,
};

/// A running title of each agent, as it sends one a turn.
fn running_title(agent: AgentId) -> &'static [u8] {
    match agent {
        AgentId::CLAUDE => "\x1b]0;\u{25D0} Claude Code\x07".as_bytes(),
        AgentId::CODEX => include_bytes!("../../../testdata/codex_working_title.bin"),
        AgentId::GROK => "\x1b]0;\u{280B} - Waiting for response\u{2026} - grok\x07".as_bytes(),
        other => panic!("no fixture for {other}"),
    }
}

/// The bytes the interrupt's keys reach the stand-in as.
const ESC: &[u8] = b"\x1b";
const CTRL_U: &[u8] = b"\x15";
const CTRL_C: &[u8] = b"\x03";

const CLAUDE_AT_PROMPT_TITLE: &[u8] = "\x1b]0;\u{2733} Claude Code\x07".as_bytes();
const GROK_AT_PROMPT_TITLE: &[u8] = b"\x1b]0;NOTOKEN wait - grok\x07";

/// A title Claude Code 2.1.322 sends once the terminal answers its Program Status Protocol query:
/// no glyph, whatever its turn does.
const CLAUDE_GLYPHLESS_TITLE: &[u8] = b"\x1b]0;Claude Code\x07";

/// What it reports of its turn instead.
const CLAUDE_REPORTS_WORKING: &[u8] = b"\x1b]7501;state=working:app=claude-code\x1b\\";
const CLAUDE_REPORTS_IDLE: &[u8] = b"\x1b]7501;state=idle:app=claude-code\x1b\\";

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
fn fixture(name: &str, agent: AgentId, status: SessionStatus, trust_watch: bool) -> Fixture {
    fixture_with(name, agent, status, trust_watch, None)
}

/// [`fixture`] whose session delivers through `transport`, a server of its agent's own.
fn fixture_with(
    name: &str,
    agent: AgentId,
    status: SessionStatus,
    trust_watch: bool,
    transport: Option<Arc<dyn crate::session::term::MessageTransport>>,
) -> Fixture {
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
    let live = match transport {
        Some(transport) => live.with_transport(transport),
        None => live,
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
fn working(name: &str, agent: AgentId) -> Fixture {
    let fixture = fixture(name, agent, SessionStatus::Working, false);
    fixture.live.on_output(running_title(agent));
    fixture
}

/// [`working`] for a Claude Code that reports its state through OSC 7501 and titles itself with no
/// glyph: its hooks say `Working`, and only its reports say the turn is running.
fn working_by_reports(name: &str) -> Fixture {
    let fixture = fixture(name, AgentId::CLAUDE, SessionStatus::Working, false);
    fixture.live.on_output(CLAUDE_GLYPHLESS_TITLE);
    fixture.live.on_output(CLAUDE_REPORTS_WORKING);
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
    let fixture = working("codex", AgentId::CODEX);
    fixture.state.turn_started("s", None);
    fixture.state.note_written_mid_turn("s");
    fixture.state.note_written_mid_turn("s");
    draw(&fixture.live, "›\n\n  100% context left");
    assert_eq!(
        input_box(AgentId::CODEX, &fixture.live.screen_text_undimmed()).as_deref(),
        Some("")
    );
    fixture.live.on_output(running_title(AgentId::CODEX));
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

/// A server of the agent's own standing in for Codex's: it records each start of its queue and
/// answers it as told.
struct QueueServer {
    starts: std::sync::Mutex<usize>,
    answer: crate::session::term::QueueStart,
}

impl QueueServer {
    fn new(answer: crate::session::term::QueueStart) -> Arc<Self> {
        Arc::new(Self {
            starts: Default::default(),
            answer,
        })
    }
}

impl crate::session::term::MessageTransport for QueueServer {
    fn send(
        &self,
        _: &str,
        _: &str,
    ) -> std::result::Result<crate::session::term::Sent, crate::platform::term::PartialWrite> {
        unreachable!("nothing is delivered here")
    }
    fn attach(&self, _: crate::session::term::TransportEvents) {}
    fn start_queue(&self) -> crate::session::term::QueueStart {
        *self.starts.lock().unwrap() += 1;
        self.answer.clone()
    }
    fn stop(&self) {}
}

/// Interrupts a Codex session delivering through `server` whose turn had one message written into
/// its terminal, and gives the note.
async fn interrupt_with_queue(name: &str, server: Arc<QueueServer>) -> String {
    let fixture = fixture_with(
        name,
        AgentId::CODEX,
        SessionStatus::Working,
        false,
        Some(server),
    );
    fixture.live.on_output(running_title(AgentId::CODEX));
    fixture.state.turn_started("s", None);
    fixture.state.note_written_mid_turn("s");
    draw(&fixture.live, "›\n\n  100% context left");
    let agent = respond(&fixture, vec![(ESC, back_at_prompt(&fixture.state))]);
    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();
    assert!(result.interrupted, "{result:?}");
    result.note
}

/// What waits in Codex's own queue was never part of the cancelled turn, whichever turn queued it:
/// the queue is started after every interrupt that stopped a turn, once the key is in, and the
/// note says what came of it. What went into the terminal keeps its warning.
#[tokio::test]
async fn codex_starts_its_queue_after_a_cancel_and_says_what_came_of_it() {
    use crate::session::term::QueueStart;
    let server = QueueServer::new(QueueStart::Started(2));
    let note = interrupt_with_queue("codex-queue", server.clone()).await;
    assert_eq!(*server.starts.lock().unwrap(), 1);
    assert!(
        note.contains("2 message(s) Octoboard put into Codex's own queue")
            && note.contains("now run as turns of their own")
            && note.contains("about 1 message(s)")
            && note.contains("Codex steers"),
        "{note}"
    );

    let behind = QueueServer::new(QueueStart::BehindOthers(1));
    let note = interrupt_with_queue("codex-queue-behind", behind).await;
    assert!(
        note.contains("behind messages someone else queued") && note.contains("did not start"),
        "{note}"
    );

    let failing = QueueServer::new(QueueStart::Failed("refused".to_string()));
    let note = interrupt_with_queue("codex-queue-refused", failing).await;
    assert!(
        note.contains("may still be waiting") && !note.contains("still in Codex's queue"),
        "{note}"
    );

    let empty = QueueServer::new(QueueStart::Nothing);
    let note = interrupt_with_queue("codex-queue-empty", empty.clone()).await;
    assert_eq!(*empty.starts.lock().unwrap(), 1);
    assert!(!note.contains("own queue"), "{note}");
}

/// A provider with no interrupt key known is sent none, and told so.
#[tokio::test]
async fn a_provider_with_no_known_interrupt_key_is_sent_nothing_and_told_so() {
    let fixture = fixture("no-key", AgentId::TEST, SessionStatus::Working, false);
    let interrupted = interrupt_session(&fixture.state, "s").await.unwrap();
    assert!(!interrupted.interrupted);
    assert!(
        interrupted
            .note
            .starts_with("No key was sent: no key is known to interrupt a turn of"),
        "{}",
        interrupted.note
    );
    assert!(interrupted.note.contains("Test Agent"));
    assert!(fixture.keys_after_marker().is_empty());
}

#[tokio::test]
async fn grok_is_sent_ctrl_u_then_ctrl_c() {
    let fixture = working("grok", AgentId::GROK);
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
    let fixture = working("grok-requeued", AgentId::GROK);
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
                    // `StopCancelled`, then the queued message's `UserPromptSubmit`. The `Idle`
                    // and `Working` they report fall between two reads of the status, so they are
                    // left out: written here one after the other, the `Idle` could be read between
                    // them, which is what this test supposes nothing does.
                    state.abandon_turn("s");
                    state.turn_started("s", Some("the queued message"));
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
    let fixture = working("claude-restored", AgentId::CLAUDE);
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

/// A Claude Code that reports its state and titles itself with no glyph is sent one Esc while its
/// reports say a turn is running; its `idle` report is the turn over, and the prompt it restores is
/// cleared as it is for one whose title turns.
#[tokio::test]
async fn a_claude_whose_turn_shows_only_in_its_reports_is_sent_one_esc_and_its_restored_prompt_is_cleared(
) {
    let fixture = working_by_reports("claude-reported-restored");
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
                    live.on_output(CLAUDE_REPORTS_IDLE);
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

/// The same cancel with nothing restored into the box: one Esc, and no Ctrl-U.
#[tokio::test]
async fn a_reported_cancel_that_restores_nothing_is_one_esc() {
    let fixture = working_by_reports("claude-reported-plain");
    draw(&fixture.live, CLEARED);
    let live = fixture.live();
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![(
            ESC,
            Box::new(move || {
                live.on_output(CLAUDE_REPORTS_IDLE);
                back();
            }),
        )],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert_eq!(fixture.keys_after_marker(), ESC);
}

/// A turn the reports say is running can be interrupted though no title was ever sent, and one
/// that a stale prompt glyph in the title would deny: the reports are what the agent says of its
/// turn.
#[tokio::test]
async fn a_working_report_allows_the_key_whatever_the_title_says_or_does_not() {
    for title in [
        None,
        Some(CLAUDE_AT_PROMPT_TITLE),
        Some(CLAUDE_GLYPHLESS_TITLE),
    ] {
        let fixture = fixture(
            "claude-no-title",
            AgentId::CLAUDE,
            SessionStatus::Working,
            false,
        );
        fixture.live.on_output(CLAUDE_REPORTS_WORKING);
        if let Some(title) = title {
            fixture.live.on_output(title);
        }
        let live = fixture.live();
        let back = back_at_prompt(&fixture.state);
        let agent = respond(
            &fixture,
            vec![(
                ESC,
                Box::new(move || {
                    live.on_output(CLAUDE_REPORTS_IDLE);
                    back();
                }),
            )],
        );

        let result = interrupt_session(&fixture.state, "s").await.unwrap();
        agent.join().unwrap();

        assert!(result.interrupted, "{title:?}: {result:?}");
        assert_eq!(fixture.keys_after_marker(), ESC, "{title:?}");
    }
}

/// Working by its hooks while the agent reports its turn over gets no key, and the caller is told
/// why: whichever state it left the turn for.
#[tokio::test]
async fn no_key_goes_in_once_the_agent_reports_the_turn_over() {
    for state in ["idle", "done", "error", "blocked"] {
        let fixture = working_by_reports(&format!("claude-reported-{state}"));
        fixture
            .live
            .on_output(format!("\x1b]7501;state={state}:app=claude-code\x1b\\").as_bytes());

        let result = interrupt_session(&fixture.state, "s").await.unwrap();

        assert!(!result.interrupted, "{state}");
        assert_eq!(result.note, NOT_SHOWN_RUNNING, "{state}");
        assert_eq!(fixture.keys_after_marker(), b"", "{state}");
    }
}

/// A running title the agent left behind says nothing once it reports: the turn it reported over
/// gets no key whatever glyph the title still carries.
#[tokio::test]
async fn a_stale_running_title_does_not_outvote_a_report_of_the_turn_over() {
    let fixture = fixture(
        "claude-stale-running",
        AgentId::CLAUDE,
        SessionStatus::Working,
        false,
    );
    fixture.live.on_output(CLAUDE_REPORTS_IDLE);
    fixture.live.on_output(running_title(AgentId::CLAUDE));

    let result = interrupt_session(&fixture.state, "s").await.unwrap();

    assert!(!result.interrupted);
    assert_eq!(result.note, NOT_SHOWN_RUNNING);
    assert_eq!(fixture.keys_after_marker(), b"");
}

/// A session whose writes are held for the usage limit is at its prompt or in a turn like any other,
/// so the user's interrupt of a turn it runs goes through.
#[tokio::test]
async fn a_session_held_for_the_usage_limit_can_be_interrupted() {
    let fixture = working("claude-usage-limit-hold", AgentId::CLAUDE);
    draw(&fixture.live, CLEARED);
    let judgement = fixture.live.usage_limit.begin_judging(1);
    fixture.live.usage_limit.judged(judgement, true);
    assert_eq!(
        fixture.state.write_hold("s"),
        Some(crate::state::WriteHold::UsageLimit)
    );
    let live = fixture.live();
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![(
            ESC,
            Box::new(move || {
                live.on_output(CLAUDE_AT_PROMPT_TITLE);
                back();
            }),
        )],
    );

    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert_eq!(fixture.keys_after_marker(), ESC);
}

/// A faint placeholder in the box before the Esc is no text of the user's: the prompt Claude Code
/// puts back is cleared as from an empty box.
#[tokio::test]
async fn a_faint_placeholder_does_not_keep_the_restored_prompt() {
    let fixture = working("claude-placeholder", AgentId::CLAUDE);
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
    let fixture = working("claude-message-during-clear", AgentId::CLAUDE);
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
                        delivery::write_message(&state, "s", "sent meanwhile", WhenBlocked::Queue);
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
    let fixture = working("claude-draft", AgentId::CLAUDE);
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
    let fixture = working("grok-restored", AgentId::GROK);
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
    let fixture = working("grok-draft", AgentId::GROK);
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

/// [`GROK_ONE_CTRL_U`] after the next press, which removes the empty line the first left: the
/// composer a row shorter.
fn grok_line_break_removed() -> String {
    GROK_ONE_CTRL_U
        .lines()
        .filter(|row| {
            !(row.contains('\u{2502}')
                && row
                    .trim_matches(|c: char| c == ' ' || c == '\u{2502}')
                    .is_empty())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A draft of several lines takes a press a line and one a line break; the Ctrl-C waits until the
/// screen shows the composer empty, or it would only clear the rest of the draft.
#[tokio::test]
async fn a_draft_of_several_lines_is_cleared_press_by_press() {
    let fixture = working("grok-draft-lines", AgentId::GROK);
    draw(&fixture.live, GROK_RESTORED_LINES);
    let shorter = grok_line_break_removed();
    assert_ne!(
        input_box(AgentId::GROK, &shorter),
        None,
        "the composer is still read"
    );
    let live = fixture.live();
    let (first, second, third, titled) = (live.clone(), live.clone(), live.clone(), live);
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(move || draw(&first, GROK_ONE_CTRL_U))),
            (CTRL_U, Box::new(move || draw(&second, &shorter))),
            (CTRL_U, Box::new(move || draw(&third, GROK_WORKING))),
            (
                CTRL_C,
                Box::new(move || {
                    titled.on_output(GROK_AT_PROMPT_TITLE);
                    back();
                }),
            ),
        ],
    );

    // Nothing is restored, so the watch of the empty box only has to run out; a short one shows
    // that.
    RESTORE_OVERRIDE.with(|restore| restore.set(Some(Duration::from_millis(200))));
    let result = interrupt_session(&fixture.state, "s").await;
    RESTORE_OVERRIDE.with(|restore| restore.set(None));
    let result = result.unwrap();
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

/// A restored prompt a press does not change is said to be still there, and is not pressed again:
/// a further press would be one sent blind.
#[tokio::test]
async fn a_restored_prompt_that_will_not_clear_is_reported() {
    let fixture = working("grok-left", AgentId::GROK);
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
            (CTRL_U, back_at_prompt(&fixture.state)),
        ],
    );

    // Nothing draws the press, so its window only has to run out; a short one shows that.
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let result = interrupt_session(&fixture.state, "s").await;
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    let result = result.unwrap();
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
        [CTRL_U, CTRL_C, CTRL_U].concat()
    );
}

/// A draft whose Ctrl-U is never drawn ends the clearing after that one press, which is not
/// repeated blind; nothing visibly changed, so none of the draft is said to be cleared, and the
/// Ctrl-C is held back.
#[tokio::test]
async fn a_draft_whose_ctrl_u_is_never_drawn_is_left_and_the_ctrl_c_held() {
    let fixture = working("grok-draft-unseen", AgentId::GROK);
    draw(&fixture.live, GROK_DRAFT);

    // Nothing draws the press, so its window only has to run out; a short one shows that.
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let result = interrupt_session(&fixture.state, "s").await;
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    let result = result.unwrap();

    assert!(!result.interrupted, "{result:?}");
    assert!(
        result.note.contains("the draft would not clear")
            && result.note.contains("did not show changing the draft")
            && !result.note.contains("cleared part of the draft"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), CTRL_U);
}

/// The agent draws each press of a draft's clearing when it gets to it, which on a slow host can
/// be well after the press: the clearing waits to see it before pressing again, so the composer is
/// emptied with the presses it takes and the Ctrl-C follows.
#[tokio::test]
async fn a_ctrl_u_slow_to_show_is_waited_for_not_pressed_again() {
    let fixture = working("grok-draft-slow", AgentId::GROK);
    draw(&fixture.live, GROK_DRAFT);
    let live = fixture.live();
    let titled = live.clone();
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![
            (
                CTRL_U,
                Box::new(move || {
                    std::thread::sleep(Duration::from_secs(1));
                    // The spinner turns on meanwhile, as Grok's does through a turn, so the title
                    // still shows the turn running for the Ctrl-C.
                    live.on_output(running_title(AgentId::GROK));
                    draw(&live, GROK_WORKING);
                }),
            ),
            (
                CTRL_C,
                Box::new(move || {
                    titled.on_output(GROK_AT_PROMPT_TITLE);
                    back();
                }),
            ),
        ],
    );

    // Nothing is restored, so the watch of the empty box only has to run out; a short one shows
    // that.
    RESTORE_OVERRIDE.with(|restore| restore.set(Some(Duration::from_millis(200))));
    let result = interrupt_session(&fixture.state, "s").await;
    RESTORE_OVERRIDE.with(|restore| restore.set(None));
    let result = result.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(
        result
            .note
            .contains("composer held a draft, which was cleared first"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), [CTRL_U, CTRL_C].concat());
}

/// Claude Code's restored prompt is cleared press by press on a host that draws each press a second
/// after it: no press goes in before the one before it has shown, and the box is not reported as
/// left holding text.
#[tokio::test]
async fn a_restored_prompt_drawn_slowly_is_cleared_without_extra_presses() {
    let fixture = working("claude-restored-slow", AgentId::CLAUDE);
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
            (
                CTRL_U,
                Box::new(move || {
                    std::thread::sleep(Duration::from_secs(1));
                    draw(&first, ONE_CTRL_U);
                }),
            ),
            (
                CTRL_U,
                Box::new(move || {
                    std::thread::sleep(Duration::from_secs(1));
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

/// An agent that turns its title to the prompt, falls silent for longer than the screen takes to
/// count as quiet, ends the turn and only then draws the prompt it puts back: the box, still empty
/// when the screen first goes quiet, is watched until the prompt shows and cleared, and a message
/// sent meanwhile waits until the clearing is over rather than joining the prompt.
#[tokio::test]
async fn a_prompt_restored_well_after_the_title_turns_is_still_cleared() {
    let fixture = working("claude-restored-late", AgentId::CLAUDE);
    draw(&fixture.live, CLEARED);
    let live = fixture.live();
    let cleared = live.clone();
    let state = fixture.state.clone();
    let (sent, delivery) = std::sync::mpsc::channel();
    let agent = respond(
        &fixture,
        vec![
            (
                ESC,
                Box::new(move || {
                    live.on_output(CLAUDE_AT_PROMPT_TITLE);
                    // Far longer than the quiet window on any platform, and than the read that
                    // follows it.
                    std::thread::sleep(Duration::from_secs(1));
                    state.apply_hook_status("s", SessionStatus::Idle).unwrap();
                    let written =
                        delivery::write_message(&state, "s", "sent meanwhile", WhenBlocked::Queue);
                    sent.send(written.unwrap()).unwrap();
                    draw(&live, RESTORED);
                }),
            ),
            (CTRL_U, Box::new(move || draw(&cleared, CLEARED))),
        ],
    );

    // The prompt is drawn a second after the title turns: the watch is given all the time there is,
    // however slow the suite runs the stand-in.
    RESTORE_OVERRIDE.with(|restore| restore.set(Some(PATIENCE)));
    let result = interrupt_session(&fixture.state, "s").await;
    RESTORE_OVERRIDE.with(|restore| restore.set(None));
    let result = result.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(!result.note.contains("input box"), "{}", result.note);
    assert_eq!(delivery.recv().unwrap(), Delivery::Queued);
    let deadline = Instant::now() + PATIENCE;
    while !String::from_utf8_lossy(&fixture.keys()).contains("sent meanwhile") {
        assert!(Instant::now() < deadline, "the message never went in");
        std::thread::sleep(Duration::from_millis(10));
    }
    let keys = fixture.keys();
    let keys_first = [ESC, CTRL_U].concat();
    assert!(keys.starts_with(&keys_first), "{keys:?}");
    assert!(String::from_utf8_lossy(&keys[keys_first.len()..]).contains("sent meanwhile"));
}

/// A turn whose end is seen before its title turns, as Grok's hook can arrive within a fifth of a
/// second of the key: the box, empty before the key, is still watched once the title turns, and
/// the prompt drawn after it is cleared.
#[tokio::test]
async fn a_prompt_restored_after_the_turn_ends_before_the_title_turns_is_cleared() {
    let fixture = working("claude-ended-before-title", AgentId::CLAUDE);
    draw(&fixture.live, CLEARED);
    let live = fixture.live();
    let cleared = live.clone();
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![
            (
                ESC,
                Box::new(move || {
                    back();
                    // Many polls of the wait, so it has seen the turn over with the title unmoved.
                    std::thread::sleep(Duration::from_millis(300));
                    live.on_output(CLAUDE_AT_PROMPT_TITLE);
                    draw(&live, RESTORED);
                }),
            ),
            (CTRL_U, Box::new(move || draw(&cleared, CLEARED))),
        ],
    );

    // The prompt is drawn right after the title turns, but the watch for it is given all the time
    // there is, however slow the suite runs the stand-in.
    RESTORE_OVERRIDE.with(|restore| restore.set(Some(PATIENCE)));
    let result = interrupt_session(&fixture.state, "s").await;
    RESTORE_OVERRIDE.with(|restore| restore.set(None));
    let result = result.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(!result.note.contains("input box"), "{}", result.note);
    assert_eq!(fixture.keys_after_marker(), [ESC, CTRL_U].concat());
}

/// A box that cannot be found on the screen after the cancel is not an empty one: it may hold the
/// prompt the agent put back, so the watch ends reporting it rather than reporting nothing.
#[tokio::test]
async fn an_input_box_that_cannot_be_found_after_the_cancel_is_reported() {
    let fixture = working("claude-box-unreadable", AgentId::CLAUDE);
    draw(&fixture.live, CLEARED);
    let live = fixture.live();
    let back = back_at_prompt(&fixture.state);
    let agent = respond(
        &fixture,
        vec![(
            ESC,
            Box::new(move || {
                draw(&live, "nothing here is an input box");
                live.on_output(CLAUDE_AT_PROMPT_TITLE);
                back();
            }),
        )],
    );

    // Nothing ever shows a box, so the watch only has to run out; a short one shows that.
    RESTORE_OVERRIDE.with(|restore| restore.set(Some(Duration::from_millis(300))));
    let result = interrupt_session(&fixture.state, "s").await;
    RESTORE_OVERRIDE.with(|restore| restore.set(None));
    let result = result.unwrap();
    agent.join().unwrap();

    assert!(result.interrupted, "{result:?}");
    assert!(
        result
            .note
            .contains("Claude Code's input box still holds text")
            && result.note.contains("could not be cleared"),
        "{}",
        result.note
    );
    assert_eq!(fixture.keys_after_marker(), ESC);
}

/// Grok's own queued message starts a turn right after the cancel, and its spinner keeps running,
/// so the title never goes back to the prompt: the box, empty before the key, is neither waited for
/// nor watched, and nothing is cleared in the agent's next turn.
#[tokio::test]
async fn a_turn_the_agent_starts_after_the_cancel_ends_the_wait_for_its_title() {
    let fixture = working("grok-next-turn", AgentId::GROK);
    fixture.state.turn_started("s", None);
    draw(&fixture.live, GROK_WORKING);
    let live = fixture.live();
    let state = fixture.state.clone();
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(|| {})),
            (
                CTRL_C,
                Box::new(move || {
                    // `StopCancelled`, then the queued message's turn, its spinner still turning.
                    state.abandon_turn("s");
                    state.turn_started("s", Some("the queued message"));
                    live.on_output(running_title(AgentId::GROK));
                    draw(&live, GROK_WORKING);
                }),
            ),
        ],
    );

    // Far longer than the call is let take, so a wait for the title shows as a long one.
    SETTLE_OVERRIDE.with(|settle| settle.set(Some(PATIENCE * 4)));
    let began = Instant::now();
    let result = interrupt_session(&fixture.state, "s").await;
    let waited = began.elapsed();
    SETTLE_OVERRIDE.with(|settle| settle.set(None));
    let result = result.unwrap();
    agent.join().unwrap();

    assert!(waited < PATIENCE, "{waited:?}");
    assert_eq!(result.status, SessionStatus::Working);
    // Its composer was empty, so nothing is reported either.
    assert!(!result.note.contains("input box"), "{}", result.note);
    assert_eq!(fixture.keys_after_marker(), [CTRL_U, CTRL_C].concat());
}

/// A prompt restored while the title is back at the prompt, and a turn of the agent's own starting
/// before the first Ctrl-U: nothing is pressed into that turn, and the box is reported as left.
#[tokio::test]
async fn a_restored_prompt_is_not_cleared_once_a_new_turn_has_started() {
    let fixture = working("claude-new-turn-before-clearing", AgentId::CLAUDE);
    fixture.state.turn_started("s", None);
    let live = fixture.live();
    let before = BeforeKey {
        stops: live.title_activity().stops(),
        ends: fixture.state.turn_ends("s"),
        prompts: live.prompts_sent(),
    };
    live.on_output(CLAUDE_AT_PROMPT_TITLE);
    draw(&live, RESTORED);
    // The agent's next turn has begun by the time the box is read, before the first press.
    live.on_output(running_title(AgentId::CLAUDE));
    let mut writes = live.input_writes();

    let restoring = clear_restored_prompt(
        &fixture.state,
        &live,
        &mut writes,
        Duration::from_millis(200),
        Duration::from_millis(200),
        before,
    );

    assert!(
        matches!(restoring, Restoring::Done(Restored::Left)),
        "the box is reported as left"
    );
    assert_eq!(fixture.keys_after_marker(), b"");
}

/// The same with the title still at the prompt: the agent's next turn is told by its hooks alone,
/// the status being `Working` again after the end of the cancelled one.
#[tokio::test]
async fn a_restored_prompt_is_not_cleared_once_the_hooks_report_a_new_turn() {
    let fixture = working("claude-new-turn-by-hooks", AgentId::CLAUDE);
    fixture.state.turn_started("s", None);
    let live = fixture.live();
    let before = BeforeKey {
        stops: live.title_activity().stops(),
        ends: fixture.state.turn_ends("s"),
        prompts: live.prompts_sent(),
    };
    live.on_output(CLAUDE_AT_PROMPT_TITLE);
    draw(&live, RESTORED);
    fixture.state.abandon_turn("s");
    fixture.state.turn_started("s", Some("the next prompt"));
    let mut writes = live.input_writes();

    let restoring = clear_restored_prompt(
        &fixture.state,
        &live,
        &mut writes,
        Duration::from_millis(200),
        Duration::from_millis(200),
        before,
    );

    assert!(
        matches!(restoring, Restoring::Done(Restored::Left)),
        "the press is refused, so the box is reported as left"
    );
    assert_eq!(fixture.keys_after_marker(), b"");
}

/// Interrupts a Claude Code session whose queued message is taken as the Esc lands: the box shows
/// `before` when the key goes in, and the stand-in draws `after` as its screen without the title
/// turning, reports the message's prompt as the hook does (it opens no new turn, as the cancelled
/// one is still open, and sets `Working` at once, so no end is counted and the status stays), and
/// only then turns the title to the prompt, which is what shows the Esc has landed. Gives the
/// answer and the fixture.
async fn interrupt_claude_with_a_prompt_after_the_esc(
    name: &str,
    before: &'static str,
    after: &'static str,
) -> (Interrupted, Fixture) {
    let fixture = working(name, AgentId::CLAUDE);
    fixture.state.turn_started("s", None);
    draw(&fixture.live, before);
    let live = fixture.live();
    let state = fixture.state.clone();
    let agent = respond(
        &fixture,
        vec![(
            ESC,
            Box::new(move || {
                draw(&live, after);
                // `UserPromptSubmit`, as the hook applies it: no `Idle` comes of it.
                state.end_record_watch("s");
                state.turn_started("s", Some("the queued message"));
                state
                    .apply_hook_status("s", SessionStatus::Working)
                    .unwrap();
                live.note_prompt_sent(Some("the queued message"));
                live.on_output(CLAUDE_AT_PROMPT_TITLE);
            }),
        )],
    );

    let began = Instant::now();
    let result = interrupt_session(&fixture.state, "s").await.unwrap();
    let waited = began.elapsed();
    agent.join().unwrap();

    assert!(waited < PATIENCE, "{waited:?}");
    assert!(result.interrupted, "{result:?}");
    assert_eq!(result.status, SessionStatus::Working);
    assert_eq!(fixture.keys_after_marker(), ESC);
    (result, fixture)
}

/// A prompt from the input box (a message queued by the user, or one Octoboard wrote before the
/// pause) reported after the Esc ends the wait once the title has shown the cancel. The text in
/// the box is not cleared, and is reported, as it cannot be told from the cancelled prompt; the
/// title turns only after the prompt, so no Ctrl-U can be pressed before it.
#[tokio::test]
async fn a_prompt_reported_after_the_esc_ends_the_turn_and_keeps_the_box_from_being_cleared() {
    let (result, _fixture) =
        interrupt_claude_with_a_prompt_after_the_esc("claude-prompt-after-esc", CLEARED, RESTORED)
            .await;

    assert!(
        result
            .note
            .contains("Claude Code's input box still holds text")
            && result.note.contains("could not be cleared"),
        "{}",
        result.note
    );
}

/// The same with the box empty once the next turn has started: nothing is left to report.
#[tokio::test]
async fn an_empty_box_after_the_next_turn_has_started_is_not_reported() {
    let (result, _fixture) =
        interrupt_claude_with_a_prompt_after_the_esc("claude-prompt-empty-box", CLEARED, CLEARED)
            .await;

    assert!(!result.note.contains("input box"), "{}", result.note);
}

/// A box that held text before the key is reported once a prompt follows the Esc, whatever the
/// screen shows after it, since the text may be the user's own.
#[tokio::test]
async fn a_box_that_held_text_before_the_key_is_reported_once_a_prompt_follows_the_esc() {
    let (result, _fixture) =
        interrupt_claude_with_a_prompt_after_the_esc("claude-text-before-esc", RESTORED, RESTORED)
            .await;

    assert!(
        result
            .note
            .contains("Claude Code's input box still holds text")
            && result.note.contains("could not be cleared"),
        "{}",
        result.note
    );
}

/// A box that cannot be found once the next turn has started may hold the prompt, so it is reported.
#[tokio::test]
async fn a_box_that_cannot_be_found_after_the_next_turn_has_started_is_reported() {
    let (result, _fixture) = interrupt_claude_with_a_prompt_after_the_esc(
        "claude-prompt-no-box",
        CLEARED,
        "nothing here is an input box",
    )
    .await;

    assert!(
        result
            .note
            .contains("Claude Code's input box still holds text")
            && result.note.contains("could not be cleared"),
        "{}",
        result.note
    );
}

/// A prompt reported after the key counts as the turn's end only for Claude Code. Grok Build's hook
/// for a message queued mid-turn is not measured to fire only once the turn is over, and its cancel
/// is told by `StopCancelled`, or by its title when no hook fires. So with the title stopped and a
/// prompt reported but no `StopCancelled`, the session is still working when the window runs out.
/// The stand-in's step must have landed inside the window, or the test proves nothing.
#[tokio::test]
async fn a_prompt_after_the_key_does_not_by_itself_end_the_wait_for_grok() {
    let fixture = working("grok-prompt-no-cancel", AgentId::GROK);
    fixture.state.turn_started("s", None);
    draw(&fixture.live, GROK_WORKING);
    let live = fixture.live();
    let stepped = Arc::new(AtomicBool::new(false));
    let before_prompts = live.prompts_sent();
    let before_stops = live.title_activity().stops();
    let agent = respond(
        &fixture,
        vec![
            (CTRL_U, Box::new(|| {})),
            (CTRL_C, {
                let live = live.clone();
                let stepped = stepped.clone();
                Box::new(move || {
                    live.on_output(GROK_AT_PROMPT_TITLE);
                    live.note_prompt_sent(Some("the queued message"));
                    stepped.store(true, Ordering::SeqCst);
                })
            }),
        ],
    );

    // No cancel hook ever comes, so the window only has to run out; a short one shows that, but
    // one that a loaded machine still lands the step inside.
    SETTLE_OVERRIDE.with(|settle| settle.set(Some(Duration::from_millis(1500))));
    let result = interrupt_session(&fixture.state, "s").await;
    SETTLE_OVERRIDE.with(|settle| settle.set(None));
    let result = result.unwrap();
    let landed = stepped.load(Ordering::SeqCst);
    agent.join().unwrap();

    assert!(landed, "the stand-in's step came after the window ran out");
    assert_ne!(live.prompts_sent(), before_prompts);
    assert_ne!(live.title_activity().stops(), before_stops);
    assert!(!result.interrupted, "{result:?}");
    assert_eq!(result.status, SessionStatus::Working);
    assert!(
        result
            .note
            .contains("The interrupt key was sent, but the session still reads as working"),
        "{}",
        result.note
    );
}

/// A turn that ends between Grok's Ctrl-U and its Ctrl-C gets no Ctrl-C, and the caller is told
/// what did go in.
#[tokio::test]
async fn grok_s_ctrl_c_is_held_once_the_turn_has_ended() {
    let fixture = working("grok-ended-between", AgentId::GROK);
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
    let fixture = working("grok-touched", AgentId::GROK);
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
    let fixture = working("codex-still-working", AgentId::CODEX);

    // Nothing answers the key, so the window only has to run out; a short one shows that.
    SETTLE_OVERRIDE.with(|settle| settle.set(Some(Duration::from_millis(200))));
    let result = interrupt_session(&fixture.state, "s").await;
    SETTLE_OVERRIDE.with(|settle| settle.set(None));
    let result = result.unwrap();

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
    let fixture = fixture("idle", AgentId::CODEX, SessionStatus::Idle, false);
    fixture.live.on_output(running_title(AgentId::CODEX));

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
        AgentId::CLAUDE,
        SessionStatus::Working,
        false,
    );
    fixture.live.on_output(running_title(AgentId::CLAUDE));
    fixture.live.on_output(CLAUDE_AT_PROMPT_TITLE);

    let result = interrupt_session(&fixture.state, "s").await.unwrap();

    assert!(!result.interrupted);
    assert_eq!(result.note, NOT_SHOWN_RUNNING);
    assert_eq!(fixture.keys_after_marker(), b"");
}

#[tokio::test]
async fn a_second_interrupt_while_one_is_under_way_sends_nothing() {
    let fixture = working("claimed", AgentId::CODEX);
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
        AgentId,
        SessionStatus,
        bool,
        Setup,
        (crate::protocol::Code, &'static str),
    );
    let cases: [Case; 11] = [
        (
            "archived",
            AgentId::CLAUDE,
            SessionStatus::Archived,
            false,
            |_| None,
            not_running,
        ),
        (
            "interrupted",
            AgentId::CLAUDE,
            SessionStatus::Interrupted,
            false,
            |_| None,
            not_running,
        ),
        (
            "starting",
            AgentId::CLAUDE,
            SessionStatus::Working,
            true,
            |_| None,
            starting,
        ),
        (
            "trust",
            AgentId::CLAUDE,
            SessionStatus::Working,
            true,
            |f| {
                f.live
                    .on_output(include_bytes!("../../../testdata/claude_trust_screen.bin"));
                None
            },
            trust,
        ),
        (
            "dialog",
            AgentId::CLAUDE,
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
            AgentId::CODEX,
            SessionStatus::Working,
            false,
            |f| {
                f.live
                    .on_output(include_bytes!("../../../testdata/codex_approval_title.bin"));
                None
            },
            waiting,
        ),
        (
            "hook-hand",
            AgentId::CLAUDE,
            SessionStatus::WaitingUser,
            false,
            |_| None,
            waiting,
        ),
        (
            "request-hand",
            AgentId::CLAUDE,
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
            AgentId::CLAUDE,
            SessionStatus::Working,
            false,
            |f| Some(Box::new(f.state.begin_switch("s").unwrap())),
            busy,
        ),
        (
            "handoff",
            AgentId::CLAUDE,
            SessionStatus::Working,
            false,
            |f| Some(Box::new(f.state.hold_outbox("s"))),
            busy,
        ),
        (
            "sharing-change",
            AgentId::CLAUDE,
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

/// The copy for a session waiting on the user names the title its own provider shows when only the
/// user can act, and no other provider's.
#[test]
fn the_waiting_for_user_copy_names_the_title_the_sessions_provider_shows() {
    assert_eq!(
        waiting_for_user(AgentId::CODEX),
        "No key was sent: the session's agent is showing something only the user can answer \
         (a dialog of its own, or Codex's `Action Required`), which a key would answer in their \
         place. It is theirs to clear in the terminal."
    );
    for agent in [AgentId::CLAUDE, AgentId::GROK, AgentId::TEST] {
        assert_eq!(
            waiting_for_user(agent),
            "No key was sent: the session's agent is showing something only the user can answer \
             (a dialog of its own), which a key would answer in their place. It is theirs to \
             clear in the terminal.",
            "{agent:?}"
        );
    }
}
