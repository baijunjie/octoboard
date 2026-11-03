//! The turn a session's process ended in the middle of, and that turn's own end arriving after it.
//!
//! An end of the process records the turn it cut short as having ended abnormally, from the status
//! the agent's events left (`Store::record_session_exit`). An agent whose hooks are asynchronous
//! commands (Codex, Claude Code's command form) does not wait for them, so a `Stop` it fired just
//! before it exited can reach the daemon after the end was written, when a session without a
//! process takes no flag of a turn. Such an event is told apart by the turn id it carries: the
//! process ended in that turn, and a process started since opens turns of its own.
//!
//! An end the daemon makes itself (an archive, an interruption, a shutdown) is not undone this
//! way: on the `SIGTERM` it stops an agent with, mid-turn, Codex 0.163.0 fires nothing and Claude
//! Code 2.1.316 only `SessionEnd`, while Grok Build 1.0.70 fires `SessionEnd` and its `Stop` with
//! `reason: "shutdown"`, naming no turn (`docs/agent-cli-reference.md`, "Stopped by a signal
//! mid-turn"); none of these is a turn's normal end.

use std::time::{Duration, Instant};

use super::AppState;
use crate::hooks::TurnOutcome;
use crate::protocol::{Session, SessionStatus};

/// How long after the end an event of the turn it cut short is still taken for that turn's. The
/// hook forwarder posts within its own deadline of 1.5 s once it runs (`cli::hook`); this leaves
/// room for its start, which can take seconds on a managed machine, and stays short enough that
/// an id is never matched against anything but a forwarder of the ended process.
const LATE_TURN_END_BOUND: Duration = Duration::from_secs(10);

/// [`LATE_TURN_END_BOUND`], widened to `PATIENCE` under test, and lowered by
/// `LATE_TURN_END_OVERRIDE` for the test proving it runs out. Read on the thread applying the
/// event.
fn late_turn_end_bound() -> Duration {
    #[cfg(test)]
    {
        LATE_TURN_END_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(LATE_TURN_END_BOUND.max(crate::test_support::PATIENCE))
    }
    #[cfg(not(test))]
    LATE_TURN_END_BOUND
}

#[cfg(test)]
thread_local! {
    static LATE_TURN_END_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// A turn the end of the session's process cut short.
pub(super) struct EndedTurn {
    /// The turn's id (`hooks::turn_key`).
    key: String,
    /// The record's `ended_at` that end wrote, which is what the late clear is conditional on.
    ended_at: i64,
    /// When the end was written.
    at: Instant,
    /// An end of the turn has been taken already; anything more of it is only dropped.
    settled: bool,
}

impl AppState {
    /// Runs `end`, a store write that ends the session's process, and remembers the turn it cut
    /// short. `end` is given the status the session's agent is at while a waiting session request
    /// holds the record at `WaitingUser` (`None` with no hand up, when the record's status is the
    /// agent's own): whether the end cut a turn short is the agent's to say, not the hand's. The
    /// hands are held across the write, so a hand raised, lowered or moved by a hook at the same
    /// moment lands wholly before or after it. Taken after the session's input lock where both are
    /// held (a frozen screen's write), the order a submit-key retry, which reads the agent's status
    /// under that lock, keeps too.
    ///
    /// A write that finds the session without a process already (the exit of a process an archive
    /// or a shutdown recorded ended) belongs to the same end, so it only moves the remembered
    /// `ended_at` to the one it wrote.
    pub(crate) fn end_process_record<E>(
        &self,
        id: &str,
        end: impl FnOnce(Option<SessionStatus>) -> Result<Option<(SessionStatus, Session)>, E>,
    ) -> Result<Option<(SessionStatus, Session)>, E> {
        let hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        let own = hands.get(id).copied();
        let ended = end(own)?;
        if let Some((before, session)) = &ended {
            let mut turns = self.ended_turns.lock().expect("ended turns lock poisoned");
            if before.has_no_process() {
                if let (Some(turn), Some(ended_at)) = (turns.get_mut(id), session.ended_at) {
                    turn.ended_at = ended_at;
                }
            } else {
                let cut_short = crate::store::cuts_turn_short(own.unwrap_or(*before));
                match (cut_short, self.turn_key_of(id), session.ended_at) {
                    (true, Some(key), Some(ended_at)) => {
                        turns.insert(
                            id.to_string(),
                            EndedTurn {
                                key,
                                ended_at,
                                at: Instant::now(),
                                settled: false,
                            },
                        );
                    }
                    _ => {
                        turns.remove(id);
                    }
                }
            }
        }
        drop(hands);
        Ok(ended)
    }

    /// Takes an event of the session's own that names turn `key` and, when that is the turn the
    /// end of its process cut short, applies it as that turn's late word, returning `true`: the
    /// event belongs to the ended process and none of it is applied otherwise, so it cannot move a
    /// process started since. The first end of the turn (`outcome`) settles it: a normal one
    /// clears the flag the end set, while the record still stands as that end left it
    /// (`Store::clear_cut_short_turn`), and an abnormal one leaves it set. Any other event of the
    /// turn, and anything after it is settled, is dropped. Returns `false` for an event of any
    /// other turn, or past [`LATE_TURN_END_BOUND`].
    pub fn apply_late_turn_event(&self, id: &str, key: &str, outcome: Option<TurnOutcome>) -> bool {
        let mut turns = self.ended_turns.lock().expect("ended turns lock poisoned");
        let Some(turn) = turns.get_mut(id) else {
            return false;
        };
        if turn.at.elapsed() > late_turn_end_bound() {
            turns.remove(id);
            return false;
        }
        if turn.key != key {
            return false;
        }
        // Kept until the bound, so a repeat of it (Claude Code re-runs a `Stop` hook under the
        // same id) is dropped too rather than reaching a process started since.
        if turn.settled || outcome.is_none() {
            return true;
        }
        turn.settled = true;
        let ended_at = turn.ended_at;
        drop(turns);
        if outcome == Some(TurnOutcome::Normal) {
            match self.store.clear_cut_short_turn(id, ended_at) {
                Ok(Some(session)) => self.publish_session(&session),
                Ok(None) => {}
                Err(err) => {
                    tracing::debug!(session = %id, %err, "clearing a late-ended turn's flag failed")
                }
            }
        }
        true
    }

    /// Drops the turn remembered for a session whose record is gone.
    pub(super) fn forget_ended_turn(&self, id: &str) {
        self.ended_turns
            .lock()
            .expect("ended turns lock poisoned")
            .remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::test_support::insert_record;
    use crate::test_support::app_state;

    /// Past the bound, an end naming the turn cut short is no longer taken for that turn's: it
    /// goes the way of any event, which a session without a process does not take a flag from.
    #[test]
    fn a_late_end_past_the_bound_is_not_taken_for_the_turn_cut_short() {
        let (state, dir) = app_state("late-turn-end-bound");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        state.note_turn_key("s", "t1".to_string());
        state
            .end_process_record("s", |own| state.store.record_session_exit("s", 10, own))
            .unwrap()
            .expect("ended");
        assert!(state.last_turn_abnormal("s"));

        LATE_TURN_END_OVERRIDE.with(|bound| bound.set(Some(Duration::ZERO)));
        std::thread::sleep(Duration::from_millis(1));
        assert!(!state.apply_late_turn_event("s", "t1", Some(TurnOutcome::Normal)));
        assert!(state.last_turn_abnormal("s"));

        // Within the bound the same end is taken, and clears it.
        LATE_TURN_END_OVERRIDE.with(|bound| bound.set(None));
        state
            .store
            .update_session_lifecycle_if(
                "s",
                SessionStatus::Interrupted,
                SessionStatus::Working,
                None,
            )
            .unwrap()
            .expect("relaunched");
        state
            .end_process_record("s", |own| state.store.record_session_exit("s", 20, own))
            .unwrap()
            .expect("ended");
        assert!(state.apply_late_turn_event("s", "t1", Some(TurnOutcome::Normal)));
        assert!(!state.last_turn_abnormal("s"));
    }
}
