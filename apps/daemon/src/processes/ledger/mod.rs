//! The processes each agent launch started, wherever they went, and stopping them when the launch
//! ends. Agent CLIs run every shell command in a terminal session of its own (`setsid`), so what a
//! command leaves running — a dev server, a preview, a watcher — is out of reach of the sweep of the
//! agent's own session (`crate::platform::proc`) and, once its shell exits, reparented away from
//! the agent too. The ledger follows those processes itself, from the process table alone, without
//! the agent having to do or remember anything; and it never stops a process it cannot attribute
//! to the launch being ended.
//!
//! Every launch is tagged ([`ProcessTag`]: [`TAG_VARIABLE`] in the agent's environment, which its
//! descendants inherit), and a dedicated thread reads the process table every
//! [`SCAN_INTERVAL`] while there is anything to follow, attributing processes to launches by the
//! rules in `attribute.rs`. A process is told by its pid and start time ([`crate::platform::proc::ProcessIdentity`]), and
//! every signal checks the start time again just before it is sent.
//!
//! A launch whose agent a starting daemon adopts is followed from the start, before the reconcile,
//! so what the previous run recorded for it stays its own and is stopped with it, not as an ended
//! launch's. Until the reconcile has read the previous run's file
//! ([`ProcessLedger::await_reconcile`]), the scanner stops nothing and nothing is written, and the
//! stop of a launch that ends meanwhile waits for it, so that file is read as it was left.
//!
//! The processes of a launch are stopped when its agent process ends ([`ProcessLedger::end_launch`],
//! called before the session reads as stopped, and so before an archive removes its worktree), when
//! one of them has worked in a removed directory for [`CWD_GRACE`], and, after a restart or crash
//! of the daemon, by [`ProcessLedger::reconcile`] from what the previous run wrote down. What the
//! user chose to keep is never stopped, and neither is the daemon, one of its ancestors, a holder
//! process (see [`crate::holder`]) or a direct child of the daemon or of a holder.
//!
//! Where the host keeps a launch's processes in a container the ledger can end as a whole (on
//! Linux, a cgroup v2 group, when the daemon's cgroup lets it make one), a launch's containment
//! lists its members, which a scan attributes to it, and the end of the launch kills what is still
//! in it once the rounds of signals are through, and removes it: nothing a launch started escapes
//! it, however it detached. Kept processes, and those moved to another launch, are moved out of
//! it when kept or moved, and again just before it is ended.
//!
//! A launch's processes can change hands instead ([`attribute::Book::transfer`]): all of them when
//! its agent ends after a handoff, an account switch, a model change or a change of an account's
//! sharing marked it to be carried over ([`ProcessLedger::mark_carry_over`]), to the launch of the
//! session that takes its place or to the restarted session's relaunch, and chosen trees of a
//! running launch at an owner's word ([`ProcessLedger::move_trees`]). The launch that took them
//! stops them when it ends. Carried over before the receiving session has a launch running, they
//! are held for its next one for as long as the caller holds the session open for it
//! ([`ProcessLedger::hold_for_launch`]), and stopped once nothing does.
//!
//! What it cannot do: a helper a kept process starts that leaves the kept process's terminal
//! session (`setsid`) and its parent (a double fork) between two scans is reached neither by the
//! chain nor by the session, and still carries the launch's tag; it is stopped with the launch.
//! A process whose environment cannot be read (an Apple platform binary on macOS with SIP on) that
//! a shell left running in the background is never attributed when that shell exits before a scan
//! has seen it: its chain no longer reaches the launch, the shell's session was never recorded, and
//! there is no tag to read. It is neither listed nor stopped with the launch.
//! A process that outlives `SIGKILL` (one in an uninterruptible sleep), or that the system refuses
//! to end (one running with more rights than the daemon), is tried again only after
//! [`STUBBORN_RETRY`] or once another launch ends.

mod attribute;
mod carry;
mod looks;
mod persist;
mod scan;
mod stop;
mod table;
#[cfg(test)]
mod tests;
mod trees;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use attribute::{Book, Launch, ENDED};
use table::{EnvTag, Identity};

use crate::platform::proc::{ContainerId, Containment, ProcessId};
use crate::protocol::{ProcessCount, ProcessTree};

pub use crate::platform::proc::TAG_VARIABLE;
pub use carry::HeldForLaunch;
#[cfg(test)]
pub(crate) use trees::AGENT_CONSOLE_OVERRIDE;
pub use trees::{MoveRefused, SharesAgentConsole, TreesStopped};

/// How often the table is read while there is anything to follow.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);

/// How long a process may work in a removed directory before it is stopped.
const CWD_GRACE: Duration = Duration::from_secs(60);

/// How many times a stop reads the table again for what started while it was stopping.
const STOP_PASSES: usize = 3;

/// The least time between two writes of the ledger file that a scan asks for. The end of a
/// launch, a keep and the reconcile at start write at once.
const WRITE_DEBOUNCE: Duration = Duration::from_secs(10);

/// How long a process found running after a `SIGKILL`, or that the system refused to kill, is
/// left before it is tried again, unless a launch ends first.
const STUBBORN_RETRY: Duration = Duration::from_secs(5 * 60);

/// Takes [`TAG_VARIABLE`] out of the daemon's own environment. A daemon started from an agent's
/// shell (a development daemon run from a worktree) carries that launch's tag, which every process
/// it starts would inherit. Changing the environment races with any other thread reading it, so
/// this is called while the daemon has no other thread yet.
pub fn forget_inherited_tag() {
    std::env::remove_var(TAG_VARIABLE);
}

/// What tells one launch's processes from any other's: the tag they carry in their environment,
/// as [`TAG_VARIABLE`], and the containment the host keeps them in where it lists their members.
#[derive(Debug, Clone)]
pub struct ProcessTag {
    instance: String,
    launch: String,
    containment: Arc<Containment>,
}

impl ProcessTag {
    pub fn launch_id(&self) -> &str {
        &self.launch
    }

    /// The id of the daemon instance the launch belongs to, which names its containment together
    /// with [`Self::launch_id`].
    pub fn instance_id(&self) -> &str {
        &self.instance
    }

    /// What the launch's process is started in and taken into once started.
    pub fn containment(&self) -> &Arc<Containment> {
        &self.containment
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

/// Whether the stop of what the previous run left waits ([`ProcessLedger::hold_previous_run`]).
#[derive(Default)]
struct PreviousRun {
    /// The stop waits.
    holding: bool,
    /// What the reconcile found left by the previous run while the stop waited: neither these
    /// nor what descends from them is stopped until [`ProcessLedger::release_previous_run`].
    held: HashSet<Identity>,
    /// The launch the previous run's file attributed each process to, for a launch no one
    /// followed when the reconcile read it back (the file's launch, not [`ENDED`]): a launch
    /// followed later, its agent adopted from a holder linked to late, takes them back
    /// ([`ProcessLedger::take_back_previous`]).
    recorded: HashMap<Identity, String>,
    /// The terminal sessions the file recorded for such launches, with their leaders' start
    /// times, by launch.
    sessions: HashMap<String, Vec<(ContainerId, Option<u64>)>>,
    /// The processes taken back so far, with the launch each went to, so that at the release
    /// what descends from them goes along.
    taken_back: HashMap<Identity, String>,
}

pub struct ProcessLedger {
    instance: String,
    /// The daemon's own pid, which with its ancestors, its direct children, and the holders and
    /// their direct children is never attributed.
    own: ProcessId,
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
    /// The previous run's file has not been read back yet: no tick scans or stops anything and
    /// nothing is written until the reconcile has (see [`Self::await_reconcile`]).
    awaiting_reconcile: AtomicBool,
    /// The launches whose end came while the reconcile was awaited: marked ending, and stopped
    /// once the reconcile has read the file ([`Self::reconciled`]). Held across the check of
    /// `awaiting_reconcile` that queues one, and across the release that takes them.
    deferred_ends: Mutex<Vec<String>>,
    /// What the previous run left that is not stopped yet ([`Self::hold_previous_run`]).
    previous: Mutex<PreviousRun>,
    /// The daemon is shutting down: nothing is carried over any more ([`Self::close`]).
    closing: AtomicBool,
    listings: Mutex<Listings>,
    /// The trees last listed for a client, by what was listed, with when (see
    /// [`Self::listed_lately`]).
    lists: Mutex<HashMap<String, (Instant, Vec<ProcessTree>)>>,
    /// The counts last computed for a client, with when (see [`Self::counted_lately`]).
    counts: Mutex<Option<(Instant, Vec<ProcessCount>)>>,
    /// How many times what was listed and counted for clients was forgotten (see
    /// [`Self::lists_generation`]).
    lists_generation: std::sync::atomic::AtomicU64,
    /// Held while the counts are computed (see [`Self::claim_counting`]).
    counting: Arc<tokio::sync::Mutex<()>>,
    /// The looks at directories that outran their deadline (see `looks.rs`).
    looks: looks::Looks,
    /// The processes found running after a `SIGKILL`, with when that was last tried, or `None`
    /// once they are to be tried again at the next tick.
    stubborn: Mutex<HashMap<Identity, Option<Instant>>>,
    /// How many of the next reads of the table fail, for a test.
    #[cfg(test)]
    failing_reads: std::sync::atomic::AtomicUsize,
    /// [`CWD_GRACE`] for a test that cannot wait that long.
    #[cfg(test)]
    cwd_grace: Mutex<Option<Duration>>,
    /// How many times the counts were computed, for a test.
    #[cfg(test)]
    counts_computed: std::sync::atomic::AtomicUsize,
    /// What looks at a working directory in place of the file system, for a test.
    #[cfg(test)]
    cwd_look: Mutex<Option<CwdLook>>,
    /// What reads the system's report of a working directory in place of the system, for a test.
    #[cfg(test)]
    cwd_read: Mutex<Option<looks::CwdRead>>,
    /// Where the holders' registry files are: the processes of the holders, and of what they run,
    /// are the daemon's own to the ledger, as the daemon's direct children are.
    holders_dir: PathBuf,
}

/// A look at a working directory a test puts in place of the file system's.
#[cfg(test)]
type CwdLook = Arc<dyn Fn(&table::CwdReport) -> Option<table::Cwd> + Send + Sync>;

impl ProcessLedger {
    /// A ledger for the daemon of the data directory `dir`, which keeps its instance id and its
    /// file there. Nothing is read from the process table until a launch starts or
    /// [`Self::start`] runs.
    pub fn new(dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            instance: persist::instance_id(dir),
            own: ProcessId(std::process::id()),
            file: dir.join(persist::LEDGER_FILE),
            book: Mutex::new(Book::default()),
            launch_left: Condvar::new(),
            scans: Mutex::new(Scans::default()),
            writes: Mutex::new(None),
            scanner: OnceLock::new(),
            reconcile_asked: AtomicBool::new(false),
            reconcile_pending: AtomicBool::new(false),
            awaiting_reconcile: AtomicBool::new(false),
            deferred_ends: Mutex::new(Vec::new()),
            previous: Mutex::new(PreviousRun::default()),
            closing: AtomicBool::new(false),
            listings: Mutex::new(Listings::new()),
            lists: Mutex::new(HashMap::new()),
            counts: Mutex::new(None),
            lists_generation: std::sync::atomic::AtomicU64::new(0),
            counting: Arc::new(tokio::sync::Mutex::new(())),
            looks: looks::Looks::default(),
            stubborn: Mutex::new(HashMap::new()),
            #[cfg(test)]
            failing_reads: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            cwd_grace: Mutex::new(None),
            #[cfg(test)]
            counts_computed: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            cwd_look: Mutex::new(None),
            #[cfg(test)]
            cwd_read: Mutex::new(None),
            holders_dir: crate::paths::holders_dir_in(dir),
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

    /// How many times the counts were computed, for a test outside the ledger.
    #[cfg(test)]
    pub(crate) fn counts_computed(&self) -> usize {
        self.counts_computed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Holds every read of the table back until dropped, for a test outside the ledger that needs
    /// a computation to be under way.
    #[cfg(test)]
    pub(crate) fn hold_scans(&self) -> impl Drop + '_ {
        self.scans
            .lock()
            .expect("process ledger scans lock poisoned")
    }

    /// The session the book follows the launch `launch_id` for, if it follows it.
    #[cfg(test)]
    pub(crate) fn followed_session(&self, launch_id: &str) -> Option<String> {
        let book = self.book();
        let launch = book.followed_as(launch_id)?;
        book.launches.get(launch)?.session_id.clone()
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

    /// Keeps the ledger from scanning for stops and from writing its file until the reconcile has
    /// run, which [`Self::start`] with `reconcile` asks for. The daemon calls it before anything
    /// registers a launch, since a launch followed earlier than the reconcile starts the scanner
    /// thread, whose first tick would otherwise stop what the previous run's file says was kept
    /// and write the file over with a book that has not read it.
    pub fn await_reconcile(&self) {
        self.awaiting_reconcile.store(true, Ordering::Release);
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
                .spawn(move || scan::run_scanner(ledger));
            if let Err(err) = spawned {
                tracing::warn!(%err, "starting the process ledger's thread failed; processes agents leave running are stopped only when their launch ends");
                self.reconciled();
            }
        });
    }

    /// Reserves a launch id for the session `session_id`'s launch about to start, for its agent's
    /// environment, with the containment its processes are kept in.
    pub fn begin_launch(self: &Arc<Self>, session_id: &str) -> std::io::Result<PendingLaunch> {
        let launch = uuid::Uuid::new_v4().to_string();
        // Made under the lock, so that the reconcile, which takes in what an earlier run left
        // under the lock too, never finds this containment before the book has the launch.
        let mut book = self.book();
        let containment = Arc::new(Containment::for_launch(&self.instance, &launch)?);
        book.launches.insert(
            launch.clone(),
            Launch {
                session_id: Some(session_id.to_string()),
                containments: vec![containment.clone()],
                ..Launch::default()
            },
        );
        Ok(PendingLaunch {
            ledger: self.clone(),
            tag: ProcessTag {
                instance: self.instance.clone(),
                launch,
                containment,
            },
        })
    }

    /// Takes into the launch `launch_id`, which a daemon that has since gone started and whose
    /// agent a daemon adopts, the containment the host still keeps for it, where the host lists
    /// one's members. Done before [`Self::launch_started`] follows the launch, and before the
    /// reconcile reads the previous run's file back.
    pub fn containment_adopted(&self, launch_id: &str) {
        let Some(containment) = Containment::reopen(&self.instance, launch_id)
            .filter(|containment| containment.members().is_some())
        else {
            return;
        };
        let mut book = self.book();
        let launch = book.launches.entry(launch_id.to_string()).or_default();
        // Taken in already while its holder could not be linked to (`Self::launch_unlinked`).
        if launch.containments.is_empty() {
            launch.containments.push(Arc::new(containment));
        }
        self.take_back_previous(&mut book, launch_id);
    }

    /// Gives startup reconciliation the containment of a launch whose holder is known gone,
    /// even when the previous ledger recorded no attributed task in it. It belongs to the
    /// previous run's leftovers: an unreadable ledger must still protect possibly kept members.
    pub fn containment_orphaned(&self, launch_id: &str) {
        let mut book = self.book();
        if book.followed_as(launch_id).is_some() {
            return;
        }
        if let Some(containment) = Containment::reopen(&self.instance, launch_id)
            .filter(|containment| containment.members().is_some())
        {
            book.launches
                .get_mut(ENDED)
                .expect("the ended launch is always present")
                .containments
                .push(Arc::new(containment));
        }
    }

    /// Has the reconcile keep, rather than stop, what the previous run left: a holder the daemon
    /// cannot link to yet may run the agent of a launch no record places, which would otherwise
    /// read as ended. The previous run's file is read back and this run's launches are followed,
    /// scanned, ended and written down as ever; only what the reconcile finds left, and what
    /// descends from it, waits for [`Self::release_previous_run`]. Called before
    /// [`Self::start`] asks for the reconcile.
    pub fn hold_previous_run(&self) {
        self.previous_run().holding = true;
    }

    /// Lets what [`Self::hold_previous_run`] kept be stopped, by the next scan, as an ended
    /// launch's: no holder is left that may run it. What a launch followed now was recorded for
    /// goes to that launch instead, and so does what descends from what it took back, attributed
    /// to an ended launch while it was held. **Blocks** on a read of the process table, unless
    /// the reconcile has kept nothing yet, which then stops as ever.
    pub fn release_previous_run(&self) {
        {
            let mut previous = self.previous_run();
            if previous.held.is_empty() {
                *previous = PreviousRun::default();
                return;
            }
        }
        let table = self.read_table();
        let mut book = self.book();
        self.take_back_followed(&mut book);
        let mut previous = self.previous_run();
        if let Some(table) = table {
            let ended: Vec<Identity> = book
                .attributed
                .iter()
                .filter(|(_, attributed)| attributed.launch == ENDED)
                .map(|(id, _)| *id)
                .collect();
            for id in ended {
                let Some(proc) = table.get(id.pid).filter(|proc| proc.id == id) else {
                    continue;
                };
                let heir = table
                    .ancestors(proc)
                    .find_map(|ancestor| previous.taken_back.get(&ancestor.id).cloned());
                if let (Some(launch), Some(attributed)) = (heir, book.attributed.get_mut(&id)) {
                    attributed.launch = launch;
                    book.dirty = true;
                }
            }
        }
        *previous = PreviousRun::default();
    }

    /// [`Self::take_back_previous`] for every launch something held was recorded for that the
    /// book follows now. Called with the book's lock held.
    fn take_back_followed(&self, book: &mut Book) {
        let launches: HashSet<String> = self.previous_run().recorded.values().cloned().collect();
        for launch in launches {
            if let Some(followed) = book.followed_as(&launch).map(str::to_string) {
                self.take_back_previous_as(book, &launch, &followed);
            }
        }
    }

    /// Moves to the launch `launch_id`, which the book follows from now on, what the reconcile
    /// kept of the previous run as that launch's while the stop was held: the processes the
    /// previous run's file attributed to it, which the read-back gave an ended launch, and its
    /// terminal sessions. They stay held, with what descends from them, until
    /// [`Self::release_previous_run`]. Called with the book's lock held.
    fn take_back_previous(&self, book: &mut Book, launch_id: &str) {
        self.take_back_previous_as(book, launch_id, launch_id);
    }

    /// [`Self::take_back_previous`] of what was recorded for `recorded`, which the book follows
    /// as `followed` (the same, or the launch it was carried over to).
    fn take_back_previous_as(&self, book: &mut Book, recorded: &str, followed: &str) {
        let mut previous = self.previous_run();
        let ids: Vec<Identity> = previous
            .recorded
            .iter()
            .filter(|(_, launch)| launch.as_str() == recorded)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            previous.recorded.remove(&id);
            if let Some(attributed) = book.attributed.get_mut(&id) {
                if attributed.launch == ENDED {
                    attributed.launch = followed.to_string();
                    book.dirty = true;
                }
            }
            previous.taken_back.insert(id, followed.to_string());
        }
        for (sid, leader_start) in previous.sessions.remove(recorded).unwrap_or_default() {
            if let Some(ended) = book.launches.get_mut(ENDED) {
                ended.sessions.remove(&sid);
            }
            if let Some(launch) = book.launches.get_mut(followed) {
                launch.sessions.entry(sid).or_insert(leader_start);
                book.dirty = true;
            }
        }
    }

    fn previous_run(&self) -> MutexGuard<'_, PreviousRun> {
        self.previous
            .lock()
            .expect("process ledger previous run lock poisoned")
    }

    /// Takes out of `doomed` what the previous run left while its stop waits, and what descends
    /// from it in `table`.
    fn spare_previous_run(&self, doomed: &mut HashSet<Identity>, table: &table::Table) {
        let previous = self.previous_run();
        if previous.held.is_empty() {
            return;
        }
        doomed.retain(|id| {
            !previous.held.contains(id)
                && !table.get(id.pid).is_some_and(|proc| {
                    table
                        .ancestors(proc)
                        .any(|ancestor| previous.held.contains(&ancestor.id))
                })
        });
    }

    /// Follows the launch `launch_id` of the session `session_id`, whose agent a holder the daemon
    /// cannot link to yet runs, with the containment the host still keeps for it where the host
    /// lists one's members: what the previous run's file attributed to the launch stays the
    /// launch's rather than going to an ended launch, so the reconcile stops none of it. Once the
    /// holder is linked to and the agent adopted, [`Self::launch_started`] follows it on; once the
    /// holder is known to be gone, [`Self::end_launch`] ends it. Done before the reconcile reads
    /// the previous run's file back.
    pub fn launch_unlinked(&self, launch_id: &str, session_id: &str) {
        let containment = Containment::reopen(&self.instance, launch_id)
            .filter(|containment| containment.members().is_some())
            .map(Arc::new);
        let mut book = self.book();
        let launch = book.launches.entry(launch_id.to_string()).or_default();
        launch
            .session_id
            .get_or_insert_with(|| session_id.to_string());
        // Its agent runs: a process carrying its tag is the launch's, not one left by an end.
        launch.started = true;
        launch.containments.extend(containment);
    }

    /// Follows the launch `launch_id` of the session `session_id`, whose agent process `pid` has
    /// just been started and is not reaped yet. A launch id not reserved before is followed all
    /// the same.
    pub fn launch_started(self: &Arc<Self>, launch_id: &str, session_id: &str, pid: ProcessId) {
        let session = ContainerId(pid.0);
        let agent = table::start_of(pid).map(|start_micros| Identity { pid, start_micros });
        let mut book = self.book();
        // A scan since the launch id was reserved may have found the agent, which carries the
        // tag, and what runs in its terminal session, and taken them for the launch's own.
        book.attributed
            .retain(|id, attributed| id.pid != pid && attributed.sid != Some(session));
        for launch in book.launches.values_mut() {
            launch.sessions.remove(&session);
        }
        let launch = book.launches.entry(launch_id.to_string()).or_default();
        launch.session_id = Some(session_id.to_string());
        launch.started = true;
        launch.agent = agent;
        // An agent adopted from a holder linked to after the reconcile: what the previous run
        // recorded for its launch is its own again.
        self.take_back_previous(&mut book, launch_id);
        if let Some(agent) = agent {
            book.horizon = Some(
                book.horizon
                    .map_or(agent.start_micros, |at| at.min(agent.start_micros)),
            );
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
    /// A launch marked to be carried over ([`Self::mark_carry_over`]) is not stopped: its processes
    /// go to the running launch of the session it was marked for, or, while that session has none
    /// and a hold for it is open ([`Self::hold_for_launch`]), are held for its next one. With
    /// neither, or once the daemon is shutting down ([`Self::close`]), they are stopped. So is a
    /// launch of a session a hold is open for: the relaunch of a restart (an account switch, a
    /// model change, a change of sharing), whose end leaves its processes held for whatever the
    /// restart decides.
    pub fn end_launch(&self, launch_id: &str) {
        if launch_id == ENDED {
            return;
        }
        {
            let mut book = self.book();
            loop {
                match book.launches.get_mut(launch_id) {
                    None => return,
                    // Carried over by an end before this one.
                    Some(launch) if launch.held => return,
                    // Its end waits for the reconcile, which no caller is to wait for.
                    Some(launch) if launch.ending && self.end_is_deferred(launch_id) => return,
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
        }
        // Before the reconcile has read the previous run's file, a scan or a stop would decide
        // from a book that has not read it: the launch stays marked ending, and the reconcile
        // ends it once it is through.
        if self.defer_end(launch_id) {
            return;
        }
        self.end_marked_launch(launch_id);
    }

    /// The rest of [`Self::end_launch`], for a launch marked ending: its carry-over if it has an
    /// heir, else its stop. **Blocks.**
    fn end_marked_launch(&self, launch_id: &str) {
        let heir = self.book().heir_of(launch_id).is_some();
        // The scan attributes what the agent started since the last one, so it goes along.
        if heir && !self.closing.load(Ordering::Acquire) {
            self.scan(false);
            if self.carry_over(launch_id) {
                return;
            }
        }
        self.stop_launch(launch_id);
    }

    /// Queues the end of the launch `launch_id` for [`Self::reconciled`] if the reconcile has not
    /// read the previous run's file yet, and says whether it did.
    fn defer_end(&self, launch_id: &str) -> bool {
        {
            let mut deferred = self
                .deferred_ends
                .lock()
                .expect("process ledger deferred ends lock poisoned");
            if !self.awaiting_reconcile.load(Ordering::Acquire) {
                return false;
            }
            deferred.push(launch_id.to_string());
        }
        // A second call for the launch that is waiting on the first looks again. The book is
        // taken first, so that a call between its look and its wait is not missed.
        drop(self.book());
        self.launch_left.notify_all();
        true
    }

    /// Whether the end of the launch `launch_id` is queued for [`Self::reconciled`].
    fn end_is_deferred(&self, launch_id: &str) -> bool {
        self.deferred_ends
            .lock()
            .expect("process ledger deferred ends lock poisoned")
            .iter()
            .any(|queued| queued == launch_id)
    }

    /// The previous run's file has been read back, or never will be: the scanner may stop things
    /// and the ledger write its file, and the ends of the launches that came meanwhile are carried
    /// out. **Blocks**, for what they take.
    fn reconciled(&self) {
        let deferred = {
            let mut deferred = self
                .deferred_ends
                .lock()
                .expect("process ledger deferred ends lock poisoned");
            self.awaiting_reconcile.store(false, Ordering::Release);
            std::mem::take(&mut *deferred)
        };
        for launch_id in deferred {
            self.end_marked_launch(&launch_id);
        }
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
        let mut unread = false;
        for pass in 0..=STOP_PASSES {
            let Some(table) = self.scan(false) else {
                unread = true;
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
            self.signal_round(&members);
        }
        // With the rounds cut short by a table that could not be read, a kill of everything in
        // the containments would reach what was never asked to stop. They are left to the ended
        // launch, as what is attributed still is, which the scanner stops with the rounds.
        if unread {
            tracing::warn!(
                launch = launch_id,
                "the process table could not be read; the launch's containments are left to the scanner"
            );
            return;
        }
        self.end_containments(launch_id);
    }

    /// Ends the containments of the launch `launch_id`, which is being stopped, where the host
    /// ends one as a whole (on Linux, the launch's cgroup v2 group and the groups of the launches
    /// carried over to it): what is still in them once the rounds of signals are through, which no
    /// scan has attributed or which the rule for an agent's own terminal session leaves out, is
    /// killed, and the groups are removed. A kept process, and one moved to another launch, is
    /// moved out first, into that launch's group or into the cgroup the ended launch's group is in.
    /// **Blocks**, for a moment.
    fn end_containments(&self, launch_id: &str) {
        let (containments, spared) = {
            let book = self.book();
            let Some(launch) = book.launches.get(launch_id) else {
                return;
            };
            let mut spared: Vec<(Identity, Option<Arc<Containment>>)> =
                book.kept.iter().map(|id| (*id, None)).collect();
            spared.extend(
                book.attributed
                    .iter()
                    .filter(|(_, attributed)| {
                        attributed.launch != launch_id && attributed.launch != ENDED
                    })
                    .map(|(id, attributed)| {
                        let into = book
                            .launches
                            .get(&attributed.launch)
                            .and_then(|launch| launch.containments.first().cloned());
                        (*id, into)
                    }),
            );
            (launch.containments.clone(), spared)
        };
        let spared: Vec<(Identity, Option<&Containment>)> = spared
            .iter()
            .map(|(id, into)| (*id, into.as_deref()))
            .collect();
        for containment in containments {
            containment.end(&spared);
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
        // What the file recorded for a launch the read-back found no one following, read under the
        // same lock, so that a launch followed later (an agent adopted late) takes it back.
        let (before, recorded, sessions) = {
            let mut book = self.book();
            let before = persisted.read_back(&mut book);
            let unfollowed = |launch: &str| launch != ENDED && book.followed_as(launch).is_none();
            let recorded: HashMap<Identity, String> = persisted
                .attributed
                .iter()
                .filter(|process| unfollowed(&process.launch))
                .map(|process| (process.identity(), process.launch.clone()))
                .collect();
            let mut sessions: HashMap<String, Vec<(ContainerId, Option<u64>)>> = HashMap::new();
            for session in persisted
                .launch_sessions
                .iter()
                .filter(|session| unfollowed(&session.launch))
            {
                sessions
                    .entry(session.launch.clone())
                    .or_default()
                    .push((ContainerId(session.sid as u32), session.leader_start));
            }
            (before, recorded, sessions)
        };
        self.reopen_containments(&persisted);
        let Some(table) = self.scan(true) else {
            tracing::warn!("listing processes failed; what a previous run left is not stopped");
            self.reconciled();
            return;
        };
        if readable {
            let members = self.book().members_of(ENDED, &table);
            let stopped = {
                let mut book = self.book();
                let mut previous = self.previous_run();
                if previous.holding && !members.is_empty() {
                    tracing::info!(
                        count = members.len(),
                        "keeping what a previous run's agents left running until no holder the daemon cannot link to is left"
                    );
                    previous.held.extend(members);
                    previous.recorded = recorded;
                    previous.sessions = sessions;
                    drop(previous);
                    // A launch followed since the read-back, during the scan, takes back now
                    // what was recorded for it.
                    self.take_back_followed(&mut book);
                    Vec::new()
                } else {
                    members
                }
            };
            if !stopped.is_empty() {
                tracing::info!(
                    count = stopped.len(),
                    "stopping what a previous run's agents left running"
                );
                self.stop_set(stopped, &table);
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
            let kept_sessions: HashSet<ContainerId> = book.kept_sessions.keys().copied().collect();
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
        // The file was read back above, so what is written from here on follows it.
        self.reconciled();
        self.persist(true);
    }

    /// Opens again the containments of the launches of an earlier run, for those the host lists
    /// the members of: the launches the previous run's ledger file attributed processes to, and
    /// those whose containments the host still has ([`Containment::leftovers`]). What is in them
    /// is held for an ended launch; one with nothing left in it is dropped, which on Linux removes
    /// its group. A launch the book follows is never taken, an adopted one included, which keeps
    /// its own containment ([`Self::containment_adopted`]): the lock is held throughout, as
    /// [`Self::begin_launch`] holds it while it makes a launch's containment.
    fn reopen_containments(&self, persisted: &persist::Persisted) {
        let mut book = self.book();
        let launches: HashSet<String> = persisted
            .attributed
            .iter()
            .map(|process| process.launch.clone())
            .chain(Containment::leftovers(&self.instance))
            .filter(|launch| launch != ENDED && book.followed_as(launch).is_none())
            .collect();
        let reopened: Vec<Arc<Containment>> = launches
            .into_iter()
            .filter_map(|launch| Containment::reopen(&self.instance, &launch))
            .filter(|containment| containment.members().is_some())
            .map(Arc::new)
            .collect();
        if let Some(ended) = book.launches.get_mut(ENDED) {
            ended.containments.extend(reopened);
        }
    }

    /// Writes the ledger file if what a restart needs changed since it was last written; unless
    /// `now`, not sooner than [`WRITE_DEBOUNCE`] after the last write.
    fn persist(&self, now: bool) {
        // What is not written now stays dirty, and the reconcile writes it.
        if self.awaiting_reconcile.load(Ordering::Acquire) {
            return;
        }
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
            persist::Persisted::from_book(&book, boot_id())
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
