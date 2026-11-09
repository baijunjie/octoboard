//! A Codex session's private app server ([`server`]), and the automatic messages submitted to its
//! queue over JSON-RPC ([`rpc`]).
//!
//! A message goes to the session's own conversation, identified once and kept: the resumed thread,
//! or for a new session the first user thread the server reports starting to a connection opened
//! before the terminal starts. Other threads the server loads are never written to: Codex's title
//! generation runs in an ephemeral thread of its own after a turn, and the terminal's agent command
//! center can start further tasks, but the session's messages belong to its original one. Once that
//! one is known, a message never goes to the terminal, whose composer may be showing another task:
//! the server unloads the original a few minutes after the terminal switched away from it, and the
//! message reloads it (`thread/resume`, which also runs what its queue holds) or waits until it can,
//! the session being told once a wait has lasted. One Codex refuses outright fails, to its sender.
//!
//! The server belongs to the terminal: it is ended once the terminal has exited, and the terminal
//! is ended when the server dies, since it would otherwise wait for a server that never comes back.
//! Where the terminal runs in a holder, the server is the holder's too, started as the terminal's
//! companion: the holder ends it with the terminal, and a daemon that adopts the terminal links to
//! it again ([`CodexTransport::adopt`]). Octoboard's messages in the queue are known by their id,
//! `<launch id>:<ticket>` as the outbox names them. A message a planned restart carries over keeps
//! its ticket (`messaging::restart_outbox`), so the run that holds the transport now finds it in
//! the queue by that id, starts it where it waits and does not add it a second time. A daemon that
//! exits without a planned stop loses its outbox, and a message kept in the store is queued again
//! only by a later resume, under a new launch and ticket: it gets another id.

mod resume;
mod rpc;
mod server;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::transport_losing_an_add;

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use super::private_server::PrivateServerPlan;
use crate::platform::shell::ResolvedBinary;
use crate::platform::term::PartialWrite;
use crate::session::term::{self, MessageTransport, QueueStart, Sent, TransportEvents};
use crate::terminal::server_host::{RunningServer, ServerEndpoint, ServerHost};
use rpc::{Endpoint, Rpc, RpcFailure};
use server::{launch_server, watch_for_exit};

/// How long a message held back waits before the queue is written out again, where no event of
/// the server's says it can be taken now: the session has no conversation yet (its terminal still
/// starting or at its trust confirmation, or the agent command center open with no task), its
/// conversation could not be loaded again, or the server did not answer.
const IDENTITY_RETRY: Duration = Duration::from_secs(2);
/// How long messages may wait for the session's conversation before the session is told.
const HELD_NOTICE_AFTER: Duration = Duration::from_secs(30);
/// The most pages of a listing read; far more than one session's server holds.
const MAX_PAGES: usize = 8;
/// How long after starting the queue a second start, while the turn it started runs or finding
/// nothing of Octoboard's waiting, starts nothing and still reports what the first started: the
/// cancel's own hook and an owner's interrupt both start it, and the interrupt's note is to count
/// what ran.
const RECENT_START: Duration = Duration::from_secs(10);

/// The code of the notice that the session's messages have waited long.
fn held_code() -> crate::protocol::Code {
    crate::protocol::Code::of_provider(crate::providers::codex::MESSAGES_HELD)
}

/// The notice that messages for the session have waited long for Codex to take them.
fn held_notice() -> crate::protocol::Notice {
    crate::protocol::Notice::new(
        held_code(),
        format!(
            "Messages {} wrote for this session are waiting, because Codex is not taking them \
             yet. They go in once it does; nothing is written into the terminal meanwhile.",
            crate::APP_NAME
        ),
        &[],
    )
}

/// What the server, its watchers and the session share.
struct Shared {
    /// The session's own conversation, once identified.
    thread: OnceLock<String>,
    events: Mutex<Events>,
    stopping: AtomicBool,
    /// A retry of the queue is already scheduled.
    retry_scheduled: AtomicBool,
    /// The `clientUserMessageId` of every message this transport added to the queue, which tells
    /// its messages there from the user's along with [`Self::launch_prefix`].
    added: Mutex<std::collections::HashSet<String>>,
    /// `<launch id>:`, which the id of every message the outbox gives the launch starts with: a
    /// queued message whose id does is Octoboard's, whichever daemon run added it. Empty where the
    /// launch is not known, and then only [`Self::added`] tells.
    launch_prefix: String,
    /// When messages began to wait, whether the session has been told of the wait, and whether
    /// the wait has been logged as a warning.
    held_since: Mutex<Option<Held>>,
    /// When the queue was last started, and how many of this transport's messages it started.
    last_start: Mutex<Option<(Instant, usize)>>,
    /// The `clientUserMessageId` of every message Codex took into the queue but would not start
    /// yet, as it was overloaded: written again, such a message is started where it waits rather
    /// than added a second time.
    start_held: Mutex<std::collections::HashSet<String>>,
    /// [`HELD_NOTICE_AFTER`], lowered by a test.
    held_notice_after: Duration,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            thread: OnceLock::new(),
            events: Mutex::default(),
            stopping: AtomicBool::new(false),
            retry_scheduled: AtomicBool::new(false),
            added: Mutex::default(),
            launch_prefix: String::new(),
            held_since: Mutex::new(None),
            last_start: Mutex::new(None),
            start_held: Mutex::default(),
            held_notice_after: HELD_NOTICE_AFTER,
        }
    }
}

/// A wait of messages for Codex to take them.
struct Held {
    since: Instant,
    told: bool,
    warned: bool,
}

/// The session's handlers, and what happened before they were attached.
#[derive(Default)]
struct Events {
    handlers: Option<TransportEvents>,
    ready: bool,
    server_gone: bool,
    held: bool,
}

impl Shared {
    /// What a transport of the launch `launch_id` starts with.
    fn for_launch(launch_id: &str) -> Self {
        Self {
            launch_prefix: if launch_id.is_empty() {
                String::new()
            } else {
                format!("{launch_id}:")
            },
            ..Self::default()
        }
    }

    fn thread(&self) -> Option<String> {
        self.thread.get().cloned()
    }

    /// Remembers `id` as the session's conversation, unless one already is; whether it now is.
    fn identify(&self, id: &str) -> bool {
        self.thread.get_or_init(|| id.to_string()) == id
    }

    fn ready(&self) {
        let mut events = self.events.lock().expect("Codex events lock poisoned");
        match &events.handlers {
            Some(handlers) => (handlers.ready)(),
            None => events.ready = true,
        }
    }

    fn server_gone(&self) {
        let mut events = self.events.lock().expect("Codex events lock poisoned");
        match &events.handlers {
            Some(handlers) => (handlers.server_gone)(),
            None => events.server_gone = true,
        }
    }

    /// Notes that a message waits, for `reason`: logged as a warning once a wait and then only
    /// for debugging, and told to the session once the wait has lasted
    /// [`Self::held_notice_after`].
    fn note_held(&self, reason: &str) {
        let mut held = self.held_since.lock().expect("Codex hold lock poisoned");
        let held = held.get_or_insert(Held {
            since: Instant::now(),
            told: false,
            warned: false,
        });
        if held.warned {
            tracing::debug!(%reason, "a message for the session still waits for Codex");
        } else {
            held.warned = true;
            tracing::warn!(%reason, "a message for the session waits for Codex");
        }
        if held.told || held.since.elapsed() < self.held_notice_after {
            return;
        }
        held.told = true;
        let mut events = self.events.lock().expect("Codex events lock poisoned");
        match &events.handlers {
            Some(handlers) => (handlers.held)(held_notice()),
            None => events.held = true,
        }
    }

    /// Ends a wait, withdrawing what the session was told of it.
    fn end_held(&self) {
        let told = self
            .held_since
            .lock()
            .expect("Codex hold lock poisoned")
            .take()
            .is_some_and(|held| held.told);
        let mut events = self.events.lock().expect("Codex events lock poisoned");
        events.held = false;
        if let (true, Some(handlers)) = (told, &events.handlers) {
            (handlers.released)(held_code());
        }
    }

    /// Has the queue written out again after [`IDENTITY_RETRY`], once however often it is asked.
    fn retry_later(self: &Arc<Self>) {
        if self.retry_scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let shared = Arc::downgrade(self);
        std::thread::spawn(move || {
            std::thread::sleep(IDENTITY_RETRY);
            if let Some(shared) = shared.upgrade() {
                shared.retry_scheduled.store(false, Ordering::Release);
                if !shared.stopping.load(Ordering::Acquire) {
                    shared.ready();
                }
            }
        });
    }

    /// How many of this transport's messages the queue start made within [`RECENT_START`] found
    /// waiting, if one did.
    fn recent_start(&self) -> Option<usize> {
        self.last_start
            .lock()
            .expect("Codex start lock poisoned")
            .filter(|(at, _)| at.elapsed() < RECENT_START)
            .map(|(_, started)| started)
    }

    /// Forgets a message's id once Codex refused it, so nothing of its is looked for in the queue.
    fn forget_added(&self, id: &str) {
        self.added
            .lock()
            .expect("Codex added lock poisoned")
            .remove(id);
    }

    /// Forgets that the message `id` waits in the queue unstarted.
    fn forget_start_held(&self, id: &str) {
        self.start_held
            .lock()
            .expect("Codex start-held lock poisoned")
            .remove(id);
    }

    fn was_added(&self, id: &str) -> bool {
        self.added
            .lock()
            .expect("Codex added lock poisoned")
            .contains(id)
    }

    /// Whether `id` names a message of the session's launch, whichever daemon run gave it.
    fn of_this_launch(&self, id: &str) -> bool {
        !self.launch_prefix.is_empty() && id.starts_with(&self.launch_prefix)
    }

    /// Whether the queued message `id` is Octoboard's: added by this transport, or named as the
    /// outbox names the launch's messages, as one an earlier daemon run added is.
    fn is_own(&self, id: &str) -> bool {
        self.was_added(id) || self.of_this_launch(id)
    }

    fn is_start_held(&self, id: &str) -> bool {
        self.start_held
            .lock()
            .expect("Codex start-held lock poisoned")
            .contains(id)
    }
}

pub struct CodexTransport {
    /// The server, where it still runs as far as this transport knows; taken out once stopped.
    server: Arc<Mutex<Option<Box<dyn RunningServer>>>>,
    endpoint: Endpoint,
    shared: Arc<Shared>,
    operation: Mutex<()>,
}

impl CodexTransport {
    /// Starts the private server of the launch `launch_id` through `host`, as `plan` says, before
    /// the terminal. A resume starts it twice: the first, on an endpoint of its own, tells the
    /// fingerprints of the hooks to trust, and is ended before the second starts. A start that
    /// fails leaves whatever the daemon ran ended; what a holder ran is ended through `host`
    /// ([`ServerHost::end`]). **Blocks.**
    pub fn start(
        binary: &ResolvedBinary,
        plan: &PrivateServerPlan,
        environment: &HashMap<String, String>,
        cwd: &Path,
        host: &dyn ServerHost,
        launch_id: &str,
    ) -> Result<Self> {
        let launch = |args: &[String]| launch_server(host, binary, args, environment, cwd);
        let mut server_args = plan.args.clone();
        let (mut server, mut endpoint) = launch(&server_args)?;
        let mut rpc = endpoint
            .connect()
            .context("initializing the private Codex app server")?;
        if plan.resume_thread.is_some() {
            // A remote resume ignores the hook-trust bypass. Trust only the generated command, by
            // Codex's own fingerprint of it, as overrides of this start alone.
            let metadata = rpc.request("hooks/list", json!({"cwds":[cwd]}))?;
            let trusts = resume::own_hook_trust(&metadata, &plan.hook_command)?;
            drop(rpc);
            // The first server goes, and what it left with it, before the next starts elsewhere.
            server.end();
            drop(server);
            server_args.extend(trusts);
            (server, endpoint) = launch(&server_args)?;
            rpc = endpoint
                .connect()
                .context("initializing the restarted private Codex app server")?;
        }
        let shared = Arc::new(Shared::for_launch(launch_id));
        if let Some(thread) = &plan.resume_thread {
            if let Some(permissions) = plan.resume_permissions {
                rpc.request(
                    "thread/resume",
                    resume::resume_params(thread, cwd, permissions),
                )
                .context("applying the Codex permission preset before the remote resume")?;
            }
            shared.identify(thread);
        } else {
            // Opened before the terminal starts, so it is told of the thread the terminal starts.
            watch_for_thread(rpc, shared.clone());
        }
        Ok(Self::running(server, endpoint, shared))
    }

    /// The transport of the launch `launch_id`, whose terminal a daemon took over, to `server`
    /// listening on `endpoint`, which a holder runs with the terminal. It opens no server. With
    /// the session's conversation known (`thread`) it is the session's at once; without, a thread
    /// the user starts from now on is watched for, and one started before is identified when a
    /// message finds it loaded, as for any session. A server already gone is reported once the
    /// session attaches. Fails only for an endpoint that names no port to reach. Never waits for
    /// the server: the thread watch is opened on a thread of its own, since a daemon adopts its
    /// terminals within the time its start is given, and a server that does not answer would hold
    /// the start up to the RPC timeout. A thread started before the watch is open is identified
    /// as one started before the adoption is.
    pub fn adopt(
        endpoint: ServerEndpoint,
        server: Box<dyn RunningServer>,
        launch_id: &str,
        thread: Option<&str>,
    ) -> Result<Self> {
        let endpoint = Endpoint::reached_at(endpoint, None)
            .ok_or_else(|| anyhow!("the private Codex app server's endpoint names no port"))?;
        let shared = Arc::new(Shared::for_launch(launch_id));
        match thread {
            Some(thread) => {
                shared.identify(thread);
            }
            None => {
                let (watched, watching) = (endpoint.clone(), shared.clone());
                std::thread::spawn(move || match watched.connect() {
                    Ok(rpc) => watch_for_thread(rpc, watching),
                    // Gone, or not answering: its end is reported by the exit watch, and a message
                    // looks for the conversation itself.
                    Err(error) => {
                        tracing::debug!(%error, "could not watch an adopted Codex server for its thread");
                    }
                });
            }
        }
        Ok(Self::running(server, endpoint, shared))
    }

    /// The transport to `server`, reached at `endpoint`, watched for its exit from now on.
    fn running(server: Box<dyn RunningServer>, endpoint: Endpoint, shared: Arc<Shared>) -> Self {
        let server = Arc::new(Mutex::new(Some(server)));
        let (stopping, gone) = (shared.clone(), shared.clone());
        watch_for_exit(
            server.clone(),
            move || stopping.stopping.load(Ordering::Acquire),
            move || {
                tracing::warn!("the private Codex app server exited; ending its terminal");
                gone.server_gone();
            },
        );
        Self {
            server,
            endpoint,
            shared,
            operation: Mutex::new(()),
        }
    }

    pub fn client_args(&self) -> Vec<String> {
        self.endpoint.client_args()
    }

    pub fn client_env(&self) -> Vec<(String, String)> {
        self.endpoint.client_env()
    }

    fn server_exited(&self) -> bool {
        self.server
            .lock()
            .expect("Codex server lock poisoned")
            .as_ref()
            .is_none_or(|server| server.exited())
    }

    /// The session's conversation, identified now if it has not been, and loaded again if the
    /// server unloaded it. A message waits while the server has loaded no conversation the session
    /// could own, or the session's own cannot be loaded; it goes to the terminal only while the
    /// session's own is not known and cannot be told apart from others.
    fn conversation(&self, rpc: &mut Rpc) -> Result<String, Refusal> {
        let loaded = loaded_threads(rpc).map_err(|error| Refusal::Decline(error.to_string()))?;
        if let Some(thread) = self.shared.thread() {
            if loaded.contains(&thread) {
                return Ok(thread);
            }
            // Its settings are left as they are: a resume naming nothing else keeps them.
            return match load_again(rpc, &thread) {
                Ok(()) => Ok(thread),
                Err(error) => {
                    tracing::debug!(%error, "could not load the session's Codex conversation again");
                    Err(Refusal::Wait(
                        "the session's conversation could not be loaded",
                    ))
                }
            };
        }
        let found =
            session_threads(rpc, &loaded).map_err(|error| Refusal::Decline(error.to_string()))?;
        match (found.candidates.len(), found.users.as_slice()) {
            (0, _) => Err(Refusal::Wait("the session has no conversation yet")),
            (_, [only]) => {
                self.shared.identify(only);
                self.shared
                    .thread()
                    .ok_or(Refusal::Wait("the session has no conversation yet"))
            }
            (_, []) => Err(Refusal::Decline(
                "none of the server's conversations was started by the user".to_string(),
            )),
            _ => Err(Refusal::Decline(
                "the server holds several conversations the session could own".to_string(),
            )),
        }
    }

    /// Whether a queue start that was refused leaves the message `id` running or to run: gone from
    /// the queue, or still in it behind a turn that is running. A thread the server unloaded
    /// meanwhile is loaded again, which runs its queue, and judged again. `None` when that cannot
    /// be told.
    fn runs_after_refused_start(&self, rpc: &mut Rpc, thread: &str, id: &str) -> Option<bool> {
        for attempt in 0..2 {
            let queued = queued_items(rpc, thread).ok()?;
            if !queued.iter().any(|item| item.id == id) {
                return Some(true);
            }
            match thread_status(rpc, thread).ok()?.as_str() {
                "active" => return Some(true),
                "notLoaded" if attempt == 0 => load_again(rpc, thread).ok()?,
                _ => return Some(false),
            }
        }
        Some(false)
    }

    /// Has the message `id`, just added to `thread`'s queue, run in its turn, without overtaking
    /// anything queued before it. An idle thread starts an item added to an empty queue itself;
    /// one added behind others waits there (Codex 0.166.0, after a cancel left items queued), so
    /// the queue's first item is started where it is Octoboard's, and nothing someone else queued
    /// is started or overtaken. Whether the message runs is read off the queue afterwards, not off
    /// what a refused start says; a start Codex refuses as overloaded leaves the message where it
    /// waits, to be started when it is written again.
    fn start_after_add(
        &self,
        rpc: &mut Rpc,
        thread: &str,
        id: &str,
    ) -> Result<AfterAdd, RpcFailure> {
        let queued = queued_items(rpc, thread)?;
        let Some(head) = queued.first() else {
            return Ok(AfterAdd::Runs);
        };
        if !queued.iter().any(|item| item.id == id) {
            return Ok(AfterAdd::Runs);
        }
        if !self.shared.is_own(&head.client_id) {
            tracing::info!("a message waits in Codex's queue behind one someone else queued");
            return Ok(AfterAdd::Runs);
        }
        let refusal = match rpc.request(
            "thread/queue/start",
            json!({"threadId": thread, "queuedSubmissionId": head.id}),
        ) {
            Ok(_) => return Ok(AfterAdd::Runs),
            Err(refusal) => refusal,
        };
        if self.runs_after_refused_start(rpc, thread, id) == Some(true) {
            return Ok(AfterAdd::Runs);
        }
        match refusal {
            RpcFailure::Rejected { code, message } if is_transient(code, &message) => {
                Ok(AfterAdd::Held)
            }
            _ => Err(RpcFailure::Transport(
                "Codex did not start the queued message".to_string(),
            )),
        }
    }

    /// The id Codex gave the message `logical_id` in `thread`'s queue, where it may be there
    /// already: Codex took it before but would not start it, as it was overloaded, or it is of the
    /// session's launch and an earlier daemon run may have queued it. An earlier run's message
    /// keeps its `logical_id` only where a planned restart carried it over with its ticket.
    /// `Some(None)` when it is not queued; `None` for a message that cannot be there.
    fn already_queued(
        &self,
        rpc: &mut Rpc,
        thread: &str,
        logical_id: &str,
    ) -> Option<Result<Option<String>, RpcFailure>> {
        if !self.shared.is_start_held(logical_id) && !self.shared.of_this_launch(logical_id) {
            return None;
        }
        Some(queued_items(rpc, thread).map(|queued| {
            queued
                .into_iter()
                .find(|item| item.client_id == logical_id)
                .map(|item| item.id)
        }))
    }

    /// Starts the first of this transport's messages waiting in `thread`'s queue, unless something
    /// someone else queued is ahead of it there, which keeps its place.
    fn start_own_queued(&self, rpc: &mut Rpc, thread: &str) -> Result<QueueStart, RpcFailure> {
        let queued = queued_items(rpc, thread)?;
        let Some(first) = queued
            .iter()
            .position(|item| self.shared.is_own(&item.client_id))
        else {
            return Ok(self
                .shared
                .recent_start()
                .map_or(QueueStart::Nothing, QueueStart::Started));
        };
        let own = queued
            .iter()
            .filter(|item| self.shared.is_own(&item.client_id))
            .count();
        // Started a moment ago, by the cancel's own hook or an owner's interrupt, and running: what
        // follows its first message runs after it, so nothing is started again, and the first
        // start's count stands where it was the larger. An idle thread had a cancel of its own
        // since, and its queue is started as after any cancel.
        if let Some(started) = self.shared.recent_start() {
            let status = thread_status(rpc, thread);
            if status.as_ref().map_or(true, |status| status == "active") {
                if let Err(error) = status {
                    tracing::debug!(%error, "could not read whether Codex runs a turn");
                }
                return Ok(QueueStart::Started(started.max(own)));
            }
        }
        if first > 0 {
            return Ok(QueueStart::BehindOthers(own));
        }
        // Codex runs what follows it after its turn, as after any turn.
        let id = &queued[first].id;
        let started = match rpc.request(
            "thread/queue/start",
            json!({"threadId": thread, "queuedSubmissionId": id}),
        ) {
            Ok(_) => true,
            Err(_) => self.runs_after_refused_start(rpc, thread, id) == Some(true),
        };
        if started {
            *self
                .shared
                .last_start
                .lock()
                .expect("Codex start lock poisoned") = Some((Instant::now(), own));
            Ok(QueueStart::Started(own))
        } else {
            Err(RpcFailure::Transport(
                "Codex did not start the queued message".to_string(),
            ))
        }
    }
}

impl Drop for CodexTransport {
    fn drop(&mut self) {
        self.stop();
    }
}

/// What came of starting a message just added to the queue.
enum AfterAdd {
    /// It runs, or is to run after what is ahead of it.
    Runs,
    /// Codex was overloaded and did not start it; it waits in the queue.
    Held,
}

/// Why a message did not go into the queue, with nothing of it sent.
enum Refusal {
    /// It is to wait, and the queue is written out again later.
    Wait(&'static str),
    /// The terminal is to take it, while the session's own conversation is not known; once it is
    /// (`send` reads it again then), the message waits instead.
    Decline(String),
}

impl MessageTransport for CodexTransport {
    fn attach(&self, events: TransportEvents) {
        let mut slot = self
            .shared
            .events
            .lock()
            .expect("Codex events lock poisoned");
        if std::mem::take(&mut slot.server_gone) {
            (events.server_gone)();
        }
        if std::mem::take(&mut slot.held) {
            (events.held)(held_notice());
        }
        if std::mem::take(&mut slot.ready) {
            (events.ready)();
        }
        slot.handlers = Some(events);
    }

    fn start_queue(&self) -> QueueStart {
        let _operation = self.operation.lock().expect("Codex message lock poisoned");
        // Nothing can be queued before the conversation is known.
        let Some(thread) = self.shared.thread() else {
            return QueueStart::Nothing;
        };
        let started = self
            .endpoint
            .connect()
            .and_then(|mut rpc| self.start_own_queued(&mut rpc, &thread));
        match started {
            Ok(outcome) => outcome,
            Err(error) => QueueStart::Failed(error.to_string()),
        }
    }

    fn stop(&self) {
        // What the session was told of a wait no longer holds once its server is gone.
        self.shared.end_held();
        self.shared.stopping.store(true, Ordering::Release);
        let server = self
            .server
            .lock()
            .expect("Codex server lock poisoned")
            .take();
        // A server the holder runs with the terminal is the holder's to end, and outlives a daemon
        // that stops before the terminal does.
        if let Some(server) = server.filter(|server| !server.ends_with_its_terminal()) {
            server.end();
        }
    }

    fn send(&self, text: &str, logical_id: &str) -> Result<Sent, PartialWrite> {
        let _operation = self.operation.lock().expect("Codex message lock poisoned");
        let wait = |reason: &str| {
            self.shared.note_held(reason);
            self.shared.retry_later();
            PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::WouldBlock,
                    term::HeldByServer(reason.to_string()),
                ),
            }
        };
        // The one place the terminal is chosen, with the conversation read at that moment: a
        // thread the watch identified meanwhile keeps the message for itself.
        let decline = |reason: String| {
            if self.shared.thread().is_some() {
                tracing::debug!(%reason, "the private Codex app server did not take a message");
                return Err(wait("the private Codex app server did not answer"));
            }
            self.shared.end_held();
            Ok(Sent::Declined(reason))
        };
        let unconfirmed = |detail: String| {
            self.shared.end_held();
            uncertain(detail)
        };
        // A message in the queue is started; one Codex was too overloaded to start waits there, to
        // be started when it is written again.
        let start =
            |rpc: &mut Rpc, thread: &str, id: &str| match self.start_after_add(rpc, thread, id) {
                Ok(AfterAdd::Runs) => {
                    self.shared.forget_start_held(logical_id);
                    self.shared.end_held();
                    Ok(Sent::Queued)
                }
                Ok(AfterAdd::Held) => {
                    self.shared
                        .start_held
                        .lock()
                        .expect("Codex start-held lock poisoned")
                        .insert(logical_id.to_string());
                    Err(wait("the private Codex app server is overloaded"))
                }
                Err(error) => {
                    self.shared.forget_start_held(logical_id);
                    Err(unconfirmed(format!(
                        "Codex accepted the queued message but its start could not be confirmed: \
                     {error}"
                    )))
                }
            };
        let mut rpc = match self.endpoint.connect() {
            Ok(rpc) => rpc,
            // A server that is gone takes its terminal with it, so the message waits for that end
            // rather than being written into a terminal that can no longer send it.
            Err(_) if self.server_exited() => {
                return Err(PartialWrite {
                    written: 0,
                    error: io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "the private Codex app server has exited",
                    ),
                })
            }
            Err(error) => return decline(error.to_string()),
        };
        let thread_id = match self.conversation(&mut rpc) {
            Ok(thread) => thread,
            Err(Refusal::Wait(reason)) => return Err(wait(reason)),
            Err(Refusal::Decline(reason)) => return decline(reason),
        };
        // Taken before, by this run or an earlier one whose planned restart carried the message
        // over under the same id, and still waiting: started where it waits, never added a second
        // time. One taken but not started, as Codex was overloaded, that is no longer queued has
        // run since.
        match self.already_queued(&mut rpc, &thread_id, logical_id) {
            None => {}
            Some(Ok(Some(id))) => return start(&mut rpc, &thread_id, &id),
            Some(Ok(None)) if self.shared.is_start_held(logical_id) => {
                self.shared.forget_start_held(logical_id);
                self.shared.end_held();
                return Ok(Sent::Queued);
            }
            Some(Ok(None)) => {}
            Some(Err(error)) => {
                tracing::debug!(%error, "could not read Codex's queue");
                return Err(wait("the private Codex app server did not answer"));
            }
        }
        // Recorded before the request, so a message whose acceptance is not confirmed is still
        // known as Octoboard's in the queue and started after a cancel.
        self.shared
            .added
            .lock()
            .expect("Codex added lock poisoned")
            .insert(logical_id.to_string());
        let added = rpc.request(
            "thread/queue/add",
            json!({
                "threadId":thread_id,
                "input":[{"type":"text","text":text,"text_elements":[]}],
                "clientUserMessageId":logical_id,
            }),
        );
        let added = match added {
            Ok(added) => added,
            // Codex answers a request it has no room for now that way; it takes nothing in.
            Err(RpcFailure::Rejected { code, message }) if is_transient(code, &message) => {
                self.shared.forget_added(logical_id);
                return Err(wait("the private Codex app server is overloaded"));
            }
            // Codex takes a message even for a conversation it has unloaded, so another refusal is
            // of the message itself: it fails, and is not tried again by any route.
            Err(RpcFailure::Rejected { message, .. }) => {
                self.shared.forget_added(logical_id);
                self.shared.end_held();
                return Err(PartialWrite {
                    written: 0,
                    error: io::Error::other(term::RefusedByAgent(message)),
                });
            }
            // Losing the answer may have lost an acceptance.
            Err(error) => return Err(unconfirmed(error.to_string())),
        };
        let Some(id) = added
            .pointer("/queuedSubmission/id")
            .and_then(Value::as_str)
        else {
            return Err(unconfirmed(
                "Codex acknowledged a queue request without its submission id".to_string(),
            ));
        };
        start(&mut rpc, &thread_id, id)
    }
}

fn uncertain(detail: impl Into<String>) -> PartialWrite {
    PartialWrite {
        written: 1,
        error: io::Error::other(term::UncertainSubmission(detail.into())),
    }
}

/// Whether a refusal of Codex's is for now only: the app server answers a request it has no room
/// for with "Server overloaded; retry later." (in Codex 0.166.0's program; the code is taken to be
/// -32001, its JSON-RPC server-error code for it).
fn is_transient(code: i64, message: &str) -> bool {
    code == -32001 || message.contains("retry later") || message.contains("overloaded")
}

/// Whether `thread`, as the server describes one, is a conversation the user holds in the
/// terminal: started by the user, kept on disk, and no sub-agent's. Codex's title generation runs
/// in an ephemeral thread of its own.
fn is_user_thread(thread: &Value) -> bool {
    is_candidate_thread(thread)
        && thread.get("threadSource").and_then(Value::as_str) == Some("user")
}

/// Whether `thread` could be the session's conversation at all: kept on disk and no sub-agent's.
fn is_candidate_thread(thread: &Value) -> bool {
    thread.get("ephemeral").and_then(Value::as_bool) == Some(false)
        && thread.get("parentThreadId").is_none_or(Value::is_null)
}

/// Every item of a paginated listing `method` answers, for `params`.
fn listed(rpc: &mut Rpc, method: &str, params: Value) -> Result<Vec<Value>, RpcFailure> {
    let mut items = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..MAX_PAGES {
        let mut params = params.clone();
        params["cursor"] = cursor;
        let page = rpc.request(method, params)?;
        items.extend(
            page.get("data")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned(),
        );
        cursor = page.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() {
            return Ok(items);
        }
    }
    Err(RpcFailure::Transport(format!(
        "the server's {method} did not fit the pages read"
    )))
}

/// The ids of the threads the server holds loaded.
fn loaded_threads(rpc: &mut Rpc) -> Result<Vec<String>, RpcFailure> {
    Ok(listed(rpc, "thread/loaded/list", json!({}))?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect())
}

/// One message waiting in a thread's queue.
struct QueuedItem {
    id: String,
    client_id: String,
}

/// The messages waiting in `thread`'s queue, in order.
fn queued_items(rpc: &mut Rpc, thread: &str) -> Result<Vec<QueuedItem>, RpcFailure> {
    Ok(
        listed(rpc, "thread/queue/list", json!({"threadId": thread}))?
            .iter()
            .filter_map(|item| {
                Some(QueuedItem {
                    id: item.get("id")?.as_str()?.to_string(),
                    client_id: item
                        .get("clientUserMessageId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect(),
    )
}

/// The status of `thread`: `active` with a turn running, `idle`, `notLoaded`, or `systemError`.
fn thread_status(rpc: &mut Rpc, thread: &str) -> Result<String, RpcFailure> {
    let read = rpc.request("thread/read", json!({"threadId": thread}))?;
    Ok(read
        .pointer("/thread/status/type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

/// Loads `thread` again after the server unloaded it, leaving its settings as they are: a resume
/// naming nothing else keeps them. Codex then runs what its queue holds. The connection that loads
/// it is subscribed to it, so a request of a turn that starts then (an approval) goes to this
/// connection while it is open and waits for the terminal once it closes, which shows it when the
/// person opens that task.
fn load_again(rpc: &mut Rpc, thread: &str) -> Result<(), RpcFailure> {
    rpc.request(
        "thread/resume",
        json!({"threadId": thread, "excludeTurns": true}),
    )
    .map(|_| ())
}

/// Which of the threads `ids` could be the session's conversation, and which of those the user
/// started.
#[derive(Default)]
struct SessionThreads {
    candidates: Vec<String>,
    users: Vec<String>,
}

fn session_threads(rpc: &mut Rpc, ids: &[String]) -> Result<SessionThreads, RpcFailure> {
    let mut found = SessionThreads::default();
    for id in ids {
        let thread = match rpc.request("thread/read", json!({"threadId": id})) {
            Ok(read) => read.get("thread").cloned().unwrap_or(Value::Null),
            // One that closed since the list, as a title thread does, is none of them.
            Err(RpcFailure::Rejected { .. }) => continue,
            Err(error) => return Err(error),
        };
        if is_candidate_thread(&thread) {
            found.candidates.push(id.clone());
            if is_user_thread(&thread) {
                found.users.push(id.clone());
            }
        }
    }
    Ok(found)
}

/// Watches a connection for the first user thread the server starts, takes it as the session's
/// conversation, and lets the session's queue be written out.
fn watch_for_thread(mut rpc: Rpc, shared: Arc<Shared>) {
    std::thread::spawn(move || loop {
        // Identified meanwhile by a message that found it loaded, or stopping: nothing to watch.
        if shared.stopping.load(Ordering::Acquire) || shared.thread().is_some() {
            return;
        }
        match rpc.next_message(Duration::from_secs(1)) {
            Ok(Some(value)) => {
                if value.get("method").and_then(Value::as_str) == Some("thread/started")
                    && value.pointer("/params/thread").is_some_and(is_user_thread)
                {
                    if let Some(id) = value.pointer("/params/thread/id").and_then(Value::as_str) {
                        if shared.identify(id) {
                            shared.ready();
                        }
                        return;
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::debug!(%error, "the Codex thread watch ended");
                return;
            }
        }
    });
}
