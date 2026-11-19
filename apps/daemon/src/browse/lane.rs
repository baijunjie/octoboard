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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use tokio::sync::{mpsc, Notify, OwnedSemaphorePermit, Semaphore};

use crate::browse::{budget, Cancelled};
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

    /// Starts `work` for request `id`, under the lane's bounds, reserving `reservation` bytes —
    /// the most its reply and its buffers can come to — for it; its reply, or its failure, goes
    /// back on the connection. Returns once the request is under way, or once its refusal is on
    /// the control queue.
    pub async fn submit<W>(
        self: &Arc<Self>,
        id: Option<String>,
        slot: Option<String>,
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
            let outcome = lane.run(&cancel, reservation, work).await;
            lane.unregister(seq, slot.as_deref());
            lane.deliver(id, &cancel, pending, outcome).await;
        });
    }

    /// Starts `work` for request `id` as [`Lane::submit`] does, for a request answered with `ack`
    /// and followed by events: `work` sends the `ack` and those events through the [`Stream`] it is
    /// handed, and returns the last event. Every event after the `ack` is carved out of
    /// `reservation`, and keeps its share until its frame has been written, so `reservation` bounds
    /// all the bytes the request sends; the last event keeps the request's place among the
    /// outstanding ones until it is written too. A failure before the `ack` is answered as
    /// [`Lane::submit`] answers one. Once the request is acknowledged its id has its answer, so
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
        }
        Some((seq, cancel))
    }

    fn unregister(&self, seq: u64, slot: Option<&str>) {
        let mut in_flight = self.in_flight.lock().expect("lane lock poisoned");
        in_flight.requests.remove(&seq);
        if let Some(slot) = slot {
            if in_flight.slots.get(slot) == Some(&seq) {
                in_flight.slots.remove(slot);
            }
        }
    }

    /// Cancels everything outstanding and refuses anything submitted later: the connection has
    /// gone, and nobody is left to read a reply.
    pub fn close(&self) {
        let mut in_flight = self.in_flight.lock().expect("lane lock poisoned");
        in_flight.closed = true;
        for cancel in in_flight.requests.values() {
            cancel.cancel();
        }
    }

    /// Waits for a reservation from the connection and from the daemon and for a read slot, then
    /// runs `work` on a blocking thread and serializes its reply into a buffer of exactly its
    /// size, shrinking both reservations to it. Gives up at any point once cancelled. A reply
    /// that would not fit its reservation is refused rather than held past the budget.
    async fn run<W>(&self, cancel: &Arc<Cancel>, reservation: usize, work: W) -> Result<Reply>
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
        drop(connection.split(unused));
        drop(retained.split(unused));
        Ok(Reply {
            text,
            retained,
            connection,
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
}
