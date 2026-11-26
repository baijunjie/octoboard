//! Clearing a draft out of the way of a message past the draft's grace, where the agent's native
//! stash cannot take it: past the grace delivery comes before the draft, so the draft is cleared,
//! handed to every client in a `draft_discarded` notice, and the message written only once the
//! screen shows the box empty. Messages of Octoboard's own that the agent held for the end of a
//! turn and put back in the box at a cancel, the latest alone or several in the order written,
//! are cleared the same way, but they are no draft of the user's: they are queued to be written
//! again right after the message they were cleared for (`crate::messaging::cleared_messages`),
//! and nobody is told, since nothing is lost; only text the user typed after them is told as a
//! draft. Only where one cannot be written again is every client told, in a
//! `message_cleared_after_cancel` notice that carries its text. Where the box cannot be cleared —
//! no key is known for the agent, a press was not seen, the clear left text behind, the box
//! cannot be read off a whole frame, or it holds only the message Octoboard wrote before, left
//! unsent there — the message waits, but only for [`UNCLEARABLE_DRAFT_WAIT`]: then it goes into
//! the box as it stands, with whatever the box holds, and every client is told in a
//! `message_sent_with_draft` notice. The rules are in
//! `docs/product/draft-preservation.md`, sections "Clearing a draft past the grace period", "When
//! the composer cannot be cleared" and "The draft grace period".
//!
//! The keys are the ones an interrupt also uses (`crate::session::input_box_clear`), each a write
//! through `LiveSession::write_keys_if`: a run of Ctrl-U presses, each seen on the screen before
//! the next, for an agent whose Ctrl-U is measured to do nothing but delete a line of its box
//! (`crate::session::screen::ScreenReader::clears_input_box_with_ctrl_u`); or, at the idle prompt,
//! a single press of the key a provider declares to empty its whole box there
//! (`crate::session::screen::ScreenReader::input_box_clearing_key`). The session's queue is not
//! paused around them: they are typed from inside the queue's own write of the message, which
//! holds the session's drain, so no other message of Octoboard's can be written meanwhile. Keys the
//! user types meanwhile go straight in and end the run, since every press expects the count of
//! writes past its own: the message then waits rather than join what they typed.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::draft::{HeldMessage, ToldLost};
use super::input_box_clear::{clear_allowance, clear_input_box, press_timeout, Cleared};
use super::swap::{archived_refusal, blocked};
use super::text::bare;
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
/// may be hidden behind a view only the user can close has the user told after as long. A new
/// refusal or hold on a message's path gets this same bound, kept per message under
/// [`LiveSession::message_key`], never under its text, per session or per process; a test of a
/// forced write also sends the next message and asserts it waits out its own grace and bound.
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

/// The messages of Octoboard's that the agent gave back at a cancel
/// ([`LiveSession::given_back_at_a_cancel`]), to be cleared.
struct GivenBack {
    /// The messages, oldest first, each as it reads and under the identity the queue wrote it
    /// with, where it wrote it.
    messages: Vec<HeldMessage>,
    /// Whether they are several messages written while the turn ran, told by the texts the
    /// typing record keeps of them (`Draft::held_messages`), rather than the latest one alone.
    several: bool,
    /// What the box showed after them: text of the user's own, cleared with them and told as a
    /// draft.
    draft: Option<String>,
}

impl GivenBack {
    /// The latest message alone, as the box showed it.
    fn latest(text: &str, id: Option<String>) -> Self {
        Self {
            messages: vec![HeldMessage {
                text: text.to_string(),
                id,
                records_prompt: true,
            }],
            several: false,
            draft: None,
        }
    }
}

/// The latest of `held`, the messages written while the turn ran, that `shown`, the input box's
/// text, begins with: one after another in the order they were written in, compared without
/// whitespace ([`bare`]), so neither the wrapping nor what an agent puts between two of them (an
/// empty row, a row break) counts. As how many they are and what the box shows after them,
/// trimmed; the longest such run. What follows them has to be set off from the last by
/// whitespace: text that only begins as a message does is none of it. Never for an empty box.
fn given_back_run(shown: &str, held: &[HeldMessage]) -> Option<(usize, String)> {
    let bare_shown = bare(shown);
    (1..=held.len()).rev().find_map(|count| {
        let run: String = held[held.len() - count..]
            .iter()
            .map(|message| bare(&message.text))
            .collect();
        if run.is_empty() || !bare_shown.starts_with(&run) {
            return None;
        }
        let rest = after_bare(shown, run.chars().count());
        (rest.is_empty() || rest.starts_with(char::is_whitespace))
            .then(|| (count, rest.trim().to_string()))
    })
}

/// `text` past its first `count` characters that are not whitespace.
fn after_bare(text: &str, count: usize) -> &str {
    let mut left = count;
    text.trim_start_matches(|c: char| {
        if left == 0 {
            return false;
        }
        if !c.is_whitespace() {
            left -= 1;
        }
        true
    })
}

/// Whether the input box's `shown` text is `message` and nothing else ([`bare`]); never for an
/// empty box.
fn same_text(shown: &str, message: &str) -> bool {
    let shown = bare(shown);
    !shown.is_empty() && shown == bare(message)
}

/// What the input box's `shown` text holds after `message`, which it begins with ([`bare`]):
/// text set off from the message by whitespace, trimmed. `None` where the box does not begin
/// with the message or nothing follows it.
fn text_after(shown: &str, message: &str) -> Option<String> {
    let message = bare(message);
    if message.is_empty() || !bare(shown).starts_with(&message) {
        return None;
    }
    let rest = after_bare(shown, message.chars().count());
    (rest.starts_with(char::is_whitespace) && !rest.trim().is_empty())
        .then(|| rest.trim().to_string())
}

/// Whether the input box's `shown` text is a part of `message` ([`bare`]); never for an empty
/// box.
fn part_of(shown: &str, message: &str) -> bool {
    let shown = bare(shown);
    !shown.is_empty() && bare(message).contains(&shown)
}

/// What a write past the grace found in the box before any key went in.
enum Found {
    /// Nothing: the box emptied since it was read.
    Empty,
    /// A draft to clear, with the box's rows and the count of writes so far, or, with `given_back`,
    /// the message Octoboard wrote last that the agent gave back at a cancel, under the identity the
    /// queue wrote it with.
    Draft {
        text: String,
        rows: Option<Vec<String>>,
        writes: u64,
        given_back: Option<GivenBack>,
        /// Whether the text is what is left of such a message, cleared in part before and queued
        /// to be written again: clearing it tells nobody.
        remainder: bool,
        /// What follows the text of messages already queued again that the box begins with:
        /// the only draft of the user's there, where the box holds both.
        after_queued: Option<String>,
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
        finish: super::Finish<'_>,
    ) -> Result<(), PartialWrite> {
        if !crate::session::screen::clears_input_box(crate::adapter::provider(self.agent).screen())
        {
            return self.write_unclearable(
                message,
                prompt,
                "the session's input box holds text and no key is known to clear it",
                finish,
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
            // The box holds nothing but the message Octoboard wrote last, with nothing typed
            // since.
            let only_the_message = composer.as_deref().is_some_and(|text| {
                !text.is_empty()
                    && !draft.typed_since_message()
                    && self.holds_only_the_message(text, draft.message_text())
            });
            // Or the very text of that message after keys typed since: a cancel the user typed
            // in the terminal is such a key, and what the agent then put back is the message, not
            // a draft. Told by its text alone, never by a placeholder a paste of the user's own
            // would show too.
            let its_text_after_keys = composer.as_deref().is_some_and(|text| {
                draft.typed_since_message()
                    && draft
                        .message_text()
                        .is_some_and(|message| same_text(text, message))
            });
            // What is left of a message already cleared in part and queued to be written again:
            // neither that message any more nor a draft of the user's.
            // Only while nothing has been typed since: text of the user's own that happens to be a
            // part of the message is a draft.
            let remainder = draft.was_queued_again()
                && !draft.typed_since_queued_again()
                && composer.as_deref().is_some_and(|text| {
                    draft
                        .queued_again_text()
                        .is_some_and(|message| part_of(text, message))
                });
            // Or that text with more after it: what follows is the user's, and the messages it
            // follows are no draft of theirs.
            let after_queued = (draft.was_queued_again() && !remainder)
                .then(|| {
                    composer.as_deref().and_then(|text| {
                        draft
                            .queued_again_text()
                            .and_then(|message| text_after(text, message))
                    })
                })
                .flatten();
            let given_back = ((only_the_message || its_text_after_keys)
                && !draft.was_queued_again()
                && self.given_back_at_a_cancel(&draft))
            .then(|| {
                GivenBack::latest(
                    composer.as_deref().unwrap_or_default(),
                    draft.message_id().map(str::to_string),
                )
            })
            // Or several of the messages written while the turn ran, given back together, with
            // or without text of the user's own after them.
            .or_else(|| {
                composer
                    .as_deref()
                    .and_then(|text| self.several_given_back(&draft, text))
            });
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
                // Octoboard's own message still in the box, left unsent, is no draft to discard;
                // one the agent held for the end of a turn and gave back at a cancel is cleared
                // as a draft is, below, and told as what it is; what is left of one already
                // handled (queued again, or told as possibly not carried out) is cleared with
                // nobody told.
                Some(_) if only_the_message && given_back.is_none() && !remainder => {
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
                    let rows = self.input_box_rows(&self.box_screen());
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
                            given_back,
                            remainder,
                            after_queued,
                        }
                    }
                }
                None => Found::Unclearable("the session's input box cannot be read to clear it"),
            }
        };
        let (draft, rows, mut writes, given_back, remainder, after_queued) = match found {
            // Emptied since it was read: nothing is left to clear.
            Found::Empty => return self.write_empty_message_input(message, prompt, true, finish),
            Found::Unclearable(reason) => {
                return self.write_unclearable(message, prompt, reason, finish)
            }
            Found::Draft {
                text,
                rows,
                writes,
                given_back,
                remainder,
                after_queued,
            } => (text, rows, writes, given_back, remainder, after_queued),
        };
        // What the presses removed is told as a draft cleared; a message given back is written
        // again instead, and what a clear before left of one is nobody's to keep.
        let write_again = |presses: usize, cleared: Cleared| {
            if let Some(given_back) = &given_back {
                self.write_again_or_report(given_back, presses, cleared, false);
            }
        };
        // What the user typed after messages given back is the only draft there is to tell.
        let own_text = match &given_back {
            Some(given_back) => given_back.draft.as_deref(),
            None if remainder => None,
            None => Some(after_queued.as_deref().unwrap_or(&draft)),
        };
        let report = |presses: usize, cleared: Cleared| {
            write_again(presses, cleared);
            if let Some(text) = own_text {
                self.report_discarded_draft(text, presses, cleared);
            }
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
            report(presses, cleared);
        }
        if unseen {
            // A press nobody saw, whatever the presses before it did: the next is not sent blind
            // on a later try either, until the box shows something other than it does now.
            let now = self.input_box_rows(&self.box_screen());
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
            // A message given back that the presses took something out of is queued again here,
            // whatever the write then finds: it is spoilt either way, and the write's own notices
            // speak of a draft. What is left of it goes in with the message as the box shows it.
            let told = removed && given_back.is_some();
            if told {
                write_again(presses, cleared);
            }
            let written = self.write_message_into_box(
                message,
                prompt,
                true,
                BoxIs::Unclearable {
                    unsent: false,
                    read_before: match &given_back {
                        Some(given_back) => given_back.draft.as_deref(),
                        None => Some(after_queued.as_deref().unwrap_or(&draft)),
                    },
                },
                finish,
            );
            // Refused after all: what the presses removed is never lost unseen.
            if written.is_err() && removed {
                if !told {
                    report(presses, cleared);
                } else if let Some(text) = own_text {
                    self.report_discarded_draft(text, presses, cleared);
                }
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
                    finish,
                );
            }
            return Err(blocked(
                "the session's input box was not seen empty off a whole frame after it was \
                 cleared; the message waits",
            ));
        }
        self.end_torn_run();
        self.write_message_into_box(message, prompt, true, BoxIs::Cleared(writes), finish)
    }

    /// Notes that an interrupt of Octoboard's own cleared `shown`, the text the input box held
    /// after its key, with `presses` presses that left the box as `cleared` says. Where that text
    /// was the message Octoboard wrote last, which the agent held and gave back at the cancel
    /// ([`Self::given_back_at_a_cancel`]), the message is queued to be written again, held until
    /// the next message is written into the session so that the agent stays stopped, or, where it
    /// cannot be written again, every client is told ([`Self::write_again_or_report`]). Where the
    /// box showed only the provider's placeholder for a long paste, which may stand for that
    /// message or for the interrupted prompt, the message is not written again and every client
    /// is told, with its text. Where it began with messages written while the turn ran
    /// ([`Self::several_given_back`]), they are queued again the same way, and text the user typed
    /// after them is told as a draft cleared (`draft_discarded`), as a write past the grace tells
    /// it. Anything else the interrupt cleared (the interrupted prompt) is nothing to write again.
    pub(super) fn given_back_cleared_by_interrupt(
        &self,
        shown: &str,
        presses: usize,
        cleared: Cleared,
    ) {
        let given_back = {
            let draft = self.draft.lock().expect("draft lock poisoned");
            // Known limit: where a cancel report already queued the messages again, text the user
            // typed after them is not told here as a draft. Only a provider declaring both cancel
            // behaviours with the interrupt key the one that gives back reaches it, and none does;
            // a fix would copy the write path's `after_queued` branch.
            if draft.was_queued_again() || !self.given_back_at_a_cancel(&draft) {
                return;
            }
            let Some(message) = draft.message_text() else {
                return;
            };
            if same_text(shown, message) {
                let id = draft.message_id().map(str::to_string);
                GivenBack::latest(shown, id)
            } else if let Some(several) = self.several_given_back(&draft, shown) {
                several
            } else if crate::adapter::provider(self.agent)
                .screen()
                .collapsed_paste(shown.trim())
            {
                // The placeholder of a long paste: the message, or the interrupted prompt, which
                // nothing here can tell apart. Written again it could be delivered twice, and
                // dropped unseen it could be lost, so it is not written again and the user is
                // told, with the text of the message itself.
                GivenBack::latest(message, None)
            } else {
                return;
            }
        };
        self.write_again_or_report(&given_back, presses, cleared, true);
        // What the user typed after the messages went with them, and is theirs to have back.
        if let Some(text) = &given_back.draft {
            self.report_discarded_draft(text, presses, cleared);
        }
    }

    /// The messages written while the turn ran that `shown`, the input box's text, begins with,
    /// where they are more than the latest one alone or text follows them
    /// ([`given_back_run`]): several messages the agent gave back at a cancel, with what the user
    /// typed after them. Told by their texts alone, whatever was typed since: a cancel typed in
    /// the terminal is a key, and what the agent then put back is the messages, not a draft.
    /// `None` where the agent gives nothing back, or where a clear before already queued them
    /// again.
    fn several_given_back(&self, draft: &super::draft::Draft, shown: &str) -> Option<GivenBack> {
        if !self.cancel_gives_back || draft.was_queued_again() {
            return None;
        }
        let held: Vec<HeldMessage> = draft.held_messages().cloned().collect();
        let (count, rest) = given_back_run(shown, &held)?;
        Some(GivenBack {
            messages: held[held.len() - count..].to_vec(),
            several: true,
            draft: (!rest.is_empty()).then_some(rest),
        })
    }

    /// Notes that a turn of the session's agent began: what was written while the turn before it
    /// ran is held by the agent no longer (`Draft::turn_began`).
    pub(crate) fn note_turn_began(&self) {
        self.draft.lock().expect("draft lock poisoned").turn_began();
    }

    /// The messages Octoboard wrote while the turn ran that the agent still held when it reported
    /// the turn cancelled, and dropped, oldest first, each as the identity the queue wrote it
    /// under (where it wrote it) and its text; none for a launch whose cancel drops nothing, as
    /// its provider answered for it (`crate::adapter::AgentProvider::launch_cancel_drops_held_messages`,
    /// by default the reader's `cancel_drops_held_messages`). They are taken off
    /// the typing record, which notes them as queued again: where the cancel was one that gave
    /// them back instead, what the box then shows is what is left of messages already queued
    /// again, cleared with nobody told, and never the messages themselves a second time.
    pub(crate) fn take_dropped_at_cancel(&self) -> Vec<(Option<String>, String)> {
        if !self.cancel_drops() {
            return Vec::new();
        }
        let mut draft = self.draft.lock().expect("draft lock poisoned");
        let count = draft.held_messages().count();
        let held: Vec<_> = draft
            .held_messages()
            .map(|message| (message.id.clone(), message.text.clone()))
            .collect();
        if count > 0 {
            draft.messages_queued_again(count);
        }
        held
    }

    /// Notes that the session's turn ended, not cancelled, for an agent that reports each message
    /// it takes as a prompt sent: the messages written while the turn ran that it has not reported
    /// taking are kept apart, for [`Self::take_untaken`] once the agent has had time to take them
    /// as a turn of their own. Says whether there were any, and hands back, each as the
    /// identity the queue wrote it under and its text, the oldest ones no longer kept for that
    /// (`Draft::turn_ended`), to be told at once.
    pub(crate) fn note_turn_ended_untaken(
        &self,
        now: Instant,
    ) -> (bool, Vec<(Option<String>, String)>) {
        let (any, pushed_off) = self
            .draft
            .lock()
            .expect("draft lock poisoned")
            .turn_ended(now);
        let pushed_off = pushed_off
            .into_iter()
            .map(|message| (message.id, message.text))
            .collect();
        (any, pushed_off)
    }

    /// Notes that the message `text`, written under `id`, has just been told as not carried out,
    /// as `told`: a report of it sent a while after still finds it ([`Self::take_told_lost`]).
    pub(crate) fn note_told_lost(&self, told: ToldLost, id: Option<String>, text: String) {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .note_told_lost(Instant::now(), told, id, text);
    }

    /// The message told as not carried out ([`Self::note_told_lost`]) that `reported`, a prompt
    /// the agent reports sending, is, as what it was told, the identity the queue wrote it under
    /// and its text: it ran after all. Taken off, so it is found once.
    pub(crate) fn take_told_lost(
        &self,
        reported: &str,
    ) -> Option<(ToldLost, Option<String>, String)> {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .take_told_lost(Instant::now(), reported, self.agent)
            .map(|(told, message)| (told, message.id, message.text))
    }

    /// Whether `reported`, a prompt the agent reports sending, is a message Octoboard wrote that
    /// the agent may still take or have taken unseen (`Draft::wrote_message`): whatever it looks
    /// like, the agent sending it is that message taken.
    pub(crate) fn wrote_message(&self, reported: &str) -> bool {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .wrote_message(reported, self.agent)
    }

    /// The messages [`Self::note_turn_ended_untaken`] kept apart at a turn's end no later than
    /// `ended_by` that the agent has not reported taking since, oldest first, each as the
    /// identity the queue wrote it under (where it wrote it) and its text. They are taken off, so
    /// none is told twice.
    pub(crate) fn take_untaken(&self, ended_by: Instant) -> Vec<(Option<String>, String)> {
        self.draft
            .lock()
            .expect("draft lock poisoned")
            .take_untaken(ended_by)
            .into_iter()
            .map(|message| (message.id, message.text))
            .collect()
    }

    /// The messages Octoboard wrote while the turn ran that the agent may have dropped at the
    /// cancel it has just reported, oldest first, each as the identity the queue wrote it under
    /// (where it wrote it, which finds its sender) and its text, on a launch that cannot tell them from
    /// messages the turn took (`LiveSession::cancel_drops_unseen`): none are written again, since
    /// one the turn took would run twice. They are taken off the list so a later cancel does not
    /// name them again, and noted as handled, as [`Self::take_dropped_at_cancel`] notes the ones
    /// it takes: where the cancel gave them back to the box instead, what the box shows of them
    /// is cleared with nobody told, never written again nor told as a draft, since the notice
    /// already carries each one's text for the user to send again. None on any other launch.
    pub(crate) fn take_unseen_at_cancel(&self) -> Vec<(Option<String>, String)> {
        if !self.cancel_drops_unseen() {
            return Vec::new();
        }
        let mut draft = self.draft.lock().expect("draft lock poisoned");
        let held: Vec<_> = draft
            .held_messages()
            .map(|message| (message.id.clone(), message.text.clone()))
            .collect();
        if !held.is_empty() {
            draft.messages_queued_again(held.len());
        }
        held
    }

    /// Whether `reported`, a prompt the agent reports sending, is `written`, a message Octoboard
    /// wrote, as the provider's reader compares the two
    /// (`crate::session::screen::ScreenReader::same_as_submitted`).
    pub(crate) fn same_as_written(&self, written: &str, reported: &str) -> bool {
        crate::adapter::provider(self.agent)
            .screen()
            .same_as_submitted(written, reported)
    }

    /// The identity the queue knows the message whose write is under way by, if the queue is
    /// writing it ([`Self::message_key`] without the fallback of a direct write).
    pub(super) fn queued_as(&self) -> Option<String> {
        self.writing.lock().expect("writing lock poisoned").clone()
    }

    /// Whether the message Octoboard wrote last, which the box shows alone (with nothing typed
    /// since, or as its very text after keys such as the Esc of a cancel typed in the terminal),
    /// is one the agent took out of its box while a turn ran and put back, unsent, when the turn
    /// was cancelled
    /// (`crate::session::screen::ScreenReader::cancel_restores_held_messages`), rather than one
    /// left unsent in the box it was written into: it would be sent with the next message, or
    /// hold that message back, so it is no message to leave where it is.
    fn given_back_at_a_cancel(&self, draft: &super::draft::Draft) -> bool {
        draft.message_held_mid_turn() && self.cancel_gives_back
    }

    /// Writes `message` into a box that holds text no key may clear now, as the box stands, once
    /// [`UNCLEARABLE_DRAFT_WAIT`] has passed for it; until then refuses it, for `reason`, to be
    /// retried.
    fn write_unclearable(
        &self,
        message: &[Input],
        prompt: &str,
        reason: &'static str,
        finish: super::Finish<'_>,
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
            finish,
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
    /// read), so the two may be sent together; `kind` says what that text was. A write into the box
    /// as it stands tells exactly one notice, this one with the box read before any clear or
    /// `draft_discarded`, so the user's text is never lost unseen nor reported twice.
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

    /// Has the messages of Octoboard's that the agent gave back at a cancel, unsent, and that were
    /// cleared from the input box out of the way of the next one, all of them (`cleared`
    /// [`Cleared::Empty`]) or only part, written again right after that one, in the order they
    /// were written in (`crate::messaging::cleared_messages`), and tells nobody: nothing is lost.
    /// Where one cannot be written again (the queue did not write it, it was more than text, it
    /// has been written again as often as a message is, or the daemon is stopping), every client
    /// is told it was not carried out, in a notice that carries its text. With `held`, no next
    /// message is being written (an interrupt cleared them): they are written again after
    /// whichever message is written into the session next.
    fn write_again_or_report(
        &self,
        given_back: &GivenBack,
        presses: usize,
        cleared: Cleared,
        held: bool,
    ) {
        let state = self
            .title
            .lock()
            .expect("title observer lock poisoned")
            .state
            .upgrade();
        // Latest first: each is put at the front of the queue, so the oldest ends up first.
        let mut lost = Vec::new();
        let mut again = 0;
        for message in given_back.messages.iter().rev() {
            let queued = message.id.as_deref().is_some_and(|id| {
                state
                    .as_ref()
                    .is_some_and(|state| state.redeliver_cleared_message(&self.id, id, held))
            });
            if queued {
                again += 1;
            } else {
                lost.push(message);
            }
        }
        if given_back.several {
            // Queued again or told, none of them is held any more, and what is left of their
            // text in the box is no draft.
            self.draft
                .lock()
                .expect("draft lock poisoned")
                .messages_queued_again(given_back.messages.len());
        } else if again > 0 {
            self.draft
                .lock()
                .expect("draft lock poisoned")
                .message_queued_again();
        } else {
            // Told as not written again, it is held no longer either: a cancel reported after
            // this does not tell it a second time.
            self.draft
                .lock()
                .expect("draft lock poisoned")
                .message_held_no_longer();
        }
        if again > 0 {
            tracing::info!(
                session = %self.id,
                agent = %self.agent.label(),
                messages = again,
                presses,
                outcome = ?cleared,
                "cleared what the agent gave back at a cancel out of the way of the next message; \
                 it is queued to be written again after that one"
            );
        }
        for message in lost.into_iter().rev() {
            self.report_not_written_again(&message.text, presses, cleared);
        }
    }

    /// Tells every client that `message`, a message of Octoboard's the agent gave back at a
    /// cancel and that was cleared from the input box, could not be written again, so it was not
    /// carried out.
    fn report_not_written_again(&self, message: &str, presses: usize, cleared: Cleared) {
        tracing::warn!(
            session = %self.id,
            agent = %self.agent.label(),
            chars = message.chars().count(),
            presses,
            outcome = ?cleared,
            "cleared a message the agent gave back at a cancel out of the way of the next; it \
             could not be queued again, so it was not carried out"
        );
        let notice = if cleared == Cleared::Empty {
            Notice::new(
                notice_code::MESSAGE_CLEARED_AFTER_CANCEL,
                format!(
                    "{} put a message it had held for the end of the turn back in the session's \
                     input box, unsent, when the turn was cancelled. It was cleared from the box \
                     and could not be written again, so it was not carried out. It read:\n{message}",
                    self.agent.label()
                ),
                &[("agent", self.agent.label()), ("detail", message)],
            )
        } else {
            Notice::new(
                notice_code::MESSAGE_CLEARED_AFTER_CANCEL,
                format!(
                    "{} put a message it had held for the end of the turn back in the session's \
                     input box, unsent, when the turn was cancelled. Part of it was cleared from \
                     the box and it could not be written again, so it was not carried out. It \
                     read:\n{message}",
                    self.agent.label()
                ),
                &[
                    ("agent", self.agent.label()),
                    ("detail", message),
                    ("cleared", "part"),
                ],
            )
        };
        self.broadcast_notice(notice);
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
