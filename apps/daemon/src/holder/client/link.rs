//! The daemon's link to one holder: the connection, the requests waiting on a reply, and where
//! each terminal's output and exit are routed. A link that drops without the daemon having parted
//! is not taken for the loss of the holder: for as long as the holder's registry file is there and
//! its verified process runs, the link is opened again, with the same terminals, pausing longer
//! after each failed try. A refused connection, a handshake that times out and a link that drops
//! again soon after it was opened again say nothing of whether the holder can go on, so none of
//! them ends it: while it cannot be reached the link is pending, its terminals still running and
//! their calls failing, and it is opened again as soon as the holder answers. Only the holder's
//! end finishes it: a holder that left no file ended on its own, one whose process is gone ended
//! too, one that told the link it was taken over by another daemon is not the daemon's to end,
//! and one that answers with a welcome this build cannot read is ended, its programs with it.
//!
//! A terminal the link hands over to the daemon that comes next ([`Link::hand_over`]) is that
//! daemon's from its mark on: its output ends there, and nothing that would act on it, end it or
//! record it as exited is done over this link any more. The one exception is the daemon's answers
//! to the queries in the output it read before the mark, which nobody else answers: they are still
//! written until the daemon parts ([`Sending::Answers`]). A link opened again after a handover
//! tells the holder which terminals it handed over, and the holder keeps those unlinked.
//!
//! The link also carries the requests for a terminal's companion (its endpoint, its start, its
//! end before the terminal binds it), to a holder whose welcome says it runs companions, and
//! routes what it learns of each companion's end to the companion's handle ([`HeldCompanion`]),
//! for the terminals it spawned and those it found alike.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::companion::HeldCompanion;
use super::held::HeldTerminal;
use super::relay::{RelayHandler, RelayRequest, Relaying};
use super::InService;
use crate::holder::protocol::{
    decode_bulk, BulkKind, CompanionEndpoint, ContainmentSpec, EndpointKind, FromHolder,
    HandoverTail, LastParting, OfflineAnswer, Ready, RelayRoute, SignalKind, SpawnSpec,
    TerminalInfo, ToHolder,
};
use crate::holder::registry::{self, RegistryEntry};
use crate::platform::proc::ProcessIdentity;
use crate::platform::term::Command;

/// How long a holder has to accept the connection and answer the hello, but at the daemon's
/// start, which takes `super::ADOPT_DEADLINE`.
pub(super) const CONNECT_DEADLINE: Duration = Duration::from_secs(10);

/// How long a terminal's tail is waited for after the holder has welcomed the daemon: the holder
/// queues each right behind the welcome.
const TAIL_DEADLINE: Duration = Duration::from_secs(10);

/// How long a link that was opened again has to stay up to count as back. One that drops again
/// sooner goes on with the pause the tries before it reached, rather than trying again at once.
const STABLE_AFTER: Duration = Duration::from_secs(5);

/// The pause before a dropped link is tried again after a try that failed, doubled after each
/// failure up to [`RECONNECT_PAUSE_MAX`], and back to this once a link stays up [`STABLE_AFTER`].
const RECONNECT_PAUSE_MIN: Duration = Duration::from_millis(500);
const RECONNECT_PAUSE_MAX: Duration = Duration::from_secs(30);

/// How long a daemon that is parting waits for the holder to have taken its last frames.
const PART_DEADLINE: Duration = Duration::from_secs(3);

/// How long a handover has to be answered: the holder marks each terminal behind the output it
/// has queued for the daemon, which the daemon reads meanwhile.
const HANDOVER_DEADLINE: Duration = Duration::from_secs(5);

/// How long a request other than a write has to be answered. A holder is on this machine, so a
/// request it does not answer in this long means it is wedged.
pub(super) const REPLY_DEADLINE: Duration = Duration::from_secs(30);

/// Chunks of one terminal's output queued for its reader before the link waits for it. The wait
/// reaches the holder, whose reader then waits on the program's PTY: what a direct PTY does when
/// nobody reads it. Known limit: the wait holds up the whole link, so a reader that stops taking
/// output delays the other terminals of the same holder, and replies to the daemon's requests,
/// until it takes some again.
const OUTPUT_CHUNKS: usize = 256;

/// [`OUTPUT_CHUNKS`] for a terminal found running in a holder: its reader starts only once the
/// daemon has decided what the terminal is, and what the program prints until then waits here
/// rather than holding up the link, which the decision itself (ending a terminal, say) needs.
const ADOPTED_OUTPUT_CHUNKS: usize = 4096;

/// What the daemon is told when a link has ended for good, with the id of the holder it was to.
pub(super) type OnLost = Arc<dyn Fn(&str) + Send + Sync>;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WireOut = futures_util::stream::SplitSink<Socket, Message>;
type WireIn = futures_util::stream::SplitStream<Socket>;

/// The holder answered the hello with something that is no welcome this code can read, so
/// nothing is known of what it is or runs.
#[derive(Debug)]
pub(super) struct Unreadable;

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the holder's welcome cannot be read")
    }
}

impl std::error::Error for Unreadable {}

/// The holder refused the hello: it was taken over by a newer daemon already, so this one is no
/// longer the holder's to link to, and the holder is not this daemon's to end.
#[derive(Debug)]
pub(super) struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the holder was taken over by another daemon")
    }
}

impl std::error::Error for Refused {}

/// A new run id: what tells one daemon's links from every other daemon's, for the holder to
/// recognize a daemon it has already been taken from. A daemon process makes one, with its
/// `Holders`; a test that runs several in one process has each stand for a daemon of its own.
pub(super) fn new_run_id() -> Arc<str> {
    uuid::Uuid::new_v4().to_string().into()
}

/// What the holder said of itself when the link opened.
#[derive(Debug, Clone)]
pub struct Welcome {
    pub holder_id: String,
    pub instance_id: String,
    pub version: String,
    pub build_id: String,
    pub last_parting: LastParting,
    pub terminals: Vec<TerminalInfo>,
    /// The queries the holder answers itself while no daemon is linked; what this link reads it
    /// answers itself, and the tails it is handed it never answers from.
    pub answers_offline: Vec<OfflineAnswer>,
    /// The holder carries out a handover ([`Link::hand_over`]).
    pub handover: bool,
    /// The holder runs companions ([`Link::open_endpoint`], [`Link::start_companion`],
    /// [`Link::end_companion`]).
    pub companions: bool,
    /// The holder accepts no new terminals.
    pub draining: bool,
    /// When the daemon read the welcome, which the ages of output since a handover's mark count
    /// back from.
    pub received_at: Instant,
}

/// A terminal [`Link::adopt`] took: as the welcome listed it, with the handover facts of the
/// welcome its tail came with; the tail; when that welcome was read; and the terminal.
pub(super) struct Adopted {
    pub(super) info: TerminalInfo,
    pub(super) tail: Vec<u8>,
    pub(super) welcomed_at: Instant,
    pub(super) held: HeldTerminal,
    /// The handle on the companion the welcome listed with the terminal.
    pub(super) companion: Option<HeldCompanion>,
}

/// A terminal as the link routes to it.
pub(super) struct Routed {
    /// Set once the holder reports the exit, after all the output before it is queued, or the
    /// link is lost.
    pub(super) exited: AtomicBool,
    /// What the program printed, in order. Closed once it has exited and all of it is queued, or
    /// the link is lost, which is the end of the reader's stream.
    output: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    /// Run when the link was opened again with the terminal still running: what it printed while
    /// the link was down went nowhere, which its owner is told of (see
    /// [`HeldTerminal::on_resumed`](super::HeldTerminal::on_resumed)).
    pub(super) resumed: Mutex<Option<Resumed>>,
    /// Run when the link became pending or stopped being so (see
    /// [`HeldTerminal::on_link_changed`](super::HeldTerminal::on_link_changed)).
    pub(super) link_changed: Mutex<Option<Resumed>>,
}

/// What a terminal's owner asked to be run when the link was opened again, or changed.
pub(super) type Resumed = Arc<dyn Fn() + Send + Sync>;

/// A companion as the link routes what it learns of it to its handle.
#[derive(Default)]
pub(super) struct CompanionRoute {
    /// Set once the companion is known to no longer run; see [`HeldCompanion`].
    pub(super) exited: AtomicBool,
    /// Its terminal bound it: it outlives the connection it was started over.
    bound: AtomicBool,
}

/// What a frame that names a terminal is to a handover ([`Link::hand_over`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sending {
    /// Anything the daemon does with the terminal: not sent once the terminal is handed over.
    Ordinary,
    /// A write of the daemon's answers to the queries in the terminal's output it read. All of
    /// that output came before the terminal's mark, if it has one, and from the mark on the holder
    /// and the next daemon answer only the queries after it, so these are still sent once the
    /// terminal is handed over, until the daemon parts. Anything but a write is sent as
    /// [`Self::Ordinary`].
    Answers,
}

/// What became of a terminal's tail ([`Link::take_tail`]).
enum Tail {
    Taken(Vec<u8>),
    /// It does not come: the terminal is to be ended.
    Missing,
    /// The link is pending: nothing is to be decided about the terminal over it.
    Unlinked,
}

/// A helper's request relayed while the link holds its relays back (see [`Link::release_relays`]).
struct HeldRelay {
    req: u64,
    route: RelayRoute,
    param: String,
    body: Vec<u8>,
    waits: bool,
}

pub(super) struct Link {
    runtime: tokio::runtime::Handle,
    /// The port helpers call the holder on, and the executable they run from.
    helper_port: u16,
    helper_exe: String,
    relay: Arc<dyn RelayHandler>,
    relaying: Relaying,
    /// Where the holder is found again when the link drops, and what is told when it cannot be.
    entry: RegistryEntry,
    /// The run id of the daemon the link is of ([`new_run_id`]), which every hello carries.
    run_id: Arc<str>,
    /// The directory the holder's registry file is in, which a holder that ended on its own has
    /// emptied.
    registry_dir: PathBuf,
    on_lost: OnLost,
    /// The frames for the connection's writer; replaced when the link is opened again.
    out: Mutex<mpsc::UnboundedSender<Message>>,
    alive: AtomicBool,
    /// [`Self::part`] has been called: the end of the connection is the daemon's own.
    parting: AtomicBool,
    /// What [`Self::part`] said of the terminals: handed over or not.
    handover: AtomicBool,
    /// The link was given up with its holder keeping every terminal ([`Self::abandon`]).
    abandoned: AtomicBool,
    /// A dropped connection is being dealt with, which may bring the link back.
    recovering: AtomicBool,
    /// Which run of [`Self::ended`] deals with the latest dropped connection: a run that a newer
    /// one has overtaken (the connection it opened again dropped at once) leaves `recovering` and
    /// `pending` to the newer one. Taken while either is set by a run.
    recovery: Mutex<u64>,
    /// The holder said another daemon took it over, so the end of the connection is that
    /// daemon's doing and not a drop to recover from.
    superseded: AtomicBool,
    /// When the link was last opened again.
    reconnected_at: Mutex<Option<Instant>>,
    /// The connection is down while the holder's process runs, and is tried again in the
    /// background: the link is neither alive nor lost.
    pending: AtomicBool,
    /// The pause before the next try to open the link again.
    pause: Mutex<Duration>,
    /// The daemon gives the holder no more terminals: it is told to drain once it is linked again.
    retired: AtomicBool,
    /// The helpers' requests held back until [`Self::release_relays`], for a link opened to a
    /// holder after the daemon's start: its sessions are adopted first. `None` once released.
    held_relays: Mutex<Option<Vec<HeldRelay>>>,
    /// The terminals this link has released, or told the holder to end for want of an owner, on
    /// the connection it is served over: frames of theirs still on their way are no news. A
    /// connection that opens again starts over from what the welcome lists.
    ended: Mutex<HashSet<String>>,
    /// The launch keys whose revoke could not be sent because the connection was down, sent once
    /// it is opened again.
    revokes: Mutex<Vec<String>>,
    /// How many connections the link has been served over.
    #[cfg(test)]
    connections: AtomicUsize,
    #[cfg(test)]
    pub(super) reconnect_attempts: AtomicUsize,
    /// Attempts to signal the holder, counted before dispatching work to another thread.
    #[cfg(test)]
    pub(super) holder_signal_attempts: AtomicUsize,
    next_request: AtomicU64,
    replies: Mutex<HashMap<u64, std_mpsc::Sender<FromHolder>>>,
    spawns: Mutex<HashMap<String, std_mpsc::Sender<FromHolder>>>,
    terminals: Mutex<HashMap<String, Arc<Routed>>>,
    /// The terminals the holder welcomed the daemon with, until [`Self::adopt`] has taken each
    /// one's tail and takes it: what its reader will read.
    found: Mutex<HashMap<String, mpsc::Receiver<Vec<u8>>>>,
    /// The terminals whose tail the welcome promised and has not arrived yet; a tail of any other
    /// terminal is nobody's to read and is dropped.
    expected_tails: Mutex<HashSet<String>>,
    /// The terminals [`Self::adopt`] has not yet taken the tail of or given up on: when the link
    /// is opened again, the new welcome's tails are expected for these. Locks are taken in the
    /// order `tails`, `adopting`, `expected_tails`, `tail_facts`.
    adopting: Mutex<HashSet<String>>,
    /// The tails the holder sent after its welcome, until they are taken.
    tails: Mutex<HashMap<String, Vec<u8>>>,
    /// For each terminal [`Self::adopt`] has not taken yet, what the welcome its tail comes, or
    /// came, with says of a handover that marked the tail, and when that welcome was read: the
    /// first welcome's, or a later one's when the tail is expected from that.
    tail_facts: Mutex<HashMap<String, (Option<HandoverTail>, Instant)>>,
    tail_arrived: Condvar,
    /// The terminals handed over to the daemon that comes next, by their marks: no frame that
    /// names one is sent from then on, and none is recorded as exited.
    handed_over: Mutex<HashSet<String>>,
    /// The companions of this link's terminals and launches, by terminal id.
    companions: Mutex<HashMap<String, Arc<CompanionRoute>>>,
    /// Where the holder runs inside the daemon's own systemd service, the record that refuses
    /// its programs once a restart is claimed ([`Self::admit`]).
    in_service: OnceLock<Arc<Mutex<InService>>>,
    welcome: Welcome,
}

pub(super) fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

impl Link {
    /// Opens the daemon link of the holder `entry` names and completes the handshake. A holder
    /// whose first frame is no welcome this code reads fails it with [`Unreadable`]. `on_lost` is
    /// called when the link has ended for good. `registry_dir` is where the holder's file is.
    /// With `hold_relays` the helpers' requests the holder relays wait for
    /// [`Self::release_relays`]. The holder has `deadline` to answer the hello, which carries
    /// `run_id`, the daemon's.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn connect(
        entry: &RegistryEntry,
        run_id: Arc<str>,
        registry_dir: PathBuf,
        runtime: tokio::runtime::Handle,
        relay: Arc<dyn RelayHandler>,
        on_lost: OnLost,
        hold_relays: bool,
        deadline: Duration,
    ) -> Result<Arc<Self>> {
        let (wire_out, wire_in, welcome) =
            Self::handshake(entry, &run_id, deadline, Vec::new()).await?;
        // Where the holder runs in a cgroup of its own (a systemd user unit), its launches' groups
        // are made, and found again, below it.
        if let Some(identity) = ProcessIdentity::parse(&entry.started) {
            crate::platform::proc::Containment::note_host(&identity);
        }
        tracing::info!(
            holder = %welcome.holder_id,
            instance = %welcome.instance_id,
            version = %welcome.version,
            answers_offline = ?welcome.answers_offline,
            "linked to a holder"
        );
        // Routed before the first frame after the welcome is read: the terminals the holder runs
        // already print.
        let mut terminals = HashMap::new();
        let mut found = HashMap::new();
        let mut companions = HashMap::new();
        for info in &welcome.terminals {
            if let Some(companion) = &info.companion {
                companions.insert(
                    info.id.clone(),
                    Arc::new(CompanionRoute {
                        exited: AtomicBool::new(companion.exited),
                        bound: AtomicBool::new(true),
                    }),
                );
            }
            let (output, events) = mpsc::channel(ADOPTED_OUTPUT_CHUNKS);
            terminals.insert(
                info.id.clone(),
                Arc::new(Routed {
                    exited: AtomicBool::new(info.exited),
                    // Nothing more is read from a terminal that had exited already.
                    output: Mutex::new((!info.exited).then_some(output)),
                    resumed: Mutex::new(None),
                    link_changed: Mutex::new(None),
                }),
            );
            found.insert(info.id.clone(), events);
        }
        let link = Arc::new(Self {
            runtime,
            helper_port: entry.port,
            helper_exe: entry.exe.clone(),
            relay,
            relaying: Relaying::default(),
            entry: entry.clone(),
            run_id,
            registry_dir,
            on_lost,
            // Closed until the connection's writer is started.
            out: Mutex::new(mpsc::unbounded_channel().0),
            alive: AtomicBool::new(false),
            parting: AtomicBool::new(false),
            handover: AtomicBool::new(false),
            abandoned: AtomicBool::new(false),
            recovering: AtomicBool::new(false),
            recovery: Mutex::new(0),
            superseded: AtomicBool::new(false),
            reconnected_at: Mutex::new(None),
            pending: AtomicBool::new(false),
            pause: Mutex::new(RECONNECT_PAUSE_MIN),
            retired: AtomicBool::new(false),
            held_relays: Mutex::new(hold_relays.then(Vec::new)),
            ended: Mutex::new(HashSet::new()),
            revokes: Mutex::new(Vec::new()),
            #[cfg(test)]
            connections: AtomicUsize::new(0),
            #[cfg(test)]
            reconnect_attempts: AtomicUsize::new(0),
            #[cfg(test)]
            holder_signal_attempts: AtomicUsize::new(0),
            next_request: AtomicU64::new(1),
            replies: Mutex::new(HashMap::new()),
            spawns: Mutex::new(HashMap::new()),
            terminals: Mutex::new(terminals),
            found: Mutex::new(found),
            expected_tails: Mutex::new(
                welcome
                    .terminals
                    .iter()
                    .map(|info| info.id.clone())
                    .collect(),
            ),
            adopting: Mutex::new(
                welcome
                    .terminals
                    .iter()
                    .map(|info| info.id.clone())
                    .collect(),
            ),
            tails: Mutex::new(HashMap::new()),
            tail_facts: Mutex::new(
                welcome
                    .terminals
                    .iter()
                    .map(|info| {
                        (
                            info.id.clone(),
                            (info.handover.clone(), welcome.received_at),
                        )
                    })
                    .collect(),
            ),
            tail_arrived: Condvar::new(),
            handed_over: Mutex::new(HashSet::new()),
            companions: Mutex::new(companions),
            in_service: OnceLock::new(),
            welcome,
        });
        link.serve(wire_out, wire_in);
        Ok(link)
    }

    /// Opens the connection to the holder `entry` names and exchanges hello and welcome, within
    /// `deadline`. The hello carries `run_id` and names `handed_over`, the terminals this link
    /// handed over before it dropped, which the holder then keeps unlinked rather than send their
    /// output to a daemon that is done with it.
    async fn handshake(
        entry: &RegistryEntry,
        run_id: &str,
        deadline: Duration,
        handed_over: Vec<String>,
    ) -> Result<(WireOut, WireIn, Welcome)> {
        let url = format!(
            "ws://127.0.0.1:{}/daemon?secret={}",
            entry.port, entry.secret
        );
        // One deadline for the whole handshake: a holder that accepts the connection and never
        // answers, or a foreign program on a port a registry file went stale on, must not hold
        // up what waits on this.
        tokio::time::timeout(deadline, async {
            let (socket, _) = tokio_tungstenite::connect_async(url)
                .await
                .context("connecting to the holder")?;
            let (mut wire_out, mut wire_in) = socket.split();
            let hello = ToHolder::Hello {
                version: env!("CARGO_PKG_VERSION").to_string(),
                build_id: crate::holder::build_id().to_string(),
                run_id: Some(run_id.to_string()),
                handed_over,
            };
            wire_out
                .send(Message::Text(serde_json::to_string(&hello)?))
                .await
                .context("greeting the holder")?;
            while let Some(message) = wire_in.next().await {
                if let Message::Text(text) = message.context("reading the holder's welcome")? {
                    if matches!(
                        serde_json::from_str::<FromHolder>(&text),
                        Ok(FromHolder::Superseded { .. })
                    ) {
                        return Err(Refused.into());
                    }
                    // The welcome is the holder's first frame, so any other first frame is a
                    // protocol this code does not speak.
                    let Ok(FromHolder::Welcome {
                        holder_id,
                        instance_id,
                        version,
                        build_id,
                        last_parting,
                        terminals,
                        answers_offline,
                        handover,
                        companions,
                        draining,
                        ..
                    }) = serde_json::from_str(&text)
                    else {
                        return Err(Unreadable.into());
                    };
                    let welcome = Welcome {
                        holder_id,
                        instance_id,
                        version,
                        build_id,
                        last_parting,
                        terminals,
                        answers_offline,
                        handover,
                        companions,
                        draining,
                        received_at: Instant::now(),
                    };
                    return Ok((wire_out, wire_in, welcome));
                }
            }
            bail!("the holder closed the link before it welcomed the daemon")
        })
        .await
        .map_err(|_| anyhow!("the holder did not welcome the daemon in time"))?
    }

    /// Starts the tasks that write the frames of [`Self::send`] to the connection and route what
    /// it carries, and marks the link alive.
    fn serve(self: &Arc<Self>, mut wire_out: WireOut, mut wire_in: WireIn) {
        let (out, mut outbox) = mpsc::unbounded_channel::<Message>();
        *locked(&self.out) = out;
        #[cfg(test)]
        self.connections.fetch_add(1, Ordering::AcqRel);
        self.alive.store(true, Ordering::Release);
        self.runtime.spawn(async move {
            while let Some(message) = outbox.recv().await {
                let closing = matches!(message, Message::Close(_));
                if wire_out.send(message).await.is_err() || closing {
                    break;
                }
            }
            wire_out.close().await.ok();
        });
        let reading = self.clone();
        self.runtime.spawn(async move {
            // Whatever ends the reader, a panic in what it routes included, the end of the
            // connection is dealt with: otherwise the link would stay alive with nobody reading.
            let mut ends = EndsLink(Some(reading.clone()));
            while let Some(Ok(message)) = wire_in.next().await {
                match message {
                    Message::Text(text) => reading.route_text(&text).await,
                    Message::Binary(bytes) => reading.route_bulk(&bytes).await,
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            if let Some(link) = ends.0.take() {
                link.ended().await;
            }
        });
    }

    /// The connection is over. One the daemon did not part from, and that no other daemon took
    /// over, is opened again for as long as the holder runs ([`Self::recover`]); only when it
    /// stays lost is it finished, every terminal reading as exited.
    async fn ended(self: Arc<Self>) {
        let run = {
            let mut recovery = locked(&self.recovery);
            *recovery += 1;
            self.recovering.store(true, Ordering::Release);
            *recovery
        };
        self.suspend();
        let ours =
            !self.parting.load(Ordering::Acquire) && !self.superseded.load(Ordering::Acquire);
        let back = ours && self.recover(run).await;
        if !back {
            self.finish();
        }
        // After `finish`, so that an owner told the link is no longer pending finds its terminal
        // exited when it is lost.
        self.if_current(run, || {
            self.recovering.store(false, Ordering::Release);
            self.set_pending(false);
        });
        if !back {
            (self.on_lost)(&self.welcome.holder_id);
        }
    }

    /// Starts a run of dealing with a dropped connection as [`Self::ended`] does, for a test of
    /// what an overtaken run may still change.
    #[cfg(test)]
    pub(super) fn next_recovery_run(&self) -> u64 {
        let mut recovery = locked(&self.recovery);
        *recovery += 1;
        *recovery
    }

    /// Runs `update` of the link's recovery state if `run` of [`Self::ended`] is still the latest.
    fn if_current(&self, run: u64, update: impl FnOnce()) {
        let recovery = locked(&self.recovery);
        if *recovery == run {
            update();
        }
    }

    /// Opens the link again, the terminals the holder still runs staying as they were, trying for
    /// as long as the holder's registry file is there and its verified process runs. After the
    /// first try that fails the link is pending: calls on it fail at once rather than wait, and
    /// the tries go on, each after a longer pause, up to [`RECONNECT_PAUSE_MAX`]; a link that was
    /// opened again moments ago and dropped already goes on with the pause it had reached. `false`
    /// when the link stays lost: the holder ended, another daemon took it over, the daemon is
    /// parting, or the holder answered with a welcome this build cannot read, which ends it.
    async fn recover(self: &Arc<Self>, run: u64) -> bool {
        let mut pause = if self.reconnected_lately() {
            Some(self.next_pause())
        } else {
            *locked(&self.pause) = RECONNECT_PAUSE_MIN;
            None
        };
        loop {
            if let Some(pause) = pause.take() {
                self.wait_in_background(run);
                tokio::time::sleep(pause).await;
                if self.parting.load(Ordering::Acquire) {
                    return false;
                }
            }
            let Some(identity) = self.holder_running().await else {
                return false;
            };
            // Right before the hello: a link given up must not take the holder from the link
            // that took its place, and one the daemon parts from is not opened again for it.
            if self.parting.load(Ordering::Acquire) {
                return false;
            }
            if !self.pending.load(Ordering::Acquire) {
                tracing::warn!(holder = %self.entry.holder_id, "the link to a holder dropped; linking again");
            }
            // A terminal handed over is the next daemon's: the holder is told not to send its
            // output here, so that no daemon is linked for it and the holder answers the queries
            // in it it may, as it did while the link was down.
            let handed_over: Vec<String> = locked(&self.handed_over).iter().cloned().collect();
            match Self::handshake(&self.entry, &self.run_id, CONNECT_DEADLINE, handed_over).await {
                Ok((wire_out, wire_in, welcome)) => {
                    return self.reopened(run, wire_out, wire_in, &welcome);
                }
                Err(err) if err.downcast_ref::<Refused>().is_some() => {
                    tracing::warn!(holder = %self.entry.holder_id, "the holder was taken over by another daemon; it is left to it");
                    self.superseded.store(true, Ordering::Release);
                    return false;
                }
                Err(err) if err.downcast_ref::<Unreadable>().is_some() => {
                    tracing::warn!(holder = %self.entry.holder_id, "the holder answered with a welcome this build cannot read; it is ended");
                    self.end_holder(identity).await;
                    return false;
                }
                Err(err) => {
                    tracing::warn!(holder = %self.entry.holder_id, %err, "the holder could not be linked to again; trying again later");
                    pause = Some(self.next_pause());
                    #[cfg(test)]
                    self.reconnect_attempts.fetch_add(1, Ordering::Release);
                }
            }
        }
    }

    /// The link is pending: what waits on it being dealt with is let go, its owners are told, and
    /// the tries go on in the background.
    pub(super) fn wait_in_background(&self, run: u64) {
        self.if_current(run, || {
            self.set_pending(true);
            self.recovering.store(false, Ordering::Release);
        });
    }

    /// The pause before the next try, and the next one's doubled.
    fn next_pause(&self) -> Duration {
        let mut pause = locked(&self.pause);
        let now = *pause;
        *pause = (now * 2).min(RECONNECT_PAUSE_MAX);
        now
    }

    /// Takes the connection the link was opened again over into use; `false` when the link was
    /// given up meanwhile, and the connection is dropped.
    fn reopened(
        self: &Arc<Self>,
        run: u64,
        wire_out: WireOut,
        wire_in: WireIn,
        welcome: &Welcome,
    ) -> bool {
        *locked(&self.reconnected_at) = Some(Instant::now());
        if self.abandoned.load(Ordering::Acquire) {
            // Given up while this was opened: closed without a word, neither served nor parted
            // over, so that nothing is said to the holder in its name.
            drop((wire_out, wire_in));
            return false;
        }
        if self.parting.load(Ordering::Acquire) {
            // The daemon is going: the connection is served only to say so over it, here as well
            // as by `part`, which may have been and gone while the link was pending.
            self.serve(wire_out, wire_in);
            self.send_now(&ToHolder::Parting {
                handover: self.handover.load(Ordering::Acquire),
            });
            locked(&self.out).send(Message::Close(None)).ok();
            return true;
        }
        self.resume(welcome);
        self.serve(wire_out, wire_in);
        self.end_unknown(welcome);
        self.send_revokes();
        if self.retired.load(Ordering::Acquire) {
            self.send_now(&ToHolder::Drain);
        }
        // Not if the connection served just now dropped already and a newer run deals with it.
        self.if_current(run, || self.set_pending(false));
        self.announce_resumed(welcome);
        true
    }

    /// Records whether the link is pending, and tells the owner of each terminal when that
    /// changes. The owners read the state when told, so hooks that run late or out of order still
    /// leave what they publish right.
    fn set_pending(&self, pending: bool) {
        if self.pending.swap(pending, Ordering::AcqRel) == pending {
            return;
        }
        if pending {
            tracing::warn!(holder = %self.entry.holder_id, "the holder cannot be linked to for now; its terminals keep running, and it is tried again");
        }
        // A terminal handed over is the next daemon's, whatever the link does after.
        let handed_over = locked(&self.handed_over).clone();
        let hooks: Vec<Resumed> = locked(&self.terminals)
            .iter()
            .filter(|(id, _)| !handed_over.contains(*id))
            .filter_map(|(_, routed)| locked(&routed.link_changed).clone())
            .collect();
        for hook in hooks {
            self.runtime.spawn_blocking(move || hook());
        }
    }

    /// Whether the link is pending: down while its holder runs, and tried again in the background.
    pub(super) fn pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }

    /// Whether the holder is gone for good: the link is down, not on its way back, and not tried
    /// again.
    pub(super) fn lost(&self) -> bool {
        !self.alive() && !self.recovering.load(Ordering::Acquire) && !self.pending()
    }

    /// Whether the holder runs a program of this daemon: a terminal or a companion that has not
    /// exited, or is not released yet.
    pub(super) fn runs_programs(&self) -> bool {
        let terminal = locked(&self.terminals)
            .values()
            .any(|routed| !routed.exited.load(Ordering::Acquire));
        terminal
            || locked(&self.companions)
                .values()
                .any(|route| !route.exited.load(Ordering::Acquire))
    }

    /// Marks the holder as running inside the daemon's own systemd service, whose `service`
    /// record then refuses its programs once a stop or restart is claimed.
    pub(super) fn place_in_service(&self, service: Arc<Mutex<InService>>) {
        let _ = self.in_service.set(service);
    }

    /// Admits a program to the holder: refused, coded as the daemon shutting down, once a stop or
    /// restart is claimed where the holder runs in the daemon's service. The guard is the
    /// record's lock, held by the caller until the program is recorded on the link, so that a
    /// claim sees it.
    fn admit(&self) -> Result<Option<MutexGuard<'_, InService>>> {
        let Some(service) = self.in_service.get() else {
            return Ok(None);
        };
        let service = locked(service);
        service.admit()?;
        Ok(Some(service))
    }

    /// Gives the holder no more terminals: it is told to drain now if it is linked, or once it is
    /// linked again.
    pub(super) fn retire(&self) {
        self.retired.store(true, Ordering::Release);
        self.send_now(&ToHolder::Drain);
    }

    /// Hands the helpers' requests held back so far to the daemon, in the order the holder
    /// relayed them, and every later one as it comes.
    pub(super) fn release_relays(self: &Arc<Self>) {
        let _runtime = self.runtime.enter();
        // Handed over under the lock, so that a request relayed meanwhile waits behind them.
        let mut held = locked(&self.held_relays);
        for relayed in held.take().unwrap_or_default() {
            self.relay_request(
                relayed.req,
                relayed.route,
                relayed.param,
                relayed.body,
                relayed.waits,
            );
        }
    }

    /// The run id of the daemon the link is of.
    #[cfg(test)]
    pub(super) fn run_id(&self) -> &str {
        &self.run_id
    }

    /// The holder's process identity, when its registry file names one that can be read.
    pub(super) fn identity(&self) -> Option<ProcessIdentity> {
        ProcessIdentity::parse(&self.entry.started)
    }

    /// The process of the holder, when there is one to link to again: its registry file is still
    /// there (a holder that ended on its own removed it) and its process runs.
    async fn holder_running(&self) -> Option<ProcessIdentity> {
        let (registry_dir, holder_id) = (self.registry_dir.clone(), self.entry.holder_id.clone());
        let listed = tokio::task::spawn_blocking(move || registry::read(&registry_dir, &holder_id))
            .await
            .ok()
            .flatten();
        if listed.is_none() {
            tracing::info!(holder = %self.entry.holder_id, "the holder left no registry file, so it ended on its own");
            return None;
        }
        let identity = ProcessIdentity::parse(&self.entry.started)?;
        identity.is_running().then_some(identity)
    }

    /// Whether the link was opened again less than [`STABLE_AFTER`] ago.
    fn reconnected_lately(&self) -> bool {
        locked(&self.reconnected_at).is_some_and(|at| at.elapsed() < STABLE_AFTER)
    }

    /// Ends the holder's process (see [`super::end_process`]) without blocking the runtime.
    async fn end_holder(&self, identity: ProcessIdentity) {
        #[cfg(test)]
        self.holder_signal_attempts.fetch_add(1, Ordering::Release);
        let ended = tokio::task::spawn_blocking(move || super::end_process(&identity)).await;
        if !matches!(ended, Ok(Ok(()))) {
            tracing::warn!(holder = %self.entry.holder_id, "ending the holder failed; it is left running");
        }
    }

    /// Brings the routing in line with the holder's welcome after the link was opened again: a
    /// terminal it no longer runs is over, one it reports as exited is too. The tail frames that
    /// follow are not wanted, since each terminal's output goes on from where it stopped, except
    /// for a terminal [`Self::adopt`] has not yet taken the tail of: the new welcome's tail is
    /// expected for it, unless the one from before the link dropped has arrived already.
    fn resume(&self, welcome: &Welcome) {
        {
            let tails = locked(&self.tails);
            let adopting = locked(&self.adopting);
            let mut expected = locked(&self.expected_tails);
            *expected = welcome
                .terminals
                .iter()
                .filter(|info| adopting.contains(&info.id) && !tails.contains_key(&info.id))
                .map(|info| info.id.clone())
                .collect();
            // The tail that comes now is this welcome's, and so is what it says of the tail.
            let mut facts = locked(&self.tail_facts);
            for info in &welcome.terminals {
                if expected.contains(&info.id) {
                    facts.insert(
                        info.id.clone(),
                        (info.handover.clone(), welcome.received_at),
                    );
                }
            }
        }
        let handed_over = locked(&self.handed_over).clone();
        for (id, routed) in locked(&self.terminals).iter() {
            // The next daemon's, whatever the holder says of it.
            if handed_over.contains(id) {
                continue;
            }
            match welcome.terminals.iter().find(|info| &info.id == id) {
                Some(info) if !info.exited => {}
                Some(_) => routed.exited.store(true, Ordering::Release),
                None => {
                    routed.exited.store(true, Ordering::Release);
                    routed.close_output();
                }
            }
        }
        // A companion runs on only where the welcome lists it running with its terminal: one no
        // terminal had bound was ended with the connection it was asked over.
        for (id, route) in locked(&self.companions).iter() {
            if handed_over.contains(id) {
                continue;
            }
            let running = welcome
                .terminals
                .iter()
                .find(|info| &info.id == id)
                .and_then(|info| info.companion.as_ref())
                .is_some_and(|companion| !companion.exited);
            if !running {
                route.exited.store(true, Ordering::Release);
            }
        }
    }

    /// Ends the terminals the welcome lists that this link does not own, whatever it did about
    /// them before the link dropped: the welcome is what the holder runs now. That is a terminal
    /// whose start was not confirmed before the link dropped, and one whose release or end was
    /// lost with the connection. `ended` then keeps only what the welcome lists, to tell the
    /// frames of those terminals on the new connection from news.
    fn end_unknown(&self, welcome: &Welcome) {
        let known = locked(&self.terminals);
        let mut ended = locked(&self.ended);
        ended.retain(|id| welcome.terminals.iter().any(|info| &info.id == id));
        for info in &welcome.terminals {
            if !known.contains_key(&info.id) {
                ended.insert(info.id.clone());
                self.end_ownerless(&info.id);
            }
        }
    }

    /// A frame names a terminal this link does not know and expects no start of: its program has
    /// no owner, so it is killed and released, once.
    fn disown(&self, id: &str) {
        if self.abandoned.load(Ordering::Acquire) || self.is_handed_over(id) {
            return;
        }
        if locked(&self.ended).insert(id.to_string()) {
            tracing::warn!(terminal = %id, "the holder runs a terminal nobody owns; it is ended");
            self.end_ownerless(id);
        }
    }

    /// Has the holder kill the program of the terminal `id`, hang it up and forget it.
    fn end_ownerless(&self, id: &str) {
        if self.abandoned.load(Ordering::Acquire) || self.is_handed_over(id) {
            return;
        }
        let id = id.to_string();
        self.send_now(&ToHolder::Signal {
            id: id.clone(),
            signal: SignalKind::Kill,
        });
        self.send_now(&ToHolder::HangUp { id: id.clone() });
        self.send_now(&ToHolder::Release { id });
    }

    /// Tells the owner of each terminal the holder still runs that the link was down: what the
    /// terminal printed meanwhile was never routed. The owners' hooks wait on the holder's answers,
    /// which this runtime routes, so they run on threads of their own.
    fn announce_resumed(&self, welcome: &Welcome) {
        // A terminal handed over is the next daemon's, so its owner is not told.
        let handed_over = locked(&self.handed_over).clone();
        let hooks: Vec<Resumed> = {
            let terminals = locked(&self.terminals);
            welcome
                .terminals
                .iter()
                .filter(|info| !info.exited && !handed_over.contains(&info.id))
                .filter_map(|info| terminals.get(&info.id))
                .filter_map(|routed| locked(&routed.resumed).clone())
                .collect()
        };
        for hook in hooks {
            self.runtime.spawn_blocking(move || hook());
        }
    }

    pub(super) fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    /// Takes the terminals the holder welcomed the daemon with, each with the tail the holder had
    /// kept of its output. **Blocks** until the tails are in, or the link drops. A terminal whose
    /// tail does not come over a live link (the holder no longer lists it, or the tail is not sent
    /// in time), or because the link is lost for good, is left out, and the holder is told to
    /// forget the terminal, which would otherwise be held for good by nobody. `None` when the
    /// link drops meanwhile and is not lost: nothing is adopted, nothing is ended, and the link is
    /// given up ([`Self::abandon`]) with the holder keeping every terminal, for a new link to
    /// adopt once it answers.
    pub(super) fn adopt(self: &Arc<Self>) -> Option<Vec<Adopted>> {
        let mut adopted = Vec::new();
        for info in &self.welcome.terminals {
            let routed = locked(&self.terminals).get(&info.id).cloned();
            let Some(routed) = routed else {
                locked(&self.found).remove(&info.id);
                locked(&self.tail_facts).remove(&info.id);
                self.stop_adopting(&info.id);
                continue;
            };
            let tail = match self.take_tail(&info.id) {
                Tail::Taken(tail) => Some(tail),
                Tail::Missing => None,
                Tail::Unlinked => {
                    tracing::warn!(holder = %self.entry.holder_id, "the holder could not be reached while its terminals were taken over; it is left to be tried again");
                    // Before what was taken so far is dropped, which would release it.
                    self.abandon();
                    return None;
                }
            };
            // What the welcome the tail came with says of it: decided by the time the tail is.
            let (handover, welcomed_at) = locked(&self.tail_facts)
                .remove(&info.id)
                .unwrap_or_else(|| (info.handover.clone(), self.welcome.received_at));
            // Taken only after the tail, so that `keep_tail` can still drop output queued before
            // a reconnect's tail.
            let Some(events) = locked(&self.found).remove(&info.id) else {
                continue;
            };
            let held = HeldTerminal::new(self.clone(), info.id.clone(), info.pid, routed, events);
            let companion = info.companion.as_ref().and_then(|found| {
                let route = locked(&self.companions).get(&info.id).cloned()?;
                Some(HeldCompanion::new(
                    self.clone(),
                    info.id.clone(),
                    route,
                    found.pid,
                    found.identity.as_deref(),
                ))
            });
            match tail {
                Some(tail) => adopted.push(Adopted {
                    info: TerminalInfo {
                        handover,
                        ..info.clone()
                    },
                    tail,
                    welcomed_at,
                    held,
                    companion,
                }),
                None => {
                    tracing::warn!(terminal = %info.id, "the holder did not send the terminal's tail; it is ended");
                    held.end();
                }
            }
        }
        Some(adopted)
    }

    /// Gives the link up without ending or releasing anything its holder runs: no frame that ends
    /// or forgets a terminal is sent over it from now on, it is not opened again, and a connection
    /// opened meanwhile is only parted over. The holder keeps every terminal for the link that
    /// takes it over next.
    pub(super) fn abandon(&self) {
        self.abandoned.store(true, Ordering::Release);
        self.parting.store(true, Ordering::Release);
    }

    /// The tail of the terminal `id`: from the welcome of the connection that was served when the
    /// terminal was welcomed, or, when that dropped, from the next one's. Missing when the link is
    /// lost for good, the holder no longer lists the terminal, or the tail does not come in time
    /// over a live link; unlinked when the link dropped and is being opened again or pending.
    /// The terminal is no longer adopted by the time this returns, decided under the `tails` lock
    /// so that a reconnect does not expect a tail nobody takes.
    fn take_tail(&self, id: &str) -> Tail {
        let deadline = Instant::now() + TAIL_DEADLINE;
        let mut tails = locked(&self.tails);
        loop {
            if let Some(tail) = tails.remove(id) {
                locked(&self.adopting).remove(id);
                return Tail::Taken(tail);
            }
            // After any reconnect, the latest welcome no longer listing the terminal means no
            // tail is coming. Not before one: the first connection's tail may not have been
            // routed yet.
            let reconnected = locked(&self.reconnected_at).is_some();
            if reconnected && !locked(&self.expected_tails).contains(id) {
                locked(&self.adopting).remove(id);
                return Tail::Missing;
            }
            if !self.alive() {
                break;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                locked(&self.adopting).remove(id);
                return Tail::Missing;
            };
            tails = self
                .tail_arrived
                .wait_timeout(tails, left)
                .unwrap_or_else(|err| err.into_inner())
                .0;
        }
        drop(tails);
        // The connection dropped. A link being opened again, or pending, is no reason to end the
        // terminal, and not waited for, so that nothing that adopts waits on a holder; a link
        // lost for good gives up on the tail. `ended` marks the link recovering before it stops
        // being alive, so a link that is neither is lost.
        if self.pending() || self.recovering.load(Ordering::Acquire) {
            return Tail::Unlinked;
        }
        self.stop_adopting(id);
        Tail::Missing
    }

    /// Takes the terminal `id` out of [`Self::adopting`], under the lock of the tails so that a
    /// reconnect either expects its tail before this or not at all.
    fn stop_adopting(&self, id: &str) {
        let _tails = locked(&self.tails);
        locked(&self.adopting).remove(id);
    }

    pub(super) fn runtime(&self) -> &tokio::runtime::Handle {
        &self.runtime
    }

    /// The port helpers call this link's holder on.
    pub(super) fn helper_port(&self) -> u16 {
        self.helper_port
    }

    /// The executable the holder runs from, which its helpers run too.
    pub(super) fn helper_exe(&self) -> &str {
        &self.helper_exe
    }

    pub(super) fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Closes the connection as a crash of the daemon's side would, without parting.
    #[cfg(test)]
    pub(super) fn drop_connection(&self) {
        locked(&self.out).send(Message::Close(None)).ok();
    }

    /// How many connections the link has been served over: more than one once it was opened
    /// again.
    #[cfg(test)]
    pub(super) fn connections(&self) -> usize {
        self.connections.load(Ordering::Acquire)
    }

    /// Waits for a dropped connection to have been dealt with, when one is, so that `alive` tells
    /// whether the link came back. **Blocks** for as long as the handshake may take.
    pub(super) fn settled(&self) {
        let deadline = std::time::Instant::now() + CONNECT_DEADLINE + Duration::from_secs(5);
        while self.recovering.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Sends `message`, after waiting for a connection that dropped to have been dealt with
    /// (see [`Self::settled`]); `false` once the link is lost. **Blocks** meanwhile, so never on
    /// the link's own runtime.
    pub(super) fn send(&self, message: &ToHolder) -> bool {
        self.settled();
        self.send_now(message)
    }

    /// Sends `message` if the link is alive now, without waiting; `false` once it is lost or while
    /// it is being opened again.
    fn send_now(&self, message: &ToHolder) -> bool {
        self.send_now_as(message, Sending::Ordinary)
    }

    /// [`Self::send_now`], sending `message` as `sending` says.
    fn send_now_as(&self, message: &ToHolder, sending: Sending) -> bool {
        if !self.alive() {
            return false;
        }
        // A terminal handed over is the next daemon's: whatever this daemon still does with it
        // (a release, a signal, a hang-up, a write, a request) is not sent, as over a link that
        // is gone. Only its answers to the output it read before the mark still go, until it
        // parts: nobody else answers those queries.
        let answers = sending == Sending::Answers
            && matches!(message, ToHolder::Write { .. })
            && !self.parting.load(Ordering::Acquire);
        if !answers && terminal_of(message).is_some_and(|id| self.is_handed_over(id)) {
            return false;
        }
        let text = serde_json::to_string(message).expect("a message is JSON");
        locked(&self.out).send(Message::Text(text)).is_ok()
    }

    /// Sends the request `make` builds from a fresh request number and waits for the reply, up to
    /// `deadline` (for ever when `None`). `None` when the link is lost, or the deadline passes.
    pub(super) fn request(
        &self,
        deadline: Option<Duration>,
        make: impl FnOnce(u64) -> ToHolder,
    ) -> Option<FromHolder> {
        self.request_as(Sending::Ordinary, deadline, make)
    }

    /// [`Self::request`], sending the request as `sending` says.
    pub(super) fn request_as(
        &self,
        sending: Sending,
        deadline: Option<Duration>,
        make: impl FnOnce(u64) -> ToHolder,
    ) -> Option<FromHolder> {
        // A call made while the link is being opened again waits for that, not for an answer that
        // a link in between cannot give.
        self.settled();
        let req = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (reply, replied) = std_mpsc::channel();
        locked(&self.replies).insert(req, reply);
        // Checked after the reply is registered: a link lost in between has cleared the replies
        // already and would never answer this one.
        if !self.send_now_as(&make(req), sending) {
            locked(&self.replies).remove(&req);
            return None;
        }
        let answered = match deadline {
            Some(deadline) => replied.recv_timeout(deadline).ok(),
            None => replied.recv().ok(),
        };
        if answered.is_none() {
            locked(&self.replies).remove(&req);
        }
        answered
    }

    /// Has the holder issue the key the helpers of the launch of `terminal` (a program of the
    /// session `session`, with the MCP token `mcp_token` when it has one) are given. **Blocks.**
    pub(super) fn issue_launch(
        &self,
        terminal: &str,
        session: &str,
        mcp_token: Option<&str>,
    ) -> Result<String> {
        let issued = self.request(Some(REPLY_DEADLINE), |req| ToHolder::IssueLaunch {
            id: terminal.to_string(),
            req,
            session: session.to_string(),
            mcp_token: mcp_token.map(str::to_string),
        });
        match issued {
            Some(FromHolder::LaunchIssued { key: Some(key), .. }) => Ok(key),
            Some(_) => bail!("the holder could not issue a key for the launch"),
            None => bail!("the holder did not issue a key for the launch"),
        }
    }

    /// Tells the holder a key it issued is not wanted. Never waits, as it may be called from a
    /// drop anywhere: one that cannot be sent while the connection is down is sent once it is
    /// opened again.
    pub(super) fn revoke_launch(&self, terminal: &str) {
        // The holder ends the launch's unbound companion with the key, as it does with the
        // connection should the revoke not get through.
        self.forget_companion(terminal);
        let revoke = ToHolder::RevokeLaunch {
            id: terminal.to_string(),
        };
        if !self.send_now(&revoke) {
            locked(&self.revokes).push(terminal.to_string());
        }
    }

    /// Sends the revokes that were held back while the connection was down.
    fn send_revokes(&self) {
        let held = std::mem::take(&mut *locked(&self.revokes));
        for id in held {
            self.revoke_launch(&id);
        }
    }

    /// Fails unless the holder runs companions: a frame of theirs is never sent to one that does
    /// not.
    fn runs_companions(&self) -> Result<()> {
        if !self.welcome.companions {
            bail!("the holder runs no companions");
        }
        Ok(())
    }

    /// Has the holder open the endpoint `kind` asks for, for the companion of the terminal `id`.
    /// **Blocks.**
    pub(super) fn open_endpoint(&self, id: &str, kind: EndpointKind) -> Result<CompanionEndpoint> {
        self.runs_companions()?;
        let opened = self.request(Some(REPLY_DEADLINE), |req| ToHolder::OpenEndpoint {
            id: id.to_string(),
            req,
            kind,
        });
        match opened {
            Some(FromHolder::EndpointOpened {
                endpoint: Some(endpoint),
                ..
            }) => {
                // The holder ended whatever companion it kept unbound under the id.
                self.forget_companion(id);
                Ok(endpoint)
            }
            Some(FromHolder::EndpointOpened {
                error: Some(error), ..
            }) => bail!("{error}"),
            Some(_) => bail!("the holder opened no endpoint for the companion"),
            None => bail!("the holder did not answer the opening of the companion's endpoint"),
        }
    }

    /// Has the holder start the companion of the terminal `id` on the endpoint opened for it, and
    /// waits for it to be ready, or not, as the holder takes, ended only by its answer or the end
    /// of the connection, as a spawn is. The companion's handle, and the port it reported.
    /// **Blocks.**
    pub(super) fn start_companion(
        self: &Arc<Self>,
        id: &str,
        command: Command,
        ready: Ready,
        within: Duration,
        containment: Option<ContainmentSpec>,
    ) -> Result<(HeldCompanion, Option<u16>)> {
        self.runs_companions()?;
        // A connection that dropped is dealt with first, so that the route is not cleared with the
        // connection it was not started over.
        self.settled();
        // Routed before the request goes out: the companion's end may arrive before the answer is
        // read.
        let route = Arc::new(CompanionRoute::default());
        let admitted = self.admit()?;
        locked(&self.companions).insert(id.to_string(), route.clone());
        drop(admitted);
        let started = self.request(None, |req| ToHolder::StartCompanion {
            id: id.to_string(),
            req,
            command: Box::new(command),
            ready,
            ready_within_ms: u64::try_from(within.as_millis()).unwrap_or(u64::MAX),
            containment,
        });
        let failure = match started {
            Some(FromHolder::CompanionStarted {
                pid: Some(pid),
                identity,
                port,
                error: None,
                ..
            }) => {
                let companion = HeldCompanion::new(
                    self.clone(),
                    id.to_string(),
                    route,
                    pid,
                    identity.as_deref(),
                );
                return Ok((companion, port));
            }
            Some(FromHolder::CompanionStarted {
                error: Some(error), ..
            }) => error,
            Some(_) => "the holder started no companion".to_string(),
            None => "the link to the holder was lost before it answered the companion's start"
                .to_string(),
        };
        route.exited.store(true, Ordering::Release);
        {
            let mut companions = locked(&self.companions);
            if companions
                .get(id)
                .is_some_and(|routed| Arc::ptr_eq(routed, &route))
            {
                companions.remove(id);
            }
        }
        bail!("{failure}")
    }

    /// Has the holder end the companion of the terminal `id` while no terminal has bound it, and
    /// waits for that. **Blocks.**
    pub(super) fn end_companion(&self, id: &str) -> Result<()> {
        self.runs_companions()?;
        let ended = self.request(Some(REPLY_DEADLINE), |req| ToHolder::EndCompanion {
            id: id.to_string(),
            req,
        });
        match ended {
            Some(FromHolder::Done { error: None, .. }) => {
                self.forget_companion(id);
                Ok(())
            }
            Some(FromHolder::Done {
                error: Some(error), ..
            }) => bail!("{error}"),
            Some(_) => bail!("the holder answered the companion's end with something else"),
            None => bail!("the holder did not answer the companion's end"),
        }
    }

    /// Stops routing to the companion of the terminal `id`, which reads as exited: its holder
    /// ended it, or ends it with its terminal. One the next daemon takes over keeps running.
    fn forget_companion(&self, id: &str) {
        let removed = locked(&self.companions).remove(id);
        let Some(route) = removed else {
            return;
        };
        let planned = self.parting.load(Ordering::Acquire) && self.handover.load(Ordering::Acquire);
        if !planned && !self.is_handed_over(id) {
            route.exited.store(true, Ordering::Release);
        }
    }

    /// Starts the terminal `spec` describes in the holder and waits for it. **Blocks.**
    pub(super) fn spawn(self: &Arc<Self>, spec: SpawnSpec) -> Result<HeldTerminal> {
        // A connection that dropped is dealt with first, so that the terminal is routed on the
        // connection the request goes out on, and not cleared with the one that is gone.
        self.settled();
        // Held until the terminal is routed, so that a restart claimed meanwhile sees it.
        let admitted = self.admit()?;
        let id = spec.id.clone();
        let binds_companion = spec.companion;
        if binds_companion {
            self.runs_companions()?;
        } else {
            // The holder ends whatever companion was left under the id.
            self.forget_companion(&id);
        }
        let (output, events) = mpsc::channel(OUTPUT_CHUNKS);
        let routed = Arc::new(Routed {
            exited: AtomicBool::new(false),
            output: Mutex::new(Some(output)),
            resumed: Mutex::new(None),
            link_changed: Mutex::new(None),
        });
        // Routed before the request goes out: the program's first output can arrive before the
        // holder's reply does.
        locked(&self.terminals).insert(id.clone(), routed.clone());
        drop(admitted);
        let (reply, replied) = std_mpsc::channel();
        locked(&self.spawns).insert(id.clone(), reply);
        let forgotten = |link: &Self| {
            locked(&link.terminals).remove(&id);
            locked(&link.spawns).remove(&id);
        };
        if !self.send_now(&ToHolder::Spawn(Box::new(spec))) {
            forgotten(self);
            if binds_companion {
                self.forget_companion(&id);
            }
            bail!("the link to the holder is lost");
        }
        // Waits for as long as the holder takes: a start it has not confirmed may be running, so
        // the caller keeps what it claimed for the launch and never starts the program again on a
        // guess. Only the holder's own answer ends the wait, or the end of the connection. A
        // program the holder started but could not confirm over it is ended by the holder, which
        // finds nobody to tell, or by this link once it is opened again and the welcome lists it.
        match replied.recv() {
            // The companion was marked bound as the frame was routed ([`Self::route_text`]).
            Ok(FromHolder::Spawned { pid, .. }) => {
                Ok(HeldTerminal::new(self.clone(), id, pid, routed, events))
            }
            // A refused spawn that expected a companion found none running, or the holder ended
            // it; one whose answer is lost had it ended with the connection.
            Ok(FromHolder::SpawnFailed { error, .. }) => {
                forgotten(self);
                if binds_companion {
                    self.forget_companion(&id);
                }
                bail!("{error}")
            }
            _ => {
                forgotten(self);
                if binds_companion {
                    self.forget_companion(&id);
                }
                bail!(
                    "the link to the holder was lost before it confirmed the start of the program"
                )
            }
        }
    }

    /// Forgets a terminal the daemon is done with, and tells the holder to. Never waits, as it may
    /// be called from a drop anywhere: a release lost to a connection that was down is made up
    /// for by the welcome of the link that opens again, which lists a terminal nobody owns.
    pub(super) fn release(&self, id: &str) {
        locked(&self.ended).insert(id.to_string());
        let routed = locked(&self.terminals).remove(id);
        if let Some(routed) = routed {
            routed.close_output();
        }
        // Its companion ends with it.
        self.forget_companion(id);
        // A link given up leaves the terminal to the holder, for the link that takes it next.
        if !self.abandoned.load(Ordering::Acquire) {
            self.send_now(&ToHolder::Release { id: id.to_string() });
        }
    }

    /// Hands the terminals this link follows over to the daemon that comes next, under the
    /// handover `token` (see [`ToHolder::Handover`]): the output of each ends at its mark, which
    /// closes the stream its reader takes from once what came before is read, and from then on
    /// the terminal is left alone ([`Self::is_handed_over`]). The ids of the terminals handed
    /// over: all the holder marked once it answers, and when it does not within
    /// [`HANDOVER_DEADLINE`] or the link drops, those whose marks were routed by then, which the
    /// holder has marked and so are the next daemon's all the same. `None` when none was marked,
    /// or the link is not alive or its holder does not carry out a handover. **Blocks.**
    pub(super) fn hand_over(&self, token: &str) -> Option<Vec<String>> {
        self.settled();
        if !self.alive() || !self.welcome.handover {
            return None;
        }
        // Whatever came of the request, the marks that came before the answer, on the same
        // connection, are routed, and one that comes after it is recorded all the same.
        let _answered = self.request(Some(HANDOVER_DEADLINE), |req| ToHolder::Handover {
            req,
            token: token.to_string(),
        });
        let marked: Vec<String> = locked(&self.handed_over).iter().cloned().collect();
        (!marked.is_empty()).then_some(marked)
    }

    /// Whether the terminal `id` was handed over to the daemon that comes next.
    pub(super) fn is_handed_over(&self, id: &str) -> bool {
        locked(&self.handed_over).contains(id)
    }

    /// Tells the holder the daemon is going away on purpose, and closes the link.
    pub(super) fn part(&self, handover: bool) {
        self.handover.store(handover, Ordering::Release);
        self.parting.store(true, Ordering::Release);
        // A connection being opened again is waited for: the holder is told over it, and the
        // daemon does not leave it believing the last link dropped.
        self.settled();
        self.send_now(&ToHolder::Parting { handover });
        locked(&self.out).send(Message::Close(None)).ok();
        // Until the holder has closed its side, so that the frames above are on the wire before a
        // process that exits next drops the connection.
        let deadline = std::time::Instant::now() + PART_DEADLINE;
        while self.alive() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    async fn route_text(self: &Arc<Self>, text: &str) {
        let Ok(message) = serde_json::from_str::<FromHolder>(text) else {
            tracing::debug!("ignoring a frame from the holder that is not a message");
            return;
        };
        match &message {
            FromHolder::Spawned { id, .. } | FromHolder::SpawnFailed { id, .. } => {
                // A companion the terminal bound outlives the connection it was started over. Marked
                // before the frame is forwarded: a drop between the two must not read it as exited.
                if matches!(message, FromHolder::Spawned { .. }) {
                    if let Some(route) = locked(&self.companions).get(id) {
                        route.bound.store(true, Ordering::Release);
                    }
                }
                let waiting = locked(&self.spawns).remove(id);
                match waiting {
                    Some(waiting) => {
                        waiting.send(message).ok();
                    }
                    // A start nobody waits for any more that the holder confirms has a program
                    // nobody owns.
                    None => {
                        if matches!(message, FromHolder::Spawned { .. })
                            && !locked(&self.terminals).contains_key(id)
                        {
                            self.disown(id);
                        }
                    }
                }
            }
            // A terminal handed over is the next daemon's to see exit.
            FromHolder::Exited { id, .. } if self.is_handed_over(id) => {}
            FromHolder::Exited { id, output_open } => {
                let routed = locked(&self.terminals).get(id).cloned();
                match routed {
                    Some(routed) => {
                        // Output a program's leftovers still print goes on reaching the reader,
                        // as it does from a PTY the daemon owns, until the terminal is hung up
                        // or released.
                        if !output_open {
                            routed.close_output();
                        }
                        routed.exited.store(true, Ordering::Release);
                    }
                    None => self.disown(id),
                }
            }
            // Taken over by a link of this very daemon (one it gave up, whose connection got
            // through late) is no loss to another daemon: the end of the connection is dealt with
            // as a drop, and the link opened again.
            FromHolder::Superseded { by } => {
                if by.as_deref() != Some(&*self.run_id) {
                    self.superseded.store(true, Ordering::Release);
                }
            }
            FromHolder::Written { req, .. }
            | FromHolder::Done { req, .. }
            | FromHolder::Answer { req, .. }
            | FromHolder::EndpointOpened { req, .. }
            | FromHolder::CompanionStarted { req, .. } => {
                if let Some(waiting) = locked(&self.replies).remove(req) {
                    waiting.send(message).ok();
                }
            }
            // A companion handed over with its terminal is the next daemon's to see end.
            FromHolder::CompanionEnded { id } if self.is_handed_over(id) => {}
            FromHolder::CompanionEnded { id } => {
                let route = locked(&self.companions).get(id).cloned();
                if let Some(route) = route {
                    route.exited.store(true, Ordering::Release);
                }
            }
            FromHolder::LaunchIssued { req, .. } => {
                if let Some(waiting) = locked(&self.replies).remove(req) {
                    waiting.send(message).ok();
                }
            }
            FromHolder::Relay {
                req,
                route,
                param,
                body,
                waits,
            } => self.relay_or_hold(HeldRelay {
                req: *req,
                route: *route,
                param: param.clone(),
                body: body.clone(),
                waits: *waits,
            }),
            FromHolder::RelayGone { req } => {
                // One still held back is dropped before it reaches the daemon.
                let held_back = match locked(&self.held_relays).as_mut() {
                    Some(queue) => {
                        queue.retain(|relayed| relayed.req != *req);
                        true
                    }
                    None => false,
                };
                if !held_back {
                    self.relaying.cancel(*req);
                }
            }
            FromHolder::HandoverMark { id } => {
                // Recorded before the output is closed, so that nothing the end of the reader's
                // stream sets off reaches the holder.
                locked(&self.handed_over).insert(id.clone());
                let routed = locked(&self.terminals).get(id).cloned();
                if let Some(routed) = routed {
                    routed.close_output();
                }
            }
            FromHolder::Welcome { .. } | FromHolder::Unknown => {}
        }
    }

    /// Holds `relayed` back while the link holds its relays ([`Self::release_relays`]), and hands
    /// it to the daemon otherwise.
    fn relay_or_hold(self: &Arc<Self>, relayed: HeldRelay) {
        {
            let mut held = locked(&self.held_relays);
            if let Some(queue) = held.as_mut() {
                queue.push(relayed);
                return;
            }
        }
        self.relay_request(
            relayed.req,
            relayed.route,
            relayed.param,
            relayed.body,
            relayed.waits,
        );
    }

    /// Hands a helper's request to the daemon's handler. A report is handled here, in the order the
    /// holder sent it. A question a helper waits on has its reply sent when the handler is done,
    /// unless the call was stopped first.
    fn relay_request(
        self: &Arc<Self>,
        req: u64,
        route: RelayRoute,
        param: String,
        body: Vec<u8>,
        waits: bool,
    ) {
        let request = RelayRequest { route, param, body };
        if !waits {
            self.relay.report(request);
            return;
        }
        let call = self.relay.relay(request);
        self.relaying.begin(req, &call);
        let link = self.clone();
        self.runtime.spawn(async move {
            let reply = call.await;
            link.relaying.end(req);
            // A call that was stopped has no reply to give; a handler that panicked leaves the
            // helper to be told its call failed.
            let (status, body) = match reply {
                Ok(reply) => (reply.status, reply.body),
                Err(err) if err.is_cancelled() => return,
                Err(_) => (500, String::new()),
            };
            link.send_now(&ToHolder::RelayReply { req, status, body });
        });
    }

    async fn route_bulk(&self, frame: &[u8]) {
        let Some((kind, id, payload)) = decode_bulk(frame) else {
            return;
        };
        match kind {
            BulkKind::Output => {
                let routed = locked(&self.terminals).get(id).cloned();
                match routed {
                    Some(routed) => routed.deliver(payload.to_vec()).await,
                    None => self.disown(id),
                }
            }
            BulkKind::Tail => {
                self.keep_tail(id, payload);
            }
        }
    }

    /// Keeps the tail of the terminal `id` for [`Self::take_tail`], if the welcome promised it.
    /// Under the lock of the tails throughout, so that the waiter sees the tail or the promise.
    fn keep_tail(&self, id: &str, payload: &[u8]) {
        let mut tails = locked(&self.tails);
        // Only the tails the welcome promised are kept, so that none piles up unread.
        if !locked(&self.expected_tails).remove(id) {
            return;
        }
        // A tail of a link opened again covers whatever of the terminal's output the connection
        // before it routed. The holder sends a terminal's tail before any of its output on every
        // connection, so nothing is queued here today; this keeps each byte shown once should
        // that order change. What follows the tail on this connection is routed after this, by
        // the same task.
        if locked(&self.reconnected_at).is_some() {
            if let Some(queued) = locked(&self.found).get_mut(id) {
                while queued.try_recv().is_ok() {}
            }
        }
        tails.insert(id.to_string(), payload.to_vec());
        drop(tails);
        self.tail_arrived.notify_all();
    }

    /// The connection is gone: no request can be answered any more, so every one waiting on a
    /// reply is answered with none.
    fn suspend(&self) {
        self.alive.store(false, Ordering::Release);
        // Under the lock the tail's waiter sleeps on, so the wake-up is not missed.
        drop(locked(&self.tails));
        self.tail_arrived.notify_all();
        locked(&self.replies).clear();
        // The routing of a start the holder has not confirmed goes with its waiter, so that the
        // welcome of the link that opens again lists the program as one nobody owns, whichever of
        // this and the waiter's own clean-up runs first.
        let unconfirmed: Vec<String> = locked(&self.spawns).drain().map(|(id, _)| id).collect();
        if !unconfirmed.is_empty() {
            let mut terminals = locked(&self.terminals);
            for id in &unconfirmed {
                terminals.remove(id);
            }
        }
        // The holder ends a companion no terminal has bound with the connection it was asked over.
        for route in locked(&self.companions).values() {
            if !route.bound.load(Ordering::Acquire) {
                route.exited.store(true, Ordering::Release);
            }
        }
        self.relaying.cancel_all();
    }

    /// The link is gone for good: every terminal of the holder has exited, but one handed over
    /// and, after a planned parting, every one, which are the next daemon's; each reader's stream
    /// is ended all the same.
    fn finish(&self) {
        let handed_over = locked(&self.handed_over).clone();
        // A planned parting leaves every terminal running in the holder for the next daemon, the
        // ones it marked none of included: none of them has exited.
        let planned = self.parting.load(Ordering::Acquire) && self.handover.load(Ordering::Acquire);
        for (id, routed) in locked(&self.terminals).iter() {
            if !planned && !handed_over.contains(id) {
                routed.exited.store(true, Ordering::Release);
            }
            routed.close_output();
        }
        // The holder is gone, or this daemon is: so is what it ran, but what is left to the next
        // daemon.
        for (id, route) in locked(&self.companions).iter() {
            if !planned && !handed_over.contains(id) {
                route.exited.store(true, Ordering::Release);
            }
        }
        // The writer and the socket of a connection that is still open are closed with it.
        locked(&self.out).send(Message::Close(None)).ok();
    }
}

/// The terminal `message` names, if it names one.
fn terminal_of(message: &ToHolder) -> Option<&str> {
    match message {
        ToHolder::Spawn(spec) => Some(spec.id.as_str()),
        ToHolder::Write { id, .. }
        | ToHolder::WriteInput { id, .. }
        | ToHolder::Resize { id, .. }
        | ToHolder::Query { id, .. }
        | ToHolder::Signal { id, .. }
        | ToHolder::HangUp { id }
        | ToHolder::PoliteStopLead { id, .. }
        | ToHolder::KillLeftovers { id, .. }
        | ToHolder::Reap { id, .. }
        | ToHolder::Release { id }
        | ToHolder::IssueLaunch { id, .. }
        | ToHolder::RevokeLaunch { id }
        | ToHolder::OpenEndpoint { id, .. }
        | ToHolder::StartCompanion { id, .. }
        | ToHolder::EndCompanion { id, .. } => Some(id.as_str()),
        ToHolder::Hello { .. }
        | ToHolder::Parting { .. }
        | ToHolder::Drain
        | ToHolder::RelayReply { .. }
        | ToHolder::Handover { .. }
        | ToHolder::Unknown => None,
    }
}

/// Has the end of a link's connection dealt with when dropped (see [`Link::serve`]).
struct EndsLink(Option<Arc<Link>>);

impl Drop for EndsLink {
    fn drop(&mut self) {
        if let Some(link) = self.0.take() {
            let runtime = link.runtime.clone();
            runtime.spawn(link.ended());
        }
    }
}

impl Routed {
    async fn deliver(&self, chunk: Vec<u8>) {
        let output = locked(&self.output).clone();
        if let Some(output) = output {
            output.send(chunk).await.ok();
        }
    }

    /// Ends the stream the reader takes from, once what is queued has been read.
    fn close_output(&self) {
        locked(&self.output).take();
    }
}
