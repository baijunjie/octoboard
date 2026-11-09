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

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::private_server::PrivateServerPlan;
use crate::platform::proc::Containment;
use crate::platform::shell::ResolvedBinary;
use crate::platform::term::PartialWrite;
use crate::session::term::{self, MessageTransport, QueueStart, Sent, TransportEvents};
use rpc::{Endpoint, Rpc, RpcFailure};
use server::{launch_server, watch_for_exit, Listen, ServerProcess};

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
    /// its messages there from the user's.
    added: Mutex<std::collections::HashSet<String>>,
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
}

pub struct CodexTransport {
    server: Arc<Mutex<Option<ServerProcess>>>,
    endpoint: Endpoint,
    shared: Arc<Shared>,
    operation: Mutex<()>,
    /// Kept for its `Drop` alone: the socket's directory goes once the server is gone.
    _listen: Option<Listen>,
}

impl CodexTransport {
    pub fn start(
        binary: &ResolvedBinary,
        plan: &PrivateServerPlan,
        environment: &HashMap<String, String>,
        cwd: &Path,
        containment: &Containment,
    ) -> Result<Self> {
        let start = |args: &[String]| -> Result<(ServerProcess, Endpoint, Listen)> {
            let listen = Listen::for_host().context("preparing the Codex server's endpoint")?;
            let (server, endpoint) =
                launch_server(binary, args, &listen, environment, cwd, containment)?;
            Ok((server, endpoint, listen))
        };
        let mut server_args = plan.args.clone();
        let (mut server, mut endpoint, mut listen) = start(&server_args)?;
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
            drop(server);
            drop(listen);
            server_args.extend(trusts);
            (server, endpoint, listen) = start(&server_args)?;
            rpc = endpoint
                .connect()
                .context("initializing the restarted private Codex app server")?;
        }
        let shared = Arc::new(Shared::default());
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
        Ok(Self {
            server,
            endpoint,
            shared,
            operation: Mutex::new(()),
            _listen: Some(listen),
        })
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
            .is_none_or(ServerProcess::exited)
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
        if !self.shared.was_added(&head.client_id) {
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

    /// The id Codex gave the message `logical_id` in `thread`'s queue, where Codex took it before
    /// but would not start it: `Some(None)` once it is no longer queued, as it has run since.
    /// `None` for a message not held so.
    fn held_in_queue(
        &self,
        rpc: &mut Rpc,
        thread: &str,
        logical_id: &str,
    ) -> Option<Result<Option<String>, RpcFailure>> {
        if !self
            .shared
            .start_held
            .lock()
            .expect("Codex start-held lock poisoned")
            .contains(logical_id)
        {
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
            .position(|item| self.shared.was_added(&item.client_id))
        else {
            return Ok(self
                .shared
                .recent_start()
                .map_or(QueueStart::Nothing, QueueStart::Started));
        };
        let own = queued
            .iter()
            .filter(|item| self.shared.was_added(&item.client_id))
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
        drop(server);
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
        // Taken before, but not started as Codex was overloaded: started where it waits.
        match self.held_in_queue(&mut rpc, &thread_id, logical_id) {
            None => {}
            Some(Ok(Some(id))) => return start(&mut rpc, &thread_id, &id),
            Some(Ok(None)) => {
                self.shared.forget_start_held(logical_id);
                self.shared.end_held();
                return Ok(Sent::Queued);
            }
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
