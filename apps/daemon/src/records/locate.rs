//! Finding an agent's records on disk: the record path each session's hook last reported (a hint),
//! and the [`Locator`], which resolves accounts' directories and lists the records in them by agent
//! session id. A listing never follows a link, and says whether it could see everything. It also
//! finds every record of a session by the one rule ([`record_paths`]), from which the conversation
//! is read.

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use std::time::SystemTime;

use crate::adapter::provider;
use crate::hostfs::{lexical_key, lexically_normalise, lexically_within};
use crate::protocol::{Account, AgentId, Session};
use crate::providers::layout::layout;
use crate::records::rows::RowAccount;
use crate::records::RecordStore;
use crate::state::AppState;

/// The record path each session's hook last reported, with the agent session id it reported it
/// for; and, for a session whose records are rows (`crate::records::rows`), how many of its
/// events have come in.
#[derive(Default)]
pub struct Hints {
    pub(super) hints: Mutex<HashMap<String, (String, PathBuf)>>,
    pub(super) stirs: Mutex<HashMap<String, u64>>,
}

impl Hints {
    /// Forgets the sessions not in `ids` and returns what is left.
    pub fn retain(&self, ids: &HashSet<&str>) -> HashMap<String, (String, PathBuf)> {
        self.stirs
            .lock()
            .expect("record stir lock poisoned")
            .retain(|id, _| ids.contains(id.as_str()));
        let mut hints = self.hints.lock().expect("record hint lock poisoned");
        hints.retain(|id, _| ids.contains(id.as_str()));
        hints.clone()
    }

    pub(crate) fn hint(&self, session: &Session) -> Option<PathBuf> {
        let hints = self.hints.lock().expect("record hint lock poisoned");
        hinted_path(&hints, session)
    }

    /// Notes that an event of session `id` came in: its conversation may have changed.
    pub(crate) fn stir(&self, id: &str) {
        let mut stirs = self.stirs.lock().expect("record stir lock poisoned");
        *stirs.entry(id.to_owned()).or_default() += 1;
    }

    /// How many events of session `id` have come in. A reader of a conversation kept as rows
    /// reads it again when this has moved since its last read.
    pub(crate) fn stirred(&self, id: &str) -> u64 {
        let stirs = self.stirs.lock().expect("record stir lock poisoned");
        stirs.get(id).copied().unwrap_or(0)
    }
}

/// The record path a hook reported for `session`, when it was reported for the session's current
/// agent session id.
pub fn hinted_path(
    hints: &HashMap<String, (String, PathBuf)>,
    session: &Session,
) -> Option<PathBuf> {
    let (id, path) = hints.get(&session.id)?;
    (Some(id) == session.agent_session_id.as_ref()).then(|| path.clone())
}

/// Notes the record path a hook of session `id` reports, read by its provider's `store`
/// (`RecordStore::hinted_record`): the exact record, including a newly created one. Subagent hooks
/// are filtered by the caller before this point. For a store whose records are rows, which no
/// path names, the event itself is what is noted: the conversation may have changed
/// (`Hints::stir`).
pub fn on_hook(state: &AppState, id: &str, store: &dyn RecordStore, payload: &Value) {
    if store.rows().is_some() {
        state.record_hints.stir(id);
    }
    let Some(hint) = store.hinted_record(payload) else {
        return;
    };
    state
        .record_hints
        .hints
        .lock()
        .expect("record hint lock poisoned")
        .insert(id.to_owned(), hint);
}

/// Every record of one agent in one account, by agent session id, listed once and kept for as long
/// as the `Locator` lives.
#[derive(Default)]
pub(crate) struct Index {
    pub(crate) records: HashMap<String, Vec<PathBuf>>,
    pub(crate) complete: bool,
}

/// Resolves accounts' directories and lists their records. The login-shell snapshot that says where
/// a default account lives is taken only when something asks for one, and an unknown location is
/// unknown, never a fallback to a different account's history.
#[derive(Default)]
pub struct Locator<'a> {
    store: Option<&'a crate::store::Store>,
    shell: OnceCell<Option<HashMap<String, String>>>,
    indexes: HashMap<(AgentId, PathBuf), Index>,
    record_dirs: HashMap<(AgentId, PathBuf, usize), Index>,
    /// For a provider whose records are rows: the conversations each account holds
    /// (`RowRecords::conversations`), by agent and the account's pinned directory, listed once.
    row_listings: RefCell<RowListings>,
}

/// What each account of a provider whose records are rows was listed to hold, by agent and the
/// account's pinned directory: `None` for an account that could not be listed.
type RowListings = HashMap<(AgentId, Option<String>), Option<HashSet<String>>>;

impl<'a> Locator<'a> {
    /// A locator that resolves default accounts through `shell`, a login-shell snapshot already
    /// taken, instead of asking for one.
    pub fn with_shell(shell: HashMap<String, String>) -> Self {
        Self {
            shell: OnceCell::from(Some(shell)),
            ..Self::default()
        }
    }

    pub(crate) fn with_store<'store>(self, store: &'store crate::store::Store) -> Locator<'store> {
        Locator {
            store: Some(store),
            shell: self.shell,
            indexes: self.indexes,
            record_dirs: self.record_dirs,
            row_listings: self.row_listings,
        }
    }

    /// The login-shell snapshot default accounts are resolved through, taken when first asked
    /// for; `None` when it could not be taken.
    pub(crate) fn shell_env(&self) -> Option<&HashMap<String, String>> {
        self.shell
            .get_or_init(|| crate::env_shell::cached_snapshot().ok())
            .as_ref()
    }

    /// Whether `session`'s own account holds its conversation, for a provider whose records are
    /// rows: `None` when that cannot be told (the provider's records are not rows, the session
    /// names no conversation of its agent's shape, the account's environment is unknown, or the
    /// account could not be listed). The account is listed once for as long as the `Locator`
    /// lives.
    pub(crate) fn holds_conversation(&self, session: &Session) -> Option<bool> {
        let (rows, agent_id) = crate::records::rows::of(session)?;
        let shell_env = self.shell_env()?;
        let mut listings = self.row_listings.borrow_mut();
        let listing = listings
            .entry((session.agent, session.config_dir.clone()))
            .or_insert_with(|| {
                rows.conversations(&RowAccount {
                    config_dir: session.config_dir.as_deref().map(Path::new),
                    shell_env,
                })
            });
        Some(listing.as_ref()?.contains(agent_id))
    }

    /// The account directory `config_dir` names, or the agent's default account when it is `None`.
    pub fn root(&self, agent: AgentId, config_dir: Option<&str>) -> Option<PathBuf> {
        match config_dir {
            Some(dir) => Some(PathBuf::from(dir)),
            None => Some(crate::adapter::default_account_dir(
                agent,
                self.shell_env()?,
            )),
        }
    }

    pub(crate) fn index(&mut self, agent: AgentId, root: &Path) -> &Index {
        self.indexes
            .entry((agent, root.to_path_buf()))
            .or_insert_with(|| index_records(agent, root))
    }

    /// The records listed directly under the records directory `records`, a directory of its own
    /// rather than reached through an account's root.
    pub(crate) fn index_at(&mut self, agent: AgentId, records: &Path) -> &Index {
        self.index_at_depth(agent, records, layout(agent).search_depth)
    }

    pub(super) fn index_at_depth(
        &mut self,
        agent: AgentId,
        records: &Path,
        depth: usize,
    ) -> &Index {
        self.record_dirs
            .entry((agent, records.to_path_buf(), depth))
            .or_insert_with(|| index_records_at(agent, records, depth))
    }

    /// A configured records directory, or no override. An unresolved environment or working
    /// directory leaves a missing record inconclusive.
    pub(super) fn alternate_root(&self, session: &Session) -> Result<Option<(PathBuf, usize)>, ()> {
        let Some(root) = provider(session.agent).records().record_root_override() else {
            return Ok(None);
        };
        let env = self.shell_env().ok_or(())?;
        let Some(path) = env.get(root.variable).filter(|path| !path.is_empty()) else {
            return Ok(None);
        };
        let home_relative = crate::platform::names::home_relative(path);
        let path = home_relative.map_or_else(
            || PathBuf::from(path),
            |relative| crate::adapter::snapshot_home(env).join(relative),
        );
        let path = if path.is_absolute() {
            path
        } else {
            let store = self.store.ok_or(())?;
            let cwd = store.session_working_directory(session).map_err(|_| ())?;
            cwd.join(path)
        };
        let path = lexically_normalise(&path);
        if !path.is_absolute() {
            return Err(());
        }
        Ok(Some((path, root.search_depth)))
    }

    pub(super) fn alternate_root_complete(&mut self, session: &Session) -> bool {
        match self.alternate_root(session) {
            Ok(Some((dir, depth))) => self.index_at_depth(session.agent, &dir, depth).complete,
            Ok(None) => true,
            Err(()) => false,
        }
    }

    pub(super) fn current_session_record(&mut self, session: &Session) -> Option<PathBuf> {
        let id = session.agent_session_id.as_deref()?;
        let agent = session.agent;
        if let Ok(Some((dir, depth))) = self.alternate_root(session) {
            if let Some(path) = self
                .index_at_depth(agent, &dir, depth)
                .records
                .get(id)
                .and_then(|paths| provider(agent).records().current_of(paths))
            {
                return Some(path);
            }
        }
        let root = self.root(agent, session.config_dir.as_deref())?;
        self.current_record(agent, &root, id)
    }

    /// The record of `agent_id` in the account at `root` that holds the conversation as it is now
    /// (`RecordStore::current_of`).
    pub fn current_record(
        &mut self,
        agent: AgentId,
        root: &Path,
        agent_id: &str,
    ) -> Option<PathBuf> {
        let paths = self.index(agent, root).records.get(agent_id)?;
        provider(agent).records().current_of(paths)
    }
}

/// The directory of an account where an agent keeps its session records; `None` for an agent that
/// keeps none (`AgentLayout::records`).
pub(crate) fn records_dir(agent: AgentId, root: &Path) -> Option<PathBuf> {
    layout(agent).records().map(|relative| root.join(relative))
}

/// What a [`walk_records`] visitor decides about one entry.
pub(crate) enum Step {
    /// The entry is a record (or otherwise settled): do not descend into it.
    Matched,
    /// Descend into it when it is a directory with levels left.
    Descend,
    /// Stop the whole walk.
    Stop,
}

/// How a [`walk_records`] pass ended.
pub(crate) struct Walked {
    /// Every directory was listed and no link was met, so what was not found is not there.
    pub(crate) complete: bool,
    pub(crate) stopped: bool,
}

/// Visits the entries of `dir`, at most `depth` levels down (`depth` 1 is `dir`'s own entries),
/// without following a link. `visit` gets each entry's path, name, type and the levels left
/// counting its own. A missing `dir` is empty; a link, an unreadable entry or a name that is not
/// UTF-8 makes the pass incomplete and is not visited.
pub(crate) fn walk_records(
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
fn index_records(agent: AgentId, root: &Path) -> Index {
    let Some(records) = records_dir(agent, root) else {
        return Index {
            complete: true,
            ..Index::default()
        };
    };
    let mut out = index_records_at(agent, &records, layout(agent).search_depth);
    out.complete &=
        !std::fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_symlink());
    out
}

/// [`index_records`] for the records directory `records` itself: each entry the provider's store
/// recognises as a record (`RecordStore::indexed`), under the agent session id it names.
pub(super) fn index_records_at(agent: AgentId, records: &Path, depth: usize) -> Index {
    let mut out = Index::default();
    let store = provider(agent).records();
    let mut visit = |path: &Path, name: &str, kind: std::fs::FileType, left: usize| {
        let Some((id, path)) = store.indexed(path, name, kind, left == 1) else {
            return Step::Descend;
        };
        let records = out.records.entry(id).or_default();
        if !records.contains(&path) {
            records.push(path);
        }
        Step::Matched
    };
    let walked = walk_records(records, depth, &mut visit);
    out.complete = walked.complete;
    out
}

pub(super) fn account_roots(
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
        } else if !roots
            .iter()
            .any(|known| lexical_key(known) == lexical_key(&root))
        {
            roots.push(root);
        }
    }
    (roots, complete)
}

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
        let Some(dir) = records_dir(agent, root) else {
            continue;
        };
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
    let alternate = locator.alternate_root(session).ok().flatten();
    if let Some((dir, depth)) = &alternate {
        for record in locator
            .index_at_depth(agent, dir, *depth)
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
            .filter_map(|root| records_dir(agent, root))
            .chain(alternate.into_iter().filter_map(|(dir, depth)| {
                (hint.strip_prefix(&dir).ok()?.components().count() == depth).then_some(dir)
            }))
            .find(|dir| lexically_within(&hint, dir));
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

/// Whether every directory from `record`'s parent up to `dir`, inclusive, is a real directory and
/// not a link to one, so that what lies under it is where it appears to be. A records directory
/// that may itself be a link (a sharing account's always is, and a user's own may lead to a
/// dotfiles copy) is resolved first and the results spelled back through the account's path.
pub(super) fn in_real_directories(record: &Path, dir: &Path) -> bool {
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
/// sits exactly where the agent puts it, under `dir` (the account's records directory), as the
/// provider's store answers it (`RecordStore::own_paths`): the depth counted here is the only one
/// it takes, so a name that merely equals a directory's, or a file inside a session's own
/// directory, never names that directory.
pub(crate) fn own_paths(agent: AgentId, agent_id: &str, record: &Path, dir: &Path) -> Vec<PathBuf> {
    if record.parent().is_none() {
        return Vec::new();
    }
    let Ok(levels_below_dir) = record
        .strip_prefix(dir)
        .map(|rest| rest.components().count())
    else {
        return Vec::new();
    };
    provider(agent)
        .records()
        .own_paths(agent_id, record, levels_below_dir)
}

/// The file that holds `session`'s conversation, for reading, and what it is read from.
pub struct Branch {
    /// The file holding the conversation as it stands (`RecordStore::branch_among`).
    pub record: PathBuf,
    /// The files the conversation is read from, in order: the record itself, or the whole current
    /// branch of a conversation the agent keeps across several files. `None` when such a chain is
    /// missing a file.
    pub stretches: Option<Vec<Stretch>>,
}

/// A file of a conversation and how much of it counts: the bytes up to the point a later file
/// picks it up, or all of it for the file holding the conversation as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stretch {
    pub path: PathBuf,
    /// How many bytes (decompressed) of the file count; `None` for all of it.
    pub cut: Option<u64>,
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
    let store = provider(session.agent).records();
    let files: Vec<PathBuf> = record_paths(hints, locator, accounts, session)
        .into_iter()
        .map(|path| store.conversation_file(path))
        .filter(|path| std::fs::metadata(path).is_ok_and(|meta| meta.is_file()))
        .collect();
    store.branch_among(files)
}

/// [`conversation_branch`], and failing that the conversation where the agent archives records
/// (`RecordStore::archived_branch`), such as a Codex thread `codex archive` moved out of
/// `sessions/`. That one a resume refuses, but the conversation in it can still be read.
pub fn history_branch(
    hints: &Hints,
    locator: &mut Locator,
    accounts: &[Account],
    session: &Session,
) -> Option<Branch> {
    conversation_branch(hints, locator, accounts, session)
        .or_else(|| archived_branch(locator, accounts, session))
}

fn archived_branch(locator: &Locator, accounts: &[Account], session: &Session) -> Option<Branch> {
    let agent_id = session
        .agent_session_id
        .as_deref()
        .filter(|id| plausible_id(id))?;
    let (roots, _) = account_roots(locator, accounts, session);
    provider(session.agent)
        .records()
        .archived_branch(&roots, agent_id)
}

/// The newest of `paths` by modification time.
pub(crate) fn newest(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut newest: Option<&PathBuf> = None;
    for path in paths {
        if newest.is_none_or(|old| modified(path) > modified(old)) {
            newest = Some(path);
        }
    }
    newest.cloned()
}

/// When `path` was last written, or `None` when it cannot be read.
pub(crate) fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Whether `id` is a canonical UUID, the shape of every agent's session ids.
pub fn plausible_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::fixtures::*;

    /// Each event of a session moves its count, and no other session's.
    #[test]
    fn an_event_stirs_its_own_session() {
        let hints = Hints::default();
        assert_eq!(hints.stirred("a"), 0);
        hints.stir("a");
        hints.stir("a");
        assert_eq!((hints.stirred("a"), hints.stirred("b")), (2, 0));
        hints.retain(&HashSet::from(["b"]));
        assert_eq!(hints.stirred("a"), 0);
    }

    /// A hook event of a session whose records are rows is noted as a stir, since no path names
    /// what changed; one of a session whose records are files is not, its record being watched
    /// by its path.
    #[test]
    fn a_hook_stirs_a_session_whose_records_are_rows_and_no_other() {
        let (state, _dir) = crate::test_support::app_state("agent-records-hook-stir");
        let payload = serde_json::json!({"session_id": ROW_ID});

        on_hook(&state, "files", provider(AgentId::TEST).records(), &payload);
        assert_eq!(state.record_hints.stirred("files"), 0);

        let _declared = crate::providers::test_provider::with_row_records();
        on_hook(&state, "rows", provider(AgentId::TEST).records(), &payload);
        on_hook(&state, "rows", provider(AgentId::TEST).records(), &payload);
        assert_eq!(
            (
                state.record_hints.stirred("rows"),
                state.record_hints.stirred("files")
            ),
            (2, 0)
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

        assert_eq!(find(AgentId::CLAUDE, CLAUDE_ID), None);
        let orphan = account.join("projects/one").join(CLAUDE_ID);
        std::fs::create_dir_all(orphan.join("subagents")).unwrap();
        assert_eq!(find(AgentId::CLAUDE, CLAUDE_ID), None);
        let old = account
            .join("projects/one")
            .join(format!("{CLAUDE_ID}.jsonl"));
        touch(&old);
        crate::test_support::move_modification_time_back(&old);
        let new = account
            .join("projects/two")
            .join(format!("{CLAUDE_ID}.jsonl"));
        touch(&new);
        assert_eq!(find(AgentId::CLAUDE, CLAUDE_ID), Some(new));

        let grok = account.join("sessions/work").join(CLAUDE_ID);
        touch(&grok.join("usage.json"));
        assert_eq!(find(AgentId::GROK, CLAUDE_ID), None);
        touch(&grok.join("chat_history.jsonl"));
        assert_eq!(
            find(AgentId::GROK, CLAUDE_ID),
            Some(grok.join("chat_history.jsonl"))
        );
    }

    /// The history of a Codex thread that `codex archive` moved out of `sessions/` is read from
    /// `archived_sessions/`, and a record in `sessions/` still wins over it.
    #[test]
    fn history_finds_a_codex_rollout_that_was_archived() {
        let dir = crate::test_support::ScratchDir::new("agent-records-history-archived");
        let account = dir.join("account");
        let own = session(AgentId::CODEX, CODEX_ID, Some(&account));
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
        let other = session(AgentId::CLAUDE, CLAUDE_ID, Some(&account));
        assert_eq!(found(&other), None);
    }

    /// The file a conversation is read from: Grok Build's `chat_history.jsonl` in its session
    /// directory, Claude Code's transcript itself.
    #[test]
    fn the_conversation_is_read_from_each_agents_own_file() {
        let dir = crate::test_support::ScratchDir::new("agent-records-branch-file");
        let root = dir.join("account");
        let mut locator = Locator::with_shell(HashMap::new());
        let hints = Hints::default();
        let grok = root.join("sessions/%2Fwork").join(CLAUDE_ID);
        touch(&grok.join("usage.json"));
        touch(&grok.join("chat_history.jsonl"));
        let branch = conversation_branch(
            &hints,
            &mut locator,
            &[],
            &session(AgentId::GROK, CLAUDE_ID, Some(&root)),
        )
        .expect("grok conversation");
        assert_eq!(branch.record, grok.join("chat_history.jsonl"));

        let transcript = root.join("projects/one").join(format!("{CLAUDE_ID}.jsonl"));
        touch(&transcript);
        let branch = conversation_branch(
            &hints,
            &mut locator,
            &[],
            &session(AgentId::CLAUDE, CLAUDE_ID, Some(&root)),
        )
        .expect("claude conversation");
        assert_eq!(branch.record, transcript);
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
            agent: AgentId::CLAUDE,
            name: "Other".to_owned(),
            config_dir: other.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
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
            &session(AgentId::CLAUDE, CLAUDE_ID, Some(&account)),
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
                &session(AgentId::CODEX, CODEX_ID, Some(&account))
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
                &session(AgentId::GROK, CLAUDE_ID, Some(&account))
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
            locator.current_record(AgentId::CODEX, &account, CODEX_ID),
            Some(compressed.clone())
        );
        assert_eq!(
            record_paths(
                &Hints::default(),
                &mut locator,
                &[],
                &session(AgentId::CODEX, CODEX_ID, Some(&account))
            ),
            [compressed]
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
                    &session(AgentId::CLAUDE, id, Some(&account))
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
                    &session(AgentId::CLAUDE, CLAUDE_ID, Some(&account))
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
                    &session(AgentId::CLAUDE, CLAUDE_ID, Some(&account))
                )
                .is_empty(),
                "{hint:?}"
            );
        }
        assert!(record_paths(
            &watch,
            &mut locator,
            &[],
            &session(AgentId::CLAUDE, "subagents", Some(&account))
        )
        .is_empty());
        // A project directory that is a link is not followed.
        let linked = account.join("projects").join("linked");
        crate::test_support::symlink(&victim, &linked).unwrap();
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
            &session(AgentId::CLAUDE, CLAUDE_ID, Some(&account))
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
                &session(AgentId::CLAUDE, CLAUDE_ID, Some(&account))
            ),
            [inside, projects.join(CLAUDE_ID)]
        );
    }

    /// An agent that keeps no record files has no records directory, and the account's own
    /// directory is never taken for one: nothing is indexed, found, hinted at or removed there,
    /// though the provider's store would claim any file it were shown (the test-only provider's
    /// variant), so that a walk that reached the account's own files would find them.
    #[test]
    fn a_layout_with_no_records_directory_has_nothing_to_walk_find_or_remove() {
        let _declared = crate::providers::test_provider::without_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-none");
        let account = dir.join("account");
        let home = dir.join("home");
        let default_account =
            home.join(crate::providers::layout::layout(AgentId::TEST).default_dir);
        let strays: Vec<PathBuf> = [&account, &default_account]
            .into_iter()
            .flat_map(|root| {
                [
                    root.join(format!("{CLAUDE_ID}.jsonl")),
                    root.join("work").join(CLAUDE_ID).join("usage.json"),
                    root.join(CLAUDE_ID).join("usage.json"),
                ]
            })
            .collect();
        for stray in &strays {
            touch(stray);
        }
        assert_eq!(records_dir(AgentId::TEST, &account), None);
        let shell = HashMap::from([("HOME".to_owned(), home.to_string_lossy().into_owned())]);

        let mut locator = Locator::with_shell(shell.clone());
        let index = locator.index(AgentId::TEST, &account);
        assert!(index.records.is_empty());
        assert!(index.complete, "what is not found is not there");
        assert!(locator
            .current_record(AgentId::TEST, &account, CLAUDE_ID)
            .is_none());

        let own = session(AgentId::TEST, CLAUDE_ID, Some(&account));
        let watch = Hints::default();
        // A hint inside the account's own directory, or inside the default one, is no record of an
        // agent that has none.
        for hinted in [
            account.join(format!("{CLAUDE_ID}.jsonl")),
            default_account.join(format!("{CLAUDE_ID}.jsonl")),
            account.join("work").join(CLAUDE_ID).join("usage.json"),
        ] {
            watch
                .hints
                .lock()
                .unwrap()
                .insert("s".to_owned(), (CLAUDE_ID.to_owned(), hinted.clone()));
            assert!(
                record_paths(&watch, &mut locator, &[], &own).is_empty(),
                "{hinted:?}"
            );
            assert!(
                crate::records::removal::removable_record_paths(&watch, &mut locator, &[], &own)
                    .is_empty(),
                "{hinted:?}"
            );
        }
        // The default account's session, with no pinned directory, is no different.
        let default_own = session(AgentId::TEST, CLAUDE_ID, None);
        assert!(record_paths(&watch, &mut locator, &[], &default_own).is_empty());
        assert!(crate::records::removal::removable_record_paths(
            &watch,
            &mut locator,
            &[],
            &default_own
        )
        .is_empty());
        for root in [&account, &default_account] {
            assert!(
                crate::records::conversation_files_in(&mut locator, AgentId::TEST, root, CLAUDE_ID)
                    .is_empty(),
                "{root:?}"
            );
        }
        for stray in &strays {
            assert!(stray.exists(), "{stray:?} was touched");
        }
    }
}
