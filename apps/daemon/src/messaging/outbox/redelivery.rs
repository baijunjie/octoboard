//! The latest messages written into each session, and writing one again: what puts a message back
//! on its session's queue once it was cleared from the agent's input box unsent
//! (`crate::session::draft_clear`) or dropped by the agent at a cancel
//! (`crate::messaging::cleared_messages`), at once or held until another message is taken, a
//! bounded number of times.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering;

use super::{MessageQueue, Outbox, Ticket};

/// How many times a message is written again after it was cleared from the input box unsent
/// ([`Outbox::redeliver`]) or given back by the session's own server or channel
/// ([`Outbox::redeliver_returned`]): each takes a turn cancelled while the agent held the message,
/// so the bound only keeps a message that is given back every time from going round without end.
/// Past it the message is dropped and the user told, with its text.
pub(crate) const MAX_REDELIVERIES: u8 = 2;

/// How many of the messages a session's own server or channel took are kept for
/// [`Outbox::redeliver_returned`], and how many of the ones written into its terminal for
/// [`Outbox::redeliver`]: as many as a session keeps of the messages written while a turn ran
/// (`crate::session::HELD_KEPT`), which are the ones it can ask to have written again, and a
/// bound for an agent that never gives a message back.
pub(super) const TAKEN_KEPT: usize = crate::session::HELD_KEPT;

/// What became of a message its session's own server or channel gave back
/// ([`Outbox::redeliver_returned`]).
#[derive(Debug, PartialEq, Eq)]
pub enum Returned {
    /// It is queued to be written again, after the next message taken for the session.
    Again,
    /// It cannot be written again (it was more than text, it has been written again
    /// [`MAX_REDELIVERIES`] times already, or the queues have been taken for the next daemon): it
    /// is dropped, and this is its text, for the user to be told.
    Dropped(String),
    /// The queue is still writing it: the server or channel took it and the agent gave it back
    /// before the write was over. The drain that writes it takes the give-back up once it is
    /// ([`super::PromptSignal::given_back_while_written`]), so nothing is to be done now.
    Writing,
    /// The queue holds no message of this id that a server or channel took: it never wrote it,
    /// was already told it was given back, or has kept [`TAKEN_KEPT`] later ones since.
    Unknown,
}

/// What the queue knows of the messages a session's own server or channel gets.
#[derive(Default)]
pub(super) struct ThroughTransport {
    /// The messages it took, oldest first, the latest [`TAKEN_KEPT`].
    taken: VecDeque<Latest>,
    /// The message a drain is writing now, and whether it was given back meanwhile: a server or
    /// channel can say so before the write has returned and the take is noted.
    writing: Option<(Ticket, bool)>,
}

/// A message written into a session, by its terminal or through its own server or channel.
pub(super) struct Latest {
    ticket: Ticket,
    text: String,
    /// How many times it had been written again already.
    redeliveries: u8,
    /// Whether it was more than text ([`crate::session::term::Shape`]), which cannot be written
    /// again from its text.
    shaped: bool,
    /// Whether it went past a hold for the account's usage limit, as it does when written again.
    past_usage_limit: bool,
}

impl Outbox {
    /// Notes that the message `ticket`, `text`, is the latest written into the terminal of the
    /// session `id`. The latest [`TAKEN_KEPT`] of a session are kept: an agent that holds messages
    /// for the end of a turn can give several back at one cancel, each to be written again.
    ///
    /// Its count of times written again is read, not taken: a message written and then put back
    /// under the same ticket (one a dialog discarded) keeps it, and it goes with the ticket's other
    /// marks when the ticket leaves the queue for good ([`Self::forget_ticket_marks`]).
    pub(crate) fn note_latest(
        &self,
        id: &str,
        ticket: Ticket,
        text: &str,
        shaped: bool,
        past_usage_limit: bool,
    ) {
        let redeliveries = self
            .redeliveries
            .lock()
            .expect("outbox redeliveries lock poisoned")
            .get(&ticket)
            .copied()
            .unwrap_or(0);
        let mut latest = self.latest.lock().expect("outbox latest lock poisoned");
        let kept = latest.entry(id.to_string()).or_default();
        kept.retain(|message| message.ticket != ticket);
        if kept.len() >= TAKEN_KEPT {
            kept.pop_front();
        }
        kept.push_back(Latest {
            ticket,
            text: text.to_string(),
            redeliveries,
            shaped,
            past_usage_limit,
        });
    }

    /// Puts the message written into the session `id` as `logical_id`, the id [`Self::drain`]
    /// wrote it under, back at the front of the session's queue, to be written again: it was
    /// cleared from the input box unsent. Called from within the write of the message that cleared
    /// it, which the drain has taken off the queue, so it is written right after that one, and
    /// still before it if that one is put back. Several messages cleared at once are put back
    /// latest first, so that they are written again in the order they were written in. Says
    /// whether it did: not where the message is no longer among the latest [`TAKEN_KEPT`] written
    /// into the session's terminal, was more than text, has been written again
    /// [`MAX_REDELIVERIES`] times already, or the queues have been taken for the next daemon; nor a
    /// second time for the same write.
    pub fn redeliver(&self, id: &str, logical_id: &str) -> bool {
        self.queue_again(id, logical_id, false).is_some()
    }

    /// [`Self::redeliver`] for a message cleared while no other message is being written (an
    /// interrupt cleared what the agent gave back): it is put back held, so no drain takes it until
    /// another message is taken for the session, right after which it is written. Counted against
    /// [`MAX_REDELIVERIES`] as any message written again.
    pub fn redeliver_held(&self, id: &str, logical_id: &str) -> bool {
        self.queue_again(id, logical_id, true).is_some()
    }

    /// [`Self::redeliver_held`] for a message the agent held for the end of a turn and dropped
    /// when the turn was cancelled, with nothing put back in its input box: it is put back held,
    /// as one an interrupt cleared is, and counted the same. Whether the agent still held it when
    /// it was cancelled is known only from what the agent reported before, so while it is held
    /// back, the agent reporting its text sent takes it off the queue again
    /// ([`Self::withdraw_dropped`]): it is never written into an agent that took it.
    pub fn redeliver_dropped(&self, id: &str, logical_id: &str) -> bool {
        let Some(ticket) = self.queue_again(id, logical_id, true) else {
            return false;
        };
        self.dropped
            .lock()
            .expect("outbox dropped lock poisoned")
            .insert(ticket);
        true
    }

    /// Takes off the queue of the session `id` the oldest message put back by
    /// [`Self::redeliver_dropped`] and still held back whose text `sent` says the agent reported
    /// sending: the agent had taken it before the cancel after all, or took it right after, so
    /// writing it again would deliver it twice. Says whether one was withdrawn. One no longer
    /// held back is left alone: it follows a message taken since, and a prompt reported with its
    /// text may be that very write.
    pub fn withdraw_dropped(&self, id: &str, sent: impl Fn(&str) -> bool) -> bool {
        let withdrawn = {
            let mut queues = self.queues.lock().expect("outbox lock poisoned");
            let Some(queue) = queues.get_mut(id) else {
                return false;
            };
            let held = self
                .held_back
                .lock()
                .expect("outbox held-back lock poisoned");
            let dropped = self.dropped.lock().expect("outbox dropped lock poisoned");
            let Some(at) = queue.iter().position(|(ticket, text, _)| {
                held.contains(ticket) && dropped.contains(ticket) && sent(text)
            }) else {
                return false;
            };
            drop(dropped);
            drop(held);
            let withdrawn = queue.remove(at);
            if queue.is_empty() {
                queues.remove(id);
            }
            withdrawn
        };
        match withdrawn {
            Some((ticket, _, _)) => {
                self.forget_ticket_marks(std::iter::once(ticket));
                true
            }
            None => false,
        }
    }

    /// Notes that a drain begins writing the message `ticket` into the session `id`, before the
    /// write: from here a give-back of it is kept ([`Returned::Writing`]) until the write is
    /// over ([`Self::note_taken_by_transport`], [`Self::end_write`]).
    pub(super) fn begin_write(&self, id: &str, ticket: Ticket) {
        self.through_transport
            .lock()
            .expect("outbox transport lock poisoned")
            .entry(id.to_string())
            .or_default()
            .writing = Some((ticket, false));
    }

    /// Ends what [`Self::begin_write`] began for a message no server or channel took: nothing of
    /// it can be given back.
    pub(super) fn end_write(&self, id: &str, ticket: Ticket) {
        let mut through = self
            .through_transport
            .lock()
            .expect("outbox transport lock poisoned");
        if let Some(of_session) = through.get_mut(id) {
            if of_session
                .writing
                .is_some_and(|(writing, _)| writing == ticket)
            {
                of_session.writing = None;
            }
        }
    }

    /// Notes that the session `id`'s own server or channel took the message `ticket`, `text`
    /// (`crate::session::term::Route::Queued`), for [`Self::redeliver_returned`], and ends what
    /// [`Self::begin_write`] began. The latest [`TAKEN_KEPT`] of a session are kept, and its
    /// count of times written again is read as [`Self::note_latest`] reads it.
    ///
    /// Says whether the message was given back while it was written
    /// ([`Returned::Writing`]): the caller then has it given back again, now that it is noted.
    #[must_use]
    pub(crate) fn note_taken_by_transport(
        &self,
        id: &str,
        ticket: Ticket,
        text: &str,
        shaped: bool,
        past_usage_limit: bool,
    ) -> bool {
        let redeliveries = self
            .redeliveries
            .lock()
            .expect("outbox redeliveries lock poisoned")
            .get(&ticket)
            .copied()
            .unwrap_or(0);
        let mut through = self
            .through_transport
            .lock()
            .expect("outbox transport lock poisoned");
        let of_session = through.entry(id.to_string()).or_default();
        let given_back = match of_session.writing {
            Some((writing, given_back)) if writing == ticket => {
                of_session.writing = None;
                given_back
            }
            _ => false,
        };
        let kept = &mut of_session.taken;
        kept.retain(|message| message.ticket != ticket);
        if kept.len() >= TAKEN_KEPT {
            kept.pop_front();
        }
        kept.push_back(Latest {
            ticket,
            text: text.to_string(),
            redeliveries,
            shaped,
            past_usage_limit,
        });
        given_back
    }

    /// Puts the message the session `id`'s own server or channel took as `logical_id`, and whose
    /// agent then gave it back unsent, back at the front of the session's queue, held as
    /// [`Self::redeliver_held`] holds one: the agent gives a message back when its turn is
    /// cancelled, and writing it at once would start the agent again. Says what became of it;
    /// a message is given back to the queue once, so a second call for it finds nothing. One
    /// whose write is not over yet is kept for the drain writing it ([`Returned::Writing`]).
    pub fn redeliver_returned(&self, id: &str, logical_id: &str) -> Returned {
        let Some(ticket) = ticket_of(logical_id) else {
            return Returned::Unknown;
        };
        let mut through = self
            .through_transport
            .lock()
            .expect("outbox transport lock poisoned");
        let Some(of_session) = through.get_mut(id) else {
            return Returned::Unknown;
        };
        let taken = of_session
            .taken
            .iter()
            .position(|message| message.ticket == ticket)
            .and_then(|at| of_session.taken.remove(at));
        let Some(message) = taken else {
            return match &mut of_session.writing {
                Some((writing, given_back)) if *writing == ticket => {
                    *given_back = true;
                    Returned::Writing
                }
                _ => Returned::Unknown,
            };
        };
        if message.shaped || message.redeliveries >= MAX_REDELIVERIES {
            return Returned::Dropped(message.text);
        }
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.taken.load(Ordering::Acquire) {
            return Returned::Dropped(message.text);
        }
        self.push_again(&mut queues, id, message, true);
        Returned::Again
    }

    /// The body of [`Self::redeliver`] and, with `held`, of [`Self::redeliver_held`]: the ticket
    /// the message is queued again under, where it is.
    fn queue_again(&self, id: &str, logical_id: &str, held: bool) -> Option<Ticket> {
        let ticket = ticket_of(logical_id)?;
        let mut latest = self.latest.lock().expect("outbox latest lock poisoned");
        let kept = latest.get_mut(id)?;
        let at = kept.iter().position(|message| {
            message.ticket == ticket && !message.shaped && message.redeliveries < MAX_REDELIVERIES
        })?;
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.taken.load(Ordering::Acquire) {
            return None;
        }
        let message = kept.remove(at)?;
        Some(self.push_again(&mut queues, id, message, held))
    }

    /// Queues `message` at the front of the session `id`'s queue under a new ticket, which it
    /// gives, counted as written again once more and, with `held`, held back until another
    /// message is taken.
    fn push_again(
        &self,
        queues: &mut HashMap<String, MessageQueue>,
        id: &str,
        message: Latest,
        held: bool,
    ) -> Ticket {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        self.redeliveries
            .lock()
            .expect("outbox redeliveries lock poisoned")
            .insert(ticket, message.redeliveries + 1);
        // The same message under a new ticket: whoever sent it sent this one.
        let mut senders = self.senders.lock().expect("outbox senders lock poisoned");
        if let Some(sender) = senders.remove(&message.ticket) {
            senders.insert(ticket, sender);
        }
        drop(senders);
        if message.past_usage_limit {
            self.past_usage_limit
                .lock()
                .expect("outbox usage limit lock poisoned")
                .insert(ticket);
        }
        if held {
            self.held_back
                .lock()
                .expect("outbox held-back lock poisoned")
                .insert(ticket);
        }
        queues
            .entry(id.to_string())
            .or_default()
            .push_front((ticket, message.text, None));
        ticket
    }

    /// How many messages queued for the session a drain may take now: all but the ones held back
    /// until another message is taken ([`Self::redeliver_held`]).
    pub fn takeable(&self, id: &str) -> usize {
        let queues = self.queues.lock().expect("outbox lock poisoned");
        let held = self
            .held_back
            .lock()
            .expect("outbox held-back lock poisoned");
        queues.get(id).map_or(0, |queue| {
            queue
                .iter()
                .filter(|(ticket, _, _)| !held.contains(ticket))
                .count()
        })
    }

    /// Forgets the latest messages written into the terminal of the session `id`, whose queue is
    /// gone, and the messages its own server or channel took.
    pub(super) fn forget_latest(&self, id: &str) {
        self.latest
            .lock()
            .expect("outbox latest lock poisoned")
            .remove(id);
        self.through_transport
            .lock()
            .expect("outbox transport lock poisoned")
            .remove(id);
    }
}

/// The ticket of the message written as `logical_id` (`<launch id>:<ticket>`, see
/// [`Outbox::drain`]).
pub(super) fn ticket_of(logical_id: &str) -> Option<Ticket> {
    logical_id
        .rsplit_once(':')
        .and_then(|(_, ticket)| ticket.parse::<Ticket>().ok())
}
