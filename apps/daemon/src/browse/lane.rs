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
//! [`budget::MAX_RETAINED_PER_CONNECTION`]: each read reserves its worst case from both before it
//! starts, gives back what its serialized reply did not need, and gives back the rest when the
//! frame has been written. Replies leave on a queue of their own, which the connection's writer
//! drains only while no control event is waiting, so a burst of file bodies never holds a status
//! change behind it.
//!
//! A request may name a slot, per connection. A newer request in the same slot cancels the older
//! one — its file read stops at the next chunk, its `git` is killed — and the older one is answered
//! `request_superseded`. Closing the connection cancels everything it had outstanding.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
}

impl Gate {
    /// `reads` reads at a time, `retained` bytes held.
    pub fn new(reads: usize, retained: usize) -> Self {
        Self {
            reads: Arc::new(Semaphore::new(reads)),
            retained: Arc::new(Semaphore::new(retained)),
            retained_max: retained,
        }
    }

    /// The retained bytes not reserved by any read or reply right now.
    #[cfg(test)]
    pub fn retained_available(&self) -> usize {
        self.retained.available_permits()
    }

    pub fn with_budget() -> Self {
        Self::new(budget::MAX_CONCURRENT_READS, budget::MAX_RETAINED_BYTES)
    }
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
        let Ok(pending) = self.pending.clone().try_acquire_owned() else {
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
            .reply(id);
            let _ = self.control.send(refusal).await;
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
        // A reservation larger than either budget could never be granted.
        let reservation = reservation
            .min(self.retained_max)
            .min(self.gate.retained_max);
        let mut connection = tokio::select! {
            permit = self.retained.clone().acquire_many_owned(reservation as u32) => {
                permit.expect("the lane's semaphores are never closed")
            }
            () = cancel.cancelled() => return Err(anyhow::Error::new(Cancelled)),
        };
        let mut retained = tokio::select! {
            permit = self.gate.retained.clone().acquire_many_owned(reservation as u32) => {
                permit.expect("the gate's semaphores are never closed")
            }
            () = cancel.cancelled() => return Err(anyhow::Error::new(Cancelled)),
        };
        let reading = tokio::select! {
            permit = self.gate.reads.clone().acquire_owned() => {
                permit.expect("the gate's semaphores are never closed")
            }
            () = cancel.cancelled() => return Err(anyhow::Error::new(Cancelled)),
        };
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
        let closed = || self.in_flight.lock().expect("lane lock poisoned").closed;
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
            Err(err) if err.downcast_ref::<Cancelled>().is_some() => superseded(id),
            Err(err) => match err.downcast_ref::<CodedError>() {
                Some(coded) => coded.reply(id),
                None => Event::internal_error(id, format!("{err:#}")),
            },
        };
        if closed() {
            return;
        }
        let _ = self.control.send(failure).await;
        drop(pending);
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

    fn code_of(event: &Event) -> Option<&str> {
        match event {
            Event::Error { code, .. } => Some(code),
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
        let (lane, _control, mut content) = lane(Gate::new(1, 1 << 20), 8);
        let release = Arc::new(AtomicBool::new(false));
        let second_started = Arc::new(AtomicBool::new(false));
        lane.submit(Some("a".into()), None, 1024, held(release.clone(), "a"))
            .await;
        let started = second_started.clone();
        lane.submit(Some("b".into()), None, 1024, move |_| {
            started.store(true, Ordering::Relaxed);
            Ok(ack("b"))
        })
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !second_started.load(Ordering::Relaxed),
            "ran past the bound"
        );
        release.store(true, Ordering::Relaxed);
        let mut answered = [next(&mut content).await.text, next(&mut content).await.text];
        answered.sort();
        assert!(answered[0].contains("\"a\"") && answered[1].contains("\"b\""));
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
        lane.submit(None, None, 1024, move |cancel| {
            while !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(2));
            }
            saw.store(true, Ordering::Relaxed);
            Err(anyhow::Error::new(Cancelled))
        })
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
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
        tokio::time::sleep(Duration::from_millis(100)).await;
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
}
