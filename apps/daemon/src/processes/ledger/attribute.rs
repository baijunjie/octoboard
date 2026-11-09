//! Deciding, from one read of the process table, which launch each process belongs to. Pure over
//! the table and what the ledger already holds, so the rules are tested on tables made up for it.
//!
//! A process P belongs to launch L when, in this order:
//!
//! 1. P is the daemon, one of its ancestors, a holder process, or a direct child of the daemon or
//!    of a holder (an agent among them), or sits in an agent's own terminal session or one led by
//!    such a child: it belongs to no launch here, whatever was decided of it before. These
//!    sessions hold internals whose lifetimes the daemon manages separately. The holders are the
//!    ones the table was given ([`Table::with_holders`]): the programs the daemon starts are
//!    their children.
//! 2. P was attributed before and is still running (same pid, same start): L stays.
//! 3. P's chain of parents, as this read shows it, reaches L's agent or a process of L's.
//! 4. P sits in the terminal session of a kept process, or in one recorded as kept: it is kept
//!    too (see below).
//! 5. P is a member of L's containment, where the host lists one's members (on Windows, the job
//!    of the launch or of a launch whose processes were carried over to L).
//! 6. P's environment carries this instance's tag with L's launch id.
//! 7. P sits in a terminal session recorded for L.
//!
//! The environment is asked for only when rules 1 to 5 decide nothing, so a process they decide
//! never has it read.
//!
//! A tag naming a launch of this instance that is not in the book belongs to a launch that has
//! ended (earlier in this run, or in a previous one); its process goes to [`ENDED`], to be
//! stopped, unless the launch's processes were carried over to another launch that is still
//! followed ([`Book::aliases`]), whose it is then. A tag of another instance is never read as
//! anything.
//!
//! Recording a session for L is sound because a pid is not handed to a new process while a
//! session or a process group still has that id, and every member of a session descends from its
//! leader. A session is recorded only when its leader is gone or is L's, so a tagged process
//! started into someone else's session does not take the whole session with it; a leader running
//! as another user is not gone, though the table leaves it out. A recorded session is dropped
//! once it has no member, or once a process with its id leads it that did not before. This is
//! what reaches an Apple platform binary (`/bin/sleep`, `/usr/bin/tail`), whose environment
//! cannot be read with SIP on, after the shell that started it exited and it was reparented.
//!
//! That a session still has members proves it is the one recorded only while the table is read
//! often enough that it cannot empty and have its id handed on between two reads. A session read
//! back from a previous run's ledger file has no such proof: once all its members exited, an
//! unrelated process could have been given its id, started a session and left members in it with
//! no leader. So it is kept only when the first read proves it ([`prove_read_back`]): its leader
//! is running with the recorded start time, or a process the file named is running in it. Such a
//! process never left the session, since a process joins another only by starting one of its own,
//! with its own pid as the id, and a pid that names a session is not handed to a new process.
//!
//! Kept processes and their descendants are never attributed: a process whose chain reaches a
//! kept one is kept itself, and so is a process found in a kept process's session with no other
//! claim on it, which is how a kept process's child is still found once its chain is broken. The
//! session of a kept process is recorded as kept by the same rule as a launch's, so it still
//! keeps what is in it once the kept process itself has exited.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::table::{Identity, Proc, Protected, Table};
use crate::platform::proc::{ContainerId, Containment, ProcessId};

/// The launch id under which the processes of launches that have ended are held until they are
/// stopped. Not a UUID, so no launch has it.
pub(crate) const ENDED: &str = "ended";

/// One agent launch the ledger follows.
#[derive(Debug, Clone, Default)]
pub(crate) struct Launch {
    /// The session the launch runs, or `None` for [`ENDED`].
    pub session_id: Option<String>,
    /// The agent's process has been started: until then the launch id is only reserved, so a
    /// process carrying it is not taken for an ended launch's.
    pub started: bool,
    /// The agent's process, once it is started and while its start time could be read.
    pub agent: Option<Identity>,
    /// Its processes are being stopped, or carried over (see [`Self::heir`]).
    pub ending: bool,
    /// The session whose launch takes the launch's processes over when its agent process ends,
    /// instead of their being stopped: the session that takes this one's place in a handoff, or
    /// this one itself for an account switch's relaunch.
    pub heir: Option<String>,
    /// The agent process has ended and its processes were carried over to `session_id`, which has
    /// no launch running yet: they are held for its next launch to take them in while a hold for
    /// the session is open ([`super::HeldForLaunch`]), and stopped once none is.
    pub held: bool,
    /// The terminal sessions recorded for the launch, each with its leader's start time when the
    /// leader was running when it was recorded.
    pub sessions: Sessions,
    /// What tells the launch's processes apart where the host lists them (see
    /// [`Containment::members`]): the launch's own, and those of the launches whose processes
    /// were carried over to it. Empty for a launch whose host lists none, which is told by tag.
    pub containments: Vec<Arc<Containment>>,
}

/// Terminal sessions by id, each with its leader's start time when the leader was running when
/// it was recorded.
pub(crate) type Sessions = HashMap<ContainerId, Option<u64>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attributed {
    pub launch: String,
    /// The terminal session the process was in when it was attributed, where the host has them.
    pub sid: Option<ContainerId>,
}

/// What the ledger knows: the launches, the processes attributed to them, and the kept ones.
#[derive(Debug)]
pub(crate) struct Book {
    pub launches: HashMap<String, Launch>,
    pub attributed: HashMap<Identity, Attributed>,
    pub kept: HashSet<Identity>,
    /// The terminal sessions of kept processes, recorded as a launch's are.
    pub kept_sessions: Sessions,
    /// The project each kept process is listed under, as an unowned tree of its (see
    /// [`super::ProcessLedger::trees_of_project`]): the project of the session whose tree it was
    /// kept from, or of the directory it was left behind in, passed on to what descends from it
    /// or is found in its terminal session. A kept process with none is listed nowhere.
    pub kept_projects: HashMap<Identity, String>,
    /// The project of each kept terminal session, as [`Self::kept_projects`].
    pub kept_session_projects: HashMap<ContainerId, String>,
    /// The launches that ended with their processes carried over to another, by launch id, each
    /// with the launch now holding them, so that a process found later carrying an earlier
    /// launch's tag is the current one's. Forgotten with the launch they point to.
    pub aliases: HashMap<String, String>,
    /// The sessions whose next launch takes in what is held for them, with how many holds are
    /// open for each ([`super::HeldForLaunch`]).
    pub awaited: HashMap<String, usize>,
    /// The earliest start of any agent the book has followed: no process that started before it
    /// can carry a tag of this run's launches, so no earlier environment is read. `None` until
    /// the first agent starts.
    pub horizon: Option<u64>,
    /// The processes the containments of the launches list as members, with the launch each is
    /// of, as the last read of the table found them ([`Self::refresh_contained`]).
    pub contained: HashMap<ProcessId, String>,
    /// The terminal sessions read back from a previous run's ledger file, not yet proven to be
    /// the ones it recorded (see [`Self::read_back_sessions`]).
    pub read_back: ReadBack,
    /// Something a restart would need changed since the ledger file was last written: a launch
    /// started or ended, a process attributed or kept, or a session recorded. Every process
    /// attributed is one, since a restart takes a session back only when a process the file
    /// names is still in it. A process going is not.
    pub dirty: bool,
}

/// The terminal sessions a reconcile read back from a previous run's ledger file, recorded again
/// in the book, and the processes that file named, which prove them (see
/// [`Book::read_back_sessions`]).
#[derive(Debug, Default)]
pub(crate) struct ReadBack {
    /// Those recorded for [`ENDED`].
    pub ended: HashSet<ContainerId>,
    /// Those recorded as kept.
    pub kept: HashSet<ContainerId>,
    /// The processes the file named, attributed or kept.
    pub named: HashSet<Identity>,
}

impl Default for Book {
    fn default() -> Self {
        Self {
            launches: HashMap::from([(ENDED.to_string(), Launch::default())]),
            attributed: HashMap::new(),
            kept: HashSet::new(),
            kept_sessions: Sessions::new(),
            kept_projects: HashMap::new(),
            kept_session_projects: HashMap::new(),
            aliases: HashMap::new(),
            awaited: HashMap::new(),
            horizon: None,
            contained: HashMap::new(),
            read_back: ReadBack::default(),
            dirty: false,
        }
    }
}

impl Book {
    /// The processes attributed to `launch` that `table` shows running.
    pub fn members_of(&self, launch: &str, table: &Table) -> Vec<Identity> {
        self.attributed
            .iter()
            .filter(|(id, attributed)| attributed.launch == launch && table.has(id))
            .map(|(id, _)| *id)
            .collect()
    }

    fn agent_launch(&self, id: &Identity) -> Option<&str> {
        self.launches
            .iter()
            .find(|(_, launch)| launch.agent.as_ref() == Some(id))
            .map(|(launch_id, _)| launch_id.as_str())
    }

    fn agent_pids(&self) -> HashSet<ProcessId> {
        self.launches
            .values()
            .filter_map(|launch| launch.agent.map(|agent| agent.pid))
            .collect()
    }

    /// Reads the members of every launch's containments, for the host that lists them, and drops
    /// the containments of ended launches that have none left.
    pub fn refresh_contained(&mut self) {
        self.contained.clear();
        for (launch_id, launch) in &mut self.launches {
            let ended = launch_id == ENDED;
            launch.containments.retain(|containment| {
                let members = containment.members();
                let listed = members.as_ref().is_some_and(|members| !members.is_empty());
                self.contained.extend(
                    members
                        .into_iter()
                        .flatten()
                        .map(|pid| (pid, launch_id.clone())),
                );
                listed || !ended
            });
        }
    }

    /// The launch id `launch` reads as in the book: itself, the launch its processes were carried
    /// over to, or `None` for a launch that is no longer followed.
    pub fn followed_as<'a>(&'a self, launch: &'a str) -> Option<&'a str> {
        if self.launches.contains_key(launch) {
            return Some(launch);
        }
        self.aliases
            .get(launch)
            .map(String::as_str)
            .filter(|to| self.launches.contains_key(*to))
    }

    /// The session the processes of `launch_id` are carried over to when it ends: the one it was
    /// marked for, or its own while a hold for its next launch is open (see
    /// [`super::ProcessLedger::end_launch`]).
    pub fn heir_of(&self, launch_id: &str) -> Option<String> {
        let launch = self.launches.get(launch_id)?;
        launch.heir.clone().or_else(|| {
            launch
                .session_id
                .clone()
                .filter(|session| self.awaited.contains_key(session))
        })
    }

    /// Makes everything attributed to `launch_id`, and the terminal sessions recorded for it, the
    /// user's, listed under the project `project`, and stops following the launch. Returns the
    /// processes it kept, which the caller moves out of the containments
    /// ([`super::ProcessLedger::release_kept`]). The launch's containments that the host can read
    /// and release members from are handed to [`ENDED`] as [`Self::hand_to_ended`] does, rather
    /// than dropped with processes in them.
    pub fn keep_launch(&mut self, launch_id: &str, project: &str) -> Vec<Identity> {
        if launch_id == ENDED {
            return Default::default();
        }
        let Some(launch) = self.launches.remove(launch_id) else {
            return Default::default();
        };
        self.aliases.retain(|_, to| to != launch_id);
        let members: Vec<Identity> = self
            .attributed
            .iter()
            .filter(|(_, attributed)| attributed.launch == launch_id)
            .map(|(id, _)| *id)
            .collect();
        for id in &members {
            self.attributed.remove(id);
            self.kept.insert(*id);
            self.kept_projects.insert(*id, project.to_string());
        }
        for (sid, leader_start) in launch.sessions {
            self.kept_sessions.entry(sid).or_insert(leader_start);
            self.kept_session_projects
                .entry(sid)
                .or_insert_with(|| project.to_string());
        }
        self.hand_containments_to_ended(
            launch
                .containments
                .into_iter()
                .filter(|containment| containment.releases_members()),
        );
        self.dirty = true;
        members
    }

    /// Hands the containments that the host can still read to [`ENDED`], which holds them until
    /// nothing is left in them.
    fn hand_containments_to_ended(&mut self, containments: impl Iterator<Item = Arc<Containment>>) {
        if let Some(ended) = self.launches.get_mut(ENDED) {
            ended
                .containments
                .extend(containments.filter(|containment| containment.members().is_some()));
        }
    }

    /// The launch of `session_id` that processes are carried over to: one whose agent has
    /// started and that is neither ending nor held, other than `except`.
    pub fn running_launch_of(&self, session_id: &str, except: &str) -> Option<String> {
        self.launches
            .iter()
            .find(|(id, launch)| {
                id.as_str() != except
                    && launch.started
                    && !launch.ending
                    && !launch.held
                    && launch.session_id.as_deref() == Some(session_id)
            })
            .map(|(id, _)| id.clone())
    }

    /// Moves processes attributed to the launch `from` to the launch `to`: `members` of them, or,
    /// with `None`, all of them, the terminal sessions recorded for `from` and the launch itself,
    /// which stops being followed and reads as `to` from then on ([`Self::aliases`]). A terminal
    /// session recorded for `from` goes with `members` when one of them sits in it and, in
    /// `table`, no process `from` keeps does. The one reassignment of processes between launches:
    /// a carry-over moves all of a launch, a transfer of process trees some of it.
    pub fn transfer(
        &mut self,
        from: &str,
        to: &str,
        members: Option<(&HashSet<Identity>, &Table)>,
    ) {
        if from == to || from == ENDED || !self.launches.contains_key(to) {
            return;
        }
        let mut moved_sessions = Sessions::new();
        match members {
            None => {
                for attributed in self.attributed.values_mut() {
                    if attributed.launch == from {
                        attributed.launch = to.to_string();
                    }
                }
                if let Some(launch) = self.launches.remove(from) {
                    moved_sessions = launch.sessions;
                    if let Some(heir) = self.launches.get_mut(to) {
                        heir.containments.extend(launch.containments);
                    }
                }
                for target in self.aliases.values_mut() {
                    if target == from {
                        *target = to.to_string();
                    }
                }
                self.aliases.insert(from.to_string(), to.to_string());
            }
            Some((members, table)) => {
                let mut sids: HashSet<ContainerId> = HashSet::new();
                for (id, attributed) in self.attributed.iter_mut() {
                    if attributed.launch == from && members.contains(id) {
                        attributed.launch = to.to_string();
                        sids.extend(
                            table
                                .get(id.pid)
                                .filter(|proc| proc.id == *id)
                                .and_then(|proc| proc.sid),
                        );
                    }
                }
                let staying: HashSet<ContainerId> = self
                    .attributed
                    .iter()
                    .filter(|(_, attributed)| attributed.launch == from)
                    .filter_map(|(id, _)| table.get(id.pid).filter(|proc| proc.id == *id))
                    .filter_map(|proc| proc.sid)
                    .collect();
                if let Some(launch) = self.launches.get_mut(from) {
                    launch.sessions.retain(|sid, leader_start| {
                        let goes = sids.contains(sid) && !staying.contains(sid);
                        if goes {
                            moved_sessions.insert(*sid, *leader_start);
                        }
                        !goes
                    });
                }
            }
        }
        if let Some(launch) = self.launches.get_mut(to) {
            for (sid, leader_start) in moved_sessions {
                launch.sessions.entry(sid).or_insert(leader_start);
            }
        }
        self.dirty = true;
    }

    /// Records the terminal sessions a previous run's ledger file recorded for its launches, for
    /// [`ENDED`], and as kept, each with its leader's start time as the file has it, where the
    /// book has none of that id yet; `named` are the processes the file names, attributed or kept.
    /// The next read of the table drops each of them unless it proves it is still the session
    /// recorded: its leader running with that start time, or one of `named` running in it.
    pub fn read_back_sessions(
        &mut self,
        ended: impl IntoIterator<Item = (ContainerId, Option<u64>)>,
        kept: impl IntoIterator<Item = (ContainerId, Option<u64>)>,
        named: impl IntoIterator<Item = Identity>,
    ) {
        if let Some(launch) = self.launches.get_mut(ENDED) {
            for (sid, leader_start) in ended {
                if let Entry::Vacant(entry) = launch.sessions.entry(sid) {
                    entry.insert(leader_start);
                    self.read_back.ended.insert(sid);
                }
            }
        }
        for (sid, leader_start) in kept {
            if let Entry::Vacant(entry) = self.kept_sessions.entry(sid) {
                entry.insert(leader_start);
                self.read_back.kept.insert(sid);
            }
        }
        self.read_back.named.extend(named);
    }

    /// Hands what is attributed to `launch_id`, with the sessions recorded for it, to [`ENDED`],
    /// to be stopped, and stops following the launch.
    pub fn hand_to_ended(&mut self, launch_id: &str) {
        if launch_id == ENDED {
            return;
        }
        self.aliases.retain(|_, to| to != launch_id);
        let (sessions, containments) = self
            .launches
            .remove(launch_id)
            .map(|launch| (launch.sessions, launch.containments))
            .unwrap_or_default();
        let mut moved = !sessions.is_empty();
        for attributed in self.attributed.values_mut() {
            if attributed.launch == launch_id {
                attributed.launch = ENDED.to_string();
                moved = true;
            }
        }
        if let Some(ended) = self.launches.get_mut(ENDED) {
            for (sid, leader_start) in sessions {
                ended.sessions.entry(sid).or_insert(leader_start);
            }
        }
        self.hand_containments_to_ended(containments.into_iter());
        self.dirty |= moved;
    }
}

/// Whether `proc` is never any launch's (rule 1).
fn never_attributed(
    book: &Book,
    protected: &Protected,
    agent_pids: &HashSet<ProcessId>,
    proc: &Proc,
) -> bool {
    protected.covers(proc)
        || proc
            .sid
            .is_some_and(|sid| agent_pids.contains(&sid.leader()))
        || book.agent_launch(&proc.id).is_some()
}

/// The sessions of the kept processes running in `table`, and those recorded as kept.
fn kept_sessions(book: &Book, table: &Table) -> HashSet<ContainerId> {
    table
        .procs()
        .filter(|proc| book.kept.contains(&proc.id))
        .filter_map(|proc| proc.sid)
        .filter(|sid| sid.0 > 1)
        .chain(book.kept_sessions.keys().copied())
        .collect()
}

/// What the attribution of one process came to.
enum Verdict {
    Launch(String),
    Kept,
}

/// The processes of `table` whose environment [`attribute`] may ask for: those that started at
/// or after `horizon` and that nothing else decides as `book` stands. What it asks for later is
/// a subset of these unless the book changes in between.
pub(crate) fn tag_candidates<'a>(
    book: &Book,
    table: &'a Table,
    own: ProcessId,
    horizon: Option<u64>,
) -> Vec<&'a Proc> {
    let Some(horizon) = horizon else {
        return Vec::new();
    };
    let protected = table.protected(own);
    let agent_pids = book.agent_pids();
    let kept_sessions = kept_sessions(book, table);
    table
        .procs()
        .filter(|proc| {
            proc.id.start_micros >= horizon
                && !book.kept.contains(&proc.id)
                && !book.contained.contains_key(&proc.id.pid)
                && !book.attributed.contains_key(&proc.id)
                && !never_attributed(book, &protected, &agent_pids, proc)
                && !proc.sid.is_some_and(|sid| kept_sessions.contains(&sid))
                && !table
                    .ancestors(proc)
                    .any(|ancestor| book.kept.contains(&ancestor.id))
                && by_chain(book, table, proc).is_none()
        })
        .collect()
}

/// Brings `book` up to date with `table`: forgets the processes that are gone, keeps what descends
/// from kept processes, drops sessions that are not, or are not shown to be, the ones recorded,
/// and attributes every process the rules give a launch. `own` is the daemon's pid. `tag_of`
/// answers the launch id in a process's tag when the tag is this instance's, and `None` otherwise
/// or when the environment cannot be read; it is asked only of a process rules 1 to 5 leave
/// undecided.
pub(crate) fn attribute(
    book: &mut Book,
    table: &Table,
    own: ProcessId,
    tag_of: &mut dyn FnMut(&Proc) -> Option<String>,
) {
    prove_read_back(book, table);
    forget_gone(book, table);
    let order = table.by_start();
    let protected = table.protected(own);
    let agent_pids = book.agent_pids();
    // Internals kept by an earlier listing must not pass that choice to new tool commands.
    let kept_count = book.kept.len()
        + book.kept_projects.len()
        + book.kept_sessions.len()
        + book.kept_session_projects.len();
    for proc in &order {
        if never_attributed(book, &protected, &agent_pids, proc) {
            book.kept.remove(&proc.id);
            book.kept_projects.remove(&proc.id);
        }
    }
    book.kept_sessions
        .retain(|sid, _| !agent_pids.contains(&sid.leader()) && !protected.covers_session(*sid));
    book.kept_session_projects
        .retain(|sid, _| !agent_pids.contains(&sid.leader()) && !protected.covers_session(*sid));
    book.dirty |= kept_count
        != book.kept.len()
            + book.kept_projects.len()
            + book.kept_sessions.len()
            + book.kept_session_projects.len();
    for proc in &order {
        if book.kept.contains(&proc.id) {
            continue;
        }
        if let Some(kept) = table
            .ancestors(proc)
            .find(|ancestor| book.kept.contains(&ancestor.id))
        {
            let project = book.kept_projects.get(&kept.id).cloned();
            keep(book, table, proc, project);
        }
    }
    check_sessions(book, table);

    // Before anything attributed earlier is taken as it stands: a scan that ran before the
    // agent's launch was known may have attributed the agent or its internals.
    for proc in &order {
        if never_attributed(book, &protected, &agent_pids, proc) {
            book.attributed.remove(&proc.id);
        }
    }
    for launch in book.launches.values_mut() {
        launch.sessions.retain(|sid, _| {
            !agent_pids.contains(&sid.leader()) && !protected.covers_session(*sid)
        });
    }

    let mut tags: HashMap<Identity, Option<String>> = HashMap::new();
    // A pass may attribute a process another one's rule depends on (a session recorded, a parent
    // attributed), and processes that started together may come in either order, so passes
    // repeat while they attribute anything.
    for _ in 0..4 {
        let mut changed = false;
        let kept_sessions = kept_sessions(book, table);
        for proc in &order {
            if book.kept.contains(&proc.id)
                || book.attributed.contains_key(&proc.id)
                || never_attributed(book, &protected, &agent_pids, proc)
            {
                continue;
            }
            let verdict = by_chain(book, table, proc)
                .map(Verdict::Launch)
                .or_else(|| {
                    proc.sid
                        .is_some_and(|sid| kept_sessions.contains(&sid))
                        .then_some(Verdict::Kept)
                })
                .or_else(|| {
                    book.contained
                        .get(&proc.id.pid)
                        .cloned()
                        .map(Verdict::Launch)
                })
                .or_else(|| {
                    tags.entry(proc.id)
                        .or_insert_with(|| tag_of(proc))
                        .clone()
                        .map(|launch| {
                            Verdict::Launch(book.followed_as(&launch).unwrap_or(ENDED).to_string())
                        })
                })
                .or_else(|| by_session(book, proc).map(Verdict::Launch));
            match verdict {
                Some(Verdict::Launch(launch)) => {
                    book.attributed.insert(
                        proc.id,
                        Attributed {
                            launch: launch.clone(),
                            sid: proc.sid,
                        },
                    );
                    record_session(book, table, &launch, proc);
                    book.dirty = true;
                }
                Some(Verdict::Kept) => {
                    let project = proc
                        .sid
                        .and_then(|sid| kept_session_project(book, table, sid));
                    keep(book, table, proc, project);
                }
                None => continue,
            }
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

/// Forgets the attributed and kept processes `table` no longer shows running. Not a change to
/// write down: what the file says of a process that is gone is never acted on.
fn forget_gone(book: &mut Book, table: &Table) {
    book.attributed.retain(|id, _| table.has(id));
    book.kept.retain(|id| table.has(id));
    book.kept_projects.retain(|id, _| table.has(id));
}

/// Drops each session read back from a previous run's file (see [`Book::read_back_sessions`])
/// that `table` does not prove is still the one recorded: neither is its leader running with the
/// recorded start time, nor does a process the file named sit in it. Done at the first read after
/// the reconcile only — by [`attribute`], and by a scan before it asks which environments to
/// read — since the reads after it follow the session.
pub(crate) fn prove_read_back(book: &mut Book, table: &Table) {
    let read_back = std::mem::take(&mut book.read_back);
    let named_in: HashSet<ContainerId> = table
        .procs()
        .filter(|proc| read_back.named.contains(&proc.id))
        .filter_map(|proc| proc.sid)
        .collect();
    let proven = |sid: ContainerId, leader_start: Option<u64>| {
        named_in.contains(&sid)
            || table.get(sid.leader()).is_some_and(|leader| {
                leader.sid == Some(sid) && leader_start == Some(leader.id.start_micros)
            })
    };
    if let Some(ended) = book.launches.get_mut(ENDED) {
        ended.sessions.retain(|sid, leader_start| {
            !read_back.ended.contains(sid) || proven(*sid, *leader_start)
        });
    }
    book.kept_sessions
        .retain(|sid, leader_start| !read_back.kept.contains(sid) || proven(*sid, *leader_start));
}

/// Drops each recorded session that no process sits in any more, or whose id a process now leads
/// that did not lead it when it was recorded: the id may since have been handed to another.
fn check_sessions(book: &mut Book, table: &Table) {
    let occupied: HashSet<ContainerId> = table.procs().filter_map(|proc| proc.sid).collect();
    let still_recorded = |sid: &ContainerId, leader_start: &mut Option<u64>| {
        if !occupied.contains(sid) {
            return false;
        }
        match table
            .get(sid.leader())
            .filter(|leader| leader.sid == Some(*sid))
        {
            Some(leader) => *leader_start == Some(leader.id.start_micros),
            None => !table.led_by_another_user(*sid),
        }
    };
    for launch in book.launches.values_mut() {
        launch.sessions.retain(still_recorded);
    }
    book.kept_sessions.retain(still_recorded);
    let kept_sessions = &book.kept_sessions;
    book.kept_session_projects
        .retain(|sid, _| kept_sessions.contains_key(sid));
}

/// The launch `proc`'s chain of parents reaches: its agent, or a process attributed to it.
fn by_chain(book: &Book, table: &Table, proc: &Proc) -> Option<String> {
    table.ancestors(proc).find_map(|ancestor| {
        book.agent_launch(&ancestor.id)
            .map(str::to_string)
            .or_else(|| {
                book.attributed
                    .get(&ancestor.id)
                    .map(|attributed| attributed.launch.clone())
            })
    })
}

/// The launch that `proc`'s terminal session is recorded for.
fn by_session(book: &Book, proc: &Proc) -> Option<String> {
    let sid = proc.sid?;
    book.launches
        .iter()
        .find(|(_, launch)| launch.sessions.contains_key(&sid))
        .map(|(launch_id, _)| launch_id.clone())
}

/// `proc`'s terminal session as one to record beside it — with its leader's start time when the
/// leader is running — when `ours` says the leader may be recorded with it: unless the leader is
/// running and is neither `proc` nor such a process, or runs as another user.
fn session_to_record(
    table: &Table,
    proc: &Proc,
    ours: impl FnOnce(&Identity) -> bool,
) -> Option<(ContainerId, Option<u64>)> {
    let sid = proc.sid.filter(|sid| sid.0 > 1)?;
    match table
        .get(sid.leader())
        .filter(|leader| leader.sid == Some(sid))
    {
        Some(leader) if leader.id == proc.id || ours(&leader.id) => {
            Some((sid, Some(leader.id.start_micros)))
        }
        Some(_) => None,
        None if table.led_by_another_user(sid) => None,
        None => Some((sid, None)),
    }
}

/// Records `proc`'s terminal session for `launch_id`, which `proc` was just attributed to, unless
/// its leader is running and is not `launch_id`'s.
fn record_session(book: &mut Book, table: &Table, launch_id: &str, proc: &Proc) {
    let attributed = &book.attributed;
    let Some((sid, leader_start)) = session_to_record(table, proc, |leader| {
        attributed
            .get(leader)
            .is_some_and(|attributed| attributed.launch == launch_id)
    }) else {
        return;
    };
    if let Some(launch) = book.launches.get_mut(launch_id) {
        launch.sessions.entry(sid).or_insert(leader_start);
    }
}

/// Makes `proc` the user's, listed under `project` (see [`Book::kept_projects`]): attributed to
/// no launch, and its terminal session recorded as kept.
pub(crate) fn keep(book: &mut Book, table: &Table, proc: &Proc, project: Option<String>) {
    book.kept.insert(proc.id);
    book.attributed.remove(&proc.id);
    if let Some(project) = project {
        book.kept_projects.insert(proc.id, project);
    }
    record_kept_session(book, table, proc);
    book.dirty = true;
}

/// The project of the kept terminal session `sid`: as recorded for it, or else of a kept process
/// running in it.
fn kept_session_project(book: &Book, table: &Table, sid: ContainerId) -> Option<String> {
    book.kept_session_projects.get(&sid).cloned().or_else(|| {
        table
            .procs()
            .filter(|proc| proc.sid == Some(sid))
            .find_map(|proc| book.kept_projects.get(&proc.id).cloned())
    })
}

/// Records `proc`'s terminal session as kept, `proc` having just been kept, unless its leader is
/// running and is not kept, with `proc`'s project when the session has none yet.
pub(crate) fn record_kept_session(book: &mut Book, table: &Table, proc: &Proc) {
    let kept = &book.kept;
    if let Some((sid, leader_start)) =
        session_to_record(table, proc, |leader| kept.contains(leader))
    {
        book.kept_sessions.entry(sid).or_insert(leader_start);
        if let Some(project) = book.kept_projects.get(&proc.id) {
            book.kept_session_projects
                .entry(sid)
                .or_insert_with(|| project.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCH: &str = "launch";

    /// The daemon's pid in these tables: not in them, and no process's parent unless a test says
    /// so.
    const OWN: ProcessId = ProcessId(99);

    fn sid(id: u32) -> ContainerId {
        ContainerId(id)
    }

    fn proc(pid: i32, start: u64, ppid: i32, sid: i32) -> Proc {
        Proc {
            id: Identity {
                pid: ProcessId(pid as u32),
                start_micros: start,
            },
            ppid: ProcessId(ppid as u32),
            sid: Some(ContainerId(sid as u32)),
            comm: String::new(),
        }
    }

    /// A book following one launch whose agent is pid 100, started at 1000, leading session 100.
    fn book() -> Book {
        let mut book = Book::default();
        book.launches.insert(
            LAUNCH.to_string(),
            Launch {
                session_id: Some("s".to_string()),
                started: true,
                agent: Some(Identity {
                    pid: ProcessId(100),
                    start_micros: 1000,
                }),
                ..Launch::default()
            },
        );
        book
    }

    fn agent() -> Proc {
        proc(100, 1000, 1, 100)
    }

    fn untagged(_: &Proc) -> Option<String> {
        None
    }

    fn launch_of(book: &Book, pid: i32) -> Option<&str> {
        book.attributed
            .iter()
            .find(|(id, _)| id.pid.0 as i32 == pid)
            .map(|(_, attributed)| attributed.launch.as_str())
    }

    /// The agent's own session is skipped, a shell that started a session of its own under it is
    /// the launch's, and so is what that shell started, which stays the launch's once the shell
    /// has exited and it was reparented.
    ///
    /// None of them has its environment asked for: the chain decides each of them first.
    #[test]
    fn a_shell_in_a_session_of_its_own_and_what_it_left_belong_to_the_launch() {
        let mut book = book();
        let table = Table::from_procs([
            agent(),
            proc(101, 1001, 100, 100),
            proc(200, 1002, 100, 200),
            proc(201, 1003, 200, 200),
        ]);
        let mut asked = Vec::new();
        attribute(&mut book, &table, OWN, &mut |proc: &Proc| {
            asked.push(proc.id.pid.0 as i32);
            None
        });
        assert_eq!(asked, Vec::<i32>::new());
        assert_eq!(launch_of(&book, 101), None, "the agent's own session");
        assert_eq!(launch_of(&book, 100), None, "the agent itself");
        assert_eq!(launch_of(&book, 200), Some(LAUNCH));
        assert_eq!(launch_of(&book, 201), Some(LAUNCH));

        let table = Table::from_procs([agent(), proc(201, 1003, 1, 200)]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert_eq!(launch_of(&book, 201), Some(LAUNCH));
        assert_eq!(launch_of(&book, 200), None, "gone, so forgotten");
    }

    /// A process first seen after the shell that started it exited — reparented, its environment
    /// unreadable — is the launch's through the session the shell was recorded in, while another
    /// session it was never seen in reaches nothing.
    #[test]
    fn a_process_first_seen_after_its_shell_exited_is_adopted_through_the_session() {
        let mut book = book();
        let table = Table::from_procs([
            agent(),
            proc(200, 1002, 100, 200),
            proc(201, 1003, 200, 200),
        ]);
        attribute(&mut book, &table, OWN, &mut untagged);
        let table = Table::from_procs([
            agent(),
            proc(201, 1003, 1, 200),
            proc(202, 1004, 1, 200),
            proc(300, 1005, 1, 300),
        ]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert_eq!(launch_of(&book, 202), Some(LAUNCH));
        assert_eq!(launch_of(&book, 300), None);
    }

    /// `proc` on a host that has no terminal sessions to group processes by.
    fn sessionless(pid: i32, start: u64, ppid: i32) -> Proc {
        Proc {
            sid: None,
            ..proc(pid, start, ppid, 0)
        }
    }

    /// Where the host lists a launch's members, a member the chain no longer reaches (its parent
    /// gone) is the launch's by the list, without its environment asked for; a process the list
    /// leaves out is not.
    #[test]
    fn a_member_the_containment_lists_is_the_launchs_without_the_chain() {
        let mut book = book();
        book.contained.insert(ProcessId(300), LAUNCH.to_string());
        let table = Table::from_procs([
            sessionless(100, 1000, 1),
            sessionless(300, 1005, 250),
            sessionless(301, 1006, 1),
        ]);
        let mut asked = Vec::new();
        attribute(&mut book, &table, OWN, &mut |proc: &Proc| {
            asked.push(proc.id.pid.0 as i32);
            None
        });
        assert_eq!(launch_of(&book, 300), Some(LAUNCH));
        assert_eq!(launch_of(&book, 301), None);
        assert_eq!(asked, vec![301]);
    }

    /// A parent's pid that a process started before the child holds now is not the child's
    /// parent: on a host that never reparents, the pid passed to that process once the child's
    /// parent exited.
    #[test]
    fn a_process_that_started_after_the_child_is_not_its_parent() {
        let mut book = book();
        let table = Table::from_procs([
            sessionless(100, 1000, 1),
            sessionless(200, 999, 100),
            sessionless(201, 1001, 100),
        ]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert_eq!(launch_of(&book, 200), None);
        assert_eq!(launch_of(&book, 201), Some(LAUNCH));
    }

    /// A process whose pid was attributed before but that started at another time is a new
    /// process: it is not the launch's, and neither is a session its pid leads now.
    #[test]
    fn a_reused_pid_is_not_taken_for_the_process_that_had_it() {
        let mut book = book();
        let table = Table::from_procs([agent(), proc(200, 1002, 100, 200)]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert_eq!(launch_of(&book, 200), Some(LAUNCH));
        let table =
            Table::from_procs([agent(), proc(200, 5000, 1, 200), proc(201, 5001, 200, 200)]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert_eq!(launch_of(&book, 200), None);
        assert_eq!(launch_of(&book, 201), None);
        assert!(book.launches[LAUNCH].sessions.is_empty());
    }

    /// A tag reaches a process outside the chain and every session; a tag of a launch the book
    /// does not follow is an ended launch's; a tag of another instance never reaches the book,
    /// since `tag_of` answers only this instance's.
    #[test]
    fn a_tag_attributes_a_process_and_one_of_an_ended_launch_is_held_to_be_stopped() {
        let mut book = book();
        let table = Table::from_procs([
            agent(),
            proc(400, 1010, 1, 400),
            proc(401, 1011, 1, 401),
            proc(402, 1012, 1, 402),
        ]);
        let mut tag_of = |proc: &Proc| match proc.id.pid.0 {
            400 => Some(LAUNCH.to_string()),
            401 => Some("gone-launch".to_string()),
            _ => None,
        };
        attribute(&mut book, &table, OWN, &mut tag_of);
        assert_eq!(launch_of(&book, 400), Some(LAUNCH));
        assert_eq!(launch_of(&book, 401), Some(ENDED));
        assert_eq!(launch_of(&book, 402), None);
    }

    /// A tagged process started into a session whose running leader is not the launch's does not
    /// take that session along; one whose leader is gone does.
    #[test]
    fn a_tagged_member_brings_its_session_only_when_the_leader_is_gone() {
        let mut book = book();
        let table = Table::from_procs([
            agent(),
            proc(500, 900, 1, 500),
            proc(501, 1010, 1, 500),
            proc(502, 1011, 500, 500),
            proc(601, 1012, 1, 600),
            proc(602, 1013, 1, 600),
        ]);
        let mut tag_of = |proc: &Proc| matches!(proc.id.pid.0, 501 | 601).then(|| LAUNCH.into());
        attribute(&mut book, &table, OWN, &mut tag_of);
        assert_eq!(launch_of(&book, 501), Some(LAUNCH));
        assert_eq!(
            launch_of(&book, 502),
            None,
            "the session of a leader not ours"
        );
        assert_eq!(launch_of(&book, 601), Some(LAUNCH));
        assert_eq!(
            launch_of(&book, 602),
            Some(LAUNCH),
            "a session whose leader is gone"
        );
    }

    /// A kept process's descendants are kept and never attributed, a process reparented out of a
    /// kept process's session is kept too, and the rest of the launch is attributed as usual.
    #[test]
    fn what_descends_from_a_kept_process_is_kept() {
        let mut book = book();
        let table = Table::from_procs([
            agent(),
            proc(200, 1002, 100, 200),
            proc(201, 1003, 200, 200),
            proc(700, 1004, 100, 700),
        ]);
        attribute(&mut book, &table, OWN, &mut untagged);
        let kept = Identity {
            pid: ProcessId(201),
            start_micros: 1003,
        };
        book.attributed.remove(&kept);
        book.kept.insert(kept);
        let table = Table::from_procs([
            agent(),
            proc(200, 1002, 100, 200),
            proc(201, 1003, 200, 200),
            proc(202, 1005, 201, 200),
            proc(203, 1006, 1, 200),
            proc(700, 1004, 100, 700),
            proc(701, 1007, 700, 700),
        ]);
        let mut tag_of = |_: &Proc| Some(LAUNCH.to_string());
        attribute(&mut book, &table, OWN, &mut tag_of);
        for pid in [202, 203] {
            assert_eq!(launch_of(&book, pid), None, "{pid}");
            assert!(book.kept.iter().any(|id| id.pid.0 as i32 == pid), "{pid}");
        }
        assert_eq!(launch_of(&book, 200), Some(LAUNCH));
        assert_eq!(launch_of(&book, 701), Some(LAUNCH));
    }

    /// The daemon's children and their sessions are excluded even before a launch is known.
    /// An agent registered afterwards also clears any earlier attribution of the agent.
    #[test]
    fn the_agent_its_session_and_the_daemon_with_its_kin_are_never_attributed() {
        let mut book = book();
        let agent_id = agent().id;
        book.launches.get_mut(LAUNCH).unwrap().agent = None;
        let table = Table::from_procs([
            proc(98, 900, 1, 98),
            proc(OWN.0 as i32, 901, 98, 98),
            proc(100, 1000, OWN.0 as i32, 100),
            proc(101, 1001, 100, 100),
            proc(300, 1002, OWN.0 as i32, 300),
        ]);
        let mut tagged = |_: &Proc| Some(LAUNCH.to_string());
        attribute(&mut book, &table, OWN, &mut tagged);
        assert_eq!(launch_of(&book, 98), None, "the daemon's parent");
        assert_eq!(launch_of(&book, OWN.0 as i32), None, "the daemon");
        assert_eq!(launch_of(&book, 100), None, "the daemon's child");
        assert_eq!(launch_of(&book, 300), None, "the daemon's child");
        assert_eq!(launch_of(&book, 101), None, "in the daemon child's session");

        let launch = book.launches.get_mut(LAUNCH).unwrap();
        launch.agent = Some(agent_id);
        launch.sessions.insert(sid(100), Some(1000));
        book.attributed.insert(
            agent_id,
            Attributed {
                launch: LAUNCH.to_string(),
                sid: Some(sid(100)),
            },
        );
        attribute(&mut book, &table, OWN, &mut tagged);
        assert_eq!(launch_of(&book, 101), None, "in the agent's session");
        assert_eq!(launch_of(&book, 100), None, "the agent");
        assert!(!book.launches[LAUNCH].sessions.contains_key(&sid(100)));
    }

    #[test]
    fn a_session_led_by_a_daemon_child_is_internal_but_its_detached_tasks_are_followed() {
        let mut book = book();
        let leader = proc(300, 1001, OWN.0 as i32, 300);
        let helper = proc(301, 1002, 300, 300);
        let task = proc(302, 1003, 300, 302);
        let table = Table::from_procs([
            agent(),
            leader,
            helper.clone(),
            task,
            proc(303, 1004, 302, 302),
            // A direct child sharing another session does not make that session internal.
            proc(400, 1005, OWN.0 as i32, 98),
            proc(401, 1006, 1, 98),
        ]);
        book.attributed.insert(
            helper.id,
            Attributed {
                launch: LAUNCH.to_string(),
                sid: Some(sid(300)),
            },
        );
        book.launches
            .get_mut(LAUNCH)
            .unwrap()
            .sessions
            .insert(sid(300), Some(1001));
        let candidates = tag_candidates(&book, &table, OWN, Some(0));
        assert!(!candidates
            .iter()
            .any(|proc| [300, 301].contains(&proc.id.pid.0)));
        let mut tagged = |proc: &Proc| {
            assert!(
                ![300, 301].contains(&proc.id.pid.0),
                "an internal's tag was read"
            );
            Some(LAUNCH.to_string())
        };
        attribute(&mut book, &table, OWN, &mut tagged);
        for pid in [300, 301, 400] {
            assert_eq!(launch_of(&book, pid), None, "internal process {pid}");
        }
        for pid in [302, 303, 401] {
            assert_eq!(launch_of(&book, pid), Some(LAUNCH), "task {pid}");
        }
        assert!(!book.launches[LAUNCH].sessions.contains_key(&sid(300)));
    }

    /// A holder process and its direct children are the daemon's own, whatever else they are: the
    /// agent a holder started and the shell it runs are never a launch's, nor is what sits in the
    /// session the shell leads, while a task that left it for a session of its own is followed.
    #[test]
    fn a_holder_and_what_it_runs_are_never_attributed_but_a_detached_task_is_followed() {
        let mut book = book();
        let holder = proc(50, 900, 1, 50);
        let shell = proc(300, 1001, 50, 300);
        let table = Table::from_procs([
            holder.clone(),
            // The agent the holder started: a direct child of the holder.
            proc(100, 1000, 50, 100),
            shell,
            proc(301, 1002, 300, 300),
            proc(302, 1003, 300, 302),
        ])
        .with_holders([holder.id]);
        let mut tagged = |_: &Proc| Some(LAUNCH.to_string());
        attribute(&mut book, &table, OWN, &mut tagged);
        for pid in [50, 100, 300, 301] {
            assert_eq!(launch_of(&book, pid), None, "managed process {pid}");
        }
        assert_eq!(launch_of(&book, 302), Some(LAUNCH));

        // A holder the table does not hold running protects nothing: its pid may be anyone's.
        let stale = Identity {
            pid: ProcessId(50),
            start_micros: 1,
        };
        let table = Table::from_procs([proc(300, 1001, 50, 300)]).with_holders([stale]);
        assert!(!table.protected(OWN).covers(&proc(301, 1002, 50, 7)));
    }

    #[test]
    fn a_previously_kept_internal_does_not_keep_new_tool_commands() {
        let mut book = book();
        let helper = proc(301, 1002, 300, 300);
        let already_kept = proc(303, 1004, 301, 303);
        let table = Table::from_procs([
            agent(),
            proc(300, 1001, OWN.0 as i32, 300),
            helper.clone(),
            proc(302, 1003, 301, 302),
            already_kept.clone(),
        ]);
        for proc in [&helper, &already_kept] {
            book.kept.insert(proc.id);
            book.kept_projects.insert(proc.id, "project".to_string());
            book.kept_sessions
                .insert(proc.sid.unwrap(), Some(proc.id.start_micros));
            book.kept_session_projects
                .insert(proc.sid.unwrap(), "project".to_string());
        }
        attribute(&mut book, &table, OWN, &mut |_| Some(LAUNCH.to_string()));
        assert_eq!(launch_of(&book, 302), Some(LAUNCH));
        assert!(!book.kept.contains(&helper.id));
        assert!(!book.kept_projects.contains_key(&helper.id));
        assert!(!book.kept_sessions.contains_key(&sid(300)));
        assert!(!book.kept_session_projects.contains_key(&sid(300)));
        assert!(book.kept.contains(&already_kept.id));
        assert_eq!(
            book.kept_projects.get(&already_kept.id).map(String::as_str),
            Some("project")
        );
    }

    /// The session of a kept process keeps what is found in it once the kept process has
    /// exited, a tag of an ended launch notwithstanding.
    #[test]
    fn a_tagged_process_in_the_session_of_a_kept_process_that_exited_is_kept() {
        let mut book = book();
        let kept = proc(500, 1010, 1, 500);
        let table = Table::from_procs([agent(), kept.clone()]);
        book.kept.insert(kept.id);
        record_kept_session(&mut book, &table, &kept);
        let table = Table::from_procs([agent(), proc(501, 1011, 1, 500)]);
        let mut ended = |_: &Proc| Some("gone-launch".to_string());
        attribute(&mut book, &table, OWN, &mut ended);
        assert_eq!(launch_of(&book, 501), None);
        assert!(book.kept.iter().any(|id| id.pid.0 == 501));
    }

    /// A session whose leader runs as another user is not taken for one whose leader is gone:
    /// a tagged process in it does not bring the session along.
    #[test]
    fn a_session_led_by_another_user_is_not_recorded() {
        let mut book = book();
        let table = Table::from_procs([agent(), proc(601, 1012, 1, 600), proc(602, 1013, 1, 600)])
            .with_foreign_leaders([sid(600)]);
        let mut tag_of = |proc: &Proc| (proc.id.pid.0 == 601).then(|| LAUNCH.to_string());
        attribute(&mut book, &table, OWN, &mut tag_of);
        assert_eq!(launch_of(&book, 601), Some(LAUNCH));
        assert_eq!(launch_of(&book, 602), None);
        assert!(book.launches[LAUNCH].sessions.is_empty());
    }

    /// A whole launch moved to another reads as that one from then on, sessions and all; a part
    /// of one takes along the sessions only its moved processes sit in.
    #[test]
    fn a_transfer_moves_processes_and_the_sessions_they_alone_sit_in() {
        let mut book = book();
        book.launches.insert(
            "other".to_string(),
            Launch {
                started: true,
                ..Launch::default()
            },
        );
        let table = Table::from_procs([
            agent(),
            proc(200, 1002, 100, 200),
            proc(300, 1003, 100, 300),
            proc(301, 1004, 300, 300),
        ]);
        attribute(&mut book, &table, OWN, &mut untagged);
        let moved = HashSet::from([Identity {
            pid: ProcessId(200),
            start_micros: 1002,
        }]);
        book.transfer(LAUNCH, "other", Some((&moved, &table)));
        assert_eq!(launch_of(&book, 200), Some("other"));
        assert_eq!(launch_of(&book, 300), Some(LAUNCH));
        assert!(book.launches["other"].sessions.contains_key(&sid(200)));
        assert!(!book.launches[LAUNCH].sessions.contains_key(&sid(200)));
        assert!(book.launches[LAUNCH].sessions.contains_key(&sid(300)));

        book.transfer(LAUNCH, "other", None);
        assert_eq!(launch_of(&book, 301), Some("other"));
        assert!(book.launches["other"].sessions.contains_key(&sid(300)));
        assert!(!book.launches.contains_key(LAUNCH));
        assert_eq!(book.followed_as(LAUNCH), Some("other"));
        book.hand_to_ended("other");
        assert_eq!(book.followed_as(LAUNCH), None);
    }

    /// What a restart needs is a change to write down: a session recorded, a process attributed,
    /// even in a session already recorded, since a restart takes a session back only when a
    /// process the file names is in it. A process going is not.
    #[test]
    fn only_a_change_a_restart_needs_is_marked_to_be_written() {
        let mut book = book();
        let table = Table::from_procs([agent(), proc(200, 1002, 100, 200)]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert!(book.dirty, "a session recorded");

        book.dirty = false;
        let table = Table::from_procs([agent(), proc(201, 1003, 1, 200)]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert_eq!(launch_of(&book, 201), Some(LAUNCH));
        assert!(book.dirty, "attributed in a session already recorded");

        book.dirty = false;
        let table = Table::from_procs([agent()]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert!(!book.dirty, "a process going");
    }

    /// A session read back from a previous run's file is taken back only when a process the
    /// file named is still in it: one whose members exited may have had its id handed to an
    /// unrelated process that started a session and left members in it with no leader, and
    /// those are neither stopped as an ended launch's nor kept.
    #[test]
    fn a_session_read_back_is_taken_back_only_with_a_process_the_file_named_in_it() {
        let mut book = book();
        let named = [
            Identity {
                pid: ProcessId(301),
                start_micros: 900,
            },
            Identity {
                pid: ProcessId(501),
                start_micros: 901,
            },
        ];
        book.read_back_sessions(
            [(sid(300), None), (sid(500), None)],
            [(sid(400), Some(800))],
            named,
        );
        let table = Table::from_procs([
            agent(),
            // Session 300's named member is gone; 301 is another process with its pid.
            proc(301, 5000, 1, 300),
            proc(302, 5001, 1, 300),
            proc(402, 5002, 1, 400),
            proc(501, 901, 1, 500),
            proc(502, 5003, 1, 500),
        ]);
        attribute(&mut book, &table, OWN, &mut untagged);
        for pid in [301, 302, 402] {
            assert_eq!(launch_of(&book, pid), None, "{pid}");
            assert!(!book.kept.iter().any(|id| id.pid.0 as i32 == pid), "{pid}");
        }
        assert!(!book.launches[ENDED].sessions.contains_key(&sid(300)));
        assert!(!book.kept_sessions.contains_key(&sid(400)));
        assert_eq!(launch_of(&book, 502), Some(ENDED), "proven by 501");
        assert!(book.launches[ENDED].sessions.contains_key(&sid(500)));
    }
}
