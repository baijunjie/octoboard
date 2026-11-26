//! A session's status taken from its terminal title where nothing else reports it
//! (`docs/plans/terminal-first-degradation.md`, D9): the launch's hooks are absent
//! (`crate::session::hook_absence`), the program has made no report of its root status record in
//! this launch (`crate::session::program_status`), and the provider's reader reads activity off its
//! titles (`ScreenReader::title_activity`, read through `crate::session::title_activity`). The
//! title then gives working and idle, and nothing more.
//!
//! **Working.** A running title the agent is still sending (`TitleActivity::shows_running`) moves
//! the session to `Working` and opens a turn where none is open, as one already under way: no
//! prompt is attributed to it. The move goes in behind the program's own reports, applied as one
//! of theirs (`LiveSession::open_turn_off_title`), so the title is a second source of the status
//! path the reports drive, not a state machine of its own.
//!
//! **Idle.** Only the closing differs. The title back at the prompt after a running one closes the
//! turn only once all of these hold, read by the watch below: it has stayed there for the title's
//! settle with no running title meanwhile, and no dialog of the agent's is up or has just gone
//! (`TurnReading::settled_at_prompt`, the settle being the title cancel watch's,
//! `crate::session::title_interrupt`); the parsed screen, looked at once at that moment, shows no
//! dialog and no prompt the provider's interaction reader recognises; and, where the provider's
//! reader declares it reads its input box wherever the agent is at its prompt
//! (`ScreenReader::reads_input_box`), the box is drawn. The turn is then closed as
//! `AppState::end_turn_off_title` closes one (`LiveSession::close_turn_off_title`): no report is
//! synthesised for an owner, no turn message is waited for, what the session itself reported is
//! kept, and an end of the agent's own that still comes is its real end. Unlike a cancel read off
//! the title, it does not mark the session paused, so a title can never produce a report or a
//! pause by mistake.
//!
//! **Never from the title**: waiting on the user. A dialog the screen readers recognise raises the
//! hand as it does without this source; while the session reads `WaitingUser` the watch closes
//! nothing. A running title that stops coming with no prompt title after it leaves the session
//! working (`TitleActivity::last_said_running`); the end of the process settles it.
//!
//! **Taken over.** The first report of the program's root record takes over for the rest of the
//! launch, as it does for the title's activity (`TitleActivity::note_report`), from the turn the
//! title opened; a hook moving the launch to present ends the source at once. Either ends the
//! watch, and neither comes back: a report is not taken back, and a launch with hooks is not read
//! as without them again.
//!
//! Residual risks, accepted (see "A session whose hooks are absent and whose program reports
//! nothing" in `docs/product/turns-hooks-do-not-report.md`): a wait on the user no reader
//! recognises, with the input box drawn, reads as idle, and the next running title opens another
//! turn; a normal end and a cancel are not told apart; a title the user turned off or a release
//! that changes the glyphs leaves the session idle; a running glyph shown outside a turn reads
//! working until it stops.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use super::interaction::Reading;
use super::title_interrupt::{settle, POLL};
use super::LiveSession;
use crate::protocol::SessionStatus;
use crate::state::AppState;

impl LiveSession {
    /// Whether the title gives the session its status now: its hooks are absent and the program
    /// has made no report of its root status record in this launch. Where the provider's reader
    /// reads no activity off its titles, no title shows a turn running, so the source never acts.
    pub(crate) fn title_gives_status(&self) -> bool {
        self.hooks_absent() && !self.title_activity().reported()
    }

    /// Takes a completed title into the session's status where the title gives it
    /// ([`Self::title_gives_status`]): a fresh running title opens the turn and moves the session
    /// to `Working`, and the first one starts the watch that closes the turns it opens. Called on
    /// the PTY reader thread after the chunk's reports have been taken. Nothing before the session
    /// is registered: the agent sends its running title several times a second, so the next one
    /// after does it.
    pub(super) fn follow_title_status(&self) {
        if !self.title_gives_status() || !self.title_activity().shows_running(Instant::now()) {
            return;
        }
        let Some((state, runtime)) = self.state_and_runtime() else {
            return;
        };
        self.open_turn_off_title();
        if !self.title_watch.swap(true, Ordering::AcqRel) {
            let Some(live) = state
                .live_session(&self.id)
                .filter(|live| live.launch_id == self.launch_id)
            else {
                self.title_watch.store(false, Ordering::Release);
                return;
            };
            // The count of stops is taken here, with the title that opened the turn, not when the
            // watch first runs: the title can be back at the prompt by then, and a stop counted
            // before the watch would never be read as this turn's end.
            let armed = live.title_activity().stops();
            watch(state, &runtime, Arc::downgrade(&live), armed, settle());
        }
    }
}

/// Whether the parsed screen, looked at once now, lets a turn close off the title: no dialog of the
/// agent's, no prompt its interaction reader recognises, and, where its reader reads its input box
/// wherever it is at its prompt, the box drawn.
fn screen_at_prompt(live: &LiveSession) -> bool {
    if live.look_for_dialog().current.is_some() {
        return false;
    }
    let provider = crate::adapter::provider(live.agent);
    if let Some(reader) = provider.interactions() {
        let screen = super::interaction::screen_of(live, reader);
        if matches!(
            reader.read(&screen, &live.interaction.calls()),
            Reading::Shown { .. } | Reading::Up
        ) {
            return false;
        }
    }
    !provider.screen().reads_input_box() || live.input_box_rows(&live.box_screen()).is_some()
}

/// Watches the title of `live`'s launch for as long as it gives the session its status, closing
/// each turn the title opened once the title has settled back at the prompt
/// ([`screen_at_prompt`] and the module's "Idle"). One per launch; ends with the source or the
/// process. `armed` is the count of the title's stops when the first turn it opened was opened;
/// `settle` is captured on the caller's thread, where a test sets it. Spawned, never awaited.
fn watch(
    state: Arc<AppState>,
    runtime: &tokio::runtime::Handle,
    live: Weak<LiveSession>,
    mut armed: u64,
    settle: Duration,
) {
    runtime.spawn(async move {
        loop {
            tokio::time::sleep(POLL).await;
            let Some(live_now) = live.upgrade() else {
                return;
            };
            if !state.is_current(&live_now)
                || live_now.has_exited()
                || !live_now.title_gives_status()
            {
                live_now.title_watch.store(false, Ordering::Release);
                return;
            }
            // A hand raised on the user is a turn waiting on them: nothing closes it here. A turn
            // whose `working` is still to be applied reads as it did before, and waits.
            let settled = {
                let reading = live_now.turn_reading();
                reading.hooks() == Some(SessionStatus::Working)
                    && reading.settled_at_prompt(armed, settle)
            };
            if !settled || !screen_at_prompt(&live_now) {
                continue;
            }
            // The stops counted so far are this turn's: the next turn's are counted from here.
            let stops = live_now.title_activity().stops();
            // Whatever the program reported meanwhile is applied first, and wins.
            live_now.settle_program_status().await;
            let still = live_now.title_gives_status()
                && live_now.turn_reading().settled_at_prompt(armed, settle);
            if !still {
                continue;
            }
            match live_now.close_turn_off_title(&state) {
                Ok(true) => {
                    tracing::debug!(
                        session = %live_now.id,
                        "the terminal title went back to the prompt; the turn it opened is closed"
                    );
                    armed = stops;
                    // Nothing else is coming to release a message queued while the turn ran.
                    state.spawn_flush_outbox(&live_now.id, SessionStatus::Idle);
                }
                Ok(false) => {}
                Err(err) => {
                    tracing::debug!(
                        session = %live_now.id, %err,
                        "closing the turn the terminal title opened failed"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests;
