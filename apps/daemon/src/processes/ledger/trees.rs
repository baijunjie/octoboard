//! The process trees a client is shown, and the stops and keeps it asks for by tree.
//!
//! A tree is named by its top process, and between a listing and the stop or keep that names one
//! of its trees the top may exit: a wrapper shell ends, and its child, reparented, is a top of its
//! own. So the ledger remembers, for each top a listing showed, its members and their descendants
//! seen by every scan since, for as long as they run, whatever was listed after; and a name whose
//! top is no longer one reaches those members still in a tree that qualifies, with what descends
//! from them there.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::attribute::{keep, Attributed, Launch, ENDED};
use super::looks::Roots;
use super::table::{self, Cwd, Identity, Table};
use super::ProcessLedger;
use crate::protocol::{ProcessCount, ProcessTree};

/// The longest command a tree reports, in bytes.
const COMMAND_LIMIT: usize = 1024;

/// How many tree tops are remembered at once: the oldest go first past it. Only the tops whose
/// members still run are kept at all, so this bounds what a long run of listings could make the
/// ledger hold.
const LISTINGS_LIMIT: usize = 1024;

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

    /// Of the trees `ids` name, those still running among the trees of `session`'s launches, as
    /// `ids` spell them: a tree whose top has exited, leaving a member to run on as a top of its
    /// own, is still the tree a listing named (see [`Self::resolve`]). What a launch that is
    /// ending is stopping counts as ended. `None` when the process table could not be read.
    /// **Blocks.**
    pub fn trees_still_running(&self, session: &str, ids: &[String]) -> Option<Vec<String>> {
        let table = self.scan(false)?;
        let of_session =
            |launch: &Launch| launch.session_id.as_deref() == Some(session) && !launch.ending;
        let named = self.resolve(ids, &table, &Roots::default(), of_session, false);
        Some(named.into_iter().map(|(id, _)| id).collect())
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
            let roots = self.roots(leftover_dirs);
            let members = self.leftover_members(&table, &roots);
            let members: HashSet<Identity> = members.into_keys().collect();
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
        let mut cwds = self.cwds(unowned.iter().map(|(top, _, _)| *top), None);
        trees.extend(
            unowned
                .into_iter()
                .map(|(top, members, kept)| describe(top, &members, None, kept, cwds.remove(&top))),
        );
        trees
    }

    /// How many trees each project and each console session has, from one read of the table and
    /// without describing any tree: one entry per project of `projects` (by id, in this order,
    /// with its directory when it is local) that has any, then one per console session of
    /// `sessions` (by id, with its project, `None` for a console session) that has any. A
    /// project's trees are those of its sessions' launches, the ones left behind in its directory
    /// and the ones kept under it. **Blocks**, where a directory does not answer, for the deadline
    /// of each pass of looks it makes: at the projects' directories ([`Self::roots`]), then at the
    /// working directories ([`Self::cwds`]).
    pub fn trees_counted(
        &self,
        sessions: &[(String, Option<String>)],
        projects: &[(String, Option<PathBuf>)],
    ) -> Vec<ProcessCount> {
        #[cfg(test)]
        self.counts_computed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let Some(table) = self.scan(false) else {
            return Vec::new();
        };
        let ids: Vec<String> = sessions.iter().map(|(id, _)| id.clone()).collect();
        let mut by_project: HashMap<String, usize> = HashMap::new();
        let mut by_console: HashMap<String, usize> = HashMap::new();
        for (session, _, _) in self.session_tops(&table, &ids) {
            match sessions.iter().find(|(id, _)| *id == session) {
                Some((_, Some(project))) => *by_project.entry(project.clone()).or_default() += 1,
                Some((id, None)) => *by_console.entry(id.clone()).or_default() += 1,
                None => {}
            }
        }
        let local: Vec<(String, PathBuf)> = projects
            .iter()
            .filter_map(|(id, dir)| dir.clone().map(|dir| (id.clone(), dir)))
            .collect();
        if !local.is_empty() {
            let dirs: Vec<PathBuf> = local.iter().map(|(_, dir)| dir.clone()).collect();
            let roots = self.roots(&dirs);
            let members = self.leftover_members(&table, &roots);
            let ids: HashSet<Identity> = members.keys().copied().collect();
            for (top, _) in group(&ids, &table) {
                let project = members
                    .get(&top)
                    .and_then(|cwd| roots.first_holding(&cwd.path));
                if let Some(index) = project {
                    *by_project.entry(local[index].0.clone()).or_default() += 1;
                }
            }
        }
        let kept = self.kept_members(&table, None);
        {
            let book = self.book();
            for (_, members) in group(&kept, &table) {
                let Some(project) = members
                    .iter()
                    .find_map(|member| book.kept_projects.get(member))
                else {
                    continue;
                };
                *by_project.entry(project.clone()).or_default() += 1;
            }
        }
        let of_projects = projects.iter().filter_map(|(id, _)| {
            Some(ProcessCount {
                project_id: Some(id.clone()),
                session_id: None,
                trees: by_project.get(id).copied()?,
            })
        });
        let of_consoles = sessions.iter().filter_map(|(id, _)| {
            Some(ProcessCount {
                project_id: None,
                session_id: Some(id.clone()),
                trees: by_console.get(id).copied()?,
            })
        });
        of_projects.chain(of_consoles).collect()
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

    /// Records `trees` as just listed for a client under `key` (see [`Self::listed_lately`]),
    /// unless what was listed has been forgotten since `generation` was read, before the trees
    /// were (see [`Self::lists_generation`]).
    pub fn note_listed(&self, key: String, generation: u64, trees: Vec<ProcessTree>) {
        let mut lists = self.lists.lock().expect("process lists lock poisoned");
        if self.lists_generation() != generation {
            return;
        }
        // A key per session and per project: past a few hundred, what is old enough to be of no
        // use any more is dropped.
        if lists.len() >= LISTINGS_LIMIT {
            lists.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
        }
        lists.insert(key, (Instant::now(), trees));
    }

    /// The counts of [`Self::trees_counted`] computed for a client less than `floor` ago, unless a
    /// stop, a keep or the end of a launch has changed what there is since (see
    /// [`Self::listed_lately`]).
    pub fn counted_lately(&self, floor: Duration) -> Option<Vec<ProcessCount>> {
        let counts = self.counts.lock().expect("process counts lock poisoned");
        counts
            .as_ref()
            .filter(|(at, _)| at.elapsed() < floor)
            .map(|(_, counts)| counts.clone())
    }

    /// Records `counts` as just computed for a client (see [`Self::counted_lately`]), unless what
    /// was counted has been forgotten since `generation` was read, before the counts were (see
    /// [`Self::lists_generation`]).
    pub fn note_counted(&self, generation: u64, counts: Vec<ProcessCount>) {
        let mut noted = self.counts.lock().expect("process counts lock poisoned");
        if self.lists_generation() == generation {
            *noted = Some((Instant::now(), counts));
        }
    }

    /// How many times what was listed and counted for clients has been forgotten: read before a
    /// listing or a count reads the table, and handed to [`Self::note_listed`] or
    /// [`Self::note_counted`], so that one read before a stop, a keep or the end of a launch is
    /// not noted after it and answered for up to the floor as if it came after.
    pub fn lists_generation(&self) -> u64 {
        self.lists_generation
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Waits until no other request is computing the counts, and claims that for the caller:
    /// requests that overlap wait for the one computation and are answered by what it noted
    /// ([`Self::note_counted`]), instead of each reading the table. The claim is the guard, and
    /// ends however its holder does.
    pub async fn claim_counting(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.counting).lock_owned().await
    }

    /// Forgets what was listed and counted for clients, after something changed what a listing
    /// would show.
    pub(super) fn forget_lists(&self) {
        // Moved on before the clearing, which takes the lock a note checks the generation under,
        // so a note either lands before the clearing or sees the new generation and is dropped.
        self.lists_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.lists
            .lock()
            .expect("process lists lock poisoned")
            .clear();
        *self.counts.lock().expect("process counts lock poisoned") = None;
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
        let roots = self.roots(leftover_dirs);
        let named = self.resolve(ids, &table, &roots, |_| true, true);
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
        let roots = self.roots(&leftover_dirs);
        let named = self.resolve(ids, &table, &roots, |launch| !launch.ending, false);
        // Read before the lock is taken, and only what may lie in a project: only a tree no
        // session owns is placed by its directory.
        let cwds = self.cwds(
            named
                .iter()
                .flat_map(|(_, members)| members.iter().copied()),
            Some(&roots),
        );
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
                None => members.iter().find_map(|member| {
                    let index = roots.first_holding(&cwds.get(member)?.path)?;
                    Some(projects[index].0.clone())
                }),
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
        let roots = self.roots(leftover_dirs);
        let mut members: HashSet<Identity> =
            self.leftover_members(&table, &roots).into_keys().collect();
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
        let named = self.resolve(&wanted, &table, &Roots::default(), of_from, false);
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

    /// The tops of the trees of `sessions` in `table`, each with its session and members.
    fn session_tops(
        &self,
        table: &Table,
        sessions: &[String],
    ) -> Vec<(String, Identity, Vec<Identity>)> {
        let launches = self.launch_trees(table);
        let mut found: Vec<(String, Identity, Vec<Identity>)> = Vec::new();
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
        found
    }

    /// The trees of `sessions` in `table` (see [`Self::trees_of_sessions`]), every one of them
    /// remembered whatever its age.
    fn session_trees(
        &self,
        table: &Table,
        sessions: &[String],
        min_age: Duration,
    ) -> Vec<ProcessTree> {
        let mut found = self.session_tops(table, sessions);
        self.remember(
            found
                .iter()
                .map(|(_, top, members)| (*top, members.as_slice())),
        );
        let oldest = table::now().saturating_sub(min_age.as_micros() as u64);
        found.retain(|(_, top, _)| top.start <= oldest);
        found.sort_by_key(|(_, top, _)| (top.start, top.pid));
        let mut cwds = self.cwds(found.iter().map(|(_, top, _)| *top), None);
        found
            .into_iter()
            .map(|(session, top, members)| {
                describe(top, &members, Some(&session), false, cwds.remove(&top))
            })
            .collect()
    }

    /// The trees `ids` name, each with the members to act on as `table` shows them, among the
    /// trees of the launches being followed that `include` accepts and those left behind in
    /// `roots`' directories. A name whose top is not a top any more reaches the members remembered
    /// for it that are in one of those trees, with what descends from them there. Each process is
    /// given to the first name that reaches it.
    fn resolve(
        &self,
        ids: &[String],
        table: &Table,
        roots: &Roots,
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
        if !roots.is_empty() {
            let leftovers: HashSet<Identity> =
                self.leftover_members(table, roots).into_keys().collect();
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
    /// kept process. Never a daemon-managed process or an internal in its child's session.
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

    /// The processes left behind in `roots`' directories (see [`Self::trees_of_project`]), each
    /// with its working directory. Only a process whose directory the system reports under one
    /// of them has the directory looked at, within the deadline of a look.
    fn leftover_members(&self, table: &Table, roots: &Roots) -> HashMap<Identity, Cwd> {
        let protected = table.protected(self.own);
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
        let mut cwds = self.cwds(candidates, Some(roots));
        cwds.retain(|_, cwd| cwd.missing && roots.holds(&cwd.path));
        cwds
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

/// What a client is shown of the tree topped by `top` with `members`, read now but for `cwd`,
/// the top's working directory as looked at already.
fn describe(
    top: Identity,
    members: &[Identity],
    session_id: Option<&str>,
    kept: bool,
    cwd: Option<Cwd>,
) -> ProcessTree {
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

#[cfg(test)]
mod tests {
    use super::super::table::Proc;
    use super::*;

    /// A listing or a count read before what was listed is forgotten — by a stop, a keep, the end
    /// of a launch — is not noted after it, where it would answer for up to the floor as if read
    /// after; one read since is.
    #[test]
    fn a_list_or_count_read_before_what_was_listed_is_forgotten_is_not_noted() {
        let dir = crate::test_support::ScratchDir::new("ledger-lists-generation");
        let ledger = ProcessLedger::new(&dir);
        let counts = vec![ProcessCount {
            project_id: Some("project".to_string()),
            session_id: None,
            trees: 1,
        }];
        let before = ledger.lists_generation();
        ledger.forget_lists();
        ledger.note_counted(before, counts.clone());
        ledger.note_listed("key".to_string(), before, Vec::new());
        assert_eq!(ledger.counted_lately(Duration::MAX), None);
        assert_eq!(ledger.listed_lately("key", Duration::MAX), None);

        let after = ledger.lists_generation();
        ledger.note_counted(after, counts.clone());
        ledger.note_listed("key".to_string(), after, Vec::new());
        assert_eq!(ledger.counted_lately(Duration::MAX), Some(counts));
        assert_eq!(ledger.listed_lately("key", Duration::MAX), Some(Vec::new()));
    }

    #[test]
    fn a_previously_kept_internal_is_not_listed_as_a_project_process() {
        let dir = crate::test_support::ScratchDir::new("ledger-kept-internal");
        let ledger = ProcessLedger::new(&dir);
        let server = Proc {
            id: Identity {
                pid: 300,
                start: 1000,
            },
            ppid: ledger.own,
            sid: 300,
            comm: String::new(),
        };
        let helper = Proc {
            id: Identity {
                pid: 301,
                start: 1001,
            },
            ppid: 300,
            sid: 300,
            comm: String::new(),
        };
        let task = Proc {
            id: Identity {
                pid: 302,
                start: 1002,
            },
            ppid: 300,
            sid: 302,
            comm: String::new(),
        };
        let table = Table::from_procs([server, helper.clone(), task.clone()]);
        {
            let mut book = ledger.book();
            for proc in [&helper, &task] {
                book.kept.insert(proc.id);
                book.kept_projects.insert(proc.id, "project".to_string());
            }
        }
        assert_eq!(
            ledger.kept_members(&table, Some("project")),
            HashSet::from([task.id])
        );
    }
}
