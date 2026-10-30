//! The processes each agent launch started, wherever they went, and stopping them when the launch
//! ends. Agent CLIs run every shell command in a terminal session of its own (`setsid`), so what a
//! command leaves running — a dev server, a preview, a watcher — is out of reach of the sweep of the
//! agent's own session (`crate::process_session`) and, once its shell exits, reparented away from
//! the agent too. The ledger follows those processes itself, from the process table alone, without
//! the agent having to do or remember anything; and it never stops a process it cannot attribute
//! to the launch being ended.
//!
//! Every launch is tagged ([`ProcessTag`]: [`TAG_VARIABLE`] in the agent's environment, which its
//! descendants inherit), and a dedicated thread reads the process table every
//! [`SCAN_INTERVAL`] while there is anything to follow, attributing processes to launches by the
//! rules in `attribute.rs`. A process is told by its pid and start time (`table::Identity`), and
//! every signal checks the start time again just before it is sent.
//!
//! The processes of a launch are stopped when its agent process ends ([`ProcessLedger::end_launch`],
//! called before the session reads as stopped, and so before an archive removes its worktree), when
//! one of them has worked in a removed directory for [`CWD_GRACE`], and, after a restart or crash
//! of the daemon, by [`ProcessLedger::reconcile`] from what the previous run wrote down. What the
//! user chose to keep is never stopped, and neither is the daemon, one of its ancestors or one of
//! its direct children.
//!
//! A launch's processes can change hands instead ([`attribute::Book::transfer`]): all of them when
//! its agent ends after a handoff, an account switch, a model change or a change of an account's
//! sharing marked it to be carried over ([`ProcessLedger::hand_over`]), to the launch of the
//! session that takes its place or to the restarted session's relaunch, and chosen trees of a
//! running launch at an owner's word ([`ProcessLedger::move_trees`]). The launch that took them
//! stops them when it ends. Carried over before the receiving session has a launch running, they
//! are held for its next one for as long as the caller holds the session open for it
//! ([`ProcessLedger::hold_for_launch`]), and stopped once nothing does.
//!
//! What it cannot do: a helper a kept process starts that leaves the kept process's terminal
//! session (`setsid`) and its parent (a double fork) between two scans is reached neither by the
//! chain nor by the session, and still carries the launch's tag; it is stopped with the launch.
//! A process whose environment cannot be read (an Apple platform binary on macOS) that a shell
//! left running in the background is never attributed when that shell exits before a scan has
//! seen it: its chain no longer reaches the launch, the shell's session was never recorded, and
//! there is no tag to read. It is neither listed nor stopped with the launch.
//! A process that outlives `SIGKILL` (one in an uninterruptible sleep) is tried again only after
//! [`STUBBORN_RETRY`] or once another launch ends.

mod attribute;
mod carry;
mod persist;
mod stop;
mod table;
#[cfg(test)]
mod tests;
mod trees;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant};

use attribute::{Attributed, Book, Launch, ENDED};
use table::{EnvTag, Identity, Proc, Table};

pub use carry::HeldForLaunch;
pub use table::TAG_VARIABLE;
pub use trees::MoveRefused;
pub use trees::ProcessTree;

/// How often the table is read while there is anything to follow.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);

/// How long a process may work in a removed directory before it is stopped.
const CWD_GRACE: Duration = Duration::from_secs(60);

/// How many times a stop reads the table again for what started while it was stopping.
const STOP_PASSES: usize = 3;

/// The least time between two writes of the ledger file that a scan asks for. The end of a
/// launch, a keep and the reconcile at start write at once.
const WRITE_DEBOUNCE: Duration = Duration::from_secs(10);

/// How long a process found running after a `SIGKILL` is left before it is tried again, unless a
/// launch ends first.
const STUBBORN_RETRY: Duration = Duration::from_secs(5 * 60);

/// Takes [`TAG_VARIABLE`] out of the daemon's own environment. A daemon started from an agent's
/// shell (a development daemon run from a worktree) carries that launch's tag, which every process
/// it starts would inherit. Changing the environment races with any other thread reading it, so
/// this is called while the daemon has no other thread yet.
pub fn forget_inherited_tag() {
    std::env::remove_var(TAG_VARIABLE);
}

/// What one launch's processes carry in their environment, as [`TAG_VARIABLE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTag {
    instance: String,
    launch: String,
}

impl ProcessTag {
    pub fn launch_id(&self) -> &str {
        &self.launch
    }

    /// The variable's value: `<instance id>:<launch id>`.
    pub fn value(&self) -> String {
        format!("{}:{}", self.instance, self.launch)
    }
}

/// A launch id reserved for a launch about to start, so that a process carrying it is never taken
/// for an ended launch's. Dropped, it gives up the reservation unless the launch has started
/// ([`ProcessLedger::launch_started`]); what was found carrying it by then is stopped as an ended
/// launch's.
pub struct PendingLaunch {
    ledger: Arc<ProcessLedger>,
    tag: ProcessTag,
}

impl PendingLaunch {
    pub fn tag(&self) -> &ProcessTag {
        &self.tag
    }
}

impl Drop for PendingLaunch {
    fn drop(&mut self) {
        let mut book = self.ledger.book();
        if book
            .launches
            .get(&self.tag.launch)
            .is_some_and(|launch| !launch.started)
        {
            book.hand_to_ended(&self.tag.launch);
        }
    }
}

/// How long a process must have been running for an environment read without a tag to be kept:
/// between its fork and its `exec`, a process shows its parent's environment under the identity
/// it keeps across the `exec`.
const SETTLED: Duration = Duration::from_secs(10);

#[cfg(test)]
thread_local! {
    /// [`SETTLED`] for a test that cannot wait that long, on the test's own thread.
    static SETTLED_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

fn settled() -> Duration {
    #[cfg(test)]
    if let Some(settled) = SETTLED_OVERRIDE.get() {
        return settled;
    }
    SETTLED
}

#[cfg(test)]
thread_local! {
    /// What [`boot_id`] answers for a test, on the test's own thread: `Some(None)` for a system
    /// that has no boot id.
    static BOOT_ID_OVERRIDE: std::cell::RefCell<Option<Option<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// The boot of the system the ledger file is written in (`table::boot_id`).
fn boot_id() -> Option<String> {
    #[cfg(test)]
    if let Some(id) = BOOT_ID_OVERRIDE.with_borrow(Clone::clone) {
        return id;
    }
    table::boot_id()
}

/// What the scans keep between reads, held for the whole of one so two never interleave.
#[derive(Default)]
struct Scans {
    /// The tag each process's environment was read to carry, by identity. One that found no tag
    /// in a process younger than [`SETTLED`] is not kept.
    tags: HashMap<Identity, EnvTag>,
    /// The processes whose environment could not be read, with the program each ran then: read
    /// again only once it runs another, since a process that execs can become readable.
    unreadable: HashMap<Identity, String>,
    /// Since when each attributed process has been seen working in a removed directory.
    missing_since: HashMap<Identity, Instant>,
    /// How many times each process's environment was read.
    #[cfg(test)]
    env_reads: HashMap<Identity, usize>,
}

/// Each listed tree top's members and their descendants seen by later scans, while they run
/// (see [`trees`]).
type Listings = HashMap<Identity, HashSet<Identity>>;

pub struct ProcessLedger {
    instance: String,
    /// The daemon's own pid, which with its ancestors and direct children is never attributed.
    own: i32,
    file: PathBuf,
    book: Mutex<Book>,
    /// Notified whenever a launch leaves the book.
    launch_left: Condvar,
    scans: Mutex<Scans>,
    /// Held across a write of the ledger file; when the last one was made.
    writes: Mutex<Option<Instant>>,
    /// Set once the scanner thread is started.
    scanner: OnceLock<()>,
    /// The reconcile was asked for, and so is never asked for again.
    reconcile_asked: AtomicBool,
    /// The reconcile was asked for and has not run yet; the scanner thread runs it.
    reconcile_pending: AtomicBool,
    /// The daemon is shutting down: nothing is carried over any more ([`Self::close`]).
    closing: AtomicBool,
    listings: Mutex<Listings>,
    /// The trees last listed for a client, by what was listed, with when (see
    /// [`Self::listed_lately`]).
    lists: Mutex<HashMap<String, (Instant, Vec<ProcessTree>)>>,
    /// The processes found running after a `SIGKILL`, with when that was last tried, or `None`
    /// once they are to be tried again at the next tick.
    stubborn: Mutex<HashMap<Identity, Option<Instant>>>,
    /// How many of the next reads of the table fail, for a test.
    #[cfg(test)]
    failing_reads: std::sync::atomic::AtomicUsize,
    /// [`CWD_GRACE`] for a test that cannot wait that long.
    #[cfg(test)]
    cwd_grace: Mutex<Option<Duration>>,
}

impl ProcessLedger {
    /// A ledger for the daemon of the data directory `dir`, which keeps its instance id and its
    /// file there. Nothing is read from the process table until a launch starts or
    /// [`Self::start`] runs.
    pub fn new(dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            instance: persist::instance_id(dir),
            own: std::process::id() as i32,
            file: dir.join(persist::LEDGER_FILE),
            book: Mutex::new(Book::default()),
            launch_left: Condvar::new(),
            scans: Mutex::new(Scans::default()),
            writes: Mutex::new(None),
            scanner: OnceLock::new(),
            reconcile_asked: AtomicBool::new(false),
            reconcile_pending: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            listings: Mutex::new(Listings::new()),
            lists: Mutex::new(HashMap::new()),
            stubborn: Mutex::new(HashMap::new()),
            #[cfg(test)]
            failing_reads: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            cwd_grace: Mutex::new(None),
        })
    }

    /// Keeps the scanner thread from ever starting, so that the table is read only when a test
    /// says so. A test's ledger follows only that test's stand-ins, and a thread of its own
    /// reading the whole table every [`SCAN_INTERVAL`] for every test would load the suite for
    /// nothing.
    #[cfg(test)]
    pub(crate) fn without_scanner(&self) {
        self.scanner.set(()).ok();
    }

    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().expect("process ledger lock poisoned")
    }

    fn cwd_grace(&self) -> Duration {
        #[cfg(test)]
        if let Some(grace) = *self.cwd_grace.lock().expect("cwd grace lock poisoned") {
            return grace;
        }
        CWD_GRACE
    }

    /// Starts the scanner thread unless it runs already, and with `reconcile` has it stop what a
    /// previous run left, once, whenever the thread was started. The daemon calls it at startup
    /// with `reconcile`; a launch starting calls it without.
    pub fn start(self: &Arc<Self>, reconcile: bool) {
        if reconcile && !self.reconcile_asked.swap(true, Ordering::AcqRel) {
            self.reconcile_pending.store(true, Ordering::Release);
        }
        self.scanner.get_or_init(|| {
            let ledger = Arc::downgrade(self);
            let spawned = std::thread::Builder::new()
                .name("process-ledger".to_string())
                .spawn(move || run_scanner(ledger));
            if let Err(err) = spawned {
                tracing::warn!(%err, "starting the process ledger's thread failed; processes agents leave running are stopped only when their launch ends");
            }
        });
    }

    /// Reserves a launch id for the session `session_id`'s launch about to start, for its agent's
    /// environment.
    pub fn begin_launch(self: &Arc<Self>, session_id: &str) -> PendingLaunch {
        let launch = uuid::Uuid::new_v4().to_string();
        self.book().launches.insert(
            launch.clone(),
            Launch {
                session_id: Some(session_id.to_string()),
                ..Launch::default()
            },
        );
        PendingLaunch {
            ledger: self.clone(),
            tag: ProcessTag {
                instance: self.instance.clone(),
                launch,
            },
        }
    }

    /// Follows the launch `launch_id` of the session `session_id`, whose agent process `agent_pid`
    /// has just been started and is not reaped yet. A launch id not reserved before is followed
    /// all the same.
    pub fn launch_started(self: &Arc<Self>, launch_id: &str, session_id: &str, agent_pid: u32) {
        let pid = agent_pid as i32;
        let agent = table::start_of(pid).map(|start| Identity { pid, start });
        let mut book = self.book();
        // A scan since the launch id was reserved may have found the agent, which carries the
        // tag, and what runs in its terminal session, and taken them for the launch's own.
        book.attributed
            .retain(|id, attributed| id.pid != pid && attributed.sid != pid);
        for launch in book.launches.values_mut() {
            launch.sessions.remove(&pid);
        }
        let launch = book.launches.entry(launch_id.to_string()).or_default();
        launch.session_id = Some(session_id.to_string());
        launch.started = true;
        launch.agent = agent;
        if let Some(agent) = agent {
            book.horizon = Some(book.horizon.map_or(agent.start, |at| at.min(agent.start)));
        }
        // What an earlier launch of the session carried over while none was running.
        let held: Vec<String> = book
            .launches
            .iter()
            .filter(|(id, launch)| {
                launch.held
                    && launch.session_id.as_deref() == Some(session_id)
                    && id.as_str() != launch_id
            })
            .map(|(id, _)| id.clone())
            .collect();
        for from in &held {
            book.transfer(from, launch_id, None);
        }
        book.dirty = true;
        drop(book);
        if !held.is_empty() {
            self.forget_lists();
        }
        self.start(false);
    }

    /// Reads the table once, for the daemon about to end an agent: what the agent started a
    /// moment ago, which no scan has seen yet, is attributed while its chain through the agent
    /// still shows. **Blocks.**
    pub fn scan_before_ending(&self) {
        self.scan(false);
    }

    /// Stops everything attributed to the launch `launch_id`, whose agent process has ended, and
    /// stops following it. Reads the table again a few times, so a process started meanwhile is
    /// caught; what is still running after that is held for an ended launch, which the scanner
    /// stops. A second call for the same launch waits for the first to finish. **Blocks**, for a
    /// few seconds when there is anything to stop.
    ///
    /// A launch marked to be carried over ([`Self::hand_over`]) is not stopped: its processes go
    /// to the running launch of the session it was marked for, or, while that session has none
    /// and a hold for it is open ([`Self::hold_for_launch`]), are held for its next one. With
    /// neither, or once the daemon is shutting down ([`Self::close`]), they are stopped. So is a
    /// launch of a session a hold is open for: the relaunch of a restart (an account switch, a
    /// model change, a change of sharing), whose end leaves its processes held for whatever the
    /// restart decides.
    pub fn end_launch(&self, launch_id: &str) {
        if launch_id == ENDED {
            return;
        }
        let heir = {
            let mut book = self.book();
            loop {
                match book.launches.get_mut(launch_id) {
                    None => return,
                    // Carried over by an end before this one.
                    Some(launch) if launch.held => return,
                    Some(launch) if launch.ending => {
                        book = self
                            .launch_left
                            .wait(book)
                            .expect("process ledger lock poisoned");
                    }
                    Some(launch) => {
                        launch.ending = true;
                        break;
                    }
                }
            }
            book.heir_of(launch_id).is_some()
        };
        // The scan attributes what the agent started since the last one, so it goes along.
        if heir && !self.closing.load(Ordering::Acquire) {
            self.scan(false);
            if self.carry_over(launch_id) {
                return;
            }
        }
        self.stop_launch(launch_id);
    }

    /// Stops what is attributed to the launch `launch_id`, which is marked ending, and stops
    /// following it (see [`Self::end_launch`]). **Blocks.**
    fn stop_launch(&self, launch_id: &str) {
        // Takes the launch out of the book however this ends, a panic included, so a second call
        // waiting on it is let go.
        let _forget = ForgetLaunch {
            ledger: self,
            launch_id,
        };
        // A read after the last round of signals too, which drops the sessions they emptied
        // before the file is written.
        for pass in 0..=STOP_PASSES {
            let Some(table) = self.scan(false) else {
                break;
            };
            // Read just before the signals, and never a kept process: a keep is refused once
            // the launch is ending, and a kept process is attributed to no launch.
            let members = self.book().members_of(launch_id, &table);
            if members.is_empty() || pass == STOP_PASSES {
                break;
            }
            if pass == 0 {
                tracing::info!(
                    launch = launch_id,
                    count = members.len(),
                    "stopping what an ended launch left running"
                );
            }
            self.signal(&members);
        }
    }

    /// Stops what a previous run of this daemon's data directory left: every process its ledger
    /// file names that is still running with the same start time, what sits in the terminal
    /// sessions it recorded that are proven still the same (see `attribute.rs`), and every process
    /// carrying a tag of this instance whose launch is not one being followed now. Kept processes,
    /// their descendants and what is in their sessions are left alone. A file written in another
    /// boot of the system names nothing. When the file cannot be read, or either its boot or this
    /// run's is not known where the other is, what it kept is not known, so every process only a
    /// tag would have stopped is kept instead, and written down as kept. **Blocks.**
    pub fn reconcile(&self) {
        let (persisted, readable) = match persist::load(&self.file) {
            persist::Loaded::Missing => (persist::Persisted::default(), true),
            persist::Loaded::Read(persisted) => match (&persisted.boot, boot_id()) {
                (written, current) if *written == current => (persisted, true),
                // One of the two boots is not known: the file was written before it recorded a
                // boot, or this run cannot read one (a restricted `/proc`). Whether its start
                // times compare with this run's cannot be told, so no process it kept can be
                // recognized.
                (None, Some(_)) | (Some(_), None) => {
                    tracing::info!(
                        written = persisted.boot.is_some(),
                        "the boot of the system the process ledger file was written in, or of this run's, is not known, so the processes it names cannot be recognized; what it kept is not known"
                    );
                    (persist::Persisted::default(), false)
                }
                // Nothing a file of another boot names is running, and on Linux its start times
                // count from that boot, so they could match this one's.
                _ => {
                    tracing::info!(
                        "the process ledger file was written in another boot of the system; nothing it names is running"
                    );
                    (persist::Persisted::default(), true)
                }
            },
            persist::Loaded::Unreadable => (persist::Persisted::default(), false),
        };
        // Into the book before anything that can fail: what the user kept must never be taken
        // for an ended launch's by a later scan. The first scan forgets what is gone, and drops
        // what would never be attributed and the sessions it cannot prove are the ones recorded.
        let before: HashSet<Identity> = {
            let mut book = self.book();
            book.kept.extend(persisted.kept.iter().copied());
            let sessions = |sessions: &[persist::PersistedSession]| {
                sessions
                    .iter()
                    .map(|session| (session.sid, session.leader_start))
                    .collect::<Vec<_>>()
            };
            book.read_back_sessions(
                sessions(&persisted.sessions),
                sessions(&persisted.kept_sessions),
                persisted
                    .attributed
                    .iter()
                    .map(persist::PersistedProcess::identity)
                    .chain(persisted.kept.iter().copied()),
            );
            for kept in &persisted.kept_projects {
                let id = Identity {
                    pid: kept.pid,
                    start: kept.start,
                };
                if book.kept.contains(&id) {
                    book.kept_projects
                        .entry(id)
                        .or_insert_with(|| kept.project.clone());
                }
            }
            for session in &persisted.kept_session_projects {
                if book.kept_sessions.contains_key(&session.sid) {
                    book.kept_session_projects
                        .entry(session.sid)
                        .or_insert_with(|| session.project.clone());
                }
            }
            for process in &persisted.attributed {
                let id = process.identity();
                if !book.kept.contains(&id) {
                    book.attributed.entry(id).or_insert(Attributed {
                        launch: ENDED.to_string(),
                        sid: process.sid,
                    });
                }
            }
            book.dirty = true;
            book.attributed
                .iter()
                .filter(|(_, attributed)| attributed.launch == ENDED)
                .map(|(id, _)| *id)
                .collect()
        };
        let Some(table) = self.scan(true) else {
            tracing::warn!("listing processes failed; what a previous run left is not stopped");
            return;
        };
        if readable {
            let members = self.book().members_of(ENDED, &table);
            if !members.is_empty() {
                tracing::info!(
                    count = members.len(),
                    "stopping what a previous run's agents left running"
                );
                self.stop_set(members, &table);
            }
        } else {
            let mut book = self.book();
            let unknown: Vec<Identity> = book
                .members_of(ENDED, &table)
                .into_iter()
                .filter(|id| !before.contains(id))
                .collect();
            for id in &unknown {
                book.attributed.remove(id);
                book.kept.insert(*id);
                if let Some(proc) = table.get(id.pid) {
                    attribute::record_kept_session(&mut book, &table, proc);
                }
            }
            let kept_sessions: HashSet<i32> = book.kept_sessions.keys().copied().collect();
            if let Some(ended) = book.launches.get_mut(ENDED) {
                ended.sessions.retain(|sid, _| !kept_sessions.contains(sid));
            }
            if !unknown.is_empty() {
                tracing::info!(
                    count = unknown.len(),
                    "keeping what a previous run's agents left running, since what it kept is not known"
                );
            }
        }
        self.persist(true);
    }

    /// Reads the table and brings the book up to date with it. A `full` scan may read the
    /// environment of every process; any other only of processes started since the earliest
    /// agent this run followed. `None` when the table cannot be read.
    fn scan(&self, full: bool) -> Option<Table> {
        let mut scans = self
            .scans
            .lock()
            .expect("process ledger scans lock poisoned");
        let Some(table) = self.read_table() else {
            tracing::warn!("listing processes failed; the process ledger is not up to date");
            return None;
        };
        scans.tags.retain(|id, _| table.has(id));
        scans.unreadable.retain(|id, _| table.has(id));
        // Which environments the attribution may ask for is decided under the book's lock, with
        // no system call; they are read outside it, so that a launch being registered on the
        // runtime never waits behind them.
        let (horizon, wanted) = {
            let mut book = self.book();
            // What a session read back would decide is decided first, as `attribute` does.
            attribute::prove_read_back(&mut book, &table);
            let horizon = if full { Some(0) } else { book.horizon };
            (
                horizon,
                attribute::tag_candidates(&book, &table, self.own, horizon),
            )
        };
        let settled_before = table::now().saturating_sub(settled().as_micros() as u64);
        let mut read: HashMap<Identity, EnvTag> = HashMap::new();
        for proc in wanted {
            if scans.tags.contains_key(&proc.id)
                || scans.unreadable.get(&proc.id) == Some(&proc.comm)
            {
                continue;
            }
            let tag = table::env_tag(proc.id);
            #[cfg(test)]
            {
                *scans.env_reads.entry(proc.id).or_default() += 1;
            }
            match tag {
                EnvTag::Unreadable => {
                    scans.unreadable.insert(proc.id, proc.comm.clone());
                }
                EnvTag::Untagged if proc.id.start >= settled_before => {}
                _ => {
                    scans.unreadable.remove(&proc.id);
                    scans.tags.insert(proc.id, tag.clone());
                }
            }
            read.insert(proc.id, tag);
        }
        let mut book = self.book();
        let instance = &self.instance;
        let tags = &scans.tags;
        let mut tag_of = |proc: &Proc| {
            if horizon.is_none_or(|horizon| proc.id.start < horizon) {
                return None;
            }
            match read.get(&proc.id).or_else(|| tags.get(&proc.id)) {
                Some(EnvTag::Tagged {
                    instance: theirs,
                    launch,
                }) if theirs == instance => Some(launch.clone()),
                _ => None,
            }
        };
        attribute::attribute(&mut book, &table, self.own, &mut tag_of);
        drop(book);
        self.advance_listings(&table);
        Some(table)
    }

    fn read_table(&self) -> Option<Table> {
        #[cfg(test)]
        if self
            .failing_reads
            .try_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            return None;
        }
        Table::read()
    }

    /// One round of signals to `targets` (see [`stop::stop`]): a process kept meanwhile is not
    /// killed, and one still running after its `SIGKILL` is left for [`STUBBORN_RETRY`], with one
    /// warning. **Blocks.**
    fn signal(&self, targets: &[Identity]) {
        let survivors = stop::stop(targets, &|id| self.book().kept.contains(id));
        let mut stubborn = self
            .stubborn
            .lock()
            .expect("stubborn processes lock poisoned");
        let now = Instant::now();
        for id in survivors {
            if stubborn.insert(id, Some(now)).is_none() {
                tracing::warn!(
                    pid = id.pid,
                    "a process was still running after SIGKILL; it is tried again only after a while"
                );
            }
        }
    }

    /// One turn of the scanner thread, for a test outside the ledger, whose ledger has none.
    #[cfg(test)]
    pub(crate) fn tick_now(&self) {
        self.tick();
    }

    /// One turn of the scanner thread: reads the table while there is anything to follow, stops
    /// what ended launches left and what has worked in a removed directory for long enough, and
    /// writes the ledger file when it changed.
    fn tick(&self) {
        let idle = {
            let book = self.book();
            book.launches.len() <= 1 && book.attributed.is_empty()
        };
        if !idle {
            if let Some(table) = self.scan(false) {
                let removed: HashSet<Identity> = self
                    .worked_in_removed_directories(&table)
                    .into_iter()
                    .collect();
                let mut doomed: HashSet<Identity> =
                    self.book().members_of(ENDED, &table).into_iter().collect();
                doomed.extend(&removed);
                {
                    let mut stubborn = self
                        .stubborn
                        .lock()
                        .expect("stubborn processes lock poisoned");
                    stubborn.retain(|id, _| table.has(id));
                    doomed.retain(|id| match stubborn.get(id) {
                        Some(Some(since)) => since.elapsed() >= STUBBORN_RETRY,
                        _ => true,
                    });
                }
                let removed_count = doomed.intersection(&removed).count();
                if removed_count != 0 {
                    tracing::info!(
                        count = removed_count,
                        "stopping processes whose working directory was removed"
                    );
                }
                if !doomed.is_empty() {
                    self.stop_set(doomed.into_iter().collect(), &table);
                }
            }
        }
        self.persist(false);
    }

    /// The attributed processes of launches being followed that have worked in a removed
    /// directory for [`CWD_GRACE`], with their descendants of the same launch.
    fn worked_in_removed_directories(&self, table: &Table) -> Vec<Identity> {
        let grace = self.cwd_grace();
        let attributed: Vec<(Identity, String)> = self
            .book()
            .attributed
            .iter()
            .filter(|(id, attributed)| attributed.launch != ENDED && table.has(id))
            .map(|(id, attributed)| (*id, attributed.launch.clone()))
            .collect();
        // Read before the lock is taken: every read is a system call or two, and the lock is held
        // for the whole of a scan.
        let missing: Vec<bool> = attributed
            .iter()
            .map(|(id, _)| table::cwd(*id).is_some_and(|cwd| cwd.missing))
            .collect();
        let mut scans = self
            .scans
            .lock()
            .expect("process ledger scans lock poisoned");
        scans.missing_since.retain(|id, _| table.has(id));
        let now = Instant::now();
        let mut expired = Vec::new();
        for ((id, launch), missing) in attributed.iter().zip(missing) {
            if !missing {
                scans.missing_since.remove(id);
                continue;
            }
            let since = *scans.missing_since.entry(*id).or_insert(now);
            if now.duration_since(since) >= grace {
                expired.push((*id, launch));
            }
        }
        drop(scans);
        let mut doomed: HashSet<Identity> = HashSet::new();
        for (top, launch) in expired {
            doomed.insert(top);
            for (id, of) in &attributed {
                let below = table
                    .get(id.pid)
                    .is_some_and(|proc| table.ancestors(proc).any(|ancestor| ancestor.id == top));
                if of == launch && below {
                    doomed.insert(*id);
                }
            }
        }
        doomed.into_iter().collect()
    }

    /// Stops `targets`, read in `before`, then, a few times, what started meanwhile that is
    /// attributed to the same launch as a target and descends from one: through its chain of
    /// parents, or by sitting in a terminal session a target led. A kept process is left out
    /// just before each round of signals. **Blocks.**
    fn stop_set(&self, mut targets: Vec<Identity>, before: &Table) {
        let launches: HashSet<String> = {
            let book = self.book();
            targets
                .iter()
                .filter_map(|id| {
                    book.attributed
                        .get(id)
                        .map(|attributed| attributed.launch.clone())
                })
                .collect()
        };
        let mut stopped: HashSet<Identity> = HashSet::new();
        let mut led: HashSet<i32> = HashSet::new();
        let mut known: HashSet<Identity> = before.procs().map(|proc| proc.id).collect();
        let mut current = Cow::Borrowed(before);
        for _ in 0..STOP_PASSES {
            {
                let book = self.book();
                targets.retain(|id| !book.kept.contains(id));
            }
            if targets.is_empty() {
                return;
            }
            led.extend(
                targets
                    .iter()
                    .filter(|id| {
                        current
                            .get(id.pid)
                            .is_some_and(|proc| proc.id == **id && proc.sid == id.pid)
                    })
                    .map(|id| id.pid),
            );
            self.signal(&targets);
            stopped.extend(targets.iter().copied());
            let Some(table) = self.scan(false) else {
                return;
            };
            targets = follow_ups(&self.book(), &table, &known, &stopped, &led, &launches);
            known.extend(table.procs().map(|proc| proc.id));
            current = Cow::Owned(table);
        }
    }

    /// Writes the ledger file if what a restart needs changed since it was last written; unless
    /// `now`, not sooner than [`WRITE_DEBOUNCE`] after the last write.
    fn persist(&self, now: bool) {
        let mut last = self
            .writes
            .lock()
            .expect("process ledger write lock poisoned");
        if !now && last.is_some_and(|at| at.elapsed() < WRITE_DEBOUNCE) {
            return;
        }
        let persisted = {
            let mut book = self.book();
            if !book.dirty {
                return;
            }
            book.dirty = false;
            persist::Persisted {
                boot: boot_id(),
                attributed: book
                    .attributed
                    .iter()
                    .map(|(id, attributed)| persist::PersistedProcess {
                        pid: id.pid,
                        start: id.start,
                        launch: attributed.launch.clone(),
                        sid: attributed.sid,
                    })
                    .collect(),
                kept: book.kept.iter().copied().collect(),
                kept_sessions: book
                    .kept_sessions
                    .iter()
                    .map(|(&sid, &leader_start)| persist::PersistedSession { sid, leader_start })
                    .collect(),
                sessions: book
                    .launches
                    .values()
                    .flat_map(|launch| launch.sessions.iter())
                    .map(|(&sid, &leader_start)| persist::PersistedSession { sid, leader_start })
                    .collect(),
                kept_projects: book
                    .kept_projects
                    .iter()
                    .map(|(id, project)| persist::PersistedKeptProject {
                        pid: id.pid,
                        start: id.start,
                        project: project.clone(),
                    })
                    .collect(),
                kept_session_projects: book
                    .kept_session_projects
                    .iter()
                    .map(|(&sid, project)| persist::PersistedSessionProject {
                        sid,
                        project: project.clone(),
                    })
                    .collect(),
            }
        };
        match persist::save(&self.file, &persisted) {
            Ok(()) => *last = Some(Instant::now()),
            Err(err) => {
                tracing::warn!(path = %self.file.display(), %err, "writing the process ledger file failed");
                self.book().dirty = true;
            }
        }
    }
}

/// What a stop goes on to after a round of signals: the processes of `table` not `known` before
/// that are attributed to one of `launches` and descend from what was `stopped`, through their
/// chain of parents or by sitting in a terminal session one of those `led`.
fn follow_ups(
    book: &Book,
    table: &Table,
    known: &HashSet<Identity>,
    stopped: &HashSet<Identity>,
    led: &HashSet<i32>,
    launches: &HashSet<String>,
) -> Vec<Identity> {
    table
        .procs()
        .filter(|proc| {
            !known.contains(&proc.id)
                && book
                    .attributed
                    .get(&proc.id)
                    .is_some_and(|attributed| launches.contains(&attributed.launch))
        })
        .filter(|proc| {
            led.contains(&proc.sid)
                || table
                    .ancestors(proc)
                    .any(|ancestor| stopped.contains(&ancestor.id))
        })
        .map(|proc| proc.id)
        .collect()
}

/// Takes a launch being ended out of the book when dropped, handing what is still attributed to
/// it to [`ENDED`], writes the ledger file, and lets a second end of it waiting go.
struct ForgetLaunch<'a> {
    ledger: &'a ProcessLedger,
    launch_id: &'a str,
}

impl Drop for ForgetLaunch<'_> {
    fn drop(&mut self) {
        {
            let mut book = self.ledger.book();
            book.hand_to_ended(self.launch_id);
            book.dirty = true;
        }
        // What outlived a `SIGKILL` is tried again at the next tick.
        if let Ok(mut stubborn) = self.ledger.stubborn.lock() {
            for since in stubborn.values_mut() {
                *since = None;
            }
        }
        self.ledger.forget_lists();
        self.ledger.launch_left.notify_all();
        self.ledger.persist(true);
    }
}

/// The scanner thread: reconciles when asked, and ticks every [`SCAN_INTERVAL`] for as long as
/// the ledger exists.
fn run_scanner(ledger: Weak<ProcessLedger>) {
    loop {
        {
            let Some(ledger) = ledger.upgrade() else {
                return;
            };
            if ledger.reconcile_pending.swap(false, Ordering::AcqRel) {
                ledger.reconcile();
            }
        }
        std::thread::sleep(SCAN_INTERVAL);
        let Some(ledger) = ledger.upgrade() else {
            return;
        };
        ledger.tick();
    }
}
