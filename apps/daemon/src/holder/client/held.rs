//! A terminal the daemon runs in a holder, as the daemon holds it: the same calls as a terminal it
//! owns (`platform::term::Pty`) and the child that runs on it (`platform::proc::ChildHandle`),
//! each made over the link and answered by the holder.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::sync::{mpsc, watch};

use super::link::{locked, Link, Routed, Sending, REPLY_DEADLINE};
use crate::holder::protocol::{Answer, FromHolder, QueryKind, SignalKind, ToHolder};
use crate::platform::proc::{Polite, ProcessId, ProcessIdentity, Sent, Swept};
use crate::platform::term::{Foreground, Input, PartialWrite, Size};

/// How long [`HeldTerminal::end`] waits for the holder to report the program's exit.
const END_DEADLINE: Duration = Duration::from_secs(3);

/// The most bytes one write request carries, so that a large paste never makes a frame bigger than
/// the holder's WebSocket accepts. The chunks are written one after the other.
const WRITE_CHUNK: usize = 1024 * 1024;

/// Where a held terminal runs: its holder and its id there, which the daemon records with the
/// launch so that a start that cannot link to the holder knows what it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldAt {
    pub holder_id: String,
    pub terminal_id: String,
}

pub struct HeldTerminal {
    link: Arc<Link>,
    id: String,
    pid: ProcessId,
    routed: Arc<Routed>,
    input_lock: Mutex<()>,
    hung_up: AtomicBool,
    reaped: AtomicBool,
    /// The holder has been told to forget the terminal, which is done once.
    released: AtomicBool,
    /// Wakes the reader to end, whatever it is waiting on.
    stop: watch::Sender<bool>,
    /// What the program printed, until the reader takes it.
    output: Mutex<Option<mpsc::Receiver<Vec<u8>>>>,
    /// What the reader returns before any output ([`Self::replay_first`]), until it is made.
    first: Mutex<Vec<u8>>,
}

impl HeldTerminal {
    pub(super) fn new(
        link: Arc<Link>,
        id: String,
        pid: u32,
        routed: Arc<Routed>,
        output: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            link,
            id,
            pid: ProcessId(pid),
            routed,
            input_lock: Mutex::new(()),
            hung_up: AtomicBool::new(false),
            reaped: AtomicBool::new(false),
            released: AtomicBool::new(false),
            stop: watch::channel(false).0,
            output: Mutex::new(Some(output)),
            first: Mutex::new(Vec::new()),
        }
    }

    pub fn pid(&self) -> ProcessId {
        self.pid
    }

    /// Has `hook` run, on a thread of its own, each time the link to the holder was down and is
    /// open again while the program still runs: what it printed in between never reached this
    /// terminal. Replaces an earlier hook.
    pub fn on_resumed(&self, hook: impl Fn() + Send + Sync + 'static) {
        *locked(&self.routed.resumed) = Some(Arc::new(hook));
    }

    /// Has `hook` run, on a thread of its own, each time the link to the holder becomes pending
    /// (down while the holder runs, tried again in the background) or stops being so, whether it
    /// comes back or is lost; [`Self::unlinked`] says which it is when the hook runs. Replaces an
    /// earlier hook.
    pub fn on_link_changed(&self, hook: impl Fn() + Send + Sync + 'static) {
        *locked(&self.routed.link_changed) = Some(Arc::new(hook));
    }

    /// Whether the holder cannot be linked to for now while it runs: the program goes on, and
    /// every call on the terminal fails until the link is back.
    pub fn unlinked(&self) -> bool {
        self.link.pending()
    }

    /// Which holder runs the terminal, and its id there.
    pub fn held_at(&self) -> HeldAt {
        HeldAt {
            holder_id: self.link.welcome().holder_id.clone(),
            terminal_id: self.id.clone(),
        }
    }

    /// The lock the owner makes every write into the input under.
    pub fn input_lock(&self) -> &Mutex<()> {
        &self.input_lock
    }

    fn gone() -> PartialWrite {
        PartialWrite {
            written: 0,
            error: io::Error::new(io::ErrorKind::BrokenPipe, "the holder is gone"),
        }
    }

    fn hung_up_write() -> PartialWrite {
        PartialWrite {
            written: 0,
            error: io::Error::new(io::ErrorKind::BrokenPipe, "the terminal has been hung up"),
        }
    }

    /// Writes, sent as `sending` says, and waits for how far the holder got. Waits for as long as
    /// the holder takes, which is bounded on its side, and ends only on its answer or the loss of
    /// the link: an answer given up on would leave the caller not knowing how much of the message
    /// went in.
    fn write(
        &self,
        sending: Sending,
        make: impl FnOnce(u64) -> ToHolder,
    ) -> Result<(), PartialWrite> {
        if self.hung_up.load(Ordering::Acquire) {
            return Err(Self::hung_up_write());
        }
        match self.link.request_as(sending, None, make) {
            Some(FromHolder::Written { written, error, .. }) => match error {
                None => Ok(()),
                Some(error) => Err(PartialWrite {
                    written,
                    error: error.into_io(),
                }),
            },
            _ => Err(Self::gone()),
        }
    }

    /// Writes `bytes` as they are, in requests of at most [`WRITE_CHUNK`] bytes. The error counts
    /// the chunks that went in whole and how far the one that failed got. Refused once the
    /// terminal is handed over.
    pub fn write_raw(&self, bytes: &[u8]) -> Result<(), PartialWrite> {
        self.write_chunks(Sending::Ordinary, bytes)
    }

    /// [`Self::write_raw`] for the daemon's answers to the queries in the output the reader took
    /// in: still written once the terminal is handed over, until the daemon parts, since all that
    /// output came before the mark and nobody else answers those queries.
    pub fn write_answers(&self, bytes: &[u8]) -> Result<(), PartialWrite> {
        self.write_chunks(Sending::Answers, bytes)
    }

    fn write_chunks(&self, sending: Sending, bytes: &[u8]) -> Result<(), PartialWrite> {
        let mut written = 0;
        for chunk in bytes.chunks(WRITE_CHUNK) {
            let sent = self.write(sending, |req| ToHolder::Write {
                id: self.id.clone(),
                req,
                bytes: chunk.to_vec(),
            });
            if let Err(mut failed) = sent {
                failed.written += written;
                return Err(failed);
            }
            written += chunk.len();
        }
        Ok(())
    }

    /// Writes `input`, which the holder encodes as its host's terminal takes it.
    pub fn write_input(&self, input: &[Input]) -> Result<(), PartialWrite> {
        self.write(Sending::Ordinary, |req| ToHolder::WriteInput {
            id: self.id.clone(),
            req,
            inputs: input.to_vec(),
        })
    }

    fn query(&self, what: QueryKind) -> Option<Answer> {
        let reply = self
            .link
            .request(Some(REPLY_DEADLINE), |req| ToHolder::Query {
                id: self.id.clone(),
                req,
                what,
            });
        match reply {
            Some(FromHolder::Answer { answer, .. }) => Some(answer),
            _ => None,
        }
    }

    pub fn size(&self) -> Result<Size> {
        if self.hung_up.load(Ordering::Acquire) {
            return Err(anyhow!("the terminal has been hung up"));
        }
        match self.query(QueryKind::Size) {
            Some(Answer::Size { size: Some(size) }) => Ok(size),
            _ => Err(anyhow!("the holder could not tell the terminal's size")),
        }
    }

    pub fn resize(&self, size: Size) -> Result<()> {
        if self.hung_up.load(Ordering::Acquire) {
            return Err(anyhow!("the terminal has been hung up"));
        }
        let reply = self
            .link
            .request(Some(REPLY_DEADLINE), |req| ToHolder::Resize {
                id: self.id.clone(),
                req,
                size,
            });
        match reply {
            Some(FromHolder::Done { error: None, .. }) => Ok(()),
            Some(FromHolder::Done {
                error: Some(error), ..
            }) => Err(anyhow!(error)),
            _ => Err(anyhow!("the holder did not answer the resize")),
        }
    }

    pub fn foreground_job(&self) -> Foreground {
        if self.hung_up.load(Ordering::Acquire) {
            return Foreground::Unsupported;
        }
        match self.query(QueryKind::ForegroundJob) {
            Some(Answer::ForegroundJob { foreground }) => foreground.into(),
            _ => Foreground::Unsupported,
        }
    }

    pub fn slave_path(&self) -> Option<PathBuf> {
        if self.hung_up.load(Ordering::Acquire) {
            return None;
        }
        match self.query(QueryKind::SlavePath) {
            Some(Answer::SlavePath { path }) => path.map(PathBuf::from),
            _ => None,
        }
    }

    pub fn polite_stop_lead(&self, polite: Polite) -> Duration {
        let reply = self
            .link
            .request(Some(REPLY_DEADLINE), |req| ToHolder::PoliteStopLead {
                id: self.id.clone(),
                req,
                polite,
            });
        match reply {
            Some(FromHolder::Answer {
                answer: Answer::Lead { millis },
                ..
            }) => Duration::from_millis(millis),
            _ => Duration::ZERO,
        }
    }

    /// Hangs the terminal up: its reader ends, and the holder closes the PTY. Every write after
    /// this one fails. The holder forgets the terminal once the program is reaped as well.
    pub fn hang_up(&self) {
        self.hung_up.store(true, Ordering::Release);
        self.stop_reader();
        // Not taken by a reader that never started, such as one of a terminal ended while it was
        // being adopted: output queued for it would otherwise wait on the link for ever.
        locked(&self.output).take();
        self.link.send(&ToHolder::HangUp {
            id: self.id.clone(),
        });
        self.release_if_done();
    }

    /// Has the holder forget the terminal, once it has been hung up and its program reaped, which
    /// is when a terminal the daemon owns is over. Before that the holder keeps it: the program
    /// may still print, and a program that is not reaped is a zombie.
    fn release_if_done(&self) {
        if self.hung_up.load(Ordering::Acquire)
            && self.reaped.load(Ordering::Acquire)
            && !self.released.swap(true, Ordering::AcqRel)
        {
            self.link.release(&self.id);
        }
    }

    /// Makes the reader return `Ok(None)`, now and from then on.
    pub fn stop_reader(&self) {
        self.stop.send_replace(true);
    }

    #[cfg(test)]
    pub fn is_hung_up(&self) -> bool {
        self.hung_up.load(Ordering::Acquire)
    }

    /// Has the reader return `bytes` before any of the output, as if the program had printed them
    /// first: the output since a handover's mark, which follows a screen restored as it was at
    /// the mark. Called before [`Self::reader`]; replaces bytes given before.
    pub fn replay_first(&self, bytes: Vec<u8>) {
        *locked(&self.first) = bytes;
    }

    /// A reader of the terminal's output; the first one gets it, and whatever
    /// [`Self::replay_first`] gave before it, and a later one reads nothing.
    pub fn reader(&self) -> HeldReader {
        HeldReader {
            runtime: self.link.runtime().clone(),
            state: Mutex::new(ReaderState {
                output: locked(&self.output).take(),
                stop: self.stop.subscribe(),
                unread: std::mem::take(&mut *locked(&self.first)),
                offset: 0,
            }),
        }
    }

    /// Sends `signal` to the program. A link that is pending is no sign the program is gone: the
    /// signal failed, and the program runs on.
    fn signal(&self, signal: SignalKind) -> Sent {
        let sent = self.link.send(&ToHolder::Signal {
            id: self.id.clone(),
            signal,
        });
        if sent {
            Sent::Delivered
        } else if self.link.pending() {
            Sent::Failed(io::Error::new(
                io::ErrorKind::NotConnected,
                "the holder cannot be reached for now",
            ))
        } else {
            Sent::Gone
        }
    }

    pub fn stop(&self, how: Polite) -> Sent {
        self.signal(SignalKind::polite(how))
    }

    pub fn kill(&self) -> Sent {
        self.signal(SignalKind::Kill)
    }

    /// Whether the holder has reported the program's exit, or is gone.
    pub fn exited_unreaped(&self) -> Option<bool> {
        Some(self.routed.exited.load(Ordering::Acquire))
    }

    pub fn kill_leftovers(&self, keep: &[ProcessIdentity]) -> Swept {
        let reply = self
            .link
            .request(Some(REPLY_DEADLINE), |req| ToHolder::KillLeftovers {
                id: self.id.clone(),
                req,
                identities: keep.iter().map(ProcessIdentity::key).collect(),
            });
        match reply {
            Some(FromHolder::Answer {
                answer: Answer::Swept { killed, complete },
                ..
            }) => Swept { killed, complete },
            _ => Swept {
                killed: 0,
                complete: false,
            },
        }
    }

    /// Ends the program and what it left behind, and the terminal with them: the program is
    /// killed unless it has exited, what it started is swept, the terminal is hung up and the
    /// program reaped once the holder reports its exit. For a terminal nothing is to own.
    /// Returns whether the end is confirmed: the holder answered that it reaped the program, or
    /// it was reaped already. A kill that could not be delivered, a sweep of what the program left
    /// behind that could not be finished, a link that is lost or a holder that does not answer, or
    /// the deadline passing, is not confirmed: the program, or what it started, may run on.
    /// **Blocks** for up to a few seconds.
    pub fn end(&self) -> bool {
        // Nobody reads what the program still prints, so what is queued for the reader must not
        // hold up the link the calls below wait on.
        locked(&self.output).take();
        let killed =
            self.routed.exited.load(Ordering::Acquire) || matches!(self.kill(), Sent::Delivered);
        let swept = self.kill_leftovers(&[]);
        swept.log(self.pid, &self.id);
        self.hang_up();
        if !killed {
            return false;
        }
        // Waited for even after a sweep that could not be finished, as the callers that do not
        // look at the answer count on the program being reaped by the time this returns.
        let deadline = std::time::Instant::now() + END_DEADLINE;
        while std::time::Instant::now() < deadline {
            match self.ask_reap() {
                Ok(Reap::Reaped) => return swept.complete,
                Ok(Reap::Running) => std::thread::sleep(Duration::from_millis(20)),
                Ok(Reap::NoAnswer) => {
                    self.note_reaped();
                    return false;
                }
                Err(_) => return false,
            }
        }
        false
    }

    /// Reaps the program if it has exited, and says whether it did. A holder that is gone has
    /// nothing left to reap.
    pub fn reap_if_exited(&self) -> io::Result<bool> {
        match self.ask_reap()? {
            Reap::Reaped => Ok(true),
            Reap::Running => Ok(false),
            Reap::NoAnswer => {
                self.note_reaped();
                Ok(true)
            }
        }
    }

    /// Notes the program as reaped, releasing the terminal if it is hung up as well.
    fn note_reaped(&self) {
        self.reaped.store(true, Ordering::Release);
        self.release_if_done();
    }

    /// Asks the holder to reap the program if it has exited, telling an answer that it did from
    /// no answer at all.
    fn ask_reap(&self) -> io::Result<Reap> {
        if self.reaped.load(Ordering::Acquire) {
            return Ok(Reap::Reaped);
        }
        if !self.routed.exited.load(Ordering::Acquire) {
            return Ok(Reap::Running);
        }
        let reply = self
            .link
            .request(Some(REPLY_DEADLINE), |req| ToHolder::Reap {
                id: self.id.clone(),
                req,
            });
        match reply {
            Some(FromHolder::Answer {
                answer:
                    Answer::Reaped {
                        reaped,
                        error: None,
                    },
                ..
            }) => {
                if reaped {
                    self.note_reaped();
                    Ok(Reap::Reaped)
                } else {
                    Ok(Reap::Running)
                }
            }
            Some(FromHolder::Answer {
                answer:
                    Answer::Reaped {
                        error: Some(error), ..
                    },
                ..
            }) => Err(io::Error::other(error)),
            // The link is lost, the holder has ended its programs, or it forgot the terminal.
            _ => Ok(Reap::NoAnswer),
        }
    }
}

/// What asking the holder to reap a program came to.
enum Reap {
    /// The program is reaped, as the holder answered or as it was already.
    Reaped,
    /// The program has not exited yet, or the holder did not reap it.
    Running,
    /// The holder gave no answer: the link is lost, or it forgot the terminal.
    NoAnswer,
}

impl Drop for HeldTerminal {
    fn drop(&mut self) {
        if !self.released.swap(true, Ordering::AcqRel) {
            self.link.release(&self.id);
        }
    }
}

/// Reads what a held terminal's program printed, from one thread at a time.
pub struct HeldReader {
    runtime: tokio::runtime::Handle,
    state: Mutex<ReaderState>,
}

struct ReaderState {
    output: Option<mpsc::Receiver<Vec<u8>>>,
    stop: watch::Receiver<bool>,
    /// Bytes not yet read whole (those [`HeldTerminal::replay_first`] gave, or a chunk too large
    /// for the buffer it was read into), and how much of them has been read.
    unread: Vec<u8>,
    offset: usize,
}

impl HeldReader {
    /// Waits for output, then reads into `buf`. Returns `Ok(Some(0))` at the end of the stream,
    /// once the program has exited and all it printed is read or the link is lost, and `Ok(None)`
    /// as soon as the reader has been stopped.
    pub fn read_chunk(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        let mut state = locked(&self.state);
        if *state.stop.borrow() {
            return Ok(None);
        }
        if state.offset == state.unread.len() {
            let ReaderState { output, stop, .. } = &mut *state;
            let Some(output) = output else {
                return Ok(Some(0));
            };
            let next = self.runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = stop.wait_for(|stopped| *stopped) => None,
                    chunk = output.recv() => Some(chunk),
                }
            });
            match next {
                None => return Ok(None),
                Some(None) => return Ok(Some(0)),
                Some(Some(chunk)) => {
                    state.unread = chunk;
                    state.offset = 0;
                }
            }
        }
        let available = &state.unread[state.offset..];
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        state.offset += n;
        Ok(Some(n))
    }
}
