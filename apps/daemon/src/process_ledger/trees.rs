//! The process trees a client is shown, and the stops and keeps it asks for by tree.
//!
//! A tree is named by its top process, and between a listing and the stop or keep that names one
//! of its trees the top may exit: a wrapper shell ends, and its child, reparented, is a top of its
//! own. So the ledger remembers, for each top a listing showed, its members and their descendants
//! seen by every scan since, for as long as they run, whatever was listed after; and a name whose
//! top is no longer one reaches those members still in a tree that qualifies, with what descends
//! from them there.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::attribute::{keep, Attributed, Launch, ENDED};
use super::table::{self, Identity, Table};
use super::ProcessLedger;

/// The longest command a tree reports, in bytes.
const COMMAND_LIMIT: usize = 1024;

/// How many tree tops are remembered at once: the oldest go first past it. Only the tops whose
/// members still run are kept at all, so this bounds what a long run of listings could make the
/// ledger hold.
const LISTINGS_LIMIT: usize = 1024;

/// A group of one launch's processes as the user sees it: a top process — one whose parent is not
/// the same launch's — and its descendants of that launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessTree {
    /// The top process, as `<pid>:<start>`: what a stop or a keep names the tree by.
    pub id: String,
    /// The session whose launch the tree belongs to; absent for a tree no session owns: one left
    /// behind, or one the user kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The user kept the tree: no session owns it and nothing stops it unless the user asks.
    pub kept: bool,
    /// The top process's arguments, joined; empty when they cannot be read.
    pub command: String,
    pub cwd: Option<String>,
    /// The top process's working directory was removed.
    pub cwd_missing: bool,
    /// When the top process started, in milliseconds since the Unix epoch.
    pub started_at: i64,
    pub process_count: usize,
    /// The resident memory of every process in the tree, summed.
    pub memory_bytes: u64,
    /// The TCP ports the tree's processes listen on, sorted.
    pub ports: Vec<u16>,
}

/// Why [`ProcessLedger::move_trees`] moved nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveRefused {
    /// The receiving session has no launch running to take the trees.
    NoLaunch,
    /// These ids name no tree of the giving session's running launch, as the process table
    /// stands now.
    NotFrom(Vec<String>),
    /// The process table could not be read.
    Unreadable,
}

impl ProcessLedger {
    /// The trees of the processes the launches of `sessions` started whose top has run for at
    /// least `min_age`, oldest first, with their ports read now. One read of the table serves
    /// every session, and only the trees shown are described. **Blocks.**
    pub fn trees_of_sessions(&self, sessions: &[String], min_age: Duration) -> Vec<ProcessTree> {
        let Some(table) = self.scan(false) else {
            return Vec::new();
        };
        self.session_trees(&table, sessions, min_age)
    }

    /// The trees of `sessions` (see [`Self::trees_of_sessions`]) followed by the processes of the
    /// project `project`, the trees no session owns: those left behind in `leftover_dirs` and
    /// those kept under the project, oldest first, from one read of the table. **Blocks.**
    ///
    /// Left behind in a set of directories — a project's, its worktrees included — is a process
    /// of the daemon's user that no launch being followed has and that is not kept, working in a
    /// directory under one of them that has been removed. The daemon's own direct children (the
    /// user's shell terminals among them) and what runs in the terminal sessions they lead are
    /// the user's, and never left behind. Kept under a project is a process the user kept from a
    /// tree of one of its sessions or left behind in it, or one that descends from such a process
    /// or sits in its terminal session. Neither is stopped unless asked to.
    pub fn trees_of_project(
        &self,
        sessions: &[String],
        project: &str,
        leftover_dirs: &[PathBuf],
    ) -> Vec<ProcessTree> {
        let Some(table) = self.scan(false) else {
            return Vec::new();
        };
        let mut trees = self.session_trees(&table, sessions, Duration::ZERO);
        let mut unowned: Vec<(Identity, Vec<Identity>, bool)> = Vec::new();
        if !leftover_dirs.is_empty() {
            let members = self.leftover_members(&table, leftover_dirs);
            unowned.extend(
                group(&members, &table)
                    .into_iter()
                    .map(|(top, members)| (top, members, false)),
            );
        }
        let kept = self.kept_members(&table, Some(project));
        unowned.extend(
            group(&kept, &table)
                .into_iter()
                .map(|(top, members)| (top, members, true)),
        );
        self.remember(
            unowned
                .iter()
                .map(|(top, members, _)| (*top, members.as_slice())),
        );
        unowned.sort_by_key(|(top, _, _)| (top.start, top.pid));
        trees.extend(
            unowned
                .into_iter()
                .map(|(top, members, kept)| describe(top, &members, None, kept)),
        );
        trees
    }

    /// The trees listed for a client under `key` less than `floor` ago, unless a stop, a keep or
    /// the end of a launch has changed what there is since: what a client polling a list on a
    /// timer of its own is answered when another client, or the same one, has just been, without
    /// a read of the table each time.
    pub fn listed_lately(&self, key: &str, floor: Duration) -> Option<Vec<ProcessTree>> {
        let lists = self.lists.lock().expect("process lists lock poisoned");
        lists
            .get(key)
            .filter(|(at, _)| at.elapsed() < floor)
            .map(|(_, trees)| trees.clone())
    }

    /// Records `trees` as just listed for a client under `key` (see [`Self::listed_lately`]).
    pub fn note_listed(&self, key: String, trees: Vec<ProcessTree>) {
        let mut lists = self.lists.lock().expect("process lists lock poisoned");
        // A key per session and per project: past a few hundred, what is old enough to be of no
        // use any more is dropped.
        if lists.len() >= LISTINGS_LIMIT {
            lists.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
        }
        lists.insert(key, (Instant::now(), trees));
    }

    /// Forgets what was listed for clients, after something changed what a listing would show.
    pub(super) fn forget_lists(&self) {
        self.lists
            .lock()
            .expect("process lists lock poisoned")
            .clear();
    }

    /// Stops each tree `ids` names that is still one: a tree of a launch being followed, one left
    /// behind in `leftover_dirs`, or one the user kept (see [`Self::trees_of_project`]). A kept
    /// tree is the user's no more once the user asks for it to be stopped: what is left of it,
    /// or starts from it meanwhile, is stopped as an ended launch's. Returns the ids of the trees
    /// it stopped. **Blocks.**
    pub fn stop_trees(&self, ids: &[String], leftover_dirs: &[PathBuf]) -> Vec<String> {
        let Some(table) = self.scan(false) else {
            return Vec::new();
        };
        let named = self.resolve(ids, &table, leftover_dirs, |_| true, true);
        let targets: Vec<Identity> = named
            .iter()
            .flat_map(|(_, members)| members.iter().copied())
            .collect();
        self.unkeep(&targets, &table);
        if !targets.is_empty() {
            tracing::info!(
                trees = named.len(),
                count = targets.len(),
                "stopping process trees on request"
            );
            self.stop_set(targets, &table);
            self.forget_lists();
        }
        named.into_iter().map(|(id, _)| id).collect()
    }

    /// Makes each tree `ids` names that is still one the user's — a tree of a launch being
    /// followed and not ending, or one left behind in the directory of one of `projects` (by id,
    /// with its directory) — so its processes, and whatever descends from them, are never
    /// stopped unless the user asks: no session owns them from then on, and they are listed
    /// among the project's processes (see [`Self::trees_of_project`]). The project is the one
    /// `session_projects` gives the session whose tree it was (by session id), or the one whose
    /// directory a tree left behind is in; a tree of a session that `session_projects` gives no
    /// project, a console session's, is not kept. Returns the ids of the trees it kept.
    /// **Blocks.**
    pub fn keep_trees(
        &self,
        ids: &[String],
        projects: &[(String, PathBuf)],
        session_projects: &HashMap<String, String>,
    ) -> Vec<String> {
        let Some(table) = self.scan(false) else {
            return Vec::new();
        };
        let leftover_dirs: Vec<PathBuf> = projects.iter().map(|(_, dir)| dir.clone()).collect();
        let named = self.resolve(ids, &table, &leftover_dirs, |launch| !launch.ending, false);
        // Read before the lock is taken, as a system call each.
        let cwds: HashMap<Identity, PathBuf> = named
            .iter()
            .flat_map(|(_, members)| members.iter().copied())
            .filter_map(|id| table::cwd(id).map(|cwd| (id, cwd.path)))
            .collect();
        let mut kept = Vec::new();
        let mut book = self.book();
        for (id, members) in named {
            // Checked again under the lock the keep is made under: a launch that began ending
            // since is stopping these processes already.
            let ending = members.iter().any(|member| {
                book.attributed
                    .get(member)
                    .and_then(|attributed| book.launches.get(&attributed.launch))
                    .is_some_and(|launch| launch.ending)
            });
            if ending {
                continue;
            }
            let session = members.iter().find_map(|member| {
                book.attributed
                    .get(member)
                    .and_then(|attributed| book.launches.get(&attributed.launch))
                    .and_then(|launch| launch.session_id.clone())
            });
            let project = match session {
                Some(session) => session_projects.get(&session).cloned(),
                None => members
                    .iter()
                    .find_map(|member| project_of_dir(cwds.get(member)?, projects)),
            };
            // A tree of a session in no project, a console session's, is not the user's to keep.
            let Some(project) = project else {
                continue;
            };
            for member in members {
                if let Some(proc) = table.get(member.pid).filter(|proc| proc.id == member) {
                    keep(&mut book, &table, proc, Some(project.clone()));
                }
            }
            kept.push(id);
        }
        if !kept.is_empty() {
            book.dirty = true;
        }
        drop(book);
        if !kept.is_empty() {
            self.forget_lists();
        }
        self.persist(true);
        kept
    }

    /// Stops the processes of the project `project`, the trees no session owns: those left behind
    /// in `leftover_dirs`, its directories, and those kept under it (see
    /// [`Self::trees_of_project`]), for the project's removal. **Blocks.**
    pub fn stop_project_processes(&self, project: &str, leftover_dirs: &[PathBuf]) {
        let Some(table) = self.scan(false) else {
            return;
        };
        let mut members = self.leftover_members(&table, leftover_dirs);
        members.extend(self.kept_members(&table, Some(project)));
        let trees = group(&members, &table);
        if trees.is_empty() {
            return;
        }
        let targets: Vec<Identity> = members.into_iter().collect();
        tracing::info!(
            project,
            trees = trees.len(),
            count = targets.len(),
            "stopping the processes of a project being removed"
        );
        self.unkeep(&targets, &table);
        self.stop_set(targets, &table);
        self.forget_lists();
    }

    /// The processes of the project `project` (see [`Self::stop_project_processes`]), as a
    /// listing describes them, oldest first: what its removal would stop. **Blocks.**
    pub fn project_processes(&self, project: &str, leftover_dirs: &[PathBuf]) -> Vec<ProcessTree> {
        self.trees_of_project(&[], project, leftover_dirs)
    }

    /// Moves the trees `ids` names from the running launch of the session `from` to that of the
    /// session `to`, whose they are from then on: listed under it, and stopped when its launch
    /// ends instead of `from`'s (see [`Book::transfer`](super::attribute::Book::transfer)). All
    /// or nothing: refused, with nothing moved, when one of `ids` is not a tree of `from`'s, or
    /// `to` has no launch running. Returns the ids moved. **Blocks.**
    pub fn move_trees(
        &self,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, MoveRefused> {
        let Some(table) = self.scan(false) else {
            return Err(MoveRefused::Unreadable);
        };
        let mut wanted: Vec<String> = Vec::new();
        for id in ids {
            if !wanted.contains(id) {
                wanted.push(id.clone());
            }
        }
        let of_from = |launch: &Launch| {
            launch.session_id.as_deref() == Some(from) && !launch.ending && !launch.held
        };
        let named = self.resolve(&wanted, &table, &[], of_from, false);
        let mut book = self.book();
        let Some(receiving) = book.running_launch_of(to, "") else {
            return Err(MoveRefused::NoLaunch);
        };
        // Read again under the lock the move is made under: a launch may have begun ending since.
        let mut moves: Vec<(String, Vec<(String, Identity)>)> = Vec::new();
        for (id, members) in &named {
            let members: Vec<(String, Identity)> = members
                .iter()
                .filter_map(|member| {
                    let launch = &book.attributed.get(member)?.launch;
                    book.launches
                        .get(launch)
                        .filter(|launch| of_from(launch))
                        .map(|_| (launch.clone(), *member))
                })
                .collect();
            if !members.is_empty() {
                moves.push((id.clone(), members));
            }
        }
        let missing: Vec<String> = wanted
            .iter()
            .filter(|id| !moves.iter().any(|(moved, _)| moved == *id))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(MoveRefused::NotFrom(missing));
        }
        let mut by_launch: HashMap<String, HashSet<Identity>> = HashMap::new();
        for (_, members) in &moves {
            for (launch, member) in members {
                by_launch.entry(launch.clone()).or_default().insert(*member);
            }
        }
        for (launch, members) in &by_launch {
            book.transfer(launch, &receiving, Some((members, &table)));
        }
        drop(book);
        tracing::info!(
            from,
            to,
            trees = moves.len(),
            "moving process trees to another session on request"
        );
        self.forget_lists();
        self.persist(true);
        Ok(moves.into_iter().map(|(id, _)| id).collect())
    }

    /// The trees of `sessions` in `table` (see [`Self::trees_of_sessions`]), every one of them
    /// remembered whatever its age.
    fn session_trees(
        &self,
        table: &Table,
        sessions: &[String],
        min_age: Duration,
    ) -> Vec<ProcessTree> {
        let launches = self.launch_trees(table);
        let mut found: Vec<(String, Identity, Vec<Identity>)> = Vec::new();
        {
            let book = self.book();
            for (launch, trees) in launches {
                let Some(session) = book
                    .launches
                    .get(&launch)
                    .and_then(|launch| launch.session_id.as_ref())
                    .filter(|session| sessions.contains(session))
                else {
                    continue;
                };
                found.extend(
                    trees
                        .into_iter()
                        .map(|(top, members)| (session.clone(), top, members)),
                );
            }
        }
        self.remember(
            found
                .iter()
                .map(|(_, top, members)| (*top, members.as_slice())),
        );
        let oldest = table::now().saturating_sub(min_age.as_micros() as u64);
        found.retain(|(_, top, _)| top.start <= oldest);
        found.sort_by_key(|(_, top, _)| (top.start, top.pid));
        found
            .into_iter()
            .map(|(session, top, members)| describe(top, &members, Some(&session), false))
            .collect()
    }

    /// The trees `ids` name, each with the members to act on as `table` shows them, among the
    /// trees of the launches being followed that `include` accepts and those left behind in
    /// `leftover_dirs`. A name whose top is not a top any more reaches the members remembered for
    /// it that are in one of those trees, with what descends from them there. Each process is
    /// given to the first name that reaches it.
    fn resolve(
        &self,
        ids: &[String],
        table: &Table,
        leftover_dirs: &[PathBuf],
        include: impl Fn(&Launch) -> bool,
        with_kept: bool,
    ) -> Vec<(String, Vec<Identity>)> {
        let launches = self.launch_trees(table);
        let mut current: BTreeMap<Identity, Vec<Identity>> = BTreeMap::new();
        {
            let book = self.book();
            for (launch, trees) in launches {
                if book.launches.get(&launch).is_some_and(&include) {
                    current.extend(trees);
                }
            }
        }
        if !leftover_dirs.is_empty() {
            let leftovers = self.leftover_members(table, leftover_dirs);
            for (top, members) in group(&leftovers, table) {
                current.entry(top).or_insert(members);
            }
        }
        if with_kept {
            let kept = self.kept_members(table, None);
            for (top, members) in group(&kept, table) {
                current.entry(top).or_insert(members);
            }
        }
        let in_trees: Vec<Identity> = current.values().flatten().copied().collect();
        let listings = self
            .listings
            .lock()
            .expect("process listings lock poisoned");
        let mut taken: HashSet<Identity> = HashSet::new();
        let mut named = Vec::new();
        for key in ids {
            let Some(top) = Identity::parse(key) else {
                continue;
            };
            let members: Vec<Identity> = match current.get(&top) {
                Some(members) => members.clone(),
                None => {
                    let Some(remembered) = listings.get(&top) else {
                        continue;
                    };
                    in_trees
                        .iter()
                        .copied()
                        .filter(|id| {
                            remembered.contains(id)
                                || table.get(id.pid).is_some_and(|proc| {
                                    table
                                        .ancestors(proc)
                                        .any(|ancestor| remembered.contains(&ancestor.id))
                                })
                        })
                        .collect()
                }
            };
            let members: Vec<Identity> =
                members.into_iter().filter(|id| taken.insert(*id)).collect();
            if !members.is_empty() {
                named.push((key.clone(), members));
            }
        }
        named
    }

    /// Adds what `listed` shows, tree top by tree top, to what is remembered of each, dropping
    /// the oldest names past the limit.
    fn remember<'a>(&self, listed: impl IntoIterator<Item = (Identity, &'a [Identity])>) {
        let mut listings = self
            .listings
            .lock()
            .expect("process listings lock poisoned");
        for (top, members) in listed {
            listings
                .entry(top)
                .or_default()
                .extend(members.iter().copied());
        }
        if listings.len() > LISTINGS_LIMIT {
            let mut tops: Vec<Identity> = listings.keys().copied().collect();
            tops.sort_by_key(|top| (top.start, top.pid));
            for top in &tops[..tops.len() - LISTINGS_LIMIT] {
                listings.remove(top);
            }
        }
    }

    /// Carries descendants into each remembered name on every scan, including scans between
    /// listings, before forgetting its dead members. A cached listing can still name an old top.
    pub(super) fn advance_listings(&self, table: &Table) {
        let mut listings = self
            .listings
            .lock()
            .expect("process listings lock poisoned");
        if listings.is_empty() {
            return;
        }
        let mut descendants: HashMap<Identity, Vec<Identity>> = HashMap::new();
        for proc in table.procs() {
            for ancestor in table.ancestors(proc) {
                descendants.entry(ancestor.id).or_default().push(proc.id);
            }
        }
        for members in listings.values_mut() {
            // A child can become a top and start children of its own before it exits. Carry
            // those descendants into every earlier name before forgetting their dead parents.
            let inherited: Vec<Identity> = members
                .iter()
                .filter_map(|member| descendants.get(member))
                .flatten()
                .copied()
                .collect();
            members.extend(inherited);
            members.retain(|member| table.has(member));
        }
        listings.retain(|_, members| !members.is_empty());
    }

    /// The trees of every launch being followed, by launch id, from `table`.
    fn launch_trees(&self, table: &Table) -> HashMap<String, BTreeMap<Identity, Vec<Identity>>> {
        let book = self.book();
        let mut members: HashMap<&str, HashSet<Identity>> = HashMap::new();
        for (id, attributed) in &book.attributed {
            if attributed.launch != ENDED && table.has(id) {
                members
                    .entry(attributed.launch.as_str())
                    .or_default()
                    .insert(*id);
            }
        }
        members
            .into_iter()
            .map(|(launch, members)| (launch.to_string(), group(&members, table)))
            .collect()
    }

    /// The kept processes running in `table` that are listed under `project` (see
    /// [`Book::kept_projects`](super::attribute::Book::kept_projects)), or with `None` every
    /// kept process. Never the daemon, one of its ancestors or one of its direct children.
    fn kept_members(&self, table: &Table, project: Option<&str>) -> HashSet<Identity> {
        let protected = table.protected(self.own);
        let book = self.book();
        book.kept
            .iter()
            .filter(|id| {
                project.is_none_or(|project| {
                    book.kept_projects.get(*id).map(String::as_str) == Some(project)
                })
            })
            .filter_map(|id| table.get(id.pid).filter(|proc| proc.id == *id))
            .filter(|proc| !protected.covers(proc))
            .map(|proc| proc.id)
            .collect()
    }

    /// Makes those of `targets` that are kept the user's no more, as processes of an ended
    /// launch, to be stopped, with the terminal sessions recorded as kept that no other kept
    /// process sits in.
    fn unkeep(&self, targets: &[Identity], table: &Table) {
        let mut book = self.book();
        let unkept: Vec<Identity> = targets
            .iter()
            .copied()
            .filter(|id| book.kept.contains(id))
            .collect();
        if unkept.is_empty() {
            return;
        }
        let mut sids: HashSet<i32> = HashSet::new();
        for id in &unkept {
            book.kept.remove(id);
            book.kept_projects.remove(id);
            let sid = table
                .get(id.pid)
                .filter(|proc| proc.id == *id)
                .map_or(0, |proc| proc.sid);
            sids.insert(sid);
            book.attributed.insert(
                *id,
                Attributed {
                    launch: ENDED.to_string(),
                    sid,
                },
            );
        }
        let still_kept: HashSet<i32> = table
            .procs()
            .filter(|proc| book.kept.contains(&proc.id))
            .map(|proc| proc.sid)
            .collect();
        for sid in sids.difference(&still_kept) {
            if let Some(leader_start) = book.kept_sessions.remove(sid) {
                book.kept_session_projects.remove(sid);
                if let Some(ended) = book.launches.get_mut(ENDED) {
                    ended.sessions.entry(*sid).or_insert(leader_start);
                }
            }
        }
        book.dirty = true;
    }

    /// The processes left behind in `dirs` (see [`Self::trees_of_project`]).
    fn leftover_members(&self, table: &Table, dirs: &[PathBuf]) -> HashSet<Identity> {
        let mut roots: Vec<PathBuf> = Vec::new();
        for dir in dirs {
            roots.push(dir.clone());
            // The system reports a working directory with its links resolved (`/private/tmp` for
            // `/tmp` on macOS).
            if let Ok(resolved) = std::fs::canonicalize(dir) {
                if resolved != *dir {
                    roots.push(resolved);
                }
            }
        }
        let protected = table.protected(self.own);
        // The terminal sessions the daemon's direct children lead: a shell terminal's, and what
        // the user runs in it.
        let users: HashSet<i32> = table
            .procs()
            .filter(|proc| proc.ppid == self.own && proc.sid == proc.id.pid)
            .map(|proc| proc.sid)
            .collect();
        let candidates: Vec<Identity> = {
            let book = self.book();
            let agents: Vec<Identity> = book
                .launches
                .values()
                .filter_map(|launch| launch.agent)
                .collect();
            table
                .procs()
                .filter(|proc| {
                    !protected.covers(proc)
                        && !users.contains(&proc.sid)
                        && !book.kept.contains(&proc.id)
                        && book
                            .attributed
                            .get(&proc.id)
                            .is_none_or(|attributed| attributed.launch == ENDED)
                        && !agents
                            .iter()
                            .any(|agent| *agent == proc.id || agent.pid == proc.sid)
                })
                .map(|proc| proc.id)
                .collect()
        };
        candidates
            .into_iter()
            .filter(|id| {
                table::cwd(*id).is_some_and(|cwd| {
                    cwd.missing && roots.iter().any(|root| cwd.path.starts_with(root))
                })
            })
            .collect()
    }
}

/// `members` grouped into trees by `table`'s parent chains: each member under the furthest
/// ancestor whose chain down to it is all members.
fn group(members: &HashSet<Identity>, table: &Table) -> BTreeMap<Identity, Vec<Identity>> {
    let mut trees: BTreeMap<Identity, Vec<Identity>> = BTreeMap::new();
    for id in members {
        let Some(proc) = table.get(id.pid) else {
            continue;
        };
        let top = table
            .ancestors(proc)
            .take_while(|ancestor| members.contains(&ancestor.id))
            .last()
            .map_or(*id, |ancestor| ancestor.id);
        trees.entry(top).or_default().push(*id);
    }
    trees
}

/// The project of `projects` (by id, with its directory) whose directory `cwd` is under.
fn project_of_dir(cwd: &Path, projects: &[(String, PathBuf)]) -> Option<String> {
    projects.iter().find_map(|(id, dir)| {
        let under = cwd.starts_with(dir)
            || std::fs::canonicalize(dir).is_ok_and(|resolved| cwd.starts_with(resolved));
        under.then(|| id.clone())
    })
}

/// What a client is shown of the tree topped by `top` with `members`, read now.
fn describe(
    top: Identity,
    members: &[Identity],
    session_id: Option<&str>,
    kept: bool,
) -> ProcessTree {
    let cwd = table::cwd(top);
    let pids: Vec<i32> = members.iter().map(|id| id.pid).collect();
    ProcessTree {
        id: top.key(),
        session_id: session_id.map(str::to_string),
        kept,
        command: command_of(top),
        cwd_missing: cwd.as_ref().is_some_and(|cwd| cwd.missing),
        cwd: cwd.map(|cwd| cwd.path.to_string_lossy().into_owned()),
        started_at: top.started_at_ms(),
        process_count: members.len(),
        memory_bytes: members.iter().filter_map(|id| table::memory(*id)).sum(),
        ports: table::listening_ports(&pids),
    }
}

/// `id`'s arguments joined by spaces, trimmed, and cut at [`COMMAND_LIMIT`] bytes.
fn command_of(id: Identity) -> String {
    let joined = table::argv(id).unwrap_or_default().join(" ");
    let trimmed = joined.trim();
    let mut end = trimmed.len().min(COMMAND_LIMIT);
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].to_string()
}
