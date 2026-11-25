//! The message a provider reads for a turn from a source of its own (a hook payload, the agent's
//! transcript), handed to the turn end the program's report closes, whose `msg` it replaces: a
//! program may cut the `msg` it reports, or report none. A provider whose program reports something
//! else as its `msg` says so (`HookInterpreter::report_msg_is_turn_message`), and a turn it gave
//! no message for then ends with none.
//!
//! The provider's hook interpreter gives it on a hook event
//! (`crate::hooks::HookInterpreter::turn_message`), and the turn is still closed once, by the
//! report. The two arrive by different routes, in either order, so a turn end of a provider that
//! declares it supplies messages (`HookInterpreter::supplies_turn_message`) waits for the message,
//! up to [`BOUND`].
//!
//! A source that has to be opened is read off the hook queue, on a thread of its own that is
//! waited for no longer than [`BOUND`] (`blocking_pass::within_deadline`): the read is of a file
//! the agent names, which can stall. A session has one such read at a time, so a read that never
//! returns leaves one thread behind and no more: while it runs, a later read for the session is
//! dropped unrun, and its turn end takes the report's `msg` at the bound.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::blocking_pass::{within_deadline, StalledPasses};
use crate::hooks::TurnMessage;
use crate::session::LiveSession;

/// How long a turn end waits for the message its provider supplies, after which the report's own
/// `msg` is what the turn said. The session stays `working` for the wait, so it is kept to what a
/// hook command's start and one read of a file take on a loaded machine.
#[cfg_attr(test, allow(dead_code))]
const BOUND: Duration = Duration::from_secs(2);

/// How often a waiting turn end looks at whether its session is gone.
const POLL: Duration = Duration::from_millis(25);

/// [`BOUND`], except under test: there the message comes from the test, so the window it has to
/// arrive in is `PATIENCE`, and a test proving the bound runs out shortens it with
/// [`shorten_turn_message_bound`]. Read on the thread that takes the reports in.
pub(super) fn bound() -> Duration {
    #[cfg(test)]
    {
        BOUND_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    BOUND
}

#[cfg(test)]
thread_local! {
    static BOUND_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// Shortens the bound a turn end waits for its message to `to`, on the calling thread, until the
/// guard is dropped: for a test, the generic ones and a provider's own alike, of a turn whose
/// message does not come (a hook that failed, nothing forwarded). The bound is read on the thread
/// that takes the reports in, which is the test's own where it feeds the session's output, and,
/// for a provider's source read, where the hook event is applied.
#[cfg(test)]
pub(crate) fn shorten_turn_message_bound(to: Duration) -> ShortenedTurnMessageBound {
    BOUND_OVERRIDE.with(|bound| bound.set(Some(to)));
    ShortenedTurnMessageBound
}

/// Gives the bound back its test value when dropped ([`shorten_turn_message_bound`]).
#[cfg(test)]
#[must_use = "the bound is shortened only while the guard lives"]
pub(crate) struct ShortenedTurnMessageBound;

#[cfg(test)]
impl Drop for ShortenedTurnMessageBound {
    fn drop(&mut self) {
        BOUND_OVERRIDE.with(|bound| bound.set(None));
    }
}

/// The message supplied for a session's current turn, shared with the task reading it.
pub(in crate::session) struct TurnMessages {
    /// The provider declares it supplies its turns' messages, so a turn end waits for one.
    awaited: bool,
    /// The `msg` of the program's report is the turn's message, and stands where none was
    /// supplied.
    msg_is_turn_message: bool,
    slot: Mutex<Slot>,
    arrived: Condvar,
    /// A read of the provider's source is running for this session, on its own thread.
    reading: Arc<AtomicBool>,
    /// The read that outran its deadline, as `within_deadline` records it; the session's one read
    /// at a time is its only key.
    stalled: StalledPasses<()>,
    /// How many turn ends are waiting for a message now.
    #[cfg(test)]
    waiters: std::sync::atomic::AtomicUsize,
}

/// A read of a provider's source with the mark that says it runs: the mark comes down when the
/// read has returned, or when it is dropped unrun.
type Reading = (Box<dyn FnOnce() -> Option<String> + Send>, ReadingMark);

/// Clears [`TurnMessages::reading`] when dropped.
struct ReadingMark(Arc<AtomicBool>);

impl Drop for ReadingMark {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Default)]
struct Slot {
    /// Counts the turns' starts and ends, so a message read for one turn is not given to another:
    /// one offered under an earlier count is dropped.
    turn: u64,
    /// What the provider gave for the turn: its message, or that it has none.
    offered: Option<Option<String>>,
}

impl TurnMessages {
    pub(in crate::session) fn new(awaited: bool, msg_is_turn_message: bool) -> Self {
        Self {
            awaited,
            msg_is_turn_message,
            slot: Mutex::default(),
            arrived: Condvar::new(),
            reading: Arc::default(),
            stalled: StalledPasses::default(),
            #[cfg(test)]
            waiters: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Whether the `msg` of the report that ends a turn is that turn's message where the provider
    /// supplied none.
    pub(super) fn report_msg_stands(&self) -> bool {
        self.msg_is_turn_message
    }

    /// How many turn ends are waiting for a message now.
    #[cfg(test)]
    pub(super) fn waiters(&self) -> usize {
        self.waiters.load(Ordering::Acquire)
    }

    /// Whether a read of the provider's source is running for this session.
    #[cfg(test)]
    pub(super) fn reading(&self) -> bool {
        self.reading.load(Ordering::Acquire)
    }

    /// Runs `read` on a thread of its own and gives what it read for `turn`, waiting for it no
    /// longer than `deadline`. **Blocks** for up to `deadline`.
    fn read_within(&self, turn: u64, reading: Reading, deadline: Duration) {
        let groups = HashMap::from([((), vec![Mutex::new(Some(reading))])]);
        let pass = within_deadline(
            &self.stalled,
            groups,
            deadline,
            |reading: &Mutex<Option<Reading>>| {
                let (read, _mark) = reading.lock().ok()?.take()?;
                read()
            },
        );
        // Nothing read, in time or at all, is the provider having no message: the turn end does
        // not wait on.
        self.offer(
            turn,
            pass.found.into_iter().next().map(|(_, message)| message),
        );
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.slot.lock().expect("turn message lock poisoned")
    }

    /// The turn a message read from now on is for.
    fn turn(&self) -> u64 {
        self.slot().turn
    }

    /// Gives `message` for `turn`, unless that turn has ended or another has started meanwhile.
    fn offer(&self, turn: u64, message: Option<String>) {
        let mut slot = self.slot();
        if slot.turn == turn {
            slot.offered = Some(message);
            self.arrived.notify_all();
        }
    }

    /// A turn started or ended with no message to take: one given before belongs to neither side
    /// of it.
    pub(super) fn turn_boundary(&self) {
        let mut slot = self.slot();
        slot.turn += 1;
        slot.offered = None;
    }

    /// The message supplied for the turn that is ending, waited for up to `bound` where the
    /// provider declares it supplies one and has not said yet; `gone` ends the wait early (the
    /// process ended, the daemon is stopping). A blank message is none. The turn is over
    /// afterwards: a message that comes later is dropped.
    pub(super) fn take(&self, bound: Duration, gone: impl Fn() -> bool) -> Option<String> {
        let mut slot = self.slot();
        if self.awaited {
            #[cfg(test)]
            let _waiting = Waiting::of(&self.waiters);
            let until = Instant::now() + bound;
            while slot.offered.is_none() && !gone() {
                let left = until.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                slot = self
                    .arrived
                    .wait_timeout(slot, left.min(POLL))
                    .expect("turn message lock poisoned")
                    .0;
            }
        }
        let offered = slot.offered.take().flatten();
        slot.turn += 1;
        offered.filter(|message| !message.trim().is_empty())
    }
}

impl LiveSession {
    /// Takes what the provider's hook interpreter gave as the current turn's message, for the turn
    /// end the program's report closes. Nothing where the reports do not give this session its
    /// status: a hook's own turn end carries the message there. A source to be read is read on a
    /// thread of its own, never on the caller's, which is the session's hook queue, and dropped
    /// unrun while an earlier read for the session is still running.
    pub(crate) fn supply_turn_message(&self, message: TurnMessage) {
        if !self.status_from_reports {
            return;
        }
        let turn = self.turn_messages.turn();
        match message {
            TurnMessage::Said(message) => self.turn_messages.offer(turn, message),
            TurnMessage::Read(read) => {
                let messages: Arc<TurnMessages> = self.turn_messages.clone();
                if messages.reading.swap(true, Ordering::AcqRel) {
                    return;
                }
                let reading: Reading = (read, ReadingMark(messages.reading.clone()));
                let deadline = bound();
                let pass = move || messages.read_within(turn, reading, deadline);
                // The pass only waits, for the deadline at most, so it may hold a blocking thread.
                match tokio::runtime::Handle::try_current() {
                    Ok(runtime) => drop(runtime.spawn_blocking(pass)),
                    Err(_) => drop(std::thread::spawn(pass)),
                }
            }
        }
    }
}

/// Counts a turn end among those waiting for a message, for as long as it lives.
#[cfg(test)]
struct Waiting<'a>(&'a std::sync::atomic::AtomicUsize);

#[cfg(test)]
impl<'a> Waiting<'a> {
    fn of(count: &'a std::sync::atomic::AtomicUsize) -> Self {
        count.fetch_add(1, Ordering::AcqRel);
        Self(count)
    }
}

#[cfg(test)]
impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
