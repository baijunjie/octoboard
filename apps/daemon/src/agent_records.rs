//! Where an agent keeps its own on-disk records of a session: Claude Code's transcript (and the
//! directory of subagent transcripts beside it), Codex's rollout files (compressed or not; one
//! per history segment of a thread), Grok Build's session directory (marked by its `usage.json`).
//! Shared by the record collector, which reads them (`record_collector.rs`), and by a session's
//! deletion, which moves them to the Trash (`record_paths`).
//!
//! A record is found by its name, which carries the agent session id, in one account's own
//! directory (the path a hook reported is only a hint). Both the id and the hint come from the
//! agent's hook payload, which nothing authenticates, so everything that names a path to remove
//! holds to one rule: the id is a canonical UUID, and every path is built from the account's records
//! directory and that id, never from the payload's own path.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde_json::Value;

use crate::hostfs::lexically_normalise;
use crate::protocol::{Account, Agent, Session};
use crate::state::AppState;

/// The record path each session's hook last reported, with the agent session id it reported it for.
#[derive(Default)]
pub struct Hints {
    hints: Mutex<HashMap<String, (String, PathBuf)>>,
}

impl Hints {
    /// Forgets the sessions not in `ids` and returns what is left.
    pub fn retain(&self, ids: &HashSet<&str>) -> HashMap<String, (String, PathBuf)> {
        let mut hints = self.hints.lock().expect("record hint lock poisoned");
        hints.retain(|id, _| ids.contains(id.as_str()));
        hints.clone()
    }

    fn hint(&self, session: &Session) -> Option<PathBuf> {
        let hints = self.hints.lock().expect("record hint lock poisoned");
        hinted_path(&hints, session)
    }
}

/// The record path a hook reported for `session`, when it was reported for the session's current
/// agent session id. Grok's hook reports none that names a record.
pub fn hinted_path(
    hints: &HashMap<String, (String, PathBuf)>,
    session: &Session,
) -> Option<PathBuf> {
    let (id, path) = hints.get(&session.id)?;
    (Some(id) == session.agent_session_id.as_ref() && session.agent != Agent::Grok)
        .then(|| path.clone())
}

/// The main session's hook provides the exact record path, including a newly created record.
/// Subagent hooks are filtered by the caller before this point.
pub fn on_hook(state: &AppState, id: &str, payload: &Value) {
    let Some(path) = payload.get("transcript_path").and_then(Value::as_str) else {
        return;
    };
    let Some(agent_id) = payload.get("session_id").and_then(Value::as_str) else {
        return;
    };
    state
        .record_hints
        .hints
        .lock()
        .expect("record hint lock poisoned")
        .insert(id.to_owned(), (agent_id.to_owned(), PathBuf::from(path)));
}

/// Every record of one agent in one account, by agent session id, listed once and kept for as long
/// as the `Locator` lives.
#[derive(Default)]
struct Index {
    records: HashMap<String, Vec<PathBuf>>,
    complete: bool,
}

/// Resolves accounts' directories and lists their records. The login-shell snapshot that says where
/// a default account lives is taken only when something asks for one, and an unknown location is
/// unknown, never a fallback to a different account's history.
#[derive(Default)]
pub struct Locator {
    shell: OnceCell<Option<HashMap<String, String>>>,
    indexes: HashMap<(Agent, PathBuf), Index>,
}

impl Locator {
    /// A locator that resolves default accounts through `shell`, a login-shell snapshot already
    /// taken, instead of asking for one.
    pub fn with_shell(shell: HashMap<String, String>) -> Self {
        Self {
            shell: OnceCell::from(Some(shell)),
            indexes: HashMap::new(),
        }
    }

    /// The account directory `config_dir` names, or the agent's default account when it is `None`.
    pub fn root(&self, agent: Agent, config_dir: Option<&str>) -> Option<PathBuf> {
        match config_dir {
            Some(dir) => Some(PathBuf::from(dir)),
            None => {
                let shell = self
                    .shell
                    .get_or_init(|| crate::env_shell::cached_snapshot().ok())
                    .as_ref()?;
                Some(crate::adapter::default_account_dir(agent, shell))
            }
        }
    }

    fn index(&mut self, agent: Agent, root: &Path) -> &Index {
        self.indexes
            .entry((agent, root.to_path_buf()))
            .or_insert_with(|| index_records(agent, root))
    }

    /// The record of `agent_id` in the account at `root` that holds the conversation as it is now:
    /// the newest one, and for Codex the head of the thread's chain of rollouts.
    pub fn current_record(&mut self, agent: Agent, root: &Path, agent_id: &str) -> Option<PathBuf> {
        let paths = self.index(agent, root).records.get(agent_id)?;
        if agent == Agent::Codex {
            return rollout_head(paths);
        }
        let mut newest: Option<&PathBuf> = None;
        for path in paths {
            if newest.is_none_or(|old| modified(path) > modified(old)) {
                newest = Some(path);
            }
        }
        newest.cloned()
    }

    /// What Codex thread `agent_id`'s conversation as it stands is made of, ending in `head` (see
    /// [`rollout_branch`]), in the account at `root`.
    pub fn rollout_stretches(
        &mut self,
        root: &Path,
        agent_id: &str,
        head: &Path,
    ) -> Option<Vec<Stretch>> {
        rollout_branch(head, self.index(Agent::Codex, root).records.get(agent_id)?)
    }

    /// The first rollout of Codex thread `agent_id` in the account at `root`, the file the thread's
    /// name alone gives, when it is on disk.
    pub fn first_rollout(&mut self, root: &Path, agent_id: &str) -> Option<PathBuf> {
        self.index(Agent::Codex, root)
            .records
            .get(agent_id)?
            .iter()
            .find(|path| path_stem(path).and_then(own_id) == Some(agent_id))
            .cloned()
    }
}

/// Where an agent keeps the titles of its conversations, besides the lines of the conversation's
/// own record.
#[derive(Default)]
pub struct TitleFiles {
    /// A file directly under the account's directory that names every conversation of the
    /// account together.
    pub shared: Option<&'static str>,
    /// A file beside the record that names the conversation alone.
    pub summary: Option<&'static str>,
}

pub fn title_files(agent: Agent) -> TitleFiles {
    match agent {
        Agent::Claude => TitleFiles::default(),
        Agent::Codex => TitleFiles {
            shared: Some("session_index.jsonl"),
            summary: None,
        },
        Agent::Grok => TitleFiles {
            shared: None,
            summary: Some("summary.json"),
        },
    }
}

/// The directory of an account where an agent keeps its session records.
fn records_dir(agent: Agent, root: &Path) -> PathBuf {
    root.join(match agent {
        Agent::Claude => "projects",
        Agent::Codex | Agent::Grok => "sessions",
    })
}

/// What a [`walk_records`] visitor decides about one entry.
enum Step {
    /// The entry is a record (or otherwise settled): do not descend into it.
    Matched,
    /// Descend into it when it is a directory with levels left.
    Descend,
    /// Stop the whole walk.
    Stop,
}

/// How a [`walk_records`] pass ended.
struct Walked {
    /// Every directory was listed and no link was met, so what was not found is not there.
    complete: bool,
    stopped: bool,
}

/// Visits the entries of `dir`, at most `depth` levels down (`depth` 1 is `dir`'s own entries),
/// without following a link. `visit` gets each entry's path, name, type and the levels left
/// counting its own. A missing `dir` is empty; a link, an unreadable entry or a name that is not
/// UTF-8 makes the pass incomplete and is not visited.
fn walk_records(
    dir: &Path,
    depth: usize,
    visit: &mut dyn FnMut(&Path, &str, std::fs::FileType, usize) -> Step,
) -> Walked {
    let mut out = Walked {
        complete: true,
        stopped: false,
    };
    walk_level(dir, depth, visit, &mut out);
    out
}

fn walk_level(
    dir: &Path,
    depth: usize,
    visit: &mut dyn FnMut(&Path, &str, std::fs::FileType, usize) -> Step,
    out: &mut Walked,
) {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        _ => {
            out.complete = false;
            return;
        }
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        out.complete = false;
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            out.complete = false;
            continue;
        };
        let (Ok(kind), Some(name)) = (
            entry.file_type(),
            entry.file_name().to_str().map(str::to_owned),
        ) else {
            out.complete = false;
            continue;
        };
        if kind.is_symlink() {
            out.complete = false;
            continue;
        }
        let path = entry.path();
        match visit(&path, &name, kind, depth) {
            Step::Stop => {
                out.stopped = true;
                return;
            }
            Step::Descend if kind.is_dir() && depth > 1 => {
                walk_level(&path, depth - 1, visit, out);
                if out.stopped {
                    return;
                }
            }
            Step::Descend | Step::Matched => {}
        }
    }
}

/// Index names once per account per pass; never read other sessions' contents or follow links.
fn index_records(agent: Agent, root: &Path) -> Index {
    let mut out = Index::default();
    let depth = match agent {
        Agent::Claude | Agent::Grok => 2,
        Agent::Codex => 6,
    };
    let mut visit = |path: &Path, name: &str, kind: std::fs::FileType, left: usize| {
        // Claude's and Grok's records sit on the last level, below a working-directory folder.
        let leaf = left == 1;
        let id = match agent {
            Agent::Claude if kind.is_file() && leaf => {
                name.strip_suffix(".jsonl").map(str::to_owned)
            }
            // A subagent directory may survive after the main transcript was removed.
            Agent::Claude if kind.is_dir() && leaf && plausible_id(name) => Some(name.to_owned()),
            Agent::Codex if kind.is_file() && name.starts_with("rollout-") => rollout_stem(name)
                .and_then(rollout_thread_id)
                .map(str::to_owned),
            Agent::Grok if kind.is_dir() && leaf => Some(name.to_owned()),
            _ => None,
        };
        let Some(id) = id else {
            return Step::Descend;
        };
        let path = match agent {
            Agent::Grok => path.join("usage.json"),
            Agent::Claude if kind.is_dir() => path.with_file_name(format!("{id}.jsonl")),
            _ => path.to_path_buf(),
        };
        let records = out.records.entry(id).or_default();
        if !records.contains(&path) {
            records.push(path);
        }
        Step::Matched
    };
    let walked = walk_records(&records_dir(agent, root), depth, &mut visit);
    out.complete = walked.complete
        && !std::fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_symlink());
    out
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Whether a directory entry is the rollout file of the Codex thread `agent_id`; the name and the
/// id both lowercased.
fn is_rollout_of(kind: std::fs::FileType, name: &str, agent_id: &str) -> bool {
    kind.is_file()
        && name.starts_with("rollout-")
        && rollout_stem(name).is_some_and(|stem| names_thread(stem, agent_id))
}

/// A Codex rollout's file name without its extension: `.jsonl`, or `.jsonl.zst` once Codex has
/// compressed it (it does so for rollouts older than a week).
pub fn rollout_stem(name: &str) -> Option<&str> {
    name.strip_suffix(".jsonl")
        .or_else(|| name.strip_suffix(".jsonl.zst"))
}

/// The thread id a rollout belongs to, from its [`rollout_stem`]: a thread's first rollout is
/// `rollout-<start>-<thread id>`, and every later history segment (started by reverting a turn)
/// `rollout-<start>-<thread id>_<segment id>`. A subagent's rollout is named by its own thread id
/// and never contains its parent's.
pub fn rollout_thread_id(stem: &str) -> Option<&str> {
    let tail = stem.len().checked_sub(36)?;
    match stem.get(..tail)?.strip_suffix('_') {
        Some(first) => first.get(first.len().checked_sub(36)?..),
        None => stem.get(tail..),
    }
}

/// Whether the rollout with this [`rollout_stem`] is a file of thread `id`, its first rollout or a
/// later segment.
pub fn names_thread(stem: &str, id: &str) -> bool {
    let segment_at = stem
        .len()
        .checked_sub(37)
        .filter(|&at| stem.as_bytes()[at] == b'_');
    let first = segment_at.and_then(|at| stem.get(..at)).unwrap_or(stem);
    first
        .strip_suffix(id)
        .is_some_and(|rest| rest.ends_with('-'))
}

/// A rollout's file name without its extension, see [`rollout_stem`].
fn path_stem(path: &Path) -> Option<&str> {
    rollout_stem(path.file_name()?.to_str()?)
}

/// The id a rollout is continued by: the last 36 characters of its [`rollout_stem`], the thread id
/// for a first rollout and the segment id for a later one.
fn own_id(stem: &str) -> Option<&str> {
    stem.get(stem.len().checked_sub(36)?..)
}

/// Whether the rollout at `path` is a later history segment of its thread, not its first rollout.
pub fn is_segment(path: &Path) -> bool {
    path_stem(path).is_some_and(|stem| rollout_thread_id(stem) != own_id(stem))
}

/// The head of the chain the rollouts `paths` of one thread form: the file no other continues,
/// which holds the conversation as it stands. Each segment's first line names the file it
/// continues from (`history_base.thread_id`: the first rollout by the thread id, a later segment
/// by its segment id, which is also the last 36 characters of its own name). Of several files
/// nothing continues, as when a fresh history was started or a shared directory split into
/// branches, the one started last by name wins, then the newest by modification time; never mtime
/// alone, which a write to an earlier segment moves.
pub fn rollout_head(paths: &[PathBuf]) -> Option<PathBuf> {
    let continued: HashSet<String> = paths
        .iter()
        .filter_map(|path| Some(history_base(path)?.0))
        .collect();
    let files: Vec<(&str, &PathBuf)> = paths
        .iter()
        .filter_map(|path| Some((path_stem(path)?, path)))
        .collect();
    files
        .iter()
        .filter(|(stem, _)| own_id(stem).is_none_or(|own| !continued.contains(own)))
        .max_by_key(|(stem, path)| (*stem, modified(path)))
        // Every file is continued by another: a loop, which has no head by lineage.
        .or_else(|| files.iter().max_by_key(|(stem, _)| *stem))
        .map(|(_, path)| (*path).clone())
}

/// Where the rollout at `path` picks up: the id of the file it continues and how many bytes of that
/// file (decompressed, ending on a line) come before it, from the `history_base` of its
/// `session_meta` line. `None` for a first rollout or a segment that started a fresh history.
fn history_base(path: &Path) -> Option<(String, u64)> {
    use std::io::{BufRead, Read};
    let mut reader = crate::line_file::open(path, 0).ok()?;
    let mut line = Vec::new();
    reader
        .by_ref()
        .take(HEAD_LINE_LIMIT)
        .read_until(b'\n', &mut line)
        .ok()?;
    let meta: Value = serde_json::from_slice(&line).ok()?;
    let base = meta.pointer("/payload/history_base")?;
    Some((
        base.get("thread_id")?.as_str()?.to_owned(),
        base.get("end_byte_offset")?.as_u64()?,
    ))
}

/// A file of a conversation and how much of it counts: the bytes up to a later segment's cut, or
/// all of it for the head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stretch {
    pub path: PathBuf,
    /// How many bytes (decompressed) of the file count; `None` for all of it.
    pub cut: Option<u64>,
}

/// What the conversation as it stands is made of, from the first rollout to the head: the head in
/// full, preceded by each file it continues up to the point it continues from, back to the root
/// (the head's `history_base`, then that file's own). `None` when a file the chain names is not
/// among `files`, or the chain loops back on itself, so the conversation cannot be put together
/// whole.
pub fn rollout_branch(head: &Path, files: &[PathBuf]) -> Option<Vec<Stretch>> {
    let mut stretches = vec![Stretch {
        path: head.to_path_buf(),
        cut: None,
    }];
    let mut at = head.to_path_buf();
    while let Some((base, cut)) = history_base(&at) {
        let file = files
            .iter()
            .filter(|path| path_stem(path).and_then(own_id) == Some(base.as_str()))
            .max_by_key(|path| modified(path))?;
        if stretches.len() > files.len() {
            return None;
        }
        stretches.insert(
            0,
            Stretch {
                path: file.clone(),
                cut: Some(cut),
            },
        );
        at = file.clone();
    }
    Some(stretches)
}

/// The most of a rollout's first line read to find what it continues; the line carries the thread's
/// instructions and can be long, but not this long.
const HEAD_LINE_LIMIT: u64 = 4 * 1024 * 1024;

/// The agent's own records of `session`, for the ones to be moved to the Trash with it: Claude's
/// transcript and the directory of the same name beside it that holds its subagents' transcripts,
/// Codex's rollout files (the first one and every later history segment of the thread), Grok's
/// session directory. Found everywhere on this machine the agent's accounts could hold them (the
/// session's own account, every account Octoboard knows of for the agent, and the default one), as
/// a switched session leaves its original copy behind in the old account, and every copy in every
/// Claude project directory.
///
/// Nothing but the session's own records is ever named: an agent session id that is not a canonical
/// UUID yields nothing, each path is built from an account's records directory and that id, a
/// hinted path counts only when it lies lexically inside one of those directories, and every
/// record must sit at its agent's exact layout depth under real (not linked) directories. A path
/// may not exist.
pub fn record_paths(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> Vec<PathBuf> {
    let Some(agent_id) = session.agent_session_id.as_deref() else {
        return Vec::new();
    };
    if !plausible_id(agent_id) {
        return Vec::new();
    }
    let agent = session.agent;
    let (roots, _) = account_roots(locator, accounts, session);
    // Each record with the records directory it was found under.
    let mut records: Vec<(PathBuf, PathBuf)> = Vec::new();
    for root in &roots {
        let dir = records_dir(agent, root);
        for record in locator
            .index(agent, root)
            .records
            .get(agent_id)
            .into_iter()
            .flatten()
        {
            records.push((dir.clone(), record.clone()));
        }
    }
    if let Some(hint) = hints.hint(session) {
        let hint = lexically_normalise(&hint);
        let home = roots
            .iter()
            .map(|root| records_dir(agent, root))
            .find(|dir| hint.starts_with(dir));
        if let (true, Some(dir)) = (hint.is_absolute(), home) {
            records.push((dir, hint));
        }
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    for (dir, record) in records {
        if !in_real_directories(&record, &dir) {
            continue;
        }
        for path in own_paths(agent, agent_id, &record, &dir) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

fn account_roots(
    locator: &Locator,
    accounts: &[Account],
    session: &Session,
) -> (Vec<PathBuf>, bool) {
    let agent = session.agent;
    let mut candidates = vec![locator.root(agent, session.config_dir.as_deref())];
    for account in accounts.iter().filter(|account| account.agent == agent) {
        candidates.push(locator.root(agent, Some(&account.config_dir)));
    }
    candidates.push(locator.root(agent, None));
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut complete = true;
    for root in candidates {
        let Some(root) = root else {
            complete = false;
            continue;
        };
        let root = lexically_normalise(&root);
        if !root.is_absolute() {
            complete = false;
        } else if !roots.contains(&root) {
            roots.push(root);
        }
    }
    (roots, complete)
}

/// The file that holds `session`'s conversation, for reading, and what it is read from.
pub struct Branch {
    /// Claude Code's transcript, Codex's rollout (the head of the thread's chain, see
    /// [`rollout_head`]), Grok Build's `chat_history.jsonl`.
    pub record: PathBuf,
    /// The files the conversation is read from, in order: the record itself for Claude Code and
    /// Grok Build, and for Codex the whole current branch of a thread that had a turn reverted
    /// (see [`rollout_branch`]). `None` when a Codex thread's chain is missing a file.
    pub stretches: Option<Vec<Stretch>>,
}

/// The conversation of `session`, found the way [`record_paths`] finds a session's records, so a
/// session switched to another account is found where it left its copy; when more than one copy
/// exists the most recently modified is taken. `None` when there is no record.
pub fn conversation_branch(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> Option<Branch> {
    let files: Vec<PathBuf> = record_paths(hints, locator, accounts, session)
        .into_iter()
        .map(|path| match session.agent {
            Agent::Grok => path.join("chat_history.jsonl"),
            Agent::Claude | Agent::Codex => path,
        })
        .filter(|path| std::fs::metadata(path).is_ok_and(|meta| meta.is_file()))
        .collect();
    if session.agent == Agent::Codex {
        let record = rollout_head(&files)?;
        let stretches = rollout_branch(&record, &files);
        return Some(Branch { record, stretches });
    }
    let record = files.into_iter().max_by_key(|path| modified(path))?;
    let stretches = Some(vec![Stretch {
        path: record.clone(),
        cut: None,
    }]);
    Some(Branch { record, stretches })
}

/// [`conversation_branch`], and for a Codex thread that `codex archive` moved out of `sessions/`,
/// the rollouts it left in the account's `archived_sessions/`. That one a resume refuses, but the
/// conversation in it can still be read.
pub fn history_branch(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> Option<Branch> {
    conversation_branch(hints, locator, accounts, session)
        .or_else(|| archived_codex_branch(locator, accounts, session))
}

fn archived_codex_branch(
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> Option<Branch> {
    let agent_id = session
        .agent_session_id
        .as_deref()
        .filter(|id| session.agent == Agent::Codex && plausible_id(id))?
        .to_ascii_lowercase();
    let (roots, _) = account_roots(locator, accounts, session);
    let mut files: Vec<PathBuf> = Vec::new();
    for root in roots {
        walk_records(
            &root.join("archived_sessions"),
            6,
            &mut |path, name, kind, _| {
                if is_rollout_of(kind, &name.to_ascii_lowercase(), &agent_id) {
                    files.push(path.to_path_buf());
                    Step::Matched
                } else {
                    Step::Descend
                }
            },
        );
    }
    let record = rollout_head(&files)?;
    let stretches = rollout_branch(&record, &files);
    Some(Branch { record, stretches })
}

/// Whether all known account locations can prove the absence of this session's native records.
/// Missing ids and unreadable or linked directories cannot establish absence.
pub fn presence(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> crate::protocol::SessionRecordPresence {
    use crate::protocol::SessionRecordPresence::{Absent, Present, Unknown};
    if !session
        .agent_session_id
        .as_deref()
        .is_some_and(plausible_id)
    {
        return Unknown;
    }
    let (roots, mut complete) = account_roots(locator, accounts, session);
    for root in &roots {
        complete &= locator.index(session.agent, root).complete;
    }
    for path in record_paths(hints, locator, accounts, session) {
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_file() || meta.file_type().is_dir() => return Present,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            _ => complete = false,
        }
    }
    if complete {
        Absent
    } else {
        Unknown
    }
}

/// Whether a session's conversation can still be resumed, judged from the agent's own records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resumability {
    Resumable,
    /// Proven gone: the record does not exist in the account that holds it. Only this answer may
    /// be acted on destructively.
    Unresumable,
    /// Not provable either way: a location that cannot be resolved or read (a pinned account
    /// directory included), a linked records directory, or a Grok session folder missing locally,
    /// which Grok restores from its remote registry.
    Unknown,
}

/// What [`judge`] found for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judgement {
    pub resumability: Resumability,
    /// Resuming first needs `codex unarchive`: the rollout sits in the account's
    /// `archived_sessions/`, which a resume refuses, and `sessions/` does not show it present.
    pub needs_unarchive: bool,
}

/// Where a session's record sits in its own account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Located {
    Present,
    /// Codex's `archived_sessions/`.
    Archived,
    Missing,
    Uncertain,
}

/// Whether `session`'s conversation can be resumed, looking only at the session's own account
/// (`Session.config_dir`, the agent's default account when none) and its current agent session id.
/// A copy left in another account by an earlier switch does not count. A session with no
/// conversation always is: it relaunches into a fresh one.
///
/// Claude Code needs `<id>.jsonl` in some project directory (the working directory's slug is not
/// required); a subagent directory alone is not a record. Codex needs a rollout, plain or `.zst`,
/// under `sessions/` or `archived_sessions/`; any file of the thread's chain (its first rollout or
/// a later history segment) counts, so a thread is never judged gone while a part of it is there.
/// Grok needs its session directory, and its absence is only ever `Unknown`. File names are
/// matched whatever the case of the id.
pub fn resumability(locator: &Locator, session: &Session) -> Resumability {
    judge(locator, session).resumability
}

/// [`resumability`], with whether resuming first needs `codex unarchive`, from one look at the
/// account's records.
pub fn judge(locator: &Locator, session: &Session) -> Judgement {
    let (resumability, needs_unarchive) = match locate_own_record(locator, session) {
        Err(answer) => (answer, false),
        Ok(Located::Present) => (Resumability::Resumable, false),
        Ok(Located::Archived) => (Resumability::Resumable, true),
        Ok(Located::Missing) if session.agent == Agent::Grok => (Resumability::Unknown, false),
        Ok(Located::Missing) => (Resumability::Unresumable, false),
        Ok(Located::Uncertain) => (Resumability::Unknown, false),
    };
    Judgement {
        resumability,
        needs_unarchive,
    }
}

/// `Err` carries the answer when the record need not, or cannot, be looked for.
fn locate_own_record(locator: &Locator, session: &Session) -> Result<Located, Resumability> {
    if !session.has_conversation {
        return Err(Resumability::Resumable);
    }
    let agent = session.agent;
    let Some(agent_id) = session
        .agent_session_id
        .as_deref()
        .filter(|id| plausible_id(id))
    else {
        return Err(Resumability::Unknown);
    };
    let Some(root) = locator.root(agent, session.config_dir.as_deref()) else {
        return Err(Resumability::Unknown);
    };
    let root = lexically_normalise(&root);
    if !root.is_absolute() {
        return Err(Resumability::Unknown);
    }
    // An account directory that cannot be read (gone, unmounted, denied) says nothing about the
    // records in it.
    if std::fs::read_dir(&root).is_err() {
        return Err(Resumability::Unknown);
    }
    let (dirs, depth): (&[&str], usize) = match agent {
        Agent::Claude => (&["projects"], 2),
        Agent::Codex => (&["sessions", "archived_sessions"], 6),
        Agent::Grok => (&["sessions"], 2),
    };
    // An account directory with none of the agent's record directories (an unmounted mount point
    // leaves an empty one) has never shown any record to be missing from.
    if !dirs
        .iter()
        .any(|dir| std::fs::symlink_metadata(root.join(dir)).is_ok())
    {
        return Err(Resumability::Unknown);
    }
    let agent_id = agent_id.to_ascii_lowercase();
    let is_record = |name: &str, kind: std::fs::FileType| {
        let name = name.to_ascii_lowercase();
        match agent {
            Agent::Claude => kind.is_file() && name == format!("{agent_id}.jsonl"),
            Agent::Codex => is_rollout_of(kind, &name, &agent_id),
            Agent::Grok => kind.is_dir() && name == agent_id,
        }
    };
    // The first directory is the one a resume reads; a record found only in a later one is
    // archived. A directory that cannot be searched does not stop the next from being.
    let (mut archived, mut uncertain) = (false, false);
    for (at, dir) in dirs.iter().enumerate() {
        match scan_for_record(&root.join(dir), depth, &is_record) {
            Located::Present if at == 0 => return Ok(Located::Present),
            Located::Present => archived = true,
            Located::Uncertain => uncertain = true,
            _ => {}
        }
    }
    Ok(if archived {
        Located::Archived
    } else if uncertain {
        Located::Uncertain
    } else {
        Located::Missing
    })
}

/// Looks for an entry `is_record` accepts in `dir`, at most `depth` levels down, without following
/// a link. A link anywhere, or anything unreadable, leaves a record that is not found `Uncertain`
/// rather than `Missing`, since it could be behind that.
fn scan_for_record(
    dir: &Path,
    depth: usize,
    is_record: &dyn Fn(&str, std::fs::FileType) -> bool,
) -> Located {
    let walked = walk_records(dir, depth, &mut |_, name, kind, _| {
        if is_record(name, kind) {
            Step::Stop
        } else {
            Step::Descend
        }
    });
    if walked.stopped {
        Located::Present
    } else if walked.complete {
        Located::Missing
    } else {
        Located::Uncertain
    }
}

/// Whether `id` is a canonical UUID, the shape of every agent's session ids.
pub fn plausible_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// Whether every directory from `record`'s parent up to `dir`, inclusive, is a real directory and
/// not a link to one, so that what lies under it is where it appears to be.
fn in_real_directories(record: &Path, dir: &Path) -> bool {
    let mut at = record.parent();
    while let Some(current) = at {
        if !std::fs::symlink_metadata(current).is_ok_and(|meta| meta.file_type().is_dir()) {
            return false;
        }
        if current == dir {
            return true;
        }
        at = current.parent();
    }
    false
}

/// What `record` stands for, when it is named for `agent_id` the way the agent names its own and
/// sits exactly where the agent puts it, under `dir` (the account's records directory): Claude
/// `<dir>/<project>/<id>.jsonl`, Codex `<dir>/<year>/<month>/<day>/rollout-<time>-<id>.jsonl` (or
/// `.jsonl.zst`; a later segment adds `_<segment id>`), Grok
/// `<dir>/<working directory>/<id>/usage.json`. No other depth is taken, so a name that merely
/// equals a directory's, or a file inside a session's own directory, never names that directory.
fn own_paths(agent: Agent, agent_id: &str, record: &Path, dir: &Path) -> Vec<PathBuf> {
    let name = record.file_name().and_then(|name| name.to_str());
    let Some(parent) = record.parent() else {
        return Vec::new();
    };
    let levels_below_dir = record
        .strip_prefix(dir)
        .map(|rest| rest.components().count());
    match (agent, levels_below_dir) {
        (Agent::Claude, Ok(2)) if name == Some(&format!("{agent_id}.jsonl")) => {
            vec![record.to_path_buf(), parent.join(agent_id)]
        }
        (Agent::Codex, Ok(4))
            if name.is_some_and(|name| {
                rollout_stem(name).is_some_and(|stem| {
                    stem.starts_with("rollout-") && names_thread(stem, agent_id)
                })
            }) =>
        {
            vec![record.to_path_buf()]
        }
        // The index names the ledger inside the session's directory; the directory is the record.
        (Agent::Grok, Ok(3))
            if name == Some("usage.json")
                && parent.file_name().and_then(|name| name.to_str()) == Some(agent_id) =>
        {
            vec![parent.to_path_buf()]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_ID: &str = "11111111-2222-3333-4444-555555555555";
    const CODEX_ID: &str = "019a0000-0000-7000-8000-000000000001";

    fn session(agent: Agent, agent_id: &str, config_dir: Option<&Path>) -> Session {
        let mut session: Session = serde_json::from_value(serde_json::json!({
            "id":"s","agent":"claude","agent_session_id":agent_id,"console_id":"console",
            "host_id":"local","role":"project","origin":"user","title":"Session",
            "status":"archived","has_conversation":true,"pinned":false,"started_at":0}))
        .unwrap();
        session.agent = agent;
        session.config_dir = config_dir.map(|dir| dir.to_string_lossy().into_owned());
        session
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "{}\n").unwrap();
    }

    #[test]
    fn native_presence_requires_a_complete_scan_and_counts_orphan_directories() {
        use crate::protocol::SessionRecordPresence::{Absent, Present, Unknown};
        let dir = crate::test_support::ScratchDir::new("agent-records-presence");
        let account = dir.join("account");
        let shell = || {
            HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )])
        };
        let hints = Hints::default();
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let mut native = session(agent, CLAUDE_ID, Some(&account));
            assert_eq!(
                presence(&hints, &mut Locator::with_shell(shell()), &[], &native),
                Absent
            );
            native.agent_session_id = None;
            assert_eq!(
                presence(&hints, &mut Locator::with_shell(shell()), &[], &native),
                Unknown
            );
            native.agent_session_id = Some("invalid".to_owned());
            assert_eq!(
                presence(&hints, &mut Locator::with_shell(shell()), &[], &native),
                Unknown
            );
        }
        let claude = session(Agent::Claude, CLAUDE_ID, Some(&account));
        let orphan = account.join("projects/work").join(CLAUDE_ID);
        std::fs::create_dir_all(orphan.join("subagents")).unwrap();
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Present
        );
        let grok = session(Agent::Grok, CLAUDE_ID, Some(&account));
        std::fs::create_dir_all(account.join("sessions/work").join(CLAUDE_ID)).unwrap();
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &grok),
            Present
        );

        std::fs::remove_dir_all(&orphan).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), account.join("projects/linked")).unwrap();
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Unknown
        );
        touch(
            &account
                .join("projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Present
        );
        std::fs::remove_file(
            account
                .join("projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        )
        .unwrap();
        std::fs::remove_file(account.join("projects/linked")).unwrap();
        std::fs::remove_dir_all(account.join("projects")).unwrap();
        touch(&account.join("projects"));
        assert_eq!(
            presence(&hints, &mut Locator::with_shell(shell()), &[], &claude),
            Unknown
        );
    }

    /// Where each agent keeps a session's records, and what of them is named: Claude's transcript
    /// in every project directory with the subagent directory beside each, Codex's rollout, Grok's
    /// session directory; one kept in another account of the same agent is found as well.
    #[test]
    fn each_agents_layout_resolves_to_exactly_its_own_records() {
        let dir = crate::test_support::ScratchDir::new("agent-records-layout");
        let (account, other) = (dir.join("account"), dir.join("other"));
        let mut locator = Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            dir.join("home").to_string_lossy().into_owned(),
        )]));
        let watch = Hints::default();
        let other_account = Account {
            id: "a".to_owned(),
            agent: Agent::Claude,
            name: "Other".to_owned(),
            config_dir: other.to_string_lossy().into_owned(),
            approval_reviewer: None,
            approval_mode: None,
        };

        let projects = account.join("projects");
        touch(&projects.join("one").join(format!("{CLAUDE_ID}.jsonl")));
        touch(&projects.join("two").join(format!("{CLAUDE_ID}.jsonl")));
        touch(&projects.join("one").join("unrelated.jsonl"));
        touch(
            &other
                .join("projects/one")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        let found = record_paths(
            &watch,
            &mut locator,
            std::slice::from_ref(&other_account),
            &session(Agent::Claude, CLAUDE_ID, Some(&account)),
        );
        let mut expected: Vec<PathBuf> = ["one", "two"]
            .iter()
            .flat_map(|name| {
                let at = projects.join(name);
                [at.join(format!("{CLAUDE_ID}.jsonl")), at.join(CLAUDE_ID)]
            })
            .chain([
                other
                    .join("projects/one")
                    .join(format!("{CLAUDE_ID}.jsonl")),
                other.join("projects/one").join(CLAUDE_ID),
            ])
            .collect();
        let mut found_sorted = found.clone();
        found_sorted.sort();
        expected.sort();
        assert_eq!(found_sorted, expected);

        let rollout = account
            .join("sessions/2026/10/16")
            .join(format!("rollout-2026-10-16T00-00-00-{CODEX_ID}.jsonl"));
        touch(&rollout);
        touch(&account.join("sessions/2026/10/16/rollout-2026-10-16T00-00-00-other.jsonl"));
        assert_eq!(
            record_paths(
                &watch,
                &mut locator,
                &[],
                &session(Agent::Codex, CODEX_ID, Some(&account))
            ),
            [rollout]
        );

        let grok = account.join("sessions/work").join(CLAUDE_ID);
        touch(&grok.join("usage.json"));
        touch(&grok.join("messages.json"));
        assert_eq!(
            record_paths(
                &watch,
                &mut locator,
                &[],
                &session(Agent::Grok, CLAUDE_ID, Some(&account))
            ),
            [grok]
        );
    }

    /// Codex compresses a rollout in place once it is a week old: the compressed file is indexed
    /// and named for the Trash like the plain one.
    #[test]
    fn a_compressed_codex_rollout_is_a_record_too() {
        let dir = crate::test_support::ScratchDir::new("agent-records-compressed");
        let account = dir.join("account");
        let mut locator = Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            dir.join("home").to_string_lossy().into_owned(),
        )]));
        let day = account.join("sessions/2026/10/01");
        let compressed = day.join(format!("rollout-2026-10-01T00-00-00-{CODEX_ID}.jsonl.zst"));
        touch(&compressed);
        touch(&day.join("rollout-2026-10-01T00-00-00-other.jsonl.zst"));

        assert_eq!(
            locator.current_record(Agent::Codex, &account, CODEX_ID),
            Some(compressed.clone())
        );
        assert_eq!(
            record_paths(
                &Hints::default(),
                &mut locator,
                &[],
                &session(Agent::Codex, CODEX_ID, Some(&account))
            ),
            [compressed]
        );
    }

    /// A Codex thread that had a turn reverted is a first rollout and later segments, plain or
    /// compressed. They are one record: all are named for the Trash, the head (the chain's leaf,
    /// not the file written last) is the one read, and a subagent's rollout or another thread's
    /// is no part of it.
    #[test]
    fn a_reverted_codex_thread_is_one_record_whose_head_is_the_leaf() {
        use crate::protocol::SessionRecordPresence::Present;
        for compressed in [false, true] {
            let dir = crate::test_support::ScratchDir::new("agent-records-chain");
            let account = dir.join("account");
            let chain = crate::test_support::codex_chain(&account, CODEX_ID, compressed);
            let day = account.join("sessions/2026/10/01");
            let child = "019a0000-0000-7000-8000-00000000000a";
            touch(&day.join(format!("rollout-2026-10-01T01-00-00-{child}.jsonl")));
            touch(&day.join("rollout-2026-10-01T01-00-00-other.jsonl"));
            let mut locator = Locator::with_shell(HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )]));
            let hints = Hints::default();
            let codex = session(Agent::Codex, CODEX_ID, Some(&account));

            let mut named = record_paths(&hints, &mut locator, &[], &codex);
            named.sort();
            assert_eq!(named, chain.all(), "compressed: {compressed}");
            assert_eq!(
                locator.current_record(Agent::Codex, &account, CODEX_ID),
                Some(chain.head.clone()),
                "compressed: {compressed}"
            );
            assert_eq!(presence(&hints, &mut locator, &[], &codex), Present);

            // The conversation is the first rollout and the middle segment up to their cuts, then
            // the head.
            let cuts = |path: &Path| {
                let text = std::fs::read(path).unwrap();
                let text = if compressed {
                    zstd::decode_all(text.as_slice()).unwrap()
                } else {
                    text
                };
                let mut lines = text.split_inclusive(|byte| *byte == b'\n');
                (lines.next().unwrap().len() + lines.next().unwrap().len()) as u64
            };
            let stretch = |path: &PathBuf, cut| Stretch {
                path: path.clone(),
                cut,
            };
            let branch = conversation_branch(&hints, &mut locator, &[], &codex).unwrap();
            assert_eq!(branch.record, chain.head, "compressed: {compressed}");
            assert_eq!(
                branch.stretches,
                Some(vec![
                    stretch(&chain.first, Some(cuts(&chain.first))),
                    stretch(&chain.middle, Some(cuts(&chain.middle))),
                    stretch(&chain.head, None),
                ]),
                "compressed: {compressed}"
            );

            // Only the head is on disk: it is still the one read, but the branch is incomplete.
            std::fs::remove_file(&chain.first).unwrap();
            std::fs::remove_file(&chain.middle).unwrap();
            let mut locator = Locator::with_shell(HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )]));
            let branch = conversation_branch(&hints, &mut locator, &[], &codex).unwrap();
            assert_eq!((branch.record, branch.stretches), (chain.head, None));
        }
    }

    /// Of two files nothing continues (a branch split off the same segment), the one started last
    /// by name is the head, not the one written last.
    #[test]
    fn of_two_leaves_the_one_started_last_is_the_head() {
        let dir = crate::test_support::ScratchDir::new("agent-records-fork");
        let chain = crate::test_support::codex_chain(&dir, CODEX_ID, false);
        let fork = chain.middle.with_file_name(format!(
            "rollout-2026-10-02T12-00-00-{CODEX_ID}_019a0000-0000-7000-8000-0000000000a3.jsonl"
        ));
        let (meta, _) = std::fs::read_to_string(&chain.head)
            .unwrap()
            .split_once('\n')
            .map(|(meta, rest)| (meta.to_owned(), rest.to_owned()))
            .unwrap();
        std::fs::write(&fork, meta + "\n").unwrap();
        assert!(
            fork.metadata().unwrap().modified().unwrap()
                > chain.head.metadata().unwrap().modified().unwrap()
        );

        assert_eq!(
            rollout_head(&[chain.first, chain.middle, fork, chain.head.clone()]),
            Some(chain.head)
        );
    }

    /// Writes a rollout of thread `CODEX_ID` whose first line says it continues the file `base`
    /// (or starts a fresh history for `None`).
    fn segment(dir: &Path, day: &str, id: &str, base: Option<&str>) -> PathBuf {
        let path = dir.join(format!(
            "rollout-2026-10-{day}T00-00-00-{CODEX_ID}_{id}.jsonl"
        ));
        let base = base.map_or(serde_json::Value::Null, |base| {
            serde_json::json!({"thread_id": base, "end_ordinal_exclusive": 1, "end_byte_offset": 1})
        });
        let meta = serde_json::json!({"type":"session_meta","payload":{"id":CODEX_ID,
            "history_mode":"paginated","history_base":base}});
        touch(&path);
        std::fs::write(&path, meta.to_string() + "\n").unwrap();
        path
    }

    /// Files that name each other (or themselves) as the one they continue form no chain: there is
    /// no conversation to put together, yet the thread still has a file to read from.
    #[test]
    fn a_chain_that_loops_has_no_branch_but_still_a_head() {
        let dir = crate::test_support::ScratchDir::new("agent-records-loop");
        let (a, b) = (
            "019a0000-0000-7000-8000-0000000000a1",
            "019a0000-0000-7000-8000-0000000000a2",
        );
        let looped = [
            segment(&dir, "01", a, Some(b)),
            segment(&dir, "02", b, Some(a)),
        ];
        let itself = segment(
            &dir,
            "03",
            "019a0000-0000-7000-8000-0000000000a3",
            Some("019a0000-0000-7000-8000-0000000000a3"),
        );

        assert_eq!(rollout_branch(&looped[0], &looped), None);
        assert_eq!(rollout_head(&looped), Some(looped[1].clone()));
        assert_eq!(rollout_branch(&itself, std::slice::from_ref(&itself)), None);
        assert_eq!(rollout_head(std::slice::from_ref(&itself)), Some(itself));
    }

    /// Which file is the head follows who continues whom, not the order of the names: the file
    /// that is continued is not the head however late it is named.
    #[test]
    fn the_head_is_the_file_nothing_continues_whatever_the_order_of_the_names() {
        let dir = crate::test_support::ScratchDir::new("agent-records-lineage");
        let (late, early) = (
            "019a0000-0000-7000-8000-0000000000a1",
            "019a0000-0000-7000-8000-0000000000a2",
        );
        let continued = segment(&dir, "05", late, None);
        let continuing = segment(&dir, "02", early, Some(late));

        assert_eq!(
            rollout_head(&[continued, continuing.clone()]),
            Some(continuing)
        );
    }

    /// A segment that started a fresh history continues nothing: it is the head, and the
    /// conversation is that file alone, not the chain before it.
    #[test]
    fn a_fresh_history_segment_is_a_head_with_a_branch_of_its_own() {
        let dir = crate::test_support::ScratchDir::new("agent-records-fresh");
        let chain = crate::test_support::codex_chain(&dir, CODEX_ID, false);
        let fresh = segment(
            &chain.head.parent().unwrap().with_file_name("04"),
            "04",
            "019a0000-0000-7000-8000-0000000000a3",
            None,
        );
        let mut files = chain.all().to_vec();
        files.push(fresh.clone());

        assert_eq!(rollout_head(&files), Some(fresh.clone()));
        assert_eq!(
            rollout_branch(&fresh, &files),
            Some(vec![Stretch {
                path: fresh,
                cut: None
            }])
        );
    }

    /// A session id or a hinted path from an unauthenticated hook payload never names anything
    /// outside the session's own records.
    #[test]
    fn a_hostile_id_or_hint_names_nothing() {
        let dir = crate::test_support::ScratchDir::new("agent-records-hostile");
        let account = dir.join("account");
        let mut locator = Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            dir.join("home").to_string_lossy().into_owned(),
        )]));
        let projects = account.join("projects/work");
        touch(&projects.join("x.jsonl"));
        let watch = Hints::default();
        for id in ["..", ".", "", "a/b", "../x"] {
            assert!(
                record_paths(
                    &watch,
                    &mut locator,
                    &[],
                    &session(Agent::Claude, id, Some(&account))
                )
                .is_empty(),
                "{id:?}"
            );
        }
        let victim = dir.join("elsewhere");
        touch(&victim.join(format!("{CLAUDE_ID}.jsonl")));
        for hint in [
            victim.join(format!("{CLAUDE_ID}.jsonl")),
            account
                .join("projects/../../elsewhere")
                .join(format!("{CLAUDE_ID}.jsonl")),
            PathBuf::from(format!("projects/work/{CLAUDE_ID}.jsonl")),
        ] {
            watch
                .hints
                .lock()
                .unwrap()
                .insert("s".to_owned(), (CLAUDE_ID.to_owned(), hint.clone()));
            assert!(
                record_paths(
                    &watch,
                    &mut locator,
                    &[],
                    &session(Agent::Claude, CLAUDE_ID, Some(&account))
                )
                .is_empty(),
                "{hint:?}"
            );
        }
        // A name equal to a directory's, one level too high or too deep, and an id that is no UUID.
        std::fs::create_dir_all(account.join("projects").join(CLAUDE_ID)).unwrap();
        std::fs::create_dir_all(projects.join("other-session")).unwrap();
        for hint in [
            account.join("projects").join(format!("{CLAUDE_ID}.jsonl")),
            projects
                .join("other-session")
                .join(format!("{CLAUDE_ID}.jsonl")),
        ] {
            watch
                .hints
                .lock()
                .unwrap()
                .insert("s".to_owned(), (CLAUDE_ID.to_owned(), hint.clone()));
            assert!(
                record_paths(
                    &watch,
                    &mut locator,
                    &[],
                    &session(Agent::Claude, CLAUDE_ID, Some(&account))
                )
                .is_empty(),
                "{hint:?}"
            );
        }
        assert!(record_paths(
            &watch,
            &mut locator,
            &[],
            &session(Agent::Claude, "subagents", Some(&account))
        )
        .is_empty());
        // A project directory that is a link is not followed.
        let linked = account.join("projects").join("linked");
        std::os::unix::fs::symlink(&victim, &linked).unwrap();
        watch.hints.lock().unwrap().insert(
            "s".to_owned(),
            (
                CLAUDE_ID.to_owned(),
                linked.join(format!("{CLAUDE_ID}.jsonl")),
            ),
        );
        assert!(record_paths(
            &watch,
            &mut locator,
            &[],
            &session(Agent::Claude, CLAUDE_ID, Some(&account))
        )
        .is_empty());
        // A hint inside the account's records is taken, though no listing holds it.
        let inside = projects.join(format!("{CLAUDE_ID}.jsonl"));
        watch
            .hints
            .lock()
            .unwrap()
            .insert("s".to_owned(), (CLAUDE_ID.to_owned(), inside.clone()));
        assert_eq!(
            record_paths(
                &watch,
                &mut locator,
                &[],
                &session(Agent::Claude, CLAUDE_ID, Some(&account))
            ),
            [inside, projects.join(CLAUDE_ID)]
        );
    }

    /// The file a conversation is read from: the transcript or rollout itself, and for Grok the
    /// chat history inside its session directory; the newest copy when a switched session left one
    /// behind, and nothing for a session that has only a directory with no conversation in it.
    #[test]
    fn the_conversation_record_is_the_newest_file_holding_the_conversation() {
        let dir = crate::test_support::ScratchDir::new("agent-records-conversation");
        let account = dir.join("account");
        let hints = Hints::default();
        let shell = || {
            HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )])
        };
        let find = |agent, id| {
            conversation_branch(
                &hints,
                &mut Locator::with_shell(shell()),
                &[],
                &session(agent, id, Some(&account)),
            )
            .map(|branch| branch.record)
        };

        assert_eq!(find(Agent::Claude, CLAUDE_ID), None);
        let orphan = account.join("projects/one").join(CLAUDE_ID);
        std::fs::create_dir_all(orphan.join("subagents")).unwrap();
        assert_eq!(find(Agent::Claude, CLAUDE_ID), None);
        let old = account
            .join("projects/one")
            .join(format!("{CLAUDE_ID}.jsonl"));
        touch(&old);
        crate::test_support::move_modification_time_back(&old);
        let new = account
            .join("projects/two")
            .join(format!("{CLAUDE_ID}.jsonl"));
        touch(&new);
        assert_eq!(find(Agent::Claude, CLAUDE_ID), Some(new));

        let grok = account.join("sessions/work").join(CLAUDE_ID);
        touch(&grok.join("usage.json"));
        assert_eq!(find(Agent::Grok, CLAUDE_ID), None);
        touch(&grok.join("chat_history.jsonl"));
        assert_eq!(
            find(Agent::Grok, CLAUDE_ID),
            Some(grok.join("chat_history.jsonl"))
        );
    }

    fn judged(session: &Session) -> Resumability {
        resumability(&Locator::default(), session)
    }

    fn needs_unarchive(session: &Session) -> bool {
        judge(&Locator::default(), session).needs_unarchive
    }

    #[test]
    fn claude_is_resumable_only_with_its_main_transcript_in_its_own_account() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-claude");
        let (account, other) = (dir.join("account"), dir.join("other"));
        let own = session(Agent::Claude, CLAUDE_ID, Some(&account));
        std::fs::create_dir_all(account.join("projects")).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);

        // A subagent folder left without its main transcript is not a record.
        std::fs::create_dir_all(account.join("projects/work").join(CLAUDE_ID)).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);

        // Another account's copy, left by an earlier switch, does not count.
        touch(
            &other
                .join("projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(judged(&own), Resumability::Unresumable);

        // Any project folder of the account will do.
        touch(
            &account
                .join("projects/elsewhere")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
    }

    #[test]
    fn a_session_without_a_conversation_is_resumable_and_an_unusable_id_is_unknown() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-fresh");
        let account = dir.join("account");
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let mut fresh = session(agent, CLAUDE_ID, Some(&account));
            fresh.has_conversation = false;
            assert_eq!(judged(&fresh), Resumability::Resumable);
            let mut no_id = session(agent, CLAUDE_ID, Some(&account));
            no_id.agent_session_id = None;
            assert_eq!(judged(&no_id), Resumability::Unknown);
            no_id.agent_session_id = Some("../x".to_owned());
            assert_eq!(judged(&no_id), Resumability::Unknown);
        }
    }

    #[test]
    fn an_account_directory_that_cannot_be_read_is_unknown_for_every_agent() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-gone");
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let gone = session(agent, CLAUDE_ID, Some(&dir.join("gone")));
            assert_eq!(judged(&gone), Resumability::Unknown);
        }
        // Present but not a directory, so it cannot be read either.
        touch(&dir.join("file"));
        let file = session(Agent::Claude, CLAUDE_ID, Some(&dir.join("file")));
        assert_eq!(judged(&file), Resumability::Unknown);
    }

    #[test]
    fn an_account_directory_without_any_records_directory_is_unknown_for_every_agent() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-empty");
        std::fs::create_dir_all(dir.join("account")).unwrap();
        for agent in [Agent::Claude, Agent::Codex, Agent::Grok] {
            let empty = session(agent, CLAUDE_ID, Some(&dir.join("account")));
            assert_eq!(judged(&empty), Resumability::Unknown);
        }
    }

    #[test]
    fn the_default_account_is_judged_through_the_shell_and_unknown_when_it_cannot_be_resolved() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-default");
        let default = session(Agent::Claude, CLAUDE_ID, None);
        let shell = HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_owned(),
            dir.join("default").to_string_lossy().into_owned(),
        )]);
        let locator = Locator::with_shell(shell);
        std::fs::create_dir_all(dir.join("default/projects")).unwrap();
        assert_eq!(resumability(&locator, &default), Resumability::Unresumable);
        touch(
            &dir.join("default/projects/work")
                .join(format!("{CLAUDE_ID}.jsonl")),
        );
        assert_eq!(resumability(&locator, &default), Resumability::Resumable);

        // A location that is not absolute is not a place to look.
        let relative = Locator::with_shell(HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_owned(),
            "relative".to_owned(),
        )]));
        assert_eq!(resumability(&relative, &default), Resumability::Unknown);
    }

    #[test]
    fn codex_counts_compressed_and_archived_rollouts_and_archived_ones_need_unarchiving() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-codex");
        let account = dir.join("account");
        let own = session(Agent::Codex, CODEX_ID, Some(&account));
        std::fs::create_dir_all(account.join("sessions")).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);
        assert!(!needs_unarchive(&own));

        // Another thread's rollout is not this one's.
        touch(&account.join("sessions/2026/10/01/rollout-2026-10-01T00-00-00-019a0000-0000-7000-8000-000000000002.jsonl"));
        assert_eq!(judged(&own), Resumability::Unresumable);

        let name = format!("rollout-2026-10-01T00-00-00-{CODEX_ID}.jsonl");
        touch(
            &account
                .join("archived_sessions")
                .join(format!("{name}.zst")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(needs_unarchive(&own));

        touch(
            &account
                .join("sessions/2026/10/01")
                .join(format!("{name}.zst")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(!needs_unarchive(&own));

        let plain = dir.join("plain");
        touch(&plain.join("sessions/2026/10/01").join(&name));
        assert_eq!(
            judged(&session(Agent::Codex, CODEX_ID, Some(&plain))),
            Resumability::Resumable
        );
    }

    /// A reverted Codex thread is one record: while any file of its chain is there the thread is
    /// not proven gone, whichever of the first rollout and the segments survive, and a chain kept
    /// only in `archived_sessions/` needs unarchiving.
    #[test]
    fn a_reverted_codex_thread_is_not_gone_while_any_of_its_chain_is_there() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-chain");
        let account = dir.join("account");
        let own = session(Agent::Codex, CODEX_ID, Some(&account));
        let chain = crate::test_support::codex_chain(&account, CODEX_ID, false);
        assert_eq!(judged(&own), Resumability::Resumable);

        // Only the segments, then only the first rollout.
        std::fs::remove_file(&chain.first).unwrap();
        assert_eq!(judged(&own), Resumability::Resumable);
        std::fs::remove_file(&chain.middle).unwrap();
        assert_eq!(judged(&own), Resumability::Resumable);
        touch(&chain.first);
        std::fs::remove_file(&chain.head).unwrap();
        assert_eq!(judged(&own), Resumability::Resumable);

        // A segment on its own in the archive.
        std::fs::remove_file(&chain.first).unwrap();
        assert_eq!(judged(&own), Resumability::Unresumable);
        let archived = account
            .join("archived_sessions")
            .join(chain.middle.file_name().unwrap());
        touch(&archived);
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(needs_unarchive(&own));
    }

    #[test]
    fn an_archived_codex_rollout_is_found_although_sessions_cannot_be_searched() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-codex-uncertain");
        let account = dir.join("account");
        let own = session(Agent::Codex, CODEX_ID, Some(&account));
        std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("elsewhere"), account.join("sessions")).unwrap();
        assert_eq!(judged(&own), Resumability::Unknown);
        assert!(!needs_unarchive(&own));

        touch(
            &account
                .join("archived_sessions")
                .join(format!("rollout-2026-10-01T00-00-00-{CODEX_ID}.jsonl")),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
        assert!(needs_unarchive(&own));
    }

    /// The history of a Codex thread that `codex archive` moved out of `sessions/` is read from
    /// `archived_sessions/`, and a record in `sessions/` still wins over it.
    #[test]
    fn history_finds_a_codex_rollout_that_was_archived() {
        let dir = crate::test_support::ScratchDir::new("agent-records-history-archived");
        let account = dir.join("account");
        let own = session(Agent::Codex, CODEX_ID, Some(&account));
        let found = |own: &Session| {
            let mut locator = Locator::with_shell(HashMap::from([(
                "HOME".to_owned(),
                dir.join("home").to_string_lossy().into_owned(),
            )]));
            history_branch(&Hints::default(), &mut locator, &[], own).map(|branch| branch.record)
        };
        assert_eq!(found(&own), None);

        let name = format!("rollout-2026-10-01T00-00-00-{CODEX_ID}.jsonl");
        let archived = account.join("archived_sessions").join(&name);
        touch(&archived);
        assert_eq!(found(&own), Some(archived));

        let live = account.join("sessions/2026/10/01").join(&name);
        touch(&live);
        assert_eq!(found(&own), Some(live));

        // Another agent's session is not looked for there.
        let other = session(Agent::Claude, CLAUDE_ID, Some(&account));
        assert_eq!(found(&other), None);
    }

    #[test]
    fn record_names_match_the_id_whatever_its_case() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-case");
        let (claude, codex) = (dir.join("claude"), dir.join("codex"));
        touch(
            &claude
                .join("projects/work")
                .join(format!("{}.jsonl", CLAUDE_ID.to_uppercase())),
        );
        assert_eq!(
            judged(&session(Agent::Claude, CLAUDE_ID, Some(&claude))),
            Resumability::Resumable
        );
        touch(&codex.join("sessions/2026/10/01").join(format!(
            "rollout-2026-10-01T00-00-00-{}.jsonl",
            CODEX_ID.to_uppercase()
        )));
        assert_eq!(
            judged(&session(Agent::Codex, CODEX_ID, Some(&codex))),
            Resumability::Resumable
        );
        // And an id recorded in capitals finds a lower-case file.
        let upper = session(Agent::Codex, &CODEX_ID.to_uppercase(), Some(&codex));
        assert_eq!(judged(&upper), Resumability::Resumable);
    }

    #[test]
    fn a_codex_rollout_in_a_deep_date_tree_is_found() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-codex-deep");
        let account = dir.join("account");
        touch(
            &account
                .join("sessions/a/b/c/d/e")
                .join(format!("rollout-x-{CODEX_ID}.jsonl")),
        );
        assert_eq!(
            judged(&session(Agent::Codex, CODEX_ID, Some(&account))),
            Resumability::Resumable
        );
    }

    #[test]
    fn grok_is_resumable_with_its_folder_and_only_unknown_without() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-grok");
        let account = dir.join("account");
        let own = session(Agent::Grok, CLAUDE_ID, Some(&account));
        std::fs::create_dir_all(account.join("sessions")).unwrap();
        assert_eq!(judged(&own), Resumability::Unknown);
        touch(
            &account
                .join("sessions/work")
                .join(CLAUDE_ID)
                .join("usage.json"),
        );
        assert_eq!(judged(&own), Resumability::Resumable);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_records_directory_is_unknown_not_unresumable() {
        let dir = crate::test_support::ScratchDir::new("agent-records-resume-link");
        let account = dir.join("account");
        std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), account.join("projects")).unwrap();
        assert_eq!(
            judged(&session(Agent::Claude, CLAUDE_ID, Some(&account))),
            Resumability::Unknown
        );
        // A link inside the records directory could hide the record as well.
        let inner = dir.join("inner");
        std::fs::create_dir_all(inner.join("projects")).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), inner.join("projects/work")).unwrap();
        assert_eq!(
            judged(&session(Agent::Claude, CLAUDE_ID, Some(&inner))),
            Resumability::Unknown
        );
    }
}
