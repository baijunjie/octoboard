//! Clearing a draft out of the way of a message past the draft's grace, through the session's
//! writes: the box is cleared a press at a time, seen empty off a whole frame, and only then is the
//! message written; where that cannot be seen, nothing of the message is written.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::{notice_code, AgentId, Event};
use crate::session::input_box_clear::PRESS_OVERRIDE;
use crate::session::{BoxIs, LiveSession};
use crate::test_support::{fake_live_session, ScratchDir, StandIn, PATIENCE};

const CTRL_U: u8 = 0x15;

/// The screen of `agent` with `lines` in its input box, one row a line.
fn draw(agent: AgentId, lines: &[&str]) -> Vec<u8> {
    let rule = "─".repeat(60);
    let screen = match agent {
        AgentId::CLAUDE => {
            let mut rows = vec![rule.clone()];
            for (at, line) in lines.iter().enumerate() {
                rows.push(format!("{} {line}", if at == 0 { '❯' } else { ' ' }));
            }
            if lines.is_empty() {
                rows.push("❯ ".to_string());
            }
            rows.push(rule);
            rows.join("\r\n")
        }
        AgentId::GROK => {
            let mut rows = vec![format!("╭{rule}╮")];
            let first = lines.first().copied().unwrap_or("");
            rows.push(format!("│ ❯ {first:50} │"));
            for line in lines.iter().skip(1) {
                rows.push(format!("│   {line:50} │"));
            }
            rows.push(format!("╰{rule}╯"));
            rows.join("\r\n")
        }
        AgentId::CODEX => {
            // As Codex 0.167.1 draws it: no border, a blank row, then its status line and hints.
            let mut rows = Vec::new();
            for (at, line) in lines.iter().enumerate() {
                rows.push(format!("{} {line}", if at == 0 { '›' } else { ' ' }));
            }
            if lines.is_empty() {
                rows.push("› ".to_string());
            }
            rows.push(String::new());
            rows.push("  gpt-5 default · ~/project".to_string());
            rows.push("  ? for shortcuts".to_string());
            rows.join("\r\n")
        }
        other => panic!("no screen for {other}"),
    };
    format!("\x1b[2J\x1b[30;1H{screen}").into_bytes()
}

/// A stand-in session of `agent` that keeps every byte written into its input in a file, so a test
/// can tell exactly what went in and in what order, with the app state that hears its notices.
struct Fixture {
    state: Arc<crate::state::AppState>,
    live: StandIn,
    keys: PathBuf,
    /// How many markers [`Self::keys`] has written.
    markers: std::cell::Cell<usize>,
    _dir: ScratchDir,
}

impl Fixture {
    fn new(name: &str, agent: AgentId) -> Self {
        let (state, dir) = crate::test_support::app_state(&format!("draft-clear-{name}"));
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        state
            .store
            .insert_session(&crate::protocol::Session {
                agent,
                ..crate::test_support::bare_session("s", crate::protocol::SessionKind::Project)
            })
            .unwrap();
        let ready = dir.join("ready");
        let keys = dir.join("keys");
        let script = format!(
            "stty raw -echo && touch '{}' && exec cat > '{}'",
            ready.display(),
            keys.display()
        );
        let live = fake_live_session("s", agent, 120, 40, &script).with_composer();
        let deadline = Instant::now() + PATIENCE;
        while !ready.exists() {
            assert!(Instant::now() < deadline, "the stand-in did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        state.register_live(live.clone());
        Self {
            state,
            live,
            keys,
            markers: std::cell::Cell::new(0),
            _dir: dir,
        }
    }

    fn live(&self) -> Arc<LiveSession> {
        (*self.live).clone()
    }

    /// What was written into the session's input, markers left out, once a marker written after
    /// the call has arrived, so an absence asserted in it is one of a reader that has caught up.
    /// Each call writes a marker of its own: one left by an earlier call, at the end of what has
    /// arrived so far, says nothing of what was written since.
    fn keys(&self) -> Vec<u8> {
        let marker = format!("<marker {}>", self.markers.get() + 1).into_bytes();
        self.markers.set(self.markers.get() + 1);
        self.live.write_input(&marker).unwrap();
        let deadline = Instant::now() + PATIENCE;
        loop {
            let keys = std::fs::read(&self.keys).unwrap_or_default();
            if keys.ends_with(&marker) || Instant::now() >= deadline {
                assert!(keys.ends_with(&marker), "{keys:?}");
                let mut keys = keys[..keys.len() - marker.len()].to_vec();
                for earlier in 1..self.markers.get() {
                    let earlier = format!("<marker {earlier}>").into_bytes();
                    if let Some(at) = keys
                        .windows(earlier.len())
                        .position(|window| window == earlier.as_slice())
                    {
                        keys.drain(at..at + earlier.len());
                    }
                }
                return keys;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The draft typed into the box and drawn there, settled as if typed well before.
    fn type_draft(&self, agent: AgentId, lines: &[&str]) {
        self.live.on_output(&draw(agent, &[]));
        self.live
            .write_terminal_input(lines.join("\n").as_bytes())
            .unwrap();
        self.live.on_output(&draw(agent, lines));
        self.live.settle_input();
    }
}

/// The params of the `draft_discarded` notices sent so far, each about the session the fixture
/// runs. It drains every pending event, not only the ones it returns: a test that needs more than
/// one kind of notice collects all events once and filters that list.
fn discarded_params(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
) -> Vec<std::collections::HashMap<String, String>> {
    let mut notices = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::SessionNotice {
            session,
            code,
            params,
            ..
        } = event
        {
            if code == notice_code::DRAFT_DISCARDED {
                assert_eq!(session, "s", "the notice is about the session cleared");
                notices.push(params.into_iter().collect());
            }
        }
    }
    notices
}

/// The `draft_discarded` notices sent so far, as their `agent` and `detail`, each about the
/// session the fixture runs.
fn discarded(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<(String, String)> {
    discarded_params(events)
        .into_iter()
        .map(|params| (params["agent"].clone(), params["detail"].clone()))
        .collect()
}

/// Draws `screens` one after another, each once the session has taken one more write than when
/// the one before was drawn, and fails when a write comes before the screen it should wait for:
/// each press is written only once the one before shows.
fn answer_presses(live: Arc<LiveSession>, screens: Vec<Vec<u8>>) -> std::thread::JoinHandle<()> {
    let before = live.input_writes();
    std::thread::spawn(move || {
        for (at, screen) in screens.into_iter().enumerate() {
            let deadline = Instant::now() + PATIENCE;
            while live.input_writes() == before + at as u64 {
                assert!(Instant::now() < deadline, "press {} did not come", at + 1);
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(
                live.input_writes(),
                before + at as u64 + 1,
                "a key went in before the screen showed the press before it"
            );
            live.on_output(&screen);
        }
    })
}

/// Past the grace, with no stash to take it, a draft in the box (here in Chinese) is cleared with
/// one Ctrl-U, the box seen empty, and only then is the message written: no write puts the message
/// in a box that holds the draft. Every client is told what the draft held.
#[tokio::test(flavor = "multi_thread")]
async fn past_the_grace_a_draft_is_cleared_before_the_message_and_handed_to_the_user() {
    const DRAFT: &str = "远程的菜单栏图标中";
    for (name, agent) in [("claude", AgentId::CLAUDE), ("grok", AgentId::GROK)] {
        let fixture = Fixture::new(name, agent);
        let live = fixture.live();
        let mut events = fixture.state.subscribe();
        fixture.type_draft(agent, &[DRAFT]);
        let screen = answer_presses(live.clone(), vec![draw(agent, &[])]);
        let framed = crate::session::term::frame_message(agent, "peer");
        live.write_message_input(&framed, "peer", true).unwrap();
        screen.join().unwrap();

        let keys = fixture.keys();
        let has_message = |keys: &[u8]| keys.windows(4).any(|window| window == b"peer");
        let press = keys
            .iter()
            .position(|&key| key == CTRL_U)
            .expect("a press cleared the box");
        assert!(!has_message(&keys[..press]), "{agent:?}: {keys:?}");
        let rest = &keys[press + 1..];
        assert!(
            has_message(rest) && !rest.contains(&CTRL_U),
            "{agent:?}: the message alone after the one press: {rest:?}"
        );
        assert_eq!(
            discarded(&mut events),
            [(agent.label().to_string(), DRAFT.to_string())],
            "{agent:?}"
        );
    }
}

/// A draft over several lines is cleared a press at a time, each press seen on the screen before
/// the next, never sent blind.
#[tokio::test(flavor = "multi_thread")]
async fn a_draft_over_several_lines_is_cleared_a_press_at_a_time() {
    let fixture = Fixture::new("lines", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["first line", "second line"]);
    // As Claude Code takes Ctrl-U: a line's text a press, then the line break before it.
    let screen = answer_presses(
        live.clone(),
        vec![
            draw(AgentId::CLAUDE, &["first line", ""]),
            draw(AgentId::CLAUDE, &["first line"]),
            draw(AgentId::CLAUDE, &[]),
        ],
    );
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    screen.join().unwrap();
    let keys = fixture.keys();
    assert_eq!(keys.iter().filter(|&&key| key == CTRL_U).count(), 3);
    let notices = discarded_params(&mut events);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0]["agent"], "Claude Code");
    assert_eq!(notices[0]["detail"], "first line\nsecond line");
    assert!(
        !notices[0].contains_key("cleared"),
        "a box emptied whole carries no partial-clear mark: {:?}",
        notices[0]
    );
}

/// A press the screen never shows leaves the draft where it is as far as anyone can tell: nothing
/// of the message is written, and the write is refused as one to retry (`WouldBlock`, nothing
/// written), which the queue answers by keeping the message at its front. Nothing was removed, so
/// no notice goes out; and no key goes in again on the retries that follow until the box shows
/// something else or a prompt is reported.
#[tokio::test(flavor = "multi_thread")]
async fn a_clear_not_seen_writes_nothing_and_presses_no_more_until_the_box_changes() {
    let fixture = Fixture::new("unseen", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    for _ in 0..3 {
        let error = live.write_message_input(&framed, "peer", true).unwrap_err();
        assert_eq!(error.written, 0);
    }
    assert_eq!(
        fixture.keys(),
        b"draft\x15",
        "one press, none again, and nothing of the message"
    );
    assert!(discarded(&mut events).is_empty(), "nothing was removed");

    // The box shows something else: the clear is tried again, a press at a time.
    live.on_output(&draw(AgentId::CLAUDE, &["draft and more"]));
    live.settle_input();
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    assert_eq!(error.written, 0);
    let keys = fixture.keys();
    assert_eq!(
        keys.iter().filter(|&&key| key == CTRL_U).count(),
        2,
        "{keys:?}"
    );
    assert!(!keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
}

/// A press nobody saw after one that showed holds the clear all the same: part of the draft was
/// removed and reported, and no key goes in again until the box shows something else.
#[tokio::test(flavor = "multi_thread")]
async fn a_press_not_seen_after_one_that_showed_presses_no_more_until_the_box_changes() {
    let fixture = Fixture::new("unseen-later", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["first line", "second line"]);
    // The first press shows; the second is never drawn.
    let screen = answer_presses(
        live.clone(),
        vec![draw(AgentId::CLAUDE, &["first line", ""])],
    );
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    screen.join().unwrap();
    assert_eq!(error.written, 0);
    assert_eq!(
        discarded(&mut events),
        [(
            "Claude Code".to_string(),
            "first line\nsecond line".to_string()
        )],
        "the part removed is reported"
    );
    live.settle_input();
    for _ in 0..3 {
        assert_eq!(
            live.write_message_input(&framed, "peer", true)
                .unwrap_err()
                .written,
            0
        );
    }
    let keys = fixture.keys();
    assert_eq!(
        keys.iter().filter(|&&key| key == CTRL_U).count(),
        2,
        "the shown press and the unseen one, none after: {keys:?}"
    );

    // The box shows something else: the clear is tried again.
    live.on_output(&draw(AgentId::CLAUDE, &["first line"]));
    live.settle_input();
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    assert_eq!(error.written, 0);
    let keys = fixture.keys();
    assert_eq!(
        keys.iter().filter(|&&key| key == CTRL_U).count(),
        3,
        "{keys:?}"
    );
    assert!(!keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
}

/// A press nobody saw holds the clear back only for a while: a turn the agent ends, or 30 seconds,
/// lets one more press go in, though the box looks the same.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_clear_is_let_go_by_a_turn_end_or_after_a_while() {
    let fixture = Fixture::new("unseen-release", AgentId::CLAUDE);
    let live = fixture.live();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let presses = || {
        let keys = fixture.keys();
        keys.iter().filter(|&&key| key == CTRL_U).count()
    };
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    assert_eq!(presses(), 1, "held after the press nobody saw");

    fixture.state.note_turn_end("s");
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    assert_eq!(presses(), 2, "a turn end lets one more press go in");
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    assert_eq!(presses(), 2, "and holds again after it");

    {
        let mut unseen = live.unseen_clear.lock().unwrap();
        let held = unseen.as_mut().expect("a held clear");
        held.at = held
            .at
            .checked_sub(super::UNSEEN_CLEAR_HOLD)
            .expect("a clock that reaches back that far");
    }
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    assert_eq!(presses(), 3, "the hold runs out");
}

/// Moves the start of the wait of the message known as `key` (its text, for a write made
/// directly) on the box back by the whole wait, as if it had begun that long ago.
fn age_unclearable_wait(live: &LiveSession, key: &str) {
    let mut since = live.unclearable_since.lock().unwrap();
    let (waiting, at, _) = since.as_mut().expect("a message waiting on the box");
    assert_eq!(waiting, key, "the wait is the message's own");
    *at = at
        .checked_sub(super::UNCLEARABLE_DRAFT_WAIT)
        .expect("a clock that reaches back that far");
}

/// The `message_sent_with_draft` notices sent so far, as their params, each about the session the
/// fixture runs. It drains every pending event, not only the ones it returns: a test that needs
/// more than one kind of notice collects all events once and filters that list.
fn sent_with_draft(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
) -> Vec<std::collections::HashMap<String, String>> {
    let mut notices = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::SessionNotice {
            session,
            code,
            params,
            ..
        } = event
        {
            if code == notice_code::MESSAGE_SENT_WITH_DRAFT {
                assert_eq!(session, "s", "the notice is about the session written into");
                notices.push(params.into_iter().collect());
            }
        }
    }
    notices
}

/// The message Octoboard wrote last, alone in the box and left unsent, holds the next message only
/// for the wait on a box no key may clear: then the next goes in after it, to be sent with it, and
/// every client is told the box held the message written before, with no draft to hand back.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_left_alone_in_the_box_holds_the_next_only_for_a_while() {
    let fixture = Fixture::new("own-bound", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.on_output(&draw(AgentId::CLAUDE, &["peer"]));
    live.settle_input();
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    for _ in 0..3 {
        let error = live.write_after_clearing(&second, "second").unwrap_err();
        assert_eq!(error.written, 0);
    }
    assert!(
        sent_with_draft(&mut events).is_empty(),
        "not before the wait is over"
    );
    assert!(!fixture.keys().windows(6).any(|window| window == b"second"));

    age_unclearable_wait(&live, "second");
    live.write_after_clearing(&second, "second").unwrap();
    let keys = fixture.keys();
    assert!(
        !keys.contains(&CTRL_U),
        "the message written before is never cleared: {keys:?}"
    );
    assert!(
        keys.windows(6).any(|window| window == b"second"),
        "{keys:?}"
    );
    let notices = sent_with_draft(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["agent"], "Claude Code");
    assert_eq!(notices[0].get("held").map(String::as_str), Some("message"));
    assert!(!notices[0].contains_key("detail"), "{:?}", notices[0]);
    assert!(
        live.unclearable_since.lock().unwrap().is_none(),
        "the wait ends with the write"
    );
}

/// A press nobody saw holds the message only for the wait on a box no key may clear: until then
/// nothing of it is written on any retry, and once it is over the message goes into the box as the
/// box stands, with no further press, and every client is handed what the box showed.
#[tokio::test(flavor = "multi_thread")]
async fn past_the_wait_a_draft_no_press_clears_goes_in_with_the_message() {
    let fixture = Fixture::new("unclearable-bound", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    for _ in 0..3 {
        let error = live.write_message_input(&framed, "peer", true).unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    }
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    assert_eq!(
        fixture.keys(),
        b"draft\x15",
        "one press and nothing of the message"
    );
    assert!(sent_with_draft(&mut events).is_empty());

    age_unclearable_wait(&live, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    let keys = fixture.keys();
    assert!(keys.starts_with(b"draft\x15"), "{keys:?}");
    let rest = &keys[b"draft\x15".len()..];
    assert!(
        !rest.contains(&CTRL_U) && rest.windows(4).any(|window| window == b"peer"),
        "the message alone, with no further press: {rest:?}"
    );
    let notices = sent_with_draft(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["agent"], "Claude Code");
    assert_eq!(notices[0]["detail"], "draft");
    assert!(!notices[0].contains_key("held"), "{:?}", notices[0]);
    assert!(
        !live.draft.lock().unwrap().holds(),
        "the text went in with the message: later messages do not wait on it"
    );
}

/// The wait run out still never puts a message into a session showing a dialog: it waits for the
/// dialog to go, and goes in then.
#[tokio::test(flavor = "multi_thread")]
async fn past_the_wait_a_dialog_still_holds_the_message() {
    let fixture = Fixture::new("unclearable-dialog", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    age_unclearable_wait(&live, "peer");

    live.on_output(
        b"\x1b[2J\x1b[HResume this conversation?\r\nEnter to confirm \xc2\xb7 Esc to dismiss\r\n",
    );
    for _ in 0..2 {
        let error = live.write_after_clearing(&framed, "peer").unwrap_err();
        assert_eq!(error.written, 0);
    }
    assert_eq!(
        fixture.keys(),
        b"draft\x15",
        "nothing more while the dialog is up"
    );
    assert!(sent_with_draft(&mut events).is_empty());

    live.on_output(&draw(AgentId::CLAUDE, &["draft"]));
    live.settle_input();
    live.write_after_clearing(&framed, "peer").unwrap();
    assert!(fixture.keys().windows(4).any(|window| window == b"peer"));
    assert_eq!(sent_with_draft(&mut events).len(), 1);
}

/// The box cleared and the message written, the keys typed before count as done with: the next
/// message goes in at once, within its grace, with no wait on them.
#[tokio::test(flavor = "multi_thread")]
async fn after_a_clear_the_next_message_does_not_wait_on_the_keys_typed_before() {
    let fixture = Fixture::new("after-clear", AgentId::CLAUDE);
    let live = fixture.live();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CLAUDE, &[])]);
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    screen.join().unwrap();
    assert!(!live.draft.lock().unwrap().holds());
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    live.write_message_input(&second, "second", false).unwrap();
    let keys = fixture.keys();
    assert!(
        keys.windows(6).any(|window| window == b"second"),
        "{keys:?}"
    );
    assert_eq!(keys.iter().filter(|&&key| key == CTRL_U).count(), 1);
}

/// A box that holds only the message Octoboard wrote last, left there unsent with nothing typed
/// since, holds no draft: it is never cleared, nor reported, and the next message waits.
#[tokio::test(flavor = "multi_thread")]
async fn the_message_written_last_is_never_cleared_as_a_draft() {
    let fixture = Fixture::new("own-message", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.on_output(&draw(AgentId::CLAUDE, &["peer"]));
    live.settle_input();
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    let error = live.write_after_clearing(&second, "second").unwrap_err();
    assert_eq!(error.written, 0);
    assert!(
        error.error.to_string().contains("only the message"),
        "{error}"
    );
    let keys = fixture.keys();
    assert!(!keys.contains(&CTRL_U), "{keys:?}");
    assert!(discarded(&mut events).is_empty());
}

/// A box the screen shows emptied only inside a frame still being drawn is not taken as cleared:
/// nothing of the message is written.
#[tokio::test(flavor = "multi_thread")]
async fn a_clear_not_confirmed_off_a_whole_frame_writes_nothing() {
    let fixture = Fixture::new("torn-clear", AgentId::CLAUDE);
    let live = fixture.live();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    let mut torn = b"\x1b[?2026h".to_vec();
    torn.extend(draw(AgentId::CLAUDE, &[]));
    let screen = answer_presses(live.clone(), vec![torn]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    screen.join().unwrap();
    assert_eq!(error.written, 0);
    assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(fixture.keys(), b"draft\x15");
}

/// Codex's composer is cleared as Claude Code's is: past the grace, a draft there is cleared with
/// Ctrl-U, the box seen empty, and only then is the message written, with what the draft held
/// handed to every client.
#[tokio::test(flavor = "multi_thread")]
async fn a_codex_draft_is_cleared_before_the_message() {
    let fixture = Fixture::new("codex", AgentId::CODEX);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CODEX, &["draft"]);
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CODEX, &[])]);
    let framed = crate::session::term::frame_message(AgentId::CODEX, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    screen.join().unwrap();
    let keys = fixture.keys();
    let press = keys
        .iter()
        .position(|&key| key == CTRL_U)
        .expect("a press cleared the box");
    assert!(
        !keys[..press].windows(4).any(|window| window == b"peer"),
        "{keys:?}"
    );
    assert!(
        keys[press + 1..].windows(4).any(|window| window == b"peer"),
        "{keys:?}"
    );
    assert_eq!(
        discarded(&mut events),
        [("Codex".to_string(), "draft".to_string())]
    );
}

/// An agent with no key measured to clear its box holds a message on a draft past the grace, with
/// no key pressed and nothing written, until the wait on a box no key may clear is over; then the
/// message goes in as the box stands.
#[tokio::test(flavor = "multi_thread")]
async fn a_draft_no_key_can_clear_holds_the_message_only_for_a_while() {
    let fixture = Fixture::new("no-key", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.write_terminal_input(b"draft").unwrap();
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    let before = live.input_writes();
    for _ in 0..2 {
        let error = live.write_after_clearing(&framed, "peer").unwrap_err();
        assert_eq!(error.written, 0);
    }
    assert_eq!(
        live.input_writes(),
        before,
        "no key and nothing of the message"
    );

    age_unclearable_wait(&live, "peer");
    live.write_after_clearing(&framed, "peer").unwrap();
    let keys = fixture.keys();
    assert!(!keys.contains(&CTRL_U), "{keys:?}");
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let notices = sent_with_draft(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        !notices[0].contains_key("detail"),
        "a box that cannot be read hands nothing back: {:?}",
        notices[0]
    );
}

/// Keys the user types while the box is being cleared end the clear: no further press goes in and
/// nothing of the message is written, so the message never joins what they typed. The press that
/// did go in is still reported, with the draft it was taken on.
#[tokio::test(flavor = "multi_thread")]
async fn typing_while_the_box_is_cleared_ends_the_clear_and_holds_the_message() {
    let fixture = Fixture::new("typing", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["first line", "second line"]);
    let before = live.input_writes();
    let typist = live.clone();
    let user = std::thread::spawn(move || {
        let deadline = Instant::now() + PATIENCE;
        while typist.input_writes() == before {
            assert!(Instant::now() < deadline, "no press came");
            std::thread::sleep(Duration::from_millis(2));
        }
        // The user types before the screen shows what the press did.
        typist.write_terminal_input(b"x").unwrap();
        typist.on_output(&draw(AgentId::CLAUDE, &["first line", ""]));
    });
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    user.join().unwrap();
    assert_eq!(error.written, 0);
    assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    let keys = fixture.keys();
    assert_eq!(
        keys.iter().filter(|&&key| key == CTRL_U).count(),
        1,
        "{keys:?}"
    );
    assert!(
        keys.ends_with(b"x"),
        "nothing went in after what the user typed: {keys:?}"
    );
    assert!(!keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let notices = discarded_params(&mut events);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0]["detail"], "first line\nsecond line");
    assert_eq!(
        notices[0].get("cleared").map(String::as_str),
        Some("part"),
        "the box was not emptied, so the notice says only part was cleared"
    );
}

/// A long paste the agent shows collapsed is told to the user as the placeholder it shows, which
/// is all the box holds of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_collapsed_paste_is_reported_as_its_placeholder() {
    const PLACEHOLDER: &str = "[Pasted text #1 +24 lines]";
    let fixture = Fixture::new("placeholder", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &[PLACEHOLDER]);
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CLAUDE, &[])]);
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    screen.join().unwrap();
    assert_eq!(
        discarded(&mut events),
        [("Claude Code".to_string(), PLACEHOLDER.to_string())]
    );
}

/// A box that holds no text when the clear comes to read it — the user sent or emptied the draft
/// meanwhile — gets the message at once, with no press and no notice.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_found_empty_is_written_into_without_a_press_or_a_notice() {
    let fixture = Fixture::new("empty", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    let keys = fixture.keys();
    assert!(!keys.contains(&CTRL_U), "{keys:?}");
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    assert!(discarded(&mut events).is_empty());
}

/// Where the screen shows a draft off a frame not drawn whole, no key clears it and the message
/// waits: the box may be empty already, and a press then would not show.
#[tokio::test(flavor = "multi_thread")]
async fn a_draft_read_off_a_torn_frame_is_not_cleared() {
    for (name, agent) in [
        ("torn-claude", AgentId::CLAUDE),
        ("torn-grok", AgentId::GROK),
    ] {
        let fixture = Fixture::new(name, agent);
        let live = fixture.live();
        let mut events = fixture.state.subscribe();
        fixture.type_draft(agent, &["draft"]);
        live.on_output(b"\x1b[?2026h");
        crate::session::draft_hold::WHOLE_FRAME_WAIT_OVERRIDE
            .with(|wait| wait.set(Some(Duration::ZERO)));
        let framed = crate::session::term::frame_message(agent, "peer");
        let error = live.write_message_input(&framed, "peer", true).unwrap_err();
        crate::session::draft_hold::WHOLE_FRAME_WAIT_OVERRIDE.with(|wait| wait.set(None));
        assert_eq!(error.written, 0, "{agent:?}");
        assert_eq!(
            error.error.kind(),
            std::io::ErrorKind::WouldBlock,
            "{agent:?}"
        );
        assert_eq!(fixture.keys(), b"draft", "{agent:?}: no press");
        assert!(discarded(&mut events).is_empty(), "{agent:?}");
    }
}

/// A clear that took out something but did not empty the box, its next press never shown, is
/// reported as a partial clear carrying the whole draft it was taken on, and the message is not
/// written.
#[tokio::test(flavor = "multi_thread")]
async fn a_clear_that_did_not_empty_the_box_is_reported_as_part() {
    let fixture = Fixture::new("part", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["first line", "second line"]);
    // The first press shows; the second is never drawn.
    let screen = answer_presses(
        live.clone(),
        vec![draw(AgentId::CLAUDE, &["first line", ""])],
    );
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    screen.join().unwrap();
    assert_eq!(error.written, 0);
    assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    let keys = fixture.keys();
    assert!(!keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let notices = discarded_params(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["agent"], "Claude Code");
    assert_eq!(notices[0]["detail"], "first line\nsecond line");
    assert_eq!(notices[0].get("cleared").map(String::as_str), Some("part"));
}

/// A press nobody saw holds the clear until the box shows something else or a prompt is reported:
/// a prompt reported lets the next try press again.
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_reported_ends_the_hold_a_press_not_seen_put_on_the_clear() {
    let fixture = Fixture::new("unseen-prompt", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    for _ in 0..2 {
        let error = live.write_message_input(&framed, "peer", true).unwrap_err();
        assert_eq!(error.written, 0);
    }
    let count = |keys: &[u8]| keys.iter().filter(|&&key| key == CTRL_U).count();
    assert_eq!(count(&fixture.keys()), 1, "held after the press not seen");
    live.note_prompt_sent(None);
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    assert_eq!(error.written, 0);
    let keys = fixture.keys();
    assert_eq!(
        count(&keys),
        2,
        "a prompt reported lets the clear press again: {keys:?}"
    );
    assert!(
        discarded(&mut events).is_empty(),
        "nothing was ever removed"
    );
}

/// What Octoboard wrote last is no draft only while nothing was typed since: keys the user typed
/// after it make the box theirs, and it is cleared as any draft is.
#[tokio::test(flavor = "multi_thread")]
async fn text_typed_after_the_message_written_last_is_cleared_as_a_draft() {
    let fixture = Fixture::new("typed-after-own", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.write_terminal_input(b" more").unwrap();
    live.on_output(&draw(AgentId::CLAUDE, &["peer more"]));
    live.settle_input();
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CLAUDE, &[])]);
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    live.write_after_clearing(&second, "second").unwrap();
    screen.join().unwrap();
    let keys = fixture.keys();
    assert_eq!(
        keys.iter().filter(|&&key| key == CTRL_U).count(),
        1,
        "{keys:?}"
    );
    assert!(
        keys.ends_with(b"second") || keys.windows(6).any(|w| w == b"second"),
        "{keys:?}"
    );
    assert_eq!(
        discarded(&mut events),
        [("Claude Code".to_string(), "peer more".to_string())]
    );
}

/// The one placeholder Claude Code shows for a long paste Octoboard wrote is the message, not a
/// draft, while nothing was typed since.
#[tokio::test(flavor = "multi_thread")]
async fn the_placeholder_of_the_message_written_last_is_never_cleared_as_a_draft() {
    let fixture = Fixture::new("own-placeholder", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "a very long message");
    live.write_message_input(&framed, "a very long message", false)
        .unwrap();
    live.on_output(&draw(AgentId::CLAUDE, &["[Pasted text #1 +24 lines]"]));
    live.settle_input();
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    let error = live.write_after_clearing(&second, "second").unwrap_err();
    assert_eq!(error.written, 0);
    let keys = fixture.keys();
    assert!(!keys.contains(&CTRL_U), "{keys:?}");
    assert!(discarded(&mut events).is_empty());
}

/// The write after the box was cleared refuses where anything else has written into the session
/// since the last press, with nothing of the message written and the typing record not told the
/// box is empty; with nothing else written it goes in, and the record is told.
#[tokio::test(flavor = "multi_thread")]
async fn the_write_after_a_clear_refuses_once_anything_else_has_written() {
    let fixture = Fixture::new("stale-clear", AgentId::CLAUDE);
    let live = fixture.live();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let cleared_at = live.input_writes();
    live.write_terminal_input(b"x").unwrap();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let error = live
        .write_message_into_box(&framed, "peer", true, BoxIs::Cleared(cleared_at))
        .unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    assert!(
        live.draft.lock().unwrap().holds(),
        "the keys typed after the last press are still held"
    );
    assert_eq!(fixture.keys(), b"x", "nothing of the message");

    // Nothing else written since the count it is told: the message goes in, the typed keys are
    // done with.
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let now = live.input_writes();
    live.write_message_into_box(&framed, "peer", true, BoxIs::Cleared(now))
        .unwrap();
    assert!(!live.draft.lock().unwrap().holds());
    let keys = fixture.keys();
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
}

/// The notices of `code` sent so far, as their params, each about the session the fixture runs.
fn notices_of(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    wanted: crate::protocol::Code,
) -> Vec<std::collections::HashMap<String, String>> {
    let mut notices = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::SessionNotice {
            session,
            code,
            params,
            ..
        } = event
        {
            if code == wanted {
                assert_eq!(session, "s");
                notices.push(params.into_iter().collect());
            }
        }
    }
    notices
}

/// The wait on a draft runs from the first attempt that found it, however much the clears take
/// out meanwhile: a clear that again removes only part of it once the wait is over has the message
/// go in as the box stands in that same attempt, and the draft as read before that clear handed
/// back in `message_sent_with_draft`, with no partial-clear notice, which would say the message
/// waits.
#[tokio::test(flavor = "multi_thread")]
async fn a_clear_that_keeps_taking_out_part_ends_in_the_message_going_in_at_the_bound() {
    let fixture = Fixture::new("partial-bound", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["first", "second"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CLAUDE, &["first", ""])]);
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    screen.join().unwrap();
    assert_eq!(
        discarded_params(&mut events).len(),
        1,
        "the first part is reported"
    );

    // The box shows something else, so the clear goes on, and takes out part again.
    live.on_output(&draw(AgentId::CLAUDE, &["first", "third"]));
    live.settle_input();
    age_unclearable_wait(&live, "peer");
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CLAUDE, &["first", ""])]);
    live.write_message_input(&framed, "peer", true).unwrap();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    screen.join().unwrap();
    let keys = fixture.keys();
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let all: Vec<Event> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let of = |wanted: crate::protocol::Code| -> Vec<std::collections::HashMap<String, String>> {
        all.iter()
            .filter_map(|event| match event {
                Event::SessionNotice { code, params, .. } if *code == wanted => {
                    Some(params.clone().into_iter().collect())
                }
                _ => None,
            })
            .collect()
    };
    let notices = of(notice_code::MESSAGE_SENT_WITH_DRAFT);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(
        notices[0]["detail"], "first\nthird",
        "the draft as the clear read it"
    );
    // The message went in: the draft is handed back whole with it, not in a partial-clear
    // notice, which says the message waits.
    assert!(of(notice_code::DRAFT_DISCARDED).is_empty());
}

/// The wait is kept under the message's identity, not its text: a second message of the same text
/// waits its own while, and the first's wait does not carry over to it.
#[tokio::test(flavor = "multi_thread")]
async fn messages_of_the_same_text_wait_apart() {
    let fixture = Fixture::new("identity", AgentId::TEST);
    let live = fixture.live();
    live.write_terminal_input(b"draft").unwrap();
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    // Through `write_after_clearing`, as a box no key clears takes it, under each identity.
    let attempt = |id: &str| {
        *live.writing.lock().unwrap() = Some(id.to_string());
        let result = live.write_after_clearing(&framed, "peer");
        *live.writing.lock().unwrap() = None;
        result
    };
    assert!(attempt("launch:1").is_err());
    age_unclearable_wait(&live, "launch:1");
    assert!(
        attempt("launch:2").is_err(),
        "the other message waits its own while"
    );
    assert!(live.writing.lock().unwrap().is_none());
    age_unclearable_wait(&live, "launch:2");
    attempt("launch:2").unwrap();
    assert!(fixture.keys().windows(4).any(|window| window == b"peer"));
    // The write that sets the identity forgets it once done, whatever came of it.
    let _ = live.write_message_input_as("launch:3", &framed, "peer", true);
    assert!(live.writing.lock().unwrap().is_none());
}

/// A message left unsent, with text typed after it, holds later messages only for the wait: then
/// the next goes into the box with both, and every client is told so and handed what the box held.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_left_unsent_with_typing_after_it_holds_the_next_only_for_a_while() {
    let fixture = Fixture::new("unsent-bound", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.mark_not_submitted(live.prompts_sent());
    live.write_terminal_input(b" more").unwrap();
    live.on_output(&draw(AgentId::CLAUDE, &["peer more"]));
    live.settle_input();
    assert!(live.unsent_waiting(), "held for the user");

    *live.unsent_waits_since.lock().unwrap() = Some(
        Instant::now()
            .checked_sub(super::UNCLEARABLE_DRAFT_WAIT)
            .expect("a clock that reaches back that far"),
    );
    assert!(!live.unsent_waiting(), "no longer once the wait is over");
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    live.write_message_input(&second, "second", false).unwrap();
    let keys = fixture.keys();
    assert!(!keys.contains(&CTRL_U), "the box is not cleared: {keys:?}");
    assert!(
        keys.windows(6).any(|window| window == b"second"),
        "{keys:?}"
    );
    let notices = notices_of(&mut events, notice_code::MESSAGE_SENT_WITH_DRAFT);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(
        notices[0].get("held").map(String::as_str),
        Some("message_and_draft")
    );
    assert_eq!(notices[0]["detail"], "peer more");
    assert!(
        live.unsent_at.lock().unwrap().is_none(),
        "the unsent mark is done with"
    );
}

/// A box that cannot be read after a key that may have brought text into it, as a history key
/// does, keeps the message waiting with no bound: a list may be hiding the box and would take the
/// message's keys. Once it has waited a while the user is told, once for that message.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_hidden_after_a_history_key_holds_the_message_and_tells_the_user_once() {
    let fixture = Fixture::new("hidden-box", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    live.write_terminal_input(b"\x1b[A").unwrap();
    live.on_output(b"\x1b[2J\x1b[H  a list of earlier prompts");
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    assert!(notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).is_empty());

    age_unclearable_wait(&live, "peer");
    for _ in 0..3 {
        assert_eq!(
            live.write_message_input(&framed, "peer", true)
                .unwrap_err()
                .written,
            0
        );
    }
    let notices = notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT);
    assert_eq!(notices.len(), 1, "told once: {notices:?}");
    assert_eq!(notices[0]["agent"], "Claude Code");
    assert!(!fixture.keys().windows(4).any(|window| window == b"peer"));
    assert!(notices_of(&mut events, notice_code::MESSAGE_SENT_WITH_DRAFT).is_empty());
}

/// Begins the wait of the message known as `key` on the box as if it had begun `by` ago.
fn begin_wait(live: &LiveSession, key: &str, by: Duration) {
    *live.unclearable_since.lock().unwrap() = Some((
        key.to_string(),
        Instant::now()
            .checked_sub(by)
            .expect("a clock that reaches back that far"),
        false,
    ));
}

/// Moves the start of the wait of the message known as `key` back by `by`.
fn backdate_wait(live: &LiveSession, key: &str, by: Duration) {
    let mut since = live.unclearable_since.lock().unwrap();
    let (waiting, at, _) = since.as_mut().expect("a message waiting on the box");
    assert_eq!(waiting, key, "the wait is the message's own");
    *at = at
        .checked_sub(by)
        .expect("a clock that reaches back that far");
}

/// A screen with no input box on it, as a list or another view leaves.
const NO_BOX: &[u8] = b"\x1b[2J\x1b[H  a list of earlier prompts";

/// Opens a synchronized update as of now, so that the screen reads as a frame not drawn whole
/// for the next second, whatever the test's own steps take.
fn tear(live: &LiveSession) {
    live.on_output(b"\x1b[?2026l\x1b[?2026h");
    live.settle_input();
}

/// What a draft in Chinese, nine characters, comes to in a notice and in the log.
const CJK_DRAFT: &str = "远程的菜单栏图标中";

/// The wait on a draft is 30 seconds, one for the message from the first attempt that found the
/// draft; attempts inside it write nothing and do not start it again, and the attempt after it
/// writes the message.
#[tokio::test(flavor = "multi_thread")]
async fn the_wait_is_thirty_seconds_from_the_first_attempt_that_found_the_draft() {
    assert_eq!(super::UNCLEARABLE_DRAFT_WAIT, Duration::from_secs(30));
    let fixture = Fixture::new("thirty", AgentId::TEST);
    let live = fixture.live();
    live.write_terminal_input(b"draft").unwrap();
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    let before = live.input_writes();
    assert!(live.write_after_clearing(&framed, "peer").is_err());
    backdate_wait(&live, "peer", super::UNCLEARABLE_DRAFT_WAIT / 2);
    for _ in 0..2 {
        let error = live.write_after_clearing(&framed, "peer").unwrap_err();
        assert_eq!(error.written, 0);
    }
    assert_eq!(live.input_writes(), before, "half the wait has passed");

    backdate_wait(&live, "peer", super::UNCLEARABLE_DRAFT_WAIT / 2);
    live.write_after_clearing(&framed, "peer").unwrap();
    assert_eq!(live.input_writes(), before + 1);
}

/// The clear is tried on every attempt, the wait run out or not: a draft the presses can remove
/// is cleared and the message goes in after it, with no `message_sent_with_draft`.
#[tokio::test(flavor = "multi_thread")]
async fn a_draft_the_presses_can_remove_is_cleared_even_once_the_wait_is_over() {
    let fixture = Fixture::new("clear-after-wait", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["first", "second"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    // The first press shows, the next is never drawn: the message waits.
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CLAUDE, &["first", ""])]);
    assert!(live.write_message_input(&framed, "peer", true).is_err());
    screen.join().unwrap();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    assert_eq!(discarded_params(&mut events).len(), 1);

    // The box now shows other rows, and the wait on it has run out; one press empties it.
    live.on_output(&draw(AgentId::CLAUDE, &["first"]));
    live.settle_input();
    backdate_wait(&live, "peer", super::UNCLEARABLE_DRAFT_WAIT);
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CLAUDE, &[])]);
    live.write_message_input(&framed, "peer", true).unwrap();
    screen.join().unwrap();
    let keys = fixture.keys();
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    assert_eq!(
        discarded(&mut events),
        [("Claude Code".to_string(), "first".to_string())]
    );
    assert!(sent_with_draft(&mut events).is_empty());
    assert!(live.unclearable_since.lock().unwrap().is_none());
}

/// A clear whose last press showed the box empty only inside a frame not drawn whole is not
/// confirmed: once the wait is over the message goes in as the box stands. The clear already
/// handed the draft back, in Chinese whole, in its `draft_discarded`, so `message_sent_with_draft`
/// carries only what the box showed, which was nothing it could read whole, and the draft is told
/// once.
#[tokio::test(flavor = "multi_thread")]
async fn a_clear_not_confirmed_once_the_wait_is_over_has_the_message_go_in_with_the_draft() {
    let fixture = Fixture::new("unconfirmed-bound", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &[CJK_DRAFT]);
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let mut torn = b"\x1b[?2026l\x1b[?2026h".to_vec();
    torn.extend(draw(AgentId::CLAUDE, &[]));
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let screen = answer_presses(live.clone(), vec![torn]);
    begin_wait(&live, "peer", super::UNCLEARABLE_DRAFT_WAIT);
    live.write_message_input(&framed, "peer", true).unwrap();
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    screen.join().unwrap();

    let keys = fixture.keys();
    let sent = |keys: &[u8]| keys.windows(4).any(|window| window == b"peer");
    let press = keys.iter().position(|&key| key == CTRL_U).expect("a press");
    assert!(!sent(&keys[..press]) && sent(&keys[press..]), "{keys:?}");
    let all: Vec<Event> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let of = |wanted: crate::protocol::Code| -> Vec<std::collections::HashMap<String, String>> {
        all.iter()
            .filter_map(|event| match event {
                Event::SessionNotice { code, params, .. } if *code == wanted => {
                    Some(params.clone().into_iter().collect())
                }
                _ => None,
            })
            .collect()
    };
    let discarded = of(notice_code::DRAFT_DISCARDED);
    assert_eq!(discarded.len(), 1, "{discarded:?}");
    assert_eq!(discarded[0]["detail"], CJK_DRAFT);
    assert!(!discarded[0].contains_key("cleared"), "{:?}", discarded[0]);
    let notices = of(notice_code::MESSAGE_SENT_WITH_DRAFT);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        !notices[0].contains_key("detail"),
        "the draft is not told twice: {:?}",
        notices[0]
    );
    assert!(!notices[0].contains_key("held"), "{:?}", notices[0]);
}

/// A box seen empty off a whole frame, with no text the typing record knows of, holds nothing the
/// message could go in with: it is written as into any empty box, with no notice, whatever the
/// wait or a clear read before.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_seen_empty_at_the_bound_is_written_into_with_no_notice() {
    for read_before in [None, Some("a draft that was cleared meanwhile")] {
        let fixture = Fixture::new("empty-at-bound", AgentId::CLAUDE);
        let live = fixture.live();
        let mut events = fixture.state.subscribe();
        live.on_output(&draw(AgentId::CLAUDE, &[]));
        live.settle_input();
        let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
        live.write_message_into_box(
            &framed,
            "peer",
            true,
            BoxIs::Unclearable {
                unsent: false,
                read_before,
            },
        )
        .unwrap();
        let keys = fixture.keys();
        assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
        assert!(sent_with_draft(&mut events).is_empty(), "{read_before:?}");
    }
}

/// The write at the bound still never goes in while a key sequence is incomplete, while the
/// session is frozen, or while it waits for the user's action: nothing of the message is
/// written, and no notice goes out.
#[tokio::test(flavor = "multi_thread")]
async fn at_the_bound_the_message_still_waits_for_what_a_write_must_not_cross() {
    let unclearable = BoxIs::Unclearable {
        unsent: false,
        read_before: None,
    };
    // An escape sequence not yet complete.
    let fixture = Fixture::new("bound-incomplete", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    live.write_terminal_input(b"\x1b[").unwrap();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let error = live
        .write_message_into_box(&framed, "peer", true, unclearable)
        .unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    assert!(!fixture.keys().windows(4).any(|window| window == b"peer"));
    assert!(sent_with_draft(&mut events).is_empty());

    // A frozen session.
    let fixture = Fixture::new("bound-frozen", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    live.pty.freeze_output();
    let before = live.input_writes();
    let error = live
        .write_message_into_box(&framed, "peer", true, unclearable)
        .unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(live.input_writes(), before);
    assert!(sent_with_draft(&mut events).is_empty());

    // Codex's "Action Required" title: the person is to answer in the terminal.
    let fixture = Fixture::new("bound-action", AgentId::CODEX);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CODEX, &["draft"]);
    assert!(live.observe_action_title(Some("[ ! ] Action Required")));
    let framed = crate::session::term::frame_message(AgentId::CODEX, "peer");
    let before = live.input_writes();
    let error = live
        .write_message_into_box(&framed, "peer", true, unclearable)
        .unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(live.input_writes(), before);
    assert!(sent_with_draft(&mut events).is_empty());
}

/// A box that cannot be read while the typing record knows of text typed into it (here in Chinese)
/// holds the message only for the wait: no key is pressed on an unreadable box, and once the wait is
/// over the message goes in as the box stands, the text counted as gone with it, and every client
/// is told, with no draft to hand back since none could be read.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_box_over_typed_text_holds_the_message_only_for_a_while() {
    let fixture = Fixture::new("unreadable-known", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &[CJK_DRAFT]);
    live.on_output(NO_BOX);
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    for _ in 0..2 {
        let error = live.write_message_input(&framed, "peer", true).unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    }
    assert_eq!(
        fixture.keys(),
        CJK_DRAFT.as_bytes(),
        "no key and nothing of the message"
    );
    assert!(sent_with_draft(&mut events).is_empty());
    assert!(notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).is_empty());

    age_unclearable_wait(&live, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    let keys = fixture.keys();
    assert!(!keys.contains(&CTRL_U), "{keys:?}");
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let notices = sent_with_draft(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["agent"], "Claude Code");
    assert!(
        !notices[0].contains_key("detail") && !notices[0].contains_key("held"),
        "{:?}",
        notices[0]
    );
    assert!(
        !live.draft.lock().unwrap().holds(),
        "the text went in with the message: later messages do not wait on it"
    );
    assert!(live.unclearable_since.lock().unwrap().is_none());
}

/// A box read empty off a frame not drawn whole, while the typing record knows of text typed into
/// it, holds the message for as long as the wait: the frame may hide that text. Past it the message
/// goes in as the box stands, and every client is told.
#[tokio::test(flavor = "multi_thread")]
async fn a_torn_empty_box_over_typed_text_holds_the_message_only_for_a_while() {
    let fixture = Fixture::new("torn-known", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &[CJK_DRAFT]);
    let mut screen = b"\x1b[?2026h".to_vec();
    screen.extend(draw(AgentId::CLAUDE, &[]));
    live.on_output(&screen);
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    for _ in 0..2 {
        tear(&live);
        let error = live.write_message_input(&framed, "peer", true).unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    }
    assert_eq!(fixture.keys(), CJK_DRAFT.as_bytes(), "nothing written");
    assert!(sent_with_draft(&mut events).is_empty());

    age_unclearable_wait(&live, "peer");
    tear(&live);
    live.write_message_input(&framed, "peer", true).unwrap();
    let keys = fixture.keys();
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let notices = sent_with_draft(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["agent"], "Claude Code");
    assert!(!notices[0].contains_key("detail"), "{:?}", notices[0]);
    assert!(!live.draft.lock().unwrap().holds());
}

/// A box that cannot be read after a key that may have brought text into it that nothing knows of
/// — a prompt from history, a completion, a paste by the agent's own key — may be hidden behind a list that would take the
/// message's keys: the message is never written however long it has waited, even with text typed
/// before. The user is told once it has waited the wait.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_that_may_be_hidden_is_never_written_into_whatever_the_wait() {
    for (name, key) in [
        ("history", &b"\x1b[A"[..]),
        ("completion", b"\t"),
        ("paste", b"\x16"),
    ] {
        let fixture = Fixture::new(&format!("hidden-{name}"), AgentId::CLAUDE);
        let live = fixture.live();
        let mut events = fixture.state.subscribe();
        fixture.type_draft(AgentId::CLAUDE, &["draft"]);
        live.write_terminal_input(key).unwrap();
        live.on_output(NO_BOX);
        live.settle_input();
        let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
        assert!(live.write_message_input(&framed, "peer", true).is_err());
        assert!(notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).is_empty());
        age_unclearable_wait(&live, "peer");
        for _ in 0..3 {
            let error = live
                .write_message_input(&framed, "peer", true)
                .expect_err(name);
            assert_eq!(error.written, 0, "{name}");
        }
        let notices = notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT);
        assert_eq!(notices.len(), 1, "{name}: {notices:?}");
        assert!(
            !fixture.keys().windows(4).any(|window| window == b"peer"),
            "{name}"
        );
        assert!(
            sent_with_draft(&mut events).is_empty(),
            "{name}: nothing was sent with the box"
        );
    }
}

/// The user is told that a message waits on a box that cannot be read once for each message, not
/// once for the box: the next message of the same text has its own wait and its own notice.
#[tokio::test(flavor = "multi_thread")]
async fn each_message_waiting_on_a_hidden_box_has_the_user_told_once() {
    let fixture = Fixture::new("hidden-each", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.write_terminal_input(b"\x1b[A").unwrap();
    live.on_output(NO_BOX);
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let attempt = |id: &str| {
        live.write_message_input_as(id, &framed, "peer", true)
            .unwrap_err()
            .written
    };
    assert_eq!(attempt("launch:1"), 0);
    age_unclearable_wait(&live, "launch:1");
    for _ in 0..2 {
        attempt("launch:1");
    }
    assert_eq!(
        notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).len(),
        1
    );

    assert_eq!(attempt("launch:2"), 0);
    assert!(
        notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).is_empty(),
        "the next message waits its own while first"
    );
    age_unclearable_wait(&live, "launch:2");
    attempt("launch:2");
    attempt("launch:2");
    assert_eq!(
        notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).len(),
        1
    );
}

/// A message left unsent alone in the box holds the next one only for the wait after its submit
/// key is no longer retried: then the next goes in after it, to be sent with it, and every client
/// is told the box held the message written before.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_left_unsent_alone_holds_the_next_only_for_the_wait() {
    let fixture = Fixture::new("unsent-alone", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.mark_not_submitted(live.prompts_sent());
    live.on_output(&draw(AgentId::CLAUDE, &["peer"]));
    live.settle_input();
    assert!(live.unsent_waiting(), "held for the user");

    let wait_began = |ago: Duration| {
        *live.unsent_waits_since.lock().unwrap() = Some(
            Instant::now()
                .checked_sub(ago)
                .expect("a clock that reaches back that far"),
        );
    };
    wait_began(super::UNCLEARABLE_DRAFT_WAIT / 2);
    assert!(live.unsent_waiting(), "half the wait has passed");
    wait_began(super::UNCLEARABLE_DRAFT_WAIT);
    assert!(!live.unsent_waiting());

    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    live.write_message_input(&second, "second", false).unwrap();
    let keys = fixture.keys();
    assert!(!keys.contains(&CTRL_U), "{keys:?}");
    assert!(
        keys.windows(6).any(|window| window == b"second"),
        "{keys:?}"
    );
    let notices = sent_with_draft(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["held"], "message");
    assert!(!notices[0].contains_key("detail"), "{:?}", notices[0]);
    assert!(live.unsent_at.lock().unwrap().is_none());
}

/// A prompt the agent reports sent after the message left unsent ends its hold at once, wait or
/// not, and no later message goes in with anything.
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_reported_ends_the_hold_of_a_message_left_unsent() {
    let fixture = Fixture::new("unsent-reported", AgentId::CLAUDE);
    let live = fixture.live();
    live.mark_not_submitted(live.prompts_sent());
    assert!(live.unsent_waiting());
    live.note_prompt_sent(None);
    assert!(!live.unsent_waiting());
    assert!(!live.unsent_hold_over());
}

/// Octoboard's WARN log of a message that went in with text in the box says so and how much, never
/// what the text was.
#[tokio::test(flavor = "multi_thread")]
async fn the_log_of_a_message_sent_with_a_draft_is_a_warning_with_no_text() {
    #[derive(Clone, Default)]
    struct Log(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Log {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let fixture = Fixture::new("log", AgentId::CLAUDE);
    let live = fixture.live();
    fixture.type_draft(AgentId::CLAUDE, &[CJK_DRAFT]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    assert!(live.write_message_input(&framed, "peer", true).is_err());
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    age_unclearable_wait(&live, "peer");

    let log = Log::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer({
            let log = log.clone();
            move || log.clone()
        })
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        live.write_message_input(&framed, "peer", true).unwrap();
    });
    let logged = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(logged.contains("WARN"), "{logged}");
    assert!(
        !logged.contains(CJK_DRAFT) && !logged.contains("peer"),
        "{logged}"
    );
}

/// Codex's composer, as Claude Code's, takes one press for a collapsed paste: the placeholder it
/// shows is all the box holds of it, and all the notice hands back.
#[tokio::test(flavor = "multi_thread")]
async fn a_codex_collapsed_paste_is_cleared_with_one_press_and_reported_as_its_placeholder() {
    const PLACEHOLDER: &str = "[Pasted Content 2400 chars]";
    let fixture = Fixture::new("codex-paste", AgentId::CODEX);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CODEX, &[PLACEHOLDER]);
    let screen = answer_presses(live.clone(), vec![draw(AgentId::CODEX, &[])]);
    let framed = crate::session::term::frame_message(AgentId::CODEX, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    screen.join().unwrap();
    let keys = fixture.keys();
    assert_eq!(keys.iter().filter(|&&key| key == CTRL_U).count(), 1);
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    assert_eq!(
        discarded(&mut events),
        [("Codex".to_string(), PLACEHOLDER.to_string())]
    );
}

/// A draft over several lines, in Chinese, is cleared out of Codex's composer a line a press, two
/// to a line, each press seen before the next, and handed back whole.
#[tokio::test(flavor = "multi_thread")]
async fn a_codex_draft_over_several_lines_is_cleared_a_press_at_a_time() {
    const FIRST: &str = "第一行文字";
    const SECOND: &str = "第二行文字";
    let fixture = Fixture::new("codex-lines", AgentId::CODEX);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CODEX, &[FIRST, SECOND]);
    let screen = answer_presses(
        live.clone(),
        vec![
            draw(AgentId::CODEX, &[FIRST, ""]),
            draw(AgentId::CODEX, &[FIRST]),
            draw(AgentId::CODEX, &[]),
        ],
    );
    let framed = crate::session::term::frame_message(AgentId::CODEX, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    screen.join().unwrap();
    let keys = fixture.keys();
    assert_eq!(keys.iter().filter(|&&key| key == CTRL_U).count(), 3);
    let notices = discarded_params(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["detail"], format!("{FIRST}\n{SECOND}"));
    assert!(!notices[0].contains_key("cleared"), "{:?}", notices[0]);
}

/// Text the box shows only on a frame not drawn whole is a draft the message cannot get out of its
/// way: the first attempt that finds it begins the message's wait, as any other draft that cannot
/// be cleared does, so that an agent which never stops redrawing cannot hold a message for ever.
#[tokio::test(flavor = "multi_thread")]
async fn text_shown_only_on_a_torn_frame_begins_the_wait_of_the_message() {
    let fixture = Fixture::new("torn-text-begins", AgentId::CLAUDE);
    let live = fixture.live();
    fixture.type_draft(AgentId::CLAUDE, &[CJK_DRAFT]);
    crate::session::draft_hold::WHOLE_FRAME_WAIT_OVERRIDE
        .with(|wait| wait.set(Some(Duration::ZERO)));
    tear(&live);
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let attempt = live.write_message_input(&framed, "peer", true);
    crate::session::draft_hold::WHOLE_FRAME_WAIT_OVERRIDE.with(|wait| wait.set(None));
    assert_eq!(attempt.unwrap_err().written, 0);
    assert_eq!(
        fixture.keys(),
        CJK_DRAFT.as_bytes(),
        "no press, nothing written"
    );
    assert!(
        live.unclearable_since.lock().unwrap().is_some(),
        "the attempt that found the draft begins the wait of the message"
    );
}

/// Once the wait on text shown only on a frame not drawn whole is over, the message goes in as the
/// box stands, and every client is told what it showed.
#[tokio::test(flavor = "multi_thread")]
async fn text_shown_only_on_a_torn_frame_holds_the_message_only_for_the_wait() {
    let fixture = Fixture::new("torn-text-bound", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_draft(AgentId::CLAUDE, &[CJK_DRAFT]);
    begin_wait(&live, "peer", super::UNCLEARABLE_DRAFT_WAIT);
    crate::session::draft_hold::WHOLE_FRAME_WAIT_OVERRIDE
        .with(|wait| wait.set(Some(Duration::ZERO)));
    tear(&live);
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let attempt = live.write_message_input(&framed, "peer", true);
    crate::session::draft_hold::WHOLE_FRAME_WAIT_OVERRIDE.with(|wait| wait.set(None));
    attempt.expect("the wait is over: the message goes in as the box stands");
    let keys = fixture.keys();
    assert!(
        !keys.contains(&CTRL_U),
        "no press on a torn frame: {keys:?}"
    );
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let notices = sent_with_draft(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["detail"], CJK_DRAFT);
}

/// Keys that typed text and then moved the cursor bring nothing Octoboard cannot know into the box:
/// a box that cannot be read after them holds the message only for the wait, as known text does,
/// and then the message goes in as the box stands, with no notice that it waits on the user.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_box_after_typing_and_a_cursor_key_holds_the_message_only_for_the_wait() {
    let fixture = Fixture::new("unreadable-cursor", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.write_terminal_input(b"draft").unwrap();
    live.write_terminal_input(b"\x1b[D").unwrap();
    live.on_output(b"\x1b[2J\x1b[H  something that is not the input box");
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    assert_eq!(
        live.write_message_input(&framed, "peer", true)
            .unwrap_err()
            .written,
        0
    );
    assert!(!fixture.keys().windows(4).any(|window| window == b"peer"));

    age_unclearable_wait(&live, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    let keys = fixture.keys();
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    let all: Vec<Event> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let codes: Vec<_> = all
        .iter()
        .filter_map(|event| match event {
            Event::SessionNotice { code, params, .. } => Some((*code, params.clone())),
            _ => None,
        })
        .collect();
    assert!(
        !codes
            .iter()
            .any(|(code, _)| *code == notice_code::MESSAGE_WAITS_ON_INPUT),
        "{codes:?}"
    );
    let sent: Vec<_> = codes
        .iter()
        .filter(|(code, _)| *code == notice_code::MESSAGE_SENT_WITH_DRAFT)
        .collect();
    assert_eq!(sent.len(), 1, "{codes:?}");
    assert!(
        !sent[0].1.iter().any(|(key, _)| key == "detail"),
        "an unreadable box hands nothing back: {codes:?}"
    );
}

/// A message forced into a box that cannot be read, through a clear that finds no box to read,
/// still never goes in after a key that may have brought in text Octoboard cannot know: a list may
/// be hiding the box. It waits, and the user is told once the wait is over.
#[tokio::test(flavor = "multi_thread")]
async fn a_forced_write_waits_on_a_hidden_box_after_a_history_key() {
    let fixture = Fixture::new("forced-hidden", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.write_terminal_input(b"x").unwrap();
    live.write_terminal_input(b"\x1b[A").unwrap();
    live.on_output(b"\x1b[2J\x1b[H  a list of earlier prompts");
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    assert_eq!(
        live.write_after_clearing(&framed, "peer")
            .unwrap_err()
            .written,
        0
    );
    age_unclearable_wait(&live, "peer");
    for _ in 0..2 {
        assert_eq!(
            live.write_after_clearing(&framed, "peer")
                .unwrap_err()
                .written,
            0
        );
    }
    assert!(!fixture.keys().windows(4).any(|window| window == b"peer"));
    assert_eq!(
        notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).len(),
        1,
        "told once"
    );
}

/// A message left unsent whose hold ran out still never goes into a box hidden after a history
/// key: it waits, and the user is told once that wait is over too.
#[tokio::test(flavor = "multi_thread")]
async fn an_unsent_hold_run_out_waits_on_a_hidden_box_after_a_history_key() {
    let fixture = Fixture::new("unsent-hidden", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.mark_not_submitted(live.prompts_sent());
    live.write_terminal_input(b"\x1b[A").unwrap();
    live.on_output(b"\x1b[2J\x1b[H  a list of earlier prompts");
    live.settle_input();
    *live.unsent_waits_since.lock().unwrap() = Some(
        Instant::now()
            .checked_sub(super::UNCLEARABLE_DRAFT_WAIT)
            .expect("a clock that reaches back that far"),
    );
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    assert_eq!(
        live.write_message_input(&second, "second", false)
            .unwrap_err()
            .written,
        0
    );
    assert!(notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).is_empty());
    age_unclearable_wait(&live, "second");
    assert_eq!(
        live.write_message_input(&second, "second", false)
            .unwrap_err()
            .written,
        0
    );
    assert!(!fixture.keys().windows(6).any(|window| window == b"second"));
    assert_eq!(
        notices_of(&mut events, notice_code::MESSAGE_WAITS_ON_INPUT).len(),
        1
    );
}

/// A box seen empty off a whole frame ends the wait of the message on the draft before it: a draft
/// typed afterwards starts a wait of its own.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_seen_empty_restarts_the_wait() {
    let fixture = Fixture::new("wait-restart", AgentId::CLAUDE);
    let live = fixture.live();
    fixture.type_draft(AgentId::CLAUDE, &["draft"]);
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    assert!(live.write_message_input(&framed, "peer", true).is_err());
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    assert!(live.unclearable_since.lock().unwrap().is_some());

    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    // Reading the box has no effect of its own; the watch's look that finds it empty ends the wait.
    let _ = live.read_box();
    assert!(live.unclearable_since.lock().unwrap().is_some());
    live.release_draft_hold();
    assert!(
        live.unclearable_since.lock().unwrap().is_none(),
        "the wait ends with the draft"
    );
}

/// A write as the box stands that finds the box empty off a whole frame is an ordinary write, with
/// no notice, whatever text the typing record still holds; the record is told the box is empty.
#[tokio::test(flavor = "multi_thread")]
async fn a_forced_write_into_a_box_seen_empty_is_an_ordinary_write() {
    let fixture = Fixture::new("forced-empty", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.write_terminal_input(b"stale").unwrap();
    live.settle_input();
    assert!(live.draft.lock().unwrap().knows_text());
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_into_box(
        &framed,
        "peer",
        true,
        BoxIs::Unclearable {
            unsent: false,
            read_before: None,
        },
    )
    .unwrap();
    assert!(fixture.keys().windows(4).any(|window| window == b"peer"));
    assert!(notices_of(&mut events, notice_code::MESSAGE_SENT_WITH_DRAFT).is_empty());
    assert!(!live.draft.lock().unwrap().knows_text());
}

/// A hold on the input that never ends — here a partly written message whose phase stands — keeps
/// a message out until the hold has lasted its bound and the message, past its grace, has waited
/// as long: then it goes into the box as the box stands, and every client is told what the box
/// held. The hold's time starts again with that write, and a later message waits out its own grace
/// and wait.
#[tokio::test(flavor = "multi_thread")]
async fn a_hold_on_the_input_keeps_each_message_out_only_for_its_own_wait() {
    use crate::session::swap::RecoveryPhase;
    let fixture = Fixture::new("swap-bound", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    // The box keeps showing the fragment after each write, which a write would otherwise wait on
    // for as long as a test allows, past the waits this test measures.
    crate::session::swap::OWN_MESSAGE_SETTLE_OVERRIDE
        .with(|settle| settle.set(Some(Duration::from_millis(50))));
    live.on_output(&draw(AgentId::CLAUDE, &["frag"]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    let refused =
        |framed: &[crate::platform::term::Input], prompt: &str, past_grace: bool, step: &str| {
            match live.write_message_input(framed, prompt, past_grace) {
                Ok(()) => panic!("{step}: {prompt} went in"),
                Err(error) => error.written == 0,
            }
        };
    live.hold_input_since(RecoveryPhase::Fragmented, Duration::from_secs(1));
    assert!(
        refused(&framed, "peer", true, "step 1"),
        "the hold is young"
    );
    live.hold_input_since(RecoveryPhase::Fragmented, super::UNCLEARABLE_DRAFT_WAIT);
    assert!(
        refused(&framed, "peer", false, "step 2"),
        "within its grace the message waits"
    );
    assert!(
        refused(&framed, "peer", true, "step 3"),
        "the hold has lasted, but this message's own wait only starts"
    );
    age_unclearable_wait(&live, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    let notices = notices_of(&mut events, notice_code::MESSAGE_SENT_WITH_DRAFT);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["detail"], "frag");
    assert!(
        !live.input_swap_held_too_long(),
        "the hold's time starts again with the write"
    );

    // A later message during the same hold waits out its grace and its own wait again.
    assert!(live.input_swap_active(), "the hold stands");
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    assert!(
        refused(&second, "second", true, "step 4"),
        "the hold's time started again"
    );
    live.hold_input_since(RecoveryPhase::Fragmented, super::UNCLEARABLE_DRAFT_WAIT);
    assert!(
        refused(&second, "second", false, "step 5"),
        "within its grace"
    );
    assert!(
        refused(&second, "second", true, "step 6"),
        "its own wait is young"
    );
    age_unclearable_wait(&live, "second");
    live.write_message_input(&second, "second", true).unwrap();

    // The marker the fixture reads the keys with is held as any other write while the hold stands.
    crate::session::swap::OWN_MESSAGE_SETTLE_OVERRIDE.with(|settle| settle.set(None));
    live.end_input_hold();
    let keys = fixture.keys();
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
    assert!(
        keys.windows(6).any(|window| window == b"second"),
        "{keys:?}"
    );
}

/// A hold for a partly written message ends once the agent reports a prompt that holds the message
/// that went in past it: the fragment went with that prompt.
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_holding_the_message_forced_past_a_fragment_ends_the_hold() {
    use crate::session::swap::RecoveryPhase;
    let fixture = Fixture::new("swap-ends", AgentId::CLAUDE);
    let live = fixture.live();
    live.on_output(&draw(AgentId::CLAUDE, &["frag"]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.hold_input_since(RecoveryPhase::Fragmented, super::UNCLEARABLE_DRAFT_WAIT);
    assert!(live.write_message_input(&framed, "peer", true).is_err());
    age_unclearable_wait(&live, "peer");
    live.write_message_input(&framed, "peer", true).unwrap();
    assert!(live.input_swap_active());
    live.note_prompt_sent(Some("an earlier prompt"));
    assert!(
        live.input_swap_active(),
        "a prompt without the message ends nothing"
    );
    live.note_prompt_sent(Some("frag peer"));
    assert!(
        !live.input_swap_active(),
        "the fragment went with the message"
    );
}

/// A write as the box stands after a clear's presses that finds the box empty after all goes in as
/// into any empty box, and the draft the presses removed is still told, as a whole clear.
#[tokio::test(flavor = "multi_thread")]
async fn a_forced_write_after_presses_that_emptied_the_box_reports_the_draft_cleared() {
    let fixture = Fixture::new("forced-emptied", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_into_box(
        &framed,
        "peer",
        true,
        BoxIs::Unclearable {
            unsent: false,
            read_before: Some("the draft"),
        },
    )
    .unwrap();
    assert!(fixture.keys().windows(4).any(|window| window == b"peer"));
    let all: Vec<Event> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let of = |wanted: crate::protocol::Code| -> Vec<std::collections::HashMap<String, String>> {
        all.iter()
            .filter_map(|event| match event {
                Event::SessionNotice { code, params, .. } if *code == wanted => {
                    Some(params.clone().into_iter().collect())
                }
                _ => None,
            })
            .collect()
    };
    let discarded = of(notice_code::DRAFT_DISCARDED);
    assert_eq!(discarded.len(), 1, "{discarded:?}");
    assert_eq!(discarded[0]["detail"], "the draft");
    assert!(!discarded[0].contains_key("cleared"), "{:?}", discarded[0]);
    assert!(of(notice_code::MESSAGE_SENT_WITH_DRAFT).is_empty());
}
