//! The holder: a process that owns the PTYs of the programs the daemon starts and serves them to
//! the daemon over one WebSocket link (`GET /daemon`). It starts the program on a PTY, forwards
//! what it prints, keeps a raw tail of it, writes what it is sent, resizes, signals and reports
//! its exit once everything it printed has been sent. It keeps no screen: the only reading of the
//! output it does is to answer, while no daemon is linked, the few queries whose answer needs none
//! ([`offline`]). One holder serves many terminals, each with a thread of its own for its input,
//! so a program that stops reading holds up only itself.
//!
//! One daemon is linked at a time: a newer connection takes the holder over and the older one is
//! closed. The holder is found by its registry file ([`super::registry`]).
//!
//! A holder outlives its daemon: the programs go on running when the link drops, their output kept
//! in the tails for the daemon that links next. It exits once it has no terminal, no start
//! under way, no companion awaiting its terminal or being ended, and either no daemon is linked
//! or the linked one has said to drain. A link that drops without the daemon having parted is
//! recorded as a crash, for the next welcome to say.
//!
//! A daemon that stops on purpose for another to take over hands its terminals over
//! ([`ToHolder::Handover`]): each terminal its link follows is marked where its output stops going
//! to that daemon, and its tail records where the mark falls, so that the next daemon is handed
//! the history before it and exactly the output the stopping one never read.
//!
//! The same listener serves the helpers of the launches the holder runs ([`helpers`]): each launch
//! has a key the holder issued, and what a helper asks is relayed to the linked daemon
//! ([`relay`]).
//!
//! A terminal may have a companion ([`companion`]): a process the holder starts for it before the
//! terminal, which the terminal binds at its spawn. The holder enforces both directions of their
//! lifetime itself, whether or not a daemon is linked.

mod companion;
mod helpers;
mod launches;
mod offline;
mod relay;
mod terminal;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};

use self::companion::{Companion, OnExit};
use self::helpers::Helpers;
use self::launches::Launches;
use self::relay::Relays;
use self::terminal::{frame, Sink, Terminal, Told};
use super::protocol::{
    Answer, CompanionEndpoint, ContainmentSpec, EndpointKind, FromHolder, LastParting,
    OfflineAnswer, QueryKind, Ready, SpawnSpec, ToHolder, WireError,
};
use super::registry::{self, RegistryEntry};
use crate::platform::proc::ProcessIdentity;
use crate::platform::proc_table;
use crate::platform::term as host_term;
use crate::platform::term::Command;
use crate::request_guard::{headers_allowed, same_secret};

/// How long a connection has to say hello.
const HELLO_DEADLINE: Duration = Duration::from_secs(10);

/// Frames queued for the daemon before the readers of the terminals wait for it.
const OUTBOUND_FRAMES: usize = 512;

/// How long the last frame of a link, and its close, have to be taken by the daemon at the other
/// end before the link is dropped without them.
const CLOSE_DEADLINE: Duration = Duration::from_secs(2);

/// How long a companion no terminal has bound is kept after its endpoint was opened or it was
/// started: a daemon spawns the terminal right after the start, so one left this long belongs to
/// a launch that will not come.
const BIND_DEADLINE: Duration = Duration::from_secs(120);

/// How long [`Holder::end_all`] waits for the ends of companions already under way: a little more
/// than a companion's grace.
const END_ALL_WAIT: Duration = Duration::from_secs(5);

/// How often the bind deadline of a companion whose start is under way is looked at again.
const BIND_RECHECK: Duration = Duration::from_secs(1);

/// How a holder is run. Everything that names a place is passed in, so a test runs one against a
/// directory of its own.
pub struct HolderConfig {
    /// Where the holder leaves its registry file.
    pub registry_dir: PathBuf,
    pub holder_id: String,
    pub version: String,
    pub build_id: String,
    /// The executable the holder runs from, for the registry file.
    pub exe: PathBuf,
    /// How long the holder waits for the first daemon link before it gives up and exits.
    pub first_link_deadline: Duration,
    /// How long a request a helper waits on waits for a daemon to link; see
    /// [`super::RELAY_DEADLINE`].
    pub relay_deadline: Duration,
}

/// Why the holder closes a link itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Closing {
    /// Another daemon linked and took the holder over: the link is told so before it closes.
    Superseded,
    /// The link cannot be served as it is, for want of room to hand it its handshake.
    Unserved,
}

/// How a link ended, for the parting the next welcome reports.
enum Ending {
    /// The daemon said it was going, handing the terminals over or not.
    Parted(bool),
    /// The connection dropped without it: a crash of the daemon.
    Dropped,
    /// The holder closed the link itself, which says nothing about the daemon: the previous
    /// parting stands.
    Closed,
}

struct LinkSlot {
    generation: u64,
    /// Tells the link in place to close, and why.
    current: Option<watch::Sender<Option<Closing>>>,
    /// Where the link in place is sent its frames, once its welcome is queued.
    sink: Option<Sink>,
    /// The run id of the daemon that linked last, which stays when its link ends; `None` when that
    /// daemon sent none.
    last_run: Option<String>,
    /// The run ids of the daemons the holder was taken over from: they are refused from then on,
    /// so that one that was stalled meanwhile cannot take the holder back.
    superseded_runs: HashSet<String>,
}

/// A companion no terminal has bound yet, by the terminal id it was opened under.
struct Unbound {
    companion: Arc<Companion>,
    /// The link it was asked over, whose end ends it.
    generation: u64,
    /// Its start is under way.
    starting: bool,
    /// When it is ended if no terminal has bound it by then.
    deadline: Instant,
}

/// An end of a companion under way, which the holder counts until it is dropped and then settles.
struct EndingGuard(Arc<Holder>);

impl Drop for EndingGuard {
    fn drop(&mut self) {
        self.0.ending.fetch_sub(1, Ordering::AcqRel);
        self.0.settle();
    }
}

/// What [`ToHolder::StartCompanion`] asks.
struct CompanionStart {
    id: String,
    req: u64,
    command: Command,
    ready: Ready,
    within: Duration,
    containment: Option<ContainmentSpec>,
}

struct Holder {
    config: HolderConfig,
    instance_id: String,
    secret: String,
    terminals: Mutex<HashMap<String, Arc<Terminal>>>,
    /// Terminals being started, which count as running when deciding whether to exit.
    pending: Mutex<HashSet<String>>,
    /// The companions no terminal has bound yet, which count as something to serve as a pending
    /// spawn does, and are never listed in a welcome.
    companions: Mutex<HashMap<String, Unbound>>,
    /// The companions being ended on threads of their own, which count as something to serve
    /// until each end is done: the holder must not exit before the grace, the kill and the sweep
    /// of an end it started.
    ending: AtomicUsize,
    link: Mutex<LinkSlot>,
    helpers: Arc<Helpers>,
    last_parting: Mutex<LastParting>,
    draining: AtomicBool,
    ever_linked: AtomicBool,
    exit: watch::Sender<bool>,
}

/// Runs a holder until it has no more to do. Returns once its listener is closed and its registry
/// file removed.
#[cfg(test)]
pub async fn run(config: HolderConfig) -> Result<()> {
    run_until(config, std::future::pending()).await
}

/// [`run`], and also until `stop` resolves, when the holder ends every program it runs and exits
/// as it does when it has nothing left to serve: the host asking the process to end.
pub async fn run_until(
    config: HolderConfig,
    stop: impl std::future::Future<Output = ()>,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("binding the holder's listener")?;
    let port = listener.local_addr()?.port();
    let pid = std::process::id();
    let started = proc_table::start_of(pid as i32)
        .map(|start_micros| {
            ProcessIdentity {
                pid: crate::platform::proc::ProcessId(pid),
                start_micros,
            }
            .key()
        })
        .unwrap_or_default();
    let secret = crate::request_guard::generate_key().context("generating the holder's secret")?;
    let entry = RegistryEntry {
        holder_id: config.holder_id.clone(),
        pid,
        started,
        port,
        secret: secret.clone(),
        version: config.version.clone(),
        build_id: config.build_id.clone(),
        exe: config.exe.to_string_lossy().into_owned(),
    };
    let registry_dir = config.registry_dir.clone();
    let holder_id = config.holder_id.clone();
    let first_link_deadline = config.first_link_deadline;
    let relay_deadline = config.relay_deadline;
    let (exit, mut exiting) = watch::channel(false);
    let holder = Arc::new(Holder {
        config,
        instance_id: uuid::Uuid::new_v4().to_string(),
        secret,
        terminals: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashSet::new()),
        companions: Mutex::new(HashMap::new()),
        ending: AtomicUsize::new(0),
        link: Mutex::new(LinkSlot {
            generation: 0,
            current: None,
            sink: None,
            last_run: None,
            superseded_runs: HashSet::new(),
        }),
        helpers: Arc::new(Helpers {
            launches: Launches::default(),
            relays: Relays::new(relay_deadline),
        }),
        last_parting: Mutex::new(LastParting::None),
        draining: AtomicBool::new(false),
        ever_linked: AtomicBool::new(false),
        exit,
    });
    let registry_written = registry_dir.clone();
    tokio::task::spawn_blocking(move || registry::write(&registry_written, &entry))
        .await?
        .context("writing the holder's registry file")?;
    tracing::info!(holder = %holder_id, port, "holder listening");

    tokio::spawn({
        let holder = holder.clone();
        async move {
            tokio::time::sleep(first_link_deadline).await;
            if !holder.ever_linked.load(Ordering::Acquire) {
                tracing::info!("no daemon linked in time; the holder is exiting");
                holder.exit.send(true).ok();
            }
        }
    });

    let app = Router::new()
        .route("/daemon", get(daemon_link))
        .with_state(holder.clone())
        .merge(helpers::router(holder.helpers.clone()));
    let served = tokio::select! {
        served = axum::serve(listener, app) => Some(served),
        _ = exiting.wait_for(|exiting| *exiting) => None,
        _ = stop => {
            tracing::info!("the holder was asked to end; it ends its programs");
            None
        }
    };
    holder.end_all().await;
    registry::remove(&registry_dir, &holder_id);
    match served {
        Some(Err(err)) => Err(err).context("serving the daemon link"),
        _ => Ok(()),
    }
}

async fn daemon_link(
    State(holder): State<Arc<Holder>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !headers_allowed(&headers) || headers.contains_key(header::ORIGIN) {
        return (StatusCode::FORBIDDEN, "forbidden origin or host").into_response();
    }
    let supplied = params.get("secret").map_or("", String::as_str);
    if !same_secret(supplied, &holder.secret) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong secret").into_response();
    }
    ws.on_upgrade(move |socket| serve_link(holder, socket))
}

impl Holder {
    fn terminals(&self) -> MutexGuard<'_, HashMap<String, Arc<Terminal>>> {
        self.terminals.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn pending(&self) -> MutexGuard<'_, HashSet<String>> {
        self.pending.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn link(&self) -> MutexGuard<'_, LinkSlot> {
        self.link.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn companions(&self) -> MutexGuard<'_, HashMap<String, Unbound>> {
        self.companions
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    fn terminal(&self, id: &str) -> Option<Arc<Terminal>> {
        self.terminals().get(id).cloned()
    }

    /// Exits when nothing is left to serve: no terminal, no spawn under way, no companion awaiting
    /// its terminal or being ended, and either no daemon linked or the linked one has said to drain.
    fn settle(&self) {
        let nothing_left = self.terminals().is_empty()
            && self.pending().is_empty()
            && self.companions().is_empty()
            && self.ending.load(Ordering::Acquire) == 0;
        let linked = self.link().current.is_some();
        if nothing_left && (!linked || self.draining.load(Ordering::Acquire)) {
            self.exit.send(true).ok();
        }
    }

    /// Ends `companion` on a thread of its own, for a caller that must not wait. The end counts as
    /// something the holder serves until it is done, and the holder settles then. Counted before
    /// this returns, so that the caller's own settle sees it.
    fn end_in_background(self: &Arc<Self>, companion: Arc<Companion>) {
        self.ending.fetch_add(1, Ordering::AcqRel);
        let counted = EndingGuard(self.clone());
        std::thread::spawn(move || {
            companion.end();
            drop(counted);
        });
    }

    /// Closes the link `generation` from this side, if it is still the one in place. The daemon
    /// finds its connection dropped and links again, and that welcome lists every terminal.
    fn close_link(&self, generation: u64) {
        let slot = self.link();
        if slot.generation != generation {
            return;
        }
        if let Some(current) = &slot.current {
            current.send_replace(Some(Closing::Unserved));
        }
    }

    /// Ends every program and forgets every terminal, and ends every companion, the terminals in
    /// parallel (each waits out its own grace) and the ends already under way waited for.
    /// **Blocks** off the runtime.
    async fn end_all(self: &Arc<Self>) {
        let all: Vec<Arc<Terminal>> = self
            .terminals()
            .drain()
            .map(|(_, terminal)| terminal)
            .collect();
        let unbound: Vec<Arc<Companion>> = self
            .companions()
            .drain()
            .map(|(_, unbound)| unbound.companion)
            .collect();
        let holder = self.clone();
        let ended = tokio::task::spawn_blocking(move || {
            std::thread::scope(|scope| {
                for terminal in &all {
                    scope.spawn(move || terminal.end());
                }
                for companion in &unbound {
                    scope.spawn(move || companion.end());
                }
                // Ends started on threads of their own before the holder was asked to end.
                let deadline = Instant::now() + END_ALL_WAIT;
                while holder.ending.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(20));
                }
            });
        })
        .await;
        if ended.is_err() {
            tracing::warn!("ending the holder's programs panicked");
        }
    }

    /// The link `generation` is over. A link another took over from changes nothing. Otherwise the
    /// terminals keep their programs, and keep the output in their tails; the holder exits if that
    /// leaves it with nothing to serve ([`Self::settle`]).
    ///
    /// `ending` is how it ended: a link that dropped without the daemon having parted counts as a
    /// crash.
    async fn link_ended(self: &Arc<Self>, generation: u64, ending: Ending) {
        self.helpers.relays.detach(generation);
        // Whatever the ending: a launch straddling it fails, as a spawn whose answer is lost does.
        self.end_unbound_of(generation);
        {
            let mut slot = self.link();
            if slot.generation != generation {
                return;
            }
            slot.current = None;
            slot.sink = None;
        }
        let parting = match ending {
            Ending::Parted(true) => Some(LastParting::Handover),
            Ending::Parted(false) => Some(LastParting::None),
            Ending::Dropped => Some(LastParting::Crash),
            Ending::Closed => None,
        };
        if let Some(parting) = parting {
            *self
                .last_parting
                .lock()
                .unwrap_or_else(|err| err.into_inner()) = parting;
        }
        // Collected first: the map's lock is not held while a terminal is detached.
        let all: Vec<Arc<Terminal>> = self.terminals().values().cloned().collect();
        for terminal in all {
            terminal.detach();
        }
        self.settle();
    }

    async fn spawn(self: Arc<Self>, spec: SpawnSpec, reply: Sink, generation: u64) {
        let id = spec.id.clone();
        let draining = self.draining.load(Ordering::Acquire);
        let refused = if draining {
            Some("the holder is draining")
        } else if self.terminals().contains_key(&id) || !self.pending().insert(id.clone()) {
            Some("a terminal of that id was spawned already")
        } else {
            None
        };
        if let Some(why) = refused {
            // The key issued for the launch, and the companion started for it, are of no use: its
            // program will not run here. Those of a terminal that was spawned already are that
            // terminal's, and stay.
            if draining {
                self.helpers.launches.revoke(&id);
                let unbound = self.companions().remove(&id);
                if let Some(unbound) = unbound {
                    self.end_in_background(unbound.companion);
                }
            }
            reply
                .send(frame(&FromHolder::SpawnFailed {
                    id,
                    error: why.to_string(),
                }))
                .await
                .ok();
            self.settle();
            return;
        }
        // The companion the terminal binds, if it expects one: a terminal is never attached to a
        // dead server. One that expects none starts unbound, whatever was left under its id. A
        // companion opened over another link than the one asking is not this launch's: the launch
        // straddles a link drop, and fails as it does on the daemon's side. What belongs to
        // the other link is left to it.
        let taken = {
            let mut companions = self.companions();
            if spec.companion
                && companions
                    .get(&id)
                    .is_some_and(|unbound| unbound.generation != generation)
            {
                None
            } else {
                Some(companions.remove(&id))
            }
        };
        let Some(unbound) = taken else {
            self.pending().remove(&id);
            self.helpers.launches.revoke(&id);
            reply
                .send(frame(&FromHolder::SpawnFailed {
                    id,
                    error: "the terminal's companion was opened over another link".to_string(),
                }))
                .await
                .ok();
            self.settle();
            return;
        };
        let companion = match (spec.companion, unbound) {
            (true, Some(unbound)) if !unbound.starting && unbound.companion.running() => {
                Some(unbound.companion)
            }
            (true, unbound) => {
                if let Some(unbound) = unbound {
                    self.end_in_background(unbound.companion);
                }
                self.pending().remove(&id);
                self.helpers.launches.revoke(&id);
                reply
                    .send(frame(&FromHolder::SpawnFailed {
                        id,
                        error: "the terminal's companion is missing or has exited".to_string(),
                    }))
                    .await
                    .ok();
                self.settle();
                return;
            }
            (false, unbound) => {
                if let Some(unbound) = unbound {
                    self.end_in_background(unbound.companion);
                }
                None
            }
        };
        // Its output goes to the link that is current as it starts, which is the request's own
        // unless another has taken the holder over since.
        let sink = self.link().sink.clone();
        let runtime = tokio::runtime::Handle::current();
        let binding = companion.clone();
        let started =
            tokio::task::spawn_blocking(move || Terminal::start(spec, sink, runtime, binding))
                .await;
        let (reply_frame, terminal) = match started {
            Ok(Ok(terminal)) => {
                let spawned = FromHolder::Spawned {
                    id: id.clone(),
                    pid: terminal.pid().0,
                    identity: terminal.identity_key(),
                };
                self.terminals().insert(id.clone(), terminal.clone());
                self.helpers.launches.started(&id);
                (spawned, Some(terminal))
            }
            Ok(Err(err)) => {
                self.helpers.launches.revoke(&id);
                let failed = FromHolder::SpawnFailed {
                    id: id.clone(),
                    error: format!("{err:#}"),
                };
                (failed, None)
            }
            Err(_) => {
                self.helpers.launches.revoke(&id);
                let failed = FromHolder::SpawnFailed {
                    id: id.clone(),
                    error: "starting the terminal panicked".to_string(),
                };
                (failed, None)
            }
        };
        if terminal.is_none() {
            // A terminal that did not start bound nothing: its companion is ended here.
            if let Some(companion) = companion {
                self.end_in_background(companion);
            }
        }
        self.pending().remove(&id);
        let delivered = reply.send(frame(&reply_frame)).await.is_ok();
        match (delivered, terminal) {
            (true, Some(terminal)) => self.follow_current(&terminal, &reply),
            (false, Some(terminal)) => {
                // The link the start was asked over is gone, so the daemon was never told of the
                // program and nobody owns it.
                tracing::warn!(terminal = %id, "a started terminal could not be confirmed to the daemon that asked; it is ended");
                self.terminals().remove(&id);
                self.helpers.launches.revoke(&id);
                let ended = tokio::task::spawn_blocking(move || terminal.end()).await;
                if ended.is_err() {
                    tracing::warn!(terminal = %id, "ending the terminal panicked");
                }
            }
            (_, None) => {}
        }
        self.settle();
    }

    /// Whether a daemon of run id `run_id` may link, noting the daemon it takes the holder over
    /// from as superseded. One without a run id (an older build) always may; the run that linked
    /// last is then not known, which the frame telling a link it was taken over says.
    fn admit(&self, run_id: Option<String>) -> bool {
        let mut slot = self.link();
        let Some(run_id) = run_id else {
            if let Some(last) = slot.last_run.take() {
                slot.superseded_runs.insert(last);
            }
            return true;
        };
        if slot.superseded_runs.contains(&run_id) {
            return false;
        }
        if let Some(last) = slot.last_run.replace(run_id.clone()) {
            if last != run_id {
                slot.superseded_runs.insert(last);
            }
        }
        true
    }

    /// Sends the output of the kept terminal `terminal` to the link in place. A link that took the
    /// holder over while the program started did not find the terminal in its handshake. If it
    /// cannot be handed the terminal and is the link the start was asked over, which now owns a
    /// terminal it cannot read, it is closed and its daemon's next link finds the terminal in its
    /// welcome, rather than output being lost without a word. Another link is left as it is: it
    /// did not ask for the terminal, and the next welcome lists it.
    fn follow_current(&self, terminal: &Terminal, asked_over: &Sink) {
        let current = {
            let slot = self.link();
            slot.sink.clone().map(|sink| (sink, slot.generation))
        };
        let Some((current, generation)) = current else {
            return;
        };
        if terminal.follow(&current) {
            return;
        }
        if current.same_channel(asked_over) {
            tracing::warn!("a daemon link could not be handed a new terminal; it is closed");
            self.close_link(generation);
        } else {
            tracing::warn!(
                "a daemon link that did not ask for a new terminal could not be handed it"
            );
        }
    }

    fn welcome(&self) -> FromHolder {
        FromHolder::Welcome {
            holder_id: self.config.holder_id.clone(),
            instance_id: self.instance_id.clone(),
            version: self.config.version.clone(),
            build_id: self.config.build_id.clone(),
            last_parting: *self
                .last_parting
                .lock()
                .unwrap_or_else(|err| err.into_inner()),
            terminals: self
                .terminals()
                .values()
                .map(|terminal| terminal.info())
                .collect(),
            facts: host_term::facts(),
            answers_offline: if terminal::answers_offline() {
                vec![OfflineAnswer::PrimaryDeviceAttributes]
            } else {
                Vec::new()
            },
            handover: true,
            companions: true,
        }
    }

    /// Ends the companions no terminal has bound that were asked for over the link `generation`.
    fn end_unbound_of(self: &Arc<Self>, generation: u64) {
        let mut ended = Vec::new();
        self.companions().retain(|_, unbound| {
            if unbound.generation == generation {
                ended.push(unbound.companion.clone());
                false
            } else {
                true
            }
        });
        if ended.is_empty() {
            return;
        }
        for companion in ended {
            self.end_in_background(companion);
        }
        self.settle();
    }

    /// Ends the companion of `id` if no terminal has bound it. **Blocks** for up to its grace.
    fn end_unbound(&self, id: &str) {
        let removed = self.companions().remove(id);
        if let Some(removed) = removed {
            removed.companion.end();
            self.settle();
        }
    }

    /// Opens the endpoint of the companion of the terminal `id`, asked for over the link
    /// `generation`, ending any companion left unbound under that id.
    fn open_endpoint(
        self: &Arc<Self>,
        id: &str,
        kind: &EndpointKind,
        generation: u64,
    ) -> Result<CompanionEndpoint, String> {
        if self.draining.load(Ordering::Acquire) {
            return Err("the holder is draining".to_string());
        }
        if self.terminals().contains_key(id) || self.pending().contains(id) {
            return Err("a terminal of that id was spawned already".to_string());
        }
        let companion = Companion::open(id, kind)?;
        let endpoint = companion.endpoint();
        let serial = companion.serial;
        let replaced = self.companions().insert(
            id.to_string(),
            Unbound {
                companion,
                generation,
                starting: false,
                deadline: Instant::now() + BIND_DEADLINE,
            },
        );
        if let Some(replaced) = replaced {
            self.end_in_background(replaced.companion);
        }
        self.watch_bind_deadline(id.to_string(), serial);
        Ok(endpoint)
    }

    /// Ends the companion `serial` of `id` once its bind deadline passes, unless a terminal has
    /// bound it or it was ended by then.
    fn watch_bind_deadline(self: &Arc<Self>, id: String, serial: u64) {
        let holder = self.clone();
        tokio::spawn(async move {
            loop {
                let mut wait = BIND_RECHECK;
                let expired = {
                    let mut companions = holder.companions();
                    let (starting, deadline) = match companions.get(&id) {
                        Some(unbound) if unbound.companion.serial == serial => {
                            (unbound.starting, unbound.deadline)
                        }
                        _ => return,
                    };
                    let now = Instant::now();
                    if starting {
                        None
                    } else if deadline > now {
                        wait = deadline - now;
                        None
                    } else {
                        companions.remove(&id)
                    }
                };
                if let Some(expired) = expired {
                    tracing::warn!(terminal = %id, "no terminal bound a companion in time; it is ended");
                    holder.end_in_background(expired.companion);
                    holder.settle();
                    return;
                }
                tokio::time::sleep(wait).await;
            }
        });
    }

    /// Starts the companion `start` asks for and answers on `reply`.
    async fn start_companion(self: Arc<Self>, start: CompanionStart, reply: Sink) {
        let (id, req) = (start.id.clone(), start.req);
        let answer = match self.run_companion(start).await {
            Ok((pid, identity, port)) => FromHolder::CompanionStarted {
                req,
                pid: Some(pid),
                identity,
                port,
                error: None,
            },
            Err(error) => {
                tracing::warn!(terminal = %id, %error, "a companion did not start");
                FromHolder::CompanionStarted {
                    req,
                    pid: None,
                    identity: None,
                    port: None,
                    error: Some(error),
                }
            }
        };
        reply.send(frame(&answer)).await.ok();
    }

    /// Starts the companion opened under `start.id` and waits for it to be ready, off the
    /// runtime. A companion that does not come up is ended with its endpoint, and one ended while
    /// it started (its link ended, a revoke, an end) fails the start. That end finds no process yet,
    /// so the start ends it itself once its readiness wait returns: the holder's exit relies on the
    /// runtime's drop waiting for blocking tasks, which a shutdown with a timeout would break.
    async fn run_companion(
        self: &Arc<Self>,
        start: CompanionStart,
    ) -> Result<(u32, Option<String>, Option<u16>), String> {
        let CompanionStart {
            id,
            command,
            ready,
            within,
            containment,
            ..
        } = start;
        if self.draining.load(Ordering::Acquire) {
            // As a spawn is refused while draining, the companion opened for it is of no use.
            let unbound = self.companions().remove(&id);
            if let Some(unbound) = unbound {
                self.end_in_background(unbound.companion);
            }
            return Err("the holder is draining".to_string());
        }
        let companion = {
            let mut companions = self.companions();
            let Some(unbound) = companions.get_mut(&id) else {
                return Err("no endpoint is open for the terminal's companion".to_string());
            };
            if unbound.starting || unbound.companion.started() {
                return Err("the terminal's companion was started already".to_string());
            }
            unbound.starting = true;
            unbound.companion.clone()
        };
        let on_exit = self.on_companion_exit(companion.clone());
        let starting = companion.clone();
        let started = tokio::task::spawn_blocking(move || {
            starting.start(&command, &ready, within, containment.as_ref(), on_exit)
        })
        .await
        .unwrap_or_else(|_| Err("starting the companion panicked".to_string()));
        let (ours, failed) = {
            let mut companions = self.companions();
            let ours = companions
                .get(&id)
                .is_some_and(|unbound| unbound.companion.serial == companion.serial);
            if !ours {
                (false, None)
            } else if started.is_ok() {
                if let Some(unbound) = companions.get_mut(&id) {
                    unbound.starting = false;
                    unbound.deadline = Instant::now() + BIND_DEADLINE;
                }
                (true, None)
            } else {
                (true, companions.remove(&id))
            }
        };
        if let Some(failed) = failed {
            self.end_in_background(failed.companion);
            self.settle();
        }
        if !ours && started.is_ok() {
            // Ended as it came up; the end took the process.
            return Err("the companion was ended as it started".to_string());
        }
        started
    }

    /// What is done once the companion `companion` exits by itself: a linked daemon is told, what
    /// it left is swept and its endpoint removed, and the terminal that bound it is ended. An
    /// unbound one stays in the map as exited until its spawn, end or deadline, so that a spawn
    /// that expects it is refused.
    fn on_companion_exit(self: &Arc<Self>, companion: Arc<Companion>) -> OnExit {
        let holder = self.clone();
        let runtime = tokio::runtime::Handle::current();
        Box::new(move |terminal: Option<Arc<Terminal>>| {
            let id = companion.id.clone();
            tracing::warn!(terminal = %id, "a companion exited by itself");
            // Sent off this thread: a full link must not hold up the ending below.
            let sink = holder.link().sink.clone();
            if let Some(sink) = sink {
                runtime.spawn(async move {
                    sink.send(frame(&FromHolder::CompanionEnded { id }))
                        .await
                        .ok();
                });
            }
            companion.end();
            if let Some(terminal) = terminal {
                terminal.end_for_companion();
            }
        })
    }
}

/// What `welcome` says of each terminal's tail: the token of the handover that marked it, and
/// where the output since the mark begins when the tail holds all of it.
fn told_tails(welcome: &FromHolder) -> HashMap<String, Told<'_>> {
    let FromHolder::Welcome { terminals, .. } = welcome else {
        return HashMap::new();
    };
    terminals
        .iter()
        .filter_map(|info| {
            let tail = info.handover.as_ref()?;
            let told = Told {
                token: Some(tail.token.as_str()),
                since: (!tail.truncated).then_some(tail.since),
            };
            Some((info.id.clone(), told))
        })
        .collect()
}

/// Serves one daemon link: the handshake, then the daemon's requests until the link closes or is
/// taken over.
async fn serve_link(holder: Arc<Holder>, socket: WebSocket) {
    let (mut wire_out, mut wire_in) = socket.split();
    let hello = tokio::time::timeout(HELLO_DEADLINE, async {
        while let Some(Ok(message)) = wire_in.next().await {
            if let Message::Text(text) = message {
                return serde_json::from_str::<ToHolder>(&text).ok();
            }
        }
        None
    })
    .await;
    let Ok(Some(ToHolder::Hello {
        run_id,
        handed_over,
        ..
    })) = hello
    else {
        return;
    };
    let handed_over: HashSet<String> = handed_over.into_iter().collect();
    if !holder.admit(run_id) {
        tracing::warn!(
            "a daemon the holder was taken over from tried to link again; it is refused"
        );
        let by = holder.link().last_run.clone();
        tell_superseded(&mut wire_out, by).await;
        tokio::time::timeout(CLOSE_DEADLINE, wire_out.close())
            .await
            .ok();
        return;
    }

    let (closing, mut closing_rx) = watch::channel(None::<Closing>);
    let (out, outbox) = mpsc::channel::<Message>(OUTBOUND_FRAMES);
    // The writer runs before anything is queued, so that the handshake frames below are taken off
    // the channel as they come, and what they add up to cannot be more than it holds. A link that
    // is taken over is told so by the writer, which then ends it.
    let writer = tokio::spawn({
        let mut closing_rx = closing_rx.clone();
        let holder = holder.clone();
        async move {
            let mut outbox = outbox;
            loop {
                // The closing signal is watched while a frame is waited for and while it is sent,
                // as the wire may have stopped taking frames. It is looked at first: a link taken
                // over loses its senders as the newer link takes the terminals, and a writer that
                // found its channel closed first would end it without saying why.
                let outcome = tokio::select! {
                    biased;
                    changed = closing_rx.changed() => {
                        Err(changed.is_ok() && *closing_rx.borrow() == Some(Closing::Superseded))
                    }
                    failed = async {
                        match outbox.recv().await {
                            Some(message) => wire_out.send(message).await.is_err(),
                            None => true,
                        }
                    } => Ok(failed),
                };
                match outcome {
                    Err(superseded) => {
                        if superseded {
                            // The run of the daemon that took over, which linked last.
                            let by = holder.link().last_run.clone();
                            tell_superseded(&mut wire_out, by).await;
                        }
                        break;
                    }
                    Ok(true) => break,
                    Ok(false) => {}
                }
            }
            // Dropped before the close, which may wait on a wire that has stopped taking frames:
            // whoever is waiting on room in the channel is let go.
            drop(outbox);
            tokio::time::timeout(CLOSE_DEADLINE, wire_out.close())
                .await
                .ok();
        }
    });

    // The welcome goes first: the terminals send their output to this link only once they are
    // attached to it, and the link's own sink is not known to anything else before that.
    let generation = {
        let mut slot = holder.link();
        if let Some(older) = slot.current.replace(closing) {
            older.send_replace(Some(Closing::Superseded));
        }
        slot.generation += 1;
        slot.sink = None;
        slot.generation
    };
    holder.ever_linked.store(true, Ordering::Release);
    let welcome = holder.welcome();
    let told = told_tails(&welcome);
    let welcomed = out.try_send(frame(&welcome)).is_ok();
    holder.link().sink = Some(out.clone());
    let terminals: Vec<Arc<Terminal>> = holder.terminals().values().cloned().collect();
    // A terminal whose tail cannot be queued would leave the daemon waiting for it, and one whose
    // tail is no longer what the welcome said (its mark was moved or has fallen off the front)
    // would mislead it, so the link is not kept. A terminal the daemon handed over before its
    // link dropped is not attached: it stays with no daemon linked for its output, so the holder
    // goes on answering the queries in it it may and keeps the output since the mark in the tail
    // for the next daemon, while this link only takes the daemon's and its clients' answers to
    // the output before the mark, and its parting.
    let attached = welcomed
        && terminals.iter().all(|terminal| {
            if handed_over.contains(&terminal.id) {
                terminal.unpin();
                return true;
            }
            let told = told.get(&terminal.id).copied().unwrap_or_default();
            terminal.attach(&out, Some(told))
        });
    if !attached {
        tracing::warn!("a daemon link could not be handed its handshake; it is closed");
        // The tails the welcome pinned that no attach let go of.
        for terminal in &terminals {
            terminal.unpin();
        }
        drop(out);
        writer.abort();
        holder.link_ended(generation, Ending::Closed).await;
        return;
    }
    // After the welcome, so the daemon knows the holder before the first helper request reaches it.
    holder.helpers.relays.attach(generation, out.clone());

    let mut parted = None;
    loop {
        tokio::select! {
            _ = closing_rx.changed() => break,
            incoming = wire_in.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<ToHolder>(&text) {
                        Ok(message) => handle(&holder, &out, &mut parted, generation, message).await,
                        Err(err) => tracing::debug!(%err, "ignoring a frame that is not a message"),
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    drop(out);
    let closed = *closing_rx.borrow();
    // The writer of a link that was taken over tells it so and ends by itself.
    if closed != Some(Closing::Superseded) {
        writer.abort();
    }
    let ending = match (parted, closed) {
        (Some(handover), _) => Ending::Parted(handover),
        (None, Some(Closing::Unserved)) => Ending::Closed,
        (None, _) => Ending::Dropped,
    };
    holder.link_ended(generation, ending).await;
}

/// The last frame of a link another daemon took over from; given up on if the daemon does not
/// take it in [`CLOSE_DEADLINE`].
async fn tell_superseded(
    wire_out: &mut (impl futures_util::Sink<Message> + Unpin),
    by: Option<String>,
) {
    tokio::time::timeout(
        CLOSE_DEADLINE,
        wire_out.send(frame(&FromHolder::Superseded { by })),
    )
    .await
    .ok();
}

async fn handle(
    holder: &Arc<Holder>,
    out: &Sink,
    parted: &mut Option<bool>,
    generation: u64,
    message: ToHolder,
) {
    match message {
        ToHolder::Hello { .. } | ToHolder::Unknown => {}
        ToHolder::Spawn(spec) => {
            tokio::spawn(holder.clone().spawn(*spec, out.clone(), generation));
        }
        ToHolder::Write { id, req, bytes } => match holder.terminal(&id) {
            Some(terminal) => terminal.write_raw(req, bytes, out.clone()),
            None => refuse_write(out, req).await,
        },
        ToHolder::WriteInput { id, req, inputs } => match holder.terminal(&id) {
            Some(terminal) => terminal.write_input(req, inputs, out.clone()),
            None => refuse_write(out, req).await,
        },
        ToHolder::Resize { id, req, size } => {
            let Some(terminal) = holder.terminal(&id) else {
                let error = Some("no such terminal".to_string());
                out.send(frame(&FromHolder::Done { req, error })).await.ok();
                return;
            };
            // A resize can wait on the host, so it is not made on the link's reader.
            let out = out.clone();
            tokio::task::spawn_blocking(move || {
                let error = terminal
                    .pty()
                    .resize(size)
                    .err()
                    .map(|err| format!("{err:#}"));
                out.blocking_send(frame(&FromHolder::Done { req, error }))
                    .ok();
            });
        }
        ToHolder::Query { id, req, what } => {
            let answer = match holder.terminal(&id) {
                Some(terminal) => answer(&terminal, what),
                None => Answer::UnknownTerminal,
            };
            out.send(frame(&FromHolder::Answer { req, answer }))
                .await
                .ok();
        }
        ToHolder::Signal { id, signal } => {
            if let Some(terminal) = holder.terminal(&id) {
                terminal.signal(signal);
            }
        }
        ToHolder::HangUp { id } => {
            if let Some(terminal) = holder.terminal(&id) {
                terminal.hang_up();
            }
        }
        ToHolder::PoliteStopLead { id, req, polite } => {
            let answer = match holder.terminal(&id) {
                Some(terminal) => Answer::Lead {
                    millis: terminal.pty().polite_stop_lead(polite).as_millis() as u64,
                },
                None => Answer::UnknownTerminal,
            };
            out.send(frame(&FromHolder::Answer { req, answer }))
                .await
                .ok();
        }
        ToHolder::KillLeftovers {
            id,
            req,
            identities,
        } => {
            let Some(terminal) = holder.terminal(&id) else {
                let answer = Answer::UnknownTerminal;
                out.send(frame(&FromHolder::Answer { req, answer }))
                    .await
                    .ok();
                return;
            };
            let keep: Vec<ProcessIdentity> = identities
                .iter()
                .filter_map(|key| ProcessIdentity::parse(key))
                .collect();
            let out = out.clone();
            tokio::task::spawn_blocking(move || {
                let swept = terminal.kill_leftovers(&keep);
                let answer = Answer::Swept {
                    killed: swept.killed,
                    complete: swept.complete,
                };
                out.blocking_send(frame(&FromHolder::Answer { req, answer }))
                    .ok();
            });
        }
        ToHolder::Reap { id, req } => {
            let answer = match holder.terminal(&id) {
                Some(terminal) => match terminal.reap_if_exited() {
                    Ok(reaped) => Answer::Reaped {
                        reaped,
                        error: None,
                    },
                    Err(err) => Answer::Reaped {
                        reaped: false,
                        error: Some(err.to_string()),
                    },
                },
                None => Answer::UnknownTerminal,
            };
            out.send(frame(&FromHolder::Answer { req, answer }))
                .await
                .ok();
        }
        ToHolder::Release { id } => {
            holder.helpers.launches.revoke(&id);
            let released = holder.terminals().remove(&id);
            if let Some(terminal) = released {
                terminal.release();
                if let Some(companion) = terminal.bound_companion() {
                    holder.end_in_background(companion);
                }
                holder.settle();
            }
        }
        ToHolder::IssueLaunch {
            id,
            req,
            session,
            mcp_token,
        } => {
            let key = match holder
                .helpers
                .launches
                .issue(&id, &session, mcp_token.as_deref())
            {
                Ok(key) => Some(key),
                Err(err) => {
                    tracing::warn!(%err, "issuing a launch key failed");
                    None
                }
            };
            out.send(frame(&FromHolder::LaunchIssued { req, key }))
                .await
                .ok();
        }
        ToHolder::RevokeLaunch { id } => {
            holder.helpers.launches.revoke(&id);
            // The launch will not come, so neither will the terminal that would bind its
            // companion.
            let removed = holder.companions().remove(&id);
            if let Some(removed) = removed {
                holder.end_in_background(removed.companion);
                holder.settle();
            }
        }
        ToHolder::RelayReply { req, status, body } => holder
            .helpers
            .relays
            .answer(req, relay::Answer { status, body }),
        ToHolder::Parting { handover } => *parted = Some(handover),
        ToHolder::Handover { req, token } => {
            // Collected first: the map's lock is not held while a terminal is handed over. Each
            // mark waits for room on the link, so the marks are not made on the link's reader;
            // the answer goes on the same channel after them, so it reaches the daemon last.
            let terminals: Vec<Arc<Terminal>> = holder.terminals().values().cloned().collect();
            let out = out.clone();
            tokio::task::spawn_blocking(move || {
                let marked = terminals
                    .iter()
                    .filter(|terminal| terminal.hand_over(&out, &token))
                    .count();
                tracing::info!(marked, "the daemon handed its terminals over");
                out.blocking_send(frame(&FromHolder::Done { req, error: None }))
                    .ok();
            });
        }
        ToHolder::Drain => {
            holder.draining.store(true, Ordering::Release);
            holder.settle();
        }
        ToHolder::OpenEndpoint { id, req, kind } => {
            let answer = match holder.open_endpoint(&id, &kind, generation) {
                Ok(endpoint) => FromHolder::EndpointOpened {
                    req,
                    endpoint: Some(endpoint),
                    error: None,
                },
                Err(error) => FromHolder::EndpointOpened {
                    req,
                    endpoint: None,
                    error: Some(error),
                },
            };
            out.send(frame(&answer)).await.ok();
        }
        ToHolder::StartCompanion {
            id,
            req,
            command,
            ready,
            ready_within_ms,
            containment,
        } => {
            let start = CompanionStart {
                id,
                req,
                command: *command,
                ready,
                within: Duration::from_millis(ready_within_ms),
                containment,
            };
            tokio::spawn(holder.clone().start_companion(start, out.clone()));
        }
        ToHolder::EndCompanion { id, req } => {
            if holder.terminal(&id).is_some() {
                let error = Some("the companion is its terminal's and ends with it".to_string());
                out.send(frame(&FromHolder::Done { req, error })).await.ok();
                return;
            }
            // The end waits out the companion's grace, so it is not made on the link's reader.
            let (holder, out) = (holder.clone(), out.clone());
            tokio::spawn(async move {
                let ended = tokio::task::spawn_blocking(move || holder.end_unbound(&id)).await;
                let error = ended
                    .err()
                    .map(|_| "ending the companion panicked".to_string());
                out.send(frame(&FromHolder::Done { req, error })).await.ok();
            });
        }
    }
}

async fn refuse_write(out: &Sink, req: u64) {
    let refused = FromHolder::Written {
        req,
        written: 0,
        error: Some(WireError {
            kind: "BrokenPipe".to_string(),
            message: "no such terminal".to_string(),
        }),
    };
    out.send(frame(&refused)).await.ok();
}

fn answer(terminal: &Terminal, what: QueryKind) -> Answer {
    match what {
        QueryKind::Size => Answer::Size {
            size: terminal.pty().size().ok(),
        },
        QueryKind::ForegroundJob => Answer::ForegroundJob {
            foreground: terminal.pty().foreground_job().into(),
        },
        QueryKind::SlavePath => Answer::SlavePath {
            path: terminal
                .pty()
                .slave_path()
                .map(|path| path.to_string_lossy().into_owned()),
        },
        QueryKind::GroupHasMembers => Answer::Flag {
            value: terminal.group_has_members(),
        },
        QueryKind::ExitedUnreaped => Answer::Flag {
            value: terminal.exited_unreaped(),
        },
        QueryKind::Unknown => Answer::Unknown,
    }
}

#[cfg(test)]
mod tests;
