//! Deciding, from one read of the process table, which launch each process belongs to. Pure over
//! the table and what the ledger already holds, so the rules are tested on tables made up for it.
//!
//! A process P belongs to launch L when, in this order:
//!
//! 1. P is the daemon, one of its ancestors or one of its direct children (an agent among them),
//!    or sits in an agent's own terminal session: it belongs to no launch here, whatever was
//!    decided of it before. An agent's session holds its internals, which the end of the agent's
//!    PTY sweeps.
//! 2. P was attributed before and is still running (same pid, same start): L stays.
//! 3. P's chain of parents, as this read shows it, reaches L's agent or a process of L's.
//! 4. P sits in the terminal session of a kept process, or in one recorded as kept: it is kept
//!    too (see below).
//! 5. P's environment carries this instance's tag with L's launch id.
//! 6. P sits in a terminal session recorded for L.
//!
//! The environment is asked for only when rules 1 to 4 decide nothing, so a process they decide
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
//! cannot be read, after the shell that started it exited and it was reparented.
//!
//! Kept processes and their descendants are never attributed: a process whose chain reaches a
//! kept one is kept itself, and so is a process found in a kept process's session with no other
//! claim on it, which is how a kept process's child is still found once its chain is broken. The
//! session of a kept process is recorded as kept by the same rule as a launch's, so it still
//! keeps what is in it once the kept process itself has exited.

use std::collections::{HashMap, HashSet};

use super::table::{Identity, Proc, Protected, Table};

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
}

/// Terminal sessions by id, each with its leader's start time when the leader was running when
/// it was recorded.
pub(crate) type Sessions = HashMap<i32, Option<u64>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attributed {
    pub launch: String,
    /// The terminal session the process was in when it was attributed.
    pub sid: i32,
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
    pub kept_session_projects: HashMap<i32, String>,
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
    /// Something a restart would need changed since the ledger file was last written: a launch
    /// started or ended, a process kept, one held for an ended launch, or a session recorded.
    /// A process attributed in a session already recorded is not such a change; it is written
    /// with the next one.
    pub dirty: bool,
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

    fn agent_pids(&self) -> HashSet<i32> {
        self.launches
            .values()
            .filter_map(|launch| launch.agent.map(|agent| agent.pid))
            .collect()
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
    /// user's, listed under the project `project`, and stops following the launch.
    pub fn keep_launch(&mut self, launch_id: &str, project: &str) {
        if launch_id == ENDED {
            return;
        }
        let Some(launch) = self.launches.remove(launch_id) else {
            return;
        };
        self.aliases.retain(|_, to| to != launch_id);
        let members: Vec<Identity> = self
            .attributed
            .iter()
            .filter(|(_, attributed)| attributed.launch == launch_id)
            .map(|(id, _)| *id)
            .collect();
        for id in members {
            self.attributed.remove(&id);
            self.kept.insert(id);
            self.kept_projects.insert(id, project.to_string());
        }
        for (sid, leader_start) in launch.sessions {
            self.kept_sessions.entry(sid).or_insert(leader_start);
            self.kept_session_projects
                .entry(sid)
                .or_insert_with(|| project.to_string());
        }
        self.dirty = true;
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
    /// a carry-over moves all of a launch, a handover of process trees some of it.
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
                }
                for target in self.aliases.values_mut() {
                    if target == from {
                        *target = to.to_string();
                    }
                }
                self.aliases.insert(from.to_string(), to.to_string());
            }
            Some((members, table)) => {
                let mut sids: HashSet<i32> = HashSet::new();
                for (id, attributed) in self.attributed.iter_mut() {
                    if attributed.launch == from && members.contains(id) {
                        attributed.launch = to.to_string();
                        sids.extend(
                            table
                                .get(id.pid)
                                .filter(|proc| proc.id == *id)
                                .map(|proc| proc.sid),
                        );
                    }
                }
                let staying: HashSet<i32> = self
                    .attributed
                    .iter()
                    .filter(|(_, attributed)| attributed.launch == from)
                    .filter_map(|(id, _)| table.get(id.pid).filter(|proc| proc.id == *id))
                    .map(|proc| proc.sid)
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

    /// Hands what is attributed to `launch_id`, with the sessions recorded for it, to [`ENDED`],
    /// to be stopped, and stops following the launch.
    pub fn hand_to_ended(&mut self, launch_id: &str) {
        if launch_id == ENDED {
            return;
        }
        self.aliases.retain(|_, to| to != launch_id);
        let sessions = self
            .launches
            .remove(launch_id)
            .map(|launch| launch.sessions)
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
        self.dirty |= moved;
    }
}

/// Whether `proc` is never any launch's (rule 1).
fn never_attributed(
    book: &Book,
    protected: &Protected,
    agent_pids: &HashSet<i32>,
    proc: &Proc,
) -> bool {
    protected.covers(proc)
        || agent_pids.contains(&proc.sid)
        || book.agent_launch(&proc.id).is_some()
}

/// The sessions of the kept processes running in `table`, and those recorded as kept.
fn kept_sessions(book: &Book, table: &Table) -> HashSet<i32> {
    table
        .procs()
        .filter(|proc| book.kept.contains(&proc.id) && proc.sid > 1)
        .map(|proc| proc.sid)
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
    own: i32,
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
            proc.id.start >= horizon
                && !book.kept.contains(&proc.id)
                && !book.attributed.contains_key(&proc.id)
                && !never_attributed(book, &protected, &agent_pids, proc)
                && !kept_sessions.contains(&proc.sid)
                && !table
                    .ancestors(proc)
                    .any(|ancestor| book.kept.contains(&ancestor.id))
                && by_chain(book, table, proc).is_none()
        })
        .collect()
}

/// Brings `book` up to date with `table`: forgets the processes that are gone, keeps what descends
/// from kept processes, drops sessions that are no longer the ones recorded, and attributes every
/// process the rules give a launch. `own` is the daemon's pid. `tag_of` answers the launch id in
/// a process's tag when the tag is this instance's, and `None` otherwise or when the environment
/// cannot be read; it is asked only of a process rules 1 to 4 leave undecided.
pub(crate) fn attribute(
    book: &mut Book,
    table: &Table,
    own: i32,
    tag_of: &mut dyn FnMut(&Proc) -> Option<String>,
) {
    forget_gone(book, table);
    let order = table.by_start();
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
    let protected = table.protected(own);
    let agent_pids = book.agent_pids();
    for proc in &order {
        if never_attributed(book, &protected, &agent_pids, proc) {
            book.attributed.remove(&proc.id);
        }
    }
    for launch in book.launches.values_mut() {
        launch.sessions.retain(|sid, _| !agent_pids.contains(sid));
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
                .or_else(|| kept_sessions.contains(&proc.sid).then_some(Verdict::Kept))
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
                    let recorded = record_session(book, table, &launch, proc);
                    book.dirty |= recorded || launch == ENDED;
                }
                Some(Verdict::Kept) => {
                    let project = kept_session_project(book, table, proc.sid);
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

/// Drops each recorded session that no process sits in any more, or whose id a process now leads
/// that did not lead it when it was recorded: the id may since have been handed to another.
fn check_sessions(book: &mut Book, table: &Table) {
    let occupied: HashSet<i32> = table.procs().map(|proc| proc.sid).collect();
    let still_recorded = |sid: &i32, leader_start: &mut Option<u64>| {
        if !occupied.contains(sid) {
            return false;
        }
        match table.get(*sid).filter(|leader| leader.sid == *sid) {
            Some(leader) => *leader_start == Some(leader.id.start),
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
    book.launches
        .iter()
        .find(|(_, launch)| launch.sessions.contains_key(&proc.sid))
        .map(|(launch_id, _)| launch_id.clone())
}

/// `proc`'s terminal session as one to record beside it — with its leader's start time when the
/// leader is running — when `ours` says the leader may be recorded with it: unless the leader is
/// running and is neither `proc` nor such a process, or runs as another user.
fn session_to_record(
    table: &Table,
    proc: &Proc,
    ours: impl FnOnce(&Identity) -> bool,
) -> Option<(i32, Option<u64>)> {
    let sid = proc.sid;
    if sid <= 1 {
        return None;
    }
    match table.get(sid).filter(|leader| leader.sid == sid) {
        Some(leader) if leader.id == proc.id || ours(&leader.id) => {
            Some((sid, Some(leader.id.start)))
        }
        Some(_) => None,
        None if table.led_by_another_user(sid) => None,
        None => Some((sid, None)),
    }
}

/// Records `proc`'s terminal session for `launch_id`, which `proc` was just attributed to, unless
/// its leader is running and is not `launch_id`'s. Answers whether the session is new to it.
fn record_session(book: &mut Book, table: &Table, launch_id: &str, proc: &Proc) -> bool {
    let attributed = &book.attributed;
    let Some((sid, leader_start)) = session_to_record(table, proc, |leader| {
        attributed
            .get(leader)
            .is_some_and(|attributed| attributed.launch == launch_id)
    }) else {
        return false;
    };
    match book.launches.get_mut(launch_id) {
        Some(launch) if !launch.sessions.contains_key(&sid) => {
            launch.sessions.insert(sid, leader_start);
            true
        }
        _ => false,
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
fn kept_session_project(book: &Book, table: &Table, sid: i32) -> Option<String> {
    book.kept_session_projects.get(&sid).cloned().or_else(|| {
        table
            .procs()
            .filter(|proc| proc.sid == sid)
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
    const OWN: i32 = 99;

    fn proc(pid: i32, start: u64, ppid: i32, sid: i32) -> Proc {
        Proc {
            id: Identity { pid, start },
            ppid,
            sid,
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
                    pid: 100,
                    start: 1000,
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
            .find(|(id, _)| id.pid == pid)
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
            asked.push(proc.id.pid);
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
        let mut tag_of = |proc: &Proc| match proc.id.pid {
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
        let mut tag_of = |proc: &Proc| matches!(proc.id.pid, 501 | 601).then(|| LAUNCH.into());
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
            pid: 201,
            start: 1003,
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
            assert!(book.kept.iter().any(|id| id.pid == pid), "{pid}");
        }
        assert_eq!(launch_of(&book, 200), Some(LAUNCH));
        assert_eq!(launch_of(&book, 701), Some(LAUNCH));
    }

    /// A scan that ran before the agent's launch was known may have attributed the agent, which
    /// carries the tag, and what runs in its session; once the agent is known, neither is any
    /// launch's. The daemon, its ancestors and its direct children never are, tag or not.
    #[test]
    fn the_agent_its_session_and_the_daemon_with_its_kin_are_never_attributed() {
        let mut book = book();
        let agent_id = agent().id;
        book.launches.get_mut(LAUNCH).unwrap().agent = None;
        let table = Table::from_procs([
            proc(98, 900, 1, 98),
            proc(OWN, 901, 98, 98),
            proc(100, 1000, OWN, 100),
            proc(101, 1001, 100, 100),
            proc(300, 1002, OWN, 300),
        ]);
        let mut tagged = |_: &Proc| Some(LAUNCH.to_string());
        attribute(&mut book, &table, OWN, &mut tagged);
        assert_eq!(launch_of(&book, 98), None, "the daemon's parent");
        assert_eq!(launch_of(&book, OWN), None, "the daemon");
        assert_eq!(launch_of(&book, 100), None, "the daemon's child");
        assert_eq!(launch_of(&book, 300), None, "the daemon's child");
        assert_eq!(
            launch_of(&book, 101),
            Some(LAUNCH),
            "before the agent is known"
        );

        let launch = book.launches.get_mut(LAUNCH).unwrap();
        launch.agent = Some(agent_id);
        launch.sessions.insert(100, Some(1000));
        book.attributed.insert(
            agent_id,
            Attributed {
                launch: LAUNCH.to_string(),
                sid: 100,
            },
        );
        attribute(&mut book, &table, OWN, &mut tagged);
        assert_eq!(launch_of(&book, 101), None, "in the agent's session");
        assert_eq!(launch_of(&book, 100), None, "the agent");
        assert!(!book.launches[LAUNCH].sessions.contains_key(&100));
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
        assert!(book.kept.iter().any(|id| id.pid == 501));
    }

    /// A session whose leader runs as another user is not taken for one whose leader is gone:
    /// a tagged process in it does not bring the session along.
    #[test]
    fn a_session_led_by_another_user_is_not_recorded() {
        let mut book = book();
        let table = Table::from_procs([agent(), proc(601, 1012, 1, 600), proc(602, 1013, 1, 600)])
            .with_foreign_leaders([600]);
        let mut tag_of = |proc: &Proc| (proc.id.pid == 601).then(|| LAUNCH.to_string());
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
            pid: 200,
            start: 1002,
        }]);
        book.transfer(LAUNCH, "other", Some((&moved, &table)));
        assert_eq!(launch_of(&book, 200), Some("other"));
        assert_eq!(launch_of(&book, 300), Some(LAUNCH));
        assert!(book.launches["other"].sessions.contains_key(&200));
        assert!(!book.launches[LAUNCH].sessions.contains_key(&200));
        assert!(book.launches[LAUNCH].sessions.contains_key(&300));

        book.transfer(LAUNCH, "other", None);
        assert_eq!(launch_of(&book, 301), Some("other"));
        assert!(book.launches["other"].sessions.contains_key(&300));
        assert!(!book.launches.contains_key(LAUNCH));
        assert_eq!(book.followed_as(LAUNCH), Some("other"));
        book.hand_to_ended("other");
        assert_eq!(book.followed_as(LAUNCH), None);
    }

    /// What a restart needs is a change to write down: a session recorded, a process held for an
    /// ended launch. Another process attributed in a session already recorded is not, and
    /// neither is a process going.
    #[test]
    fn only_a_change_a_restart_needs_is_marked_to_be_written() {
        let mut book = book();
        let table = Table::from_procs([agent(), proc(200, 1002, 100, 200)]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert!(book.dirty, "a session recorded");

        book.dirty = false;
        let table = Table::from_procs([
            agent(),
            proc(200, 1002, 100, 200),
            proc(201, 1003, 200, 200),
        ]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert_eq!(launch_of(&book, 201), Some(LAUNCH));
        let table = Table::from_procs([agent(), proc(200, 1002, 100, 200)]);
        attribute(&mut book, &table, OWN, &mut untagged);
        assert!(!book.dirty);

        let table = Table::from_procs([agent(), proc(200, 1002, 100, 200), proc(400, 1010, 1, 1)]);
        attribute(&mut book, &table, OWN, &mut |proc: &Proc| {
            (proc.id.pid == 400).then(|| "gone-launch".to_string())
        });
        assert_eq!(launch_of(&book, 400), Some(ENDED));
        assert!(book.dirty, "held for an ended launch");
    }
}
