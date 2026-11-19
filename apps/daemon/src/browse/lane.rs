//! How browse requests share a control connection with everything else on it without crowding it
//! out.
//!
//! Several bounds hold at once. Per connection, at most [`budget::MAX_PENDING_REQUESTS`] browse
//! requests are outstanding — waiting, being read, or queued until their frame is written — and
//! one more is refused on the spot, the refusal itself waiting for room on the control queue so a
//! client that floods the socket without reading it is slowed rather than buffered for. Across the
//! daemon, at most [`budget::MAX_CONCURRENT_READS`] reads run at a time, and the bytes held for
//! browse replies — a read's own buffers while it runs, a finished reply until its frame is on the
//! socket — stay within [`budget::MAX_RETAINED_BYTES`], of which one connection may hold at most
//! [`budget::MAX_RETAINED_PER_CONNECTION`], as do the reads of the agents' MCP tools all together
//! (see [`Gate::admit`]): each read reserves its worst case from both before it starts, gives back
//! what its serialized reply did not need, and gives back the rest when the frame has been
//! written. Replies leave on a queue of their own, which the connection's writer drains only
//! while no control event is waiting, so a burst of file bodies never holds a status change behind
//! it.
//!
//! A request may name a slot, per connection. A newer request in the same slot cancels the older
//! one — its file read stops at the next chunk, its `git` is killed — and the older one is answered
//! `request_superseded`. Closing the connection cancels everything it had outstanding.
//!
//! A streamed request ([`Lane::submit_stream`], the content search) is answered with `ack` and
//! then sends events while it runs, each carved out of its reservation and holding its share
//! until written, so the reservation bounds everything it sends. Once acknowledged, its id has its
//! answer: a cancel ends it with nothing more sent rather than `request_superseded`, and it waits
//! for room on the reply queue with its read slot given back, its reading threads paused until
//! the slot is taken back, and its cancel watched (see [`Stream::send`]).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use tokio::sync::{mpsc, oneshot, Notify, OwnedSemaphorePermit, Semaphore};

use crate::browse::{budget, reply_parts, Cancelled};
use crate::protocol::{error_code, CodedError, Event};

/// The daemon-wide part of the bounds: one per daemon, shared by every connection.
#[derive(Debug)]
pub struct Gate {
    reads: Arc<Semaphore>,
    retained: Arc<Semaphore>,
    retained_max: usize,
    /// The share of the retained bytes that all the reads of the agents' MCP tools hold together,
    /// as a connection's lane holds its own.
    tools: Arc<Semaphore>,
    tools_max: usize,
}

impl Gate {
    /// `reads` reads at a time, `retained` bytes held.
    pub fn new(reads: usize, retained: usize) -> Self {
        Self {
            reads: Arc::new(Semaphore::new(reads)),
            retained: Arc::new(Semaphore::new(retained)),
            retained_max: retained,
            tools: Arc::new(Semaphore::new(retained)),
            tools_max: retained,
        }
    }

    /// The same gate with the agents' MCP tools held to `tools` retained bytes together.
    pub fn with_tools_share(mut self, tools: usize) -> Self {
        self.tools = Arc::new(Semaphore::new(tools));
        self.tools_max = tools;
        self
    }

    /// The retained bytes not reserved by any read or reply right now.
    #[cfg(test)]
    pub fn retained_available(&self) -> usize {
        self.retained.available_permits()
    }

    /// The read slots no read holds right now.
    #[cfg(test)]
    pub fn reads_available(&self) -> usize {
        self.reads.available_permits()
    }

    /// Waits for a place in the agents' share, a read slot and `reservation` retained bytes, for
    /// a read that comes from inside the daemon rather than over a connection (an MCP tool's), so
    /// it is held to the same daemon-wide bounds as a window's, and all such reads together to one
    /// connection's share: a window still gets its own share beside them. All three are given
    /// back when the admission drops.
    pub async fn admit(&self, reservation: usize) -> Admission {
        let reservation = reservation.min(self.retained_max).min(self.tools_max);
        let tools = self
            .tools
            .clone()
            .acquire_many_owned(reservation as u32)
            .await
            .expect("the gate's semaphores are never closed");
        let retained = self
            .retained
            .clone()
            .acquire_many_owned(reservation as u32)
            .await
            .expect("the gate's semaphores are never closed");
        let reading = self
            .reads
            .clone()
            .acquire_owned()
            .await
            .expect("the gate's semaphores are never closed");
        Admission {
            _reading: reading,
            _retained: retained,
            _tools: tools,
        }
    }

    pub fn with_budget() -> Self {
        Self::new(budget::MAX_CONCURRENT_READS, budget::MAX_RETAINED_BYTES)
            .with_tools_share(budget::MAX_RETAINED_PER_CONNECTION)
    }
}

/// A read slot and a share of the retained bytes, held while one read runs; see [`Gate::admit`].
#[derive(Debug)]
pub struct Admission {
    _reading: OwnedSemaphorePermit,
    _retained: OwnedSemaphorePermit,
    _tools: OwnedSemaphorePermit,
}

/// A finished reply waiting for the socket. It holds its request's place among the connection's
/// outstanding ones and its share of the retained bytes until the writer has sent it and dropped
/// it.
#[derive(Debug)]
pub struct Frame {
    pub text: String,
    _retained: OwnedSemaphorePermit,
    _connection: OwnedSemaphorePermit,
    _pending: OwnedSemaphorePermit,
    /// For a part of a held reply, its turn: the reply's room for one part is in use until the
    /// frame is written, the next part request waits until then, and the reply's idle time counts
    /// from then.
    _part: Option<PartTurn>,
}

impl Frame {
    /// A frame holding permits of its own, for a test of what is done with frames.
    #[cfg(test)]
    pub fn standalone(text: &str) -> Self {
        let permit = || {
            Arc::new(Semaphore::new(1))
                .try_acquire_owned()
                .expect("a permit")
        };
        Self {
            text: text.to_string(),
            _retained: permit(),
            _connection: permit(),
            _pending: permit(),
            _part: None,
        }
    }
}

/// A cancel a blocking read polls and an async wait can be woken by.
#[derive(Debug, Default)]
struct Cancel {
    flag: AtomicBool,
    notify: Notify,
}

impl Cancel {
    fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug, Default)]
struct InFlight {
    next: u64,
    requests: HashMap<u64, Arc<Cancel>>,
    slots: HashMap<String, u64>,
    closed: bool,
    /// Replies held to be read in parts, by handle.
    held: HashMap<String, HeldReply>,
    next_reply: u64,
    /// The handles of the latest replies a newer request in their slot released, so that a part
    /// asked for afterwards is answered `request_superseded` rather than `reply_part_unavailable`.
    superseded_replies: VecDeque<String>,
}

/// How many released handles [`InFlight::superseded_replies`] remembers.
const SUPERSEDED_REPLIES_KEPT: usize = 64;

impl InFlight {
    /// Forgets request `seq`, and its hold on `slot` unless a newer request has taken it.
    fn unregister(&mut self, seq: u64, slot: Option<&str>) {
        self.requests.remove(&seq);
        if let Some(slot) = slot {
            if self.slots.get(slot) == Some(&seq) {
                self.slots.remove(slot);
            }
        }
    }

    /// Releases the held reply `handle`: drops its text, and its reservation once no part of it
    /// is left to write; a part request waiting for its turn is answered as for a released one.
    fn release(&mut self, handle: &str, superseded: bool) {
        let Some(held) = self.held.remove(handle) else {
            return;
        };
        held.shared.released.cancel();
        if superseded {
            if self.superseded_replies.len() == SUPERSEDED_REPLIES_KEPT {
                self.superseded_replies.pop_front();
            }
            self.superseded_replies.push_back(handle.to_string());
        }
    }

    /// The answer to a part asked of `handle`, a reply no longer held.
    fn gone(&self, id: Option<String>, handle: &str) -> Event {
        if self.superseded_replies.iter().any(|h| h == handle) {
            superseded(id)
        } else {
            reply_parts::unavailable(id, handle)
        }
    }
}

/// A browse reply larger than the part size its request asked for, kept to be read in parts.
#[derive(Debug)]
struct HeldReply {
    text: Arc<String>,
    /// The slot of the request it answers: a newer request in that slot releases it.
    slot: Option<String>,
    /// The part size the request asked for, the default of each `read_reply_part`.
    part_bytes: usize,
    shared: Arc<HeldShared>,
}

/// What a held reply keeps until its last part is written: its reservation (the reply's text and
/// the room for one part's frame), its place among the connection's outstanding requests, and when
/// it expires.
#[derive(Debug)]
struct HeldShared {
    /// The bytes of the reservation one part's frame may use beside the text.
    room: usize,
    /// Set when the reply is released, waking the part requests waiting for their turn.
    released: Cancel,
    /// The end of the queue of part requests: the turn the next one waits on. One part is in
    /// flight at a time, so that the room covers it, and parts are served in the order they were
    /// asked for, since a request takes its place here before anything it does is spawned.
    tail: Mutex<Option<oneshot::Receiver<()>>>,
    /// The part requests queued or in flight; a reply with any is not expired.
    queued: AtomicUsize,
    expires: Mutex<tokio::time::Instant>,
    _retained: OwnedSemaphorePermit,
    _connection: OwnedSemaphorePermit,
    _pending: OwnedSemaphorePermit,
}

impl HeldShared {
    /// Takes the next place in the queue of part requests: the turn to wait for (none when the
    /// queue is empty) and this request's own, which lets the next one go when dropped.
    fn take_turn(self: &Arc<Self>) -> (Option<oneshot::Receiver<()>>, PartTurn) {
        let (done, next) = oneshot::channel();
        let previous = self
            .tail
            .lock()
            .expect("held reply lock poisoned")
            .replace(next);
        self.queued.fetch_add(1, Ordering::Relaxed);
        (
            previous,
            PartTurn {
                shared: self.clone(),
                _done: done,
            },
        )
    }
}

/// A part request's place in its reply's queue, carried by the part's [`Frame`] until it is
/// written: the next part request goes once it is dropped, and the reply's idle time counts from
/// then.
#[derive(Debug)]
pub struct PartTurn {
    shared: Arc<HeldShared>,
    _done: oneshot::Sender<()>,
}

impl Drop for PartTurn {
    fn drop(&mut self) {
        self.shared.queued.fetch_sub(1, Ordering::Relaxed);
        *self
            .shared
            .expires
            .lock()
            .expect("held reply lock poisoned") =
            tokio::time::Instant::now() + budget::HELD_REPLY_IDLE;
    }
}

/// One control connection's browse lane.
#[derive(Debug)]
pub struct Lane {
    gate: Arc<Gate>,
    pending: Arc<Semaphore>,
    pending_max: usize,
    /// This connection's share of the gate's retained bytes.
    retained: Arc<Semaphore>,
    retained_max: usize,
    in_flight: Mutex<InFlight>,
    control: mpsc::Sender<Event>,
    content: mpsc::Sender<Frame>,
    /// Set as a streamed request's last event starts waiting for room on the reply queue.
    #[cfg(test)]
    last_event_waits: AtomicBool,
}

/// Counts the bytes a value serializes to, so the frame can be allocated once at its size.
struct Counter(usize);

impl std::io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Lane {
    /// A lane answering errors on `control` and sending replies on `content`, with at most
    /// `pending` requests outstanding and `retained` bytes held.
    pub fn new(
        gate: Arc<Gate>,
        pending: usize,
        retained: usize,
        control: mpsc::Sender<Event>,
        content: mpsc::Sender<Frame>,
    ) -> Self {
        Self {
            gate,
            pending: Arc::new(Semaphore::new(pending)),
            pending_max: pending,
            retained: Arc::new(Semaphore::new(retained)),
            retained_max: retained,
            in_flight: Mutex::new(InFlight::default()),
            control,
            content,
            #[cfg(test)]
            last_event_waits: AtomicBool::new(false),
        }
    }

    /// [`Lane::submit_paged`] for a request whose reply is never held, as most tests submit one.
    #[cfg(test)]
    pub async fn submit<W>(
        self: &Arc<Self>,
        id: Option<String>,
        slot: Option<String>,
        reservation: usize,
        work: W,
    ) where
        W: FnOnce(&AtomicBool) -> Result<Event> + Send + 'static,
    {
        self.submit_paged(id, slot, None, reservation, work).await;
    }

    /// Starts `work` for request `id`, under the lane's bounds, reserving `reservation` bytes —
    /// the most its reply and its buffers can come to — for it; its reply, or its failure, goes
    /// back on the connection. Returns once the request is under way, or once its refusal is on
    /// the control queue.
    ///
    /// With `part_bytes` (already cut and raised to the bounds), the request asked for its reply in
    /// parts of at most that many bytes: a reply larger than that is held, and its first part sent
    /// as a `reply_part`; the rest are read with [`Lane::read_part`]. The reply keeps
    /// room in its reservation for one part's frame beside its text, and its place among the
    /// outstanding requests, until it is released.
    pub async fn submit_paged<W>(
        self: &Arc<Self>,
        id: Option<String>,
        slot: Option<String>,
        part_bytes: Option<usize>,
        reservation: usize,
        work: W,
    ) where
        W: FnOnce(&AtomicBool) -> Result<Event> + Send + 'static,
    {
        let Some(pending) = self.admit_pending(&id).await else {
            return;
        };
        let Some((seq, cancel)) = self.register(slot.as_deref()) else {
            return;
        };
        let lane = self.clone();
        tokio::spawn(async move {
            let room = part_bytes.map_or(0, |_| budget::REPLY_PART_ROOM);
            let outcome = lane.run(&cancel, reservation, room, work).await;
            match part_bytes {
                // Still in its slot, so that holding the reply and leaving the slot are one step
                // a newer request in the slot cannot come between.
                Some(part_bytes) => {
                    lane.deliver_paged(id, seq, &cancel, pending, outcome, slot, part_bytes)
                        .await;
                }
                None => {
                    lane.unregister(seq, slot.as_deref());
                    lane.deliver(id, &cancel, pending, outcome).await;
                }
            }
        });
    }

    /// Answers `read_reply_part`: the part of the held reply `handle` from `offset`, `length`
    /// bytes of it (the request's part size when absent; cut and raised to the bounds), as a
    /// `reply_part` on the reply queue. Parts are served in the order they were asked for, one at a
    /// time: a request waits for its turn while an earlier part of the same reply is still being
    /// written. The part reaching the end of the reply releases it. A reply no
    /// longer held is answered `reply_part_unavailable`, or `request_superseded` where a newer
    /// request in its slot released it.
    pub async fn read_part(
        self: &Arc<Self>,
        id: Option<String>,
        handle: String,
        offset: u64,
        length: Option<u64>,
    ) {
        let Some(pending) = self.admit_pending(&id).await else {
            return;
        };
        // The place in the queue is taken here, before anything is spawned, so that parts are
        // served in the order their frames came.
        let found = {
            let in_flight = self.in_flight.lock().expect("lane lock poisoned");
            if in_flight.closed {
                return;
            }
            match in_flight.held.get(&handle) {
                Some(held) => {
                    let (previous, turn) = held.shared.take_turn();
                    Ok((previous, turn, held.shared.clone()))
                }
                None => Err(in_flight.gone(id.clone(), &handle)),
            }
        };
        let (previous, turn, shared) = match found {
            Ok(found) => found,
            Err(gone) => {
                self.answer(gone, pending).await;
                return;
            }
        };
        let lane = self.clone();
        tokio::spawn(async move {
            if let Some(previous) = previous {
                tokio::select! {
                    _ = previous => {}
                    () = shared.released.cancelled() => {}
                }
            }
            let found = {
                let in_flight = lane.in_flight.lock().expect("lane lock poisoned");
                match in_flight.held.get(&handle) {
                    Some(held) => Ok((
                        held.text.clone(),
                        length.map_or(held.part_bytes, budget::reply_part_bytes),
                    )),
                    None => Err(in_flight.gone(id.clone(), &handle)),
                }
            };
            let (text, want) = match found {
                Ok(found) => found,
                Err(gone) => {
                    drop(turn);
                    lane.answer(gone, pending).await;
                    return;
                }
            };
            lane.send_part(id, &handle, &text, offset, want, turn, pending)
                .await;
        });
    }

    /// Builds the part of `text` from `offset` and puts it on the reply queue, releasing the reply
    /// when the part reaches its end; a part that cannot fit the reply's room is answered with its
    /// error.
    #[allow(clippy::too_many_arguments)]
    async fn send_part(
        &self,
        id: Option<String>,
        handle: &str,
        text: &str,
        offset: u64,
        want: usize,
        turn: PartTurn,
        pending: OwnedSemaphorePermit,
    ) {
        let room = turn.shared.room;
        let part = match reply_parts::part_frame(id.clone(), handle, text, offset, want, room) {
            Ok(part) => part,
            Err(err) => {
                drop(turn);
                self.answer(Event::from_error(id, &err), pending).await;
                return;
            }
        };
        if part.reaches_end {
            self.in_flight
                .lock()
                .expect("lane lock poisoned")
                .release(handle, false);
        }
        let frame = Frame {
            text: part.text,
            _retained: self.empty_share(),
            _connection: self.empty_share(),
            _pending: pending,
            _part: Some(turn),
        };
        let _ = self.content.send(frame).await;
    }

    /// A share of nothing, for a frame whose bytes another holder reserved.
    fn empty_share(&self) -> OwnedSemaphorePermit {
        self.retained
            .clone()
            .try_acquire_many_owned(0)
            .expect("a share of nothing is always granted")
    }

    /// [`Lane::deliver`] for a request that asked for parts of `part_bytes`, still registered as
    /// `seq` in its slot: a reply that fits one part is sent whole, with the room kept for a part
    /// given back; a larger one is held and its first part sent. Leaving the slot and holding the
    /// reply happen under one lock, so a newer request in the slot either cancelled this one before
    /// (it is answered `request_superseded`) or comes after and releases the held reply.
    #[allow(clippy::too_many_arguments)]
    async fn deliver_paged(
        self: &Arc<Self>,
        id: Option<String>,
        seq: u64,
        cancel: &Cancel,
        pending: OwnedSemaphorePermit,
        outcome: Result<Reply>,
        slot: Option<String>,
        part_bytes: usize,
    ) {
        let mut reply = match outcome {
            Ok(reply) if !cancel.is_cancelled() && reply.text.len() > part_bytes => reply,
            Ok(mut reply) => {
                self.unregister(seq, slot.as_deref());
                drop(reply.retained.split(reply.room));
                drop(reply.connection.split(reply.room));
                reply.room = 0;
                return self.deliver(id, cancel, pending, Ok(reply)).await;
            }
            outcome => {
                self.unregister(seq, slot.as_deref());
                return self.deliver(id, cancel, pending, outcome).await;
            }
        };
        let shared = Arc::new(HeldShared {
            room: std::mem::take(&mut reply.room),
            released: Cancel::default(),
            tail: Mutex::new(None),
            queued: AtomicUsize::new(0),
            expires: Mutex::new(tokio::time::Instant::now() + budget::HELD_REPLY_IDLE),
            _retained: reply.retained,
            _connection: reply.connection,
            _pending: pending,
        });
        let text = Arc::new(reply.text);
        let held = {
            let mut in_flight = self.in_flight.lock().expect("lane lock poisoned");
            in_flight.unregister(seq, slot.as_deref());
            if in_flight.closed || cancel.is_cancelled() {
                None
            } else {
                let handle = format!("reply-{}", in_flight.next_reply);
                in_flight.next_reply += 1;
                let (_, turn) = shared.take_turn();
                in_flight.held.insert(
                    handle.clone(),
                    HeldReply {
                        text: text.clone(),
                        slot,
                        part_bytes,
                        shared: shared.clone(),
                    },
                );
                Some((handle, turn))
            }
        };
        let Some((handle, turn)) = held else {
            let closed = self.in_flight.lock().expect("lane lock poisoned").closed;
            drop(text);
            // Its place among the outstanding requests goes with `shared`, so the refusal is
            // queued on its own; nobody is left to answer once the lane is closed.
            drop(shared);
            if !closed {
                let _ = self.control.send(superseded(id)).await;
            }
            return;
        };
        self.expire_when_idle(handle.clone(), Arc::downgrade(&shared));
        drop(shared);
        let pending = self.pending_share();
        self.send_part(id, &handle, &text, 0, part_bytes, turn, pending)
            .await;
    }

    /// A place among the outstanding requests that holds none, for the first part of a held
    /// reply: the reply itself holds the request's.
    fn pending_share(&self) -> OwnedSemaphorePermit {
        self.pending
            .clone()
            .try_acquire_many_owned(0)
            .expect("a share of nothing is always granted")
    }

    /// Releases the held reply `handle` once it has gone the lane's idle time with no part in
    /// flight since its latest was written; ends as soon as it is released otherwise.
    fn expire_when_idle(self: &Arc<Self>, handle: String, shared: std::sync::Weak<HeldShared>) {
        let lane = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(at) = shared
                    .upgrade()
                    .map(|shared| *shared.expires.lock().expect("held reply lock poisoned"))
                else {
                    return;
                };
                tokio::time::sleep_until(at).await;
                let Some(lane) = lane.upgrade() else {
                    return;
                };
                let mut in_flight = lane.in_flight.lock().expect("lane lock poisoned");
                let Some(held) = in_flight.held.get(&handle) else {
                    return;
                };
                let in_use = held.shared.queued.load(Ordering::Relaxed) > 0;
                let expires = *held
                    .shared
                    .expires
                    .lock()
                    .expect("held reply lock poisoned");
                if in_use {
                    // A part asked for or being written: its turn sets the expiry anew once
                    // written.
                    *held
                        .shared
                        .expires
                        .lock()
                        .expect("held reply lock poisoned") =
                        tokio::time::Instant::now() + budget::HELD_REPLY_IDLE;
                } else if expires <= tokio::time::Instant::now() {
                    in_flight.release(&handle, false);
                    return;
                }
            }
        });
    }

    /// Starts `work` for request `id` as [`Lane::submit_paged`] does, for a request answered with `ack`
    /// and followed by events: `work` sends the `ack` and those events through the [`Stream`] it is
    /// handed, and returns the last event. Every event after the `ack` is carved out of
    /// `reservation`, and keeps its share until its frame has been written, so `reservation` bounds
    /// all the bytes the request sends; the last event keeps the request's place among the
    /// outstanding ones until it is written too. A failure before the `ack` is answered as
    /// [`Lane::submit_paged`] answers one. Once the request is acknowledged its id has its answer, so
    /// nothing else is ever sent for it: a cancel ends it with nothing more sent, since a newer
    /// request has taken its place or nobody is left to read it, and `work` itself ends every
    /// other way it can stop with its last event (a failure it returns, or a panic, is only
    /// logged).
    pub async fn submit_stream<W>(
        self: &Arc<Self>,
        id: Option<String>,
        slot: Option<String>,
        reservation: usize,
        work: W,
    ) where
        W: FnOnce(&AtomicBool, &mut Stream) -> Result<Event> + Send + 'static,
    {
        let Some(pending) = self.admit_pending(&id).await else {
            return;
        };
        let Some((seq, cancel)) = self.register(slot.as_deref()) else {
            return;
        };
        let lane = self.clone();
        tokio::spawn(async move {
            let outcome = lane
                .run_stream(&cancel, id.clone(), reservation, work)
                .await;
            // Still in its slot while its last event waits for room, so that a newer request in
            // the slot, or the connection closing, cancels that wait too.
            lane.deliver_stream(id, &cancel, pending, outcome).await;
            lane.unregister(seq, slot.as_deref());
        });
    }

    /// A place among the connection's outstanding requests, or `None` once the refusal of one past
    /// the bound is on the control queue.
    async fn admit_pending(&self, id: &Option<String>) -> Option<OwnedSemaphorePermit> {
        if let Ok(pending) = self.pending.clone().try_acquire_owned() {
            return Some(pending);
        }
        let max = self.pending_max.to_string();
        let refusal = CodedError {
            code: error_code::LIMIT_EXCEEDED,
            message: "too many browse requests are outstanding on this connection".to_string(),
            params: [
                ("limit".to_string(), "pending_requests".to_string()),
                ("max".to_string(), max),
            ]
            .into(),
        }
        .reply(id.clone());
        let _ = self.control.send(refusal).await;
        None
    }

    /// Waits for `reservation` bytes from the connection and from the daemon, then for a read
    /// slot, giving up once cancelled. A reservation larger than either budget, which could never
    /// be granted, is cut to it.
    async fn admit(&self, cancel: &Cancel, reservation: usize) -> Result<Admitted> {
        let reservation = reservation
            .min(self.retained_max)
            .min(self.gate.retained_max);
        let connection = until_cancelled(
            cancel,
            self.retained.clone().acquire_many_owned(reservation as u32),
        )
        .await?;
        let retained = until_cancelled(
            cancel,
            self.gate
                .retained
                .clone()
                .acquire_many_owned(reservation as u32),
        )
        .await?;
        let reading = until_cancelled(cancel, self.gate.reads.clone().acquire_owned()).await?;
        Ok(Admitted {
            reservation,
            connection,
            retained,
            reading,
        })
    }

    /// [`Lane::run`] for a streamed request: the same reservations and read slot, with `work`
    /// handed a [`Stream`] over the reservation. The read slot is given back as soon as `work`
    /// returns, before the last event waits for room on the reply queue.
    async fn run_stream<W>(
        &self,
        cancel: &Arc<Cancel>,
        id: Option<String>,
        reservation: usize,
        work: W,
    ) -> Result<StreamEnd>
    where
        W: FnOnce(&AtomicBool, &mut Stream) -> Result<Event> + Send + 'static,
    {
        let admitted = self.admit(cancel, reservation).await?;
        let mut stream = Stream {
            id,
            acknowledged: false,
            connection: admitted.connection,
            retained: admitted.retained,
            reading: Some(admitted.reading),
            reads: self.gate.reads.clone(),
            cancel: cancel.clone(),
            runtime: tokio::runtime::Handle::current(),
            control: self.control.clone(),
            content: self.content.clone(),
        };
        let flag = cancel.clone();
        let end = tokio::task::spawn_blocking(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                work(&flag.flag, &mut stream)
            }))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("the streamed request panicked")));
            drop(stream.reading.take());
            let acknowledged = stream.acknowledged;
            let last = outcome.and_then(move |event| {
                let text = stream.frame_text(&event)?;
                Ok((text, stream))
            });
            StreamEnd { acknowledged, last }
        })
        .await?;
        Ok(end)
    }

    /// Sends the end of a streamed request back: its last event on the reply queue, or the failure
    /// of one not yet acknowledged as an `error` on the control queue, as [`Lane::deliver`] does.
    /// After the `ack` nothing but the last event is sent: a cancelled request ends silently, and
    /// a failure is logged.
    async fn deliver_stream(
        &self,
        id: Option<String>,
        cancel: &Cancel,
        pending: OwnedSemaphorePermit,
        outcome: Result<StreamEnd>,
    ) {
        let failure = match outcome {
            Ok(StreamEnd {
                last: Ok((text, stream)),
                ..
            }) if !cancel.is_cancelled() => {
                let frame = Frame {
                    text,
                    _retained: stream.retained,
                    _connection: stream.connection,
                    _pending: pending,
                    _part: None,
                };
                // Superseded while its last event waits for room, it sends nothing more. The wait
                // has no deadline of its own: a client that takes no frame for the connection's
                // write deadline is disconnected, which closes the queue and the lane.
                #[cfg(test)]
                self.last_event_waits.store(true, Ordering::Relaxed);
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => {}
                    _ = self.content.send(frame) => {}
                }
                return;
            }
            Ok(StreamEnd {
                acknowledged: true,
                last,
            }) => {
                if let Err(err) = last {
                    if err.downcast_ref::<Cancelled>().is_none() {
                        tracing::error!(error = %format!("{err:#}"), "an acknowledged stream failed");
                    }
                }
                return;
            }
            Ok(StreamEnd { last: Ok(_), .. }) => superseded(id),
            Ok(StreamEnd { last: Err(err), .. }) | Err(err) => failure_of(id, &err),
        };
        self.answer(failure, pending).await;
    }

    /// Records a new request, cancelling whatever held its slot; `None` once the lane is closed.
    fn register(&self, slot: Option<&str>) -> Option<(u64, Arc<Cancel>)> {
        let mut in_flight = self.in_flight.lock().expect("lane lock poisoned");
        if in_flight.closed {
            return None;
        }
        let seq = in_flight.next;
        in_flight.next += 1;
        let cancel = Arc::new(Cancel::default());
        in_flight.requests.insert(seq, cancel.clone());
        if let Some(slot) = slot {
            if let Some(previous) = in_flight.slots.insert(slot.to_string(), seq) {
                if let Some(previous) = in_flight.requests.get(&previous) {
                    previous.cancel();
                }
            }
            let released: Vec<String> = in_flight
                .held
                .iter()
                .filter(|(_, held)| held.slot.as_deref() == Some(slot))
                .map(|(handle, _)| handle.clone())
                .collect();
            for handle in released {
                in_flight.release(&handle, true);
            }
        }
        Some((seq, cancel))
    }

    fn unregister(&self, seq: u64, slot: Option<&str>) {
        self.in_flight
            .lock()
            .expect("lane lock poisoned")
            .unregister(seq, slot);
    }

    /// Cancels everything outstanding and refuses anything submitted later: the connection has
    /// gone, and nobody is left to read a reply.
    pub fn close(&self) {
        let mut in_flight = self.in_flight.lock().expect("lane lock poisoned");
        in_flight.closed = true;
        for cancel in in_flight.requests.values() {
            cancel.cancel();
        }
        let held: Vec<String> = in_flight.held.keys().cloned().collect();
        for handle in held {
            in_flight.release(&handle, false);
        }
    }

    /// Waits for a reservation from the connection and from the daemon and for a read slot, then
    /// runs `work` on a blocking thread and serializes its reply into a buffer of exactly its
    /// size, shrinking both reservations to it. Gives up at any point once cancelled. A reply
    /// that would not fit its reservation is refused rather than held past the budget.
    ///
    /// `keep` bytes of what the reply does not need, at most, stay reserved beside it, as the room
    /// a reply held for parts sends each part's frame from.
    async fn run<W>(
        &self,
        cancel: &Arc<Cancel>,
        reservation: usize,
        keep: usize,
        work: W,
    ) -> Result<Reply>
    where
        W: FnOnce(&AtomicBool) -> Result<Event> + Send + 'static,
    {
        let Admitted {
            reservation,
            mut connection,
            mut retained,
            reading,
        } = self.admit(cancel, reservation).await?;
        let flag = cancel.clone();
        let text = tokio::task::spawn_blocking(move || {
            let _reading = reading;
            let event = work(&flag.flag)?;
            let mut counter = Counter(0);
            serde_json::to_writer(&mut counter, &event)?;
            if counter.0 > reservation {
                let max = reservation.to_string();
                return Err(CodedError::raised(
                    error_code::LIMIT_EXCEEDED,
                    format!("the reply would be larger than the {reservation}-byte limit"),
                    &[("limit", "reply_bytes"), ("max", &max)],
                ));
            }
            let mut buffer = Vec::with_capacity(counter.0);
            serde_json::to_writer(&mut buffer, &event)?;
            drop(event);
            Ok(String::from_utf8(buffer).expect("JSON is UTF-8"))
        })
        .await??;
        let unused = reservation.saturating_sub(text.len());
        let room = keep.min(unused);
        drop(connection.split(unused - room));
        drop(retained.split(unused - room));
        Ok(Reply {
            text,
            retained,
            connection,
            room,
        })
    }

    /// Sends the outcome of request `id` back: its frame on the reply queue, or its failure as an
    /// `error` on the control queue. A cancelled request is answered `request_superseded`, unless
    /// the lane is closed and there is nobody to answer. The request keeps its place among the
    /// outstanding ones (`pending`) until its answer is queued — a reply's until it is written —
    /// so a client that never reads its socket cannot pile up answers behind the bound.
    async fn deliver(
        &self,
        id: Option<String>,
        cancel: &Cancel,
        pending: OwnedSemaphorePermit,
        outcome: Result<Reply>,
    ) {
        let failure = match outcome {
            Ok(reply) if !cancel.is_cancelled() => {
                let frame = Frame {
                    text: reply.text,
                    _retained: reply.retained,
                    _connection: reply.connection,
                    _pending: pending,
                    _part: None,
                };
                let _ = self.content.send(frame).await;
                return;
            }
            Ok(reply) => {
                // Give its reserved bytes back before waiting for room on the control queue.
                drop(reply);
                superseded(id)
            }
            Err(err) => failure_of(id, &err),
        };
        self.answer(failure, pending).await;
    }

    /// Puts `failure` on the control queue, unless the lane is closed and there is nobody to
    /// answer; the request keeps its place among the outstanding ones (`pending`) until then.
    async fn answer(&self, failure: Event, pending: OwnedSemaphorePermit) {
        if self.in_flight.lock().expect("lane lock poisoned").closed {
            return;
        }
        let _ = self.control.send(failure).await;
        drop(pending);
    }
}

/// The reservations and the read slot a request runs under.
struct Admitted {
    reservation: usize,
    connection: OwnedSemaphorePermit,
    retained: OwnedSemaphorePermit,
    reading: OwnedSemaphorePermit,
}

/// `wait`'s permit, or [`Cancelled`] once `cancel` is.
async fn until_cancelled(
    cancel: &Cancel,
    wait: impl std::future::Future<Output = Result<OwnedSemaphorePermit, tokio::sync::AcquireError>>,
) -> Result<OwnedSemaphorePermit> {
    tokio::select! {
        permit = wait => Ok(permit.expect("the lane's and the gate's semaphores are never closed")),
        () = cancel.cancelled() => Err(anyhow::Error::new(Cancelled)),
    }
}

/// The answer to a request that failed with `err`: `request_superseded` for a cancel, otherwise
/// its own error.
fn failure_of(id: Option<String>, err: &anyhow::Error) -> Event {
    if err.downcast_ref::<Cancelled>().is_some() {
        superseded(id)
    } else {
        Event::from_error(id, err)
    }
}

/// How a streamed request ended: whether it had been acknowledged, and its last event serialized
/// with what is left of its reservation, or why it failed.
struct StreamEnd {
    acknowledged: bool,
    last: Result<(String, Stream)>,
}

/// What became of an event handed to [`Stream::send`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    /// Not sent: it would not leave the room asked for in the reservation.
    TooLarge,
    /// Not sent: the deadline passed while the reply queue had no room for it.
    OutOfTime,
}

/// What a streamed request sends through while it runs: its `ack`, then its events, each holding a
/// share of the request's reservation until its frame has been written. Used from the blocking
/// thread the request runs on, which holds a read slot except while it waits for room on the reply
/// queue.
#[derive(Debug)]
pub struct Stream {
    id: Option<String>,
    acknowledged: bool,
    connection: OwnedSemaphorePermit,
    retained: OwnedSemaphorePermit,
    reading: Option<OwnedSemaphorePermit>,
    reads: Arc<Semaphore>,
    cancel: Arc<Cancel>,
    runtime: tokio::runtime::Handle,
    control: mpsc::Sender<Event>,
    content: mpsc::Sender<Frame>,
}

impl Stream {
    /// Answers the request with `ack`, on the control queue, which the connection's writer empties
    /// before the reply queue: the `ack` is written before any event sent after it.
    pub fn ack(&mut self) -> Result<()> {
        if self.acknowledged {
            return Ok(());
        }
        self.control
            .blocking_send(Event::Ack {
                id: self.id.clone(),
            })
            .map_err(|_| anyhow::Error::new(Cancelled))?;
        self.acknowledged = true;
        Ok(())
    }

    /// The bytes of the reservation not yet carved out for an event.
    pub fn remaining(&self) -> usize {
        self.connection.num_permits()
    }

    /// Sends `event` on the reply queue, with a share of the reservation of its serialized size.
    /// [`SendOutcome::TooLarge`] leaves it unsent: it would not leave `keep` bytes of the reservation
    /// behind it. While the queue is full it waits for room, with the read slot given back so that
    /// a client slow to read holds up no other read, until `deadline` ([`SendOutcome::OutOfTime`]) or a
    /// cancel: a request cancelled, before or during the wait, sends nothing more and is
    /// [`Cancelled`], as is one whose connection has gone. `readers` is told `true` before the slot
    /// is given back and `false` once it is taken back, or the wait ends without it: the request's
    /// reading threads pause in between, so that nothing reads without the slot.
    pub fn send(
        &mut self,
        event: &Event,
        keep: usize,
        deadline: Instant,
        readers: &mut dyn FnMut(bool),
    ) -> Result<SendOutcome> {
        if self.cancel.is_cancelled() {
            return Err(anyhow::Error::new(Cancelled));
        }
        let mut counter = Counter(0);
        serde_json::to_writer(&mut counter, event)?;
        if counter.0.saturating_add(keep) > self.remaining() {
            return Ok(SendOutcome::TooLarge);
        }
        let room = match self.content.clone().try_reserve_owned() {
            Ok(room) => room,
            Err(mpsc::error::TrySendError::Closed(_)) => return Err(anyhow::Error::new(Cancelled)),
            Err(mpsc::error::TrySendError::Full(_)) => {
                match self.wait_for_room(deadline, readers)? {
                    Some(room) => room,
                    None => return Ok(SendOutcome::OutOfTime),
                }
            }
        };
        // Superseded while it waited: room found as the cancel came is not used.
        if self.cancel.is_cancelled() {
            return Err(anyhow::Error::new(Cancelled));
        }
        let mut buffer = Vec::with_capacity(counter.0);
        serde_json::to_writer(&mut buffer, event)?;
        let size = buffer.len();
        room.send(Frame {
            text: String::from_utf8(buffer).expect("JSON is UTF-8"),
            _retained: self
                .retained
                .split(size)
                .expect("checked against the reservation"),
            _connection: self
                .connection
                .split(size)
                .expect("checked against the reservation"),
            _pending: self
                .connection
                .split(0)
                .expect("an empty share always splits"),
            _part: None,
        });
        Ok(SendOutcome::Sent)
    }

    /// Waits for room on the reply queue without the read slot, then takes the slot back: the room,
    /// or `None` once `deadline` passes first. Room found with the slot still taken at the deadline
    /// is used without it, the request being about to stop at its time anyway.
    fn wait_for_room(
        &mut self,
        deadline: Instant,
        readers: &mut dyn FnMut(bool),
    ) -> Result<Option<mpsc::OwnedPermit<Frame>>> {
        readers(true);
        drop(self.reading.take());
        let cancel = self.cancel.clone();
        let content = self.content.clone();
        let reads = self.reads.clone();
        let deadline = tokio::time::Instant::from_std(deadline);
        let waited = self.runtime.block_on(async move {
            let room = tokio::select! {
                room = content.reserve_owned() => {
                    room.map_err(|_| anyhow::Error::new(Cancelled))?
                }
                () = cancel.cancelled() => return Err(anyhow::Error::new(Cancelled)),
                () = tokio::time::sleep_until(deadline) => return Ok((None, None)),
            };
            let reading = tokio::select! {
                permit = reads.acquire_owned() => {
                    Some(permit.expect("the gate's semaphores are never closed"))
                }
                () = cancel.cancelled() => return Err(anyhow::Error::new(Cancelled)),
                () = tokio::time::sleep_until(deadline) => None,
            };
            Ok((Some(room), reading))
        });
        let (room, reading) = match waited {
            Ok(waited) => waited,
            Err(err) => {
                readers(false);
                return Err(err);
            }
        };
        self.reading = reading;
        readers(false);
        Ok(room)
    }

    /// `event` serialized, as the last event, which keeps the rest of the reservation: refused as
    /// a reply past its reservation is when it does not fit.
    fn frame_text(&self, event: &Event) -> Result<String> {
        let text = serde_json::to_string(event)?;
        if text.len() > self.remaining() {
            let max = self.remaining().to_string();
            return Err(CodedError::raised(
                error_code::LIMIT_EXCEEDED,
                "the last event would be larger than what is left of the reservation",
                &[("limit", "reply_bytes"), ("max", &max)],
            ));
        }
        Ok(text)
    }
}

/// A serialized reply with its shares of the retained bytes, before it becomes a [`Frame`].
struct Reply {
    text: String,
    retained: OwnedSemaphorePermit,
    connection: OwnedSemaphorePermit,
    /// The bytes of the shares kept beside the text as room for one part's frame.
    room: usize,
}

fn superseded(id: Option<String>) -> Event {
    CodedError {
        code: error_code::REQUEST_SUPERSEDED,
        message: "a newer request took this one's place".to_string(),
        params: Default::default(),
    }
    .reply(id)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    use super::*;
    use crate::test_support::PATIENCE;

    fn lane(
        gate: Gate,
        pending: usize,
    ) -> (Arc<Lane>, mpsc::Receiver<Event>, mpsc::Receiver<Frame>) {
        let (control_tx, control_rx) = mpsc::channel(64);
        let (content_tx, content_rx) = mpsc::channel(64);
        let lane = Lane::new(Arc::new(gate), pending, 1 << 20, control_tx, content_tx);
        (Arc::new(lane), control_rx, content_rx)
    }

    fn ack(id: &str) -> Event {
        Event::Ack {
            id: Some(id.to_string()),
        }
    }

    /// A work item that blocks until `release` is set, then answers `ack`.
    fn held(
        release: Arc<AtomicBool>,
        id: &'static str,
    ) -> impl FnOnce(&AtomicBool) -> Result<Event> {
        move |cancel| {
            while !release.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    return Err(anyhow::Error::new(Cancelled));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(ack(id))
        }
    }

    async fn next<T>(rx: &mut mpsc::Receiver<T>) -> T {
        tokio::time::timeout(PATIENCE, rx.recv())
            .await
            .expect("an answer in time")
            .expect("the channel is open")
    }

    fn code_of(event: &Event) -> Option<crate::protocol::Code> {
        match event {
            Event::Error { code, .. } => Some(*code),
            _ => None,
        }
    }

    #[tokio::test]
    async fn a_request_past_the_pending_bound_is_refused_at_once() {
        let (lane, mut control, _content) = lane(Gate::new(4, 1 << 20), 2);
        let release = Arc::new(AtomicBool::new(false));
        lane.submit(Some("a".into()), None, 1024, held(release.clone(), "a"))
            .await;
        lane.submit(Some("b".into()), None, 1024, held(release.clone(), "b"))
            .await;
        lane.submit(Some("c".into()), None, 1024, held(release.clone(), "c"))
            .await;
        let refusal = next(&mut control).await;
        assert_eq!(code_of(&refusal), Some(error_code::LIMIT_EXCEEDED));
        release.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn reads_past_the_concurrency_bound_wait_and_then_run() {
        let gate = Arc::new(Gate::new(1, 1 << 20));
        let reading = gate.reads.clone().try_acquire_owned().unwrap();
        let (control, _control_rx) = mpsc::channel(8);
        let (content, mut reply) = mpsc::channel(8);
        let lane = Arc::new(Lane::new(gate, 8, 1 << 20, control, content));
        let started = Arc::new(AtomicBool::new(false));
        let second_started = started.clone();
        lane.submit(Some("read".into()), None, 1024, move |_| {
            second_started.store(true, Ordering::Relaxed);
            Ok(ack("read"))
        })
        .await;
        crate::test_support::runtime_settled().await;
        assert!(!started.load(Ordering::Relaxed), "ran past the bound");
        drop(reading);
        assert!(next(&mut reply).await.text.contains("\"read\""));
        assert!(started.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn retained_bytes_are_held_until_the_frame_is_written_and_shrink_to_its_size() {
        let gate = Gate::new(4, 1000);
        let retained = gate.retained.clone();
        let (lane, _control, mut content) = lane(gate, 8);
        lane.submit(Some("a".into()), None, 600, |_| Ok(ack("a")))
            .await;
        let frame = next(&mut content).await;
        assert_eq!(retained.available_permits(), 1000 - frame.text.len());
        assert_eq!(
            lane.pending.available_permits(),
            7,
            "the frame keeps its request's place"
        );
        drop(frame);
        assert_eq!(retained.available_permits(), 1000);
        assert_eq!(lane.pending.available_permits(), 8);
    }

    #[tokio::test]
    async fn a_reply_larger_than_its_reservation_is_refused_rather_than_held() {
        let (lane, mut control, _content) = lane(Gate::new(4, 1000), 8);
        lane.submit(Some("big".into()), None, 100, |_| Ok(ack(&"x".repeat(200))))
            .await;
        assert_eq!(
            code_of(&next(&mut control).await),
            Some(error_code::LIMIT_EXCEEDED)
        );
    }

    #[tokio::test]
    async fn a_newer_request_in_a_slot_supersedes_the_older_one() {
        let (lane, mut control, mut content) = lane(Gate::new(4, 1 << 20), 8);
        let never = Arc::new(AtomicBool::new(false));
        lane.submit(
            Some("old".into()),
            Some("viewer".into()),
            1024,
            held(never, "old"),
        )
        .await;
        lane.submit(Some("new".into()), Some("viewer".into()), 1024, |_| {
            Ok(ack("new"))
        })
        .await;
        let superseded = next(&mut control).await;
        assert_eq!(code_of(&superseded), Some(error_code::REQUEST_SUPERSEDED));
        assert!(matches!(superseded, Event::Error { id: Some(ref id), .. } if id == "old"));
        assert!(next(&mut content).await.text.contains("\"new\""));
    }

    #[tokio::test]
    async fn closing_the_lane_cancels_what_it_had_outstanding() {
        let (lane, _control, _content) = lane(Gate::new(1, 1 << 20), 8);
        let saw_cancel = Arc::new(AtomicBool::new(false));
        let saw = saw_cancel.clone();
        let started = Arc::new(AtomicBool::new(false));
        let starting = started.clone();
        lane.submit(None, None, 1024, move |cancel| {
            starting.store(true, Ordering::Relaxed);
            while !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(2));
            }
            saw.store(true, Ordering::Relaxed);
            Err(anyhow::Error::new(Cancelled))
        })
        .await;
        // Closed once the read runs: a read the close came before would never start to see it.
        let running =
            crate::test_support::within(PATIENCE, move || started.load(Ordering::Relaxed)).await;
        assert!(running, "the read never started");
        lane.close();
        let deadline = std::time::Instant::now() + PATIENCE;
        while !saw_cancel.load(Ordering::Relaxed) {
            assert!(
                std::time::Instant::now() < deadline,
                "the read never saw the cancel"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn a_failure_keeps_its_place_among_the_outstanding_until_it_is_queued() {
        let (control_tx, mut control) = mpsc::channel(1);
        let (content_tx, _content) = mpsc::channel(8);
        let lane = Arc::new(Lane::new(
            Arc::new(Gate::new(4, 1 << 20)),
            2,
            1 << 20,
            control_tx.clone(),
            content_tx,
        ));
        // The control queue is full and nobody reads it.
        control_tx.send(ack("filler")).await.unwrap();
        lane.submit(Some("failing".into()), None, 1024, |_| {
            Err(anyhow::anyhow!("fails at once"))
        })
        .await;
        crate::test_support::runtime_settled().await;
        assert_eq!(
            lane.pending.available_permits(),
            1,
            "an unqueued failure gave its place up"
        );
        next(&mut control).await;
        next(&mut control).await;
        let deadline = std::time::Instant::now() + PATIENCE;
        while lane.pending.available_permits() != 2 {
            assert!(
                std::time::Instant::now() < deadline,
                "the place was never given back"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// A deadline no test reaches.
    fn later() -> Instant {
        Instant::now() + PATIENCE
    }

    fn batch(text: &str) -> Event {
        Event::ProjectContentMatches {
            search: "s".into(),
            project: text.into(),
            matches: Vec::new(),
        }
    }

    #[tokio::test]
    async fn a_stream_is_acknowledged_before_its_events_and_ends_with_its_last() {
        let (lane, mut control, mut content) = lane(Gate::new(4, 1 << 20), 8);
        lane.submit_stream(Some("search".into()), None, 4096, |_, stream| {
            stream.ack()?;
            assert_eq!(
                stream.send(&batch("first"), 0, later(), &mut |_| {})?,
                SendOutcome::Sent
            );
            assert_eq!(
                stream.send(&batch("second"), 0, later(), &mut |_| {})?,
                SendOutcome::Sent
            );
            Ok(batch("last"))
        })
        .await;
        assert!(
            matches!(next(&mut control).await, Event::Ack { id: Some(ref id) } if id == "search")
        );
        for expected in ["first", "second", "last"] {
            assert!(next(&mut content).await.text.contains(expected));
        }
    }

    #[tokio::test]
    async fn a_stream_holds_its_events_within_its_reservation() {
        let gate = Gate::new(4, 1 << 20);
        let retained = gate.retained.clone();
        let (lane, _control, mut content) = lane(gate, 8);
        lane.submit_stream(None, None, 300, |_, stream| {
            stream.ack()?;
            let event = batch("held");
            let size = serde_json::to_string(&event)?.len();
            assert_eq!(
                stream.send(&event, 0, later(), &mut |_| {})?,
                SendOutcome::Sent
            );
            assert_eq!(stream.remaining(), 300 - size);
            // One that would not leave the room asked for is not sent.
            assert_eq!(
                stream.send(&batch("refused"), 300 - size, later(), &mut |_| {})?,
                SendOutcome::TooLarge
            );
            Ok(batch("last"))
        })
        .await;
        let first = next(&mut content).await;
        let last = next(&mut content).await;
        assert!(first.text.contains("held") && last.text.contains("last"));
        assert!(retained.available_permits() <= (1 << 20) - first.text.len());
        drop((first, last));
        assert_eq!(retained.available_permits(), 1 << 20);
    }

    #[tokio::test]
    async fn a_stream_failing_before_its_ack_is_answered_with_its_error() {
        let (lane, mut control, _content) = lane(Gate::new(4, 1 << 20), 8);
        lane.submit_stream(Some("bad".into()), None, 4096, |_, _| {
            Err(CodedError::raised(
                error_code::LIMIT_EXCEEDED,
                "refused",
                &[],
            ))
        })
        .await;
        let answer = next(&mut control).await;
        assert_eq!(code_of(&answer), Some(error_code::LIMIT_EXCEEDED));
        assert!(matches!(answer, Event::Error { id: Some(ref id), .. } if id == "bad"));
    }

    #[tokio::test]
    async fn a_superseded_stream_stops_and_sends_nothing_more() {
        let (lane, mut control, mut content) = lane(Gate::new(4, 1 << 20), 8);
        let saw_cancel = Arc::new(AtomicBool::new(false));
        let saw = saw_cancel.clone();
        lane.submit_stream(
            Some("old".into()),
            Some("find".into()),
            4096,
            move |cancel, stream| {
                stream.ack()?;
                stream.send(&batch("old batch"), 0, later(), &mut |_| {})?;
                if crate::session::wait::wait_for(PATIENCE, || cancel.load(Ordering::Relaxed)) {
                    saw.store(true, Ordering::Relaxed);
                }
                Ok(batch("old last"))
            },
        )
        .await;
        assert!(matches!(next(&mut control).await, Event::Ack { id: Some(ref id) } if id == "old"));
        assert!(next(&mut content).await.text.contains("old batch"));
        lane.submit_stream(
            Some("new".into()),
            Some("find".into()),
            4096,
            |_, stream| {
                stream.ack()?;
                Ok(batch("new last"))
            },
        )
        .await;
        assert!(matches!(next(&mut control).await, Event::Ack { id: Some(ref id) } if id == "new"));
        assert!(next(&mut content).await.text.contains("new last"));
        let stopped =
            crate::test_support::within(PATIENCE, move || saw_cancel.load(Ordering::Relaxed)).await;
        assert!(stopped, "the older search never saw the cancel");
        // Both requests have given their places back once each has been delivered.
        let watched = lane.clone();
        let delivered =
            crate::test_support::within(PATIENCE, move || watched.pending.available_permits() == 8)
                .await;
        assert!(delivered, "the older search never ended");
        assert!(
            control.try_recv().is_err(),
            "the older search was answered again"
        );
        assert!(
            content.try_recv().is_err(),
            "the older search sent its last event"
        );
    }

    /// A lane with one read slot and room for one reply on the queue, which the test reads.
    fn narrow_lane() -> (
        Arc<Lane>,
        Arc<Semaphore>,
        mpsc::Receiver<Event>,
        mpsc::Receiver<Frame>,
    ) {
        let gate = Arc::new(Gate::new(1, 1 << 20));
        let reads = gate.reads.clone();
        let (control_tx, control_rx) = mpsc::channel(8);
        let (content_tx, content_rx) = mpsc::channel(1);
        let lane = Arc::new(Lane::new(gate, 8, 1 << 20, control_tx, content_tx));
        (lane, reads, control_rx, content_rx)
    }

    #[tokio::test]
    async fn a_stream_waiting_for_room_gives_its_read_slot_back_and_stops_when_superseded() {
        let (lane, reads, mut control, mut content) = narrow_lane();
        // What the stream told its readers, in order.
        let told = Arc::new(Mutex::new(Vec::new()));
        let tells = told.clone();
        lane.submit_stream(
            Some("old".into()),
            Some("find".into()),
            4096,
            move |_, stream| {
                stream.ack()?;
                let mut readers = |paused| tells.lock().unwrap().push(paused);
                assert_eq!(
                    stream.send(&batch("fills the queue"), 0, later(), &mut readers)?,
                    SendOutcome::Sent
                );
                stream.send(&batch("waits for room"), 0, later(), &mut readers)?;
                Ok(batch("old last"))
            },
        )
        .await;
        assert!(matches!(next(&mut control).await, Event::Ack { id: Some(ref id) } if id == "old"));
        let watched = reads.clone();
        let paused = told.clone();
        let given_back = crate::test_support::within(PATIENCE, move || {
            *paused.lock().unwrap() == [true] && watched.available_permits() == 1
        })
        .await;
        assert!(
            given_back,
            "the waiting stream kept its read slot, or its readers on"
        );
        // The newer search runs on the slot the older one gave back.
        lane.submit_stream(
            Some("new".into()),
            Some("find".into()),
            4096,
            |_, stream| {
                stream.ack()?;
                Ok(batch("new last"))
            },
        )
        .await;
        assert!(matches!(next(&mut control).await, Event::Ack { id: Some(ref id) } if id == "new"));
        assert!(next(&mut content).await.text.contains("fills the queue"));
        assert!(next(&mut content).await.text.contains("new last"));
        let watched = lane.clone();
        let delivered =
            crate::test_support::within(PATIENCE, move || watched.pending.available_permits() == 8)
                .await;
        assert!(delivered, "the older search never ended");
        // Its readers were let go on once the wait ended, to see the cancel.
        assert_eq!(*told.lock().unwrap(), [true, false]);
        assert!(content.try_recv().is_err(), "the older search sent more");
        assert!(
            control.try_recv().is_err(),
            "the older search was answered again"
        );
    }

    #[tokio::test]
    async fn a_last_event_waiting_for_room_is_not_sent_once_superseded() {
        let (lane, _reads, mut control, mut content) = narrow_lane();
        lane.submit_stream(
            Some("old".into()),
            Some("find".into()),
            4096,
            move |_, stream| {
                stream.ack()?;
                stream.send(&batch("fills the queue"), 0, later(), &mut |_| {})?;
                Ok(batch("old last"))
            },
        )
        .await;
        assert!(matches!(next(&mut control).await, Event::Ack { .. }));
        // The older stream's last event waits behind the full queue.
        let watched = lane.clone();
        let waiting = crate::test_support::within(PATIENCE, move || {
            watched.last_event_waits.load(Ordering::Relaxed)
        })
        .await;
        assert!(
            waiting,
            "the older stream's last event never waited for room"
        );
        lane.submit_stream(
            Some("new".into()),
            Some("find".into()),
            4096,
            |_, stream| {
                stream.ack()?;
                Ok(batch("new last"))
            },
        )
        .await;
        assert!(next(&mut content).await.text.contains("fills the queue"));
        assert!(next(&mut content).await.text.contains("new last"));
        let watched = lane.clone();
        let ended =
            crate::test_support::within(PATIENCE, move || watched.pending.available_permits() == 8)
                .await;
        assert!(ended, "the older stream never ended");
        assert!(
            content.try_recv().is_err(),
            "the older stream sent its last event"
        );
    }

    #[tokio::test]
    async fn a_stream_whose_deadline_passes_while_it_waits_for_room_is_told_so() {
        let (lane, _reads, mut control, mut content) = narrow_lane();
        let told = Arc::new(Mutex::new(None));
        let tells = told.clone();
        lane.submit_stream(Some("late".into()), None, 4096, move |_, stream| {
            stream.ack()?;
            stream.send(&batch("fills the queue"), 0, later(), &mut |_| {})?;
            let mut paused = Vec::new();
            let late = stream.send(&batch("too late"), 0, Instant::now(), &mut |yes| {
                paused.push(yes)
            })?;
            assert_eq!(paused, [true, false]);
            *tells.lock().unwrap() = Some(late);
            Ok(batch("last"))
        })
        .await;
        assert!(matches!(next(&mut control).await, Event::Ack { .. }));
        // The queue is read only once the stream has been told, so it stays full until then.
        let watched = told.clone();
        let answered =
            crate::test_support::within(PATIENCE, move || watched.lock().unwrap().is_some()).await;
        assert!(answered, "the stream was never told");
        assert_eq!(*told.lock().unwrap(), Some(SendOutcome::OutOfTime));
        assert!(next(&mut content).await.text.contains("fills the queue"));
        assert!(next(&mut content).await.text.contains("last"));
    }

    #[tokio::test]
    async fn a_stream_failing_after_its_ack_sends_nothing_more() {
        let (lane, mut control, mut content) = lane(Gate::new(4, 1 << 20), 8);
        lane.submit_stream(Some("failing".into()), None, 4096, |_, stream| {
            stream.ack()?;
            panic!("a walk thread panicked");
        })
        .await;
        assert!(matches!(next(&mut control).await, Event::Ack { .. }));
        let watched = lane.clone();
        let ended =
            crate::test_support::within(PATIENCE, move || watched.pending.available_permits() == 8)
                .await;
        assert!(ended, "the stream never ended");
        assert!(control.try_recv().is_err(), "an error followed the ack");
        assert!(content.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn the_mcp_tools_share_one_connections_worth_of_the_retained_bytes() {
        let gate = Gate::new(4, 3000).with_tools_share(1000);
        let first = gate.admit(600).await;
        // A second tool read does not fit the share although the gate has room for it: on the
        // paused clock the timeout ends only once nothing more can run.
        let blocked = tokio::time::timeout(Duration::from_millis(50), gate.admit(600)).await;
        assert!(blocked.is_err());
        assert_eq!(gate.retained_available(), 2400);
        // And a window's read still gets the rest of the gate.
        let window = gate.retained.clone().try_acquire_many_owned(2400);
        assert!(window.is_ok());
        drop(window);
        drop(first);
        let second = tokio::time::timeout(PATIENCE, gate.admit(600)).await;
        assert!(second.is_ok());
    }

    /// What the tests of replies read in parts need of a lane: both of its queues, and the gate
    /// whose retained bytes they watch.
    struct Paged {
        lane: Arc<Lane>,
        gate: Arc<Gate>,
        control: mpsc::Receiver<Event>,
        content: mpsc::Receiver<Frame>,
    }

    /// The retained bytes the gate and the lane of a paged test each hold.
    const ROOMY: usize = 8 << 20;

    /// What a request in a paged test reserves: more than the largest reply it makes and the room
    /// a held reply keeps beside it.
    const RESERVE: usize = 4 << 20;

    fn paged(pending: usize) -> Paged {
        let gate = Arc::new(Gate::new(4, ROOMY));
        let (control_tx, control) = mpsc::channel(64);
        let (content_tx, content) = mpsc::channel(64);
        let lane = Arc::new(Lane::new(
            gate.clone(),
            pending,
            ROOMY,
            control_tx,
            content_tx,
        ));
        Paged {
            lane,
            gate,
            control,
            content,
        }
    }

    /// A work item whose reply serializes to `n` bytes more than an empty `ack` does.
    fn big(n: usize) -> impl FnOnce(&AtomicBool) -> Result<Event> + Send + 'static {
        move |_| Ok(ack(&"x".repeat(n)))
    }

    /// The bytes a reply made of `big(n)` serializes to.
    fn overhead() -> usize {
        serde_json::to_string(&ack("")).unwrap().len()
    }

    /// The text of `big(n)`'s reply sent whole by a plain submit.
    async fn whole(n: usize) -> String {
        let mut p = paged(8);
        p.lane
            .submit(Some("plain".into()), None, RESERVE, big(n))
            .await;
        next(&mut p.content).await.text
    }

    fn json_of(frame: &Frame) -> serde_json::Value {
        serde_json::from_str(&frame.text).expect("a frame is JSON")
    }

    fn data_of(frame: &serde_json::Value) -> Vec<u8> {
        STANDARD
            .decode(frame["data"].as_str().expect("data is text"))
            .expect("data is base64")
    }

    fn handle_of(frame: &serde_json::Value) -> String {
        frame["reply"].as_str().expect("a reply handle").to_string()
    }

    fn param(event: &Event, name: &str) -> Option<String> {
        match event {
            Event::Error { params, .. } => params.get(name).cloned(),
            _ => None,
        }
    }

    fn error_id(event: &Event) -> Option<String> {
        match event {
            Event::Error { id, .. } => id.clone(),
            _ => None,
        }
    }

    /// The first part of a reply of `n` extra bytes asked for in parts of `part_bytes` under
    /// `slot`, as a frame still unwritten, and the reply's handle.
    async fn first_part(
        p: &mut Paged,
        slot: Option<&str>,
        part_bytes: usize,
        n: usize,
    ) -> (Frame, String) {
        p.lane
            .submit_paged(
                Some("ask".into()),
                slot.map(str::to_string),
                Some(part_bytes),
                RESERVE,
                big(n),
            )
            .await;
        let frame = next(&mut p.content).await;
        let handle = handle_of(&json_of(&frame));
        (frame, handle)
    }

    /// Asks for the part of `handle` from `offset` as request `id` and takes its frame.
    async fn part_at(
        p: &mut Paged,
        id: &str,
        handle: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Frame {
        p.lane
            .read_part(Some(id.into()), handle.to_string(), offset, length)
            .await;
        next(&mut p.content).await
    }

    /// Reads every part after the first, writing each before asking for the next; returns the
    /// bytes of the reply from `offset` on, with each part's length.
    async fn read_to_end(
        p: &mut Paged,
        handle: &str,
        mut offset: u64,
        total: u64,
        lengths: &[Option<u64>],
    ) -> (Vec<u8>, Vec<usize>) {
        let mut joined = Vec::new();
        let mut sizes = Vec::new();
        let mut round = 0;
        while offset < total {
            let id = format!("read-{round}");
            let frame = part_at(p, &id, handle, offset, lengths[round % lengths.len()]).await;
            let json = json_of(&frame);
            assert_eq!(json["type"], "reply_part");
            assert_eq!(json["id"], id.as_str());
            assert_eq!(json["reply"], handle);
            assert_eq!(json["offset"], offset);
            assert_eq!(json["total"], total);
            let data = data_of(&json);
            assert!(!data.is_empty(), "a part before the end carries bytes");
            offset += data.len() as u64;
            sizes.push(data.len());
            joined.extend(data);
            drop(frame);
            round += 1;
        }
        (joined, sizes)
    }

    #[tokio::test(start_paused = true)]
    async fn a_reply_that_fits_the_part_size_is_sent_whole_and_keeps_nothing_extra() {
        let mut plain = paged(8);
        plain
            .lane
            .submit(Some("a".into()), None, RESERVE, big(100))
            .await;
        let whole_frame = next(&mut plain.content).await;
        let plain_held = ROOMY - plain.gate.retained_available();

        let mut p = paged(8);
        p.lane
            .submit_paged(
                Some("a".into()),
                None,
                Some(budget::MIN_REPLY_PART_BYTES),
                RESERVE,
                big(100),
            )
            .await;
        let frame = next(&mut p.content).await;
        assert_eq!(frame.text, whole_frame.text);
        assert_eq!(json_of(&frame)["type"], "ack");
        assert_eq!(
            ROOMY - p.gate.retained_available(),
            plain_held,
            "the gate holds what a plain submit leaves"
        );
        assert_eq!(
            ROOMY - p.lane.retained.available_permits(),
            plain_held,
            "the connection holds what a plain submit leaves"
        );
        assert_eq!(p.lane.pending.available_permits(), 7);
        drop(frame);
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.retained.available_permits(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn a_reply_of_exactly_the_part_size_is_sent_whole_and_one_byte_more_is_held() {
        let part = budget::MIN_REPLY_PART_BYTES;
        let mut p = paged(8);
        p.lane
            .submit_paged(
                Some("exact".into()),
                None,
                Some(part),
                RESERVE,
                big(part - overhead()),
            )
            .await;
        let frame = next(&mut p.content).await;
        assert_eq!(frame.text.len(), part);
        assert_eq!(json_of(&frame)["type"], "ack");
        drop(frame);

        p.lane
            .submit_paged(
                Some("over".into()),
                None,
                Some(part),
                RESERVE,
                big(part - overhead() + 1),
            )
            .await;
        let frame = next(&mut p.content).await;
        let json = json_of(&frame);
        assert_eq!(json["type"], "reply_part");
        assert_eq!(json["total"], part + 1);
        assert_eq!(data_of(&json).len(), part);
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_without_a_part_size_is_never_held() {
        let mut p = paged(8);
        p.lane
            .submit(Some("a".into()), None, RESERVE, big(100_000))
            .await;
        let frame = next(&mut p.content).await;
        assert_eq!(json_of(&frame)["type"], "ack");
    }

    #[tokio::test(start_paused = true)]
    async fn a_larger_reply_is_answered_with_its_first_part_and_a_handle() {
        let n = 100_000;
        let text = whole(n).await;
        let part = 20_000;
        let mut p = paged(8);
        let (frame, handle) = first_part(&mut p, None, part, n).await;
        let json = json_of(&frame);
        assert_eq!(json["type"], "reply_part");
        assert_eq!(json["id"], "ask");
        assert!(!handle.is_empty());
        assert_eq!(json["offset"], 0);
        assert_eq!(json["total"], text.len());
        assert_eq!(data_of(&json), &text.as_bytes()[..part]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_parts_of_a_held_reply_joined_in_order_are_the_reply_a_plain_submit_sends() {
        let n = 100_000;
        let text = whole(n).await;
        let part = 20_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, part, n).await;
        let mut joined = data_of(&json_of(&first));
        drop(first);
        let (rest, sizes) =
            read_to_end(&mut p, &handle, part as u64, text.len() as u64, &[None]).await;
        joined.extend(rest);
        assert_eq!(joined, text.as_bytes());
        // Each part asked for without a length is the size the request asked for, the last
        // holding what is left.
        let (last, full) = sizes.split_last().unwrap();
        assert!(full.iter().all(|size| *size == part), "{sizes:?}");
        assert_eq!(*last, text.len() - part - full.len() * part);
    }

    #[tokio::test(start_paused = true)]
    async fn parts_of_different_lengths_still_join_to_the_reply() {
        let n = 150_000;
        let text = whole(n).await;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        let mut joined = data_of(&json_of(&first));
        drop(first);
        let lengths = [Some(30_000), Some(17_000), None, Some(100_000)];
        let (rest, _) = read_to_end(
            &mut p,
            &handle,
            joined.len() as u64,
            text.len() as u64,
            &lengths,
        )
        .await;
        joined.extend(rest);
        assert_eq!(joined, text.as_bytes());
    }

    #[tokio::test(start_paused = true)]
    async fn a_length_below_the_floor_is_raised_and_one_above_the_ceiling_is_cut() {
        let n = 2 << 20;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        drop(first);
        let tiny = part_at(&mut p, "tiny", &handle, 20_000, Some(1)).await;
        assert_eq!(data_of(&json_of(&tiny)).len(), budget::MIN_REPLY_PART_BYTES);
        drop(tiny);
        let huge = part_at(&mut p, "huge", &handle, 20_000, Some(u64::MAX)).await;
        assert_eq!(data_of(&json_of(&huge)).len(), budget::MAX_REPLY_PART_BYTES);
    }

    #[tokio::test(start_paused = true)]
    async fn an_offset_at_or_past_the_end_answers_empty_data_at_the_end_and_releases_the_reply() {
        let n = 50_000;
        let total = whole(n).await.len() as u64;
        for offset in [total, total + 1_000, u64::MAX] {
            let mut p = paged(8);
            let (first, handle) = first_part(&mut p, None, 20_000, n).await;
            drop(first);
            let frame = part_at(&mut p, "end", &handle, offset, None).await;
            let json = json_of(&frame);
            assert_eq!(json["type"], "reply_part", "offset {offset}");
            assert_eq!(json["data"], "", "offset {offset}");
            assert_eq!(json["offset"], total, "offset {offset}");
            assert_eq!(json["total"], total, "offset {offset}");
            drop(frame);

            p.lane
                .read_part(Some("after".into()), handle.clone(), 0, None)
                .await;
            let refusal = next(&mut p.control).await;
            assert_eq!(
                code_of(&refusal),
                Some(error_code::REPLY_PART_UNAVAILABLE),
                "offset {offset}"
            );
            assert_eq!(p.gate.retained_available(), ROOMY, "offset {offset}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_part_reaching_the_end_releases_the_reply_and_gives_everything_back() {
        let n = 50_000;
        let total = whole(n).await.len() as u64;
        let part = 20_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, part, n).await;
        drop(first);
        let (_, sizes) = read_to_end(&mut p, &handle, part as u64, total, &[None]).await;
        assert_eq!(sizes.iter().sum::<usize>() as u64, total - part as u64);

        // The last part is written, which released the reply: a part asked for now is refused.
        p.lane
            .read_part(Some("late".into()), handle.clone(), 0, None)
            .await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REPLY_PART_UNAVAILABLE));
        assert_eq!(param(&refusal, "reply").as_deref(), Some(handle.as_str()));
        assert_eq!(error_id(&refusal).as_deref(), Some("late"));

        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.retained.available_permits(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn the_last_part_gives_back_the_reservation_only_once_its_frame_is_written() {
        let n = 50_000;
        let total = whole(n).await.len() as u64;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        drop(first);
        let last = part_at(&mut p, "last", &handle, 0, Some(total)).await;
        assert!(data_of(&json_of(&last)).len() as u64 == total);
        assert!(
            p.gate.retained_available() < ROOMY,
            "the part still being written holds the reply's room"
        );
        drop(last);
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn a_held_reply_keeps_its_text_and_room_for_one_part_until_it_is_released() {
        let n = 100_000;
        let text_len = whole(n).await.len();
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        drop(first);
        let held = text_len + budget::REPLY_PART_ROOM;
        assert_eq!(ROOMY - p.gate.retained_available(), held);
        assert_eq!(ROOMY - p.lane.retained.available_permits(), held);
        assert_eq!(
            p.lane.pending.available_permits(),
            7,
            "the held reply is one outstanding request"
        );

        let second = part_at(&mut p, "second", &handle, 20_000, None).await;
        drop(second);
        assert_eq!(ROOMY - p.gate.retained_available(), held);
        assert_eq!(p.lane.pending.available_permits(), 7);
    }

    #[tokio::test(start_paused = true)]
    async fn a_held_reply_counts_against_the_pending_bound_until_it_is_released() {
        let n = 50_000;
        let total = whole(n).await.len() as u64;
        let part = 20_000;
        let mut p = paged(2);
        let (first, handle) = first_part(&mut p, None, part, n).await;
        drop(first);

        // The held reply holds one of the two places; a blocked read holds the other.
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        p.lane
            .submit(Some("blocked".into()), None, 1024, move |_| {
                let _ = blocked.recv();
                Ok(ack("blocked"))
            })
            .await;
        p.lane.submit(Some("c".into()), None, 1024, big(1)).await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::LIMIT_EXCEEDED));
        assert_eq!(
            param(&refusal, "limit").as_deref(),
            Some("pending_requests")
        );
        assert_eq!(param(&refusal, "max").as_deref(), Some("2"));
        assert_eq!(error_id(&refusal).as_deref(), Some("c"));

        // Asking for a part takes a place too, so it is refused the same way while none is free.
        p.lane
            .read_part(Some("part".into()), handle.clone(), part as u64, None)
            .await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::LIMIT_EXCEEDED));
        assert_eq!(
            param(&refusal, "limit").as_deref(),
            Some("pending_requests")
        );
        assert_eq!(error_id(&refusal).as_deref(), Some("part"));

        release.send(()).unwrap();
        drop(next(&mut p.content).await);
        read_to_end(&mut p, &handle, part as u64, total, &[None]).await;

        // Released by its last part: both places are free again.
        p.lane.submit(Some("d".into()), None, 1024, big(1)).await;
        p.lane.submit(Some("e".into()), None, 1024, big(1)).await;
        let (d, e) = (next(&mut p.content).await, next(&mut p.content).await);
        p.lane.submit(Some("f".into()), None, 1024, big(1)).await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::LIMIT_EXCEEDED));
        assert_eq!(error_id(&refusal).as_deref(), Some("f"));
        drop((d, e));
    }

    #[tokio::test(start_paused = true)]
    async fn a_newer_request_in_the_slot_releases_the_held_reply_and_a_part_asked_for_is_superseded(
    ) {
        let n = 100_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, Some("viewer"), 20_000, n).await;
        drop(first);
        p.lane
            .submit(Some("newer".into()), Some("viewer".into()), RESERVE, |_| {
                Ok(ack("newer"))
            })
            .await;
        p.lane
            .read_part(Some("late".into()), handle.clone(), 20_000, None)
            .await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REQUEST_SUPERSEDED));
        assert_eq!(error_id(&refusal).as_deref(), Some("late"));
        let reply = next(&mut p.content).await;
        assert!(reply.text.contains("\"newer\""));
        drop(reply);
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_in_another_slot_or_none_leaves_a_held_reply_alone() {
        let n = 100_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, Some("viewer"), 20_000, n).await;
        drop(first);
        p.lane
            .submit(Some("other".into()), Some("elsewhere".into()), 1024, |_| {
                Ok(ack("other"))
            })
            .await;
        p.lane
            .submit(Some("slotless".into()), None, 1024, |_| Ok(ack("slotless")))
            .await;
        drop(next(&mut p.content).await);
        drop(next(&mut p.content).await);
        let part = part_at(&mut p, "part", &handle, 20_000, None).await;
        assert_eq!(json_of(&part)["type"], "reply_part");
    }

    #[tokio::test(start_paused = true)]
    async fn a_reply_that_finishes_after_a_newer_request_took_its_slot_is_superseded_not_held() {
        let mut p = paged(8);
        let (finish, finishing) = std::sync::mpsc::channel::<()>();
        p.lane
            .submit_paged(
                Some("old".into()),
                Some("viewer".into()),
                Some(20_000),
                RESERVE,
                move |_| {
                    // Ignores the cancel, as a read past its last check would.
                    let _ = finishing.recv();
                    Ok(ack(&"x".repeat(100_000)))
                },
            )
            .await;
        p.lane
            .submit(Some("new".into()), Some("viewer".into()), 1024, |_| {
                Ok(ack("new"))
            })
            .await;
        let _ = finish.send(());
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REQUEST_SUPERSEDED));
        assert_eq!(error_id(&refusal).as_deref(), Some("old"));
        let reply = next(&mut p.content).await;
        assert!(reply.text.contains("\"new\""));
        drop(reply);
        crate::test_support::tasks_idle().await;
        assert!(p.content.try_recv().is_err(), "no part of the old reply");
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn only_one_part_of_a_reply_is_in_flight_and_the_next_waits_for_its_frame_to_be_written()
    {
        let n = 100_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        p.lane
            .read_part(Some("second".into()), handle.clone(), 20_000, None)
            .await;
        crate::test_support::tasks_idle().await;
        assert!(
            p.content.try_recv().is_err(),
            "sent while the first was in flight"
        );
        assert!(p.control.try_recv().is_err());
        drop(first);
        let second = next(&mut p.content).await;
        let json = json_of(&second);
        assert_eq!(json["id"], "second");
        assert_eq!(json["offset"], 20_000);
    }

    #[tokio::test(start_paused = true)]
    async fn a_part_waiting_for_its_turn_is_superseded_when_a_newer_request_releases_the_reply() {
        let n = 100_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, Some("viewer"), 20_000, n).await;
        p.lane
            .read_part(Some("waiting".into()), handle.clone(), 20_000, None)
            .await;
        crate::test_support::tasks_idle().await;
        p.lane
            .submit(Some("newer".into()), Some("viewer".into()), 1024, |_| {
                Ok(ack("newer"))
            })
            .await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REQUEST_SUPERSEDED));
        assert_eq!(error_id(&refusal).as_deref(), Some("waiting"));
        drop(first);
        drop(next(&mut p.content).await);
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn closing_the_lane_releases_the_held_replies() {
        let n = 100_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        drop(first);
        assert!(p.gate.retained_available() < ROOMY);
        p.lane.close();
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.retained.available_permits(), ROOMY);

        // Nobody is left to read an answer: nothing is sent for a part asked for now.
        p.lane
            .read_part(Some("late".into()), handle, 20_000, None)
            .await;
        crate::test_support::tasks_idle().await;
        assert!(p.control.try_recv().is_err());
        assert!(p.content.try_recv().is_err());
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn a_part_asked_of_a_reply_that_never_existed_is_unavailable() {
        let mut p = paged(8);
        p.lane
            .read_part(Some("lost".into()), "reply-99".into(), 0, None)
            .await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REPLY_PART_UNAVAILABLE));
        assert_eq!(param(&refusal, "reply").as_deref(), Some("reply-99"));
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn a_held_reply_is_released_after_the_idle_time_counted_from_its_latest_part_written() {
        let n = 100_000;
        let text_len = whole(n).await.len();
        let held = text_len + budget::REPLY_PART_ROOM;
        let second = Duration::from_secs(1);
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        drop(first);

        // Short of the idle time since the first part was written: still held.
        tokio::time::advance(budget::HELD_REPLY_IDLE - second).await;
        crate::test_support::tasks_idle().await;
        assert_eq!(ROOMY - p.gate.retained_available(), held);

        // A part asked for now moves the time on from when it is written.
        drop(part_at(&mut p, "second", &handle, 20_000, None).await);
        tokio::time::advance(budget::HELD_REPLY_IDLE - second).await;
        crate::test_support::tasks_idle().await;
        assert_eq!(
            ROOMY - p.gate.retained_available(),
            held,
            "counted from the first part, not the latest"
        );

        // Past it with no part asked for: released, and everything is given back.
        tokio::time::advance(second * 2).await;
        crate::test_support::tasks_idle().await;
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.retained.available_permits(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
        p.lane
            .read_part(Some("late".into()), handle.clone(), 40_000, None)
            .await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REPLY_PART_UNAVAILABLE));
        assert_eq!(param(&refusal, "reply").as_deref(), Some(handle.as_str()));
    }

    #[tokio::test(start_paused = true)]
    async fn a_part_still_being_written_is_not_expired_and_the_idle_time_starts_when_it_is() {
        let n = 100_000;
        let text_len = whole(n).await.len();
        let held = text_len + budget::REPLY_PART_ROOM;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;

        // Many idle times pass with the frame unwritten, as on a stalled link.
        tokio::time::advance(budget::HELD_REPLY_IDLE * 5).await;
        crate::test_support::tasks_idle().await;
        assert_eq!(ROOMY - p.gate.retained_available(), held);

        // The idle time counts from the write, not from the frame being queued.
        drop(first);
        tokio::time::advance(budget::HELD_REPLY_IDLE - Duration::from_secs(1)).await;
        crate::test_support::tasks_idle().await;
        assert_eq!(ROOMY - p.gate.retained_available(), held);
        let part = part_at(&mut p, "after", &handle, 20_000, None).await;
        assert_eq!(json_of(&part)["type"], "reply_part");
    }

    #[tokio::test(start_paused = true)]
    async fn parts_asked_back_to_back_are_served_in_the_order_asked_one_at_a_time() {
        let n = 100_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        // Asked at once, with the first part's frame still unwritten and nothing awaited between.
        for (id, offset) in [("p1", 20_000), ("p2", 40_000), ("p3", 60_000)] {
            p.lane
                .read_part(Some(id.into()), handle.clone(), offset, None)
                .await;
        }
        crate::test_support::tasks_idle().await;
        assert!(
            p.content.try_recv().is_err(),
            "sent while the first was in flight"
        );
        drop(first);
        for (id, offset) in [("p1", 20_000), ("p2", 40_000), ("p3", 60_000)] {
            let frame = next(&mut p.content).await;
            let json = json_of(&frame);
            assert_eq!(json["id"], id);
            assert_eq!(json["offset"], offset);
            crate::test_support::tasks_idle().await;
            assert!(
                p.content.try_recv().is_err(),
                "{id} was still in flight when the next was sent"
            );
            assert!(p.control.try_recv().is_err(), "{id}: nothing is refused");
            drop(frame);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_middle_part_asked_before_the_last_is_served_before_it_and_is_not_unavailable() {
        let n = 50_000;
        let total = whole(n).await.len() as u64;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        p.lane
            .read_part(Some("middle".into()), handle.clone(), 20_000, Some(20_000))
            .await;
        p.lane
            .read_part(Some("last".into()), handle.clone(), 40_000, Some(total))
            .await;
        drop(first);
        let middle = next(&mut p.content).await;
        assert_eq!(json_of(&middle)["id"], "middle");
        assert_eq!(json_of(&middle)["type"], "reply_part");
        drop(middle);
        let last = next(&mut p.content).await;
        let json = json_of(&last);
        assert_eq!(
            (json["id"].as_str(), json["offset"].as_u64()),
            (Some("last"), Some(40_000))
        );
        assert_eq!(data_of(&json).len() as u64, total - 40_000);
        assert!(p.control.try_recv().is_err());
        drop(last);
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn a_part_asked_after_the_one_reaching_the_end_finds_the_reply_released() {
        let n = 50_000;
        let total = whole(n).await.len() as u64;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        p.lane
            .read_part(Some("last".into()), handle.clone(), 20_000, Some(total))
            .await;
        p.lane
            .read_part(Some("after".into()), handle.clone(), 20_000, None)
            .await;
        drop(first);
        let last = next(&mut p.content).await;
        assert_eq!(json_of(&last)["id"], "last");
        drop(last);
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REPLY_PART_UNAVAILABLE));
        assert_eq!(error_id(&refusal).as_deref(), Some("after"));
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }

    #[tokio::test(start_paused = true)]
    async fn a_reply_with_a_part_queued_behind_one_in_flight_is_not_expired() {
        let n = 100_000;
        let mut p = paged(8);
        let (first, handle) = first_part(&mut p, None, 20_000, n).await;
        p.lane
            .read_part(Some("queued".into()), handle.clone(), 20_000, None)
            .await;
        // Many idle times pass with the first frame unwritten and a part waiting for it.
        tokio::time::advance(budget::HELD_REPLY_IDLE * 5).await;
        crate::test_support::tasks_idle().await;
        assert!(p.control.try_recv().is_err());
        drop(first);
        let queued = next(&mut p.content).await;
        let json = json_of(&queued);
        assert_eq!(json["type"], "reply_part");
        assert_eq!(json["id"], "queued");
        assert_eq!(json["offset"], 20_000);
    }

    #[tokio::test(start_paused = true)]
    async fn a_reply_that_finishes_for_a_cancelled_request_is_answered_superseded_and_gives_everything_back(
    ) {
        let mut p = paged(8);
        let (finish, finishing) = std::sync::mpsc::channel::<()>();
        p.lane
            .submit_paged(
                Some("old".into()),
                Some("viewer".into()),
                Some(20_000),
                RESERVE,
                move |_| {
                    let _ = finishing.recv();
                    Ok(ack(&"x".repeat(100_000)))
                },
            )
            .await;
        p.lane
            .submit(Some("new".into()), Some("viewer".into()), 1024, |_| {
                Ok(ack("new"))
            })
            .await;
        let _ = finish.send(());
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REQUEST_SUPERSEDED));
        assert_eq!(error_id(&refusal).as_deref(), Some("old"));
        drop(next(&mut p.content).await);
        crate::test_support::tasks_idle().await;
        // Nothing was held for the old request, so no reply is there to read, and its slot is free.
        p.lane
            .read_part(Some("late".into()), "reply-0".into(), 0, None)
            .await;
        let refusal = next(&mut p.control).await;
        assert_eq!(code_of(&refusal), Some(error_code::REPLY_PART_UNAVAILABLE));
        assert_eq!(p.gate.retained_available(), ROOMY);
        assert_eq!(p.lane.retained.available_permits(), ROOMY);
        assert_eq!(p.lane.pending.available_permits(), 8);
    }
}
