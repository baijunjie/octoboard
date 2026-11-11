//! The per-session FIFO for automatic messages, with one drainer per session.
//!
//! Each ticket keeps its first delivery-attempt time across retries, except one refused while the
//! session's input is held for a draft or a fragment, whose grace starts again once the hold ends,
//! and one the session's own server held back, whose grace starts again once, at most.
//! Draft protection can delay writing only for a bounded grace period; afterwards a write may
//! include unfinished user input.
//! Lifecycle holds, trust confirmations and agent dialogs remain separate reasons to wait.
//!
//! A physical partial write cannot be repeated without duplicating its accepted prefix. It and
//! the queue behind it are reported lost, because another paste would join the unterminated one.
//! A fragment written while the draft was stashed loses only itself: the session holds whatever
//! follows until nothing can join the fragment, so the queue behind it waits instead.
//! An uncertain native submission instead consumes only its own ticket and retains later ones:
//! there is no terminal fragment, but repeating the request could deliver the message twice.
//! A message the agent's own queue refuses outright fails alone: it is never written again, by any
//! route, its refusal is kept for its sender and announced once, and the queue behind it goes on.
//!
//! A complete write missing its prompt acknowledgement briefly holds later messages. Its payload
//! never goes back into the queue; the running session owns the bounded submit-key retry.
//! At a planned stop the drainers are stopped before the terminals are handed over: no write starts
//! once the handover barrier is raised ([`PromptSignal::writes_stopped`]), and what is still queued
//! at the end is taken whole ([`Outbox::seal`]) and saved for the next daemon
//! (`super::restart_outbox`); from then on the outbox takes no message
//! ([`Outbox::push_unless_sealed`]).
//!
//! While a session's writes are held for its account's usage limit
//! ([`PromptSignal::usage_limit_held`]), only the messages pushed past that hold
//! ([`Outbox::push_past_usage_limit`]) are written, in their order, and the rest keep their places.
//!
//! For the delivery contract see docs/product/message-delivery.md's
//! "Messages held until a session can take them" section, and for a fragment written while the
//! draft was stashed its "A message only partly written while the draft is stashed" section.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::platform::term::PartialWrite;
use crate::session::dialogs::Sighting;
use crate::session::term::{self, Route};
use crate::session::LiveSession;

/// How long draft preservation may delay an automatic message before combined delivery is allowed.
pub(crate) const DRAFT_WAIT: Duration = Duration::from_secs(3);

/// What a drain amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drain {
    /// Nothing is left queued for this session.
    Clear,
    /// Something is still queued, and a later drain will take it.
    Pending,
    /// What is still queued is held for the account's usage limit
    /// ([`PromptSignal::usage_limit_held`]), and nothing past that hold is left: every message
    /// taken off the queue before this drain ended has been written, as for [`Self::Clear`].
    HeldForUsageLimit,
    /// A message was written in part, so it is gone and the queue behind it with it.
    Lost,
    /// The message with this ticket was written in part while the session's draft was stashed
    /// ([`crate::session::HeldFragment`]). It is gone, but the session holds Octoboard's later
    /// writes until a prompt is sent after it, so nothing written next can run together with the
    /// fragment: the queue behind it stays, and is written in order once the hold ends.
    Fragmented(Ticket),
    /// The native queue may have accepted this ticket; it is never submitted again.
    Uncertain(Ticket),
    /// The message with this ticket was written, but the agent did not report sending it. Its text
    /// is not repeated; later messages wait for a prompt acknowledgement or a bounded submit retry.
    Unsent(Ticket),
    /// The message with this ticket was written while a dialog came up that threw it away. It is
    /// back at the front of the queue, which waits for the dialog to be answered.
    Discarded(Ticket),
    /// The message with this ticket was written while a view of the agent's background agents came
    /// up, which took it in place of the conversation. It is not queued again; the rest of the
    /// queue waits for the view to be left.
    Diverted(Ticket),
}

/// What [`Outbox::push`] hands back for a message, for telling it apart in a drain's outcome.
pub type Ticket = u64;

/// What a session's queue is held for ([`Outbox::hold`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldKind {
    /// The session is being handed off, and what is queued goes to its successor.
    Handoff,
    /// The session is being restarted for a change of its account's sharing.
    Sharing,
    /// The session's process is being restarted by a switch of its account or a change of its
    /// model or effort, and what is queued goes to the relaunched process.
    Restart,
    /// [`Self::Restart`] once the relaunch has started a process: what is queued is written into
    /// it as into any session, and is kept, not forgotten, if that process ends, for whatever
    /// brings the session back up next.
    Relaunched,
}

impl HoldKind {
    /// Whether a hold of this kind keeps the queue from being written.
    fn blocks_writes(self) -> bool {
        self != Self::Relaunched
    }
}

/// The first message's place, reserved before its session is published and while its queue is paused.
pub(crate) struct FirstTicket {
    id: String,
    ticket: Ticket,
}

/// What a drain learns from the agent about whether what it wrote was sent.
pub trait PromptSignal {
    /// How many prompts the agent has reported sending so far.
    fn sent(&self) -> u64;
    /// Whether a message written now should be followed by a report of it being sent: the agent
    /// reports each prompt it sends, and is at its prompt rather than mid-turn.
    fn expects_send(&self) -> bool;
    /// Waits, for a bounded time, until [`Self::sent`] has passed `before`; says whether it did.
    fn wait_past(&self, before: u64) -> bool;
    /// Records that a message written when the count stood at `before` was not sent.
    fn mark_unsent(&self, before: u64);
    /// Records that a message written when the count stood at `before` was left in the input box,
    /// its submit key pressed or held back by the write itself (`crate::session::NotSubmitted`), so
    /// no later retry presses it.
    fn mark_not_submitted(&self, before: u64);
    /// Whether a message marked unsent still holds delivery before acknowledgement or submit retry.
    fn unsent_waiting(&self) -> bool;
    /// Records that a message the agent's own queue may have taken, written when the count stood
    /// at `before`, was not confirmed taken: later messages wait a little, and it is never written
    /// again. Nothing for a session without such a queue.
    fn mark_submission_uncertain(&self, _before: u64) {}
    /// Whether a message marked by [`Self::mark_submission_uncertain`] still holds later ones; never
    /// for a session that marks none.
    fn submission_uncertain(&self) -> bool {
        false
    }
    /// Looks at the agent's screen now: whether it shows a dialog only the user can answer, which
    /// a message written into it would answer destructively, and how many times dialogs of each
    /// kind have come up so far, including any that have gone again.
    fn sighting(&self) -> Sighting;
    /// Starts (`true`) or ends (`false`) having the screen looked at on every chunk of output, so
    /// that a dialog drawn and closed between two calls of [`Self::sighting`] is still counted.
    fn watch_screen(&self, on: bool);
    /// Whether the agent's folder-trust screen has been sighted and not answered, which a message
    /// written into it would answer.
    fn trust_holds_writes(&self) -> bool;
    /// Whether the agent is in the middle of a turn, read before a message is written so the
    /// message that opens a turn is not taken for one written into it.
    fn mid_turn(&self) -> bool {
        false
    }
    /// Records that a message went into the agent in the middle of a turn.
    fn note_written_mid_turn(&self) {}
    /// A complete message reached the CLI, whose native draft restoration is still pending.
    fn input_recovery_pending(&self) -> bool {
        false
    }
    /// Whether the session's input is held for a draft being stashed or restored, or for a
    /// fragment ([`crate::session::RecoveryPhase`]), which refuses every message until it ends.
    fn input_held(&self) -> bool {
        false
    }
    /// Whether Octoboard's writes are held because a turn stopped on the account's usage limit,
    /// which only messages pushed past it go through ([`Outbox::push_past_usage_limit`]).
    fn usage_limit_held(&self) -> bool {
        false
    }
    /// Whether no write may start any more: a planned stop is handing the terminals over, and
    /// what is still queued goes to the next daemon. Read before each message.
    fn writes_stopped(&self) -> bool {
        false
    }
}

/// A screen watch that ends when it is dropped, so no return from a drain leaves it on.
struct ScreenWatch<'a, P: PromptSignal>(&'a P);

impl<'a, P: PromptSignal> ScreenWatch<'a, P> {
    fn start(prompts: &'a P) -> Self {
        prompts.watch_screen(true);
        Self(prompts)
    }
}

impl<P: PromptSignal> Drop for ScreenWatch<'_, P> {
    fn drop(&mut self) {
        self.0.watch_screen(false);
    }
}

type MessageQueue = VecDeque<(Ticket, String, Option<Instant>)>;

#[derive(Default)]
pub struct Outbox {
    queues: Mutex<HashMap<String, MessageQueue>>,
    /// The ticket the next message pushed gets. A message is written as `<launch id>:<ticket>`
    /// (see [`Self::drain`]), and an agent's own queue keeps that id across daemon runs, so a
    /// daemon's tickets start from the clock ([`Self::seeded_from_clock`]).
    next_ticket: AtomicU64,
    /// The sessions a drainer currently owns.
    draining: Mutex<HashSet<String>>,
    /// The sessions whose queue is held, and what for: messages are still accepted but none is
    /// written.
    held: Mutex<HashMap<String, HoldKind>>,
    /// The messages a hold for the account's usage limit does not hold, by ticket: the user's own,
    /// sent from a client.
    past_usage_limit: Mutex<HashSet<Ticket>>,
    /// The instruction a relaunch after a restart put at the front of the session's held queue
    /// ([`Outbox::push_relaunch_instruction`]), by session: withdrawn if the relaunch fails, as a
    /// restore of the session after that gives none.
    relaunch_instructions: Mutex<HashMap<String, Ticket>>,
    /// The sessions whose queue is paused for a moment (`AppState::pause_outbox`), each with how
    /// many pauses are held on it: like a hold, nothing is written, but it is no handoff and has no
    /// successor.
    paused: Mutex<HashMap<String, usize>>,
    /// The messages that are also kept in the store (`Store::add_kept_message`), by ticket, and
    /// the row each is kept in. A row is deleted only once its message has been written into the
    /// session ([`Self::take_written`]), so a daemon that exits before the queue drains, or a
    /// process that ends first, leaves it for the next resume.
    kept: Mutex<Keeps>,
    /// The messages the agent's own queue refused outright (`term::RefusedByAgent`), by ticket:
    /// for the sender waiting on one ([`Self::take_refusal`]), and to be announced once
    /// ([`Self::refusals_to_announce`]).
    refused: Mutex<HashMap<Ticket, Refusal>>,
    /// The messages whose draft grace a hold of the session's own server has already started
    /// again, which it does once a message.
    grace_restarted: Mutex<HashSet<Ticket>>,
    /// Set by [`Self::seal`] first: no new message is accepted, but a drain still writing may put
    /// back the message it could not write, to be taken with the rest. Changed under the queues'
    /// lock.
    sealed: AtomicBool,
    /// Set by [`Self::seal`] once it has taken the queues for the next daemon: nothing put on a
    /// queue from then on is ever written. Changed under the queues' lock.
    taken: AtomicBool,
}

/// A message the agent's own queue refused outright.
struct Refusal {
    session: String,
    reason: String,
    /// When it was announced, if it has been.
    announced: Option<Instant>,
}

/// How long a refusal is kept for its sender once announced, counted while no drain of its session
/// runs: the sender waiting on it takes it right after the drain that met it, however long that
/// drain goes on writing, so one left longer had no sender waiting.
const REFUSAL_KEPT: Duration = Duration::from_secs(60);

/// What [`Outbox::seal`] took for one session, oldest first: each message's ticket, text and the
/// row it is kept in, if it is.
pub type SealedQueue = (String, Vec<(Ticket, String, Option<i64>)>);

/// A session's drain claim, from [`Outbox::claim`].
struct DrainClaim<'a> {
    outbox: &'a Outbox,
    id: &'a str,
}

impl Drop for DrainClaim<'_> {
    fn drop(&mut self) {
        self.outbox
            .draining
            .lock()
            .expect("outbox claim lock poisoned")
            .remove(self.id);
        // The time a refusal is kept counts from the end of the drain its sender waits on.
        let now = Instant::now();
        for refusal in self
            .outbox
            .refused
            .lock()
            .expect("outbox refused lock poisoned")
            .values_mut()
            .filter(|refusal| refusal.session == self.id)
        {
            if refusal.announced.is_some() {
                refusal.announced = Some(now);
            }
        }
    }
}

#[derive(Default)]
struct Keeps {
    /// Ticket to row, for a message still queued or being written.
    queued: HashMap<Ticket, i64>,
    /// The same the other way round, so a row is queued once however often it is read.
    tickets: HashMap<i64, Ticket>,
    /// The rows whose messages have been written, not yet deleted from the store.
    written: Vec<i64>,
}

impl Outbox {
    /// An outbox whose tickets start at the microseconds since the Unix epoch, so that a later
    /// daemon run, adopting a session whose agent still queues messages an earlier run wrote,
    /// never gives one of its own the id of one of those: a run would have to give out more than a
    /// ticket per microsecond to catch up with the next. A clock set back between the runs by more
    /// than that margin can still repeat an id.
    pub fn seeded_from_clock() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_micros()).unwrap_or(u64::MAX / 2)
            });
        Self {
            next_ticket: AtomicU64::new(seed),
            ..Self::default()
        }
    }

    /// Accepts a message for a session, to be written as soon as it can take one; `None`, with
    /// nothing queued, once [`Self::seal`] has taken the queues for the next daemon, since nothing
    /// queued from then on would ever be written: the caller says what became of the message.
    /// Unlike [`Self::push_unless_sealed`] it still takes a message while the seal waits for the
    /// drains, as a handoff ending then forwards what it held, which is taken with the rest.
    pub fn push(&self, id: &str, text: &str) -> Option<Ticket> {
        // Taken under the queue's lock, so that tickets follow the order messages are queued in.
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.taken.load(Ordering::Acquire) {
            return None;
        }
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        queues
            .entry(id.to_string())
            .or_default()
            .push_back((ticket, text.to_string(), None));
        Some(ticket)
    }

    /// [`Self::push_unless_sealed`] for a message a hold for the account's usage limit does not
    /// hold: written as soon as nothing else holds the session, ahead of the messages that hold
    /// keeps back, which stay in their order behind it.
    pub fn push_past_usage_limit(&self, id: &str, text: &str) -> Option<Ticket> {
        self.push_unsealed(id, text, true)
    }

    /// The instruction a relaunch gives a session whose queue a restart holds
    /// ([`HoldKind::Restart`]): put at the front, ahead of what the restart carries over, and noted
    /// so that [`Self::withdraw_relaunch_instruction`] can take it back. `None`, with nothing
    /// queued, once the queues have been sealed for the next daemon ([`Self::seal`]).
    pub fn push_relaunch_instruction(&self, id: &str, text: &str) -> Option<Ticket> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.sealed.load(Ordering::Acquire) {
            return None;
        }
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        queues
            .entry(id.to_string())
            .or_default()
            .push_front((ticket, text.to_string(), None));
        self.relaunch_instructions
            .lock()
            .expect("outbox relaunch lock poisoned")
            .insert(id.to_string(), ticket);
        Some(ticket)
    }

    /// Takes back the instruction a failed relaunch put on the session's queue
    /// ([`Self::push_relaunch_instruction`]), if it is still queued.
    pub fn withdraw_relaunch_instruction(&self, id: &str) {
        let Some(ticket) = self
            .relaunch_instructions
            .lock()
            .expect("outbox relaunch lock poisoned")
            .remove(id)
        else {
            return;
        };
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if let Some(queue) = queues.get_mut(id) {
            queue.retain(|(queued, _, _)| *queued != ticket);
            if queue.is_empty() {
                queues.remove(id);
            }
        }
        drop(queues);
        self.unkeep(std::iter::once(ticket));
    }

    /// Whether the message `ticket` is still queued for the session `id`, not taken by a drain.
    pub fn is_queued(&self, id: &str, ticket: Ticket) -> bool {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .get(id)
            .is_some_and(|queue| queue.iter().any(|(queued, _, _)| *queued == ticket))
    }

    /// [`Self::push`], unless the queues have been sealed for the next daemon ([`Self::seal`]):
    /// then `None`, and nothing is queued. Judged under the queues' lock, so a message either is
    /// among those sealed or is refused.
    pub fn push_unless_sealed(&self, id: &str, text: &str) -> Option<Ticket> {
        self.push_unsealed(id, text, false)
    }

    /// [`Self::push_unless_sealed`], marking the message as going past a hold for the account's
    /// usage limit when `past_usage_limit`.
    fn push_unsealed(&self, id: &str, text: &str, past_usage_limit: bool) -> Option<Ticket> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.sealed.load(Ordering::Acquire) {
            return None;
        }
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        if past_usage_limit {
            self.past_usage_limit
                .lock()
                .expect("outbox usage limit lock poisoned")
                .insert(ticket);
        }
        queues
            .entry(id.to_string())
            .or_default()
            .push_back((ticket, text.to_string(), None));
        Some(ticket)
    }

    /// Whether the queues have been sealed for the next daemon ([`Self::seal`]).
    #[cfg(test)]
    pub fn is_sealed(&self) -> bool {
        self.sealed.load(Ordering::Acquire)
    }

    /// Takes every queue, each with its messages oldest first and the row each is kept in, and
    /// takes no message from then on: what a planned stop saves for the next daemon, once no drain
    /// begins another write. A message is accepted no more from the first step on
    /// ([`Self::push_unless_sealed`], [`Self::push_kept`], [`Self::push_first`]); then a drain
    /// still writing is waited for, for at most `drains`, so that the message whose write the
    /// handover refused, which it puts back, is taken with the rest rather than lost. The rows
    /// stay in the store, for the next daemon's queue to keep them as this one did. Sessions come
    /// in no particular order. Once the queues are taken, [`Self::push`] refuses too. **Blocks**
    /// for up to `drains`.
    pub fn seal(&self, drains: Duration) -> Vec<SealedQueue> {
        {
            let _queues = self.queues.lock().expect("outbox lock poisoned");
            self.sealed.store(true, Ordering::Release);
        }
        let deadline = Instant::now() + drains;
        while !self
            .draining
            .lock()
            .expect("outbox claim lock poisoned")
            .is_empty()
        {
            if Instant::now() >= deadline {
                tracing::error!("a drain was still writing when the queues were saved for the next daemon; a message it puts back is lost");
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        self.taken.store(true, Ordering::Release);
        let taken = std::mem::take(&mut *queues);
        drop(queues);
        let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
        taken
            .into_iter()
            .map(|(id, queue)| {
                let messages = queue
                    .into_iter()
                    .map(|(ticket, text, _)| {
                        let row = kept.queued.remove(&ticket);
                        if let Some(row) = row {
                            kept.tickets.remove(&row);
                        }
                        (ticket, text, row)
                    })
                    .collect();
                (id, messages)
            })
            .collect()
    }

    /// Reserves the first ticket without queuing a placeholder that could be delivered or lost.
    pub(crate) fn reserve_first(&self, id: &str) -> anyhow::Result<FirstTicket> {
        anyhow::ensure!(
            self.is_paused(id),
            "a first message requires a paused queue"
        );
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        anyhow::ensure!(
            !queues.contains_key(id),
            "the first message must be reserved before other messages"
        );
        queues.insert(id.to_string(), MessageQueue::new());
        Ok(FirstTicket {
            id: id.to_string(),
            ticket: self.next_ticket.fetch_add(1, Ordering::Relaxed),
        })
    }

    pub(crate) fn push_first(&self, first: FirstTicket, text: &str) -> anyhow::Result<Ticket> {
        anyhow::ensure!(
            self.is_paused(&first.id),
            "a reserved first message requires its queue pause"
        );
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.sealed.load(Ordering::Acquire) {
            return Err(crate::protocol::CodedError::raised(
                crate::protocol::error_code::DAEMON_SHUTTING_DOWN,
                "the daemon is stopping and has saved the queued messages for the next daemon, so \
                 this message is not accepted",
                &[],
            ));
        }
        let queue = queues.entry(first.id).or_default();
        anyhow::ensure!(
            queue
                .front()
                .is_none_or(|(ticket, _, _)| *ticket > first.ticket),
            "a reserved first ticket must precede the queue"
        );
        queue.push_front((first.ticket, text.to_string(), None));
        Ok(first.ticket)
    }

    pub(crate) fn cancel_first(&self, first: FirstTicket) {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if queues.get(&first.id).is_some_and(MessageQueue::is_empty) {
            queues.remove(&first.id);
        }
    }

    /// [`Self::push`] for a message kept in the store as row `row`, which goes once it is written.
    /// A row that already has a message queued or being written is not queued again, whatever
    /// session it is asked for: a queue held across a restart still has it. Nor is one once the
    /// queues have been sealed for the next daemon ([`Self::seal`]): the row stays in the store,
    /// for the session's next resume.
    pub fn push_kept(&self, id: &str, text: &str, row: i64) -> Option<Ticket> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.sealed.load(Ordering::Acquire) {
            return None;
        }
        let ticket = {
            let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
            if kept.tickets.contains_key(&row) {
                return None;
            }
            let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
            kept.queued.insert(ticket, row);
            kept.tickets.insert(row, ticket);
            ticket
        };
        queues
            .entry(id.to_string())
            .or_default()
            .push_back((ticket, text.to_string(), None));
        Some(ticket)
    }

    /// Queues a message a planned stop saved under the ticket it had, so that the id it is written
    /// under (see [`Self::drain`]) is the one an agent's own queue may already know it by: a
    /// message the agent took but did not start is then started in place, not added again.
    /// `row` is the store row it is kept in, if it is. Otherwise as [`Self::push`] and
    /// [`Self::push_kept`]: `None` once sealed, or when `row` is already queued. A ticket that is
    /// already queued is never given twice: such a message gets a new one. The next ticket is kept
    /// above every restored one, so no later message repeats it.
    pub fn push_restored(
        &self,
        id: &str,
        text: &str,
        row: Option<i64>,
        ticket: Ticket,
    ) -> Option<Ticket> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.sealed.load(Ordering::Acquire) {
            return None;
        }
        let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
        if row.is_some_and(|row| kept.tickets.contains_key(&row)) {
            return None;
        }
        let in_use = kept.queued.contains_key(&ticket)
            || queues
                .values()
                .any(|queue| queue.iter().any(|(queued, _, _)| *queued == ticket));
        let ticket = if in_use {
            self.next_ticket.fetch_add(1, Ordering::Relaxed)
        } else {
            self.next_ticket
                .fetch_max(ticket.saturating_add(1), Ordering::Relaxed);
            ticket
        };
        if let Some(row) = row {
            kept.queued.insert(ticket, row);
            kept.tickets.insert(row, ticket);
        }
        drop(kept);
        queues
            .entry(id.to_string())
            .or_default()
            .push_back((ticket, text.to_string(), None));
        Some(ticket)
    }

    /// Whether the message kept as row `row` is queued or being written.
    pub fn is_row_queued(&self, row: i64) -> bool {
        self.kept
            .lock()
            .expect("outbox kept lock poisoned")
            .tickets
            .contains_key(&row)
    }

    /// The rows of kept messages written into their sessions since the last call, for the store to
    /// delete.
    pub fn take_written(&self) -> Vec<i64> {
        std::mem::take(&mut self.kept.lock().expect("outbox kept lock poisoned").written)
    }

    /// The refusal of the message `ticket`, for its sender, which takes it.
    pub fn take_refusal(&self, ticket: Ticket) -> Option<String> {
        self.refused
            .lock()
            .expect("outbox refused lock poisoned")
            .remove(&ticket)
            .map(|refusal| refusal.reason)
    }

    /// The refusals of messages for session `id` not announced yet, by ticket with the agent's
    /// reason, which are now marked announced; whichever drain asks first announces them, and
    /// each sender still finds its own ([`Self::take_refusal`]).
    pub fn refusals_to_announce(&self, id: &str) -> Vec<(Ticket, String)> {
        let draining = self
            .draining
            .lock()
            .expect("outbox claim lock poisoned")
            .clone();
        let mut refused = self.refused.lock().expect("outbox refused lock poisoned");
        // A refusal of a session still being drained may have its sender waiting on that drain;
        // its time restarts when the drain ends ([`DrainClaim`]).
        refused.retain(|_, refusal| {
            draining.contains(&refusal.session)
                || refusal
                    .announced
                    .is_none_or(|announced| announced.elapsed() < REFUSAL_KEPT)
        });
        let mut announce: Vec<(Ticket, String)> = refused
            .iter_mut()
            .filter(|(_, refusal)| refusal.session == id && refusal.announced.is_none())
            .map(|(ticket, refusal)| {
                refusal.announced = Some(Instant::now());
                (*ticket, refusal.reason.clone())
            })
            .collect();
        announce.sort_by_key(|(ticket, _)| *ticket);
        announce
    }

    /// The message with this ticket is in its session, so its row, if it has one, may go.
    fn mark_written(&self, ticket: Ticket) {
        self.forget_ticket_marks(std::iter::once(ticket));
        let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
        if let Some(row) = kept.queued.remove(&ticket) {
            kept.tickets.remove(&row);
            kept.written.push(row);
        }
    }

    /// The messages with these tickets will not be written by this queue, so their rows stay.
    fn unkeep(&self, tickets: impl Iterator<Item = Ticket> + Clone) {
        self.forget_ticket_marks(tickets.clone());
        let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
        for ticket in tickets {
            if let Some(row) = kept.queued.remove(&ticket) {
                kept.tickets.remove(&row);
            }
        }
    }

    /// Forgets what is noted of these messages beside the queue as they leave it: whether a hold of
    /// the server started their grace again, and whether they go past a hold for the usage limit.
    fn forget_ticket_marks(&self, tickets: impl Iterator<Item = Ticket>) {
        let mut restarted = self
            .grace_restarted
            .lock()
            .expect("outbox grace lock poisoned");
        let mut past = self
            .past_usage_limit
            .lock()
            .expect("outbox usage limit lock poisoned");
        for ticket in tickets {
            restarted.remove(&ticket);
            past.remove(&ticket);
        }
    }

    /// Writes out whatever is queued for this session, oldest first.
    ///
    /// **Blocks** on the PTY write, which polls for seconds when the child is not draining, and on
    /// the wait for a written message to be sent.
    pub fn drain(&self, session: &LiveSession, prompts: &impl PromptSignal) -> Drain {
        self.drain_with(
            &session.id,
            &mut |ticket, text, allow_mixed| {
                term::send_message_with_id(
                    session,
                    text,
                    &format!("{}:{ticket}", session.launch_id),
                    allow_mixed,
                    &|| prompts.writes_stopped(),
                )
            },
            prompts,
        )
    }

    /// The body of [`Self::drain`], with the write and the agent's prompt signal as parameters so
    /// the queue's own rules can be exercised without a PTY or an agent behind them.
    fn drain_with(
        &self,
        id: &str,
        write: &mut impl FnMut(Ticket, &str, bool) -> Result<Route, PartialWrite>,
        prompts: &impl PromptSignal,
    ) -> Drain {
        // Re-claimed around each pass. A producer that pushed while the previous pass was between
        // its last pop and releasing the claim would otherwise leave a queue with nothing to drain
        // it — and for a session producing no further events, nothing would come along to retry.
        // The claim is released before the queue is re-read, so there is no ordering in which the
        // producer both fails to claim and lets this loop exit.
        loop {
            if self.blocks_writes(id) || self.is_paused(id) {
                return Drain::Pending;
            }
            let Some(claim) = self.claim(id) else {
                // Another drainer owns it and will take whatever was just pushed.
                return Drain::Pending;
            };
            let outcome = self.drain_once(id, write, prompts);
            drop(claim);
            match outcome {
                Drain::Clear if self.len(id) > 0 => continue,
                outcome => return outcome,
            }
        }
    }

    fn drain_once(
        &self,
        id: &str,
        write: &mut impl FnMut(Ticket, &str, bool) -> Result<Route, PartialWrite>,
        prompts: &impl PromptSignal,
    ) -> Drain {
        loop {
            // Re-read for every message: a hold taken while this pass runs stops it at the next one.
            if self.blocks_writes(id) || self.is_paused(id) {
                return Drain::Pending;
            }
            if prompts.unsent_waiting()
                || prompts.submission_uncertain()
                || prompts.trust_holds_writes()
            {
                return Drain::Pending;
            }
            // Left for the next daemon, whole: a message cut by the handover's mark half-way would
            // be lost, and one never begun is not.
            if prompts.writes_stopped() {
                return Drain::Pending;
            }
            // Asked afresh for every message: once one is sent the agent reads as working, and what
            // follows is queued by the agent until that turn ends.
            let checked = prompts.expects_send();
            // From before the look at the screen until the write has been waited on, so that a
            // dialog coming up in between, and gone again, is counted all the same. The look is
            // also the mark the count is compared with.
            let _watch = checked.then(|| ScreenWatch::start(prompts));
            let seen = prompts.sighting();
            if seen.current.is_some() {
                return Drain::Pending;
            }
            let next = if prompts.usage_limit_held() {
                match self.pop_past_usage_limit(id) {
                    Some(next) => Some(next),
                    // What is left is held, not drained.
                    None if self.len(id) > 0 => return Drain::HeldForUsageLimit,
                    None => None,
                }
            } else {
                self.pop(id)
            };
            let Some((ticket, text, protection_started)) = next else {
                return Drain::Clear;
            };
            let protection_started = protection_started.unwrap_or_else(Instant::now);
            let before = prompts.sent();
            let mid_turn = prompts.mid_turn();
            let written = write(ticket, &text, protection_started.elapsed() >= DRAFT_WAIT);
            // A message the agent's own queue took is no part of the running turn: it runs after it.
            if mid_turn && matches!(written, Ok(Route::Terminal)) {
                prompts.note_written_mid_turn();
            }
            match written {
                // Nothing of it is in the input box, and the queue answered for it.
                Ok(Route::Queued) => self.mark_written(ticket),
                Ok(Route::Terminal) if prompts.input_recovery_pending() => {
                    self.mark_written(ticket);
                    return Drain::Pending;
                }
                Ok(Route::Terminal) if !checked || prompts.wait_past(before) => {
                    self.mark_written(ticket)
                }
                Ok(Route::Terminal) => {
                    let now = prompts.sighting();
                    if now.discarded_seen > seen.discarded_seen {
                        // Its paste went to a dialog that discarded it, so unlike an unsent
                        // message nothing of it is in the input box.
                        tracing::warn!(session = %id, "a dialog discarded a written message");
                        self.push_front(id, ticket, text, Some(protection_started));
                        return Drain::Discarded(ticket);
                    }
                    if now.diverted_seen > seen.diverted_seen {
                        tracing::warn!(session = %id, "a view of background agents took a message");
                        self.mark_written(ticket);
                        return Drain::Diverted(ticket);
                    }
                    tracing::warn!(session = %id, "a written message was not sent by the agent");
                    // Its text is in the input box, which is as far as a written message goes.
                    self.mark_written(ticket);
                    prompts.mark_unsent(before);
                    return Drain::Unsent(ticket);
                }
                // Written in full, but its submit key left it in the input box
                // (`crate::session::NotSubmitted`): an unsent message, whatever the agent reports.
                Err(partial)
                    if partial
                        .error
                        .get_ref()
                        .is_some_and(|error| error.is::<crate::session::NotSubmitted>()) =>
                {
                    tracing::warn!(session = %id, "a written message was left in the input box");
                    self.mark_written(ticket);
                    prompts.mark_not_submitted(before);
                    return Drain::Unsent(ticket);
                }
                // The agent's own queue may have taken it, whatever the count of what went in says,
                // so it is never put back.
                Err(partial)
                    if partial.error.get_ref().is_some_and(|error| {
                        error.is::<crate::session::term::UncertainSubmission>()
                    }) =>
                {
                    self.mark_written(ticket);
                    prompts.mark_submission_uncertain(before);
                    tracing::error!(session = %id, error = %partial.error,
                        "the native queue did not confirm accepting a message; it will not be resent");
                    return Drain::Uncertain(ticket);
                }
                // The agent's own queue refused it outright: it fails to its sender, is never written
                // again, and the queue behind it goes on.
                Err(partial)
                    if partial
                        .error
                        .get_ref()
                        .is_some_and(|error| error.is::<term::RefusedByAgent>()) =>
                {
                    tracing::warn!(session = %id, error = %partial.error, "the agent's own queue refused a message");
                    self.mark_written(ticket);
                    self.refused
                        .lock()
                        .expect("outbox refused lock poisoned")
                        .insert(
                            ticket,
                            Refusal {
                                session: id.to_string(),
                                reason: partial.error.to_string(),
                                announced: None,
                            },
                        );
                    continue;
                }
                // Nothing went in, so the session simply could not take it — most likely a modal its
                // reported state does not show. Put it back at the front and stop: letting the next
                // one through would reorder them.
                Err(partial) if partial.written == 0 => {
                    if partial.error.kind() != std::io::ErrorKind::WouldBlock {
                        tracing::warn!(session = %id, error = %partial.error, "a queued message could not be written yet");
                    }
                    // A hold on the input, or the agent's own server holding the message back, is
                    // no wait for a draft, which the message's grace measures: spent behind it, the
                    // grace would let the message join whatever the user has in the box the moment
                    // the terminal takes it.
                    // Started again once a message, so that a session flipping between the two kinds
                    // of hold cannot keep a message waiting past its grace.
                    let held_by_server = partial
                        .error
                        .get_ref()
                        .is_some_and(|error| error.is::<term::HeldByServer>())
                        && self
                            .grace_restarted
                            .lock()
                            .expect("outbox grace lock poisoned")
                            .insert(ticket);
                    let protection_started =
                        (!prompts.input_held() && !held_by_server).then_some(protection_started);
                    self.push_front(id, ticket, text, protection_started);
                    return Drain::Pending;
                }
                Err(partial)
                    if partial
                        .error
                        .get_ref()
                        .is_some_and(|error| error.is::<crate::session::HeldFragment>()) =>
                {
                    // Only this message is lost; the session holds what follows until it can no
                    // longer join the fragment, so the queue behind it waits, in order, as it does
                    // for any hold. A drain meanwhile finds the session refusing writes.
                    self.mark_written(ticket);
                    tracing::error!(
                        session = %id,
                        written = partial.written,
                        error = %partial.error,
                        "a message was only partly written while the draft was stashed; it has \
                         been dropped and the queue behind it waits for the session's hold to end"
                    );
                    return Drain::Fragmented(ticket);
                }
                Err(partial) => {
                    // The queue behind it goes too. The next message written into this session is
                    // run together with the fragment, so it would arrive spoiled — delivering it
                    // spoiled is worse than saying it was not delivered.
                    // The sender is told these messages were lost, so their rows are done with
                    // too, and a resume does not deliver what was reported undelivered.
                    self.mark_written(ticket);
                    let abandoned = self.forget_as_done(id);
                    tracing::error!(
                        session = %id,
                        written = partial.written,
                        error = %partial.error,
                        abandoned,
                        "a message was only partly written; it and the rest of the queue have been \
                         dropped"
                    );
                    return Drain::Lost;
                }
            }
        }
    }

    /// How many messages are still queued for this session.
    pub fn len(&self, id: &str) -> usize {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .get(id)
            .map(|queue| queue.len())
            .unwrap_or(0)
    }

    /// Whether anything is queued for this session or being written into it now.
    pub fn is_pending(&self, id: &str) -> bool {
        self.len(id) > 0
            || self
                .draining
                .lock()
                .expect("outbox claim lock poisoned")
                .contains(id)
    }

    /// [`Self::forget`] for a queue that is reported lost: the rows of its kept messages go too.
    fn forget_as_done(&self, id: &str) -> usize {
        let queue = self
            .queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .unwrap_or_default();
        for (ticket, _, _) in &queue {
            self.mark_written(*ticket);
        }
        queue.len()
    }

    /// Forgets this session's queue and says how much went with it. Called when its process is gone
    /// — there is nothing to write to, and a resume starts the agent at its prompt rather than
    /// replaying a queue it never saw — and when a write left the input line holding a fragment.
    pub fn forget(&self, id: &str) -> usize {
        let queue = self
            .queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .unwrap_or_default();
        self.unkeep(queue.iter().map(|(ticket, _, _)| *ticket));
        queue.len()
    }

    /// Stops writing this session's queue until [`Self::release_hold`]. Pushing still works.
    pub fn hold(&self, id: &str, kind: HoldKind) {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .insert(id.to_string(), kind);
    }

    /// Stops writing this session's queue until every pause taken has been ended with
    /// [`Self::resume`]. Pushing still works.
    pub fn pause(&self, id: &str) {
        *self
            .paused
            .lock()
            .expect("outbox pause lock poisoned")
            .entry(id.to_string())
            .or_default() += 1;
    }

    /// Ends one pause; the queue is written again once none is left.
    pub fn resume(&self, id: &str) {
        let mut paused = self.paused.lock().expect("outbox pause lock poisoned");
        if let Some(count) = paused.get_mut(id) {
            *count -= 1;
            if *count == 0 {
                paused.remove(id);
            }
        }
    }

    pub fn is_paused(&self, id: &str) -> bool {
        self.paused
            .lock()
            .expect("outbox pause lock poisoned")
            .contains_key(id)
    }

    pub fn release_hold(&self, id: &str) {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .remove(id);
        self.relaunch_instructions
            .lock()
            .expect("outbox relaunch lock poisoned")
            .remove(id);
    }

    /// Whether this session's queue is held from being written ([`HoldKind::blocks_writes`]).
    pub fn blocks_writes(&self, id: &str) -> bool {
        self.held_for(id).is_some_and(HoldKind::blocks_writes)
    }

    /// Whether this session's queue is held for anything, which keeps it when the session's
    /// process ends.
    pub fn is_held(&self, id: &str) -> bool {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .contains_key(id)
    }

    /// What the hold on this session's queue is for, if it is held.
    pub fn held_for(&self, id: &str) -> Option<HoldKind> {
        self.held
            .lock()
            .expect("outbox hold lock poisoned")
            .get(id)
            .copied()
    }

    /// Removes and returns what is queued for this session, oldest first, each with the row it is
    /// kept in, if it is.
    pub fn take(&self, id: &str) -> Vec<(String, Option<i64>)> {
        let queue = self
            .queues
            .lock()
            .expect("outbox lock poisoned")
            .remove(id)
            .unwrap_or_default();
        // Whoever takes it forwards it, and what is kept for the session is theirs to deal with.
        let kept = self.kept.lock().expect("outbox kept lock poisoned");
        let rows: Vec<Option<i64>> = queue
            .iter()
            .map(|(ticket, _, _)| kept.queued.get(ticket).copied())
            .collect();
        drop(kept);
        self.unkeep(queue.iter().map(|(ticket, _, _)| *ticket));
        queue
            .into_iter()
            .zip(rows)
            .map(|((_, text, _), row)| (text, row))
            .collect()
    }

    /// Makes the caller the session's one drainer until the claim is dropped, or `None` when
    /// another drainer is. Released on `Drop`, so a write that panics cannot leave the session's
    /// queue with a claim nobody holds, and so never drained again.
    fn claim<'a>(&'a self, id: &'a str) -> Option<DrainClaim<'a>> {
        let claimed = self
            .draining
            .lock()
            .expect("outbox claim lock poisoned")
            .insert(id.to_string());
        claimed.then(|| DrainClaim { outbox: self, id })
    }

    fn pop(&self, id: &str) -> Option<(Ticket, String, Option<Instant>)> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let queue = queues.get_mut(id)?;
        let next = queue.pop_front();
        if queue.is_empty() {
            queues.remove(id);
        }
        next
    }

    /// Takes the oldest message of the session's queue that a hold for the usage limit does not
    /// hold, leaving the others where they are.
    fn pop_past_usage_limit(&self, id: &str) -> Option<(Ticket, String, Option<Instant>)> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let queue = queues.get_mut(id)?;
        let past = self
            .past_usage_limit
            .lock()
            .expect("outbox usage limit lock poisoned");
        let index = queue
            .iter()
            .position(|(ticket, _, _)| past.contains(ticket))?;
        drop(past);
        let next = queue.remove(index);
        if queue.is_empty() {
            queues.remove(id);
        }
        next
    }

    /// Puts a message a drain could not write back at the front of its queue. Accepted after the
    /// queues were sealed, while [`Self::seal`] waits for the drains, so that the message is taken
    /// with the rest; one put back once they were taken is lost, and said so in the log.
    fn push_front(
        &self,
        id: &str,
        ticket: Ticket,
        text: String,
        protection_started: Option<Instant>,
    ) {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.taken.load(Ordering::Acquire) {
            tracing::error!(session = %id, "a message that could not be written was put back after the queues were saved for the next daemon; it is lost");
        }
        queues
            .entry(id.to_string())
            .or_default()
            .push_front((ticket, text, protection_started));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentId;
    use crate::session::dialogs::{Dialog, WrittenMessage};
    use std::cell::Cell;
    use std::sync::Arc;

    /// An agent mid-turn, or one that reports no prompts at all: nothing written is waited on.
    struct MidTurn;

    impl PromptSignal for MidTurn {
        fn sent(&self) -> u64 {
            0
        }
        fn expects_send(&self) -> bool {
            false
        }
        fn wait_past(&self, _before: u64) -> bool {
            unreachable!("nothing is waited on mid-turn")
        }
        fn mark_unsent(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn mark_not_submitted(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn unsent_waiting(&self) -> bool {
            false
        }
        fn sighting(&self) -> Sighting {
            Sighting::default()
        }
        fn watch_screen(&self, _on: bool) {
            unreachable!("nothing is checked mid-turn")
        }
        fn trust_holds_writes(&self) -> bool {
            false
        }
    }

    /// The id a message is written under outlives the daemon run in an agent's own queue, so a
    /// later run's tickets must start where no earlier run reached: from the clock, in
    /// microseconds since the Unix epoch.
    #[test]
    fn an_outbox_seeded_from_the_clock_numbers_its_tickets_from_the_time_it_was_made() {
        let micros = || {
            u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("a clock after the epoch")
                    .as_micros(),
            )
            .unwrap()
        };
        let before = micros();
        let outbox = Outbox::seeded_from_clock();
        let first = outbox.push("s", "one").expect("accepted");
        let second = outbox.push("s", "two").expect("accepted");
        let after = micros();
        assert!(
            (before..=after).contains(&first),
            "{first} is not between {before} and {after}"
        );
        assert_eq!(second, first + 1, "tickets still follow one another");
    }

    /// A message the agent's own queue refused outright fails alone: it is not written again, its
    /// sender can be told why, and the queue behind it goes on.
    #[test]
    fn a_refused_message_fails_alone_and_the_queue_goes_on() {
        let outbox = Outbox::default();
        let refused = outbox.push("s", "refused").expect("accepted");
        let next = outbox.push("s", "next").expect("accepted");
        let mut attempts = Vec::new();
        let outcome = outbox.drain_with(
            "s",
            &mut |ticket, text, _| {
                attempts.push(ticket);
                if text == "refused" {
                    Err(PartialWrite {
                        written: 0,
                        error: std::io::Error::other(crate::session::term::RefusedByAgent(
                            "input too long".into(),
                        )),
                    })
                } else {
                    Ok(Route::Queued)
                }
            },
            &MidTurn,
        );
        assert_eq!(outcome, Drain::Clear);
        assert_eq!(attempts, [refused, next]);
        // Another flush of the session, one that lost the drain claim (a hook's, the server's
        // readiness), announces it before its sender looks: the sender still finds it, by its
        // ticket, and so is never told the message was delivered.
        let announced = outbox.refusals_to_announce("s");
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].0, refused);
        assert!(outbox.refusals_to_announce("s").is_empty());
        assert!(outbox.refusals_to_announce("other").is_empty());
        assert!(outbox
            .take_refusal(refused)
            .is_some_and(|reason| reason.contains("input too long")));
        assert_eq!(outbox.take_refusal(refused), None);
        assert_eq!(outbox.take_refusal(next), None);
    }

    /// An announced refusal is kept for its sender while a drain of its session runs, however
    /// long that drain goes on, and dropped once it has been left that long with none running.
    #[test]
    fn a_refusal_outlasts_the_drain_its_sender_waits_on() {
        for draining in [true, false] {
            let outbox = Outbox::default();
            let long_ago = Instant::now()
                .checked_sub(REFUSAL_KEPT + Duration::from_secs(1))
                .unwrap();
            outbox.refused.lock().unwrap().insert(
                7,
                Refusal {
                    session: "s".into(),
                    reason: "input too long".into(),
                    announced: Some(long_ago),
                },
            );
            let claim = draining.then(|| outbox.claim("s").unwrap());
            drop(claim);
            assert!(outbox.refusals_to_announce("other").is_empty());
            assert_eq!(outbox.take_refusal(7).is_some(), draining);
        }
    }

    /// [`MidTurn`] for a session whose writes are held for its account's usage limit, or not.
    struct UsageLimited(Cell<bool>);

    impl PromptSignal for UsageLimited {
        fn sent(&self) -> u64 {
            0
        }
        fn expects_send(&self) -> bool {
            false
        }
        fn wait_past(&self, _before: u64) -> bool {
            unreachable!("nothing is waited on mid-turn")
        }
        fn mark_unsent(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn mark_not_submitted(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn unsent_waiting(&self) -> bool {
            false
        }
        fn sighting(&self) -> Sighting {
            Sighting::default()
        }
        fn watch_screen(&self, _on: bool) {
            unreachable!("nothing is checked mid-turn")
        }
        fn trust_holds_writes(&self) -> bool {
            false
        }
        fn usage_limit_held(&self) -> bool {
            self.0.get()
        }
    }

    /// While a usage limit holds the session's writes, only the user's messages go, in their
    /// order, past the ones held; once it lets go the held ones follow in theirs.
    #[test]
    fn a_usage_limit_hold_lets_only_the_user_s_messages_past() {
        let outbox = Outbox::default();
        let held = UsageLimited(Cell::new(true));
        outbox.push("s", "first");
        outbox.push_past_usage_limit("s", "mine");
        outbox.push("s", "second");
        outbox.push_past_usage_limit("s", "mine too");
        let mut written = Vec::new();
        let mut write = |_, text: &str, _| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &held),
            Drain::HeldForUsageLimit
        );
        assert_eq!(outbox.len("s"), 2);
        assert!(outbox.past_usage_limit.lock().unwrap().is_empty());
        assert_eq!(
            outbox.drain_with("s", &mut write, &held),
            Drain::HeldForUsageLimit
        );
        held.0.set(false);
        assert_eq!(outbox.drain_with("s", &mut write, &held), Drain::Clear);
        // Nothing held and nothing of the user's: an empty queue is clear, as ever.
        held.0.set(true);
        assert_eq!(outbox.drain_with("s", &mut write, &held), Drain::Clear);
        assert_eq!(written, ["mine", "mine too", "first", "second"]);
    }

    /// What a hold of the server noted of a message's grace goes once the message leaves the
    /// queue.
    #[test]
    fn a_grace_restart_is_forgotten_once_the_message_is_written() {
        let outbox = Outbox::default();
        outbox.push("s", "message");
        let mut held = true;
        let mut write = |_, _: &str, _| {
            if std::mem::take(&mut held) {
                Err(PartialWrite {
                    written: 0,
                    error: std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        crate::session::term::HeldByServer("not yet".into()),
                    ),
                })
            } else {
                Ok(Route::Queued)
            }
        };
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
        assert_eq!(outbox.grace_restarted.lock().unwrap().len(), 1);
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Clear);
        assert!(outbox.grace_restarted.lock().unwrap().is_empty());
    }

    /// A hold of the session's own server starts a message's draft grace again only once, so a
    /// session flipping between that hold and another cannot keep the message waiting past it.
    #[test]
    fn a_hold_of_the_server_starts_the_grace_again_once() {
        let outbox = Outbox::default();
        outbox.push("s", "message");
        let held = || {
            Err(PartialWrite {
                written: 0,
                error: std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    crate::session::term::HeldByServer("not yet".into()),
                ),
            })
        };
        let front = |outbox: &Outbox| outbox.queues.lock().unwrap()["s"].front().unwrap().2;
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| held(), &MidTurn),
            Drain::Pending
        );
        assert_eq!(front(&outbox), None);
        // The next attempt starts its grace; a second hold keeps it.
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| held(), &MidTurn),
            Drain::Pending
        );
        assert!(front(&outbox).is_some());
    }

    /// A message the agent's own server holds back has not waited for a draft: its grace starts
    /// afresh, so a terminal that takes it later still waits for the user's draft. Other holds of
    /// nothing written keep the grace they began.
    #[test]
    fn a_message_held_by_the_server_keeps_its_whole_draft_grace() {
        for (held_by_server, keeps) in [(true, false), (false, true)] {
            let outbox = Outbox::default();
            outbox.push("s", "message");
            let outcome = outbox.drain_with(
                "s",
                &mut |_, _, _| {
                    Err(PartialWrite {
                        written: 0,
                        error: if held_by_server {
                            std::io::Error::new(
                                std::io::ErrorKind::WouldBlock,
                                crate::session::term::HeldByServer("not yet".into()),
                            )
                        } else {
                            std::io::Error::new(std::io::ErrorKind::WouldBlock, "composer busy")
                        },
                    })
                },
                &MidTurn,
            );
            assert_eq!(outcome, Drain::Pending);
            let queues = outbox.queues.lock().unwrap();
            let (_, _, protection_started) = queues["s"].front().unwrap();
            assert_eq!(protection_started.is_some(), keeps);
        }
    }

    /// [`MidTurn`] with the session's input held, as for a draft being stashed or a fragment.
    struct HeldInput;

    impl PromptSignal for HeldInput {
        fn sent(&self) -> u64 {
            0
        }
        fn expects_send(&self) -> bool {
            false
        }
        fn wait_past(&self, _before: u64) -> bool {
            unreachable!("nothing is written while the input is held")
        }
        fn mark_unsent(&self, _before: u64) {
            unreachable!("nothing is written while the input is held")
        }
        fn mark_not_submitted(&self, _before: u64) {
            unreachable!("nothing is written while the input is held")
        }
        fn unsent_waiting(&self) -> bool {
            false
        }
        fn sighting(&self) -> Sighting {
            Sighting::default()
        }
        fn watch_screen(&self, _on: bool) {
            unreachable!("nothing is checked while the input is held")
        }
        fn trust_holds_writes(&self) -> bool {
            false
        }
        fn input_held(&self) -> bool {
            true
        }
    }

    /// An agent at its prompt that sends a written message, or none while `sends` is off, and is
    /// busy with a turn once anything is sent. [`Self::prompt_sent`] is any prompt reported sent,
    /// the agent's own or one from elsewhere, such as the user pressing Enter.
    struct AtPrompt {
        sent: Cell<u64>,
        sends: Cell<bool>,
        waits: Cell<u32>,
        unsent: Cell<Option<u64>>,
        /// The message marked unsent was left in the box, its submit key pressed or held back.
        not_submitted: Cell<bool>,
        busy: Cell<bool>,
        dialog: Cell<Option<Dialog>>,
        /// What the screen has shown so far.
        sighting: Cell<Sighting>,
        /// A dialog that comes up while the next message is waited on, and whether it is gone
        /// again by the time the wait ends.
        dialog_on_wait: Cell<Option<(Dialog, bool)>>,
        /// Screen watches in force, and the most that ever were.
        watching: Cell<i32>,
        watched_during_wait: Cell<bool>,
        trust_holds: Cell<bool>,
        recovering_input: Cell<bool>,
    }

    impl AtPrompt {
        fn new(sends: bool) -> Self {
            Self {
                sent: Cell::new(0),
                sends: Cell::new(sends),
                waits: Cell::new(0),
                unsent: Cell::new(None),
                not_submitted: Cell::new(false),
                busy: Cell::new(false),
                dialog: Cell::new(None),
                sighting: Cell::new(Sighting::default()),
                dialog_on_wait: Cell::new(None),
                watching: Cell::new(0),
                watched_during_wait: Cell::new(false),
                trust_holds: Cell::new(false),
                recovering_input: Cell::new(false),
            }
        }

        /// The screen now shows `dialog`, or none.
        fn show(&self, dialog: Option<Dialog>) {
            self.dialog.set(dialog);
            self.sighting.set(self.sighting.get().after(dialog));
        }

        fn prompt_sent(&self) {
            self.sent.set(self.sent.get() + 1);
            self.busy.set(true);
        }
    }

    impl PromptSignal for AtPrompt {
        fn sent(&self) -> u64 {
            self.sent.get()
        }

        fn input_recovery_pending(&self) -> bool {
            self.recovering_input.get()
        }
        fn expects_send(&self) -> bool {
            !self.busy.get()
        }
        fn wait_past(&self, before: u64) -> bool {
            self.waits.set(self.waits.get() + 1);
            self.watched_during_wait.set(self.watching.get() > 0);
            if let Some((dialog, gone_again)) = self.dialog_on_wait.take() {
                self.show(Some(dialog));
                if gone_again {
                    self.show(None);
                }
            }
            if self.sends.get() {
                self.prompt_sent();
            }
            self.sent.get() > before
        }
        fn mark_unsent(&self, before: u64) {
            self.unsent.set(Some(before));
        }
        fn mark_not_submitted(&self, before: u64) {
            self.unsent.set(Some(before));
            self.not_submitted.set(true);
        }
        fn unsent_waiting(&self) -> bool {
            self.unsent
                .get()
                .is_some_and(|before| self.sent.get() <= before)
        }
        fn sighting(&self) -> Sighting {
            self.sighting.get()
        }
        fn watch_screen(&self, on: bool) {
            self.watching
                .set(self.watching.get() + if on { 1 } else { -1 });
        }
        fn trust_holds_writes(&self) -> bool {
            self.trust_holds.get()
        }
    }

    fn dialog(written: WrittenMessage) -> Dialog {
        Dialog {
            code: "test_dialog",
            written,
            kind: crate::session::dialogs::DialogKind::Question,
        }
    }

    fn partial(written: usize) -> PartialWrite {
        PartialWrite {
            written,
            error: std::io::Error::new(std::io::ErrorKind::TimedOut, "no room"),
        }
    }

    #[test]
    fn a_reserved_first_message_preserves_ticket_order_and_cancel_keeps_later_messages() {
        let outbox = Outbox::default();
        outbox.pause("s");
        let first = outbox.reserve_first("s").unwrap();
        assert!(
            outbox.reserve_first("s").is_err(),
            "the first place is already reserved"
        );
        let report = outbox.push("s", "child report").unwrap();
        let bootstrap = outbox.push_first(first, "bootstrap").unwrap();
        assert!(bootstrap < report);
        outbox.resume("s");
        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |ticket, text, _| {
                    written.push((ticket, text.to_string()));
                    Ok(Route::Terminal)
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(
            written,
            [
                (bootstrap, "bootstrap".to_string()),
                (report, "child report".to_string())
            ]
        );
        outbox.pause("cancelled");
        let cancelled = outbox.reserve_first("cancelled").unwrap();
        outbox.push("cancelled", "external input");
        outbox.cancel_first(cancelled);
        outbox.resume("cancelled");
        assert_eq!(
            outbox.take("cancelled"),
            [("external input".to_string(), None)]
        );
    }

    #[test]
    fn everything_queued_is_written_in_order() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(Route::Terminal)
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(written, vec!["first", "second"]);
        assert_eq!(outbox.len("s"), 0);
    }

    /// A paused queue is written only once every pause on it has ended, so two that overlap do not
    /// let it go early.
    #[test]
    fn a_paused_queue_waits_for_the_last_pause_to_end() {
        let outbox = Outbox::default();
        outbox.push("s", "held");
        outbox.pause("s");
        outbox.pause("s");
        let mut written = Vec::new();
        let mut drain = |outbox: &Outbox| {
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(Route::Terminal)
                },
                &MidTurn,
            )
        };
        assert_eq!(drain(&outbox), Drain::Pending);
        outbox.resume("s");
        assert_eq!(drain(&outbox), Drain::Pending);
        outbox.resume("s");
        assert_eq!(drain(&outbox), Drain::Clear);
        assert_eq!(written, vec!["held"]);
    }

    /// A message queued while a drain is already under way is written by that drain rather than left
    /// for a later one. This does not reach the outer re-claim loop — landing between the final pop
    /// and the release needs a second thread, and the argument for that window is the ordering noted
    /// in `drain_with` rather than something a test can arrange.
    #[test]
    fn a_message_queued_during_a_drain_is_written_by_it() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let mut written = Vec::new();
        let mut pushed_late = false;
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    if !pushed_late {
                        pushed_late = true;
                        // Stands in for a producer arriving as this pass finishes: by the time the
                        // drainer re-reads the queue the message is in it.
                        outbox.push("s", "late arrival");
                    }
                    Ok(Route::Terminal)
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(written, vec!["first", "late arrival"]);
    }

    /// A message kept in the store is queued once per row however often it is pushed, its row is
    /// handed on once it is written, a queue that is forgotten or taken leaves the row alone, and a
    /// queue reported lost counts as done.
    #[test]
    fn a_kept_message_is_queued_once_and_its_row_goes_when_it_is_written_or_lost() {
        let outbox = Outbox::default();
        assert!(outbox.push_kept("s", "kept", 7).is_some());
        assert!(outbox.push_kept("s", "kept", 7).is_none());
        assert_eq!(outbox.len("s"), 1);
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn),
            Drain::Clear
        );
        assert_eq!(outbox.take_written(), [7]);
        // Written, the row may be queued again by a later resume that still finds it.
        assert!(outbox.push_kept("s", "kept", 7).is_some());

        // Taken, it reports its row and is no longer queued.
        let taken = outbox.take("s");
        assert_eq!(taken, [("kept".to_string(), Some(7))]);
        assert!(outbox.push_kept("s", "kept", 7).is_some());
        outbox.forget("s");
        assert!(outbox.take_written().is_empty());

        // Lost: reported to the sender as undelivered, so its row and those behind it go.
        outbox.push_kept("s", "first", 1);
        outbox.push_kept("s", "second", 2);
        outbox.push("s", "plain");
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(3)), &MidTurn),
            Drain::Lost
        );
        let mut rows = outbox.take_written();
        rows.sort();
        assert_eq!(rows, [1, 2]);
    }

    /// [`MidTurn`] once a planned stop has stopped the writes, after the first message written.
    struct StopsAfterFirst(Cell<u32>);

    impl PromptSignal for StopsAfterFirst {
        fn sent(&self) -> u64 {
            0
        }
        fn expects_send(&self) -> bool {
            false
        }
        fn wait_past(&self, _before: u64) -> bool {
            unreachable!("nothing is waited on mid-turn")
        }
        fn mark_unsent(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn mark_not_submitted(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn unsent_waiting(&self) -> bool {
            false
        }
        fn sighting(&self) -> Sighting {
            Sighting::default()
        }
        fn watch_screen(&self, _on: bool) {
            unreachable!("nothing is checked mid-turn")
        }
        fn trust_holds_writes(&self) -> bool {
            false
        }
        fn writes_stopped(&self) -> bool {
            let looked = self.0.get();
            self.0.set(looked + 1);
            looked > 0
        }
    }

    /// A drain under way when a planned stop stops the writes finishes the message it is writing
    /// and begins no other: the rest stays queued, whole and in order, for the next daemon.
    #[test]
    fn a_drain_stops_between_messages_once_the_writes_are_stopped() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        outbox.push("s", "third");
        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(Route::Terminal)
                },
                &StopsAfterFirst(Cell::new(0))
            ),
            Drain::Pending
        );
        assert_eq!(written, vec!["first"]);
        let sealed = outbox.seal(Duration::ZERO);
        assert_eq!(sealed.len(), 1);
        let texts: Vec<&str> = sealed[0]
            .1
            .iter()
            .map(|(_, text, _)| text.as_str())
            .collect();
        assert_eq!(texts, ["second", "third"]);
    }

    /// [`MidTurn`] whose writes stop when the flag is raised, as a planned stop's barrier does.
    struct Barrier(Arc<AtomicBool>);

    impl PromptSignal for Barrier {
        fn sent(&self) -> u64 {
            0
        }
        fn expects_send(&self) -> bool {
            false
        }
        fn wait_past(&self, _before: u64) -> bool {
            unreachable!("nothing is waited on mid-turn")
        }
        fn mark_unsent(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn mark_not_submitted(&self, _before: u64) {
            unreachable!("nothing is checked mid-turn")
        }
        fn unsent_waiting(&self) -> bool {
            false
        }
        fn sighting(&self) -> Sighting {
            Sighting::default()
        }
        fn watch_screen(&self, _on: bool) {
            unreachable!("nothing is checked mid-turn")
        }
        fn trust_holds_writes(&self) -> bool {
            false
        }
        fn writes_stopped(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
    }

    /// A message the agent's own server declines after the handover barrier rose is not written
    /// into the terminal: it goes back to the front of its queue, and the seal takes it with the
    /// rest, in order, for the next daemon.
    #[test]
    fn a_message_declined_after_the_barrier_rose_is_put_back_and_sealed_in_order() {
        let barrier = Arc::new(AtomicBool::new(false));
        let raised = barrier.clone();
        let server = crate::test_support::DecliningServer::new(move || {
            raised.store(true, Ordering::Release)
        });
        let live = crate::test_support::idle_stand_in_of("declined-after-barrier", AgentId::CODEX)
            .with_transport(server.clone());
        let outbox = Outbox::default();
        let first = outbox.push_kept(&live.id, "first", 7).unwrap();
        outbox.push(&live.id, "second");
        let before = live.input_writes();

        assert_eq!(
            outbox.drain(&live, &Barrier(barrier.clone())),
            Drain::Pending
        );
        assert_eq!(live.input_writes(), before, "something was written");
        assert_eq!(server.offered().len(), 1, "only the first was offered");
        assert_eq!(outbox.len(&live.id), 2);

        let sealed = outbox.seal(Duration::ZERO);
        assert_eq!(sealed.len(), 1);
        let texts: Vec<&str> = sealed[0]
            .1
            .iter()
            .map(|(_, text, _)| text.as_str())
            .collect();
        assert_eq!(texts, ["first", "second"]);
        assert_eq!(sealed[0].1[0].0, first);
        assert_eq!(sealed[0].1[0].2, Some(7), "the kept row went with it");
    }

    /// Before the barrier, a message the server declines is still written into the terminal and the
    /// queue goes on. The stand-in is of an agent whose paste and Enter go in together on every
    /// host, so the write waits on no input box the stand-in never draws.
    #[test]
    fn a_message_declined_before_the_barrier_is_written_into_the_terminal() {
        let barrier = Arc::new(AtomicBool::new(false));
        let server = crate::test_support::DecliningServer::new(|| {});
        let live = crate::test_support::idle_stand_in("declined-before-barrier")
            .with_transport(server.clone());
        let outbox = Outbox::default();
        outbox.push(&live.id, "first");
        let before = live.input_writes();

        assert_eq!(outbox.drain(&live, &Barrier(barrier)), Drain::Clear);
        assert_eq!(live.input_writes(), before + 1);
        assert_eq!(server.offered().len(), 1);
        assert_eq!(outbox.len(&live.id), 0);
    }

    /// Sealing takes every queue whole, kept rows with them, and leaves the rows unqueued for the
    /// next daemon to keep; from then on the outbox takes no message by the sealed push.
    #[test]
    fn sealing_takes_every_queue_and_refuses_what_comes_after() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first").unwrap();
        outbox.push_kept("s", "kept", 7);
        outbox.push("u", "other");
        assert!(!outbox.is_sealed());
        let mut sealed = outbox.seal(Duration::ZERO);
        sealed.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(sealed[0].0, "s");
        assert_eq!(sealed[0].1[0], (first, "first".to_string(), None));
        assert_eq!(sealed[0].1[1].1, "kept");
        assert_eq!(sealed[0].1[1].2, Some(7));
        assert_eq!(sealed[1].0, "u");
        assert!(outbox.is_sealed());
        assert!(!outbox.is_row_queued(7));
        assert_eq!(outbox.len("s"), 0);
        assert_eq!(outbox.push_unless_sealed("s", "late"), None);
        assert_eq!(outbox.push_kept("s", "kept late", 8), None);
        // Nor by the plain push, once the queues are taken: nothing would ever write it.
        assert_eq!(outbox.push("s", "after"), None);
        assert!(!outbox.is_row_queued(8));
        assert_eq!(outbox.len("s"), 0);
    }

    /// A message whose write the handover refused while the queues were being sealed is put back
    /// and taken with the rest, since the seal waits for the drain still writing.
    #[test]
    fn a_message_put_back_while_the_seal_waits_is_taken_with_the_rest() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        // Borrowed, so that the drain's thread takes the borrow and the channel's sender with it.
        let outbox = &outbox;
        let (writing, being_written) = std::sync::mpsc::channel();
        let sealed = std::thread::scope(|scope| {
            let draining = scope.spawn(move || {
                outbox.drain_with(
                    "s",
                    &mut |_, _, _| {
                        // The message is off the queue and being written: the seal starts now,
                        // and the write goes on until it has, so the seal finds it still writing.
                        writing.send(()).ok();
                        crate::session::wait::wait_for(crate::test_support::PATIENCE, || {
                            outbox.is_sealed()
                        });
                        Err(partial(0))
                    },
                    &MidTurn,
                )
            });
            being_written
                .recv_timeout(crate::test_support::PATIENCE)
                .expect("the drain never began its write");
            let sealed = outbox.seal(crate::test_support::PATIENCE);
            assert_eq!(draining.join().unwrap(), Drain::Pending);
            sealed
        });
        let texts: Vec<&str> = sealed[0]
            .1
            .iter()
            .map(|(_, text, _)| text.as_str())
            .collect();
        assert_eq!(texts, ["first", "second"]);
    }

    /// Nothing went in, so the session could not take it: it stays at the front, and the one behind
    /// it must not be let past.
    #[test]
    fn a_message_that_could_not_be_written_stays_at_the_front() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(0)), &MidTurn),
            Drain::Pending
        );
        assert_eq!(outbox.len("s"), 2);

        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(Route::Terminal)
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(written, vec!["first", "second"]);
    }

    /// A half-written message leaves an unterminated paste in the session's input line, so retrying
    /// it would deliver the fragment twice and writing the next one would append it inside the
    /// fragment. Both it and the queue behind it go, and the loss is reported.
    #[test]
    fn a_half_written_message_takes_the_queue_with_it_and_is_reported() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(3)), &MidTurn),
            Drain::Lost
        );
        assert_eq!(outbox.len("s"), 0);
    }

    /// A message written in part while the draft was stashed leaves a session that holds every
    /// later write until a prompt is sent after it, so only that message is lost, its row with it:
    /// the queue behind it waits, rows kept, refused meanwhile, and is written in order once the
    /// session takes writes again.
    #[test]
    fn a_held_fragment_loses_only_its_own_message() {
        let outbox = Outbox::default();
        let first = outbox.push_kept("s", "first", 1).unwrap();
        outbox.push_kept("s", "second", 2);
        outbox.push("s", "third");
        let held = || PartialWrite {
            written: 3,
            error: std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                crate::session::HeldFragment("no room".to_string()),
            ),
        };
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(held()), &MidTurn),
            Drain::Fragmented(first)
        );
        assert_eq!(outbox.len("s"), 2);
        assert_eq!(outbox.take_written(), [1]);
        assert!(outbox.is_row_queued(2));
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(0)), &MidTurn),
            Drain::Pending
        );
        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(Route::Terminal)
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(written, vec!["second", "third"]);
        assert_eq!(outbox.take_written(), [2]);
    }

    /// A message refused while the session's input is held keeps none of its draft grace, so once
    /// the hold ends it gets the whole grace again rather than joining what is in the box.
    #[test]
    fn a_hold_on_the_input_spends_none_of_a_message_s_draft_grace() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let (ticket, text, _) = outbox.pop("s").unwrap();
        outbox.push_front("s", ticket, text, Some(Instant::now() - DRAFT_WAIT));
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Err(partial(0)), &HeldInput),
            Drain::Pending
        );
        let mut mixed = None;
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, _, allow_mixed| {
                    mixed = Some(allow_mixed);
                    Ok(Route::Terminal)
                },
                &MidTurn
            ),
            Drain::Clear
        );
        assert_eq!(mixed, Some(false));
    }

    #[test]
    fn only_one_drainer_at_a_time_owns_a_session() {
        let outbox = Outbox::default();
        let claim = outbox.claim("s");
        assert!(claim.is_some());
        assert!(outbox.claim("s").is_none());
        drop(claim);
        assert!(outbox.claim("s").is_some());
    }

    #[test]
    fn a_drain_whose_write_panics_leaves_the_session_drainable() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            outbox.drain_with("s", &mut |_, _, _| panic!("the write failed"), &MidTurn)
        }));
        assert!(panicked.is_err());
        assert!(outbox.claim("s").is_some());
    }

    #[test]
    fn a_drain_a_concurrent_drainer_already_owns_reports_pending() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let _claim = outbox.claim("s").unwrap();
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn),
            Drain::Pending
        );
        assert_eq!(outbox.len("s"), 1);
    }

    #[test]
    fn forgetting_a_session_reports_what_it_threw_away() {
        let outbox = Outbox::default();
        outbox.push("s", "one");
        outbox.push("s", "two");
        assert_eq!(outbox.forget("s"), 2);
        assert_eq!(outbox.len("s"), 0);
        assert_eq!(outbox.forget("s"), 0);
    }

    /// A message an idle agent sends goes through, and what follows it in the same pass is not
    /// waited on: the agent is busy with the first and queues the rest itself.
    #[test]
    fn a_message_the_agent_sends_is_written_and_the_rest_follow_unchecked() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        outbox.push("s", "second");
        let agent = AtPrompt::new(true);
        let mut written = Vec::new();
        let outcome = outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &agent,
        );
        assert_eq!(outcome, Drain::Clear);
        assert_eq!(written, vec!["first", "second"]);
        assert_eq!(agent.waits.get(), 1);
    }

    /// A message the agent does not send is reported by its ticket and not put back, and nothing
    /// more is written — not even a message queued afterwards — until the agent next reports a
    /// prompt sent.
    #[test]
    fn a_message_the_agent_does_not_send_holds_the_queue_until_the_next_prompt() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first").unwrap();
        outbox.push("s", "second");
        let agent = AtPrompt::new(false);
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &agent),
            Drain::Unsent(first)
        );
        outbox.push("s", "third");
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        assert_eq!(outbox.len("s"), 2);

        agent.prompt_sent();
        agent.sends.set(true);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first", "second", "third"]);
    }

    /// A message the write itself left in the input box, its submit key pressed or held back, is
    /// unsent, for an agent whose prompts the outbox does not wait on as for one it does: not lost,
    /// and the queue behind it held until a prompt is sent.
    #[test]
    fn a_message_left_in_the_box_by_its_submit_key_is_unsent_and_holds_the_queue() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first").unwrap();
        outbox.push("s", "second");
        let agent = AtPrompt::new(false);
        agent.busy.set(true);
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            if text == "first" {
                return Err(PartialWrite {
                    written: 20,
                    error: std::io::Error::other(crate::session::NotSubmitted),
                });
            }
            Ok(Route::Terminal)
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &agent),
            Drain::Unsent(first)
        );
        assert!(agent.unsent_waiting());
        assert!(
            agent.not_submitted.get(),
            "a message left in its input box by the write is marked so, for no retry to press it"
        );
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        assert_eq!(outbox.len("s"), 1);

        agent.prompt_sent();
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first", "second"]);
    }

    /// Nothing is written while a dialog is up, and the queue goes on once it is gone.
    #[test]
    fn nothing_is_written_while_a_dialog_is_up() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let agent = AtPrompt::new(true);
        agent.show(Some(dialog(WrittenMessage::Draft)));
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        };
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        assert_eq!(outbox.len("s"), 1);

        agent.show(None);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first"]);
    }

    /// A trust screen sighted while a drain runs stops it at the next message.
    #[test]
    fn nothing_is_written_while_the_trust_screen_holds_writes() {
        let outbox = Outbox::default();
        outbox.push("s", "first");
        let agent = AtPrompt::new(true);
        agent.trust_holds.set(true);
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        };
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        assert_eq!(outbox.len("s"), 1);
        assert!(written.is_empty());
    }

    /// A dialog that came up around a write threw the message away: it goes back at the front
    /// under its ticket, the agent is not marked as holding an unsent message, and the message
    /// behind it is not let past. The dialog is gone again by the end of the wait, as one a write
    /// closes is, and the screen was being watched throughout.
    #[test]
    fn a_message_a_dialog_threw_away_goes_back_at_the_front() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first").unwrap();
        outbox.push("s", "second");
        let agent = AtPrompt::new(false);
        agent
            .dialog_on_wait
            .set(Some((dialog(WrittenMessage::Discarded), true)));
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &agent),
            Drain::Discarded(first)
        );
        assert!(agent.watched_during_wait.get());
        assert_eq!(agent.watching.get(), 0);
        assert_eq!(outbox.len("s"), 2);
        assert!(!agent.unsent_waiting());

        agent.sends.set(true);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first", "first", "second"]);
    }

    /// A view that took the message has delivered it, to the wrong place: it is not put back, no
    /// unsent mark holds the queue, and the message behind it waits for the view to be left.
    #[test]
    fn a_message_a_view_took_is_not_put_back() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first").unwrap();
        outbox.push("s", "second");
        let agent = AtPrompt::new(false);
        agent
            .dialog_on_wait
            .set(Some((dialog(WrittenMessage::Diverted), false)));
        let mut written = Vec::new();
        let mut write = |_: Ticket, text: &str, _: bool| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        };
        assert_eq!(
            outbox.drain_with("s", &mut write, &agent),
            Drain::Diverted(first)
        );
        assert_eq!(outbox.len("s"), 1);
        assert!(!agent.unsent_waiting());

        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
        agent.show(None);
        agent.sends.set(true);
        assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
        assert_eq!(written, vec!["first", "second"]);
    }

    /// A draft a dialog kept (the idle resume dialog) is in the input box, so it counts as unsent.
    #[test]
    fn a_message_left_as_a_draft_by_a_dialog_is_unsent() {
        let outbox = Outbox::default();
        let first = outbox.push("s", "first").unwrap();
        let agent = AtPrompt::new(false);
        agent
            .dialog_on_wait
            .set(Some((dialog(WrittenMessage::Draft), false)));
        assert_eq!(
            outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &agent),
            Drain::Unsent(first)
        );
        assert_eq!(outbox.len("s"), 0);
    }
    #[test]
    fn a_written_message_with_pending_draft_recovery_is_not_retried_or_marked_unsent() {
        let outbox = Outbox::default();
        outbox.push("s", "already written");
        outbox.push("s", "still queued");
        let agent = AtPrompt::new(false);
        agent.recovering_input.set(true);
        let mut written = Vec::new();
        assert_eq!(
            outbox.drain_with(
                "s",
                &mut |_, text, _| {
                    written.push(text.to_string());
                    Ok(Route::Terminal)
                },
                &agent
            ),
            Drain::Pending
        );
        assert_eq!(written, ["already written"]);
        assert_eq!(outbox.len("s"), 1);
        assert!(!agent.unsent_waiting());
        assert_eq!(agent.waits.get(), 0);
    }

    /// An agent whose own queue left a message's acceptance unconfirmed: it holds later messages
    /// for [`DRAFT_WAIT`] after the mark, or until a prompt is reported, as a session does.
    #[derive(Default)]
    struct UncertainQueue {
        sent: Cell<u64>,
        mark: Cell<Option<(u64, Instant)>>,
        marked_unsent: Cell<bool>,
    }

    impl PromptSignal for UncertainQueue {
        fn sent(&self) -> u64 {
            self.sent.get()
        }
        fn expects_send(&self) -> bool {
            false
        }
        fn wait_past(&self, _before: u64) -> bool {
            unreachable!("nothing is waited on")
        }
        fn mark_unsent(&self, _before: u64) {
            self.marked_unsent.set(true);
        }
        fn mark_not_submitted(&self, _before: u64) {
            self.marked_unsent.set(true);
        }
        fn unsent_waiting(&self) -> bool {
            false
        }
        fn mark_submission_uncertain(&self, before: u64) {
            self.mark.set(Some((before, Instant::now())));
        }
        fn submission_uncertain(&self) -> bool {
            self.mark
                .get()
                .is_some_and(|(before, at)| self.sent.get() <= before && at.elapsed() < DRAFT_WAIT)
        }
        fn sighting(&self) -> Sighting {
            Sighting::default()
        }
        fn watch_screen(&self, _on: bool) {}
        fn trust_holds_writes(&self) -> bool {
            false
        }
    }

    fn uncertain(written: usize) -> PartialWrite {
        PartialWrite {
            written,
            error: std::io::Error::other(crate::session::term::UncertainSubmission(
                "connection closed after add".into(),
            )),
        }
    }

    /// A message whose acceptance is uncertain is never written again, by the queue or the
    /// terminal (no unsent mark, which would retry its submit key), even where its error says
    /// nothing went in; later messages wait until a prompt is reported or the hold runs out.
    #[test]
    fn uncertain_native_acceptance_preserves_later_messages_without_resending() {
        for written in [1, 0] {
            for released_by_prompt in [true, false] {
                let outbox = Outbox::default();
                let first = outbox.push("s", "first").expect("accepted");
                outbox.push("s", "second");
                let agent = UncertainQueue::default();
                let mut attempts = Vec::new();
                let outcome = outbox.drain_with(
                    "s",
                    &mut |ticket, text, _| {
                        attempts.push((ticket, text.to_string()));
                        Err(uncertain(written))
                    },
                    &agent,
                );
                assert_eq!(outcome, Drain::Uncertain(first));
                assert_eq!(attempts, [(first, "first".to_string())]);
                assert_eq!(outbox.len("s"), 1);
                assert!(!agent.marked_unsent.get());
                assert_eq!(
                    outbox.drain_with("s", &mut |_, _, _| panic!("held"), &agent),
                    Drain::Pending
                );
                if released_by_prompt {
                    agent.sent.set(agent.sent.get() + 1);
                } else {
                    let (before, _) = agent.mark.get().unwrap();
                    agent.mark.set(Some((before, Instant::now() - DRAFT_WAIT)));
                }
                assert_eq!(
                    outbox.drain_with(
                        "s",
                        &mut |ticket, text, _| {
                            attempts.push((ticket, text.to_string()));
                            Ok(Route::Queued)
                        },
                        &agent
                    ),
                    Drain::Clear
                );
                assert_eq!(attempts[1].1, "second");
            }
        }
    }

    #[test]
    fn an_unsubmitted_draft_does_not_prevent_delivery_after_the_grace_period() {
        let outbox = Outbox::default();
        let live = crate::test_support::fake_live_session(
            "bounded-draft",
            crate::protocol::AgentId::CLAUDE,
            120,
            40,
            "sleep 30",
        )
        .with_composer();
        live.on_output(include_bytes!("../../testdata/claude_input_restored.bin"));
        live.write_terminal_input(b"unfinished user input").unwrap();
        outbox.push(&live.id, "automatic message");
        let before = live.input_writes();
        assert_eq!(outbox.drain(&live, &MidTurn), Drain::Pending);
        assert_eq!(live.input_writes(), before);
        let queued = outbox.pop(&live.id).unwrap();
        outbox.push_front(
            &live.id,
            queued.0,
            queued.1,
            Some(Instant::now() - DRAFT_WAIT),
        );
        assert_eq!(outbox.drain(&live, &MidTurn), Drain::Clear);
        assert_eq!(live.input_writes(), before + 1);
        assert_eq!(outbox.len(&live.id), 0);
    }

    #[test]
    fn draft_wait_expires_for_the_same_ticket_across_retries() {
        let outbox = Outbox::default();
        let ticket = outbox.push("s", "automatic").unwrap();
        let mut allowances = Vec::new();
        let mut write = |seen, _: &str, allow_mixed| {
            assert_eq!(seen, ticket);
            allowances.push(allow_mixed);
            Err(PartialWrite {
                written: 0,
                error: std::io::ErrorKind::WouldBlock.into(),
            })
        };
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
        let queued = outbox.pop("s").unwrap();
        assert!(queued.2.is_some());
        outbox.push_front("s", queued.0, queued.1, Some(Instant::now() - DRAFT_WAIT));
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
        assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
        assert_eq!(allowances, [false, true, true]);
    }
}
