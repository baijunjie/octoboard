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
//! in the tails for the daemon that links next. It exits once it has no terminal and no start
//! under way, and either no daemon is linked or the linked one has said to drain. A link that
//! drops without the daemon having parted is recorded as a crash, for the next welcome to say.
//!
//! A daemon that stops on purpose for another to take over hands its terminals over
//! ([`ToHolder::Handover`]): each terminal its link follows is marked where its output stops going
//! to that daemon, and its tail records where the mark falls, so that the next daemon is handed
//! the history before it and exactly the output the stopping one never read.
//!
//! The same listener serves the helpers of the launches the holder runs ([`helpers`]): each launch
//! has a key the holder issued, and what a helper asks is relayed to the linked daemon
//! ([`relay`]).

mod helpers;
mod launches;
mod offline;
mod relay;
mod terminal;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};

use self::helpers::Helpers;
use self::launches::Launches;
use self::relay::Relays;
use self::terminal::{frame, Sink, Terminal, Told};
use super::protocol::{
    Answer, FromHolder, LastParting, OfflineAnswer, QueryKind, SpawnSpec, ToHolder, WireError,
};
use super::registry::{self, RegistryEntry};
use crate::platform::proc::ProcessIdentity;
use crate::platform::proc_table;
use crate::platform::term as host_term;
use crate::request_guard::{headers_allowed, same_secret};

/// How long a connection has to say hello.
const HELLO_DEADLINE: Duration = Duration::from_secs(10);

/// Frames queued for the daemon before the readers of the terminals wait for it.
const OUTBOUND_FRAMES: usize = 512;

/// How long the last frame of a link, and its close, have to be taken by the daemon at the other
/// end before the link is dropped without them.
const CLOSE_DEADLINE: Duration = Duration::from_secs(2);

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

struct Holder {
    config: HolderConfig,
    instance_id: String,
    secret: String,
    terminals: Mutex<HashMap<String, Arc<Terminal>>>,
    /// Terminals being started, which count as running when deciding whether to exit.
    pending: Mutex<HashSet<String>>,
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

    fn terminal(&self, id: &str) -> Option<Arc<Terminal>> {
        self.terminals().get(id).cloned()
    }

    /// Exits when nothing is left to serve: no terminal, no spawn under way, and either no daemon
    /// linked or the linked one has said to drain.
    fn settle(&self) {
        let nothing_left = self.terminals().is_empty() && self.pending().is_empty();
        let linked = self.link().current.is_some();
        if nothing_left && (!linked || self.draining.load(Ordering::Acquire)) {
            self.exit.send(true).ok();
        }
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

    /// Ends every program and forgets every terminal. **Blocks** off the runtime.
    async fn end_all(self: &Arc<Self>) {
        let all: Vec<Arc<Terminal>> = self
            .terminals()
            .drain()
            .map(|(_, terminal)| terminal)
            .collect();
        let ended = tokio::task::spawn_blocking(move || {
            for terminal in all {
                terminal.end();
            }
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

    async fn spawn(self: Arc<Self>, spec: SpawnSpec, reply: Sink) {
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
            // The key issued for the launch is of no use: its program will not run here. A key of
            // a terminal that was spawned already is that terminal's, and stays.
            if draining {
                self.helpers.launches.revoke(&id);
            }
            reply
                .send(frame(&FromHolder::SpawnFailed {
                    id,
                    error: why.to_string(),
                }))
                .await
                .ok();
            return;
        }
        // Its output goes to the link that is current as it starts, which is the request's own
        // unless another has taken the holder over since.
        let sink = self.link().sink.clone();
        let runtime = tokio::runtime::Handle::current();
        let started =
            tokio::task::spawn_blocking(move || Terminal::start(spec, sink, runtime)).await;
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
        }
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
                        Ok(message) => handle(&holder, &out, &mut parted, message).await,
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

async fn handle(holder: &Arc<Holder>, out: &Sink, parted: &mut Option<bool>, message: ToHolder) {
    match message {
        ToHolder::Hello { .. } | ToHolder::Unknown => {}
        ToHolder::Spawn(spec) => {
            tokio::spawn(holder.clone().spawn(*spec, out.clone()));
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
        ToHolder::RevokeLaunch { id } => holder.helpers.launches.revoke(&id),
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
