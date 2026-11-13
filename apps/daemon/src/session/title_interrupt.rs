//! Reading a cancelled turn off the agent's terminal title (`title_activity`), for the cancels that
//! leave no other trace.
//!
//! **Claude Code.** Esc pressed before the model has shown anything — while the request waits for
//! its response, or the model is still thinking — fires no hook, and on 2.1.310 writes nothing to
//! the transcript either: the prompt entry written at submit stays the last user entry, and Claude
//! Code puts the prompt's text back in its input box. So the interrupt watch of
//! `crate::adapter::claude::transcript_watch` never sees it, and the session would read `Working`
//! until its next prompt. What does change is the terminal title: Claude Code starts it with a
//! turning half-filled circle (`◐`, `◑`) while a turn runs and with `✳` when it is back at its
//! prompt, and the glyph turns to `✳` within a tenth of a second of the Esc. From 2.1.322, once the
//! terminal answers its OSC 7501 support query, it drops the glyphs and reports `working` and
//! `idle` instead, the `idle` about 80 ms after the Esc, which the title activity takes in their
//! place (`title_activity`). See `docs/agent-cli-reference.md`, section "## Claude Code".
//!
//! **Grok Build.** Ctrl-C pressed before the first token arrives fires no hook either (1.0.70,
//! against a stand-in inference server): no `StopCancelled`, no `Stop`, and the prompt goes back
//! into the composer. Its title loses the braille spinner it starts with while a turn runs within a
//! few tens of milliseconds, and keeps it through tool calls. Left alone, the session reads
//! `Working` until Grok's `idle_prompt` backstop about a minute later, which then finds the turn
//! open and quiet and has a report synthesised for a turn the user cancelled. So a Grok turn whose
//! title goes back to the prompt is closed as cancelled, not only moved to `Idle`; and since the
//! title is no hook, a `Stop` that still comes for that turn closes it as its own end after all
//! ([`AppState::end_turn_off_title`]): Grok's hooks are commands, and a slow start of one can bring
//! a turn's `Stop` after the title that follows it. See `docs/agent-cli-reference.md`, section
//! "## Grok Build".
//!
//! The title is back at the prompt when every turn ends too, and for Claude Code whenever a
//! permission prompt or a question waits on the user, tens of milliseconds after the hook that
//! says so was posted, and those hooks move the session out of `Working` by themselves. So a title
//! back at the prompt only ends the turn once it has held for [`SETTLE`], the session's queue of
//! hook events has drained, and the session still reads `Working`: a turn that went back to its
//! prompt with no hook. And it counts only if it follows a running title that was current when the
//! watch was armed or came after, so the prompt title left by the previous turn does not end this
//! one.
//!
//! A cancel this close to the prompt's submission can come before the watch is armed, which happens
//! when the daemon applies the hook event that started the turn; the title has then already turned,
//! and the session keeps reading `Working`, as it did before this watch existed. So does a session
//! whose title the agent has been told not to set and that reports no status, a release that
//! changes the glyphs, and a running frame and the prompt title after it read from the PTY in one
//! chunk, of which only the last title is seen. A prompt title while one of
//! `crate::session::dialogs`' dialogs is up is a turn waiting on the user, with no hook to say so,
//! and is not counted either.
//!
//! Codex reports its own cancels (`Interrupt`), so its title is not watched for one.

use std::sync::Arc;
use std::time::Duration;

use super::screen::TitleCancel;
use crate::protocol::SessionStatus;
use crate::records::watch::back_at_prompt;
use crate::state::AppState;

/// How long the title must have read as back at the prompt before the turn counts as ended without
/// a hook. Far longer than the hooks that come with the same title take to arrive, short enough
/// that the session follows a cancel in about a second.
const SETTLE: Duration = Duration::from_secs(1);

#[cfg(test)]
thread_local! {
    static SETTLE_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

/// Lets a test drive the watch's polling clock without waiting on its wall-clock title stamps.
/// The watch captures this on its caller's thread before it spawns.
#[cfg(test)]
pub(crate) fn without_settle() -> impl Drop {
    struct Reset(Option<Duration>);
    impl Drop for Reset {
        fn drop(&mut self) {
            SETTLE_OVERRIDE.with(|value| value.set(self.0));
        }
    }
    Reset(SETTLE_OVERRIDE.with(|value| value.replace(Some(Duration::ZERO))))
}

fn settle() -> Duration {
    #[cfg(test)]
    if let Some(value) = SETTLE_OVERRIDE.with(std::cell::Cell::get) {
        return value;
    }
    SETTLE
}

/// How often the watch looks at the title.
const POLL: Duration = Duration::from_millis(250);

/// Runs the watch through its next polling opportunities on the runtime's controlled clock.
#[cfg(test)]
pub(crate) async fn polls_settled() {
    crate::test_support::runtime_advanced(POLL * 2).await;
}

/// Starts watching the terminal title of `session_id`, a session a hook has just moved into
/// `Working` whose provider leaves some cancels in the title alone
/// ([`crate::session::screen::ScreenReader::title_cancel`]), for its turn ending with no hook.
/// `generation` is the record watch generation armed for the same move (for Claude Code, the
/// transcript interrupt watch's): the next prompt retires it, and with it this watch, before its
/// turn opens, and the session is put back at its prompt only while it is still current. Spawned,
/// never awaited.
pub fn watch(state: &Arc<AppState>, session_id: &str, generation: u64) {
    let Some(live) = state.live_session(session_id) else {
        return;
    };
    let Some(cancel) = crate::adapter::provider(live.agent).screen().title_cancel() else {
        return;
    };
    let settle = settle();
    let armed = live.title_activity().stops();
    let live = Arc::downgrade(&live);
    let state = state.clone();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(POLL).await;
            if !state.record_watch_current(&session_id, generation) {
                return;
            }
            // A hand raised by a session request comes down to `Working` with no hook to arm
            // another watch, so this one waits it out; one raised by a hook comes down with a hook
            // that retires this watch and arms another, which counts only what the title shows
            // from then on. Anything else ends the turn.
            let Some(live_now) = live.upgrade() else {
                return; // The process is gone.
            };
            let reading = live_now.turn_reading();
            match reading.hooks() {
                Some(SessionStatus::Working) => {}
                Some(SessionStatus::WaitingUser) | None => continue,
                Some(_) => return,
            }
            // Whether the title has held at the prompt for the settle with no dialog up. The
            // reading and the session are let go before the wait below, which they cannot be held
            // across.
            let settled = reading.settled_at_prompt(armed, settle);
            drop(reading);
            drop(live_now);
            if !settled {
                continue;
            }
            // A hook posted with the title may still be waiting to be applied; whatever it says
            // about the session wins over the title.
            state.hook_queues.settled(&session_id).await;
            let still = live
                .upgrade()
                .is_some_and(|live| live.turn_reading().settled_at_prompt(armed, settle));
            if !still {
                continue;
            }
            // Conditional on the session still reading `Working`: one a hook moved on, the turn
            // ending as usual or a prompt waiting on the user, is left where the hook put it.
            match cancel {
                TitleCancel::CloseAsCancelled => {
                    cancelled_before_output(&state, &session_id, generation)
                }
                TitleCancel::BackAtPrompt => back_at_prompt(
                    &state,
                    &session_id,
                    generation,
                    SessionStatus::Working,
                    "cancel in the terminal title",
                ),
            }
            return;
        }
    });
}

/// Puts a session whose title went back to the prompt with no hook at its prompt, and closes its
/// turn as cancelled ([`TitleCancel::CloseAsCancelled`], Grok Build), while watch `generation` is
/// still current and the session still reads `Working`.
fn cancelled_before_output(state: &Arc<AppState>, session_id: &str, generation: u64) {
    // One step under the watch's bookkeeping, for the reason `back_at_prompt` gives; the turn is
    // closed in the same step, so a prompt landing just after cannot have its own turn closed
    // instead.
    let moved = state.while_record_watch_current(session_id, generation, || {
        let moved = state.report_paused_if(session_id, SessionStatus::Working);
        if matches!(moved, Ok(true)) {
            state.end_turn_off_title(session_id);
        }
        moved
    });
    match moved {
        Some(Ok(true)) => {
            tracing::debug!(
                session = %session_id,
                "the terminal title went back to the prompt with no hook; the turn was cancelled"
            );
            // Nothing else is coming to release a message queued while the turn ran.
            state.spawn_flush_outbox(session_id, SessionStatus::Idle);
        }
        Some(Ok(false)) | None => {
            tracing::debug!(
                session = %session_id,
                "the terminal title went back to the prompt, but the session had already moved on"
            );
        }
        Some(Err(err)) => {
            tracing::debug!(
                session = %session_id, %err,
                "returning the session to its prompt after its title went back to it failed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentId;
    use crate::protocol::Session;
    use crate::session::title_activity::Shown;
    use crate::session::title_activity::TitleActivity;
    use crate::session::LiveSession;
    use crate::state::TurnClose;
    use crate::test_support::{
        app_state, console, console_session, idle_stand_in, idle_stand_in_of, output_containing,
        within, ScratchDir, StandIn,
    };
    use std::time::Instant;

    /// Whether `activity` went back to the prompt after `armed`, the count when the watch was
    /// armed, and has stayed there for [`SETTLE`] by `now`.
    fn settled_at_prompt(activity: &TitleActivity, armed: u64, now: Instant) -> bool {
        activity.settled_at_prompt(armed, now, SETTLE)
    }

    const WORKING: Option<Shown> = Some(Shown::Running);
    const TURNING: Option<Shown> = Some(Shown::Running);
    const IDLE: Option<Shown> = Some(Shown::AtPrompt);

    /// Captures from Claude Code 2.1.310; see `testdata/claude-activity-titles.md`.
    const IDLE_TITLE: &[u8] = include_bytes!("../../testdata/claude_title_idle.bin");
    const CANCEL_TITLES: &[u8] = include_bytes!("../../testdata/claude_title_cancel.bin");
    const PERMISSION_TITLES: &[u8] = include_bytes!("../../testdata/claude_title_permission.bin");

    /// A title Claude Code 2.1.322 sends once the terminal answers its Program Status Protocol
    /// query, and the reports it makes of its turn instead of the glyphs.
    const GLYPHLESS_TITLE: &[u8] = b"\x1b]0;Claude Code\x07";
    const REPORTS_WORKING: &[u8] = b"\x1b]7501;state=working:app=claude-code\x1b\\";
    const REPORTS_IDLE: &[u8] = b"\x1b]7501;state=idle:app=claude-code\x1b\\";
    const REPORTS_BLOCKED: &[u8] = b"\x1b]7501;state=blocked:app=claude-code:kind=permission\x1b\\";

    /// Captures from Grok Build 1.0.70; see `testdata/grok-activity-titles.md`.
    const GROK_CANCEL_TITLES: &[u8] =
        include_bytes!("../../testdata/grok_title_cancel_before_output.bin");
    const GROK_TURN_END_TITLES: &[u8] = include_bytes!("../../testdata/grok_title_turn_end.bin");
    const GROK_TOOL_TITLES: &[u8] = include_bytes!("../../testdata/grok_title_tool.bin");

    #[test]
    fn a_turn_counts_as_stopped_once_the_title_has_held_at_the_prompt() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        let armed = activity.stops();
        activity.note(WORKING, start);
        activity.note(TURNING, start);
        assert!(!settled_at_prompt(&activity, armed, start + SETTLE));
        activity.note(IDLE, start);
        assert!(!settled_at_prompt(&activity, armed, start + SETTLE / 2));
        assert!(settled_at_prompt(&activity, armed, start + SETTLE));
    }

    #[test]
    fn the_previous_turn_s_idle_title_does_not_end_this_one() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note(IDLE, start);
        activity.note(WORKING, start);
        activity.note(IDLE, start);
        // Armed with the title back at the prompt: only a turn seen running from here on counts.
        let armed = activity.stops();
        activity.note(IDLE, start);
        assert!(!settled_at_prompt(&activity, armed, start + SETTLE * 10));
        activity.note(WORKING, start);
        activity.note(IDLE, start);
        assert!(settled_at_prompt(&activity, armed, start + SETTLE));
    }

    #[test]
    fn a_turn_already_running_when_armed_counts() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note(WORKING, start);
        let armed = activity.stops();
        activity.note(IDLE, start);
        assert!(settled_at_prompt(&activity, armed, start + SETTLE));
    }

    #[test]
    fn running_again_before_the_settle_runs_out_is_not_a_stop() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        let armed = activity.stops();
        activity.note(WORKING, start);
        activity.note(IDLE, start);
        activity.note(WORKING, start + SETTLE / 2);
        assert!(!settled_at_prompt(&activity, armed, start + SETTLE * 10));
    }

    struct Fixture {
        live: StandIn,
        state: Arc<AppState>,
        _dir: ScratchDir,
    }

    fn fixture(name: &str, status: SessionStatus) -> Fixture {
        let (state, dir) = app_state(&format!("title-interrupt-{name}"));
        state.store.insert_console(&console(&dir)).expect("console");
        state
            .store
            .insert_session(&console_session("s", status))
            .expect("session");
        let live = idle_stand_in("s");
        state.register_live(live.clone());
        Fixture {
            live,
            state,
            _dir: dir,
        }
    }

    /// A Grok Build session working on a turn that has started, with its output read, so what is
    /// written into it can be waited for.
    fn grok_fixture(name: &str) -> Fixture {
        let (state, dir) = app_state(&format!("title-interrupt-{name}"));
        state.store.insert_console(&console(&dir)).expect("console");
        state
            .store
            .insert_session(&Session {
                agent: AgentId::GROK,
                ..console_session("s", SessionStatus::Working)
            })
            .expect("session");
        let live = idle_stand_in_of("s", AgentId::GROK);
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        state.turn_started("s", None);
        Fixture {
            live,
            state,
            _dir: dir,
        }
    }

    /// Feeds `titles` one byte at a time, so each title completes in its own chunk as it did on
    /// the PTY.
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
            .map(|s| s.status)
    }

    async fn reaches_within(state: &Arc<AppState>, status: SessionStatus, wait: Duration) -> bool {
        let state = state.clone();
        within(wait, move || self::status(&state) == Some(status)).await
    }

    async fn reaches(state: &Arc<AppState>, status: SessionStatus) -> bool {
        reaches_within(state, status, crate::test_support::PATIENCE).await
    }

    fn armed(fixture: &Fixture) -> u64 {
        let generation = fixture.state.begin_record_watch("s");
        let _settle = without_settle();
        watch(&fixture.state, "s", generation);
        generation
    }

    /// A turn cancelled before the model showed anything, which only the title shows, puts the
    /// session back at its prompt.
    #[tokio::test]
    async fn a_cancel_only_the_title_shows_puts_the_session_at_its_prompt() {
        let fixture = fixture("cancel", SessionStatus::Working);
        armed(&fixture);
        replay(&fixture.live, CANCEL_TITLES);
        assert!(reaches(&fixture.state, SessionStatus::Idle).await);
        assert!(
            fixture.state.last_turn_abnormal("s"),
            "a cancelled turn leaves it paused"
        );
    }

    /// Claude Code 2.1.322 titles itself with no glyph and reports its state through OSC 7501: a
    /// turn it reports `working` and then `idle`, as an Esc before any output makes it, puts the
    /// session back at its prompt as the glyphs turning to `✳` do.
    #[tokio::test]
    async fn a_cancel_only_the_reports_show_puts_the_session_at_its_prompt() {
        let fixture = fixture("cancel-reported", SessionStatus::Working);
        armed(&fixture);
        fixture.live.on_output(GLYPHLESS_TITLE);
        fixture.live.on_output(REPORTS_WORKING);
        fixture.live.on_output(REPORTS_IDLE);
        assert!(reaches(&fixture.state, SessionStatus::Idle).await);
        assert!(
            fixture.state.last_turn_abnormal("s"),
            "a cancelled turn leaves it paused"
        );
    }

    /// A turn the agent reports running, and goes on running, ends nothing.
    #[tokio::test]
    async fn a_turn_reported_working_is_left_running() {
        let fixture = fixture("reported-working", SessionStatus::Working);
        armed(&fixture);
        fixture.live.on_output(GLYPHLESS_TITLE);
        fixture.live.on_output(REPORTS_WORKING);
        polls_settled().await;
        assert_eq!(status(&fixture.state), Some(SessionStatus::Working));
    }

    /// The idle report the previous turn left is no cancel of this one.
    #[tokio::test]
    async fn the_idle_report_the_previous_turn_left_ends_nothing() {
        let fixture = fixture("reported-previous", SessionStatus::Working);
        fixture.live.on_output(REPORTS_WORKING);
        fixture.live.on_output(REPORTS_IDLE);
        armed(&fixture);
        fixture.live.on_output(REPORTS_IDLE);
        polls_settled().await;
        assert_eq!(status(&fixture.state), Some(SessionStatus::Working));
    }

    /// A report of `blocked` is the agent waiting on the user, which the hook that says so, applied
    /// within the settle, keeps up; the watch waits while the session waits for the user.
    #[tokio::test]
    async fn a_blocked_report_with_a_hook_waiting_on_the_user_does_not_end_the_turn() {
        let fixture = fixture("reported-blocked", SessionStatus::Working);
        armed(&fixture);
        fixture.live.on_output(REPORTS_WORKING);
        fixture.live.on_output(REPORTS_BLOCKED);
        assert!(fixture
            .state
            .apply_reported_status_if("s", SessionStatus::Working, SessionStatus::WaitingUser)
            .unwrap());
        polls_settled().await;
        assert_eq!(status(&fixture.state), Some(SessionStatus::WaitingUser));
        assert!(
            !fixture.state.last_turn_abnormal("s"),
            "a write that moved nothing leaves no mark"
        );
    }

    /// The same cancel read off the parsed screen's titles, as a host that renders what the program
    /// draws has them read, puts the session back at its prompt as well.
    #[tokio::test]
    async fn a_cancel_only_the_parsed_screen_s_title_shows_puts_the_session_at_its_prompt() {
        let fixture = fixture("cancel-rendered", SessionStatus::Working);
        fixture.live.read_as_rendered();
        fixture.live.on_output(b"$ ");
        armed(&fixture);
        replay(&fixture.live, CANCEL_TITLES);
        assert!(reaches(&fixture.state, SessionStatus::Idle).await);
        assert!(fixture.state.last_turn_abnormal("s"));
    }

    /// The title left at the prompt by the turn before is no cancel of this one.
    #[tokio::test]
    async fn the_title_the_previous_turn_left_ends_nothing() {
        let fixture = fixture("previous", SessionStatus::Working);
        replay(&fixture.live, CANCEL_TITLES);
        armed(&fixture);
        replay(&fixture.live, IDLE_TITLE);
        polls_settled().await;
        assert_ne!(status(&fixture.state), Some(SessionStatus::Idle));
    }

    /// The title is back at the prompt while a permission dialog waits as well; the hook that
    /// raised the hand, applied within the settle, keeps it up.
    #[tokio::test]
    async fn a_session_a_hook_moved_on_stays_where_it_was_put() {
        let fixture = fixture("moved-on", SessionStatus::Working);
        armed(&fixture);
        replay(&fixture.live, PERMISSION_TITLES);
        assert!(fixture
            .state
            .apply_reported_status_if("s", SessionStatus::Working, SessionStatus::WaitingUser)
            .unwrap());
        polls_settled().await;
        assert_ne!(status(&fixture.state), Some(SessionStatus::Idle));
        assert_eq!(status(&fixture.state), Some(SessionStatus::WaitingUser));
        assert!(
            !fixture.state.last_turn_abnormal("s"),
            "a write that moved nothing leaves no mark"
        );
    }

    /// A hook event still waiting to be applied may say where the session went; the title waits
    /// for it.
    #[tokio::test]
    async fn a_hook_event_still_queued_is_waited_for() {
        let fixture = fixture("queued", SessionStatus::Working);
        armed(&fixture);
        fixture
            .state
            .hook_queues
            .push("s", bytes::Bytes::from_static(b"{}"));
        replay(&fixture.live, CANCEL_TITLES);
        polls_settled().await;
        assert_ne!(
            status(&fixture.state),
            Some(SessionStatus::Idle),
            "nothing may move while an event is queued"
        );
        fixture.state.hook_queues.complete("s");
        assert!(reaches(&fixture.state, SessionStatus::Idle).await);
    }

    /// A dialog no hook reports is the turn waiting on the user, whatever the title says.
    #[tokio::test]
    async fn a_dialog_on_the_screen_keeps_the_turn_running() {
        let fixture = fixture("dialog", SessionStatus::Working);
        armed(&fixture);
        fixture.live.on_output(
            "We're changing auto mode.\r\nEnter to continue \u{B7} Esc to cancel\r\n".as_bytes(),
        );
        assert!(fixture.live.look_for_dialog().current.is_some());
        replay(&fixture.live, CANCEL_TITLES);
        polls_settled().await;
        assert_ne!(status(&fixture.state), Some(SessionStatus::Idle));
    }

    /// A watch the next prompt retired does not end the turn that prompt opened.
    #[tokio::test]
    async fn a_retired_watch_ends_nothing() {
        let fixture = fixture("retired", SessionStatus::Working);
        armed(&fixture);
        fixture.state.end_record_watch("s");
        replay(&fixture.live, CANCEL_TITLES);
        polls_settled().await;
        assert_ne!(status(&fixture.state), Some(SessionStatus::Idle));
    }

    /// Ctrl-C before Grok Build's first token, the user's own or `interrupt_session`'s, fires no
    /// hook: the title alone puts the session at its prompt and closes the turn as cancelled, so
    /// the `idle_prompt` backstop a minute later owes no report, and a message held meanwhile goes
    /// in.
    #[tokio::test]
    async fn a_grok_cancel_before_the_first_token_closes_the_turn_with_no_report() {
        let fixture = grok_fixture("grok-cancel");
        fixture.state.queue_message("s", "held while working");
        armed(&fixture);
        replay(&fixture.live, GROK_CANCEL_TITLES);
        assert!(reaches(&fixture.state, SessionStatus::Idle).await);
        assert!(
            fixture.state.last_turn_abnormal("s"),
            "a cancelled turn leaves it paused"
        );
        assert_eq!(fixture.state.close_turn("s", true), TurnClose::NoTurn);
        let heard = output_containing(&fixture.live, "held while working");
        assert!(heard.contains("held while working"), "{heard}");
    }

    /// Grok's hooks are commands, so a turn's `Stop` can come after the title that follows it; it
    /// still closes the turn as its end, and the report it owes is made.
    #[tokio::test]
    async fn a_stop_that_comes_after_the_title_closed_the_turn_is_its_end() {
        let fixture = grok_fixture("grok-late-stop");
        armed(&fixture);
        replay(&fixture.live, GROK_CANCEL_TITLES);
        assert!(reaches(&fixture.state, SessionStatus::Idle).await);
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::OwesReport);
        assert_eq!(fixture.state.close_turn("s", true), TurnClose::NoTurn);
    }

    /// A Grok turn that ended as usual has its `Stop` in the queue when the title loses its
    /// spinner; the title waits for it, and the turn is the `Stop`'s to close.
    #[tokio::test]
    async fn a_grok_turn_that_ended_with_its_stop_is_left_to_the_stop() {
        let fixture = grok_fixture("grok-turn-end");
        armed(&fixture);
        fixture
            .state
            .hook_queues
            .push("s", bytes::Bytes::from_static(b"{}"));
        replay(&fixture.live, GROK_TURN_END_TITLES);
        polls_settled().await;
        assert_ne!(
            status(&fixture.state),
            Some(SessionStatus::Idle),
            "nothing may move while an event is queued"
        );
        assert_eq!(fixture.state.close_turn("s", false), TurnClose::OwesReport);
        fixture
            .state
            .apply_hook_status("s", SessionStatus::Idle)
            .unwrap();
        fixture.state.hook_queues.complete("s");
        assert!(reaches(&fixture.state, SessionStatus::Idle).await);
        assert_eq!(fixture.state.close_turn("s", true), TurnClose::NoTurn);
    }

    /// Grok keeps its spinner while a tool runs, so a long tool call is no cancel.
    #[tokio::test]
    async fn a_grok_turn_running_a_tool_keeps_running() {
        let fixture = grok_fixture("grok-tool");
        armed(&fixture);
        replay(&fixture.live, GROK_TOOL_TITLES);
        polls_settled().await;
        assert_ne!(status(&fixture.state), Some(SessionStatus::Idle));
    }
}
