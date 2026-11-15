//! Clearing a draft out of the way of a message past the draft's grace, where the agent's native
//! stash cannot take it: a message is never written together with text in the input box, and past
//! the grace delivery comes before the draft, so the draft is cleared, handed to every client in a
//! `draft_discarded` notice, and the message written only once the screen shows the box empty. The
//! rules are in `docs/product/draft-preservation.md`, sections "Clearing a draft past the grace
//! period" and "The draft grace period".
//!
//! The keys are the run of Ctrl-U presses an interrupt also uses
//! (`crate::session::input_box_clear`), one a write, each through `LiveSession::write_keys_if` and
//! seen on the screen before the next, and only for
//! an agent whose Ctrl-U is measured to do nothing but delete a line of its box
//! (`crate::session::screen::ScreenReader::clears_input_box_with_ctrl_u`). The session's queue is
//! not paused around them: they are typed from inside the queue's own write of the message, which
//! holds the session's drain, so no other message of Octoboard's can be written meanwhile. Keys the
//! user types meanwhile go straight in and end the run, since every press expects the count of
//! writes past its own: the message then waits rather than join what they typed.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::input_box_clear::{clear_allowance, clear_input_box, press_timeout, Cleared};
use super::swap::{archived_refusal, blocked};
use super::LiveSession;
use crate::platform::term::{Input, PartialWrite};
use crate::protocol::{notice_code, Notice};

/// How long a press nobody saw holds a draft clear back at most ([`LiveSession::clear_held`]):
/// past it one more press goes in, as it does once anything else has moved.
const UNSEEN_CLEAR_HOLD: Duration = Duration::from_secs(30);

/// What stood when a press clearing the input box was not seen to change it.
pub(super) struct UnseenClear {
    /// The box's rows as the screen showed them after the run.
    rows: Vec<String>,
    /// The count of prompts the agent had reported sent.
    prompts: u64,
    /// The count of turns it had ended.
    turn_ends: u64,
    at: Instant,
}

impl LiveSession {
    /// Writes `message`, past the draft's grace, into an input box that shows text no stash can
    /// take: clears the box, waits for a whole frame to show it empty, then writes the message as
    /// into any empty box, refused if anything else has written since the last press. Refused,
    /// with nothing of the message written and the message left to be retried, where the agent's
    /// box has no measured clearing key, the box cannot be read off a whole frame, a key the user
    /// typed may not be drawn yet or is not complete, the box holds only the message Octoboard
    /// wrote before (which is no draft), a press before was not seen and the box has not changed
    /// since, or the box is not seen to empty. Presses that removed something are reported.
    /// **Blocks** while the presses show, for [`press_timeout`] at most a press.
    pub(super) fn write_after_clearing(
        &self,
        message: &[Input],
        prompt: &str,
    ) -> Result<(), PartialWrite> {
        if !crate::adapter::provider(self.agent)
            .screen()
            .clears_input_box_with_ctrl_u()
        {
            return Err(blocked(
                "the session's input box holds text and no key is known to clear it, so the \
                 message waits for it to be emptied",
            ));
        }
        self.wait_for_whole_frame();
        let (draft, rows, mut writes) = {
            let _input = self.pty.lock_input();
            if self.is_frozen() {
                return Err(archived_refusal());
            }
            if self.input_swap_active() {
                return Err(blocked("an input replacement holds the composer"));
            }
            if !self.takes_automatic_keys() {
                return Err(blocked(
                    "the session is waiting for user action or showing a dialog or trust \
                     confirmation",
                ));
            }
            let draft = self.draft.lock().expect("draft lock poisoned");
            let now = Instant::now();
            if !draft.at_input_boundary(now) {
                return Err(blocked(
                    "a key typed into the session is still waiting for the rest of its bytes",
                ));
            }
            if draft.text_may_be_undrawn(now) {
                return Err(blocked(
                    "keys just typed into the session may not be drawn in its input box yet",
                ));
            }
            let (composer, whole, _) = self.read_box();
            let text = match composer {
                Some(text) if text.is_empty() => None,
                Some(text) if whole.is_some() => Some(text),
                Some(_) => {
                    return Err(blocked(
                        "the session's input box shows text off a frame not yet drawn whole",
                    ))
                }
                None => {
                    return Err(blocked(
                        "the session's input box cannot be read to clear it",
                    ))
                }
            };
            if let Some(text) = &text {
                // Octoboard's own message still in the box, left unsent, is no draft to discard.
                if !draft.typed_since_message()
                    && self.holds_only_the_message(text, draft.message_text())
                {
                    let tell = self.note_own_message_held(draft.message_count());
                    drop(draft);
                    drop(_input);
                    if tell {
                        self.tell_own_message_held();
                    }
                    return Err(blocked(
                        "the session's input box holds only the message written before, which \
                         is not cleared",
                    ));
                }
            }
            drop(draft);
            let rows = self.input_box_rows(&self.screen_text_undimmed());
            if text.is_some() && self.clear_held(rows.as_deref()) {
                return Err(blocked(
                    "a key clearing the session's input box was not seen to change it, and the \
                     box has not changed since",
                ));
            }
            (text, rows, self.input_writes.load(Ordering::Acquire))
        };
        let Some(draft) = draft else {
            // Emptied since it was read: nothing is left to clear.
            return self.write_empty_message_input(message, prompt, true);
        };
        let (cleared, presses, changed, unseen) = clear_input_box(
            self,
            &mut writes,
            press_timeout(),
            clear_allowance(),
            || self.takes_automatic_keys(),
        );
        // A press that showed late but left the box empty erased the draft all the same.
        if presses > 0 && (changed || cleared == Cleared::Empty) {
            self.report_discarded_draft(&draft, presses, cleared);
        }
        if unseen {
            // A press nobody saw, whatever the presses before it did: the next is not sent blind
            // on a later try either, until the box shows something other than it does now.
            let now = self.input_box_rows(&self.screen_text_undimmed());
            *self
                .unseen_clear
                .lock()
                .expect("unseen clear lock poisoned") = Some(UnseenClear {
                rows: now.or(rows).unwrap_or_default(),
                prompts: self.prompts_sent(),
                turn_ends: self.turn_ends(),
                at: Instant::now(),
            });
            tracing::warn!(session = %self.id, presses, "a key clearing the input box was not seen to change it; the message waits");
        }
        match cleared {
            Cleared::Empty => {}
            Cleared::Refused => {
                return Err(blocked(
                    "something else wrote into the session while its input box was being \
                     cleared; the message waits",
                ))
            }
            Cleared::Left => {
                return Err(blocked(
                    "the session's input box was not seen to clear; the message waits",
                ))
            }
        }
        let confirmed = super::wait::wait_for(press_timeout(), || {
            let (composer, whole, _) = self.read_box();
            (composer.as_deref() == Some("") && whole.is_some()) || self.is_frozen()
        });
        if !confirmed {
            return Err(blocked(
                "the session's input box was not seen empty off a whole frame after it was \
                 cleared; the message waits",
            ));
        }
        self.end_torn_run();
        self.write_message_into_empty_box(message, prompt, true, Some(writes))
    }

    /// Whether a press clearing the box before was not seen to change it and nothing has moved
    /// since: the box, `rows` as the screen shows it now, is the same, no prompt has been reported,
    /// no turn has ended, and [`UNSEEN_CLEAR_HOLD`] has not passed. No key goes in blind again
    /// meanwhile. Forgets such a press once any of them has moved, which allows one more press.
    fn clear_held(&self, rows: Option<&[String]>) -> bool {
        let mut unseen = self
            .unseen_clear
            .lock()
            .expect("unseen clear lock poisoned");
        let held = unseen.as_ref().is_some_and(|before| {
            rows == Some(before.rows.as_slice())
                && self.prompts_sent() == before.prompts
                && self.turn_ends() == before.turn_ends
                && before.at.elapsed() < UNSEEN_CLEAR_HOLD
        });
        if !held {
            *unseen = None;
        }
        held
    }

    /// How many turns the session's agent has ended, as the app state counts them, or 0 where the
    /// session is not registered with one (a stand-in under test).
    fn turn_ends(&self) -> u64 {
        let state = self
            .title
            .lock()
            .expect("title observer lock poisoned")
            .state
            .upgrade();
        state.map_or(0, |state| state.turn_ends(&self.id))
    }

    /// Notes that a clear was held back because the box holds only the message Octoboard wrote
    /// last, `write` ([`Draft::message_count`]), and says whether that has now lasted
    /// [`super::UNSENT_RETRY_BOUND`] for that write without the user being told, as they then are
    /// ([`Self::tell_own_message_held`], once the caller has let go of its locks). Each write has
    /// a wait of its own, even of the same text.
    fn note_own_message_held(&self, write: u64) -> bool {
        let mut held = self
            .own_message_held
            .lock()
            .expect("own message lock poisoned");
        match held.as_mut() {
            Some((at, since, told)) if *at == write => {
                let due = !*told && since.elapsed() >= super::UNSENT_RETRY_BOUND;
                *told |= due;
                due
            }
            _ => {
                *held = Some((write, Instant::now(), false));
                false
            }
        }
    }

    /// Tells every client that the message Octoboard wrote last waits unsent in the box, holding
    /// the queue: `message_not_sent` with no retry, which a prompt reported after now withdraws.
    fn tell_own_message_held(&self) {
        *self
            .unsent_notice
            .lock()
            .expect("unsent notice lock poisoned") = Some(self.prompts_sent());
        tracing::warn!(session = %self.id, "the message written last waits unsent in the input box, holding the queue");
        self.broadcast_notice(crate::messaging::write_queue::unsent_notice(
            self.agent, false,
        ));
    }

    /// Tells every client about the session that `draft`, the text its input box showed, was
    /// cleared out of the way of a message, all of it (`cleared` [`Cleared::Empty`]) or only part,
    /// so the user can put it back.
    fn report_discarded_draft(&self, draft: &str, presses: usize, cleared: Cleared) {
        tracing::warn!(
            session = %self.id,
            agent = %self.agent.label(),
            chars = draft.chars().count(),
            presses,
            outcome = ?cleared,
            "cleared the draft in the input box out of the way of a message; the draft is discarded"
        );
        let notice = if cleared == Cleared::Empty {
            Notice::new(
                notice_code::DRAFT_DISCARDED,
                format!(
                    "The draft in the session's input box was cleared so that a message would not \
                     be sent together with it. It held:\n{draft}"
                ),
                &[("agent", self.agent.label()), ("detail", draft)],
            )
        } else {
            Notice::new(
                notice_code::DRAFT_DISCARDED,
                format!(
                    "Part of the draft in the session's input box was cleared so that a message \
                     would not be sent together with it, and the message was not written. Before \
                     that it held:\n{draft}"
                ),
                &[
                    ("agent", self.agent.label()),
                    ("detail", draft),
                    ("cleared", "part"),
                ],
            )
        };
        self.broadcast_notice(notice);
    }
}

#[cfg(test)]
mod tests;
