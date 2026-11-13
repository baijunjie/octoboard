//! The draft hold read off the screen, through the session's writes: what the keys typed leave in
//! the box, whether the frame it is read off is whole, and the checks past the draft's grace.

use crate::platform::term::Input;

/// Moves the latest key and the latest output back as far as the empty-box release waits on
/// them: as if the agent had been still since well after it drew what the key did.
fn settle(live: &crate::session::LiveSession) {
    live.settle_input();
}

/// Draws `screen` as the whole of the agent's screen.
fn draw(live: &crate::session::LiveSession, screen: &[u8]) {
    let mut bytes = b"\x1b[2J\x1b[H".to_vec();
    bytes.extend_from_slice(screen);
    live.on_output(&bytes);
}

/// A capture's text as the bytes that draw it.
fn drawn(capture: &str) -> Vec<u8> {
    capture.trim_end().replace('\n', "\r\n").into_bytes()
}

/// The screen of `agent` with its input box empty, as measured.
fn empty_box(agent: crate::protocol::AgentId) -> Vec<u8> {
    use crate::protocol::AgentId;
    use crate::session::input_box::captures::{CLEARED, GROK_CLEARED};
    match agent {
        AgentId::CLAUDE => drawn(CLEARED),
        AgentId::GROK => drawn(GROK_CLEARED),
        _ => include_bytes!("../../../testdata/codex_fresh_empty.bin").to_vec(),
    }
}

/// Keys that leave the input box empty — a lone Esc, text typed and cleared again, text moved
/// through and cut, a prompt sent whose hook named no text it was typed as — hold no
/// message once the agent has drawn the box empty after them, for every agent.
#[test]
fn keys_that_leave_the_box_empty_hold_no_message_once_it_is_drawn_empty() {
    use crate::protocol::AgentId;
    for agent in [AgentId::CLAUDE, AgentId::GROK, AgentId::CODEX] {
        for keys in [&b"\x1b"[..], b"abc\x15", b"abc\x1b[D\x17", b"typed\r"] {
            let live = crate::test_support::fake_live_session(
                "typed-then-empty",
                agent,
                140,
                45,
                "sleep 30",
            )
            .with_composer();
            draw(&live, &empty_box(agent));
            live.write_terminal_input(keys).unwrap();
            draw(&live, &empty_box(agent));
            assert_eq!(
                live.input_box(&live.screen_text_undimmed()).as_deref(),
                Some("")
            );
            settle(&live);
            let before = live.input_writes();
            live.write_message_input(&[Input::Text("peer".into())], "peer", false)
                .unwrap_or_else(|error| panic!("{agent:?} after {keys:?}: {}", error.error));
            assert_eq!(live.input_writes(), before + 1);
        }
    }
}

/// The prompt Claude Code puts back in its box when the user's Esc cancels a turn holds a
/// message as a draft does, until the user clears it and the box is drawn empty.
#[test]
fn a_prompt_an_escape_put_back_holds_a_message_until_it_is_cleared() {
    use crate::protocol::AgentId;
    let live = crate::test_support::fake_live_session(
        "restored-then-cleared",
        AgentId::CLAUDE,
        140,
        45,
        "sleep 30",
    )
    .with_composer();
    draw(&live, &empty_box(AgentId::CLAUDE));
    live.write_terminal_input(b"\x1b").unwrap();
    draw(
        &live,
        include_bytes!("../../../testdata/claude_input_restored.bin"),
    );
    settle(&live);
    let before = live.input_writes();
    let error = live
        .write_message_input(&[Input::Text("peer".into())], "peer", false)
        .unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(live.input_writes(), before);
    live.write_terminal_input(b"\x15").unwrap();
    draw(&live, &empty_box(AgentId::CLAUDE));
    settle(&live);
    let before = live.input_writes();
    live.write_message_input(&[Input::Text("peer".into())], "peer", false)
        .unwrap();
    assert_eq!(live.input_writes(), before + 1);
}

/// A message queued while the user's keys hold the session is written by the session's watch
/// at its next look once the box is drawn empty after them, not at its retry two seconds on.
/// The clock is paused, so the watch's looks come in a few milliseconds of real time, while the
/// retry waits two seconds of it.
#[tokio::test(start_paused = true)]
async fn the_watch_writes_a_message_held_for_typing_once_the_box_is_drawn_empty() {
    use crate::protocol::{AgentId, SessionStatus};
    use crate::session::input_box::captures::DRAFT;
    let (state, dir) = crate::test_support::app_state("draft-release-watch");
    crate::state::test_support::insert_record(&state, &dir, "s", SessionStatus::Working);
    let live = crate::test_support::fake_live_session("s", AgentId::CLAUDE, 140, 45, "sleep 30")
        .with_composer();
    state.register_live(live.clone());
    draw(&live, &empty_box(AgentId::CLAUDE));
    live.write_terminal_input(b"draft text").unwrap();
    draw(&live, &drawn(DRAFT));
    assert_ne!(
        live.input_box(&live.screen_text_undimmed()).as_deref(),
        Some("")
    );
    state.queue_message("s", "peer message");
    crate::session::dialogs::watch(&state, &live);
    live.write_terminal_input(b"\x15").unwrap();
    draw(&live, &empty_box(AgentId::CLAUDE));
    settle(&live);
    // The release and the write it starts run in blocking tasks, which hold the clock still
    // until they are done, so each look the watch makes is over by the next one.
    for _ in 0..4 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if state.outbox.len("s") == 0 {
            break;
        }
    }
    assert_eq!(
        state.outbox.len("s"),
        0,
        "the box drawn empty did not release the message at the watch's next look"
    );
    assert!(!live.draft.lock().unwrap().holds());
}

/// A frame read in part can show the box cleared and not yet drawn again: inside an open
/// synchronized update an empty box holds a message over a draft whose text the mirror cannot
/// know, and once the update ends the box as drawn decides.
#[test]
fn an_empty_box_inside_a_frame_still_being_drawn_holds_a_message() {
    use crate::protocol::AgentId;
    use crate::session::input_box::captures::{CLEARED, DRAFT};
    for ends_empty in [false, true] {
        let live = crate::test_support::fake_live_session(
            "torn-frame",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        draw(&live, &empty_box(AgentId::CLAUDE));
        // Text typed, then moved through: the mirror no longer knows what the box holds.
        live.write_terminal_input(b"draft text\x1b[D").unwrap();
        draw(&live, &drawn(DRAFT));
        let mut cleared = b"\x1b[?2026h\x1b[2J\x1b[H".to_vec();
        cleared.extend(drawn(CLEARED));
        live.on_output(&cleared);
        settle(&live);
        assert_eq!(
            live.input_box(&live.screen_text_undimmed()).as_deref(),
            Some("")
        );
        let before = live.input_writes();
        let error = live
            .write_message_input(&[Input::Text("peer".into())], "peer", false)
            .unwrap_err();
        assert_eq!(error.written, 0);
        assert_eq!(live.input_writes(), before);
        let mut rest = b"\x1b[2J\x1b[H".to_vec();
        rest.extend(drawn(if ends_empty { CLEARED } else { DRAFT }));
        rest.extend(b"\x1b[?2026l");
        live.on_output(&rest);
        settle(&live);
        let written = live.write_message_input(&[Input::Text("peer".into())], "peer", false);
        assert_eq!(written.is_ok(), ends_empty, "{written:?}");
    }
}

/// A key that may put text in the box the mirror cannot know — a prompt from history, a
/// completion — holds a message over an empty box drawn after it, for every agent: the text it
/// brings may be drawn later still.
#[test]
fn a_history_or_completion_key_holds_a_message_over_an_empty_box() {
    use crate::protocol::AgentId;
    for agent in [AgentId::CLAUDE, AgentId::GROK, AgentId::CODEX] {
        for keys in [&b"\x1b[A"[..], b"\t", b"\x1b[A\x15"] {
            let live =
                crate::test_support::fake_live_session("inserting-key", agent, 140, 45, "sleep 30")
                    .with_composer();
            draw(&live, &empty_box(agent));
            live.write_terminal_input(keys).unwrap();
            draw(&live, &empty_box(agent));
            settle(&live);
            let before = live.input_writes();
            let error = live
                .write_message_input(&[Input::Text("peer".into())], "peer", false)
                .unwrap_err();
            assert_eq!(error.written, 0, "{agent:?} after {keys:?}");
            assert!(
                live.draft.lock().unwrap().holds(),
                "{agent:?} after {keys:?}"
            );
            assert_eq!(live.input_writes(), before);
        }
    }
}

/// After Up and Ctrl-U a first message waits for the grace, and once it is written a second one
/// on the same empty box goes in at once.
#[test]
fn a_message_written_after_a_history_key_frees_the_next_one() {
    use crate::protocol::AgentId;
    let live = crate::test_support::fake_live_session(
        "after-up-message",
        AgentId::CLAUDE,
        140,
        45,
        "sleep 30",
    )
    .with_composer();
    draw(&live, &empty_box(AgentId::CLAUDE));
    live.write_terminal_input(b"\x1b[A").unwrap();
    live.write_terminal_input(b"\x15").unwrap();
    draw(&live, &empty_box(AgentId::CLAUDE));
    settle(&live);
    let before = live.input_writes();
    assert!(live
        .write_message_input(&[Input::Text("first".into())], "first", false)
        .is_err());
    live.write_message_input(&[Input::Text("first".into())], "first", true)
        .unwrap();
    draw(&live, &empty_box(AgentId::CLAUDE));
    settle(&live);
    live.write_message_input(&[Input::Text("second".into())], "second", false)
        .unwrap();
    assert_eq!(live.input_writes(), before + 2);
}

/// An Esc typed while the hooks report a turn running is no Esc at the prompt, though the title
/// still shows the prompt, as right after a submit: it waits for output like any other key.
#[test]
fn a_lone_escape_while_the_hooks_report_a_turn_waits_for_output() {
    use crate::protocol::{AgentId, SessionStatus};
    let (state, dir) = crate::test_support::app_state("escape-hooks-working");
    crate::state::test_support::insert_record(&state, &dir, "s", SessionStatus::Working);
    let live = crate::test_support::fake_live_session("s", AgentId::CLAUDE, 140, 45, "sleep 30")
        .with_composer();
    state.register_live(live.clone());
    draw(&live, &empty_box(AgentId::CLAUDE));
    live.write_terminal_input(b"\x1b").unwrap();
    settle(&live);
    let before = live.input_writes();
    assert!(live
        .write_message_input(&[Input::Text("peer".into())], "peer", false)
        .is_err());
    assert_eq!(live.input_writes(), before);
    draw(&live, &empty_box(AgentId::CLAUDE));
    settle(&live);
    live.write_message_input(&[Input::Text("peer".into())], "peer", false)
        .unwrap();
}

/// Past the draft's grace a box read empty off a frame still being drawn holds a message, and
/// one read off the whole frame takes it.
#[test]
fn past_the_grace_an_empty_box_inside_a_frame_still_being_drawn_holds_a_message() {
    use crate::protocol::AgentId;
    use crate::session::input_box::captures::CLEARED;
    let live = crate::test_support::fake_live_session(
        "grace-torn-frame",
        AgentId::CLAUDE,
        140,
        45,
        "sleep 30",
    )
    .with_composer();
    let mut cleared = b"\x1b[?2026h\x1b[2J\x1b[H".to_vec();
    cleared.extend(drawn(CLEARED));
    live.on_output(&cleared);
    settle(&live);
    let before = live.input_writes();
    assert!(live
        .write_message_input(&[Input::Text("peer".into())], "peer", true)
        .is_err());
    assert_eq!(live.input_writes(), before);
    live.on_output(b"\x1b[?2026l");
    settle(&live);
    live.write_message_input(&[Input::Text("peer".into())], "peer", true)
        .unwrap();
    assert_eq!(live.input_writes(), before + 1);
}

/// Past the draft's grace a message still waits while the box cannot be read after a key that
/// may have brought back text the mirror cannot know, and goes in once the box can be read.
#[test]
fn past_the_grace_an_unreadable_box_after_a_history_key_holds_a_message() {
    use crate::protocol::AgentId;
    let live = crate::test_support::fake_live_session(
        "unreadable-after-up",
        AgentId::CLAUDE,
        140,
        45,
        "sleep 30",
    )
    .with_composer();
    draw(&live, &empty_box(AgentId::CLAUDE));
    live.write_terminal_input(b"\x1b[A").unwrap();
    draw(&live, b"a screen with no input box on it");
    settle(&live);
    let before = live.input_writes();
    let error = live
        .write_message_input(&[Input::Text("peer".into())], "peer", true)
        .unwrap_err();
    assert_eq!(error.written, 0);
    assert_eq!(live.input_writes(), before);
    draw(&live, &empty_box(AgentId::CLAUDE));
    settle(&live);
    live.write_message_input(&[Input::Text("peer".into())], "peer", true)
        .unwrap();
    assert_eq!(live.input_writes(), before + 1);
}

/// A lone Esc at an empty box that Claude Code answers by printing nothing, as 2.1.322 at times
/// does, holds no message once the Esc has settled; one typed while the title showed a turn
/// running still waits for output, since the prompt the cancel puts back can come later.
#[test]
fn a_lone_escape_the_agent_printed_nothing_for_holds_no_message_at_the_prompt() {
    use crate::protocol::AgentId;
    for running in [false, true] {
        let live = crate::test_support::fake_live_session(
            "silent-escape",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        if running {
            live.on_output("\x1b]0;\u{25D0} Working\x07".as_bytes());
        }
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b").unwrap();
        settle(&live);
        let before = live.input_writes();
        let written = live.write_message_input(&[Input::Text("peer".into())], "peer", false);
        assert_eq!(written.is_ok(), !running, "{written:?}");
        assert_eq!(live.input_writes(), before + u64::from(!running));
        if running {
            assert!(live.draft.lock().unwrap().holds());
        }
    }
}

/// An empty box read off a screen the agent has not drawn since the user's last key does not
/// show what the key did, and one drawn empty although the user typed text and nothing that
/// could have removed it may be a screen drawn before the text was: either holds a message.
#[test]
fn an_empty_box_that_may_predate_the_keys_holds_a_message() {
    use crate::protocol::AgentId;
    for (keys, redrawn) in [(&b"ab\x1b[D"[..], false), (b"draft", true)] {
        let live = crate::test_support::fake_live_session(
            "stale-empty-box",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(keys).unwrap();
        if redrawn {
            draw(&live, &empty_box(AgentId::CLAUDE));
        }
        settle(&live);
        let before = live.input_writes();
        let error = live
            .write_message_input(&[Input::Text("peer".into())], "peer", false)
            .unwrap_err();
        assert_eq!(error.written, 0, "{keys:?}");
        assert!(live.draft.lock().unwrap().holds(), "{keys:?}");
        assert_eq!(live.input_writes(), before);
    }
}

/// An Esc typed after a running title the agent stopped sending long ago is still typed mid-turn:
/// the cancel it may be puts the prompt back after any output, so the message waits for output,
/// until the title goes back to the prompt.
#[test]
fn a_lone_escape_after_a_running_title_gone_stale_still_waits_for_output() {
    use crate::protocol::AgentId;
    use crate::session::title_activity::Shown;
    for back_at_prompt in [false, true] {
        let live = crate::test_support::fake_live_session(
            "stale-title-escape",
            AgentId::CLAUDE,
            140,
            45,
            "sleep 30",
        )
        .with_composer();
        // Far past the window a running title counts as still being sent in.
        let sent = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(3600))
            .expect("a clock that reaches back that far");
        live.title
            .lock()
            .unwrap()
            .activity
            .note(Some(Shown::Running), sent);
        if back_at_prompt {
            live.on_output("\x1b]0;\u{2733} Claude Code\x07".as_bytes());
        }
        draw(&live, &empty_box(AgentId::CLAUDE));
        live.write_terminal_input(b"\x1b").unwrap();
        settle(&live);
        let before = live.input_writes();
        let written = live.write_message_input(&[Input::Text("peer".into())], "peer", false);
        assert_eq!(written.is_ok(), back_at_prompt, "{written:?}");
        assert_eq!(live.input_writes(), before + u64::from(back_at_prompt));
        if !back_at_prompt {
            assert!(live.draft.lock().unwrap().holds());
            draw(&live, &empty_box(AgentId::CLAUDE));
            settle(&live);
            live.write_message_input(&[Input::Text("peer".into())], "peer", false)
                .unwrap();
        }
    }
}

/// What a message past the draft's grace is refused with for a box read empty off a frame not
/// drawn whole.
const TORN_REFUSAL: &str = "off a frame not yet drawn whole";

/// A session whose screen was left inside an open synchronized update that shows its input box
/// empty.
fn torn_session(name: &str) -> crate::test_support::StandIn {
    use crate::protocol::AgentId;
    use crate::session::input_box::captures::CLEARED;
    let live = crate::test_support::fake_live_session(name, AgentId::CLAUDE, 140, 45, "sleep 30")
        .with_composer();
    let mut cleared = b"\x1b[?2026h\x1b[2J\x1b[H".to_vec();
    cleared.extend(drawn(CLEARED));
    live.on_output(&cleared);
    live
}

/// Has the agent open a synchronized update as of now, and be quiet since, so that a reading right
/// after finds the frame not drawn whole whatever time has passed in the test's own steps.
fn keep_torn(live: &crate::session::LiveSession) {
    {
        let mut pace = live.draft_hold.output_pace.lock().unwrap();
        *pace = crate::session::output_pace::OutputPace::default();
        pace.note(b"\x1b[?2026h", std::time::Instant::now());
    }
    settle(live);
}

/// The refusal past the draft's grace as a message sent now would meet it.
fn refusal_now(live: &crate::session::LiveSession) -> Option<&'static str> {
    let draft = live.draft.lock().unwrap();
    live.past_grace_refusal(&draft)
}

fn refused_as_torn(live: &crate::session::LiveSession) -> bool {
    refusal_now(live).is_some_and(|refusal| refusal.contains(TORN_REFUSAL))
}

/// Moves the stamps of the run of refusals going on back, as if it had begun `first` earlier and
/// the latest refusal in it had come `last` earlier.
fn age_run(
    live: &crate::session::LiveSession,
    first: std::time::Duration,
    last: std::time::Duration,
) {
    let mut torn = live.draft_hold.torn_since.lock().unwrap();
    let (since, latest) = (*torn).expect("a run of refusals is going on");
    *torn = Some((
        since
            .checked_sub(first)
            .expect("a clock that reaches back that far"),
        latest
            .checked_sub(last)
            .expect("a clock that reaches back that far"),
    ));
}

fn run_going_on(live: &crate::session::LiveSession) -> bool {
    live.draft_hold.torn_since.lock().unwrap().is_some()
}

/// Past the grace a box read empty off a frame not drawn whole holds a message, retry after retry,
/// only for the wait; once the run of retries has lasted that long the message goes in on that
/// reading, and the run is kept, so that a write that fails for another reason waits no longer,
/// until the run ends.
#[test]
fn past_the_grace_an_empty_box_off_a_torn_frame_holds_a_message_for_the_wait_alone() {
    let wait = super::TORN_FRAME_WAIT;
    let live = torn_session("torn-wait");
    for _ in 0..3 {
        keep_torn(&live);
        assert!(refused_as_torn(&live), "inside the wait");
    }
    assert!(run_going_on(&live));

    age_run(&live, wait / 2, std::time::Duration::ZERO);
    keep_torn(&live);
    assert!(refused_as_torn(&live), "the run has lasted half the wait");

    age_run(&live, wait, std::time::Duration::ZERO);
    for _ in 0..2 {
        keep_torn(&live);
        assert_eq!(refusal_now(&live), None, "the run has lasted the wait");
        assert!(run_going_on(&live), "the run is kept until it is ended");
    }

    live.end_torn_run();
    keep_torn(&live);
    assert!(refused_as_torn(&live), "the next such reading starts a run");
}

/// The same through the write: refused inside the wait, written once the run has lasted it, which
/// ends the run.
#[test]
fn past_the_grace_a_message_goes_in_on_a_torn_frame_once_the_retries_have_lasted_the_wait() {
    let live = torn_session("torn-write");
    keep_torn(&live);
    let before = live.input_writes();
    let error = live
        .write_message_input(&[Input::Text("peer".into())], "peer", true)
        .unwrap_err();
    assert_eq!(error.written, 0);
    assert!(error.error.to_string().contains(TORN_REFUSAL), "{error}");
    assert_eq!(live.input_writes(), before);
    assert!(run_going_on(&live));

    age_run(&live, super::TORN_FRAME_WAIT, std::time::Duration::ZERO);
    keep_torn(&live);
    live.write_message_input(&[Input::Text("peer".into())], "peer", true)
        .unwrap();
    assert_eq!(live.input_writes(), before + 1);
    assert!(!run_going_on(&live), "the message written ended the run");
}

/// The retries of a held message come on the session watch's period, two seconds, and later where
/// the host is busy: they are one run, so the message goes in once the run has lasted the wait.
#[test]
fn refusals_on_the_watch_s_retry_period_make_one_run_that_lasts_the_wait() {
    use std::time::Duration;
    for (period, refusals) in [
        (Duration::from_secs(1), 2),
        (Duration::from_secs(2), 1),
        (Duration::from_millis(2500), 1),
        (Duration::from_secs(9), 1),
    ] {
        let live = torn_session("torn-cadence");
        keep_torn(&live);
        assert!(refused_as_torn(&live), "{period:?}: the first reading");
        let mut refused = 1;
        for _ in 0..5 {
            // A retry one period on.
            age_run(&live, period, period);
            keep_torn(&live);
            match refusal_now(&live) {
                None => break,
                Some(refusal) => {
                    assert!(refusal.contains(TORN_REFUSAL), "{period:?}");
                    refused += 1;
                }
            }
        }
        assert_eq!(refused, refusals, "{period:?}");
        assert!(run_going_on(&live), "{period:?}");
    }
}

/// A refusal as long after the last one as the run's gap is not a retry of the same message: it
/// starts a new run, which gets its whole wait; one just inside the gap continues the run.
#[test]
fn a_torn_frame_refusal_long_after_the_last_one_starts_a_new_run() {
    use std::time::Duration;
    let (wait, gap) = (super::TORN_FRAME_WAIT, super::TORN_RUN_GAP);
    let live = torn_session("torn-gap");
    keep_torn(&live);
    assert!(refused_as_torn(&live));

    // Just inside the gap: the run goes on, and has lasted the wait.
    age_run(&live, gap * 2, gap - Duration::from_secs(1));
    keep_torn(&live);
    assert_eq!(refusal_now(&live), None, "the run continued");

    // The gap itself: the whole run happened long ago.
    age_run(&live, gap * 3, gap);
    keep_torn(&live);
    assert!(refused_as_torn(&live), "no instant pass after a long gap");
    keep_torn(&live);
    assert!(refused_as_torn(&live), "the new run is inside its wait");

    age_run(&live, wait, Duration::ZERO);
    keep_torn(&live);
    assert_eq!(refusal_now(&live), None, "the new run has lasted the wait");
}

/// A reading that is no torn frame ends the run: a box read off a whole frame, or a box that is
/// not empty; a torn frame afterwards has the whole wait again.
#[test]
fn a_reading_that_is_not_a_torn_frame_ends_the_run() {
    use crate::protocol::AgentId;
    use crate::session::input_box::captures::DRAFT;
    let wait = super::TORN_FRAME_WAIT;
    for whole_frame in [true, false] {
        let live = torn_session("torn-ended");
        keep_torn(&live);
        assert!(refused_as_torn(&live));
        age_run(&live, wait * 2, std::time::Duration::ZERO);
        assert!(run_going_on(&live));

        if whole_frame {
            live.on_output(b"\x1b[?2026l");
        } else {
            keep_torn(&live);
            draw(&live, &drawn(DRAFT));
        }
        settle(&live);
        assert_eq!(refusal_now(&live), None, "whole frame: {whole_frame}");
        assert!(!run_going_on(&live), "whole frame: {whole_frame}");

        // A run that had lasted the wait would have let this one in.
        draw(&live, &empty_box(AgentId::CLAUDE));
        keep_torn(&live);
        assert!(refused_as_torn(&live), "whole frame: {whole_frame}");
    }
}

/// A refusal for another reason, a box that cannot be read after a key that may have put text in
/// it, neither starts a run nor ends the one going on.
#[test]
fn a_refusal_for_another_reason_neither_starts_nor_ends_the_torn_frame_run() {
    use crate::protocol::AgentId;
    let unreadable = |live: &crate::session::LiveSession| {
        live.write_terminal_input(b"\x1b[A").unwrap();
        draw(live, b"a screen with no input box on it");
        settle(live);
        let refusal = refusal_now(live).expect("refused");
        assert!(refusal.contains("cannot be read"), "{refusal}");
    };

    let live = torn_session("torn-other-none");
    draw(&live, &empty_box(AgentId::CLAUDE));
    unreadable(&live);
    assert!(!run_going_on(&live), "no run was started");

    let live = torn_session("torn-other-going");
    keep_torn(&live);
    assert!(refused_as_torn(&live));
    let run = *live.draft_hold.torn_since.lock().unwrap();
    unreadable(&live);
    assert_eq!(*live.draft_hold.torn_since.lock().unwrap(), run);
}
