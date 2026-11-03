//! The turn bookkeeping a synthesised report rests on: when a turn is open, whether the session
//! reported during it, and what closing it amounts to.

use super::AppState;
use crate::time::now_millis;

/// How long an open turn must have been quiet before a clock-attributed turn end may be read as
/// belonging to it. Grok's `idle_prompt` backstop fires about 60 s after a turn ends and carries no
/// turn id, so a turn still producing events cannot be the one it is talking about. Set under that
/// delay so jitter does not make a legitimate backstop unusable.
const BACKSTOP_QUIET: std::time::Duration = std::time::Duration::from_secs(45);

/// What the daemon remembers about a session's current turn.
#[derive(Default)]
pub(super) struct TurnState {
    /// A turn has started and no turn-end signal has been read for it yet. Turn ends are only
    /// acted on while this holds, which is what keeps a turn-end signal that arrives twice from
    /// being read as two turns ending.
    open: bool,
    /// The session called `report` during this turn, so its stop needs no synthesised report.
    reported: bool,
    /// Its owner has been told during this turn that the session waits for the user (see
    /// [`AppState::claim_waiting_notice`]).
    waiting_told: bool,
    /// When this turn last showed any sign of life. A clock-attributed turn end (Grok's
    /// `idle_prompt`) can only belong to a turn that has been quiet at least as long as that
    /// signal's own delay; without this, one left over from a finished turn would close the turn
    /// running now and hand the console session a report for work still in flight.
    last_event_at: i64,
    /// A turn of this session has ended through a signal that names its own turn, and Grok's
    /// clock-attributed echo of that ending has not been seen yet. The quiet gate alone is not
    /// enough: Grok emits nothing between `PreToolUse` and `PostToolUse`, so a tool call that runs
    /// longer than the backstop delay — a build, a test run — leaves a *working* turn looking
    /// quiet, and the previous turn's echo would then close it. The echo is consumed rather than
    /// acted on.
    echo_pending: bool,
    /// Turns not started yet that are credited as reported, because what starts each is something
    /// of Octoboard's own that needs no report back: the turn whose prompt opens with that text
    /// (`credit_turn_opened_by`). Each is spent by that turn's start.
    credits: Vec<String>,
    /// The turn was closed off the terminal title, as cancelled, rather than by a hook
    /// (`end_turn_off_title`), so a turn end the agent still reports for it is its real end.
    ended_off_title: bool,
    /// How many messages Octoboard wrote into the session while this turn ran: for Codex, the ones
    /// a cancel of the turn leaves unread.
    written_mid_turn: usize,
    /// How many times a turn of this session has ended, closed or cancelled, or been seen back at
    /// its prompt (`note_turn_end`). Only ever moves on, so a reader that compares it with an
    /// earlier reading sees an end even when the next turn has already started.
    ends: u64,
    /// The id of the turn the session's latest event that names one belongs to
    /// (`hooks::turn_key`): while the agent is working, its current turn's.
    key: Option<String>,
}

/// What closing a turn amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnClose {
    /// The turn is closed and nobody reported for it, so a report has to be synthesised.
    OwesReport,
    /// The turn is closed and the session reported for itself.
    Reported,
    /// No turn was open. The signal is a repeat, or belongs to something Octoboard never saw start.
    NoTurn,
    /// A clock-attributed signal that cannot belong to the turn currently open, so nothing was
    /// closed — and nothing it says about the session may be believed either.
    NotThisTurn,
}

impl AppState {
    /// A turn has begun, with `prompt` the text that started it where the agent reports one. This
    /// is also what re-arms the turn-end signal, so a session that reported in its previous turn is
    /// not credited for this one.
    ///
    /// A credit given ahead of it covers this turn only if `prompt` opens with the credited text,
    /// and otherwise waits for the turn that does: the next turn to start is not always the one
    /// credited, since a backgrounded tool call's result arrives as a turn of its own and nothing
    /// orders it against a message written into the session. With no prompt to go by, the oldest
    /// credit is spent by this turn.
    pub fn turn_started(&self, id: &str, prompt: Option<&str>) {
        self.quota_recovery.cancel(id);
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let turn = turns.entry(id.to_string()).or_default();
        // A prompt that arrives while the turn is open joins it, as one steered into a Codex turn
        // does, carrying the same turn id; only a turn that opens here starts the count afresh.
        if !turn.open {
            turn.written_mid_turn = 0;
            turn.waiting_told = false;
        }
        turn.open = true;
        turn.ended_off_title = false;
        let credit = match prompt {
            Some(prompt) => turn
                .credits
                .iter()
                .position(|opening| prompt.starts_with(opening.as_str())),
            None => (!turn.credits.is_empty()).then_some(0),
        };
        turn.reported = credit.is_some();
        if let Some(credit) = credit {
            turn.credits.remove(credit);
        }
        turn.last_event_at = now_millis();
    }

    /// Whether a clock-attributed turn end would be consumed as an echo of an earlier ending rather
    /// than acted on. Nothing in the daemon has to ask — `close_turn` decides it — but the rule is
    /// subtle enough that its test says so directly rather than inferring it.
    #[cfg(test)]
    pub fn echo_pending(&self, id: &str) -> bool {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .get(id)
            .is_some_and(|turn| turn.echo_pending)
    }

    /// Notes the turn id an event of the session's own carried (`hooks::turn_key`).
    pub fn note_turn_key(&self, id: &str, key: String) {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .entry(id.to_string())
            .or_default()
            .key = Some(key);
    }

    /// The id of the turn the session's latest event named, if any (see [`Self::note_turn_key`]).
    pub(super) fn turn_key_of(&self, id: &str) -> Option<String> {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .get(id)
            .and_then(|turn| turn.key.clone())
    }

    /// This session is still working on its open turn. Anything that is not a turn ending counts,
    /// and the point is only the clock: a turn producing events cannot be the one a backstop that
    /// fires a minute after a turn ends is talking about.
    pub fn touch_turn(&self, id: &str) {
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            turn.last_event_at = now_millis();
        }
    }

    /// The turn ended without owing anything — the user cancelled it. Closing it matters even
    /// though no report follows: left open, a later clock-attributed end would find it and invent
    /// one.
    pub fn abandon_turn(&self, id: &str) {
        self.quota_recovery.cancel(id);
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            turn.open = false;
            turn.reported = false;
            turn.ended_off_title = false;
            turn.last_event_at = now_millis();
            turn.ends += 1;
            // Grok's backstop fires after a cancelled turn too.
            turn.echo_pending = true;
        }
    }

    /// The turn ended as cancelled by what the terminal title showed, with no hook to say so
    /// (`crate::session::title_interrupt`). Closed as [`Self::abandon_turn`] closes a cancelled
    /// one, except that what the session reported is kept and a turn end of the agent's own that
    /// still comes closes it as its real end ([`Self::close_turn`]): the title only infers the
    /// cancel.
    pub fn end_turn_off_title(&self, id: &str) {
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            if !turn.open {
                return;
            }
            turn.open = false;
            turn.ended_off_title = true;
            turn.last_event_at = now_millis();
            turn.ends += 1;
            turn.echo_pending = true;
        }
    }

    /// A watch saw the session back at its prompt with no hook, its turn left open
    /// (`crate::records::watch::claude_transcript::back_at_prompt`).
    pub fn note_turn_end(&self, id: &str) {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .entry(id.to_string())
            .or_default()
            .ends += 1;
    }

    /// How many times a turn of this session has ended so far; see [`TurnState::ends`].
    pub fn turn_ends(&self, id: &str) -> u64 {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .get(id)
            .map_or(0, |turn| turn.ends)
    }

    /// Counts a message Octoboard wrote into the session while its turn ran.
    pub fn note_written_mid_turn(&self, id: &str) {
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            turn.written_mid_turn += 1;
        }
    }

    /// How many messages Octoboard wrote into the session while its current or last turn ran.
    pub fn written_mid_turn(&self, id: &str) -> usize {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .get(id)
            .map_or(0, |turn| turn.written_mid_turn)
    }

    /// Whether the owner of the session is to be told now that it waits for the user: once a turn,
    /// whether the session has reported in it or not (a report says what it did, not that it then
    /// stopped for a person), and once for a wait before the first turn, which the turn's start
    /// clears. Taken by whoever asks first.
    pub fn claim_waiting_notice(&self, id: &str) -> bool {
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let turn = turns.entry(id.to_string()).or_default();
        if turn.waiting_told {
            return false;
        }
        turn.waiting_told = true;
        true
    }

    /// The session reported during its current turn, so its stop needs nothing synthesised.
    pub fn mark_reported(&self, id: &str) {
        self.set_reported(id, true);
    }

    /// Credits as reported the turn something of Octoboard's own will start, one the session has
    /// nothing to report back on and whose prompt opens with `opening`. The credit is spent by that
    /// turn's start (see `turn_started`) and otherwise goes with the session's bookkeeping when its
    /// process ends. Crediting the same turn again changes nothing.
    ///
    /// `opening` is specific to that one text, not a prefix other texts share, such as the notice
    /// prefix: the turn of whichever of them starts first would spend it. Text from outside it
    /// embeds (an account name) is passed as written into the session, through
    /// `session::term::stripped`, since the prompt a turn reports is the stripped text.
    pub fn credit_turn_opened_by(&self, id: &str, opening: String) {
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let credits = &mut turns.entry(id.to_string()).or_default().credits;
        if !credits.contains(&opening) {
            credits.push(opening);
        }
    }

    /// Takes the credit for the turn opening with `opening` back, for something that will not
    /// start one after all.
    pub fn clear_turn_credit(&self, id: &str, opening: &str) {
        if let Some(turn) = self.turns.lock().expect("turn lock poisoned").get_mut(id) {
            turn.credits.retain(|credit| credit != opening);
        }
    }

    /// Takes that credit back, for a report that turned out not to reach its owner. Left set, the
    /// session's stop would produce no synthesised report either and the owner would get nothing.
    pub fn clear_reported(&self, id: &str) {
        self.set_reported(id, false);
    }

    fn set_reported(&self, id: &str, reported: bool) {
        self.turns
            .lock()
            .expect("turn lock poisoned")
            .entry(id.to_string())
            .or_default()
            .reported = reported;
    }

    /// Closes the open turn and says what that amounted to. `clock_attributed` marks a signal that
    /// names no turn of its own, which may only be read as ending a turn that has already gone
    /// quiet.
    pub fn close_turn(&self, id: &str, clock_attributed: bool) -> TurnClose {
        let mut turns = self.turns.lock().expect("turn lock poisoned");
        let Some(turn) = turns.get_mut(id) else {
            return TurnClose::NoTurn;
        };
        if !turn.open && turn.ended_off_title && !clock_attributed {
            // The title closed the turn as cancelled, and this is the turn's own end after all,
            // reported late: it is what the turn amounted to. Its echo is the one the title's close
            // already armed.
            let reported = turn.reported;
            turn.ended_off_title = false;
            turn.reported = false;
            turn.last_event_at = now_millis();
            turn.ends += 1;
            return if reported {
                TurnClose::Reported
            } else {
                TurnClose::OwesReport
            };
        }
        if !turn.open {
            // Each ending arms exactly one echo, and this spends it whether or not a turn is open —
            // which is the usual case: the echo arrives about a minute later, and a session that has
            // not been given more work is still idle by then. Left armed, it would be spent on the
            // next turn instead, and that turn might be one Grok reports no stop event for at all,
            // whose echo is its only turn-end signal.
            if clock_attributed {
                turn.echo_pending = false;
            }
            return TurnClose::NoTurn;
        }
        if clock_attributed {
            // An ending that named its own turn has already been acted on, and this is its echo.
            if turn.echo_pending {
                turn.echo_pending = false;
                return TurnClose::NotThisTurn;
            }
            if now_millis().saturating_sub(turn.last_event_at) < BACKSTOP_QUIET.as_millis() as i64 {
                return TurnClose::NotThisTurn;
            }
        }
        let reported = turn.reported;
        turn.open = false;
        turn.reported = false;
        turn.ended_off_title = false;
        turn.last_event_at = now_millis();
        turn.ends += 1;
        // Only a signal that named its own turn leaves an echo behind; the echo itself does not.
        turn.echo_pending = !clock_attributed;
        if reported {
            TurnClose::Reported
        } else {
            TurnClose::OwesReport
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::outbox::Drain;
    use crate::protocol::{Agent, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session};

    /// A message written while a turn runs is counted for that turn: a prompt joining the open
    /// turn, as one steered into a Codex turn does, keeps the count, a message that opens a turn is
    /// not counted, and the next turn starts again from none.
    #[tokio::test]
    async fn messages_written_while_a_turn_runs_are_counted_for_it() {
        let (state, dir) = app_state("state-written-mid-turn");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", Agent::Codex, 80, 24, "sleep 30");
        state.register_live(live.clone());
        state.turn_started("s", None);

        state.queue_message("s", "steer this turn");
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        state.turn_started("s", Some("steer this turn"));
        assert_eq!(state.written_mid_turn("s"), 1);

        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        state.queue_message("s", "open the next turn");
        assert_eq!(state.flush_outbox("s", SessionStatus::Idle), Drain::Clear);
        assert_eq!(
            state.written_mid_turn("s"),
            1,
            "a message at the prompt is not counted"
        );
        state.turn_started("s", Some("open the next turn"));
        assert_eq!(state.written_mid_turn("s"), 0);
    }

    /// Only one report may come out of one turn, and only when the session did not report for
    /// itself. The repeat case is not hypothetical: Grok fires its `idle_prompt` backstop about a
    /// minute after every turn, whether or not a `Stop` already reported it.
    #[test]
    fn a_turn_can_only_be_closed_once() {
        let (state, _dir) = app_state("state-turns");
        // Nothing has started, so nothing is owed a report — a stop event arriving before any turn
        // start (an agent's own teardown, a slash command) must not produce one.
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);

        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);

        state.turn_started("s", None);
        state.mark_reported("s");
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);

        // A session credited for one turn is not credited for the next.
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A credit given ahead of a turn covers the turn its message starts, and only that one: the
    /// turn open when it is given keeps its own account, and a turn something else starts first
    /// does not spend it.
    #[test]
    fn a_credit_is_spent_by_the_turn_its_message_starts() {
        let (state, _dir) = app_state("state-turn-credit");
        state.turn_started("s", Some("work"));
        state.credit_turn_opened_by("s", "Message from Octoboard".to_string());
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        state.turn_started("s", Some("<task-notification>"));
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        state.turn_started("s", Some("Message from Octoboard about it"));
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);
        state.turn_started("s", Some("Message from Octoboard about it"));
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// Where the turn's start carries no prompt, there is nothing to tell the credited turn by, so
    /// the next turn to start is the one credited.
    #[test]
    fn without_a_prompt_the_next_turn_spends_the_credit() {
        let (state, _dir) = app_state("state-turn-credit-fallback");
        state.credit_turn_opened_by("s", "Message from Octoboard".to_string());
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A clock-attributed end left over from a finished turn must not close the turn running now,
    /// or the console session gets a report for work still in flight and the real end finds nothing
    /// open.
    #[test]
    fn a_clock_attributed_end_is_refused_while_the_open_turn_is_still_active() {
        let (state, _dir) = app_state("state-backstop");
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", true), TurnClose::NotThisTurn);
        // Still open, so its own end is still reportable.
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// The quiet gate alone is not enough: Grok emits nothing between `PreToolUse` and
    /// `PostToolUse`, so a tool call longer than the backstop delay leaves a working turn looking
    /// quiet and the previous turn's echo would close it. The echo has to be consumed instead.
    #[test]
    fn the_echo_of_an_ending_already_acted_on_is_consumed_not_acted_on() {
        let (state, _dir) = app_state("state-echo");
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        assert!(state.echo_pending("s"));

        // The next turn begins and is still running when the previous turn's echo arrives.
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", true), TurnClose::NotThisTurn);
        assert!(!state.echo_pending("s"));
        // And this turn's own ending is still reportable.
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// The echo usually arrives with no turn open at all — a minute later, with the session still
    /// idle. It has to be spent there too: left armed, it would be spent on the next turn instead,
    /// and that turn may be one Grok reports no stop event for, whose echo is its only ending.
    #[test]
    fn an_echo_that_arrives_with_no_turn_open_is_still_spent() {
        let (state, _dir) = app_state("state-echo-idle");
        state.turn_started("s", None);
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
        // The echo of that ending, with nothing open.
        assert_eq!(state.close_turn("s", true), TurnClose::NoTurn);
        assert!(!state.echo_pending("s"));
    }

    /// A report that never reached the console session must not leave the session looking as
    /// though it reported, or its stop produces no synthesised report either and the console
    /// session gets nothing at all.
    #[test]
    fn credit_for_reporting_can_be_taken_back() {
        let (state, _dir) = app_state("state-reported");
        state.turn_started("s", None);
        state.mark_reported("s");
        state.clear_reported("s");
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);
    }

    /// A cancelled turn owes no report, but leaving it open is what lets a later backstop invent
    /// one for it.
    #[test]
    fn an_abandoned_turn_is_closed_without_owing_a_report() {
        let (state, _dir) = app_state("state-abandon");
        state.turn_started("s", None);
        state.abandon_turn("s");
        assert_eq!(state.close_turn("s", false), TurnClose::NoTurn);
    }
}
