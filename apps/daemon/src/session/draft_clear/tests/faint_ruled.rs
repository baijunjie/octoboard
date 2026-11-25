//! A provider whose input box can only be found by what it draws faint, and whose cancel gives
//! back the messages it held: the box is read off the screen as drawn, a box of several rows is
//! cleared a press at a time within the run's bound, and a message the agent gave back is cleared
//! out of the way of the next one and queued to be written again after it, by the id the queue
//! wrote it under, with nobody told unless it cannot be. The provider is the test-only one, reading
//! through a reader of this module's own.

use std::collections::HashMap;
use std::time::Duration;

use super::{answer_presses, discarded, discarded_params, Fixture, CTRL_U};
use crate::messaging::texts::NOTICE_PREFIX;
use crate::messaging::untaken_messages::GraceOverride;
use crate::protocol::{notice_code, AgentId, Event, SessionStatus};
use crate::session::input_box_clear::{Cleared, PRESS_OVERRIDE};
use crate::session::screen::{ScreenReader, ScreenText};
use crate::session::swap::message_seen_at_once;
use crate::test_support::test_provider::{
    declaring_hooks, declaring_launch_without_status_integration, declaring_reported_signals,
    declaring_screen,
};
use crate::test_support::PATIENCE;

/// The hint the reader's provider draws, faint, in an empty box.
const HINT: &str = "Type a message";

/// Reads a box with no prompt mark between the last two full-width rules of the screen, which the
/// provider draws faint as it does the footer under them, so on a screen with faint cells blanked
/// nothing is left to find the box by. Its Ctrl-U deletes a line a press, and at the start of a
/// line the line break before it.
struct FaintRuled {
    /// Whether a cancel puts the messages the provider held back into the box.
    gives_back: bool,
    /// Whether a cancel can drop the messages the provider held.
    drops: bool,
    /// Whether an empty box after a write the agent did not report sending means the message went
    /// elsewhere.
    elsewhere: bool,
}

static HOLDING: FaintRuled = FaintRuled {
    gives_back: true,
    drops: false,
    elsewhere: false,
};
static PLAIN: FaintRuled = FaintRuled {
    gives_back: false,
    drops: false,
    elsewhere: false,
};
/// A provider whose cancel drops what it held, with nothing put back.
static DROPPING: FaintRuled = FaintRuled {
    gives_back: false,
    drops: true,
    elsewhere: false,
};
/// A provider whose empty box after an unreported write means the message went elsewhere.
static ELSEWHERE: FaintRuled = FaintRuled {
    gives_back: false,
    drops: false,
    elsewhere: true,
};
/// A provider one of whose cancel keys gives the held messages back and another drops them.
static EITHER: FaintRuled = FaintRuled {
    gives_back: true,
    drops: true,
    elsewhere: false,
};

fn is_rule(row: &str) -> bool {
    let row = row.trim();
    row.chars().count() >= 20 && row.chars().all(|c| c == '─')
}

impl ScreenReader for FaintRuled {
    fn input_box_screen(&self) -> ScreenText {
        ScreenText::AsDrawn
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        let rows: Vec<&str> = screen.lines().collect();
        let lower = rows.iter().rposition(|row| is_rule(row))?;
        let upper = rows[..lower].iter().rposition(|row| is_rule(row))?;
        (upper + 1 < lower).then(|| {
            rows[upper + 1..lower]
                .iter()
                .map(|row| row.trim().to_string())
                .collect()
        })
    }

    fn input_placeholder(&self, row: &str) -> bool {
        row == HINT
    }

    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    fn cancel_restores_held_messages(&self) -> bool {
        self.gives_back
    }

    fn cancel_drops_held_messages(&self) -> bool {
        self.drops
    }

    fn empty_box_means_write_went_elsewhere(&self) -> bool {
        self.elsewhere
    }

    fn collapsed_paste(&self, text: &str) -> bool {
        text.starts_with("[Pasted text #") && text.ends_with(']')
    }
}

/// The screen with `rows` between two faint rules, over a faint footer; no row is an empty box of
/// one row.
fn frame(rows: &[String]) -> Vec<u8> {
    let rule = format!("\x1b[2m{}\x1b[22m", "─".repeat(60));
    let mut screen = vec![rule.clone()];
    screen.extend(rows.iter().cloned());
    if rows.is_empty() {
        screen.push(String::new());
    }
    screen.push(rule);
    screen.push("\x1b[2m~/project   a model\x1b[22m".to_string());
    format!("\x1b[2J\x1b[1;1H{}", screen.join("\r\n")).into_bytes()
}

/// [`frame`] with `lines` as the box's text.
fn ruled(lines: &[&str]) -> Vec<u8> {
    frame(
        &lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>(),
    )
}

/// `count` lines of a draft.
fn lines_of(count: usize) -> Vec<String> {
    (1..=count).map(|at| format!("line {at}")).collect()
}

/// The screens the provider draws as Ctrl-U is pressed into a box holding `lines`, one a press: a
/// press empties the last line, the next removes its line break, down to an empty box.
fn clearing(lines: &[String]) -> Vec<Vec<u8>> {
    let mut rows = lines.to_vec();
    let mut screens = Vec::new();
    loop {
        *rows.last_mut().expect("a box has a row") = String::new();
        screens.push(frame(&rows));
        if rows.len() == 1 {
            return screens;
        }
        rows.pop();
        screens.push(frame(&rows));
    }
}

impl Fixture {
    /// The draft `lines` typed into the ruled box and drawn there, settled as if typed well before.
    fn type_ruled_draft(&self, lines: &[String]) {
        self.live.on_output(&frame(&[]));
        self.live
            .write_terminal_input(lines.join("\n").as_bytes())
            .unwrap();
        self.live.on_output(&frame(lines));
        self.live.settle_input();
    }
}

/// Which of the two notices a clear tells, in the order sent, each as whether it is
/// `message_cleared_after_cancel` (`draft_discarded` otherwise) with its params; every pending
/// event is drained.
fn cleared_notices(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
) -> Vec<(bool, HashMap<String, String>)> {
    let mut notices = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::SessionNotice {
            session,
            code,
            params,
            ..
        } = event
        {
            let message = code == notice_code::MESSAGE_CLEARED_AFTER_CANCEL;
            if message || code == notice_code::DRAFT_DISCARDED {
                assert_eq!(session, "s", "the notice is about the session cleared");
                notices.push((message, params.into_iter().collect()));
            }
        }
    }
    notices
}

/// The id the queue would write a message under.
const QUEUED_AS: &str = "launch:7";

impl Fixture {
    /// A message written while a turn runs, as the queue writes one, then the turn cancelled and
    /// the message, which the agent held, back in its box.
    fn give_back(&self, text: &str) {
        let live = self.live();
        live.on_output(&ruled(&[]));
        live.settle_input();
        self.state
            .apply_hook_status("s", SessionStatus::Working)
            .unwrap();
        self.state.outbox.note_latest("s", 7, text, false, false);
        let framed = crate::session::term::frame_message(AgentId::TEST, text);
        live.write_message_input_as(QUEUED_AS, &framed, text, false)
            .unwrap();
        self.state
            .apply_hook_status("s", SessionStatus::Idle)
            .unwrap();
        live.on_output(&ruled(&[text]));
        live.settle_input();
    }
}

fn presses(keys: &[u8]) -> usize {
    keys.iter().filter(|&&key| key == CTRL_U).count()
}

fn has(keys: &[u8], text: &str) -> bool {
    keys.windows(text.len())
        .any(|window| window == text.as_bytes())
}

/// The box between faint rules is found only on the screen as drawn, which is the one every
/// reading of the box is given for this reader; its faint hint is an empty box there, on a host
/// that renders nothing itself. A provider that declares nothing is still read with faint cells
/// blanked.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_framed_by_faint_rules_is_read_off_the_screen_as_drawn() {
    assert_eq!(
        crate::adapter::provider(AgentId::TEST)
            .screen()
            .input_box_screen(),
        ScreenText::FaintBlanked,
        "the reader of a provider that declares nothing"
    );
    let _reader = declaring_screen(&PLAIN);
    let fixture = Fixture::new("as-drawn", AgentId::TEST);
    let live = fixture.live();
    live.on_output(&ruled(&["first row", "second row"]));
    assert_eq!(
        live.input_box(&live.screen_text_undimmed()),
        None,
        "with faint cells blanked the rules are gone"
    );
    assert_eq!(
        live.input_box_rows(&live.box_screen()),
        Some(vec!["first row".to_string(), "second row".to_string()])
    );
    assert_eq!(
        live.read_box().0.as_deref(),
        Some("first row\nsecond row"),
        "the draft paths read the same box"
    );
    live.on_output(&ruled(&[&format!("\x1b[2m{HINT}\x1b[22m")]));
    assert_eq!(live.input_box(&live.box_screen()).as_deref(), Some(""));
    live.on_output(&ruled(&[HINT, "typed under it"]));
    assert_eq!(
        live.input_box(&live.box_screen()),
        Some(format!("{HINT}\ntyped under it")),
        "the hint's words over other text are text"
    );
}

/// A box of one row, of three and of many is emptied with a key that clears a row a press: the
/// key goes in again, each press seen on the screen before the next, until the box reads empty,
/// and only then is the message written.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_of_several_rows_is_emptied_a_row_a_press() {
    let _reader = declaring_screen(&PLAIN);
    for rows in [1, 3, 12] {
        let fixture = Fixture::new(&format!("rows-{rows}"), AgentId::TEST);
        let live = fixture.live();
        let mut events = fixture.state.subscribe();
        let draft = lines_of(rows);
        fixture.type_ruled_draft(&draft);
        let screen = answer_presses(live.clone(), clearing(&draft));
        let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
        live.write_message_input(&framed, "peer", true).unwrap();
        screen.join().unwrap();
        let keys = fixture.keys();
        assert_eq!(presses(&keys), rows * 2 - 1, "{rows} rows: {keys:?}");
        let last = keys.iter().rposition(|&key| key == CTRL_U).unwrap();
        assert!(
            !has(&keys[..last], "peer") && has(&keys[last..], "peer"),
            "{rows} rows: the message alone, after the last press: {keys:?}"
        );
        assert_eq!(
            discarded(&mut events),
            [(AgentId::TEST.label().to_string(), draft.join("\n"))],
            "{rows} rows"
        );
    }
}

/// A box of more rows than the run's presses can clear is given the most presses a run spends and
/// no more: the message is not written into what is left, and what was removed is reported as a
/// part.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_of_more_rows_than_the_bound_gets_the_bound_s_presses() {
    let _reader = declaring_screen(&PLAIN);
    let fixture = Fixture::new("rows-bound", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    let draft = lines_of(30);
    fixture.type_ruled_draft(&draft);
    let mut screens = clearing(&draft);
    screens.truncate(40);
    let screen = answer_presses(live.clone(), screens);
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    screen.join().unwrap();
    assert_eq!(error.written, 0);
    assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    let keys = fixture.keys();
    assert_eq!(presses(&keys), 40, "{keys:?}");
    assert!(!has(&keys, "peer"), "{keys:?}");
    let notices = discarded_params(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["cleared"], "part");
    assert_eq!(notices[0]["detail"], draft.join("\n"));
}

/// A box that every press changes and none empties is pressed into only as often as its rows
/// call for, two a row and one more, and the message is not written.
#[tokio::test(flavor = "multi_thread")]
async fn a_box_that_never_empties_ends_the_presses_at_their_count() {
    let _reader = declaring_screen(&PLAIN);
    let fixture = Fixture::new("never-empty", AgentId::TEST);
    let live = fixture.live();
    fixture.type_ruled_draft(&lines_of(1));
    let screen = answer_presses(
        live.clone(),
        vec![ruled(&["still"]), ruled(&["there"]), ruled(&["after all"])],
    );
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    let error = live.write_message_input(&framed, "peer", true).unwrap_err();
    screen.join().unwrap();
    assert_eq!(error.written, 0);
    let keys = fixture.keys();
    assert_eq!(presses(&keys), 3, "{keys:?}");
    assert!(!has(&keys, "peer"), "{keys:?}");
}

/// A press that leaves the box as it was ends the presses: no key goes in blind after it, on that
/// attempt or the ones that follow, and nothing of the message is written.
#[tokio::test(flavor = "multi_thread")]
async fn a_press_that_leaves_the_box_unchanged_is_not_repeated() {
    let _reader = declaring_screen(&PLAIN);
    let fixture = Fixture::new("unchanged", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.type_ruled_draft(&lines_of(3));
    PRESS_OVERRIDE.with(|shown| shown.set(Some(Duration::from_millis(200))));
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    for _ in 0..3 {
        let error = live.write_message_input(&framed, "peer", true).unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(error.error.kind(), std::io::ErrorKind::WouldBlock);
    }
    PRESS_OVERRIDE.with(|shown| shown.set(None));
    let keys = fixture.keys();
    assert_eq!(presses(&keys), 1, "{keys:?}");
    assert!(!has(&keys, "peer"), "{keys:?}");
    assert!(discarded(&mut events).is_empty(), "nothing was removed");
}

/// A message written while a turn ran, which the agent took out of its box and held, is put back
/// there by a cancel. It is no message left unsent: before the next message is written the box is
/// cleared of it, so the next message is neither held behind it nor sent with it. Nor is it lost
/// or a draft of the user's: it is queued to be written again, as the queue wrote it, and nobody
/// is told anything.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_the_agent_gave_back_at_a_cancel_is_cleared_and_queued_again() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.give_back("peer");
    let _settle = message_seen_at_once();
    let screen = answer_presses(live.clone(), vec![ruled(&[])]);
    let second = crate::session::term::frame_message(AgentId::TEST, "second");
    live.write_message_input(&second, "second", true).unwrap();
    screen.join().unwrap();
    let keys = fixture.keys();
    assert_eq!(presses(&keys), 1, "{keys:?}");
    let press = keys.iter().position(|&key| key == CTRL_U).unwrap();
    assert!(
        !has(&keys[..press], "second") && has(&keys[press..], "second"),
        "the next message alone, after the press: {keys:?}"
    );
    let notices = cleared_notices(&mut events);
    assert!(notices.is_empty(), "nothing is lost: {notices:?}");
    assert_eq!(fixture.state.take_queued("s"), ["peer"]);
}

/// Text typed after the message the agent gave back is the user's and the message is not: the
/// box is cleared, the message queued again, and only what was typed handed back as a draft.
#[tokio::test(flavor = "multi_thread")]
async fn text_typed_after_a_message_given_back_is_the_only_draft_handed_back() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-typed", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.give_back("peer");
    live.write_terminal_input(b" and mine").unwrap();
    live.on_output(&ruled(&["peer and mine"]));
    live.settle_input();
    let _settle = message_seen_at_once();
    let screen = answer_presses(live.clone(), vec![ruled(&[])]);
    let second = crate::session::term::frame_message(AgentId::TEST, "second");
    live.write_message_input(&second, "second", true).unwrap();
    screen.join().unwrap();
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(!*message, "told as a draft: {params:?}");
    assert_eq!(params["detail"], "and mine");
    assert_eq!(fixture.state.take_queued("s"), ["peer"]);
}

/// A cancel the user types in the terminal is a key typed after the message, but what the agent
/// then puts back is still the message: a box showing its very text is cleared and the message
/// queued again, not handed back as a draft.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_given_back_by_a_cancel_typed_in_the_terminal_is_queued_again() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-esc", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&ruled(&[]));
    live.settle_input();
    fixture
        .state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    fixture
        .state
        .outbox
        .note_latest("s", 7, "peer", false, false);
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    live.write_message_input_as(QUEUED_AS, &framed, "peer", false)
        .unwrap();
    // The user cancels the turn with Esc, and the agent gives the message back.
    live.write_terminal_input(b"\x1b").unwrap();
    fixture
        .state
        .apply_hook_status("s", SessionStatus::Idle)
        .unwrap();
    live.on_output(&ruled(&["peer"]));
    live.settle_input();
    let _settle = message_seen_at_once();
    let screen = answer_presses(live.clone(), vec![ruled(&[])]);
    let second = crate::session::term::frame_message(AgentId::TEST, "second");
    live.write_message_input(&second, "second", true).unwrap();
    screen.join().unwrap();
    let notices = cleared_notices(&mut events);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(fixture.state.take_queued("s"), ["peer"]);
}

/// A message given back that cannot be written again is told to the user as a message not
/// carried out, with its text, and never as a draft: one the queue did not write, so it has
/// nothing to write again from, and one that was more than text.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_given_back_that_cannot_be_written_again_is_told_to_the_user() {
    for (name, queued) in [("direct", false), ("shaped", true)] {
        let _reader = declaring_screen(&HOLDING);
        let fixture = Fixture::new(&format!("given-back-{name}"), AgentId::TEST);
        let live = fixture.live();
        let mut events = fixture.state.subscribe();
        live.on_output(&ruled(&[]));
        live.settle_input();
        fixture
            .state
            .apply_hook_status("s", SessionStatus::Working)
            .unwrap();
        let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
        if queued {
            fixture
                .state
                .outbox
                .note_latest("s", 7, "peer", true, false);
            live.write_message_input_as(QUEUED_AS, &framed, "peer", false)
                .unwrap();
        } else {
            live.write_message_input(&framed, "peer", false).unwrap();
        }
        fixture
            .state
            .apply_hook_status("s", SessionStatus::Idle)
            .unwrap();
        live.on_output(&ruled(&["peer"]));
        live.settle_input();
        let _settle = message_seen_at_once();
        let screen = answer_presses(live.clone(), vec![ruled(&[])]);
        let second = crate::session::term::frame_message(AgentId::TEST, "second");
        live.write_message_input(&second, "second", true).unwrap();
        screen.join().unwrap();
        let notices = cleared_notices(&mut events);
        assert_eq!(notices.len(), 1, "{name}: {notices:?}");
        let (message, params) = &notices[0];
        assert!(*message, "{name}: told as a message: {params:?}");
        assert_eq!(params["agent"], AgentId::TEST.label(), "{name}");
        assert_eq!(params["detail"], "peer", "{name}");
        assert!(!params.contains_key("cleared"), "{name}: {params:?}");
        assert!(fixture.state.take_queued("s").is_empty(), "{name}");
    }
}

/// What an interrupt of Octoboard's own cleared from the box right after its key, where it was
/// the message the agent held and gave back, is queued to be written again, once, with nobody
/// told; the interrupted prompt, or any other text, is nothing to write again.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_given_back_that_an_interrupt_cleared_is_queued_again() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-interrupt", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.give_back("peer");
    live.given_back_cleared_by_interrupt("the prompt that was interrupted", 1, Cleared::Empty);
    assert_eq!(fixture.state.queued_count("s"), 0, "not the message");
    live.given_back_cleared_by_interrupt("peer", 1, Cleared::Empty);
    live.given_back_cleared_by_interrupt("peer", 1, Cleared::Empty);
    assert!(cleared_notices(&mut events).is_empty());
    assert_eq!(fixture.state.take_queued("s"), ["peer"], "queued once");
}

/// Where the box an interrupt cleared showed only the placeholder of a long paste, which stands
/// for the message given back or for the interrupted prompt alike, the message is not written
/// again, which could deliver it twice, nor dropped unseen: the user is told, with the text of the
/// message itself rather than the placeholder.
#[tokio::test(flavor = "multi_thread")]
async fn a_placeholder_an_interrupt_cleared_is_told_with_the_message_s_own_text() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-placeholder", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.give_back("peer");
    live.given_back_cleared_by_interrupt("[Pasted text #1 +24 lines]", 1, Cleared::Empty);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(*message, "{params:?}");
    assert_eq!(params["detail"], "peer");
    assert!(
        fixture.state.take_queued("s").is_empty(),
        "not written again"
    );
}

/// What a partial clear left of a message queued again is cleared without a word only while
/// nothing has been typed since: text the user types, even a part of that message, is a draft,
/// and is handed back as one.
#[tokio::test(flavor = "multi_thread")]
async fn text_typed_after_a_partial_clear_is_a_draft_whatever_it_reads() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-remainder-typed", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.give_back("peer review");
    // An interrupt's clear took part of it, and it is queued again.
    live.given_back_cleared_by_interrupt("peer review", 1, Cleared::Left);
    assert_eq!(fixture.state.take_queued("s"), ["peer review"]);
    live.on_output(&ruled(&[]));
    live.write_terminal_input(b"peer").unwrap();
    live.on_output(&ruled(&["peer"]));
    live.settle_input();
    let _settle = message_seen_at_once();
    let screen = answer_presses(live.clone(), vec![ruled(&[])]);
    let second = crate::session::term::frame_message(AgentId::TEST, "second");
    live.write_message_input(&second, "second", true).unwrap();
    screen.join().unwrap();
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(!*message, "told as a draft: {params:?}");
    assert_eq!(params["detail"], "peer");
}

/// A message an interrupt cleared that cannot be written again is told to the user, with its
/// text, as one cleared for the next message is.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_an_interrupt_cleared_that_cannot_be_written_again_is_told_to_the_user() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-interrupt-told", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&ruled(&[]));
    live.settle_input();
    fixture
        .state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
    live.write_message_input(&framed, "peer", false).unwrap();
    live.given_back_cleared_by_interrupt("peer", 1, Cleared::Left);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(*message, "{params:?}");
    assert_eq!(params["detail"], "peer");
    assert_eq!(params["cleared"], "part");
    assert!(fixture.state.take_queued("s").is_empty());
}

/// The message written last is cleared only where it was given back: one written at the prompt
/// and still in the box was left unsent there, and one written mid-turn into the box of a provider
/// whose cancel gives nothing back is no different. Neither is cleared, as for every provider
/// that declares nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_left_unsent_is_still_not_cleared() {
    for (name, reader, mid_turn) in [("at-prompt", &HOLDING, false), ("no-hold", &PLAIN, true)] {
        let _reader = declaring_screen(reader);
        let fixture = Fixture::new(&format!("unsent-{name}"), AgentId::TEST);
        let live = fixture.live();
        let mut events = fixture.state.subscribe();
        live.on_output(&ruled(&[]));
        live.settle_input();
        if mid_turn {
            fixture
                .state
                .apply_hook_status("s", SessionStatus::Working)
                .unwrap();
        }
        let framed = crate::session::term::frame_message(AgentId::TEST, "peer");
        live.write_message_input(&framed, "peer", false).unwrap();
        live.on_output(&ruled(&["peer"]));
        live.settle_input();
        let second = crate::session::term::frame_message(AgentId::TEST, "second");
        let error = live
            .write_after_clearing(&second, "second", None)
            .unwrap_err();
        assert_eq!(error.written, 0, "{name}");
        assert!(
            error.error.to_string().contains("only the message"),
            "{name}: {error}"
        );
        let keys = fixture.keys();
        assert!(!keys.contains(&CTRL_U), "{name}: {keys:?}");
        assert!(discarded(&mut events).is_empty(), "{name}");
    }
}

impl Fixture {
    /// The messages `texts` written while a turn runs, as the queue writes them, under the
    /// tickets 7 and on; the agent holds them, so the box is empty again.
    fn hold(&self, texts: &[&str]) {
        self.hold_from(7, texts);
    }

    /// [`Self::hold`], with the queue's tickets for `texts` counted from `first`.
    fn hold_from(&self, first: u64, texts: &[&str]) {
        let live = self.live();
        live.on_output(&ruled(&[]));
        live.settle_input();
        self.state.turn_started("s", None);
        self.state
            .apply_hook_status("s", SessionStatus::Working)
            .unwrap();
        for (at, text) in texts.iter().enumerate() {
            let ticket = first + at as u64;
            self.state
                .outbox
                .note_latest("s", ticket, text, false, false);
            let framed = crate::session::term::frame_message(AgentId::TEST, text);
            live.write_message_input_as(&format!("launch:{ticket}"), &framed, text, false)
                .unwrap();
            live.on_output(&ruled(&[]));
            live.settle_input();
        }
    }

    /// The turn cancelled, with `rows` in the box afterwards.
    fn cancelled_leaving(&self, rows: &[&str]) {
        self.state
            .apply_hook_status("s", SessionStatus::Idle)
            .unwrap();
        self.live.on_output(&ruled(rows));
        self.live.settle_input();
    }

    /// The next message, `text`, written past its grace into a box whose clearing draws `rows`
    /// emptied a press at a time.
    fn write_next(&self, text: &str, rows: &[&str]) {
        let live = self.live();
        let _settle = message_seen_at_once();
        let rows: Vec<String> = rows.iter().map(|row| row.to_string()).collect();
        let screen = answer_presses(live.clone(), clearing(&rows));
        let framed = crate::session::term::frame_message(AgentId::TEST, text);
        live.write_message_input(&framed, text, true).unwrap();
        screen.join().unwrap();
    }
}

/// Several messages the agent held and gave back together, each its own text on a row of its
/// own, are cleared out of the way of the next message and queued again in the order they were
/// written in, with nobody told.
#[tokio::test(flavor = "multi_thread")]
async fn several_messages_given_back_together_are_queued_again_in_their_order() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-several", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second", "third"]);
    fixture.cancelled_leaving(&["first", "second", "third"]);
    fixture.write_next("next", &["first", "second", "third"]);
    let keys = fixture.keys();
    let press = keys.iter().rposition(|&key| key == CTRL_U).unwrap();
    assert!(
        has(&keys[press..], "next"),
        "the next message alone, after the presses: {keys:?}"
    );
    let notices = cleared_notices(&mut events);
    assert!(notices.is_empty(), "nothing is lost: {notices:?}");
    assert_eq!(fixture.state.take_queued("s"), ["first", "second", "third"]);
}

/// Only the latest of the messages held: one the agent had taken into
/// the turn before the cancel is not in the box, and is not written again.
#[tokio::test(flavor = "multi_thread")]
async fn only_the_messages_the_box_shows_are_queued_again() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-latest-two", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second", "third"]);
    fixture.cancelled_leaving(&["second", "third"]);
    fixture.write_next("next", &["second", "third"]);
    assert!(cleared_notices(&mut events).is_empty());
    assert_eq!(fixture.state.take_queued("s"), ["second", "third"]);
}

/// What the user typed after the messages given back is theirs: the messages are queued again
/// and the draft handed back holds the typed text alone.
#[tokio::test(flavor = "multi_thread")]
async fn text_typed_after_several_messages_given_back_is_the_only_draft() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-several-typed", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&["first", "second"]);
    live.write_terminal_input(b"my own").unwrap();
    live.on_output(&ruled(&["first", "second", "my own"]));
    live.settle_input();
    fixture.write_next("next", &["first", "second", "my own"]);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(!*message, "told as a draft: {params:?}");
    assert_eq!(params["detail"], "my own");
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// Text that only begins as a held message does is the user's whole: nothing is cut off it and
/// nothing is queued again.
#[tokio::test(flavor = "multi_thread")]
async fn text_that_only_begins_like_a_held_message_is_a_draft_whole() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-prefix", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "ok"]);
    fixture.cancelled_leaving(&[]);
    live.write_terminal_input(b"okay then").unwrap();
    live.on_output(&ruled(&["okay then"]));
    live.settle_input();
    fixture.write_next("next", &["okay then"]);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0].1["detail"], "okay then");
    assert!(fixture.state.take_queued("s").is_empty());
}

/// Of several messages given back, one that cannot be written again (more than text) is told to
/// the user with its own text, and the others are queued again all the same.
#[tokio::test(flavor = "multi_thread")]
async fn one_of_several_given_back_that_cannot_be_written_again_is_told_alone() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-several-shaped", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    // The first was more than text: nothing to write it again from.
    fixture
        .state
        .outbox
        .note_latest("s", 7, "first", true, false);
    fixture.cancelled_leaving(&["first", "second"]);
    fixture.write_next("next", &["first", "second"]);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(*message, "told as a message: {params:?}");
    assert_eq!(params["detail"], "first");
    assert_eq!(fixture.state.take_queued("s"), ["second"]);
}

/// Several messages an interrupt of Octoboard's own cleared right after its key, here with an
/// empty row between two as some agents put them back, are queued again in their order, once.
#[tokio::test(flavor = "multi_thread")]
async fn several_messages_an_interrupt_cleared_are_queued_again_in_their_order() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-several-interrupt", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&["first", "", "second"]);
    live.given_back_cleared_by_interrupt("first\n\nsecond", 3, Cleared::Empty);
    live.given_back_cleared_by_interrupt("first\n\nsecond", 3, Cleared::Empty);
    assert!(cleared_notices(&mut events).is_empty());
    assert_eq!(
        fixture.state.take_queued("s"),
        ["first", "second"],
        "queued once"
    );
}

/// Messages an interrupt of Octoboard's own cleared with text the user typed after them are
/// queued again in their order, and the typed text alone is handed back as a draft cleared, as a
/// write past the grace treats the same box: neither is lost unseen.
#[tokio::test(flavor = "multi_thread")]
async fn text_typed_after_messages_an_interrupt_cleared_is_handed_back_as_a_draft() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-interrupt-typed", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&["first", "", "second", "", "my own"]);
    live.given_back_cleared_by_interrupt("first\n\nsecond\n\nmy own", 5, Cleared::Empty);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(!*message, "told as a draft: {params:?}");
    assert_eq!(params["detail"], "my own");
    assert!(!params.contains_key("cleared"), "{params:?}");
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// A turn that began since ends what the agent held: the same texts in the box are then a
/// draft, cleared and handed back as one, and nothing is queued again.
#[tokio::test(flavor = "multi_thread")]
async fn messages_of_a_turn_before_are_held_no_longer() {
    let _reader = declaring_screen(&HOLDING);
    let fixture = Fixture::new("given-back-turn-before", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.state.abandon_turn("s");
    fixture.state.turn_started("s", None);
    fixture.cancelled_leaving(&["first", "second"]);
    fixture.write_next("next", &["first", "second"]);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(!notices[0].0, "told as a draft: {notices:?}");
    assert!(fixture.state.take_queued("s").is_empty());
}

/// An agent whose cancel drops what it held has the messages written into the cancelled turn
/// queued again, in their order, held until another message is taken, with nobody told.
#[tokio::test(flavor = "multi_thread")]
async fn messages_dropped_at_a_cancel_are_queued_again_held() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.queued_count("s"), 2);
    assert_eq!(
        fixture.state.outbox.takeable("s"),
        0,
        "held: the agent was just stopped"
    );
    // Told of the same cancel twice, nothing more is queued.
    fixture.state.write_dropped_messages_again("s");
    assert!(std::iter::from_fn(|| events.try_recv().ok())
        .all(|event| !matches!(event, Event::SessionNotice { .. })));
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// A launch of such an agent that is without the reports the declaration relies on (its hooks
/// switched off, so nothing reports a held message the turn took) has nothing queued again at a
/// cancel: a message the turn had taken would be written twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_without_its_prompt_reports_queues_nothing_again_at_a_cancel() {
    let _reader = declaring_screen(&DROPPING);
    let _without = declaring_launch_without_status_integration();
    let fixture = Fixture::new("dropped-launch-without-reports", AgentId::TEST);
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.queued_count("s"), 0);
}

/// Each message written during the turn of such a launch is told to the user, with its text, as
/// one that may not have been carried out, in the order written, and none is written again; a
/// second cancel report of the same turn tells nothing more.
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_without_its_prompt_reports_tells_each_message_a_cancel_may_have_dropped() {
    let _reader = declaring_screen(&DROPPING);
    let _without = declaring_launch_without_status_integration();
    let fixture = Fixture::new("maybe-dropped-told", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.queued_count("s"), 0);
    let told: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            Event::SessionNotice {
                session,
                code,
                params,
                ..
            } if session == "s" && code == notice_code::MESSAGE_MAYBE_DROPPED_AT_CANCEL => {
                params.get("detail").cloned()
            }
            _ => None,
        })
        .collect();
    assert_eq!(told, ["first", "second"]);
}

/// The session that sent a message such a cancel may have dropped is told so too, with its text,
/// through the channel a sender is told by (kept for it here, as it is interrupted); a message
/// Octoboard sent on its own tells nobody but the user.
#[tokio::test(flavor = "multi_thread")]
async fn the_sender_of_a_message_a_cancel_may_have_dropped_is_told() {
    let _reader = declaring_screen(&DROPPING);
    let _without = declaring_launch_without_status_integration();
    let fixture = Fixture::new("maybe-dropped-sender", AgentId::TEST);
    fixture
        .state
        .store
        .insert_session(&crate::protocol::Session {
            agent: AgentId::TEST,
            status: SessionStatus::Interrupted,
            ..crate::test_support::bare_session("sender", crate::protocol::SessionKind::Project)
        })
        .unwrap();
    // "first" is the queue's ticket 7, sent by the session; "second" is Octoboard's own.
    fixture.state.outbox.note_sender(7, "sender");
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");

    let state = fixture.state.clone();
    let kept = move || {
        state
            .store
            .kept_messages("sender")
            .unwrap()
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
    };
    let told = kept.clone();
    assert!(
        crate::test_support::within(PATIENCE, move || !told().is_empty()).await,
        "the sender was never told"
    );
    let kept = kept();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(
        kept[0].contains("may not have been carried out"),
        "{kept:?}"
    );
    assert!(kept[0].contains("session `s`"), "{kept:?}");
    assert!(kept[0].starts_with(NOTICE_PREFIX), "{kept:?}");
    assert!(kept[0].contains("It read:\n> first"), "{kept:?}");
    assert!(!kept[0].contains("second"), "{kept:?}");
}

/// A launch whose cancel drops what it held and whose reports tell what the turn took has its
/// messages queued again, with nothing told as possibly not carried out.
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_with_its_prompt_reports_tells_no_message_as_possibly_dropped() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("maybe-dropped-untold", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.take_queued("s"), ["first"]);
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(
            event,
            Event::SessionNotice { code, .. } if code == notice_code::MESSAGE_MAYBE_DROPPED_AT_CANCEL
        )
    });
    assert!(!told);
}

/// An agent that declares no such cancel has nothing queued again by a cancel report: the three
/// released agents among them.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_queues_nothing_again_for_an_agent_that_drops_nothing() {
    for (name, reader) in [("plain", &PLAIN), ("holding", &HOLDING)] {
        let _reader = declaring_screen(reader);
        let fixture = Fixture::new(&format!("dropped-undeclared-{name}"), AgentId::TEST);
        fixture.hold(&["first"]);
        fixture.cancelled_leaving(&[]);
        fixture.state.write_dropped_messages_again("s");
        assert_eq!(fixture.state.queued_count("s"), 0);
    }
}

/// A message the agent reported sending before the cancel had been taken into the turn: it is
/// not among the ones queued again.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_message_reported_sent_is_not_queued_again() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped-one-sent", AgentId::TEST);
    let live = fixture.live();
    fixture.hold(&["first", "second"]);
    fixture.state.note_prompt_sent(&live, Some("first"));
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.take_queued("s"), ["second"]);
}

/// A prompt the agent reports submitted inside the running turn, as a steered message the turn
/// takes is, joins that turn rather than beginning one: the message it names is taken, and one
/// still held in the same turn stays held, so a cancel then queues it again. Applied in the order
/// a hook's prompt is: the turn's start first, then the prompt counted sent.
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_submitted_inside_the_turn_takes_only_its_own_message() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped-steered", AgentId::TEST);
    let live = fixture.live();
    fixture.hold(&["first", "second"]);
    fixture.state.turn_started("s", Some("first"));
    fixture.state.note_prompt_sent(&live, Some("first"));
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.take_queued("s"), ["second"]);
}

/// A message queued again as dropped that the agent then reports sending, while it is still
/// held, had been taken after all: it leaves the queue, so it is never written a second time.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_queued_again_as_dropped_is_withdrawn_when_reported_sent() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped-then-sent", AgentId::TEST);
    let live = fixture.live();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    fixture.state.note_prompt_sent(&live, Some("second"));
    assert_eq!(fixture.state.queued_count("s"), 1);
    fixture.state.note_prompt_sent(&live, Some("unrelated"));
    assert_eq!(fixture.state.take_queued("s"), ["first"]);
}

/// A message dropped that cannot be queued again (the queue did not write it) is told to the
/// user with its text, under the code of a message dropped at a cancel: the agent gave nothing
/// back.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_dropped_that_cannot_be_written_again_is_told_to_the_user() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped-direct", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&ruled(&[]));
    live.settle_input();
    fixture.state.turn_started("s", None);
    fixture
        .state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    let framed = crate::session::term::frame_message(AgentId::TEST, "direct");
    live.write_message_input(&framed, "direct", false).unwrap();
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.queued_count("s"), 0);
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(
            event,
            Event::SessionNotice { session, code, message, .. }
                if session == "s"
                    && code == notice_code::MESSAGE_DROPPED_AT_CANCEL
                    && message.ends_with("direct")
        )
    });
    assert!(told, "the user is told what the message read");
}

/// An agent one of whose cancel keys gives the held messages back and another drops them: the
/// cancel report queues them again, and where the key was the one that gave them back, what the
/// box then shows is what is left of messages already queued again, cleared with nobody told and
/// never queued a second time.
#[tokio::test(flavor = "multi_thread")]
async fn messages_given_back_by_an_agent_that_can_also_drop_them_are_queued_once() {
    let _reader = declaring_screen(&EITHER);
    let fixture = Fixture::new("dropped-or-given-back", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&["first", "second"]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(fixture.state.queued_count("s"), 2);
    fixture.write_next("next", &["first", "second"]);
    assert!(cleared_notices(&mut events).is_empty());
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// What a provider whose hooks report a cancelled turn says of its events: `Cancelled` is the
/// cancel, and nothing else of its events is claimed.
struct CancelHooks;

impl crate::hooks::HookInterpreter for CancelHooks {
    fn past_startup(&self, _event: &str, _payload: &serde_json::Value) -> bool {
        true
    }

    fn turn_cancelled(&self, event: &str) -> bool {
        event == "Cancelled"
    }
}

static CANCEL_HOOKS: CancelHooks = CancelHooks;

impl Fixture {
    /// The hook event of a cancelled turn, applied the way the hook route applies it: queued,
    /// then drained.
    fn hook_cancel(&self) {
        self.state.hook_queues.push(
            "s",
            bytes::Bytes::from_static(br#"{"hook_event_name": "Cancelled"}"#),
        );
        crate::hooks::apply::drain_hook_queue(&self.state, "s");
    }

    /// The session's program reporting `report` (`working`, `idle`), with every report it made
    /// applied.
    async fn reports(&self, report: &str) {
        self.live
            .on_output(format!("\x1b]7501;state={report}\x07").as_bytes());
        self.live.settle_program_status().await;
    }
}

/// A cancel a hook reports, for a turn that was open, has the messages the agent dropped with
/// it queued again in their order: the call the hook route makes, not the queueing alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_a_hook_reports_queues_the_dropped_messages_again() {
    let _reader = declaring_screen(&DROPPING);
    let _hooks = declaring_hooks(&CANCEL_HOOKS);
    let fixture = Fixture::new("dropped-by-hook", AgentId::TEST);
    fixture.hold(&["first", "second"]);
    fixture.hook_cancel();
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// A cancel a hook reports with no turn open drops nothing of a turn, so nothing is queued
/// again.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_a_hook_reports_with_no_turn_open_queues_nothing_again() {
    let _reader = declaring_screen(&DROPPING);
    let _hooks = declaring_hooks(&CANCEL_HOOKS);
    let fixture = Fixture::new("dropped-by-hook-no-turn", AgentId::TEST);
    fixture.hold(&["first"]);
    fixture.state.abandon_turn("s");
    fixture.hook_cancel();
    assert_eq!(fixture.state.queued_count("s"), 0);
}

/// A session whose status comes from its program's reports reads `idle` straight after
/// `working`, in a turn that was open, as the turn cancelled: the messages the agent dropped
/// with it are queued again in their order.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_the_status_reports_show_queues_the_dropped_messages_again() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped-by-reports", AgentId::TEST);
    fixture.hold(&["first", "second"]);
    fixture.reports("working").await;
    fixture.reports("idle").await;
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// The same reports with no turn open end no turn, and queue nothing again.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_the_status_reports_show_with_no_turn_open_queues_nothing_again() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped-by-reports-no-turn", AgentId::TEST);
    fixture.hold(&["first"]);
    fixture.reports("working").await;
    fixture.state.abandon_turn("s");
    fixture.reports("idle").await;
    assert_eq!(fixture.state.queued_count("s"), 0);
}

/// The reports that declare a cancel beside the provider's own status integration
/// (`ReportedSignals::cancel`) close the turn the hooks opened, and the messages the agent
/// dropped with it are queued again in their order, as after a cancel a hook reports.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_the_declared_signals_report_queues_the_dropped_messages_again() {
    let _reader = declaring_screen(&DROPPING);
    let _signals = declaring_reported_signals(DECLARED_CANCEL);
    let fixture = Fixture::new("dropped-by-declared-signals", AgentId::TEST);
    fixture.hold(&["first", "second"]);
    fixture.reports("working").await;
    fixture.reports("idle").await;
    let state = fixture.state.clone();
    assert!(
        crate::test_support::within(PATIENCE, move || state.queued_count("s") == 2).await,
        "the declared cancel queued nothing again"
    );
    assert!(!fixture.state.turn_is_open("s"));
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// What a declared cancel does with the messages the agent dropped is done before the session
/// leaves `Working`: an interrupt returns once it has, and its caller must find them accounted
/// for. Seen through a message the queue did not write, told as not carried out ahead of the
/// session's move to its prompt.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_the_declared_signals_report_settles_the_dropped_messages_before_the_status() {
    let _reader = declaring_screen(&DROPPING);
    let _signals = declaring_reported_signals(DECLARED_CANCEL);
    let fixture = Fixture::new("dropped-by-declared-signals-order", AgentId::TEST);
    let live = fixture.live();
    fixture.hold(&[]);
    fixture.reports("working").await;
    let framed = crate::session::term::frame_message(AgentId::TEST, "direct");
    live.write_message_input(&framed, "direct", false).unwrap();
    let mut events = fixture.state.subscribe();
    fixture.reports("idle").await;
    let settled = live.clone();
    assert!(
        crate::test_support::within(PATIENCE, move || settled.signals.cancels_settled() == 1).await,
        "the declared cancel was never acted on"
    );
    let order: Vec<&str> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            Event::SessionNotice { session, code, .. }
                if session == "s" && code == notice_code::MESSAGE_DROPPED_AT_CANCEL =>
            {
                Some("told")
            }
            Event::SessionUpserted { session }
                if session.id == "s" && session.status != SessionStatus::Working =>
            {
                Some("moved")
            }
            _ => None,
        })
        .collect();
    assert_eq!(order.first(), Some(&"told"), "{order:?}");
    assert!(order.contains(&"moved"), "{order:?}");
}

/// A declared cancel with no turn open moves the session to its prompt and queues nothing
/// again.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_the_declared_signals_report_with_no_turn_open_queues_nothing_again() {
    let _reader = declaring_screen(&DROPPING);
    let _signals = declaring_reported_signals(DECLARED_CANCEL);
    let fixture = Fixture::new("dropped-by-declared-signals-no-turn", AgentId::TEST);
    fixture.hold(&["first"]);
    fixture.reports("working").await;
    fixture.state.abandon_turn("s");
    fixture.reports("idle").await;
    let live = fixture.live();
    assert!(
        crate::test_support::within(PATIENCE, move || live.signals.cancels_settled() == 1).await,
        "the declared cancel was never acted on"
    );
    assert_eq!(fixture.state.queued_count("s"), 0);
}

/// Reports that say a cancel beside the provider's own status integration, and no wait.
const DECLARED_CANCEL: crate::session::ReportedSignals = crate::session::ReportedSignals {
    wait_on_user: false,
    cancel: true,
};

/// An agent that can both give the held messages back and drop them, whose box after the cancel
/// report holds the messages that report queued again with text of the user's after them: only
/// that text is told as a draft, never the messages, which are written again once.
#[tokio::test(flavor = "multi_thread")]
async fn text_after_messages_already_queued_again_is_the_only_draft_told() {
    let _reader = declaring_screen(&EITHER);
    let fixture = Fixture::new("dropped-then-typed", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&["first", "second", "my own"]);
    fixture.state.write_dropped_messages_again("s");
    fixture.write_next("next", &["first", "second", "my own"]);
    let notices = cleared_notices(&mut events);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let (message, params) = &notices[0];
    assert!(!*message, "told as a draft: {params:?}");
    assert_eq!(params["detail"], "my own");
    assert_eq!(fixture.state.take_queued("s"), ["first", "second"]);
}

/// A message given back that a write past the grace clears and cannot queue again is told once,
/// as cleared after the cancel: the cancel reported after that, by an agent that can also drop
/// what it held, does not tell it again as dropped.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_told_as_cleared_is_not_told_again_as_dropped() {
    let _reader = declaring_screen(&EITHER);
    let fixture = Fixture::new("cleared-then-dropped", AgentId::TEST);
    let live = fixture.live();
    let mut events = fixture.state.subscribe();
    live.on_output(&ruled(&[]));
    live.settle_input();
    fixture.state.turn_started("s", None);
    fixture
        .state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    // Not the queue's: it cannot be queued again.
    let framed = crate::session::term::frame_message(AgentId::TEST, "direct");
    live.write_message_input(&framed, "direct", false).unwrap();
    live.on_output(&ruled(&[]));
    live.settle_input();
    // The cancel key put it back; the report of the cancel has not come yet.
    live.on_output(&ruled(&["direct"]));
    live.settle_input();
    fixture.write_next("next", &["direct"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");

    let told: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            Event::SessionNotice { code, message, .. } if message.ends_with("direct") => Some(code),
            _ => None,
        })
        .collect();
    assert_eq!(
        told,
        [notice_code::MESSAGE_CLEARED_AFTER_CANCEL],
        "{told:?}"
    );
}

impl Fixture {
    /// A session `sender`, interrupted, so that what it is told is kept for it.
    fn interrupted_sender(&self) {
        self.state
            .store
            .insert_session(&crate::protocol::Session {
                agent: AgentId::TEST,
                status: SessionStatus::Interrupted,
                ..crate::test_support::bare_session("sender", crate::protocol::SessionKind::Project)
            })
            .unwrap();
    }

    /// The turn ends, not cancelled, and the grace after it has passed: what is left untaken is
    /// told now.
    fn turn_ends_past_its_grace(&self) {
        self.state.watch_untaken_messages("s");
        self.state
            .tell_untaken_messages("s", std::time::Instant::now());
    }

    /// The texts of the `message_not_taken` notices about `s` sent so far.
    fn not_taken(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<String> {
        std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event {
                Event::SessionNotice {
                    session,
                    code,
                    params,
                    ..
                } if session == "s" && code == notice_code::MESSAGE_NOT_TAKEN => {
                    params.get("detail").cloned()
                }
                _ => None,
            })
            .collect()
    }
}

/// For an agent whose writes are confirmed by its report of each prompt, a message written while
/// the turn ran that it never reported taking is told to the user as not taken once the turn has
/// ended, and to the session that sent it; one it reported taking mid-turn is not.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_no_turn_took_is_told_to_the_user_and_its_sender() {
    let _reader = declaring_screen(&PLAIN);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("not-taken-told", AgentId::TEST);
    fixture.interrupted_sender();
    let mut events = fixture.state.subscribe();
    fixture.state.outbox.note_sender(7, "sender");
    fixture.hold(&["first", "second"]);
    fixture.live().note_prompt_sent(Some("second"));
    fixture.turn_ends_past_its_grace();

    assert_eq!(Fixture::not_taken(&mut events), ["first"]);
    let state = fixture.state.clone();
    let kept = move || {
        state
            .store
            .kept_messages("sender")
            .unwrap()
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
    };
    let told = kept.clone();
    assert!(
        crate::test_support::within(PATIENCE, move || !told().is_empty()).await,
        "the sender was never told"
    );
    let kept = kept();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(
        kept[0].contains("did not take it before the turn ended"),
        "{kept:?}"
    );
    assert!(kept[0].starts_with(NOTICE_PREFIX), "{kept:?}");
    assert!(kept[0].contains("It read:\n> first"), "{kept:?}");
    // Told once: a second look finds nothing left.
    fixture
        .state
        .tell_untaken_messages("s", std::time::Instant::now());
    assert!(Fixture::not_taken(&mut events).is_empty());
}

/// Claude Code's shape: the agent reports a message it queues mid-turn as it queues it, so every
/// message written into the turn that it queued is never told as not taken, whatever comes after.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_reported_as_queued_mid_turn_is_never_told_as_not_taken() {
    let _reader = declaring_screen(&PLAIN);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("not-taken-queued-mid-turn", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first", "second"]);
    fixture.live().note_prompt_sent(Some("first"));
    fixture.live().note_prompt_sent(Some("second"));
    fixture.turn_ends_past_its_grace();
    assert!(Fixture::not_taken(&mut events).is_empty());
}

/// OpenCode's shape: a message written as the turn ends is taken by a turn of its own after the
/// idle, which reports it then; reported within the grace, it is never told as not taken.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_taken_as_a_turn_of_its_own_after_the_end_is_not_told() {
    let _reader = declaring_screen(&PLAIN);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("not-taken-own-turn", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first"]);
    fixture.state.watch_untaken_messages("s");
    // The next turn opens with it, after the end.
    fixture.state.turn_started("s", Some("first"));
    fixture.live().note_prompt_sent(Some("first"));
    fixture
        .state
        .tell_untaken_messages("s", std::time::Instant::now());
    assert!(Fixture::not_taken(&mut events).is_empty());
}

/// An agent whose writes are not confirmed by reported prompts has nothing told at a turn's end:
/// nothing would ever report the messages it took.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_not_confirming_by_reported_prompt_tells_nothing_at_a_turn_end() {
    let _reader = declaring_screen(&PLAIN);
    let fixture = Fixture::new("not-taken-undeclared", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first"]);
    fixture.turn_ends_past_its_grace();
    assert!(Fixture::not_taken(&mut events).is_empty());
}

/// A turn closed by its end through the path every turn end takes
/// (`crate::messaging::reporting::close_turn_and_report`) has the messages written during it that
/// the agent never reported taking told as not taken once the grace after it has passed.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_closed_by_its_end_has_its_untaken_messages_told() {
    let _reader = declaring_screen(&PLAIN);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let grace = GraceOverride::held();
    let fixture = Fixture::new("not-taken-closed-turn", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first"]);
    crate::messaging::reporting::close_turn_and_report(
        &fixture.state,
        "s",
        crate::hooks::TurnEnd {
            last_assistant_message: None,
            failed: false,
            backstop: false,
        },
    );
    assert!(
        Fixture::not_taken(&mut events).is_empty(),
        "not before the grace"
    );
    grace.release();
    assert!(grace.told_within(PATIENCE), "the wait never told");
    assert_eq!(Fixture::not_taken(&mut events), ["first"]);
}

/// The wait after a turn's end tells, once, only what that end found untaken: a message a later
/// end found waits for that end's own grace, however long the earlier wait outlasts it.
#[tokio::test(flavor = "multi_thread")]
async fn the_wait_after_a_turn_end_tells_only_what_that_end_found_once() {
    let _reader = declaring_screen(&PLAIN);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("not-taken-own-end", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold_from(7, &["first"]);
    let first = GraceOverride::held();
    fixture.state.watch_untaken_messages("s");
    // The next turn's end, whose wait is held for as long as the test runs.
    fixture.hold_from(8, &["second"]);
    let second = GraceOverride::held();
    fixture.state.watch_untaken_messages("s");

    first.release();
    assert!(first.told_within(PATIENCE), "the first wait never told");
    // That wait has told all it will: the signal comes once it is done.
    assert_eq!(Fixture::not_taken(&mut events), ["first"]);
    // What the later end found is still kept for its own wait.
    second.release();
    assert!(second.told_within(PATIENCE), "the second wait never told");
    assert_eq!(Fixture::not_taken(&mut events), ["second"]);
}

impl Fixture {
    /// What the interrupted session `sender` has been told, as kept for it.
    fn kept_for_sender(&self) -> Vec<String> {
        self.state
            .store
            .kept_messages("sender")
            .unwrap()
            .into_iter()
            .map(|(_, text)| text)
            .collect()
    }

    /// What the session `sender` has been told once `count` notes have come, in the order kept.
    async fn sender_told(&self, count: usize) -> Vec<String> {
        let state = self.state.clone();
        let came = crate::test_support::within(PATIENCE, move || {
            state.store.kept_messages("sender").unwrap().len() >= count
        })
        .await;
        let kept = self.kept_for_sender();
        assert!(
            came,
            "the sender was told {} of {count}: {kept:?}",
            kept.len()
        );
        kept
    }

    /// The `detail` of each withdrawal of a notice with `code` about `s` sent so far.
    fn withdrawn(
        events: &mut tokio::sync::broadcast::Receiver<Event>,
        code: crate::protocol::Code,
    ) -> Vec<Option<String>> {
        std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event {
                Event::SessionNoticeWithdrawn {
                    session,
                    code: withdrawn,
                    detail,
                } if session == "s" && withdrawn == code => Some(detail),
                _ => None,
            })
            .collect()
    }

    /// The `detail` of each notice with `code` about `s` sent so far.
    fn notices(
        events: &mut tokio::sync::broadcast::Receiver<Event>,
        code: crate::protocol::Code,
    ) -> Vec<String> {
        std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event {
                Event::SessionNotice {
                    session,
                    code: told,
                    params,
                    ..
                } if session == "s" && told == code => params.get("detail").cloned(),
                _ => None,
            })
            .collect()
    }
}

/// Qwen Code's shape on a launch without its prompt reports: one cancel key gives the held
/// messages back and nothing tells what the turn took. The messages told as possibly not carried
/// out are handled: what the box then shows of them is cleared with nobody told, never written
/// again and never told as a draft, whether one message or several came back.
#[tokio::test(flavor = "multi_thread")]
async fn messages_told_at_a_cancel_and_given_back_are_cleared_as_handled() {
    for (name, held) in [
        ("one", &["first"][..]),
        ("several", &["first", "second"][..]),
    ] {
        let _reader = declaring_screen(&EITHER);
        let _without = declaring_launch_without_status_integration();
        let fixture = Fixture::new(&format!("unseen-given-back-{name}"), AgentId::TEST);
        let mut events = fixture.state.subscribe();
        fixture.hold(held);
        fixture.cancelled_leaving(held);
        fixture.state.write_dropped_messages_again("s");
        let told = Fixture::notices(&mut events, notice_code::MESSAGE_MAYBE_DROPPED_AT_CANCEL);
        assert_eq!(told, held, "{name}");
        fixture.write_next("next", held);
        let notices = cleared_notices(&mut events);
        assert!(notices.is_empty(), "{name}: told again: {notices:?}");
        assert!(
            fixture.state.take_queued("s").is_empty(),
            "{name}: written again"
        );
    }
}

/// A sender of several messages a cancel may have dropped gets one note naming each of them, in
/// order, as a notice of Octoboard's own.
#[tokio::test(flavor = "multi_thread")]
async fn a_sender_of_several_messages_gets_one_note() {
    let _reader = declaring_screen(&DROPPING);
    let _without = declaring_launch_without_status_integration();
    let fixture = Fixture::new("maybe-dropped-one-note", AgentId::TEST);
    fixture.interrupted_sender();
    fixture.state.outbox.note_sender(7, "sender");
    fixture.state.outbox.note_sender(8, "sender");
    fixture.hold(&["first", "second"]);
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    let kept = fixture.sender_told(1).await;
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].starts_with(NOTICE_PREFIX), "{kept:?}");
    assert!(
        kept[0].contains("They read:\n> first\n\n> second"),
        "{kept:?}"
    );
}

/// A message a cancel dropped that cannot be written again is told to its sender as well as the
/// user.
#[tokio::test(flavor = "multi_thread")]
async fn the_sender_of_a_message_dropped_at_a_cancel_and_lost_is_told() {
    let _reader = declaring_screen(&DROPPING);
    let fixture = Fixture::new("dropped-lost-sender", AgentId::TEST);
    fixture.interrupted_sender();
    let mut events = fixture.state.subscribe();
    fixture.state.outbox.note_sender(7, "sender");
    fixture.hold(&["first"]);
    // The queue no longer knows the message it wrote, so it cannot write it again.
    fixture.state.outbox.forget("s");
    fixture.cancelled_leaving(&[]);
    fixture.state.write_dropped_messages_again("s");
    assert_eq!(
        Fixture::notices(&mut events, notice_code::MESSAGE_DROPPED_AT_CANCEL),
        ["first"]
    );
    let kept = fixture.sender_told(1).await;
    assert!(kept[0].contains("dropped it unsent"), "{kept:?}");
    assert!(kept[0].contains("It read:\n> first"), "{kept:?}");
}

/// A message told as not taken that the agent then reports sending after all has its own notice
/// withdrawn, by its text, and its sender told that it ran; another message's notice stands.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_report_withdraws_the_not_taken_notice_and_tells_the_sender() {
    let _reader = declaring_screen(&PLAIN);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("not-taken-late", AgentId::TEST);
    fixture.interrupted_sender();
    let mut events = fixture.state.subscribe();
    fixture.state.outbox.note_sender(7, "sender");
    fixture.hold(&["first", "second"]);
    fixture.turn_ends_past_its_grace();
    assert_eq!(Fixture::not_taken(&mut events), ["first", "second"]);
    fixture.sender_told(1).await;

    fixture
        .state
        .note_prompt_sent(&fixture.live(), Some("first"));
    assert_eq!(
        Fixture::withdrawn(&mut events, notice_code::MESSAGE_NOT_TAKEN),
        [Some("first".to_string())]
    );
    let kept = fixture.sender_told(2).await;
    assert!(kept[1].contains("ran after all"), "{kept:?}");
    assert!(kept[1].contains("It read:\n> first"), "{kept:?}");
    // Found once: a second report withdraws nothing more.
    fixture
        .state
        .note_prompt_sent(&fixture.live(), Some("first"));
    assert!(Fixture::withdrawn(&mut events, notice_code::MESSAGE_NOT_TAKEN).is_empty());
}

/// What a turn's end left untaken is told at once when the session's process ends within the
/// grace, rather than forgotten with it.
#[tokio::test(flavor = "multi_thread")]
async fn untaken_messages_are_told_at_once_when_the_process_ends() {
    let _reader = declaring_screen(&PLAIN);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("not-taken-process-ends", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    fixture.hold(&["first"]);
    let _grace = GraceOverride::held();
    fixture.state.watch_untaken_messages("s");
    assert!(Fixture::not_taken(&mut events).is_empty());
    crate::coordinator::exit_watch::end_process_for_test(&fixture.state, "s");
    assert_eq!(Fixture::not_taken(&mut events), ["first"]);
}

/// Hooks whose `Prompt` event reports the prompt sent, and which take a prompt quoting a task
/// notification for one the agent sent itself, as Claude Code's do.
struct PromptHooks;

impl crate::hooks::HookInterpreter for PromptHooks {
    fn past_startup(&self, _event: &str, _payload: &serde_json::Value) -> bool {
        true
    }

    fn status(&self, event: &str, _payload: &serde_json::Value) -> Option<SessionStatus> {
        (event == "Prompt").then_some(SessionStatus::Working)
    }

    fn starts_conversation(&self, event: &str, _payload: &serde_json::Value) -> bool {
        event == "Prompt"
    }

    fn submitted_prompt<'a>(&self, payload: &'a serde_json::Value) -> Option<&'a str> {
        payload.get("prompt").and_then(serde_json::Value::as_str)
    }

    fn is_self_sent(&self, prompt: &str) -> bool {
        prompt.contains("<task-notification>")
    }
}

static PROMPT_HOOKS: PromptHooks = PromptHooks;

/// A message Octoboard wrote that quotes what the agent's own prompts look like is still taken
/// off what is untaken when the agent reports sending it: it is that message, whatever it quotes.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_quoting_a_self_sent_prompt_is_taken_when_reported() {
    let _reader = declaring_screen(&PLAIN);
    let _hooks = declaring_hooks(&PROMPT_HOOKS);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("not-taken-quoting", AgentId::TEST);
    let mut events = fixture.state.subscribe();
    let quoting = "a prompt opens with <task-notification>";
    fixture.hold(&[quoting]);
    let grace = GraceOverride::held();
    fixture.state.watch_untaken_messages("s");
    let payload = serde_json::json!({"hook_event_name": "Prompt", "prompt": quoting});
    fixture
        .state
        .hook_queues
        .push("s", bytes::Bytes::from(payload.to_string()));
    crate::hooks::apply::drain_hook_queue(&fixture.state, "s");
    grace.release();
    assert!(grace.told_within(PATIENCE), "the wait never ended");
    assert!(Fixture::not_taken(&mut events).is_empty());
}

/// A message written into the session that the agent does not report sending, with its box empty
/// after, went elsewhere: the user is told with its text, and its sender, answered before the
/// drain met it, is told too. The message behind it does not wait on its text as on a draft. A
/// late report of it withdraws that notice and tells the sender it ran after all. Drained on the
/// test's thread, where the reader is declared: the drains the runtime starts read the plain
/// provider.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_gone_elsewhere_is_told_to_its_sender_and_withdrawn_by_a_late_report() {
    let _reader = declaring_screen(&ELSEWHERE);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("went-elsewhere-sender", AgentId::TEST);
    fixture.interrupted_sender();
    let live = fixture.live();
    live.on_output(&ruled(&[]));
    live.settle_input();
    *fixture.state.send_wait.lock().unwrap() = Some(Duration::ZERO);
    let mut events = fixture.state.subscribe();
    let ticket = fixture
        .state
        .queue_message("s", "please do the next step")
        .unwrap();
    fixture.state.outbox.note_sender(ticket, "sender");
    assert_eq!(
        fixture.state.flush_outbox("s", SessionStatus::Idle),
        crate::messaging::outbox::Drain::WentElsewhere(ticket)
    );
    assert_eq!(
        Fixture::notices(&mut events, notice_code::MESSAGE_NOT_DELIVERED),
        ["please do the next step"]
    );
    let kept = fixture.sender_told(1).await;
    assert!(kept[0].contains("was not delivered"), "{kept:?}");
    // The next message goes in as into an empty box, not held as behind a draft.
    assert!(
        !live.draft.lock().unwrap().holds(),
        "the typing record still holds the message, which would hold the next as a draft"
    );

    fixture
        .state
        .note_prompt_sent(&live, Some("please do the next step"));
    assert_eq!(
        Fixture::withdrawn(&mut events, notice_code::MESSAGE_NOT_DELIVERED),
        [Some("please do the next step".to_string())]
    );
    let kept = fixture.sender_told(2).await;
    assert!(kept[1].contains("ran after all"), "{kept:?}");
}

/// The writer waiting on the drain that meets its message gone elsewhere is answered with the
/// error of that code, carrying the message's text.
#[tokio::test(flavor = "multi_thread")]
async fn a_writer_waiting_on_a_message_gone_elsewhere_gets_the_error_alone() {
    let _reader = declaring_screen(&ELSEWHERE);
    let _confirms = crate::providers::test_provider::with_confirmation_by_reported_prompt();
    let fixture = Fixture::new("went-elsewhere-writer", AgentId::TEST);
    let live = fixture.live();
    live.on_output(&ruled(&[]));
    live.settle_input();
    *fixture.state.send_wait.lock().unwrap() = Some(Duration::ZERO);
    // On the test's thread, where the reader is declared.
    let error = crate::messaging::delivery::write_message_from(
        &fixture.state,
        "s",
        "please do the next step",
        crate::messaging::delivery::WhenBlocked::Queue,
        Some("sender"),
    )
    .expect_err("not delivered");
    let coded = error
        .downcast_ref::<crate::protocol::CodedError>()
        .expect("a coded error");
    assert_eq!(
        coded.code,
        crate::protocol::error_code::MESSAGE_NOT_DELIVERED
    );
    assert_eq!(coded.params["detail"], "please do the next step");
}
