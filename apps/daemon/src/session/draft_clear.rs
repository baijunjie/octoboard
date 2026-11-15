//! Clearing a draft out of the way of a message past the draft's grace, where the agent's native
//! stash cannot take it: past the grace delivery comes before the draft, so the draft is cleared,
//! handed to every client in a `draft_discarded` notice, and the message written only once the
//! screen shows the box empty. Where the box cannot be cleared — no key is known for the agent, a
//! press was not seen, the clear left text behind, the box cannot be read off a whole frame, or it
//! holds only the message Octoboard wrote before — the message waits, but only for
//! [`UNCLEARABLE_DRAFT_WAIT`]: then it goes into the box as it stands, with whatever the box holds,
//! and every client is told in a `message_sent_with_draft` notice. The rules are in
//! `docs/product/draft-preservation.md`, sections "Clearing a draft past the grace period", "When
//! the composer cannot be cleared" and "The draft grace period".
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
use super::{BoxIs, LiveSession};
use crate::platform::term::{Input, PartialWrite};
use crate::protocol::{notice_code, Notice};

/// How long a press nobody saw holds a draft clear back at most ([`LiveSession::clear_held`]):
/// past it one more press goes in, as it does once anything else has moved.
const UNSEEN_CLEAR_HOLD: Duration = Duration::from_secs(30);

/// How long a message past its grace waits on a draft in the input box that it cannot get out of
/// its way, from its first attempt that found one ([`LiveSession::unclearable_wait_over`]), however
/// much the clears meanwhile took out: past it, the message goes into the box as the box stands and
/// may be sent together with that text. A message is never to wait without end on a box Octoboard
/// cannot empty; the stash and the clear are tried first on every attempt meanwhile. A message
/// left unsent that waits for the user holds later ones as long, and a message held by a box that
/// may be hidden behind a view only the user can close has the user told after as long.
pub(super) const UNCLEARABLE_DRAFT_WAIT: Duration = Duration::from_secs(30);

/// What a message's wait on the box is for, as its log names it ([`LiveSession::wait_on_box`]).
#[derive(Debug, Clone, Copy)]
enum WaitCause {
    /// Text in the box that cannot be got out of the message's way.
    Text,
    /// A hold on the session's input.
    InputHold,
    /// A box that may be hidden behind a view only the user can close.
    HiddenBox,
}

/// What the input box held when a message went in with it ([`LiveSession::report_sent_with_draft`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Held {
    /// A draft, or text that could not be read.
    Draft,
    /// Only the message Octoboard wrote before, left unsent.
    Message,
    /// The message Octoboard wrote before, left unsent, with text typed after it.
    MessageAndDraft,
}

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

/// What a write past the grace found in the box before any key went in.
enum Found {
    /// Nothing: the box emptied since it was read.
    Empty,
    /// A draft to clear, with the box's rows and the count of writes so far.
    Draft {
        text: String,
        rows: Option<Vec<String>>,
        writes: u64,
    },
    /// Text no key may clear now, and why.
    Unclearable(&'static str),
}

impl LiveSession {
    /// Writes `message`, past the draft's grace, into an input box that shows text no stash can
    /// take: clears the box, waits for a whole frame to show it empty, then writes the message as
    /// into any empty box, refused if anything else has written since the last press. Refused,
    /// with nothing of the message written and the message left to be retried, where the box
    /// cannot be read off a whole frame, a key the user typed may not be drawn yet or is not
    /// complete, a dialog is up, or the box is not seen to empty. Where no key may clear it — the
    /// agent's box has no measured clearing key, a press before was not seen and the box has not
    /// changed since, the box shows text off a frame not drawn whole or cannot be read, or it holds
    /// only the message Octoboard wrote before (which is no draft) — the message is refused the
    /// same way until [`UNCLEARABLE_DRAFT_WAIT`] has passed for it, and then written into the box
    /// as it stands ([`BoxIs::Unclearable`]). Presses that removed something are reported.
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
            return self.write_unclearable(
                message,
                prompt,
                "the session's input box holds text and no key is known to clear it",
            );
        }
        self.wait_for_whole_frame();
        let found = {
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
            // A sequence never completed takes no clearing key, but no longer keeps out a message
            // written as the box stands, once the wait on the box is over.
            let incomplete = !draft.at_input_boundary(now);
            if incomplete && !draft.sequence_abandoned(now) {
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
            match composer {
                _ if incomplete => Found::Unclearable(
                    "a key typed into the session never got the rest of its bytes",
                ),
                Some(text) if text.is_empty() => {
                    if whole.is_some() {
                        self.end_unclearable_wait();
                    }
                    Found::Empty
                }
                // Octoboard's own message still in the box, left unsent, is no draft to discard.
                Some(text)
                    if !draft.typed_since_message()
                        && self.holds_only_the_message(&text, draft.message_text()) =>
                {
                    Found::Unclearable(
                        "the session's input box holds only the message written before, which \
                         is not cleared",
                    )
                }
                Some(_) if whole.is_none() => Found::Unclearable(
                    "the session's input box shows text off a frame not yet drawn whole",
                ),
                Some(text) => {
                    drop(draft);
                    // The wait on a draft runs from the first attempt that found one, whatever the
                    // clears take out of it meanwhile.
                    self.unclearable_wait_over(prompt);
                    let rows = self.input_box_rows(&self.screen_text_undimmed());
                    if self.clear_held(rows.as_deref()) {
                        Found::Unclearable(
                            "a key clearing the session's input box was not seen to change it, \
                             and the box has not changed since",
                        )
                    } else {
                        Found::Draft {
                            text,
                            rows,
                            writes: self.input_writes.load(Ordering::Acquire),
                        }
                    }
                }
                None => Found::Unclearable("the session's input box cannot be read to clear it"),
            }
        };
        let (draft, rows, mut writes) = match found {
            // Emptied since it was read: nothing is left to clear.
            Found::Empty => return self.write_empty_message_input(message, prompt, true),
            Found::Unclearable(reason) => return self.write_unclearable(message, prompt, reason),
            Found::Draft { text, rows, writes } => (text, rows, writes),
        };
        let (cleared, presses, changed, unseen) = clear_input_box(
            self,
            &mut writes,
            press_timeout(),
            clear_allowance(),
            || self.takes_automatic_keys(),
        );
        // A clear that did not end in an empty box, once the wait on the draft is over, has the
        // message go in as the box stands, the draft as read before the presses handed back with
        // it.
        let left = cleared != Cleared::Empty;
        let force = left && self.unclearable_wait_over(prompt);
        // A press that showed late but left the box empty erased the draft all the same. Where
        // the message is to go in now, the draft is handed back with it, and told here only if it
        // does not go in.
        let removed = presses > 0 && (changed || cleared == Cleared::Empty);
        if removed && !force {
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
        if force {
            let written = self.write_message_into_box(
                message,
                prompt,
                true,
                BoxIs::Unclearable {
                    unsent: false,
                    read_before: Some(&draft),
                },
            );
            // Refused after all: what the presses removed is never lost unseen.
            if written.is_err() && removed {
                self.report_discarded_draft(&draft, presses, cleared);
            }
            return written;
        }
        let refusal = match cleared {
            Cleared::Empty => None,
            Cleared::Refused => Some(
                "something else wrote into the session while its input box was being cleared; \
                 the message waits",
            ),
            Cleared::Left => {
                Some("the session's input box was not seen to clear; the message waits")
            }
        };
        if let Some(refusal) = refusal {
            return Err(blocked(refusal));
        }
        let confirmed = super::wait::wait_for(press_timeout(), || {
            let (composer, whole, _) = self.read_box();
            (composer.as_deref() == Some("") && whole.is_some()) || self.is_frozen()
        });
        if !confirmed {
            if self.unclearable_wait_over(prompt) {
                // The clear emptied the box and has told what it removed already.
                return self.write_message_into_box(
                    message,
                    prompt,
                    true,
                    BoxIs::Unclearable {
                        unsent: false,
                        read_before: None,
                    },
                );
            }
            return Err(blocked(
                "the session's input box was not seen empty off a whole frame after it was \
                 cleared; the message waits",
            ));
        }
        self.end_torn_run();
        self.write_message_into_box(message, prompt, true, BoxIs::Cleared(writes))
    }

    /// Writes `message` into a box that holds text no key may clear now, as the box stands, once
    /// [`UNCLEARABLE_DRAFT_WAIT`] has passed for it; until then refuses it, for `reason`, to be
    /// retried.
    fn write_unclearable(
        &self,
        message: &[Input],
        prompt: &str,
        reason: &'static str,
    ) -> Result<(), PartialWrite> {
        if !self.unclearable_wait_over(prompt) {
            return Err(blocked(reason));
        }
        self.write_message_into_box(
            message,
            prompt,
            true,
            BoxIs::Unclearable {
                unsent: false,
                read_before: None,
            },
        )
    }

    /// Notes that the message `prompt` found a draft in the box it could not get out of its way,
    /// starting its wait where this is the first time, and says whether [`UNCLEARABLE_DRAFT_WAIT`]
    /// has passed since. The wait is the message's own, kept under its identity
    /// ([`LiveSession::message_key`]): one for another message starts afresh, and a message that
    /// went in ends it ([`Self::end_unclearable_wait`]).
    pub(super) fn unclearable_wait_over(&self, prompt: &str) -> bool {
        self.wait_on_box(prompt, WaitCause::Text).0
    }

    /// [`Self::unclearable_wait_over`] for a message that a hold on the session's input keeps out
    /// (`InputSwap`): the same wait, logged as what it is.
    pub(super) fn input_hold_wait_over(&self, prompt: &str) -> bool {
        self.wait_on_box(prompt, WaitCause::InputHold).0
    }

    /// The wait of the message `prompt` on the box ([`Self::unclearable_wait_over`]), started where
    /// it was not, for `cause`, as whether it is over and whether the user has been told of it
    /// ([`Self::waits_on_input_due`]).
    fn wait_on_box(&self, prompt: &str, cause: WaitCause) -> (bool, bool) {
        let key = self.message_key(prompt);
        let mut since = self
            .unclearable_since
            .lock()
            .expect("unclearable wait lock poisoned");
        match since.as_ref() {
            Some((waiting, at, told)) if *waiting == key => {
                (at.elapsed() >= UNCLEARABLE_DRAFT_WAIT, *told)
            }
            _ => {
                *since = Some((key, Instant::now(), false));
                match cause {
                    WaitCause::Text => tracing::warn!(
                        session = %self.id,
                        wait_secs = UNCLEARABLE_DRAFT_WAIT.as_secs(),
                        "the session's input box holds text in a message's way; the message waits, \
                         then goes in with that text where it cannot be cleared"
                    ),
                    WaitCause::InputHold => tracing::warn!(
                        session = %self.id,
                        wait_secs = UNCLEARABLE_DRAFT_WAIT.as_secs(),
                        "a hold on the session's input (a partly written message, typing not yet \
                         replayed) keeps a message out; the message waits, then goes in as the box \
                         stands"
                    ),
                    WaitCause::HiddenBox => tracing::warn!(
                        session = %self.id,
                        wait_secs = UNCLEARABLE_DRAFT_WAIT.as_secs(),
                        "the session's input box cannot be read after keys that may have brought \
                         text into it; the message waits, and the user is told after a while"
                    ),
                }
                (false, false)
            }
        }
    }

    /// Notes that the message `prompt` waits on a box that may be hidden behind a view only the
    /// user can close (`PastGraceRefusal::waits_on_user`), and says whether the user is now to be
    /// told: once its wait has lasted [`UNCLEARABLE_DRAFT_WAIT`], once for that message.
    pub(super) fn waits_on_input_due(&self, prompt: &str) -> bool {
        let (over, told) = self.wait_on_box(prompt, WaitCause::HiddenBox);
        if !over || told {
            return false;
        }
        if let Some((_, _, told)) = self
            .unclearable_since
            .lock()
            .expect("unclearable wait lock poisoned")
            .as_mut()
        {
            *told = true;
        }
        true
    }

    /// Tells every client about the session that a message waits on its input box, which may be
    /// hidden behind a list or another view, until the user closes it or sends or clears what the
    /// box holds: `message_waits_on_input`.
    pub(super) fn tell_waits_on_input(&self) {
        tracing::warn!(
            session = %self.id,
            agent = %self.agent.label(),
            "a message has waited on an input box that cannot be read after keys that may have \
             brought text into it; the user is told"
        );
        self.broadcast_notice(Notice::new(
            notice_code::MESSAGE_WAITS_ON_INPUT,
            "A message waits on the session's input box, which cannot be read and may be hidden \
             behind a list or another view. Close it, or send or clear what the input box holds, \
             so the message can go in.",
            &[("agent", self.agent.label())],
        ));
    }

    /// Ends the wait of a message on text no key could clear, once a message has gone in.
    pub(super) fn end_unclearable_wait(&self) {
        *self
            .unclearable_since
            .lock()
            .expect("unclearable wait lock poisoned") = None;
    }

    /// Tells every client about the session that a message went into its input box with `held`,
    /// the text the box showed before the write (`None` where it showed none or could not be
    /// read), so the two may be sent together; `kind` says what that text was.
    pub(super) fn report_sent_with_draft(&self, held: Option<&str>, kind: Held) {
        tracing::warn!(
            session = %self.id,
            agent = %self.agent.label(),
            chars = held.map_or(0, |text| text.chars().count()),
            held = ?kind,
            "the input box could not be emptied in time; wrote a message into it as it stood, with \
             its Enter, so it was sent together with what the box held (the unsent check reports it \
             if the Enter did not take)"
        );
        const UNSENT_CHECK: &str =
            "If the Enter did not take, the message is reported as not sent.";
        let agent = self.agent.label();
        let notice = match held {
            _ if kind == Held::Message => Notice::new(
                notice_code::MESSAGE_SENT_WITH_DRAFT,
                format!(
                    "The session's input box still held the message written before, unsent, so a \
                     message was written into it with its Enter and sent together with that \
                     message. {UNSENT_CHECK}"
                ),
                &[("agent", agent), ("held", "message")],
            ),
            Some(text) if kind == Held::MessageAndDraft => Notice::new(
                notice_code::MESSAGE_SENT_WITH_DRAFT,
                format!(
                    "The session's input box still held the message written before, unsent, with \
                     text typed after it, so a message was written into it with its Enter and sent \
                     together with both. {UNSENT_CHECK} The box held:\n{text}"
                ),
                &[
                    ("agent", agent),
                    ("detail", text),
                    ("held", "message_and_draft"),
                ],
            ),
            Some(text) => Notice::new(
                notice_code::MESSAGE_SENT_WITH_DRAFT,
                format!(
                    "The session's input box could not be cleared in time, so a message was written \
                     into it with its Enter and sent together with what it held. {UNSENT_CHECK} It \
                     held:\n{text}"
                ),
                &[("agent", agent), ("detail", text)],
            ),
            None => Notice::new(
                notice_code::MESSAGE_SENT_WITH_DRAFT,
                format!(
                    "The session's input box could not be cleared in time, so a message was written \
                     into it with its Enter and sent together with whatever it held. {UNSENT_CHECK}"
                ),
                &[("agent", agent)],
            ),
        };
        self.broadcast_notice(notice);
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

    /// Tells every client that the presses of a clear emptied the box of `draft`, found so only by
    /// the write of the message that followed them (`LiveSession::write_message_into_box`).
    pub(super) fn report_draft_cleared(&self, draft: &str) {
        self.report_discarded_draft(draft, 0, Cleared::Empty);
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
