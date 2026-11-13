//! One reading of whether a session's agent is in a turn, for everything that acts on it: the
//! interrupt key (`interrupt`), the cancel read off the title (`title_interrupt`) and the lone Esc
//! the draft's key timing marks as typed mid-turn (`draft`). Each signal is read here once, in one
//! order — the title observer's lock, then the dialog sighting's, then the store — and each
//! consumer combines them as its own method below, so where they differ the difference is written
//! down in one place.
//!
//! The consumers differ on purpose in which way they err, each toward the side a wrong reading
//! costs least on. The interrupt key would cancel whatever turn it lands in, so it goes in only on
//! a running title the agent is still sending ([`TitleActivity::shows_running`]); an Esc read as
//! typed at the prompt lets a message in without waiting for the agent to draw the prompt a cancel
//! puts back, so an Esc counts as mid-turn on any running title not yet followed by the prompt,
//! however old ([`TitleActivity::last_said_running`]). See `docs/product/interrupting-turns.md`,
//! section "When a key is sent", and `docs/product/message-delivery.md`, section "Keys that leave
//! the composer empty are confirmed by the screen".

use std::cell::OnceCell;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use super::title_activity::TitleActivity;
use super::LiveSession;
use crate::protocol::SessionStatus;
use crate::state::AppState;

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
        TurnReading {
            live: self,
            state,
            at: Instant::now(),
            activity,
            dialog_up: self.agent_dialog().is_some(),
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
    /// the terminal shows the turn running by a title still being sent or a `working` report, and
    /// nothing of the agent's own is up that the key would answer: a dialog, or Codex's title
    /// waiting on the user. Read under the input lock, right before the key.
    pub(crate) fn interrupt_may_press(&self) -> bool {
        self.terminal_running_fresh()
            && !self.dialog_up
            && !self.action_title
            && self.statuses() == Some((SessionStatus::Working, SessionStatus::Working))
    }

    /// Whether a lone Esc typed now is typed mid-turn, where it cancels the turn and the prompt the
    /// agent puts back can come after any output: the title or the reports last showed a turn
    /// running, however long ago, or the hooks report one, as right after a submit, when the title
    /// can still show the prompt.
    pub(crate) fn escape_mid_turn(&self) -> bool {
        self.terminal_last_said_running() || self.hooks_working()
    }

    /// Whether the title has gone back to the prompt since `armed`, the count of stops taken
    /// earlier, held there for `settle`, and no dialog is up: a turn that went back to its prompt
    /// with no hook, for the title's cancel watch.
    pub(crate) fn settled_at_prompt(&self, armed: u64, settle: Duration) -> bool {
        !self.dialog_up && self.activity.settled_at_prompt(armed, self.at, settle)
    }
}

#[cfg(test)]
mod tests;
