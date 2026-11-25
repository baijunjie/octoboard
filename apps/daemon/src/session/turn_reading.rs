//! One reading of whether a session's agent is in a turn, for everything that acts on it: the
//! interrupt key (`interrupt`), the cancel read off the title (`title_interrupt`) and the lone Esc
//! the draft's key timing marks as typed mid-turn (`draft`). Each signal is read here once, in one
//! order — the title observer's lock, then the dialog sighting's, then the tool calls', then the
//! store — and each
//! consumer combines them as its own method below, so where they differ the difference is written
//! down in one place.
//!
//! The consumers differ on purpose in which way they err, each toward the side a wrong reading
//! costs least on. The interrupt key would cancel whatever turn it lands in, so it goes in only on
//! a running title the agent is still sending ([`TitleActivity::shows_running`]) or an equivalent
//! report or screen reading. An Esc read as typed at the prompt lets a message in without waiting
//! for the agent to redraw the prompt after a cancel. An Esc therefore counts as mid-turn on any
//! running title not yet followed by the prompt,
//! however old ([`TitleActivity::last_said_running`]). See `docs/product/interrupting-turns.md`,
//! section "When a key is sent", and `docs/product/draft-preservation.md`, section "Keys that leave
//! the composer empty are confirmed by the screen".

use std::cell::OnceCell;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use super::title_activity::TitleActivity;
use super::LiveSession;
use crate::protocol::SessionStatus;
use crate::state::{AppState, ToolCallActivity};

/// What the session's signals said about its turn at one moment. The hooks' status is read from
/// the store on first use only, as a consumer that has its answer from the terminal needs no more.
pub(crate) struct TurnReading<'a> {
    live: &'a LiveSession,
    state: Weak<AppState>,
    /// When the reading was taken.
    at: Instant,
    /// What the title, or the status the program reports, has said about its turns.
    activity: TitleActivity,
    /// Whether one of the dialogs only a person can answer was up at the latest look
    /// ([`LiveSession::agent_dialog`]).
    dialog_up: bool,
    /// When the screen last showed a dialog go away
    /// ([`crate::session::dialogs::Sighting::gone_at`]).
    dialog_gone_at: Option<Instant>,
    /// Whether the hooks report a tool call open, and when the last one ended
    /// ([`AppState::tool_call_activity`]).
    tool_calls: ToolCallActivity,
    /// Whether the title says the agent waits on the user (Codex's `Action Required`).
    action_title: bool,
    /// The record's own status and the status the session reads as the agent's
    /// ([`AppState::agent_status`]), `None` for a session bound to no state or with no record.
    statuses: OnceCell<Option<(SessionStatus, SessionStatus)>>,
}

impl LiveSession {
    /// Reads what the session's signals say about its turn now ([`TurnReading`]).
    pub(crate) fn turn_reading(&self) -> TurnReading<'_> {
        let (activity, state) = {
            let observer = self.title.lock().expect("title observer lock poisoned");
            (observer.activity, observer.state.clone())
        };
        let sighting = self.dialog_sighting();
        let tool_calls = state
            .upgrade()
            .map(|state| state.tool_call_activity(&self.id))
            .unwrap_or_default();
        TurnReading {
            live: self,
            state,
            at: Instant::now(),
            activity,
            dialog_up: sighting.current.is_some(),
            dialog_gone_at: sighting.gone_at,
            tool_calls,
            action_title: self.needs_user_action(),
            statuses: OnceCell::new(),
        }
    }
}

impl TurnReading<'_> {
    fn statuses(&self) -> Option<(SessionStatus, SessionStatus)> {
        *self.statuses.get_or_init(|| {
            let state: Arc<AppState> = self.state.upgrade()?;
            let record = state.session_record(&self.live.id).ok()?;
            Some((record.status, state.agent_status(&record)))
        })
    }

    /// The status the session reads as its agent's ([`AppState::agent_status`]): as the hooks left
    /// it, or `WaitingUser` while Codex's title waits on the user; `None` for a session bound to no
    /// state or with no record.
    pub(crate) fn hooks(&self) -> Option<SessionStatus> {
        self.statuses().map(|(_, agent)| agent)
    }

    /// Whether the agent's hooks last reported a turn running, as the session's status reads.
    fn hooks_working(&self) -> bool {
        self.hooks() == Some(SessionStatus::Working)
    }

    /// Whether the title shows a turn running and the agent is still sending it, or the program
    /// last reported `working` ([`TitleActivity::shows_running`]).
    fn terminal_running_fresh(&self) -> bool {
        self.activity.shows_running(self.at)
    }

    /// Whether the last title or report that said anything showed a turn running, however long ago
    /// ([`TitleActivity::last_said_running`]).
    fn terminal_last_said_running(&self) -> bool {
        self.activity.last_said_running()
    }

    /// Whether the program's status reports have taken over from the title.
    #[cfg(test)]
    fn reported(&self) -> bool {
        self.activity.reported()
    }

    /// Whether one of the dialogs only a person can answer was up at the latest look.
    #[cfg(test)]
    fn dialog_up(&self) -> bool {
        self.dialog_up
    }

    /// Whether an interrupt key may go in: the record and the agent's status both read `Working`,
    /// the terminal shows the turn running by a current title, a `working` report or a provider's
    /// screen, and nothing of the agent's own is up that the key would answer: a dialog, or a title
    /// waiting on the user. Read under the input lock, right before the key.
    pub(crate) fn interrupt_may_press(&self) -> bool {
        !self.dialog_up
            && !self.action_title
            && self.statuses() == Some((SessionStatus::Working, SessionStatus::Working))
            && (self.terminal_running_fresh()
                || crate::adapter::provider(self.live.agent)
                    .screen()
                    .interrupt_running_screen()
                    .is_some_and(|running| running(&self.live.screen_text())))
    }

    /// Whether the session is at its agent's idle prompt, for a key that means something else
    /// mid-turn ([`crate::session::screen::ScreenReader::input_box_clearing_key`]): it errs toward
    /// a turn running, which such a key would interrupt. The title or the reports last said the
    /// agent was at its prompt, no dialog is up, nothing of the agent's own waits on the user, and
    /// the record and the agent's status both read `Idle`. Read under the input lock, right before
    /// the key.
    pub(crate) fn at_idle_prompt(&self) -> bool {
        !self.terminal_last_said_running()
            && !self.dialog_up
            && !self.action_title
            && self.statuses() == Some((SessionStatus::Idle, SessionStatus::Idle))
    }

    /// Whether a lone Esc typed now is typed mid-turn, where it cancels the turn and the prompt the
    /// agent puts back can come after any output: the title or the reports last showed a turn
    /// running, however long ago, or the hooks report one, as right after a submit, when the title
    /// can still show the prompt.
    pub(crate) fn escape_mid_turn(&self) -> bool {
        self.terminal_last_said_running() || self.hooks_working()
    }

    /// Whether the title has gone back to the prompt since `armed`, the count of stops taken
    /// earlier, held there for `settle`, and neither a dialog nor a tool call is open, nor has one
    /// ended within `settle`: a turn that went back to its prompt with no hook, for the title's
    /// cancel watch.
    ///
    /// A dialog raised mid-turn leaves the title at the prompt while it is up, and so can a wait
    /// raised from inside a running tool call that no hook reports and no screen pattern knows,
    /// such as an MCP server's own confirmation. A cancel while a tool runs needs no title to be
    /// seen: the model has shown output by then, so the agent's other channels report it (see
    /// `docs/product/turns-hooks-do-not-report.md`, section "Cancelling a Claude Code turn"). The
    /// agent can take a moment after either ends to show its turn running again, so the settle
    /// starts over when one does.
    pub(crate) fn settled_at_prompt(&self, armed: u64, settle: Duration) -> bool {
        let settled_since = |at: Option<Instant>| {
            at.is_none_or(|at| self.at.saturating_duration_since(at) >= settle)
        };
        !self.dialog_up
            && !self.tool_calls.open
            && settled_since(self.dialog_gone_at)
            && settled_since(self.tool_calls.last_closed_at)
            && self.activity.settled_at_prompt(armed, self.at, settle)
    }
}

#[cfg(test)]
mod tests;
