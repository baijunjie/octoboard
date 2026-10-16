//! Where an agent keeps its own on-disk records of a session: Claude Code's transcript (and the
//! directory of subagent transcripts beside it), Codex's rollout file, Grok Build's session
//! directory with its usage ledger. Shared by the usage collector, which reads them (`usage.rs`),
//! and by a session's deletion, which moves them to the Trash (`record_paths`).
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
    #[cfg(test)]
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

    /// The newest record of `agent_id` in the account at `root`.
    pub fn newest(&mut self, agent: Agent, root: &Path, agent_id: &str) -> Option<PathBuf> {
        let paths = self.index(agent, root).records.get(agent_id)?;
        let mut newest: Option<&PathBuf> = None;
        for path in paths {
            if newest.is_none_or(|old| modified(path) > modified(old)) {
                newest = Some(path);
            }
        }
        newest.cloned()
    }
}

/// The directory of an account where an agent keeps its session records.
fn records_dir(agent: Agent, root: &Path) -> PathBuf {
    root.join(match agent {
        Agent::Claude => "projects",
        Agent::Codex | Agent::Grok => "sessions",
    })
}

/// Index names once per account per pass; never read other sessions' contents or follow links.
fn index_records(agent: Agent, root: &Path) -> Index {
    fn walk(agent: Agent, dir: &Path, depth: usize, out: &mut Index) {
        match std::fs::symlink_metadata(dir) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
            _ => {
                out.complete = false;
                return;
            }
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => {
                out.complete = false;
                return;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                out.complete = false;
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                out.complete = false;
                continue;
            };
            if kind.is_symlink() {
                out.complete = false;
                continue;
            }
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                out.complete = false;
                continue;
            };
            let id = match agent {
                Agent::Claude if kind.is_file() && depth == 1 => {
                    name.strip_suffix(".jsonl").map(str::to_owned)
                }
                // A subagent directory may survive after the main transcript was removed.
                Agent::Claude if kind.is_dir() && depth == 1 && plausible_id(&name) => {
                    Some(name.clone())
                }
                Agent::Codex if kind.is_file() && name.starts_with("rollout-") => name
                    .strip_suffix(".jsonl")
                    .filter(|name| name.len() >= 36)
                    .and_then(|name| name.get(name.len() - 36..))
                    .map(str::to_owned),
                Agent::Grok if kind.is_dir() && depth == 1 => Some(name),
                _ => None,
            };
            if let Some(id) = id {
                let path = match agent {
                    Agent::Grok => path.join("usage.json"),
                    Agent::Claude if kind.is_dir() => path.with_file_name(format!("{id}.jsonl")),
                    _ => path,
                };
                let records = out.records.entry(id).or_default();
                if !records.contains(&path) {
                    records.push(path);
                }
            } else if kind.is_dir() && depth > 1 {
                walk(agent, &path, depth - 1, out);
            }
        }
    }
    let mut out = Index {
        complete: true,
        ..Index::default()
    };
    let depth = match agent {
        Agent::Claude | Agent::Grok => 2,
        Agent::Codex => 6,
    };
    if std::fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_symlink()) {
        out.complete = false;
    }
    walk(agent, &records_dir(agent, root), depth, &mut out);
    out
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// The agent's own records of `session`, for the ones to be moved to the Trash with it: Claude's
/// transcript and the directory of the same name beside it that holds its subagents' transcripts,
/// Codex's rollout file, Grok's session directory. Found everywhere on this machine the agent's
/// accounts could hold them (the session's own account, every account Octoboard knows of for the
/// agent, and the default one), as a switched session leaves its original copy behind in the old
/// account, and every copy in every Claude project directory.
///
/// Nothing but the session's own records is ever named: an agent session id that is not a canonical
/// UUID yields nothing, each path is built from an account's records directory and that id,
/// a hinted path counts only when it lies lexically inside one of those directories, and every record
/// must sit at its agent's exact layout depth under real (not linked) directories. A path may not
/// exist.
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

/// Whether `id` is a canonical UUID, the shape of every agent's session ids.
fn plausible_id(id: &str) -> bool {
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
/// `<dir>/<project>/<id>.jsonl`, Codex `<dir>/<year>/<month>/<day>/rollout-<time>-<id>.jsonl`, Grok
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
                name.strip_suffix(".jsonl").is_some_and(|stem| {
                    stem.starts_with("rollout-")
                        && stem.len() >= 36
                        && stem.get(stem.len() - 36..) == Some(agent_id)
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
}
