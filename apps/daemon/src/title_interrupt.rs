//! Reading a cancelled Claude Code turn off its terminal title, for the one cancel that leaves no
//! other trace.
//!
//! Esc pressed before the model has shown anything — while the request waits for its response, or
//! the model is still thinking — fires no hook, and on 2.1.310 writes nothing to the transcript
//! either: the prompt entry written at submit stays the last user entry, and Claude Code puts the
//! prompt's text back in its input box. So `crate::transcript`'s interrupt watch never sees it, and
//! the session would read `Working` until its next prompt. What does change is the terminal title:
//! Claude Code starts it with a turning half-filled circle (`◐`, `◑`) while a turn runs and with
//! `✳` when it is back at its prompt, and the glyph turns to `✳` within a tenth of a second of the
//! Esc. See `docs/agent-cli-reference.md`, section "## Claude Code".
//!
//! The same `✳` comes up when every turn ends and whenever a permission prompt or a question waits
//! on the user, tens of milliseconds after the `Stop` or `PermissionRequest` hook was posted, and
//! those hooks move the session out of `Working` by themselves. So a `✳` only ends the turn once it
//! has held for [`SETTLE`], the session's queue of hook events has drained, and the session still
//! reads `Working`: a turn that went back to its prompt with no hook. And it counts only if it
//! follows a working glyph that was current when the watch was armed or came after, so the `✳` left
//! on the title by the previous turn does not end this one.
//!
//! A cancel this close to the prompt's submission can come before the watch is armed, which happens
//! when the daemon applies the hook event that started the turn; the glyph has then already turned,
//! and the session keeps reading `Working`, as it did before this watch existed. So does a session
//! whose title Claude Code has been told not to set, a release that changes the glyphs, and a
//! running frame and the `✳` after it read from the PTY in one chunk, of which only the last title
//! is seen. A `✳` while one of `crate::agent_dialogs`' dialogs is up is a turn waiting on the user,
//! with no hook to say so, and is not counted either.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::protocol::SessionStatus;
use crate::state::AppState;
use crate::transcript;

/// The glyphs Claude Code starts its title with while a turn runs, turning about once a second.
const RUNNING: std::ops::RangeInclusive<char> = '\u{25D0}'..='\u{25D3}';

/// The glyph it starts its title with when it is back at its prompt.
const AT_PROMPT: char = '\u{2733}';

/// How long the title must have read as back at the prompt before the turn counts as ended without
/// a hook. Far longer than the hooks that come with the same glyph take to arrive, short enough
/// that the session follows a cancel in about a second.
const SETTLE: Duration = Duration::from_secs(1);

/// How often the watch looks at the title.
const POLL: Duration = Duration::from_millis(250);

/// What Claude Code's terminal title has said about its turns, kept per session by the title
/// observer on the PTY reader thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TitleActivity {
    /// Whether the last title that showed either glyph showed a turn running.
    working: bool,
    /// How many times the title has gone from a running turn back to the prompt.
    stops: u64,
    /// When it last did.
    stopped_at: Option<Instant>,
}

impl TitleActivity {
    /// Notes a completed title that started with `glyph`, or with none. A title with neither
    /// glyph, such as the empty one at exit, changes nothing.
    pub fn note(&mut self, glyph: Option<char>, now: Instant) {
        match glyph {
            Some(glyph) if RUNNING.contains(&glyph) => self.working = true,
            Some(AT_PROMPT) if self.working => {
                self.working = false;
                self.stops += 1;
                self.stopped_at = Some(now);
            }
            _ => {}
        }
    }

    /// Whether the title went back to the prompt after `armed`, the count when the watch was armed,
    /// and has stayed there for [`SETTLE`] by `now`.
    fn settled_at_prompt(&self, armed: u64, now: Instant) -> bool {
        self.stops > armed
            && !self.working
            && self
                .stopped_at
                .is_some_and(|at| now.saturating_duration_since(at) >= SETTLE)
    }
}

/// Starts watching the terminal title of `session_id`, a Claude Code session a hook has just moved
/// into `Working`, for its turn ending with no hook. `generation` is the transcript interrupt
/// watch's, armed for the same move: the next prompt retires it, and with it this watch, before its
/// turn opens, and the session is put back at its prompt only while it is still current. Spawned,
/// never awaited.
pub fn watch(state: &Arc<AppState>, session_id: &str, generation: u64) {
    let Some(live) = state.live_session(session_id) else {
        return;
    };
    let armed = live.title_activity().stops;
    let live = Arc::downgrade(&live);
    let state = state.clone();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let at_prompt = || {
            live.upgrade().map(|live| {
                live.title_activity()
                    .settled_at_prompt(armed, Instant::now())
                    && live.agent_dialog().is_none()
            })
        };
        loop {
            tokio::time::sleep(POLL).await;
            if !state.record_watch_current(&session_id, generation) {
                return;
            }
            // A hand raised by a session request comes down to `Working` with no hook to arm
            // another watch, so this one waits it out; one raised by a hook comes down with a hook
            // that retires this watch. Anything else ends the turn.
            match state.session_record(&session_id) {
                Ok(record) => match state.agent_status(&record) {
                    SessionStatus::Working => {}
                    SessionStatus::WaitingUser => continue,
                    _ => return,
                },
                Err(_) => continue,
            }
            match at_prompt() {
                None => return, // The process is gone.
                Some(false) => continue,
                Some(true) => {}
            }
            // A hook posted with the glyph may still be waiting to be applied; whatever it says
            // about the session wins over the title.
            state.hook_queues.settled(&session_id).await;
            if at_prompt() != Some(true) {
                continue;
            }
            // Conditional on the session still reading `Working`: one a hook moved on, the turn
            // ending as usual or a prompt waiting on the user, is left where the hook put it.
            transcript::back_at_prompt(
                &state,
                &session_id,
                generation,
                SessionStatus::Working,
                "cancel in the terminal title",
            );
            return;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::LiveSession;
    use crate::test_support::{
        app_state, console, console_session, idle_stand_in, within, ScratchDir, StandIn,
    };

    const WORKING: Option<char> = Some('\u{25D0}');
    const TURNING: Option<char> = Some('\u{25D1}');
    const IDLE: Option<char> = Some(AT_PROMPT);

    /// Captures from Claude Code 2.1.310; see `testdata/claude-activity-titles.md`.
    const IDLE_TITLE: &[u8] = include_bytes!("../testdata/claude_title_idle.bin");
    const CANCEL_TITLES: &[u8] = include_bytes!("../testdata/claude_title_cancel.bin");
    const PERMISSION_TITLES: &[u8] = include_bytes!("../testdata/claude_title_permission.bin");

    #[test]
    fn a_turn_counts_as_stopped_once_the_title_has_held_at_the_prompt() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        let armed = activity.stops;
        activity.note(WORKING, start);
        activity.note(TURNING, start);
        assert!(!activity.settled_at_prompt(armed, start + SETTLE));
        activity.note(IDLE, start);
        assert!(!activity.settled_at_prompt(armed, start + SETTLE / 2));
        assert!(activity.settled_at_prompt(armed, start + SETTLE));
    }

    #[test]
    fn the_previous_turn_s_idle_title_does_not_end_this_one() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note(IDLE, start);
        activity.note(WORKING, start);
        activity.note(IDLE, start);
        // Armed with the title back at the prompt: only a turn seen running from here on counts.
        let armed = activity.stops;
        activity.note(IDLE, start);
        assert!(!activity.settled_at_prompt(armed, start + SETTLE * 10));
        activity.note(WORKING, start);
        activity.note(IDLE, start);
        assert!(activity.settled_at_prompt(armed, start + SETTLE));
    }

    #[test]
    fn a_turn_already_running_when_armed_counts() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note(WORKING, start);
        let armed = activity.stops;
        activity.note(IDLE, start);
        assert!(activity.settled_at_prompt(armed, start + SETTLE));
    }

    #[test]
    fn running_again_before_the_settle_runs_out_is_not_a_stop() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        let armed = activity.stops;
        activity.note(WORKING, start);
        activity.note(IDLE, start);
        activity.note(WORKING, start + SETTLE / 2);
        assert!(!activity.settled_at_prompt(armed, start + SETTLE * 10));
    }

    #[test]
    fn titles_with_neither_glyph_change_nothing() {
        let start = Instant::now();
        let mut activity = TitleActivity::default();
        activity.note(WORKING, start);
        let before = activity;
        for glyph in [None, Some('\u{2722}'), Some('\u{280B}')] {
            activity.note(glyph, start);
        }
        assert_eq!(activity, before);
    }

    /// Long enough for a watch to have seen a settled title and acted on it.
    const ACTED: Duration = Duration::from_millis(SETTLE.as_millis() as u64 * 3);

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
        reaches_within(state, status, ACTED).await
    }

    fn armed(fixture: &Fixture) -> u64 {
        let generation = fixture.state.begin_record_watch("s");
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
    }

    /// The title left at the prompt by the turn before is no cancel of this one.
    #[tokio::test]
    async fn the_title_the_previous_turn_left_ends_nothing() {
        let fixture = fixture("previous", SessionStatus::Working);
        replay(&fixture.live, CANCEL_TITLES);
        armed(&fixture);
        replay(&fixture.live, IDLE_TITLE);
        assert!(!reaches(&fixture.state, SessionStatus::Idle).await);
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
        assert!(!reaches(&fixture.state, SessionStatus::Idle).await);
        assert_eq!(status(&fixture.state), Some(SessionStatus::WaitingUser));
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
        assert!(
            !reaches_within(&fixture.state, SessionStatus::Idle, SETTLE + SETTLE / 2).await,
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
        assert!(!reaches(&fixture.state, SessionStatus::Idle).await);
    }

    /// A watch the next prompt retired does not end the turn that prompt opened.
    #[tokio::test]
    async fn a_retired_watch_ends_nothing() {
        let fixture = fixture("retired", SessionStatus::Working);
        armed(&fixture);
        fixture.state.end_record_watch("s");
        replay(&fixture.live, CANCEL_TITLES);
        assert!(!reaches(&fixture.state, SessionStatus::Idle).await);
    }
}
