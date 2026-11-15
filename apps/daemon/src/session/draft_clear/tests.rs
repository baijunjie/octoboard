//! Clearing a draft out of the way of a message past the draft's grace, through the session's
//! writes: the box is cleared a press at a time, seen empty off a whole frame, and only then is the
//! message written; where that cannot be seen, nothing of the message is written.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::{notice_code, AgentId, Event};
use crate::session::input_box_clear::PRESS_OVERRIDE;
use crate::session::LiveSession;
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
            _dir: dir,
        }
    }

    fn live(&self) -> Arc<LiveSession> {
        (*self.live).clone()
    }

    /// What was written into the session's input, once a marker written after the call has
    /// arrived, so an absence asserted in it is one of a reader that has caught up.
    fn keys(&self) -> Vec<u8> {
        const MARKER: &[u8] = b"marker";
        self.live.write_input(MARKER).unwrap();
        let deadline = Instant::now() + PATIENCE;
        loop {
            let keys = std::fs::read(&self.keys).unwrap_or_default();
            if keys.ends_with(MARKER) || Instant::now() >= deadline {
                assert!(keys.ends_with(MARKER), "{keys:?}");
                return keys[..keys.len() - MARKER.len()].to_vec();
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
/// runs.
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

/// The message Octoboard wrote last, alone in the box and holding the queue, is told to the user
/// once it has waited a while: once for that message, not on every retry.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_left_alone_in_the_box_is_told_to_the_user_once() {
    let fixture = Fixture::new("own-told", AgentId::CLAUDE);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    let framed = crate::session::term::frame_message(AgentId::CLAUDE, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.on_output(&draw(AgentId::CLAUDE, &["peer"]));
    live.settle_input();
    let second = crate::session::term::frame_message(AgentId::CLAUDE, "second");
    let not_sent = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
        let mut count = 0;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionNotice { code, params, .. } = event {
                if code == notice_code::MESSAGE_NOT_SENT {
                    assert_eq!(params.get("submit_retry"), None);
                    count += 1;
                }
            }
        }
        count
    };
    let age = || {
        let mut held = live.own_message_held.lock().unwrap();
        let since = &mut held.as_mut().expect("held").1;
        *since = since
            .checked_sub(crate::session::UNSENT_RETRY_BOUND)
            .expect("a clock that reaches back that far");
    };
    assert!(live.write_after_clearing(&second, "second").is_err());
    assert_eq!(not_sent(&mut events), 0, "not before the bound");
    age();
    for _ in 0..3 {
        assert!(live.write_after_clearing(&second, "second").is_err());
    }
    assert_eq!(not_sent(&mut events), 1, "told once");

    // The same text written again is another write, with a wait and a notice of its own.
    live.on_output(&draw(AgentId::CLAUDE, &[]));
    live.settle_input();
    live.write_message_input(&framed, "peer", false).unwrap();
    live.on_output(&draw(AgentId::CLAUDE, &["peer"]));
    live.settle_input();
    assert!(live.write_after_clearing(&second, "second").is_err());
    assert_eq!(not_sent(&mut events), 0, "the new write waits afresh");
    age();
    assert!(live.write_after_clearing(&second, "second").is_err());
    assert_eq!(not_sent(&mut events), 1, "and is told once in its turn");
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

/// Codex has no key measured to clear its box, so past the grace a draft there holds the message,
/// with no key pressed and nothing written.
#[test]
fn a_draft_no_key_can_clear_holds_the_message() {
    let live =
        fake_live_session("codex-draft", AgentId::CODEX, 140, 45, "sleep 30").with_composer();
    live.on_output(include_bytes!(
        "../../../testdata/codex_blank_first_draft.bin"
    ));
    live.write_terminal_input(b"draft").unwrap();
    live.settle_input();
    let before = live.input_writes();
    let framed = crate::session::term::frame_message(AgentId::CODEX, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(live.input_writes(), before);
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
        .write_message_into_empty_box(&framed, "peer", true, Some(cleared_at))
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
    live.write_message_into_empty_box(&framed, "peer", true, Some(now))
        .unwrap();
    assert!(!live.draft.lock().unwrap().holds());
    let keys = fixture.keys();
    assert!(keys.windows(4).any(|window| window == b"peer"), "{keys:?}");
}
