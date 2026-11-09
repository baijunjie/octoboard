//! The daemon's side of the holders: starting the holder terminals are run in, the link to it,
//! and the terminals it holds ([`HeldTerminal`]).
//!
//! A [`Holders`] owns the link to the holder of the current build, starts one on the first
//! terminal asked for (from the copy of the build under `~/.octoboard/builds`, see
//! [`super::build`]), and starts another if the link is lost, or pending while its holder cannot
//! be reached. The holders it finds running at start are adopted when their version is in the
//! compatible range ([`super::compat`]); a holder outside it is ended. One that runs but cannot be
//! linked to is not ended: it is tried again in the background until it is linked to, and taken
//! over then, or known to be gone ([`Resolved`]). The holders a quit of the application left to end
//! ([`Holders::end_quit_targets`]) are the exception: each is never adopted, and one that runs but
//! cannot be linked to is ended by its verified process, as the daemon's ordinary stop ends it. The
//! link runs on a runtime of its own, so that its calls can be made from any thread, a runtime's
//! workers included, the way the calls on a terminal the daemon owns are.
//!
//! A daemon that stops for another to take over hands its terminals over
//! ([`Holders::hand_over`]) and parts from its holders leaving every one running
//! ([`Holders::part_planned`]). An ordinary stop asked for once that has begun ends those holders
//! by their verified process instead, with all they run ([`Holders::end_handed_over`]): the
//! terminals they hold are no longer this daemon's to end one by one.

mod held;
mod link;
mod relay;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};

pub use self::held::{HeldAt, HeldReader, HeldTerminal};
use self::link::{locked, Adopted, Link, OnLost, Refused, Unreadable, CONNECT_DEADLINE};
pub use self::relay::{RelayHandler, RelayReply, RelayRequest};
use super::build::Builds;
use super::compat::{Range, Verdict};
use super::protocol::{HandoverTail, LastParting, OfflineAnswer, SpawnSpec, ToHolder};
use super::quit_targets::QuitTargets;
use super::registry::{self, RegistryEntry};
use crate::platform::proc::{self, Polite, ProcessIdentity, Sent, Signal};
use crate::platform::term::Size;

/// How long a started holder has to leave its registry file.
const START_DEADLINE: Duration = Duration::from_secs(10);

/// How long a holder found running at the daemon's start has to answer its one try, all of them
/// at once: the start stays well inside the time the application gives the daemon to report its
/// address. One that does not answer in time is tried again off the start, as a pending holder.
const ADOPT_DEADLINE: Duration = Duration::from_secs(3);

/// The pause before a holder that runs but could not be linked to at start is tried again,
/// doubled after each failed try up to [`PENDING_PAUSE_MAX`].
const PENDING_PAUSE_MIN: Duration = Duration::from_secs(1);
const PENDING_PAUSE_MAX: Duration = Duration::from_secs(30);

/// How long a start that ends the holders a quit left ([`Holders::wait_endings`]) waits for those
/// endings before it goes on: the application waits for the daemon's startup line only so long,
/// after the adoption's own [`ADOPT_DEADLINE`]. An ending still running when it passes carries on
/// in the daemon.
pub const ENDINGS_WAIT: Duration = Duration::from_secs(4);

/// How long a holder that was asked to end has to be gone before it is killed. It ends every
/// program it runs first, a few seconds each at most.
const END_GRACE: Duration = Duration::from_secs(10);

/// How often, and for how long, the copies of builds are swept after a holder's link ended, which
/// is when its process may still be on its way out.
const LOSS_SWEEP_TRIES: u32 = 20;
const LOSS_SWEEP_INTERVAL: Duration = Duration::from_millis(500);

/// Starts the holder `exe` runs as, named `holder_id`, that leaves its registry file in
/// `registry_dir`. It returns once the holder is started, not once it is ready.
pub type Launch = Box<dyn Fn(&Path, &str, &Path) -> std::io::Result<()> + Send + Sync>;

/// Where the holders are and how one is started.
pub struct ClientConfig {
    pub registry_dir: PathBuf,
    /// The running executable, which a holder is started from when no copy of the build is made.
    pub exe: PathBuf,
    /// Where the copies of builds are made, and removed; `None` starts holders from `exe`.
    pub builds: Option<Arc<Builds>>,
    /// The versions of the holders found running that are adopted.
    pub range: Range,
    pub launch: Launch,
    /// Handles the requests the holder relays from its helpers.
    pub relay: Arc<dyn RelayHandler>,
}

/// A [`Launch`] that runs `exe holder` detached from this process, so that it outlives it
/// (`platform::proc::start_detached`). `application` is the desktop application's pid, which a
/// holder is started under where the daemon's own job would end it.
pub fn launch_detached(application: Option<u32>) -> Launch {
    Box::new(move |exe, holder_id, registry_dir| {
        // Started in the registry's directory: a holder outlives the daemon, and one whose
        // working directory were the daemon's would keep that folder (the installation's, say)
        // from being replaced or removed.
        crate::platform::fs::create_private_dir(registry_dir)?;
        let mut command = std::process::Command::new(exe);
        command
            .arg("holder")
            .arg("--holder-id")
            .arg(holder_id)
            .arg("--registry-dir")
            .arg(registry_dir)
            .current_dir(registry_dir);
        crate::platform::proc::start_detached(command, application)
    })
}

/// A launch's credentials, as its holder issued them: what its helpers call, and the key they
/// carry. The key is the holder's until the terminal is released, and is revoked when this is
/// dropped without the program having been started.
pub struct Issued {
    /// The port of the holder's helper routes.
    pub port: u16,
    pub key: String,
    /// The executable the holder runs from, for the helpers to run.
    pub exe: String,
    terminal: String,
    link: Arc<Link>,
    used: bool,
}

impl Issued {
    /// Starts the program on the holder that issued the key, as the terminal the key is for;
    /// `spec.id` is replaced by that terminal's id. **Blocks.**
    pub fn spawn(mut self, mut spec: SpawnSpec) -> Result<HeldTerminal> {
        self.used = true;
        spec.id = self.terminal.clone();
        self.link.spawn(spec)
    }
}

impl Drop for Issued {
    fn drop(&mut self) {
        if !self.used {
            self.link.revoke_launch(&self.terminal);
        }
    }
}

/// A holder found running when the daemon started, with the terminals it held for it.
pub struct Found {
    pub holder_id: String,
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the adoption tests"))]
    pub version: String,
    pub build_id: String,
    /// How the holder's last link to a daemon ended.
    pub last_parting: LastParting,
    pub terminals: Vec<FoundTerminal>,
}

/// A terminal a holder was found running, as its handshake and tail give it. Its output is read
/// from the first byte after the tail.
pub struct FoundTerminal {
    pub id: String,
    pub size: Option<Size>,
    /// The daemon's own record of the terminal, as it was given when the terminal was started.
    pub meta: serde_json::Value,
    /// The program had exited already, which the terminal reports like any exit.
    pub exited: bool,
    /// The raw output the holder kept: the last of what the program printed.
    pub tail: Vec<u8>,
    pub held: HeldTerminal,
    /// Present when a handover marked the tail: where the output since that handover's mark
    /// begins in it (unless it says the mark was cut off), and how old each part of that is.
    pub handover: Option<HandoverTail>,
    /// When the daemon read the welcome the tail came with, which the ages in `handover` count
    /// back from.
    pub welcomed_at: Instant,
    /// The queries the holder answers itself while no daemon is linked, as its welcome lists them.
    pub answers_offline: Vec<OfflineAnswer>,
    /// The holder that runs the terminal.
    pub holder_id: String,
}

impl FoundTerminal {
    /// The output since the handover's mark, which is all of the tail a screen saved at the mark
    /// leaves to replay; the whole tail when no handover marked it.
    pub fn since_handover(&self) -> &[u8] {
        since_mark(&self.tail, self.handover.as_ref())
    }
}

/// The part of `tail` since the mark of `handover`; all of it when no handover marked it.
fn since_mark<'a>(tail: &'a [u8], handover: Option<&HandoverTail>) -> &'a [u8] {
    let since = handover.map_or(0, |handover| handover.since);
    let since = usize::try_from(since).unwrap_or(usize::MAX);
    &tail[since.min(tail.len())..]
}

/// What a start found in the registry: the holders it took over, and those whose verified
/// process runs but that could not be linked to.
pub struct Adoption {
    pub found: Vec<Found>,
    /// The ids of the holders that could not be linked to. Each is tried again in the background
    /// and resolved through the [`OnResolved`] the start gave.
    pub pending: Vec<String>,
    /// The ids of the holders not adopted that are being ended off the start (a version outside
    /// the range or that cannot be read, a welcome that cannot be read or names another holder),
    /// each resolved as gone through the [`OnResolved`] once it is ended.
    pub ending: Vec<String>,
}

/// How a holder that could not be linked to at start turned out.
pub enum Resolved {
    /// It was linked to, and taken over as at start. `holder_id` is its registry file's, which its
    /// welcome matched. The helpers' requests it relays wait until `relays` is dropped, so that
    /// the sessions they name are adopted first.
    Linked {
        holder_id: String,
        found: Found,
        relays: HeldRelays,
    },
    /// Nothing it ran is the daemon's any more: it ended on its own or its process is gone,
    /// another daemon took it over, or it was ended for a version outside the compatible range or
    /// a welcome this build cannot read.
    ///
    /// `tails` holds what each terminal of a holder that was refused, by the start or once linked
    /// to, showed last, to rebuild its screen from; empty for a holder gone otherwise.
    Gone {
        holder_id: String,
        tails: Vec<EndedTail>,
    },
}

/// The tail a terminal of a holder that was refused had kept when it was ended: the last of what
/// its program printed, and the size of its screen.
pub struct EndedTail {
    pub terminal_id: String,
    pub size: Option<Size>,
    pub tail: Vec<u8>,
    /// Present when a handover marked the tail ([`FoundTerminal::handover`]): what tells whether a
    /// screen saved at that handover is the one the tail follows.
    pub handover: Option<HandoverTail>,
}

impl EndedTail {
    /// The output since the handover's mark, all of the tail a screen saved at the mark leaves to
    /// replay; the whole tail when no handover marked it.
    pub fn since_handover(&self) -> &[u8] {
        since_mark(&self.tail, self.handover.as_ref())
    }
}

/// What is told how a holder that could not be linked to at start turned out. Called on a thread
/// of the holders' own, which it may block.
pub type OnResolved = Arc<dyn Fn(Resolved) + Send + Sync>;

/// The helpers' requests a holder linked to after the start relays, held back until this is
/// dropped.
pub struct HeldRelays {
    link: Arc<Link>,
    /// Where the holder is noted as left to the next daemon ([`Self::leave_for_successor`]).
    shared: Weak<Shared>,
}

impl HeldRelays {
    /// Notes that the holder, linked to while a planned stop is under way, is left running
    /// untouched for the next daemon, which the planned stop's handover never marks: an ordinary
    /// stop that escalates the planned stop ends it with those handed over
    /// ([`Holders::end_handed_over`]), and one that has already begun to end them ends this one
    /// now, by its verified process. **Blocks** then, for up to the grace an ended holder has.
    pub fn leave_for_successor(&self) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let holder_id = self.link.welcome().holder_id.clone();
        {
            // Under the lock `Holders::end_handed_over` turns the planned stop ordinary under.
            let _pending = locked(&shared.pending);
            if shared.planned.load(Ordering::Acquire) {
                locked(&shared.handed_over).insert(holder_id);
                return;
            }
        }
        if !shared.stopping.load(Ordering::Acquire) {
            return;
        }
        self.link.part(false);
        end_identified(&holder_id, self.link.identity(), LEFT_HOLDER);
    }
}

impl Drop for HeldRelays {
    fn drop(&mut self) {
        self.link.release_relays();
    }
}

/// What `Holders` shares with the threads that try again the holders it could not link to.
struct Shared {
    registry_dir: PathBuf,
    /// The daemon's run id, which every link's hello carries.
    run_id: Arc<str>,
    range: Range,
    relay: Arc<dyn RelayHandler>,
    handle: tokio::runtime::Handle,
    /// What a lost link has done: the copies of builds swept.
    on_lost: OnLost,
    /// The links to the holders found running at start, or linked to later, and those that were
    /// current while their holder could not be reached: parted from with the rest. Locked after
    /// [`Self::pending`] where both are.
    adopted: Mutex<Vec<Arc<Link>>>,
    /// The holders that could not be linked to at start, by id, while they are tried again.
    pending: Mutex<HashMap<String, RegistryEntry>>,
    /// The daemon is parting from its holders: none is tried again from now on.
    stopping: AtomicBool,
    /// The parting is planned ([`Holders::begin_planned_stop`], [`Holders::part_planned`]): every
    /// holder is left running. Set before `stopping`.
    planned: AtomicBool,
    /// The holders a quit of the application left to end ([`Holders::end_quit_targets`]): none is
    /// adopted, whatever its version, and each is resolved in the record as it is confirmed gone.
    quit: OnceLock<Arc<QuitTargets>>,
    /// The threads that end the holders not adopted at start that are quit targets
    /// ([`Self::quit`]), by the holder's id, each telling whether what its holder ran has
    /// ended ([`Holders::wait_endings`]).
    endings: Mutex<Vec<(String, std::thread::JoinHandle<bool>)>>,
    /// The holders that handed terminals over to the next daemon ([`Holders::hand_over`]), or that
    /// were linked to during the planned stop and left to it untouched
    /// ([`HeldRelays::leave_for_successor`]), by id. Locked after [`Self::pending`] where both
    /// are.
    handed_over: Mutex<HashSet<String>>,
    /// The holders a planned parting let go, linked to or not, by id, with the process identity
    /// their registry file names when it can be read ([`Holders::part_planned`]). Locked after
    /// [`Self::pending`] where both are.
    parted: Mutex<Vec<(String, Option<ProcessIdentity>)>>,
}

pub struct Holders {
    config: ClientConfig,
    /// The runtime the links run on; taken out and shut down without waiting when this is
    /// dropped, which may be in a runtime's own thread.
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
    shared: Arc<Shared>,
    current: Mutex<Option<Arc<Link>>>,
    /// The id and version of the current link's holder, kept apart from `current`, which
    /// [`Holders::link`] holds for as long as a holder takes to start: [`Holders::versions`]
    /// reads this one without waiting for it. Set and cleared where `current` is.
    current_holder: Mutex<Option<(String, String)>>,
}

/// The ending of a holder that is not adopted, or of its terminals, to be run where waiting on it
/// is no harm; what it came to says whether what the holder ran has ended.
type Ending = Box<dyn FnOnce() -> Ended + Send>;

/// What an [`Ending`] came to.
struct Ended {
    /// What the holder ran has ended: its terminals were, or the holder is gone.
    gone: bool,
    /// The tails of the terminals it ended, which no daemon will show again.
    tails: Vec<EndedTail>,
}

impl Ended {
    /// A holder ended by its process, which leaves no tail.
    fn by_process(gone: bool) -> Self {
        Self {
            gone,
            tails: Vec::new(),
        }
    }
}

/// What [`Shared::take_over`] made of a holder linked to.
enum TakenOver {
    Found(Found),
    /// The link became pending while the terminals were taken: none was taken or ended.
    Unlinked,
    /// Not adopted, and to be ended (or its terminals) by the ending given.
    Ending(Ending),
}

/// What became of one holder the registry lists, at start.
enum Taken {
    Adopted(Found, Arc<Link>),
    /// Its process runs and it could not be linked to: it is tried again in the background.
    Pending,
    /// Not adopted, and to be ended (or its terminals) off the start: what it runs stays claimed
    /// until then.
    Ending(Ending),
    /// Not adopted, or gone.
    Nothing,
}

impl Holders {
    pub fn new(config: ClientConfig) -> Result<Self> {
        // The build id hashes the executable the first time it is asked for, which takes seconds
        // for a large one: it is taken here, before any holder is started or linked to, so that
        // the first hello does not wait on it while a holder's deadlines run.
        super::build_id();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("holder-link")
            .enable_all()
            .build()
            .context("starting the holder link's runtime")?;
        let shared = Arc::new(Shared {
            registry_dir: config.registry_dir.clone(),
            run_id: link::new_run_id(),
            range: config.range.clone(),
            relay: config.relay.clone(),
            handle: runtime.handle().clone(),
            on_lost: sweep_after_loss(config.builds.clone(), config.registry_dir.clone()),
            adopted: Mutex::new(Vec::new()),
            pending: Mutex::new(HashMap::new()),
            stopping: AtomicBool::new(false),
            planned: AtomicBool::new(false),
            quit: OnceLock::new(),
            endings: Mutex::new(Vec::new()),
            handed_over: Mutex::new(HashSet::new()),
            parted: Mutex::new(Vec::new()),
        });
        Ok(Self {
            config,
            runtime: Mutex::new(Some(runtime)),
            shared,
            current: Mutex::new(None),
            current_holder: Mutex::new(None),
        })
    }

    /// Has the holders `targets` names, which a quit of the application left running, ended with
    /// their terminals as one outside the compatible range is, found at start or later: none is
    /// adopted or made the current one, and one whose verified process runs but that cannot be
    /// linked to is ended by that process rather than tried again. Every target is resolved in
    /// the record as its ending is confirmed, or it is found gone, and stays there otherwise;
    /// those with no registry file are resolved now. A holder the registry has besides is adopted
    /// as usual. Called once, before [`Self::adopt_existing`].
    pub fn end_quit_targets(&self, targets: Arc<QuitTargets>) {
        for target in targets.remaining() {
            // Only a file that is certainly absent: one that cannot be read may be a holder's.
            let path = registry::path_of(&self.shared.registry_dir, &target.holder_id);
            if path.try_exists().is_ok_and(|exists| !exists) {
                targets.resolved(&target.holder_id);
            }
        }
        if self.shared.quit.set(targets).is_err() {
            tracing::warn!("the holders to end were given already; the later ones are ignored");
        }
    }

    /// The ids of the holders that could not be linked to and are tried again now.
    pub fn pending_holders(&self) -> Vec<String> {
        locked(&self.shared.pending).keys().cloned().collect()
    }

    /// Links to the holders the registry lists, which outlived the previous daemon, and takes the
    /// terminals each runs. A file whose holder is gone is removed, and so is one whose process
    /// cannot be verified (its file names no identity that can be read) while nothing listens on
    /// its port. Each holder is tried once, all at once, within [`ADOPT_DEADLINE`]. One whose
    /// verified process runs but that cannot be linked to in time is not ended: a refused or
    /// timed-out connection does not show it cannot go on. It is returned as pending and tried
    /// again in the background, pausing longer after each try, until it is linked to or gone,
    /// which `on_resolved` is told ([`Resolved`]). A holder that is not adopted is ended, or its
    /// terminals are, off the start. One whose process
    /// cannot be verified and that cannot be linked to while something listens on its port is
    /// left, with its file, for a later start to try again. A holder of the current build stays
    /// the one new terminals go to, the others are told to take no more. A holder outside the
    /// compatible range is not adopted: its terminals are ended, and it is told to take no more.
    /// One whose welcome cannot be read is ended by its process ([`end_process`]). **Blocks.**
    pub fn adopt_existing(&self, on_resolved: OnResolved) -> Adoption {
        let mut adoption = Adoption {
            found: Vec::new(),
            pending: Vec::new(),
            ending: Vec::new(),
        };
        let mut links = Vec::new();
        let entries = registry::list(&self.shared.registry_dir);
        // At once, so that the start waits for the slowest holder alone.
        let taken: Vec<Taken> = std::thread::scope(|scope| {
            let trying: Vec<_> = entries
                .iter()
                .map(|entry| scope.spawn(move || self.shared.adopt_entry(entry)))
                .collect();
            trying
                .into_iter()
                .map(|tried| tried.join().unwrap_or(Taken::Nothing))
                .collect()
        });
        for (entry, taken) in entries.into_iter().zip(taken) {
            match taken {
                Taken::Adopted(found, link) => {
                    adoption.found.push(found);
                    links.push(link);
                }
                Taken::Pending => {
                    adoption.pending.push(entry.holder_id.clone());
                    self.shared.follow_pending(entry, on_resolved.clone());
                }
                Taken::Ending(ending) => {
                    // Ended off the start, which does not wait: what it runs stays claimed, as a
                    // pending holder's, until it is gone.
                    adoption.ending.push(entry.holder_id.clone());
                    let (on_resolved, shared) = (on_resolved.clone(), self.shared.clone());
                    let holder_id = entry.holder_id.clone();
                    let waited_for = shared.is_quit_target(&entry);
                    let ended = std::thread::spawn(move || {
                        let Ended { gone, tails } = ending();
                        if gone {
                            shared.quit_resolved(&entry.holder_id);
                            on_resolved(Resolved::Gone {
                                holder_id: entry.holder_id,
                                tails,
                            });
                        } else {
                            // It could not be ended and runs on: what it runs stays claimed, and
                            // it is tried again as a holder that cannot be linked to.
                            shared.follow_pending(entry, on_resolved);
                        }
                        gone
                    });
                    // Waited for only where the holder is a quit target; the others' threads are
                    // left detached.
                    if waited_for {
                        locked(&self.shared.endings).push((holder_id, ended));
                    }
                }
                Taken::Nothing => {}
            }
        }
        let mut current = locked(&self.current);
        for link in &links {
            let own_build = link.welcome().build_id == super::build_id();
            if own_build && current.is_none() {
                self.set_current(&mut current, link.clone());
            } else {
                // Told again if the link drops before it takes this.
                link.retire();
            }
        }
        locked(&self.shared.adopted).extend(links);
        adoption
    }

    /// Waits, for up to `bound` in all, for the endings of the quit targets
    /// [`Self::adopt_existing`] left running off the start ([`Self::end_quit_targets`]), and tells
    /// whether every one finished and confirmed that what its holder ran has ended (its terminals
    /// were, or the holder is gone). A holder that could not be ended is tried again off the
    /// start, and an ending still running once `bound` has passed carries on, both logged; the
    /// record keeps every target not confirmed ended. The endings are taken from the holders, so a
    /// second call waits for none of them. **Blocks.**
    pub fn wait_endings(&self, bound: Duration) -> bool {
        let endings: Vec<_> = locked(&self.shared.endings).drain(..).collect();
        let deadline = Instant::now() + bound;
        let mut confirmed = true;
        for (holder_id, ending) in endings {
            while !ending.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if !ending.is_finished() {
                tracing::warn!(holder = %holder_id, "a holder's ending is not done within the wait; it carries on");
                confirmed = false;
            } else if !ending.join().unwrap_or(false) {
                tracing::warn!(holder = %holder_id, "a holder could not be ended; it is tried again once it can be linked to");
                confirmed = false;
            }
        }
        confirmed
    }

    /// Starts the terminal `spec` describes in the current build's holder, starting the holder
    /// first if there is none. **Blocks**, for as long as a holder takes to start when there is
    /// none.
    pub fn spawn(&self, spec: SpawnSpec) -> Result<HeldTerminal> {
        self.link()?.spawn(spec)
    }

    /// Has the current build's holder issue the key for the helpers of a launch of the session
    /// `session_id`, bound to its MCP token `mcp_token` when it has one, starting the holder first
    /// if there is none. The program of the launch has to be started with [`Issued::spawn`], on
    /// the holder that issued the key. **Blocks**, as [`Holders::spawn`] does.
    pub fn issue_launch(&self, session_id: &str, mcp_token: Option<&str>) -> Result<Issued> {
        let link = self.link()?;
        let terminal = uuid::Uuid::new_v4().to_string();
        let key = link.issue_launch(&terminal, session_id, mcp_token)?;
        Ok(Issued {
            port: link.helper_port(),
            exe: link.helper_exe().to_string(),
            key,
            terminal,
            link,
            used: false,
        })
    }

    /// The link to the holder, a new one if the last was lost or cannot be reached for now.
    fn link(&self) -> Result<Arc<Link>> {
        let mut current = locked(&self.current);
        if let Some(link) = current.as_ref() {
            // A link that dropped just now may be on its way back, and starting another holder
            // beside it would leave two.
            link.settled();
            if link.alive() {
                return Ok(link.clone());
            }
            // One whose holder runs but cannot be reached keeps its terminals and is tried again
            // in the background: new terminals go to a holder of their own meanwhile, and it takes
            // no more once it is back.
            if link.pending() {
                link.retire();
                let mut adopted = locked(&self.shared.adopted);
                if !adopted.iter().any(|kept| Arc::ptr_eq(kept, link)) {
                    adopted.push(link.clone());
                }
            }
        }
        let link = self.start_holder()?;
        self.set_current(&mut current, link.clone());
        Ok(link)
    }

    /// Makes `link` the current one, `current` being the locked slot.
    fn set_current(&self, current: &mut Option<Arc<Link>>, link: Arc<Link>) {
        let welcome = link.welcome();
        *locked(&self.current_holder) = Some((welcome.holder_id.clone(), welcome.version.clone()));
        *current = Some(link);
    }

    /// Starts a holder from the copy of the current build, which stays until the holder is
    /// linked to: a copy is removed only when no holder runs from it, and one being started is
    /// not yet in the registry.
    fn start_holder(&self) -> Result<Arc<Link>> {
        let holder_id = uuid::Uuid::new_v4().to_string();
        let start = |exe: &Path| -> Result<Arc<Link>> {
            (self.config.launch)(exe, &holder_id, &self.config.registry_dir)
                .context("starting the holder")?;
            let entry = self.wait_for_registry(&holder_id)?;
            self.connect(&entry)
        };
        match &self.config.builds {
            Some(builds) => builds.run_from(super::build_id(), &self.config.exe, start),
            None => start(&self.config.exe),
        }
    }

    /// Removes the copies of builds no holder runs from: what the start of the daemon does once
    /// it has adopted what it found, and what a lost link does on its own. **Blocks.**
    pub fn sweep_builds(&self) {
        if let Some(builds) = &self.config.builds {
            builds.sweep(&self.config.registry_dir);
        }
    }

    /// Opens the daemon link of the holder `entry` names. **Blocks.**
    fn connect(&self, entry: &RegistryEntry) -> Result<Arc<Link>> {
        self.shared.connect(entry, false, CONNECT_DEADLINE)
    }

    fn wait_for_registry(&self, holder_id: &str) -> Result<RegistryEntry> {
        let deadline = Instant::now() + START_DEADLINE;
        loop {
            if let Some(entry) = registry::read(&self.config.registry_dir, holder_id) {
                return Ok(entry);
            }
            if Instant::now() >= deadline {
                bail!("the holder did not register itself in time");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Begins a planned stop: from now on a holder that could not be linked to is no longer tried
    /// again, and one linked to after all meanwhile is parted from and left running, untouched,
    /// rather than reported to a daemon that is stopping. Returns the holders still pending then,
    /// by id: those the stop leaves untouched, whose saved screens it carries over for the next
    /// daemon. Taken under the lock a holder that becomes linked reads the flags under, so that
    /// each holder is either among those returned or linked, and reported, before this.
    pub fn begin_planned_stop(&self) -> Vec<String> {
        let pending = locked(&self.shared.pending);
        self.shared.planned.store(true, Ordering::Release);
        self.shared.stopping.store(true, Ordering::Release);
        pending.keys().cloned().collect()
    }

    /// Hands the terminals of the holders `holders` names that are linked to now over to the
    /// daemon that comes next, under the handover `token` (see [`ToHolder::Handover`]): each
    /// holder that carries it out marks the end of each terminal's output to this daemon, which
    /// ends the stream its reader takes from there, and keeps what follows in the terminal's tail
    /// for the next daemon. From then on this daemon does nothing with those terminals over the
    /// link, and never records them as exited. A linked holder `holders` does not name, one whose
    /// terminals the caller did not take into account, is left untouched. The holders are asked
    /// all at once, and of one that does not answer within a few seconds only the terminals marked
    /// by then are returned. A holder whose link is being opened again is waited for first, which
    /// may take a while longer. Begins the planned stop first, if the caller has not
    /// ([`Self::begin_planned_stop`]). Returns the terminals handed over. **Blocks.**
    pub fn hand_over(&self, token: &str, holders: &[String]) -> Vec<HeldAt> {
        self.begin_planned_stop();
        let links: Vec<Arc<Link>> = self
            .links()
            .into_iter()
            .filter(|link| holders.contains(&link.welcome().holder_id))
            .collect();
        let handed: Vec<(String, Vec<String>)> = std::thread::scope(|scope| {
            let asked: Vec<_> = links
                .iter()
                .map(|link| scope.spawn(move || (link, link.hand_over(token))))
                .collect();
            asked
                .into_iter()
                .filter_map(|asked| asked.join().ok())
                .map(|(link, handed)| {
                    (link.welcome().holder_id.clone(), handed.unwrap_or_default())
                })
                .collect()
        });
        // For an ordinary stop that may still follow, which has to end these holders whole.
        locked(&self.shared.handed_over).extend(
            handed
                .iter()
                .filter(|(_, terminals)| !terminals.is_empty())
                .map(|(holder_id, _)| holder_id.clone()),
        );
        handed
            .into_iter()
            .flat_map(|(holder_id, terminals)| {
                terminals.into_iter().map(move |terminal_id| HeldAt {
                    holder_id: holder_id.clone(),
                    terminal_id,
                })
            })
            .collect()
    }

    /// The version each holder known to this daemon reports, by holder id: the holders linked to
    /// from their welcome, those still tried again from their registry entry.
    pub fn versions(&self) -> HashMap<String, String> {
        // Copied and released before the links are read, which are locked after it where both
        // are.
        let pending: Vec<(String, String)> = locked(&self.shared.pending)
            .iter()
            .map(|(holder_id, entry)| (holder_id.clone(), entry.version.clone()))
            .collect();
        let adopted = locked(&self.shared.adopted).clone();
        let mut versions: HashMap<String, String> = adopted
            .iter()
            .map(|link| {
                let welcome = link.welcome();
                (welcome.holder_id.clone(), welcome.version.clone())
            })
            .collect();
        // Not through `current`, which a link being opened holds for a long while.
        if let Some((holder_id, version)) = locked(&self.current_holder).clone() {
            versions.insert(holder_id, version);
        }
        for (holder_id, version) in pending {
            versions.entry(holder_id).or_insert(version);
        }
        versions
    }

    /// The links to every holder: the adopted and the current one, each once.
    fn links(&self) -> Vec<Arc<Link>> {
        let adopted = locked(&self.shared.adopted).clone();
        let current = locked(&self.current).clone();
        with_current(adopted, current)
    }

    /// Ends the daemon's use of its holders for another daemon to take over: each linked holder
    /// is told the terminals are handed over, and keeps running with every terminal it has; a
    /// holder that cannot be linked to is no longer tried again, and is left running too. No
    /// holder is ended, so a later [`Self::shutdown`] (this being dropped) finds nothing to do.
    /// Every holder let go is remembered for an ordinary stop that may still follow
    /// ([`Self::end_handed_over`]). **Blocks** for up to a few seconds for each holder, while it
    /// takes the parting.
    pub fn part_planned(&self) {
        let mut tried = locked(&self.shared.pending);
        self.shared.planned.store(true, Ordering::Release);
        self.shared.stopping.store(true, Ordering::Release);
        // Left running: a holder tried again in the background is let go, not ended.
        let pending: Vec<RegistryEntry> = tried.drain().map(|(_, entry)| entry).collect();
        drop(tried);
        let links = self.take_links();
        locked(&self.shared.parted).extend(
            pending
                .iter()
                .map(|entry| {
                    (
                        entry.holder_id.clone(),
                        ProcessIdentity::parse(&entry.started),
                    )
                })
                .chain(
                    links
                        .iter()
                        .map(|link| (link.welcome().holder_id.clone(), link.identity())),
                ),
        );
        // All at once, so that an ordinary stop that may still follow waits for the slowest
        // parting alone.
        part_all(&links, true);
    }

    /// The holders a planned stop has handed terminals over from, parted from or left untouched
    /// for the next daemon, by id: what they run is no longer this daemon's to end terminal by
    /// terminal, only with the holder ([`Self::end_handed_over`]).
    pub fn left_to_successor(&self) -> HashSet<String> {
        let mut left = locked(&self.shared.handed_over).clone();
        left.extend(
            locked(&self.shared.parted)
                .iter()
                .map(|(holder_id, _)| holder_id.clone()),
        );
        left
    }

    /// Ends now, by its verified process, every holder a planned stop has handed terminals over
    /// from, parted from or left untouched for the next daemon ([`Self::left_to_successor`]), with
    /// all it runs: for an ordinary stop asked for once a planned stop has begun to hand the
    /// terminals over. What such a holder runs can no longer be ended terminal by terminal: a
    /// terminal handed over is the next daemon's, and nothing for it is sent over the link any
    /// more, a holder parted from has no link left, and what a holder left untouched runs was
    /// never this daemon's. A holder asked to end ends every program it runs before it exits
    /// ([`end_process`]). The links still open to them are closed as the holders end, all at
    /// once. A holder that could not be linked to and was not parted from is left to
    /// [`Self::shutdown`], as is every other link; one linked to, or left to the next daemon, from
    /// now on while the daemon stops is ended rather than left running. A holder whose process
    /// cannot be verified is left running. Each holder is ended once: a second call finds none of
    /// them. **Blocks** for up to the grace an ended holder has.
    pub fn end_handed_over(&self) {
        {
            // Under the lock `Shared::link_pending` reads it under, so that a holder linked to
            // from here on is ended, and one linked to before is among those parted from.
            let _pending = locked(&self.shared.pending);
            self.shared.planned.store(false, Ordering::Release);
        }
        let handed = std::mem::take(&mut *locked(&self.shared.handed_over));
        let links = self.take_links_of(&handed);
        let mut targets = std::mem::take(&mut *locked(&self.shared.parted));
        for link in &links {
            let holder_id = &link.welcome().holder_id;
            if !targets.iter().any(|(parted, _)| parted == holder_id) {
                targets.push((holder_id.clone(), link.identity()));
            }
        }
        if targets.is_empty() {
            return;
        }
        tracing::info!(
            holders = targets.len(),
            "an ordinary stop follows the planned stop; the holders handed over are ended with what they run"
        );
        // TODO: a holder asked to end kills its programs, and the output since each terminal's
        // mark goes with it. Ending marked terminals politely, with their tails handed back for
        // the previews, needs a holder request that does not exist yet.
        // At once, each on a thread of its own: each may take the whole grace.
        let ending: Vec<_> = targets
            .into_iter()
            .map(|(holder, identity)| {
                std::thread::spawn(move || end_identified(&holder, identity, LEFT_HOLDER))
            })
            .collect();
        // Their holders are ending: nothing is left to hand over.
        part_all(&links, false);
        for ended in ending {
            ended.join().ok();
        }
    }

    /// Takes the links to the holders `holders` names out of use, as [`Self::take_links`] takes
    /// every link.
    fn take_links_of(&self, holders: &HashSet<String>) -> Vec<Arc<Link>> {
        if holders.is_empty() {
            return Vec::new();
        }
        let mut taken = Vec::new();
        locked(&self.shared.adopted).retain(|link| {
            let of = holders.contains(&link.welcome().holder_id);
            if of {
                taken.push(link.clone());
            }
            !of
        });
        let mut slot = locked(&self.current);
        if slot
            .as_ref()
            .is_some_and(|link| holders.contains(&link.welcome().holder_id))
        {
            let current = slot.take();
            // Cleared with `current`, which it follows; locked after it, as `set_current` does.
            *locked(&self.current_holder) = None;
            taken = with_current(taken, current);
        }
        taken
    }

    /// Takes the links to every holder out of use: the adopted and the current one, each once.
    fn take_links(&self) -> Vec<Arc<Link>> {
        let adopted = locked(&self.shared.adopted).drain(..).collect();
        let mut slot = locked(&self.current);
        let current = slot.take();
        // Cleared with `current`, which it follows; locked after it, as `set_current` does.
        *locked(&self.current_holder) = None;
        with_current(adopted, current)
    }

    /// Ends the daemon's use of its holders on purpose: each is told, and one with no terminal
    /// left exits. What a holder still runs keeps running; the daemon ends every terminal before
    /// it comes here. A holder that cannot be linked to, whose terminals the daemon could not end,
    /// is ended by its verified process instead ([`end_process`]), as the daemon's ordinary stop
    /// ends every program; it is no longer tried again. **Blocks** for up to the grace an ended
    /// holder has.
    pub fn shutdown(&self) {
        self.shared.stopping.store(true, Ordering::Release);
        let pending: Vec<RegistryEntry> = locked(&self.shared.pending)
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        let links = self.take_links();
        let unreachable: Vec<(String, Option<ProcessIdentity>)> = pending
            .iter()
            .map(|entry| {
                (
                    entry.holder_id.clone(),
                    ProcessIdentity::parse(&entry.started),
                )
            })
            .chain(
                links
                    .iter()
                    .filter(|link| link.pending())
                    .map(|link| (link.welcome().holder_id.clone(), link.identity())),
            )
            .collect();
        // At once, each on a thread of its own: each may take the whole grace.
        let ending: Vec<_> = unreachable
            .into_iter()
            .map(|(holder, identity)| {
                std::thread::spawn(move || end_identified(&holder, identity, UNREACHABLE_HOLDER))
            })
            .collect();
        for link in links {
            // Nothing is handed over: the daemon has ended every terminal itself. A pending link
            // stops being tried.
            link.part(false);
        }
        for ended in ending {
            ended.join().ok();
        }
    }
}

impl Drop for Holders {
    fn drop(&mut self) {
        self.shutdown();
        let runtime = locked(&self.runtime).take();
        if let Some(runtime) = runtime {
            runtime.shutdown_background();
        }
    }
}

impl Shared {
    /// Links to the holder `entry` names and takes its terminals, or deals with it as
    /// [`Holders::adopt_existing`] says when it is not adopted. **Blocks.**
    fn adopt_entry(&self, entry: &RegistryEntry) -> Taken {
        let running = ProcessIdentity::parse(&entry.started).map(|id| id.is_running());
        if running == Some(false) {
            tracing::info!(holder = %entry.holder_id, "removing the registry file of a holder that is gone");
            self.remove_gone(&entry.holder_id);
            return Taken::Nothing;
        }
        // A file whose process cannot be verified is removed once nothing listens on its port: the
        // holder is gone. A holder that answers and then fails the handshake is left.
        if running.is_none() && registry::nothing_listens(entry.port) {
            tracing::info!(holder = %entry.holder_id, "removing the registry file of a holder nothing listens for");
            self.remove_gone(&entry.holder_id);
            return Taken::Nothing;
        }
        let link = match self.connect(entry, false, ADOPT_DEADLINE) {
            Ok(link) => link,
            Err(err) if err.downcast_ref::<Unreadable>().is_some() => {
                tracing::warn!(holder = %entry.holder_id, "a holder's welcome cannot be read; it is ended");
                return Taken::Ending(self.ending_by_process(entry));
            }
            Err(err) if err.downcast_ref::<Refused>().is_some() => {
                tracing::warn!(holder = %entry.holder_id, "a holder refused this daemon, having been taken over by another; it is left to it");
                return Taken::Nothing;
            }
            Err(err) => {
                if running.is_none() {
                    tracing::warn!(holder = %entry.holder_id, %err, "a holder whose process cannot be verified could not be reached; it and its registry file are left for a later start");
                    return Taken::Nothing;
                }
                if self.is_quit_target(entry) {
                    tracing::warn!(holder = %entry.holder_id, %err, "a holder the quit left could not be linked to; it is ended by its process");
                    return Taken::Ending(self.ending_by_process(entry));
                }
                tracing::warn!(holder = %entry.holder_id, %err, "a holder that runs could not be linked to; it is tried again in the background, and what it runs stays claimed");
                return Taken::Pending;
            }
        };
        match self.take_over(entry, &link) {
            TakenOver::Found(found) => Taken::Adopted(found, link),
            TakenOver::Unlinked if self.is_quit_target(entry) => {
                tracing::warn!(holder = %entry.holder_id, "a holder the quit left could not be reached while its terminals were taken; it is ended by its process");
                Taken::Ending(self.ending_by_process(entry))
            }
            TakenOver::Unlinked => {
                tracing::warn!(holder = %entry.holder_id, "a holder could not be reached while its terminals were taken over; it is tried again in the background, and what it runs stays claimed");
                Taken::Pending
            }
            TakenOver::Ending(ending) => Taken::Ending(ending),
        }
    }

    /// Takes the terminals of the holder `entry` names, linked to over `link`, when its version is
    /// in the compatible range and it is no quit target ([`Holders::end_quit_targets`]). One
    /// outside the range, or a quit target, is refused, its terminals ended; one whose version
    /// cannot be read, or whose welcome names another holder than its registry file, is ended by
    /// its process and parted from. Each of those endings is returned to be run, not run, since it
    /// takes seconds. A link that becomes pending while the terminals are taken is given up with
    /// the holder keeping them all ([`Link::adopt`]), for the holder to be tried again.
    /// **Blocks.**
    fn take_over(&self, entry: &RegistryEntry, link: &Arc<Link>) -> TakenOver {
        let welcome = link.welcome();
        if welcome.holder_id != entry.holder_id {
            tracing::warn!(holder = %entry.holder_id, answered = %welcome.holder_id, "a holder's welcome names another holder, which cannot be read as its own; it is ended");
            let (link, ending) = (link.clone(), self.ending_by_process(entry));
            return TakenOver::Ending(Box::new(move || {
                link.part(false);
                ending()
            }));
        }
        match self.range.judge(&welcome.version) {
            Verdict::Adoptable if !self.is_quit_target(entry) => {}
            Verdict::Unreadable => {
                tracing::warn!(holder = %welcome.holder_id, version = %welcome.version, "a holder's version cannot be read; it is ended");
                return TakenOver::Ending(self.ending_by_process(entry));
            }
            Verdict::Adoptable => {
                tracing::warn!(holder = %welcome.holder_id, "a holder the quit left is not adopted; its terminals are ended");
                return self.refusing(entry, link);
            }
            refused => {
                tracing::warn!(
                    holder = %welcome.holder_id,
                    version = %welcome.version,
                    verdict = ?refused,
                    "a holder outside the compatible range is not adopted; its terminals are ended"
                );
                return self.refusing(entry, link);
            }
        }
        let Some(adopted) = link.adopt() else {
            return TakenOver::Unlinked;
        };
        TakenOver::Found(Found {
            holder_id: welcome.holder_id.clone(),
            version: welcome.version.clone(),
            build_id: welcome.build_id.clone(),
            last_parting: welcome.last_parting,
            terminals: adopted
                .into_iter()
                .map(
                    |Adopted {
                         info,
                         tail,
                         welcomed_at,
                         held,
                     }| FoundTerminal {
                        id: info.id,
                        size: info.size,
                        meta: info.meta,
                        exited: info.exited,
                        tail,
                        held,
                        handover: info.handover,
                        welcomed_at,
                        answers_offline: welcome.answers_offline.clone(),
                        holder_id: welcome.holder_id.clone(),
                    },
                )
                .collect(),
        })
    }

    /// The ending of the terminals of the holder `entry` names, linked to over `link`, which is not
    /// adopted: each takes a few seconds to end, so it is returned to be run, not run.
    fn refusing(&self, entry: &RegistryEntry, link: &Arc<Link>) -> TakenOver {
        let (link, registry_dir, entry) = (link.clone(), self.registry_dir.clone(), entry.clone());
        TakenOver::Ending(Box::new(move || refuse(&link, &registry_dir, &entry)))
    }

    /// Has the holder `entry` names, which runs but could not be linked to at start, tried again
    /// on a thread of its own ([`Self::link_pending`]).
    fn follow_pending(self: &Arc<Self>, entry: RegistryEntry, on_resolved: OnResolved) {
        locked(&self.pending).insert(entry.holder_id.clone(), entry.clone());
        let shared = self.clone();
        let holder_id = entry.holder_id.clone();
        let spawned = std::thread::Builder::new()
            .name("holder-pending".to_string())
            .spawn(move || shared.link_pending(entry, on_resolved));
        if let Err(err) = spawned {
            tracing::warn!(holder = %holder_id, %err, "the holder cannot be tried again; what it runs stays claimed until the next start");
        }
    }

    /// Tries to link to the holder `entry` names until it is linked to or gone, pausing longer
    /// after each try, and tells `on_resolved` how it turned out. Stops without telling once the
    /// daemon parts from its holders, which ends this one ([`Holders::shutdown`]). **Blocks**, on a
    /// thread of its own.
    fn link_pending(self: &Arc<Self>, entry: RegistryEntry, on_resolved: OnResolved) {
        let mut pause = PENDING_PAUSE_MIN;
        // The holder linked to and taken over, or the tails of a holder that is gone.
        let resolved: Result<(Found, Arc<Link>), Vec<EndedTail>> = loop {
            std::thread::sleep(pause);
            pause = (pause * 2).min(PENDING_PAUSE_MAX);
            if self.stopping.load(Ordering::Acquire) {
                return;
            }
            if holder_gone(&self.registry_dir, &entry.holder_id) {
                tracing::info!(holder = %entry.holder_id, "a holder that could not be linked to is gone");
                self.remove_gone(&entry.holder_id);
                break Err(Vec::new());
            }
            match self.connect(&entry, true, CONNECT_DEADLINE) {
                Ok(link) => {
                    tracing::info!(holder = %entry.holder_id, "linked to a holder that could not be linked to at start");
                    match self.take_over(&entry, &link) {
                        TakenOver::Found(found) => break Ok((found, link)),
                        TakenOver::Ending(ending) => {
                            let Ended { gone, tails } = ending();
                            if gone {
                                self.quit_resolved(&entry.holder_id);
                                break Err(tails);
                            }
                            // Not ended, it runs on: tried again, what it runs still claimed.
                        }
                        // It dropped again before its terminals were taken: tried again, every
                        // terminal still its; a quit target is ended by its process instead.
                        TakenOver::Unlinked => {
                            if self.end_quit_target_by_process(&entry) {
                                break Err(Vec::new());
                            }
                        }
                    }
                }
                Err(err) if err.downcast_ref::<Refused>().is_some() => {
                    tracing::warn!(holder = %entry.holder_id, "a holder refused this daemon, having been taken over by another; it is left to it");
                    break Err(Vec::new());
                }
                Err(err) if err.downcast_ref::<Unreadable>().is_some() => {
                    tracing::warn!(holder = %entry.holder_id, "a holder's welcome cannot be read; it is ended");
                    if self.end_by_process_resolving(&entry) {
                        break Err(Vec::new());
                    }
                    // Not ended, it runs on: tried again, what it runs still claimed.
                }
                Err(err) => {
                    tracing::debug!(holder = %entry.holder_id, %err, "the holder still cannot be linked to; trying again later");
                    if self.end_quit_target_by_process(&entry) {
                        break Err(Vec::new());
                    }
                }
            }
        };
        {
            // Under the lock the shutdown drains: a holder it no longer finds pending is one it
            // parts from or ends with the rest.
            let mut pending = locked(&self.pending);
            pending.remove(&entry.holder_id);
            if let Ok((_, link)) = &resolved {
                if self.stopping.load(Ordering::Acquire) {
                    // Read under the lock, which `Holders::end_handed_over` writes it under.
                    let planned = self.planned.load(Ordering::Acquire);
                    if planned {
                        // For an ordinary stop that may still follow, as the parting remembers
                        // the holders it lets go.
                        locked(&self.parted).push((entry.holder_id.clone(), link.identity()));
                    }
                    drop(pending);
                    // A planned parting leaves every holder running, its terminals with it.
                    if planned {
                        link.part(true);
                    } else {
                        link.part(false);
                        end_by_process(&self.registry_dir, &entry);
                    }
                    return;
                }
                locked(&self.adopted).push(link.clone());
            }
        }
        match resolved {
            Ok((found, link)) => {
                // New terminals go to the current build's holder, whatever this one's build.
                link.retire();
                on_resolved(Resolved::Linked {
                    holder_id: entry.holder_id,
                    found,
                    relays: HeldRelays {
                        link,
                        shared: Arc::downgrade(self),
                    },
                });
            }
            Err(tails) => on_resolved(Resolved::Gone {
                holder_id: entry.holder_id,
                tails,
            }),
        }
    }

    /// Whether the holder `entry` names, with the process identity it was recorded under, is a
    /// quit target not resolved yet.
    fn is_quit_target(&self, entry: &RegistryEntry) -> bool {
        self.quit
            .get()
            .is_some_and(|quit| quit.is_target(&entry.holder_id, &entry.started))
    }

    /// Notes the holder `holder_id` as resolved in the record of the quit targets, if it is one.
    fn quit_resolved(&self, holder_id: &str) {
        if let Some(quit) = self.quit.get() {
            quit.resolved(holder_id);
        }
    }

    /// Removes the registry file of the holder `holder_id`, which is gone, and resolves it in the
    /// record of the quit targets.
    fn remove_gone(&self, holder_id: &str) {
        registry::remove(&self.registry_dir, holder_id);
        self.quit_resolved(holder_id);
    }

    /// Ends the holder `entry` names by its process ([`end_by_process`]) and, when it is gone
    /// after that, resolves it in the record of the quit targets. **Blocks** for up to the grace.
    fn end_by_process_resolving(&self, entry: &RegistryEntry) -> bool {
        let gone = self.ending_by_process(entry)().gone;
        if gone {
            self.quit_resolved(&entry.holder_id);
        }
        gone
    }

    /// Ends the holder `entry` names by its verified process when it is a quit target that
    /// cannot be linked to: a target whose process cannot be verified is left, quietly, and stays
    /// in the record. True when the holder is gone after it. **Blocks** for up to the grace.
    fn end_quit_target_by_process(&self, entry: &RegistryEntry) -> bool {
        if !self.is_quit_target(entry) || ProcessIdentity::parse(&entry.started).is_none() {
            return false;
        }
        tracing::warn!(holder = %entry.holder_id, "a holder the quit left cannot be linked to; it is ended by its process");
        self.end_by_process_resolving(entry)
    }

    /// The ending of the holder `entry` names by its process ([`end_by_process`]), to be run off
    /// the start, since it takes up to a grace the start does not wait out; it tells whether the
    /// holder is gone after it.
    fn ending_by_process(&self, entry: &RegistryEntry) -> Ending {
        let (registry_dir, entry) = (self.registry_dir.clone(), entry.clone());
        Box::new(move || {
            end_by_process(&registry_dir, &entry);
            Ended::by_process(holder_gone(&registry_dir, &entry.holder_id))
        })
    }

    /// Opens the daemon link of the holder `entry` names, which has `deadline` to answer; with
    /// `hold_relays`, the requests it relays wait for [`Link::release_relays`]. **Blocks.**
    fn connect(
        &self,
        entry: &RegistryEntry,
        hold_relays: bool,
        deadline: Duration,
    ) -> Result<Arc<Link>> {
        let entry = entry.clone();
        let (linked, link) = std::sync::mpsc::channel();
        let run_id = self.run_id.clone();
        let registry_dir = self.registry_dir.clone();
        let handle = self.handle.clone();
        let relay = self.relay.clone();
        let on_lost = self.on_lost.clone();
        self.handle.spawn(async move {
            let connected = Link::connect(
                &entry,
                run_id,
                registry_dir,
                handle,
                relay,
                on_lost,
                hold_relays,
                deadline,
            )
            .await;
            linked.send(connected).ok();
        });
        link.recv()
            .map_err(|_| anyhow!("the holder link's runtime is gone"))?
    }
}

/// The links `adopted` names plus `current`, the current one once: it may be one of the adopted.
fn with_current(mut adopted: Vec<Arc<Link>>, current: Option<Arc<Link>>) -> Vec<Arc<Link>> {
    if let Some(current) = current {
        if !adopted.iter().any(|link| Arc::ptr_eq(link, &current)) {
            adopted.push(current);
        }
    }
    adopted
}

/// Ends the holder `entry` names by its process, and removes its registry file: it is asked to
/// end first, then killed after a grace ([`end_process`]). A process that cannot be told from
/// another by its identity is left alone, as is its file. A kill that was delivered counts as the
/// holder gone: the file is removed without waiting for the process to disappear, which is what
/// [`holder_gone`] then reads. **Blocks** for up to the grace.
fn end_by_process(registry_dir: &Path, entry: &RegistryEntry) {
    let Some(identity) = ProcessIdentity::parse(&entry.started) else {
        tracing::warn!(holder = %entry.holder_id, "the holder's process cannot be verified; it is left running");
        return;
    };
    if let Err(err) = end_process(&identity) {
        tracing::warn!(holder = %entry.holder_id, %err, "ending the holder failed; it is left running");
        return;
    }
    registry::remove(registry_dir, &entry.holder_id);
}

/// What [`end_identified`] calls a holder that cannot be linked to as the daemon stops.
const UNREACHABLE_HOLDER: &str = "a holder that cannot be linked to";

/// What [`end_identified`] calls a holder a planned stop handed terminals over from, parted from
/// or left untouched for the next daemon, which an ordinary stop that follows ends.
const LEFT_HOLDER: &str = "a holder left to the next daemon";

/// Ends the holder `holder`, `what` says which, by its process `identity`, with what it runs, as
/// the daemon stops: one whose process cannot be verified is left running. **Blocks** for up to
/// the grace.
fn end_identified(holder: &str, identity: Option<ProcessIdentity>, what: &str) {
    let Some(identity) = identity else {
        tracing::warn!(holder, "{what} cannot be verified; it is left running");
        return;
    };
    tracing::info!(holder, "ending {what}, with what it runs");
    if let Err(err) = end_process(&identity) {
        tracing::warn!(holder, %err, "ending the holder failed; it is left running");
    }
}

/// Parts from every holder of `links` at once, each on a thread of its own, telling it whether
/// the terminals are `handover`: each parting may wait a few seconds for its holder to take it.
/// **Blocks** for the slowest.
fn part_all(links: &[Arc<Link>], handover: bool) {
    std::thread::scope(|scope| {
        for link in links {
            scope.spawn(move || link.part(handover));
        }
    });
}

/// Ends the process `identity` names, a holder, which ends the programs it runs: it is asked to
/// first, and killed if it is still running after [`END_GRACE`]. A holder takes the request
/// (SIGTERM, on Unix) as the daemon's own end, ending every program before it exits. Where the
/// host has no polite request (Windows) it is killed at once, which closes the pseudoconsoles its
/// programs run on, and that ends them. A holder that runs in this very process (a test's) is
/// never signalled: that would end the daemon. **Blocks** for up to the grace.
pub(super) fn end_process(identity: &ProcessIdentity) -> std::io::Result<()> {
    if identity.pid.0 == std::process::id() {
        return Err(std::io::Error::other("the holder runs in this process"));
    }
    match proc::signal_identified(identity, Signal::Polite(Polite::Terminate)) {
        Sent::Delivered => {
            let deadline = Instant::now() + END_GRACE;
            while Instant::now() < deadline {
                if !identity.is_running() {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            tracing::warn!(
                pid = identity.pid.0,
                "a holder asked to end is still running; it is killed"
            );
        }
        Sent::Gone => return Ok(()),
        // The host takes no polite request.
        Sent::Failed(_) => {}
    }
    match proc::signal_identified(identity, Signal::Kill) {
        Sent::Failed(err) => Err(err),
        Sent::Delivered | Sent::Gone => Ok(()),
    }
}

/// Ends the terminals of a holder that is not adopted and has it take no more, so that it exits
/// once they are gone. The daemon's side of the link is closed, which a holder with nothing left
/// to run takes as its cue. A link that drops while the terminals are taken leaves nothing to end
/// them over: the holder `entry` names, which is not to be adopted, is ended by its process
/// instead, as it is when a terminal's end is not confirmed ([`HeldTerminal::end`]). Whether what
/// it ran has ended (every terminal's end was confirmed, or the holder is gone), with the tail
/// each terminal had kept, for the screens no daemon will show again. **Blocks.**
fn refuse(link: &Arc<Link>, registry_dir: &Path, entry: &RegistryEntry) -> Ended {
    let Some(terminals) = link.adopt() else {
        end_by_process(registry_dir, entry);
        return Ended::by_process(holder_gone(registry_dir, &entry.holder_id));
    };
    let mut tails = Vec::with_capacity(terminals.len());
    // A terminal the holder listed that `adopt` left out had its tail missing, or was no longer
    // listed: its end was not confirmed here.
    let mut confirmed = terminals.len() == link.welcome().terminals.len();
    for Adopted {
        info, tail, held, ..
    } in terminals
    {
        confirmed &= held.end();
        tails.push(EndedTail {
            terminal_id: info.id,
            size: info.size,
            tail,
            handover: info.handover,
        });
    }
    link.send(&ToHolder::Drain);
    link.part(false);
    if confirmed {
        return Ended { gone: true, tails };
    }
    // A program whose end is not confirmed may run on in the holder, which the daemon would then
    // publish as gone: the holder is ended by its process, and what it ran is gone only if it is.
    tracing::warn!(holder = %entry.holder_id, "a terminal's end was not confirmed; the holder is ended by its process");
    end_by_process(registry_dir, entry);
    Ended {
        gone: holder_gone(registry_dir, &entry.holder_id),
        tails,
    }
}

/// What a link that ended does when `builds` is set: sweeps the copies of builds once the holder's
/// process is gone, retrying for a while, since the process may still be on its way out and, on
/// Windows, a file of the copy it runs from cannot be deleted until it is.
fn sweep_after_loss(builds: Option<Arc<Builds>>, registry_dir: PathBuf) -> OnLost {
    Arc::new(move |holder_id| {
        let Some(builds) = builds.clone() else {
            return;
        };
        let (registry_dir, holder_id) = (registry_dir.clone(), holder_id.to_string());
        std::thread::spawn(move || {
            for _ in 0..LOSS_SWEEP_TRIES {
                if holder_gone(&registry_dir, &holder_id) && builds.sweep(&registry_dir) {
                    return;
                }
                std::thread::sleep(LOSS_SWEEP_INTERVAL);
            }
        });
    })
}

/// Whether the holder `holder_id` has no process any more: it left no registry file, or its file
/// names a process that does not run, or one whose process cannot be verified has nothing
/// listening on its port.
fn holder_gone(registry_dir: &Path, holder_id: &str) -> bool {
    match registry::read(registry_dir, holder_id) {
        None => true,
        Some(entry) => match ProcessIdentity::parse(&entry.started) {
            Some(identity) => !identity.is_running(),
            None => registry::nothing_listens(entry.port),
        },
    }
}

static INSTALLED: OnceLock<Holders> = OnceLock::new();

/// Has the terminals this process starts run in a holder, from now on: [`current`] returns it.
/// Called once, by the daemon; a process that never calls it runs its terminals itself.
pub fn install(config: ClientConfig) -> Result<()> {
    let holders = Holders::new(config)?;
    INSTALLED
        .set(holders)
        .map_err(|_| anyhow!("the holders were installed already"))
}

/// The holders the terminals of this process run in, if [`install`] was called.
pub fn current() -> Option<&'static Holders> {
    INSTALLED.get()
}

#[cfg(test)]
mod adoption_tests;
#[cfg(test)]
pub(crate) mod test_holders;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use self::test_holders::{
    in_process, in_process_relaying, in_process_with, unresolved, Unavailable,
};
