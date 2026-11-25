//! The per-session FIFO for automatic messages, with one drainer per session.
//!
//! Each ticket keeps its first delivery-attempt time across retries, except one refused while the
//! session's input is held for a draft or a fragment, whose grace starts again once the hold ends,
//! and one the session's own server held back, whose grace starts again once, at most.
//! Draft protection waits for the user's edits only for a bounded grace period; afterwards a draft
//! no stash can take is cleared before the message is written, and a message goes in together with
//! text in the input box only once that text has resisted clearing for a bounded wait
//! (`crate::session::LiveSession::write_message_input`).
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
//! The latest messages written into each session are kept beside the queue
//! ([`Outbox::redeliver`]): where the agent held one for the end of a turn, gave it back at a
//! cancel and it was cleared from the input box unsent (`crate::session::draft_clear`), or
//! dropped it at the cancel, it is put back at the front of the queue,
//! to be written again right after the message whose write cleared it,
//! [`redelivery::MAX_REDELIVERIES`] times at most. Where an interrupt of Octoboard's own cleared it, no message is being written: it is put
//! back held ([`Outbox::redeliver_held`]), a queued message no drain takes until another message
//! is taken for the session, which it then follows, so the interrupt still leaves the agent
//! stopped. A held message is a queued one in every other respect: it is saved, carried over and
//! forgotten with the queue, and written as any other once it has been carried to another process
//! or daemon.
//!
//! For the delivery contract see docs/product/message-delivery.md's
//! "Messages held until a session can take them" section, and for a fragment written while the
//! draft was stashed docs/product/draft-preservation.md's "A message only partly written while the
//! draft is stashed" section.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::platform::term::PartialWrite;
use crate::session::dialogs::Sighting;
use crate::session::term::{self, Route, Shape};
use crate::session::LiveSession;

mod redelivery;

pub use redelivery::Returned;
#[cfg(test)]
pub(crate) use redelivery::MAX_REDELIVERIES;
use redelivery::{Latest, ThroughTransport};

/// How long draft preservation waits for the user's edits to be confirmed before an automatic
/// message no longer waits on them, and a draft no stash can take is cleared out of its way.
pub(crate) const DRAFT_WAIT: Duration = Duration::from_secs(3);

/// Which end of a session's queue a message goes to.
#[derive(Clone, Copy)]
enum End {
    Front,
    Back,
}

/// What a drain amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drain {
    /// Nothing is left for a drain to take for this session: its queue is empty, or holds only
    /// messages held back until another is taken ([`Outbox::redeliver_held`]).
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
    /// writes until a prompt is sent after it, or each message's bounded wait runs out
    /// (`crate::session::LiveSession::write_message_input`): the queue behind it stays, and is
    /// written in order.
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
    /// The message with this ticket was written, the agent did not report sending it, and its
    /// input box then read empty: the message went somewhere other than the box or the
    /// conversation (a dialog the screen reader did not recognise, say), and may be lost. It is
    /// not written again, and nothing holds the queue behind it.
    WentElsewhere(Ticket),
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
    /// Whether a write the agent did not report sending went elsewhere: its provider declares that
    /// an empty box then means so, and the box reads empty off a whole frame now. `false` where
    /// the box cannot be read so, and for every provider that declares nothing.
    fn write_went_elsewhere(&self) -> bool {
        false
    }
    /// Tells of the message `ticket`, which read `text`, that it went elsewhere
    /// ([`Self::write_went_elsewhere`]): right before the drain stops on it.
    fn went_elsewhere(&self, _ticket: Ticket, _text: &str) {}
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
    /// The message written as `ticket`, which the session's own server or channel took, was given
    /// back by the agent before its write was over ([`Returned::Writing`]); the queue holds it as
    /// taken now, so the session has it written again as it does one given back later. Nothing to
    /// do where the session has no server or channel that gives messages back.
    fn given_back_while_written(&self, _ticket: Ticket) {}
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
    /// How the messages that are more than text are written, by ticket (images, a slash command;
    /// see [`Shape`]). Such a message is never carried across processes or daemons as plain text:
    /// [`Self::seal`] and [`Self::take`] drop it, and say so in the log.
    shapes: Mutex<HashMap<Ticket, Shape>>,
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
    /// The latest messages written into each session's terminal, oldest first, for
    /// [`Self::redeliver`]. Forgotten with the session's queue.
    latest: Mutex<HashMap<String, VecDeque<Latest>>>,
    /// The messages each session's own server or channel took, and the one being written now,
    /// for [`Self::redeliver_returned`]. Forgotten with the session's queue. Locked before the
    /// queues.
    through_transport: Mutex<HashMap<String, ThroughTransport>>,
    /// How many times each message queued by [`Self::redeliver`] or [`Self::redeliver_returned`]
    /// has been written again, by ticket.
    redeliveries: Mutex<HashMap<Ticket, u8>>,
    /// The queued messages no drain takes until another message is taken for their session
    /// ([`Self::redeliver_held`]), by ticket. Locked after the queues.
    held_back: Mutex<HashSet<Ticket>>,
    /// The queued messages put back because the agent dropped them at a cancel
    /// ([`Self::redeliver_dropped`]), by ticket: while one is still held back, the agent
    /// reporting its text sent withdraws it ([`Self::withdraw_dropped`]). Locked after the
    /// held-back set.
    dropped: Mutex<HashSet<Ticket>>,
    /// The session that sent each message a session sent, by ticket ([`Self::note_sender`]): kept
    /// past the write, as long as a cancel or a turn's end can still show the message was not
    /// carried out, and bounded by [`SENDERS_KEPT`]. The user's messages and Octoboard's own have
    /// none. Locked on its own, or after the queues (`Self::push_again`).
    senders: Mutex<BTreeMap<Ticket, String>>,
    /// The messages whose writer waits on the drain to answer it ([`Self::await_answer`]), and
    /// the text of each of them a drain met gone elsewhere, for that writer to take
    /// ([`Self::take_answer`]). Locked on its own.
    awaited: Mutex<Awaited>,
    /// Set by [`Self::seal`] first: no new message is accepted, but a drain still writing may put
    /// back the message it could not write, to be taken with the rest. Changed under the queues'
    /// lock.
    sealed: AtomicBool,
    /// Set by [`Self::seal`] once it has taken the queues for the next daemon: nothing put on a
    /// queue from then on is ever written. Changed under the queues' lock.
    taken: AtomicBool,
    /// How many drains have been started, for a test that a caller started none.
    #[cfg(test)]
    drains: std::sync::atomic::AtomicU64,
}

/// A message that failed alone, for its sender (and the session, for the agent's refusals) to be
/// told of.
struct Refusal {
    session: String,
    refused: Refused,
    /// When it was announced, if it has been.
    announced: Option<Instant>,
}

/// Why a message failed alone, and what its sender is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub reason: String,
    pub kind: RefusalKind,
}

/// Who refused a message that failed alone, and so how the session is told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalKind {
    /// The agent's own queue (`term::RefusedByAgent`): a `message_refused` notice.
    Agent,
    /// It could not be written as sent (`term::Unwritable`), not the agent's doing: a
    /// `message_not_written` notice.
    Unwritable,
    /// A slash command, the first, that the agent would have run as the second, which its
    /// completion offered (`term::CommandNotRecognised`): a `command_not_recognised` notice.
    OtherCommand(String, String),
    /// A slash command the agent has none of (`term::CommandNotFound`): a `command_not_found`
    /// notice.
    NoSuchCommand(String),
}

/// The refusal a failed write is, where it fails to its sender alone rather than by the agent's
/// doing: a slash command the agent would not run as written, or a message that could not be
/// written as sent.
fn refusal_kind(error: &(dyn std::error::Error + Send + Sync + 'static)) -> Option<RefusalKind> {
    if let Some(other) = error.downcast_ref::<term::CommandNotRecognised>() {
        return Some(RefusalKind::OtherCommand(
            other.command.clone(),
            other.offered.clone(),
        ));
    }
    if let Some(none) = error.downcast_ref::<term::CommandNotFound>() {
        return Some(RefusalKind::NoSuchCommand(none.command.clone()));
    }
    error
        .is::<term::Unwritable>()
        .then_some(RefusalKind::Unwritable)
}

/// How long a refusal is kept for its sender once announced, counted while no drain of its session
/// runs: the sender waiting on it takes it right after the drain that met it, however long that
/// drain goes on writing, so one left longer had no sender waiting.
const REFUSAL_KEPT: Duration = Duration::from_secs(60);

/// How many senders [`Outbox::note_sender`] keeps, the oldest tickets going first: far more than
/// the messages a session keeps of those written while a turn ran (`crate::session::HELD_KEPT`)
/// for every session that may be running at once, and a bound for the messages written and never
/// asked about. Every message a session sends counts, so a message still held or just written is
/// among those forgotten only once more than this many have been sent since it: an assumption
/// about how much sessions send within one turn, not a guarantee.
const SENDERS_KEPT: usize = 4096;

/// The writers waiting on a drain to answer them ([`Outbox::await_answer`]), and what a drain met
/// for them that the drain's own answer may not carry.
#[derive(Default)]
struct Awaited {
    /// The messages whose writer waits.
    tickets: HashSet<Ticket>,
    /// The text of each of those a drain met gone elsewhere ([`Drain::WentElsewhere`]).
    went_elsewhere: HashMap<Ticket, String>,
}

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
        self.push_unsealed(id, text, true, None)
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

    /// Notes that the message `ticket` was sent by the session `sender`, for whoever learns later
    /// that the message may not have been carried out to tell that session too
    /// ([`Self::sender_of`]). Kept past the write and through the message being written again,
    /// and forgotten once the message leaves the queue unwritten, or among the oldest past
    /// [`SENDERS_KEPT`].
    pub(crate) fn note_sender(&self, ticket: Ticket, sender: &str) {
        let mut senders = self.senders.lock().expect("outbox senders lock poisoned");
        senders.insert(ticket, sender.to_string());
        while senders.len() > SENDERS_KEPT {
            senders.pop_first();
        }
    }

    /// The session that sent the message the queue wrote as `logical_id` (`<launch id>:<ticket>`,
    /// see [`Self::drain`]), where a session sent it ([`Self::note_sender`]).
    pub(crate) fn sender_of(&self, logical_id: &str) -> Option<String> {
        let ticket = redelivery::ticket_of(logical_id)?;
        self.senders
            .lock()
            .expect("outbox senders lock poisoned")
            .get(&ticket)
            .cloned()
    }

    /// Notes that the writer of the message `ticket` waits on the drain to answer it: what a drain
    /// meets for it is kept for that writer ([`Self::claim_went_elsewhere`]), whichever drain
    /// meets it, until it takes it ([`Self::take_answer`]).
    pub(crate) fn await_answer(&self, ticket: Ticket) {
        self.awaited
            .lock()
            .expect("outbox awaited lock poisoned")
            .tickets
            .insert(ticket);
    }

    /// Ends the wait of the writer of the message `ticket` ([`Self::await_answer`]), handing it the
    /// text of its message where a drain met it gone elsewhere.
    pub(crate) fn take_answer(&self, ticket: Ticket) -> Option<String> {
        let mut awaited = self.awaited.lock().expect("outbox awaited lock poisoned");
        awaited.tickets.remove(&ticket);
        awaited.went_elsewhere.remove(&ticket)
    }

    /// Keeps that the message `ticket`, which read `text`, went elsewhere, for its writer, where
    /// one waits on the drain to answer it, and says whether one does: where none does, the
    /// sender has been answered already and is told by the caller.
    pub(crate) fn claim_went_elsewhere(&self, ticket: Ticket, text: &str) -> bool {
        let mut awaited = self.awaited.lock().expect("outbox awaited lock poisoned");
        if !awaited.tickets.contains(&ticket) {
            return false;
        }
        awaited.went_elsewhere.insert(ticket, text.to_string());
        true
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
        self.push_unsealed(id, text, false, None)
    }

    /// [`Self::push_unless_sealed`] for a message of the given `shape` (`past_usage_limit` as
    /// [`Self::push_past_usage_limit`]): written as the shape says, and not carried over anywhere
    /// as text.
    pub fn push_shaped(
        &self,
        id: &str,
        text: &str,
        shape: Shape,
        past_usage_limit: bool,
    ) -> Option<Ticket> {
        self.push_unsealed(id, text, past_usage_limit, Some(shape))
    }

    /// [`Self::push_unless_sealed`], marking the message as going past a hold for the account's
    /// usage limit when `past_usage_limit`, and noting its `shape`.
    fn push_unsealed(
        &self,
        id: &str,
        text: &str,
        past_usage_limit: bool,
        shape: Option<Shape>,
    ) -> Option<Ticket> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        if self.sealed.load(Ordering::Acquire) {
            return None;
        }
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        if let Some(shape) = shape {
            self.shapes
                .lock()
                .expect("outbox shapes lock poisoned")
                .insert(ticket, shape);
        }
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
    /// in no particular order. Once the queues are taken, [`Self::push`] refuses too. A message of a
    /// [`Shape`] (images, a slash command) is dropped, with a warning in the log, and not saved: it
    /// cannot be saved as text, the image files go with the session's scratch directory, and a
    /// command written as text would reach the model. **Blocks** for up to `drains`.
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
                    .filter_map(|(ticket, text, _)| {
                        let row = kept.queued.remove(&ticket);
                        if let Some(row) = row {
                            kept.tickets.remove(&row);
                        }
                        let shaped = self
                            .shapes
                            .lock()
                            .expect("outbox shapes lock poisoned")
                            .remove(&ticket)
                            .is_some();
                        if shaped {
                            tracing::warn!(session = %id, "a queued message with images or a slash command cannot be carried to the next daemon as text; it is dropped");
                            return None;
                        }
                        Some((ticket, text, row))
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
        self.push_kept_at(id, text, row, End::Back)
    }

    /// [`Self::push_kept`] at the front of the session's queue, for the first task of a launch that
    /// writes it as a message (`crate::adapter::LaunchPlan::first_message`): whatever was queued
    /// for the session while it was being launched goes after it.
    pub fn push_kept_first(&self, id: &str, text: &str, row: i64) -> Option<Ticket> {
        self.push_kept_at(id, text, row, End::Front)
    }

    /// Queues a message kept in the store as row `row` at `end` of the session's queue, unless the
    /// queues are sealed or the row already has a message queued or being written.
    fn push_kept_at(&self, id: &str, text: &str, row: i64, end: End) -> Option<Ticket> {
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
        let queue = queues.entry(id.to_string()).or_default();
        let message = (ticket, text.to_string(), None);
        match end {
            End::Front => queue.push_front(message),
            End::Back => queue.push_back(message),
        }
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
    pub fn take_refusal(&self, ticket: Ticket) -> Option<Refused> {
        self.refused
            .lock()
            .expect("outbox refused lock poisoned")
            .remove(&ticket)
            .map(|refusal| refusal.refused)
    }

    /// Records that the message `ticket`, written for the session `id`, failed alone, and is no
    /// longer queued.
    fn refuse(&self, id: &str, ticket: Ticket, reason: String, kind: RefusalKind) {
        self.mark_written(ticket);
        self.refused
            .lock()
            .expect("outbox refused lock poisoned")
            .insert(
                ticket,
                Refusal {
                    session: id.to_string(),
                    refused: Refused { reason, kind },
                    announced: None,
                },
            );
    }

    /// The refusals of messages for session `id` not announced yet, by ticket, which are now marked
    /// announced; whichever drain asks first announces them, and
    /// each sender still finds its own ([`Self::take_refusal`]).
    pub fn refusals_to_announce(&self, id: &str) -> Vec<(Ticket, Refused)> {
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
        let mut announce: Vec<(Ticket, Refused)> = refused
            .iter_mut()
            .filter(|(_, refusal)| refusal.session == id && refusal.announced.is_none())
            .map(|(ticket, refusal)| {
                refusal.announced = Some(Instant::now());
                (*ticket, refusal.refused.clone())
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
        // Never written by this queue, so nothing can show later that it was not carried out.
        let mut senders = self.senders.lock().expect("outbox senders lock poisoned");
        for ticket in tickets.clone() {
            senders.remove(&ticket);
        }
        drop(senders);
        let mut kept = self.kept.lock().expect("outbox kept lock poisoned");
        for ticket in tickets {
            if let Some(row) = kept.queued.remove(&ticket) {
                kept.tickets.remove(&row);
            }
        }
    }

    /// Forgets what is noted of these messages beside the queue as they leave it: whether a hold of
    /// the server started their grace again, whether they go past a hold for the usage limit, and
    /// their shape, whether they are held back, and how many times they have been written again.
    fn forget_ticket_marks(&self, tickets: impl Iterator<Item = Ticket>) {
        let mut restarted = self
            .grace_restarted
            .lock()
            .expect("outbox grace lock poisoned");
        let mut past = self
            .past_usage_limit
            .lock()
            .expect("outbox usage limit lock poisoned");
        let mut shapes = self.shapes.lock().expect("outbox shapes lock poisoned");
        let mut held = self
            .held_back
            .lock()
            .expect("outbox held-back lock poisoned");
        let mut redeliveries = self
            .redeliveries
            .lock()
            .expect("outbox redeliveries lock poisoned");
        let mut dropped = self.dropped.lock().expect("outbox dropped lock poisoned");
        for ticket in tickets {
            restarted.remove(&ticket);
            past.remove(&ticket);
            shapes.remove(&ticket);
            held.remove(&ticket);
            redeliveries.remove(&ticket);
            dropped.remove(&ticket);
        }
    }

    /// Writes out whatever is queued for this session, oldest first.
    ///
    /// **Blocks** on the PTY write, which polls for seconds when the child is not draining, and on
    /// the wait for a written message to be sent.
    pub fn drain(&self, session: &LiveSession, prompts: &impl PromptSignal) -> Drain {
        self.drain_with(
            &session.id,
            &mut |ticket, text, past_grace| {
                let logical_id = format!("{}:{ticket}", session.launch_id);
                match self.shape_of(ticket) {
                    Some(shape) => term::send_shaped_with_id(
                        session,
                        text,
                        &shape,
                        &logical_id,
                        past_grace,
                        &|| prompts.writes_stopped(),
                    ),
                    None => {
                        term::send_message_with_id(session, text, &logical_id, past_grace, &|| {
                            prompts.writes_stopped()
                        })
                    }
                }
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
        #[cfg(test)]
        self.drains.fetch_add(1, Ordering::AcqRel);
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
                Drain::Clear if self.takeable(id) > 0 => continue,
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
                    None if self.takeable(id) > 0 => return Drain::HeldForUsageLimit,
                    None => None,
                }
            } else {
                self.pop(id)
            };
            let Some((ticket, text, protection_started)) = next else {
                return Drain::Clear;
            };
            let protection_started = protection_started.unwrap_or_else(Instant::now);
            // A slash command is no prompt: a built-in one reports none, so its hook is not waited
            // for. Its write confirmed that it left the input box (`LiveSession::write_command`).
            let checked = checked && self.shape_of(ticket) != Some(Shape::Command);
            let before = prompts.sent();
            let mid_turn = prompts.mid_turn();
            let shaped = self.shape_of(ticket).is_some();
            let past_usage_limit = self
                .past_usage_limit
                .lock()
                .expect("outbox usage limit lock poisoned")
                .contains(&ticket);
            // Before the write: a server or channel that took it can say the agent gave it back
            // before the write has returned.
            self.begin_write(id, ticket);
            let written = write(ticket, &text, protection_started.elapsed() >= DRAFT_WAIT);
            // In the terminal, whole or in part: it is the latest written there from now on.
            if matches!(&written, Ok(Route::Terminal))
                || matches!(&written, Err(partial) if partial.written > 0)
            {
                self.note_latest(id, ticket, &text, shaped, past_usage_limit);
            }
            // Out of the terminal's way, so nothing of it can be cleared from the input box; an
            // agent that gives it back says so through its server or channel.
            let given_back = if matches!(&written, Ok(Route::Queued)) {
                self.note_taken_by_transport(id, ticket, &text, shaped, past_usage_limit)
            } else {
                self.end_write(id, ticket);
                false
            };
            // A message the agent's own queue took is no part of the running turn: it runs after it.
            if mid_turn && matches!(written, Ok(Route::Terminal)) {
                prompts.note_written_mid_turn();
            }
            match written {
                // Nothing of it is in the input box, and the queue answered for it.
                Ok(Route::Queued) => {
                    self.mark_written(ticket);
                    // Given back while it was written, when the queue did not hold it as taken
                    // yet: taken up now that it does.
                    if given_back {
                        prompts.given_back_while_written(ticket);
                    }
                }
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
                    if prompts.write_went_elsewhere() {
                        // Neither sent nor in the box: it went where nothing can see it, and
                        // nothing of it is left to press Enter on or to wait for.
                        tracing::warn!(
                            session = %id,
                            "a written message was not sent and the input box reads empty; it \
                             may be lost"
                        );
                        self.mark_written(ticket);
                        prompts.went_elsewhere(ticket, &text);
                        return Drain::WentElsewhere(ticket);
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
                // The agent would have run another command than the one written, or has none of
                // it, or the message could not be written as sent: it fails to its sender alone, is
                // never written again, and the queue behind it goes on. Not the agent refusing: the
                // session is told with a notice of its own kind (`command_not_recognised`,
                // `command_not_found`, `message_not_written`), and the log holds no text of it.
                Err(partial) if partial.error.get_ref().and_then(refusal_kind).is_some() => {
                    tracing::warn!(session = %id, "a message was refused: it could not be written as sent");
                    let kind = partial
                        .error
                        .get_ref()
                        .and_then(refusal_kind)
                        .expect("matched as a refusal");
                    self.refuse(id, ticket, partial.error.to_string(), kind);
                    continue;
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
                    self.refuse(id, ticket, partial.error.to_string(), RefusalKind::Agent);
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
                    // grace would have whatever the user has in the box cleared out of the
                    // message's way the moment the terminal takes it.
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

    /// How many messages are still queued for this session, the ones held back until another is
    /// taken included ([`Self::takeable`] leaves those out).
    pub fn len(&self, id: &str) -> usize {
        self.queues
            .lock()
            .expect("outbox lock poisoned")
            .get(id)
            .map(|queue| queue.len())
            .unwrap_or(0)
    }

    /// Whether anything a drain may take is queued for this session, or is being written into it
    /// now. A message held back until another is taken ([`Self::redeliver_held`]) is not pending:
    /// nothing will write it by itself, so a session with only such messages is as idle as one
    /// with none.
    pub fn is_pending(&self, id: &str) -> bool {
        self.takeable(id) > 0 || self.is_draining(id)
    }

    /// Whether a drain owns the session's queue now.
    pub(crate) fn is_draining(&self, id: &str) -> bool {
        self.draining
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
        self.forget_latest(id);
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
        self.forget_latest(id);
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
    /// kept in, if it is. A message of a [`Shape`] is dropped, with a warning in the log, and not
    /// returned: whoever forwards what is returned writes it as text.
    #[cfg(test)]
    pub fn take(&self, id: &str) -> Vec<(String, Option<i64>)> {
        self.take_counting_shaped(id).0
    }

    /// [`Self::take`], also counting the messages of a [`Shape`] it dropped.
    pub fn take_counting_shaped(&self, id: &str) -> (Vec<(String, Option<i64>)>, usize) {
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
        let shaped: HashSet<Ticket> = {
            let shapes = self.shapes.lock().expect("outbox shapes lock poisoned");
            queue
                .iter()
                .map(|(ticket, _, _)| *ticket)
                .filter(|ticket| shapes.contains_key(ticket))
                .collect()
        };
        self.unkeep(queue.iter().map(|(ticket, _, _)| *ticket));
        if !shaped.is_empty() {
            tracing::warn!(session = %id, dropped = shaped.len(), "queued messages with images or a slash command cannot be forwarded as text; they are dropped");
        }
        let messages = queue
            .into_iter()
            .zip(rows)
            .filter(|((ticket, _, _), _)| !shaped.contains(ticket))
            .map(|((_, text, _), row)| (text, row))
            .collect();
        (messages, shaped.len())
    }

    /// How the message `ticket`, still queued, is written besides its text, if it is shaped.
    pub fn shape_of(&self, ticket: Ticket) -> Option<Shape> {
        self.shapes
            .lock()
            .expect("outbox shapes lock poisoned")
            .get(&ticket)
            .cloned()
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

    /// Takes the oldest message of the session's queue that is not held back
    /// ([`Self::redeliver_held`]); the ones held back are held no longer once one is taken, so
    /// they follow it.
    fn pop(&self, id: &str) -> Option<(Ticket, String, Option<Instant>)> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let queue = queues.get_mut(id)?;
        let mut held = self
            .held_back
            .lock()
            .expect("outbox held-back lock poisoned");
        let index = queue
            .iter()
            .position(|(ticket, _, _)| !held.contains(ticket))?;
        let next = queue.remove(index);
        for (ticket, _, _) in queue.iter() {
            held.remove(ticket);
        }
        drop(held);
        if queue.is_empty() {
            queues.remove(id);
        }
        next
    }

    /// Takes the oldest message of the session's queue that a hold for the usage limit does not
    /// hold, leaving the others where they are. One held back until another message is taken
    /// ([`Self::redeliver_held`]) is not taken for going past that hold, as in [`Self::pop`].
    fn pop_past_usage_limit(&self, id: &str) -> Option<(Ticket, String, Option<Instant>)> {
        let mut queues = self.queues.lock().expect("outbox lock poisoned");
        let queue = queues.get_mut(id)?;
        let past = self
            .past_usage_limit
            .lock()
            .expect("outbox usage limit lock poisoned");
        let mut held = self
            .held_back
            .lock()
            .expect("outbox held-back lock poisoned");
        let index = queue
            .iter()
            .position(|(ticket, _, _)| past.contains(ticket) && !held.contains(ticket))?;
        drop(past);
        let next = queue.remove(index);
        // A message taken releases the ones held back until one is.
        for (ticket, _, _) in queue.iter() {
            held.remove(ticket);
        }
        drop(held);
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
mod tests;
