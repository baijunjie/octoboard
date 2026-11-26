//! The sessions' composer queues: the user's messages waiting to be sent into a session, kept in
//! the store (`crate::store::Store::replace_queued_messages_owning`) and mirrored here with what each
//! message's hand-over to the session's delivery stands at, which the store does not keep.
//!
//! Every change goes through [`AppState`] here: the mirror is changed and the session's queue
//! written under one lock, then the whole queue is broadcast (`session_message_queue`). Sending a
//! message is the coordinator's (`crate::coordinator::composer`); what delivery later reports of
//! a message it took from the queue comes back here by its ticket ([`AppState::composer_outcome`],
//! [`AppState::composer_returned_late`]). The rules are in "The composer queue" in
//! `apps/daemon/PROTOCOL.md`.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use anyhow::Result;

use super::AppState;
use crate::messaging::outbox::Ticket;
use crate::protocol::{
    error_code, notice_code, CodedError, Event, Notice, QueuedMessage, QueuedMessageFailure,
    QueuedMessageFailureKind, QueuedMessageState, QueuedMode, SessionMessageQueue,
};

/// The most messages one session's queue holds.
pub const MAX_QUEUED: usize = 100;

/// How many messages written from the queue are remembered, daemon-wide, for delivery to report
/// one as not carried out after all and have it put back in its place: far more than one turn's
/// worth, as the delivery's own record of what it wrote in a turn keeps sixteen per session.
const DELIVERED_KEPT: usize = 256;

/// How a message is being handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// The daemon's own dispatch at an idle: a refusal that may pass leaves the message waiting.
    Idle,
    /// The user's send now.
    Now,
}

/// What delivery made of a message it took from the queue.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Written into the session: it leaves the queue.
    Written,
    /// Not carried out, or refused once queued: it comes back to its place as `manual`, with
    /// this failure.
    Back(QueuedMessageFailure),
}

#[derive(Default)]
pub struct ComposerQueues {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Whether `queues` has been read from the store.
    loaded: bool,
    /// Each session's queue, in order, as the store holds it, with each message's `state`.
    queues: BTreeMap<String, Vec<QueuedMessage>>,
    /// The messages being handed over, by id.
    sending: HashMap<String, Sending>,
    /// The message each ticket delivery gave a hand-over is, while it is sending or remembered as
    /// written.
    tickets: HashMap<Ticket, String>,
    /// The messages written from a queue, oldest first, bounded by [`DELIVERED_KEPT`].
    delivered: VecDeque<Delivered>,
    /// The sessions whose present idle has had its `when_idle` message, with when a slash command
    /// sent at it was written, where one was ([`Dispatched`]).
    dispatched: HashMap<String, Dispatched>,
    /// The composer's message each session's agent left unsent last, by ticket, for a later report
    /// that it sent it after all.
    unsent: HashMap<String, Ticket>,
    /// What delivery reported of a composer's ticket before the composer queue was given that
    /// ticket ([`AppState::composer_note_ticket`]), applied once it is: delivery queues the
    /// message, and a drain may write it, in the moment before. Bounded by [`EARLY_KEPT`].
    early: VecDeque<(Ticket, String, Deferred)>,
    /// The paths each message's own create or update moved into the session's composer
    /// directory, by message id: the only files withdrawing or editing it may remove. Stored
    /// with the message's row (`queued_messages.owned`), never sent to clients.
    owned: HashMap<String, Vec<String>>,
}

/// How long a session must still read idle after a slash command from its queue was written
/// before its next `when_idle` message goes. A command may start a turn whose report
/// arrives a moment after the write (one the agent expands into a prompt), or run long without
/// any (a compaction): the next message waits this long for a turn to show, and goes at once on
/// a turn's end otherwise. Provider-neutral: it waits on the session's reported status alone.
pub const COMMAND_SETTLE: Duration = Duration::from_secs(5);

/// What a session's present idle has had sent.
#[derive(Debug, Clone, Copy, Default)]
struct Dispatched {
    /// When a slash command from the queue was written at this idle: the idle is let go once the
    /// session still reads idle [`COMMAND_SETTLE`] after it. Tokio's clock, which a test pauses
    /// and advances; off a runtime it reads the system's.
    command_written: Option<Instant>,
}

/// How many reports of tickets the composer queue has not been given yet are kept.
const EARLY_KEPT: usize = 64;

/// A report of delivery's about a ticket, deferred until the composer queue is given the ticket.
#[derive(Debug, Clone)]
enum Deferred {
    Outcome(Outcome),
    NotCarriedOut(QueuedMessageFailure),
}

/// A message being handed over.
#[derive(Debug, Clone)]
struct Sending {
    session: String,
    how: How,
    /// The ticket delivery queued it under, once it has one.
    ticket: Option<Ticket>,
    /// Waiting for the session a send now resumes to be able to take a message.
    after_resume: bool,
    /// The resume `after_resume` waits on has been made: the session ending again without taking
    /// the message puts it back ([`AppState::composer_resume_ended`]).
    resumed: bool,
}

/// Where a message stands as far as withdrawing it goes.
enum Withdrawal {
    /// Waiting, or waiting for a resume: taken off the queue.
    Free,
    /// Queued in delivery under this ticket: taken out of delivery first, if it still holds it.
    Queued(Ticket),
    /// Being handed to delivery or written: refused.
    InFlight,
}

/// A message written from a queue, remembered for delivery to report it not carried out after
/// all.
#[derive(Debug, Clone)]
struct Delivered {
    session: String,
    message: QueuedMessage,
    /// The messages next to it when it was written, by id, to find its place again.
    before: Option<String>,
    after: Option<String>,
    index: usize,
    ticket: Ticket,
    /// The message as it was put back after all, for a later report that it ran to take it out
    /// again if the user has left it as it came back.
    returned: Option<QueuedMessage>,
}

/// A change to a queue, to broadcast once the lock is let go.
struct Changed(Option<Event>);

impl Changed {
    fn of(session: &str, inner: &Inner) -> Self {
        Self(Some(Event::SessionMessageQueue {
            session: session.to_string(),
            messages: inner.queues.get(session).cloned().unwrap_or_default(),
        }))
    }
}

impl AppState {
    /// Runs `change` on the composer's state, loaded from the store the first time; what it
    /// answers with a change is broadcast after the lock is let go.
    fn composer_change<R>(
        &self,
        change: impl FnOnce(&mut Inner) -> Result<(R, Changed)>,
    ) -> Result<R> {
        let (answer, changed) = {
            let mut inner = self.composer.inner.lock().expect("composer lock poisoned");
            self.composer_load(&mut inner)?;
            change(&mut inner)?
        };
        if let Some(event) = changed.0 {
            self.broadcast(event);
        }
        Ok(answer)
    }

    fn composer_load(&self, inner: &mut Inner) -> Result<()> {
        if !inner.loaded {
            inner.queues = self.store.load_queued_messages()?;
            inner.owned = self.store.load_queued_owned()?;
            inner.loaded = true;
        }
        Ok(())
    }

    /// Writes the session's queue as the mirror now holds it.
    fn composer_store(&self, inner: &Inner, session: &str) -> Result<()> {
        let empty = Vec::new();
        let queue = inner.queues.get(session).unwrap_or(&empty);
        Ok(self
            .store
            .replace_queued_messages_owning(session, queue, &inner.owned)?)
    }

    /// Changes the session's queue with `edit`, on a copy that replaces the mirror only once the
    /// store has it, and broadcasts it.
    fn composer_edit<R>(
        &self,
        session: &str,
        edit: impl FnOnce(&mut Vec<QueuedMessage>, &mut Inner) -> Result<R>,
    ) -> Result<R> {
        self.composer_change(|inner| {
            let before = inner.queues.get(session).cloned().unwrap_or_default();
            let owned_before = inner.owned.clone();
            let mut queue = before.clone();
            let answer = match edit(&mut queue, inner) {
                Ok(answer) => answer,
                Err(err) => {
                    inner.owned = owned_before;
                    return Err(err);
                }
            };
            if queue != before || inner.owned != owned_before {
                if let Err(err) =
                    self.store
                        .replace_queued_messages_owning(session, &queue, &inner.owned)
                {
                    inner.owned = owned_before;
                    return Err(err.into());
                }
                if queue == before {
                    return Ok((answer, Changed(None)));
                }
                if queue.is_empty() {
                    inner.queues.remove(session);
                } else {
                    inner.queues.insert(session.to_string(), queue);
                }
                return Ok((answer, Changed::of(session, inner)));
            }
            Ok((answer, Changed(None)))
        })
    }

    /// The session's queue, in order.
    pub fn queued_messages(&self, session: &str) -> Result<Vec<QueuedMessage>> {
        self.composer_change(|inner| {
            Ok((
                inner.queues.get(session).cloned().unwrap_or_default(),
                Changed(None),
            ))
        })
    }

    /// Every session's queue that holds a message, for a `snapshot`.
    pub fn message_queues(&self) -> Vec<SessionMessageQueue> {
        self.composer_change(|inner| {
            Ok((
                inner
                    .queues
                    .iter()
                    .filter(|(_, messages)| !messages.is_empty())
                    .map(|(session, messages)| SessionMessageQueue {
                        session: session.clone(),
                        messages: messages.clone(),
                    })
                    .collect(),
                Changed(None),
            ))
        })
        .unwrap_or_else(|err| {
            tracing::warn!(%err, "reading the composer queues failed");
            Vec::new()
        })
    }

    /// Puts `message` at the end of the session's queue, owning the files `owned` its create moved
    /// into the session's composer directory. Refused with `limit_exceeded` (`queued_messages`)
    /// for a full queue.
    pub fn composer_add(
        &self,
        session: &str,
        message: QueuedMessage,
        owned: Vec<String>,
    ) -> Result<()> {
        self.composer_edit(session, |queue, inner| {
            if queue.len() >= MAX_QUEUED {
                return Err(CodedError::raised(
                    error_code::LIMIT_EXCEEDED,
                    format!("a session's composer queue holds at most {MAX_QUEUED} messages"),
                    &[
                        ("limit", "queued_messages"),
                        ("max", &MAX_QUEUED.to_string()),
                    ],
                ));
            }
            if !owned.is_empty() {
                inner.owned.insert(message.id.clone(), owned);
            }
            queue.push(message);
            Ok(())
        })
    }

    /// Changes the waiting message `id` with `change`, clearing its failure. Refused with
    /// `queued_message_not_found` and `queued_message_sending`.
    pub fn composer_update(
        &self,
        session: &str,
        id: &str,
        change: impl FnOnce(&mut QueuedMessage) -> Result<()>,
    ) -> Result<()> {
        self.composer_edit(session, |queue, _| {
            let message = waiting_mut(queue, session, id)?;
            change(message)?;
            message.failure = None;
            Ok(())
        })
    }

    /// Moves the message `id` to `position` among the others; past the end puts it last.
    pub fn composer_move(&self, session: &str, id: &str, position: usize) -> Result<()> {
        self.composer_edit(session, |queue, _| {
            let index = index_of(queue, session, id)?;
            let message = queue.remove(index);
            queue.insert(position.min(queue.len()), message);
            Ok(())
        })
    }

    /// Withdraws the message `id`: one waiting, or waiting for a resume, leaves the queue; one
    /// queued in delivery leaves it once `take_out` has taken it out of delivery, which answers
    /// whether delivery still held it unwritten. Answers the message with the files its withdrawal
    /// may remove: those its own create or update moved in, unless it was written already (it
    /// came back as not carried out, or delivery remembers it as written), as the agent may still
    /// read them. Refused with `queued_message_not_found`, and with `queued_message_sending` for
    /// one being handed over or written. Under the composer's lock throughout, so no hand-over can
    /// take the message in between.
    pub fn composer_withdraw(
        &self,
        session: &str,
        id: &str,
        take_out: impl FnOnce(Ticket) -> bool,
    ) -> Result<(QueuedMessage, Vec<String>)> {
        self.composer_edit(session, |queue, inner| {
            let index = index_of(queue, session, id)?;
            let withdrawal = match inner.sending.get(id) {
                None => Withdrawal::Free,
                Some(sending) if sending.after_resume => Withdrawal::Free,
                Some(Sending {
                    ticket: Some(ticket),
                    ..
                }) => Withdrawal::Queued(*ticket),
                Some(_) => Withdrawal::InFlight,
            };
            match withdrawal {
                Withdrawal::Free => {}
                Withdrawal::Queued(ticket) if take_out(ticket) => {}
                Withdrawal::Queued(_) | Withdrawal::InFlight => return Err(sending(session, id)),
            }
            if let Some(sending) = inner.sending.remove(id) {
                if let Some(ticket) = sending.ticket {
                    inner.tickets.remove(&ticket);
                }
            }
            let message = queue.remove(index);
            let owned = inner.owned.remove(id).unwrap_or_default();
            let written = message
                .failure
                .as_ref()
                .is_some_and(|failure| failure.kind == QueuedMessageFailureKind::NotDelivered)
                || inner
                    .delivered
                    .iter()
                    .any(|delivered| delivered.message.id == id);
            let removable = if written {
                Vec::new()
            } else {
                owned
                    .into_iter()
                    .filter(|path| message.images.contains(path) || message.files.contains(path))
                    .collect()
            };
            Ok((message, removable))
        })
    }

    /// Changes the waiting message `id` with `change`, clearing its failure, and answers the
    /// files it named before the change and no longer does that its own create or update moved
    /// in, which the caller may remove: read under the same lock as the change, so a concurrent
    /// update cannot make it remove a file this one added. None for a message that was written
    /// already (see [`Self::composer_withdraw`]). Refused as [`Self::composer_update`] is.
    pub fn composer_update_files(
        &self,
        session: &str,
        id: &str,
        moved: Vec<String>,
        change: impl FnOnce(&mut QueuedMessage) -> Result<()>,
    ) -> Result<Vec<String>> {
        self.composer_edit(session, |queue, inner| {
            let message = waiting_mut(queue, session, id)?;
            let written = message
                .failure
                .as_ref()
                .is_some_and(|failure| failure.kind == QueuedMessageFailureKind::NotDelivered)
                || inner
                    .delivered
                    .iter()
                    .any(|delivered| delivered.message.id == id);
            change(message)?;
            message.failure = None;
            let named: Vec<&String> = message.images.iter().chain(&message.files).collect();
            let owned = inner.owned.entry(id.to_string()).or_default();
            let (mut kept, dropped): (Vec<String>, Vec<String>) =
                owned.drain(..).partition(|path| named.contains(&path));
            kept.extend(moved);
            if kept.is_empty() {
                inner.owned.remove(id);
            } else {
                *owned = kept;
            }
            Ok(if written { Vec::new() } else { dropped })
        })
    }

    /// Starts handing the waiting message `id` over, `how` says why: it reads `sending`, its
    /// failure cleared. Answers the message. Refused with `queued_message_not_found` and
    /// `queued_message_sending`.
    pub fn composer_begin_send(
        &self,
        session: &str,
        id: &str,
        how: How,
        after_resume: bool,
    ) -> Result<QueuedMessage> {
        self.composer_edit(session, |queue, inner| {
            let message = waiting_mut(queue, session, id)?;
            message.state = QueuedMessageState::Sending;
            message.failure = None;
            inner.sending.insert(
                id.to_string(),
                Sending {
                    session: session.to_string(),
                    how,
                    ticket: None,
                    after_resume,
                    resumed: false,
                },
            );
            Ok(message.clone())
        })
    }

    /// Notes the ticket delivery queued the message `id` under, for what it reports of that
    /// ticket to find the message, and applies what delivery reported of it already. Answers
    /// whether the message is being handed over still: false once it was withdrawn, or placed by
    /// what delivery reported.
    pub fn composer_note_ticket(&self, id: &str, ticket: Ticket) -> bool {
        let noted = self.composer_change(|inner| {
            let Some(sending) = inner.sending.get_mut(id) else {
                return Ok((None, Changed(None)));
            };
            sending.ticket = Some(ticket);
            sending.after_resume = false;
            let session = sending.session.clone();
            inner.tickets.insert(ticket, id.to_string());
            let mut early = Vec::new();
            inner.early.retain(|(known, _, deferred)| {
                if *known == ticket {
                    early.push(deferred.clone());
                    false
                } else {
                    true
                }
            });
            Ok((Some((session, early)), Changed(None)))
        });
        let Ok(Some((session, early))) = noted else {
            return false;
        };
        let held = early.is_empty();
        for deferred in early {
            match deferred {
                Deferred::Outcome(outcome) => self.composer_outcome(&session, ticket, outcome),
                Deferred::NotCarriedOut(failure) => {
                    self.composer_not_carried_out(&session, ticket, failure)
                }
            }
        }
        held
    }

    /// Keeps what delivery reported of `ticket` for when the composer queue is given it, where it
    /// has not been yet; answers whether it was kept.
    fn composer_defer(&self, session: &str, ticket: Ticket, deferred: Deferred) -> bool {
        let mut inner = self.composer.inner.lock().expect("composer lock poisoned");
        if inner.tickets.contains_key(&ticket) {
            return false;
        }
        inner
            .early
            .push_back((ticket, session.to_string(), deferred));
        while inner.early.len() > EARLY_KEPT {
            inner.early.pop_front();
        }
        true
    }

    /// Notes that the resume a send now of the message `id` waits on has been made, and answers
    /// whether it still waits: from now on the session ending again without taking the message
    /// puts it back ([`Self::composer_resume_ended`]).
    pub fn composer_resumed(&self, id: &str) -> bool {
        let mut inner = self.composer.inner.lock().expect("composer lock poisoned");
        match inner.sending.get_mut(id) {
            Some(sending) if sending.after_resume => {
                sending.resumed = true;
                true
            }
            _ => false,
        }
    }

    /// Whether `ticket` is a message delivery took from a composer queue.
    pub fn composer_owns_ticket(&self, ticket: Ticket) -> bool {
        self.composer
            .inner
            .lock()
            .expect("composer lock poisoned")
            .tickets
            .contains_key(&ticket)
    }

    /// Puts the message `id`, being handed over, back to waiting: with `failure` where the
    /// hand-over failed for good, switched to `mode` where given. Nothing for a message no longer
    /// being handed over: what delivery reported of it has already placed it.
    pub fn composer_back(
        &self,
        session: &str,
        id: &str,
        failure: Option<QueuedMessageFailure>,
        mode: Option<QueuedMode>,
    ) {
        let edited = self.composer_edit(session, |queue, inner| {
            let Some(sending) = inner.sending.remove(id) else {
                return Ok(());
            };
            if let Some(ticket) = sending.ticket {
                inner.tickets.remove(&ticket);
            }
            if sending.how == How::Idle {
                inner.dispatched.remove(session);
            }
            if let Some(message) = queue.iter_mut().find(|message| message.id == id) {
                message.state = QueuedMessageState::Waiting;
                message.failure = failure;
                if let Some(mode) = mode {
                    message.mode = mode;
                }
            }
            Ok(())
        });
        if let Err(err) = edited {
            tracing::warn!(session = %session, %err, "putting a queued message back failed");
        }
    }

    /// What delivery made of the message it queued as `ticket` in the session `session`
    /// (`crate::messaging::outbox::PromptSignal::composer_outcome`). Nothing for a ticket that is
    /// no queued message's, or one already placed.
    pub fn composer_outcome(&self, session: &str, ticket: Ticket, outcome: Outcome) {
        if self.composer_defer(session, ticket, Deferred::Outcome(outcome.clone())) {
            return;
        }
        let edited = self.composer_edit(session, |queue, inner| {
            let Some(id) = inner.tickets.get(&ticket).cloned() else {
                return Ok(());
            };
            let Some(sending) = inner.sending.get(&id).cloned() else {
                return Ok(());
            };
            let Some(index) = queue.iter().position(|message| message.id == id) else {
                return Ok(());
            };
            inner.sending.remove(&id);
            match outcome {
                Outcome::Written => {
                    let mut message = queue.remove(index);
                    message.state = QueuedMessageState::Waiting;
                    // A slash command, sent at an idle or sent now, may start a turn whose report
                    // comes a moment late, or none: the idle counts as having had its message until
                    // the session still reads idle a while after it (`COMMAND_SETTLE`) or a turn
                    // shows, so a waiting `when_idle` message does not overtake it.
                    if message.command {
                        inner.dispatched.insert(
                            session.to_string(),
                            Dispatched {
                                command_written: Some(Instant::now()),
                            },
                        );
                    }
                    // Written: its files are the agent's to read from now on.
                    inner.owned.remove(&id);
                    inner.delivered.push_back(Delivered {
                        session: sending.session,
                        message,
                        before: index.checked_sub(1).map(|before| queue[before].id.clone()),
                        after: queue.get(index).map(|after| after.id.clone()),
                        index,
                        ticket,
                        returned: None,
                    });
                    while inner.delivered.len() > DELIVERED_KEPT {
                        if let Some(old) = inner.delivered.pop_front() {
                            inner.tickets.remove(&old.ticket);
                        }
                    }
                }
                Outcome::Back(failure) => {
                    inner.tickets.remove(&ticket);
                    if sending.how == How::Idle {
                        inner.dispatched.remove(session);
                    }
                    let message = &mut queue[index];
                    message.state = QueuedMessageState::Waiting;
                    message.mode = QueuedMode::Manual;
                    message.failure = Some(failure);
                }
            }
            Ok(())
        });
        if let Err(err) = edited {
            tracing::warn!(session = %session, %err, "recording what became of a queued message failed");
        }
    }

    /// Delivery reported the message it wrote as `ticket` not carried out after all, with
    /// `failure`: it is put back in its place as `manual`. With no `failure`, the agent reported
    /// taking a message so reported after all: one put back and left as it came back leaves the
    /// queue again.
    pub fn composer_returned_late(
        &self,
        session: &str,
        ticket: Ticket,
        failure: Option<QueuedMessageFailure>,
    ) {
        let edited = self.composer_edit(session, |queue, inner| {
            let Some(delivered) = inner
                .delivered
                .iter_mut()
                .find(|delivered| delivered.ticket == ticket && delivered.session == session)
            else {
                return Ok(());
            };
            match failure {
                Some(failure) => {
                    if delivered.returned.is_some() {
                        return Ok(());
                    }
                    if queue.len() >= MAX_QUEUED {
                        tracing::warn!(session = %session, "a queued message delivery did not carry out finds its queue full; it is not put back");
                        return Ok(());
                    }
                    let mut message = delivered.message.clone();
                    message.mode = QueuedMode::Manual;
                    message.failure = Some(failure);
                    let place = place_of(queue, delivered);
                    queue.insert(place, message.clone());
                    delivered.returned = Some(message);
                    // Its idle let nothing else go: the next message may.
                    inner.dispatched.remove(session);
                }
                None => {
                    let Some(returned) = delivered.returned.take() else {
                        return Ok(());
                    };
                    if let Some(index) = queue.iter().position(|message| *message == returned) {
                        queue.remove(index);
                    }
                }
            }
            Ok(())
        });
        if let Err(err) = edited {
            tracing::warn!(session = %session, %err, "putting back a queued message delivery did not carry out failed");
        }
    }

    /// Delivery wrote the message `ticket` and found it not carried out (left unsent, taken by a
    /// view, gone elsewhere, not taken by the turn, dropped or given back at a cancel), with
    /// `failure`: it leaves the queue as written and comes back to its place as `manual`, so that
    /// a later report that it ran after all can take it out again.
    pub fn composer_not_carried_out(
        &self,
        session: &str,
        ticket: Ticket,
        failure: QueuedMessageFailure,
    ) {
        if self.composer_defer(session, ticket, Deferred::NotCarriedOut(failure.clone())) {
            return;
        }
        self.composer_outcome(session, ticket, Outcome::Written);
        self.composer_returned_late(session, ticket, Some(failure));
    }

    /// [`Self::composer_not_carried_out`] for the message delivery wrote as `logical_id`
    /// (`<launch id>:<ticket>`), told with the notice `notice`; `None` says the agent reported
    /// taking such a message after all. Nothing for a message that is no composer's.
    pub fn composer_logical_not_carried_out(
        &self,
        session: &str,
        logical_id: &str,
        notice: Option<&Notice>,
    ) {
        let Some(ticket) = crate::messaging::outbox::Outbox::ticket_of(logical_id) else {
            return;
        };
        if !self.composer_owns_ticket(ticket) {
            return;
        }
        match notice {
            Some(notice) => self.composer_not_carried_out(
                session,
                ticket,
                failure_of_notice(notice.code.as_str(), notice.params.clone(), &notice.message),
            ),
            None => self.composer_returned_late(session, ticket, None),
        }
    }

    /// Notes that the session's agent left the composer's message `ticket` unsent.
    pub fn composer_note_unsent(&self, session: &str, ticket: Ticket) {
        self.composer
            .inner
            .lock()
            .expect("composer lock poisoned")
            .unsent
            .insert(session.to_string(), ticket);
    }

    /// The session's agent sent the message it had left unsent after all: a composer's message so
    /// left, back in the queue as it came back, leaves it again.
    pub fn composer_unsent_sent(&self, session: &str) {
        let ticket = self
            .composer
            .inner
            .lock()
            .expect("composer lock poisoned")
            .unsent
            .remove(session);
        if let Some(ticket) = ticket {
            self.composer_returned_late(session, ticket, None);
        }
    }

    /// Puts back to waiting every message of the session being handed over whose ticket
    /// `still_queued` no longer finds in delivery: what delivery holds was dropped or carried
    /// elsewhere (the process ended, a relaunch or a handoff took its queue) without the message
    /// being written; it comes back with no failure and its mode kept.
    pub fn composer_reconcile(&self, session: &str, still_queued: impl Fn(Ticket) -> bool) {
        let gone: Vec<String> = {
            let inner = self.composer.inner.lock().expect("composer lock poisoned");
            inner
                .sending
                .iter()
                .filter(|(_, sending)| {
                    sending.session == session
                        && sending.ticket.is_some_and(|ticket| !still_queued(ticket))
                })
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in gone {
            self.composer_back(session, &id, None, None);
        }
    }

    /// The session has no process again: a send now that waited on the resume made for it, and
    /// that the session ended without taking, comes back refused (`session_not_running`), its
    /// mode kept. One whose resume is still being made waits on.
    pub fn composer_resume_ended(&self, session: &str) {
        let failure = || {
            let error = CodedError::raised(
                error_code::SESSION_NOT_RUNNING,
                "the session resumed to send the message ended before it could take it",
                &[("session", session)],
            );
            let coded = error
                .downcast_ref::<CodedError>()
                .expect("a raised error is coded")
                .clone();
            failure_of(QueuedMessageFailureKind::Refused, &coded)
        };
        let ended: Vec<String> = {
            let inner = self.composer.inner.lock().expect("composer lock poisoned");
            inner
                .sending
                .iter()
                .filter(|(_, sending)| {
                    sending.session == session && sending.after_resume && sending.resumed
                })
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ended {
            self.composer_back(session, &id, Some(failure()), None);
        }
    }

    /// The first `when_idle` message of the session's queue, begun as handed over for this idle,
    /// unless the idle has had its message already or a message of the session is being handed
    /// over.
    pub fn composer_take_idle(&self, session: &str) -> Option<QueuedMessage> {
        let id = {
            let mut inner = self.composer.inner.lock().expect("composer lock poisoned");
            // The caller found the session idle: a command's idle that has settled is let go.
            if command_settled(&inner, session) {
                inner.dispatched.remove(session);
            }
            if self.composer_load(&mut inner).is_err()
                || inner.dispatched.contains_key(session)
                || is_sending(&inner, session)
            {
                return None;
            }
            let id = inner
                .queues
                .get(session)?
                .iter()
                .find(|message| {
                    message.mode == QueuedMode::WhenIdle
                        && message.state == QueuedMessageState::Waiting
                })?
                .id
                .clone();
            inner
                .dispatched
                .insert(session.to_string(), Dispatched::default());
            id
        };
        match self.composer_begin_send(session, &id, How::Idle, false) {
            Ok(message) => Some(message),
            Err(_) => {
                self.composer_idle_over(session);
                None
            }
        }
    }

    /// The session left its idle (a turn began): its next idle may have a message again.
    pub fn composer_idle_over(&self, session: &str) {
        self.composer
            .inner
            .lock()
            .expect("composer lock poisoned")
            .dispatched
            .remove(session);
    }

    /// Whether the session, idle as `idle` says, could have a message handed over now: one a send
    /// now waits to give a resumed session, or, at an idle that has not had its message (or whose
    /// command has settled, [`COMMAND_SETTLE`]), a waiting `when_idle` one. What makes a retry
    /// drain the session for the composer queue's sake.
    pub fn composer_could_dispatch(&self, session: &str, idle: bool) -> bool {
        let mut inner = self.composer.inner.lock().expect("composer lock poisoned");
        if self.composer_load(&mut inner).is_err() {
            return false;
        }
        if waits_on_resume(&inner, session) {
            return true;
        }
        let idle_free = !inner.dispatched.contains_key(session) || command_settled(&inner, session);
        idle && idle_free && !is_sending(&inner, session) && has_waiting_idle(&inner, session)
    }

    /// Whether the session has a message waiting for its idle, or one a send now waits to hand
    /// over once a resumed session can take it: what makes a drain look at the queue.
    pub fn composer_has_work(&self, session: &str) -> bool {
        let mut inner = self.composer.inner.lock().expect("composer lock poisoned");
        if self.composer_load(&mut inner).is_err() {
            return false;
        }
        waits_on_resume(&inner, session) || has_waiting_idle(&inner, session)
    }

    /// The messages a send now waits to hand over once the resumed session can take one, taken
    /// off that wait.
    pub fn composer_take_after_resume(&self, session: &str) -> Vec<QueuedMessage> {
        let mut inner = self.composer.inner.lock().expect("composer lock poisoned");
        let ids: Vec<String> = inner
            .sending
            .iter_mut()
            .filter(|(_, sending)| sending.session == session && sending.after_resume)
            .map(|(id, sending)| {
                sending.after_resume = false;
                id.clone()
            })
            .collect();
        let queue = inner.queues.get(session).cloned().unwrap_or_default();
        queue
            .into_iter()
            .filter(|message| ids.contains(&message.id))
            .collect()
    }

    /// Drops the archived session's queue and hands each message's content back to the user in a
    /// `queued_message_discarded` notice. The files stay, with the composer directory, until the
    /// session is deleted, so the paths handed back still lead to them.
    pub(crate) fn composer_discard(&self, session: &str) {
        let dropped = self.composer_change(|inner| {
            let dropped = inner.queues.remove(session).unwrap_or_default();
            if dropped.is_empty() {
                return Ok((dropped, Changed(None)));
            }
            forget_session(inner, session);
            if let Err(err) = self.composer_store(inner, session) {
                // Kept in memory as gone: the rows go with the session's record when it is
                // deleted, and a resume reads the queue from memory until the daemon restarts.
                tracing::warn!(session = %session, %err, "dropping an archived session's composer queue from the store failed");
            }
            Ok((dropped, Changed::of(session, inner)))
        });
        for message in dropped.unwrap_or_default() {
            self.broadcast(discarded_notice(&message).about(session));
        }
    }

    /// Forgets the deleted session's queue and removes its composer directory.
    pub(crate) fn composer_forget(&self, session: &str) {
        // No event: every client drops the session's queue with the session.
        let _ = self.composer_change(|inner| {
            inner.queues.remove(session);
            forget_session(inner, session);
            Ok(((), Changed(None)))
        });
        let directory = self.composer_dir(session);
        if let Err(err) = std::fs::remove_dir_all(&directory) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(session = %session, %err, "removing a session's composer directory failed");
            }
        }
    }

    /// Removes those of `paths` that are in the session's composer directory: files a withdrawn
    /// or edited message's own create or update moved in, which nothing else names.
    pub fn composer_remove_files(&self, session: &str, paths: &[String]) {
        let directory = self.composer_dir(session);
        for path in paths {
            let path = Path::new(path);
            if path.starts_with(&directory)
                && !path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                if let Err(err) = std::fs::remove_file(path) {
                    if err.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!(session = %session, %err, "removing a withdrawn message's file failed");
                    }
                }
            }
        }
    }
}

/// Whether the session's idle had a slash command written at it that has settled: the session
/// has read idle for [`COMMAND_SETTLE`] since.
fn command_settled(inner: &Inner, session: &str) -> bool {
    inner.dispatched.get(session).is_some_and(|dispatched| {
        dispatched
            .command_written
            .is_some_and(|written| written.elapsed() >= COMMAND_SETTLE)
    })
}

/// Whether the session's queue has a waiting `when_idle` message.
fn has_waiting_idle(inner: &Inner, session: &str) -> bool {
    inner.queues.get(session).is_some_and(|queue| {
        queue.iter().any(|message| {
            message.mode == QueuedMode::WhenIdle && message.state == QueuedMessageState::Waiting
        })
    })
}

/// Whether a send now of the session waits to hand its message to the session once resumed.
fn waits_on_resume(inner: &Inner, session: &str) -> bool {
    inner
        .sending
        .values()
        .any(|sending| sending.session == session && sending.after_resume)
}

/// Whether a message of the session is being handed over.
fn is_sending(inner: &Inner, session: &str) -> bool {
    inner
        .sending
        .values()
        .any(|sending| sending.session == session)
}

/// Drops what is held in memory for a session whose queue is gone.
fn forget_session(inner: &mut Inner, session: &str) {
    let ids: Vec<String> = inner
        .sending
        .iter()
        .filter(|(_, sending)| sending.session == session)
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if let Some(ticket) = inner.sending.remove(&id).and_then(|sending| sending.ticket) {
            inner.tickets.remove(&ticket);
        }
    }
    inner
        .delivered
        .retain(|delivered| delivered.session != session);
    inner.dispatched.remove(session);
    inner.unsent.remove(session);
    inner.early.retain(|(_, of, _)| of != session);
    let queued: HashSet<String> = inner
        .queues
        .values()
        .flatten()
        .map(|message| message.id.clone())
        .collect();
    inner.owned.retain(|id, _| queued.contains(id));
}

/// The place in `queue` a message written from it goes back to: after the message that was
/// before it, else before the one that was after it, else where it stood.
fn place_of(queue: &[QueuedMessage], delivered: &Delivered) -> usize {
    let at = |id: &Option<String>| {
        id.as_ref()
            .and_then(|id| queue.iter().position(|message| &message.id == id))
    };
    if let Some(before) = at(&delivered.before) {
        return before + 1;
    }
    if let Some(after) = at(&delivered.after) {
        return after;
    }
    delivered.index.min(queue.len())
}

fn index_of(queue: &[QueuedMessage], session: &str, id: &str) -> Result<usize> {
    queue
        .iter()
        .position(|message| message.id == id)
        .ok_or_else(|| not_found(session, id))
}

fn waiting_mut<'a>(
    queue: &'a mut [QueuedMessage],
    session: &str,
    id: &str,
) -> Result<&'a mut QueuedMessage> {
    let index = index_of(queue, session, id)?;
    let message = &mut queue[index];
    if message.state == QueuedMessageState::Sending {
        return Err(sending(session, id));
    }
    Ok(message)
}

/// The refusal of a message no queue holds.
pub fn not_found(session: &str, id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::QUEUED_MESSAGE_NOT_FOUND,
        "the session's composer queue holds no such message: it may have been sent or withdrawn",
        &[("session", session), ("message", id)],
    )
}

/// The refusal of a change to a message being handed over.
pub fn sending(session: &str, id: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::QUEUED_MESSAGE_SENDING,
        "the message is being sent and can no longer be changed",
        &[("session", session), ("message", id)],
    )
}

/// The failure a coded error is, of `kind`.
pub fn failure_of(kind: QueuedMessageFailureKind, error: &CodedError) -> QueuedMessageFailure {
    QueuedMessageFailure {
        kind,
        code: error.code.as_str().to_string(),
        params: error.params.clone(),
        message: error.message.clone(),
    }
}

/// The failure a delivery notice about a message is.
pub fn failure_of_notice(
    code: &str,
    params: crate::protocol::Params,
    message: &str,
) -> QueuedMessageFailure {
    QueuedMessageFailure {
        kind: QueuedMessageFailureKind::NotDelivered,
        code: code.to_string(),
        params,
        message: message.to_string(),
    }
}

/// What a queued message dropped at an archive hands back: its text, then the paths of its
/// images and files, one to a line.
pub fn content_of(message: &QueuedMessage) -> String {
    std::iter::once(message.text.as_str())
        .filter(|text| !text.is_empty())
        .chain(message.images.iter().map(String::as_str))
        .chain(message.files.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn discarded_notice(message: &QueuedMessage) -> Notice {
    let content = content_of(message);
    Notice::new(
        notice_code::QUEUED_MESSAGE_DISCARDED,
        "The session was archived, so its unsent queued message was dropped; its content is \
         handed back to copy.",
        &[("detail", &content)],
    )
}

#[cfg(test)]
mod tests;
