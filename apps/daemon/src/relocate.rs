//! Copying one session's conversation record from one account's config directory into another's,
//! which is what lets a switch of the session's account resume the conversation instead of
//! starting a new one: every agent keeps a session's record inside its config directory, so a
//! resume under another directory does not find it.
//!
//! The record is found by name under the root where the agent writes its records, and copied to
//! the same path *relative to the config directory* in the target, creating the directories in
//! between. Nothing here derives Claude Code's project slug or Grok's encoding of a working
//! directory: the source directory already holds the path the agent wrote, and a second
//! implementation of that rule would be a guess at something the agent already decided. For Grok
//! the directories are the account's own (the source home a per-session home is built from), never
//! a per-session home, which is discarded with its process.
//!
//! The record alone is sufficient for Claude Code and Grok Build to resume — no index, registry or
//! per-project entry is copied with it — but it is not the whole conversation, so what the agent
//! keeps beside it travels too ([`companions`]): Claude Code's directory of the same name (subagent
//! transcripts, tool results) and its rewind checkpoints. For Codex the record alone is not
//! sufficient: a thread that had a turn reverted is a chain of rollouts, the record is the chain's
//! head, and the first rollout and the segments between are its companions. Nothing else is
//! touched in either directory: the copy only ever adds (or replaces) those items of the one
//! session, besides a transient staging directory at the target's root that it creates and
//! removes again, and, for Codex, the one `rollout_path` its database keeps of the thread
//! ([`repoint_thread_row`]). It copies rather than moves, so a switch that fails afterwards leaves
//! the original where it was and switching back needs no second copy. The measurements are in
//! `docs/agent-cli-reference.md`.

use std::collections::{BTreeMap, HashMap};
use std::fs::FileType;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;

use crate::agent_records::{names_thread, plausible_id, rollout_head, rollout_stem};
use crate::protocol::{error_code, Agent, CodedError};
use crate::shared_config::FailureCause as Cause;

/// Where an agent keeps its session records inside a config directory (`root`), how deep below it
/// a record sits (`depth`, counting `root`'s own entries as 1), and what a record is called. The
/// entry's own type is what is tested, so a symbolic link is never a record.
/// Codex files its records under dated directories it does not look them up by, so its depth is
/// generous rather than exact.
struct Layout {
    root: &'static str,
    depth: usize,
    is_record: fn(name: &str, file_type: FileType, agent_session_id: &str) -> bool,
}

fn layout(agent: Agent) -> Layout {
    match agent {
        // `projects/<cwd slug>/<session id>.jsonl`
        Agent::Claude => Layout {
            root: "projects",
            depth: 2,
            is_record: |name, kind, id| kind.is_file() && name == format!("{id}.jsonl"),
        },
        // `sessions/<date directories>/rollout-<timestamp>-<thread id>.jsonl`, or `….jsonl.zst`
        // once Codex has compressed it, and for a thread that had a turn reverted a later segment
        // `rollout-<timestamp>-<thread id>_<segment id>` for each
        Agent::Codex => Layout {
            root: "sessions",
            depth: 6,
            is_record: |name, kind, id| {
                kind.is_file()
                    && name.starts_with("rollout-")
                    && rollout_stem(name).is_some_and(|stem| names_thread(stem, id))
            },
        },
        // `sessions/<url-encoded cwd>/<session id>/`, a directory kept whole
        Agent::Grok => Layout {
            root: "sessions",
            depth: 2,
            is_record: |name, kind, id| kind.is_dir() && name == id,
        },
    }
}

/// The path, relative to `source`, of the session's record in the account whose directory is
/// `source`: for Codex the head of the thread's chain of rollouts. Refused with
/// `conversation_not_found` when there is none, which the caller meets before it has ended
/// anything. Compared by entry name only, never joined into a path, so an id the agent reported
/// cannot steer the search out of the directory.
pub fn locate(agent: Agent, source: &Path, agent_session_id: &str) -> Result<PathBuf> {
    let found = find(agent, source, agent_session_id);
    let record = if agent == Agent::Codex {
        let absolute: Vec<PathBuf> = found.iter().map(|relative| source.join(relative)).collect();
        rollout_head(&absolute)
            .and_then(|head| head.strip_prefix(source).ok().map(Path::to_path_buf))
    } else {
        // More than one is not expected; if it happens the one written to last is the live one.
        found
            .into_iter()
            .max_by_key(|relative| modified(&source.join(relative)))
    };
    record.ok_or_else(|| {
        CodedError::raised(
            error_code::CONVERSATION_NOT_FOUND,
            format!(
                "no conversation record of this session was found in `{}`",
                source.display()
            ),
            &[
                ("agent", agent.label()),
                ("path", &source.to_string_lossy()),
            ],
        )
    })
}

/// Every record of the session in `source`, as paths relative to it.
fn find(agent: Agent, source: &Path, agent_session_id: &str) -> Vec<PathBuf> {
    let layout = layout(agent);
    let mut found = Vec::new();
    if !agent_session_id.is_empty() {
        search(
            source,
            Path::new(layout.root),
            1,
            &layout,
            agent_session_id,
            &mut found,
        );
    }
    found
}

/// What the agent keeps for the session besides its record, as paths relative to `source` that
/// exist there, for a switch to carry with it. Claude Code: the directory of the record's own
/// name beside it (subagent transcripts, tool results, the custom title) and `file-history/<id>`
/// (rewind checkpoints). Codex: the other rollouts of the thread's chain, which with `record` (the
/// head) are the whole conversation: a target given the head alone is refused, and one given the
/// first rollout alone resumes a stale branch. An id that is no canonical UUID yields nothing,
/// as it is joined into a path here.
pub fn companions(
    agent: Agent,
    source: &Path,
    record: &Path,
    agent_session_id: &str,
) -> Vec<PathBuf> {
    if !plausible_id(agent_session_id) {
        return Vec::new();
    }
    match agent {
        Agent::Claude => [
            record.with_extension(""),
            Path::new("file-history").join(agent_session_id),
        ]
        .into_iter()
        .filter(|relative| std::fs::symlink_metadata(source.join(relative)).is_ok())
        .collect(),
        // A subagent's rollout is named by its own thread id and never matches the thread's, so
        // it stays behind. Grok keeps everything inside the session directory.
        Agent::Codex => find(agent, source, agent_session_id)
            .into_iter()
            .filter(|relative| relative != record)
            .collect(),
        Agent::Grok => Vec::new(),
    }
}

/// How long the target's database may be busy before the update gives up.
const DATABASE_PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Points a Codex home's own record of `thread` at the rollout the switch just carried in, which
/// is the one write a switch makes to an agent's own state beside the copy. Codex resumes a thread
/// by the rollout its database row names, and falls back to the rollouts' names only for a thread
/// with no row (measured with 0.162.1), so a home that held the thread before it was reverted
/// elsewhere would follow its row to the stale head however whole the chain copied in is. The
/// update changes the row's `rollout_path` and no other column or table: the row's other columns
/// (title, pin, section, project) and the rows other tables keep of the thread (links, tools) stay
/// as the user left them. The path written is the head's in the target without any `.zst`, the
/// form Codex keeps for a rollout it has compressed. A home with no row of the thread has nothing
/// to point, and neither has one whose row names no path or already names that same file: the
/// path it names and the head resolve to one file when the two accounts share `sessions/` and the
/// database, and a row that names another file is pointed even if the database is shared (two
/// homes may keep one database without sharing their rollouts).
///
/// `launch_env` is the environment the relaunch will see, which with the home's `config.toml` says
/// where the home keeps its database ([`database_directory`]). A switch fails
/// (`relocation_failed`, before anything is relaunched) when where the database is cannot be
/// determined (a `config.toml` that cannot be read or parsed, a relative `CODEX_SQLITE_HOME`, a
/// directory that cannot be listed) or it cannot be written to or stays busy: a row left standing
/// would resume the stale branch with no sign of it.
pub fn repoint_thread_row(
    agent: Agent,
    target: &Path,
    head: &Path,
    thread: &str,
    launch_env: &HashMap<String, String>,
) -> Result<()> {
    repoint_within(agent, target, head, thread, launch_env, DATABASE_PATIENCE)
}

/// Why a conversation could not be moved, in the words of the wire's `cause` param on
/// `relocation_failed`: the file system's own causes ([`Cause`]), which a user can act on, and the
/// ones of this move alone — `config_unreadable` (the target's `config.toml` cannot be read or
/// parsed), `sqlite_home` (where its thread database is cannot be told), `symlink` (a record holds
/// a symbolic link, which is not copied) and `not_restored` (an older item could not be put back).
/// `other` is everything else. Not found and conflicts are told only as `other`.
fn cause_of_io(err: &io::Error) -> &'static str {
    narrowed(Cause::of(err))
}

/// The wire word for `cause` where this move tells it apart, else `other`.
fn narrowed(cause: Cause) -> &'static str {
    match cause {
        Cause::PermissionDenied | Cause::Busy | Cause::NoSpace => cause.as_str(),
        _ => "other",
    }
}

/// A failure of where the thread database is, with its cause.
struct DatabaseDirectoryError {
    cause: &'static str,
    text: String,
}

/// A symbolic link met while copying a record, which fails the copy.
#[derive(Debug)]
struct SymbolicLink(String);

impl std::fmt::Display for SymbolicLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` is a symbolic link", self.0)
    }
}

impl std::error::Error for SymbolicLink {}

fn repoint_within(
    agent: Agent,
    target: &Path,
    head: &Path,
    thread: &str,
    launch_env: &HashMap<String, String>,
    patience: std::time::Duration,
) -> Result<()> {
    if agent != Agent::Codex || !plausible_id(thread) {
        return Ok(());
    }
    let failed = |cause: &str, detail: String| {
        let message = format!(
            "the conversation was copied but Codex's record of the thread was not updated: \
             {detail}"
        );
        CodedError::raised(
            error_code::RELOCATION_FAILED,
            message,
            &[
                ("path", &target.to_string_lossy()),
                ("stage", "thread_record"),
                ("cause", cause),
                ("detail", &detail),
            ],
        )
    };
    let directory =
        database_directory(target, launch_env).map_err(|err| failed(err.cause, err.text))?;
    let mut rollout = target.join(head);
    if rollout.extension().is_some_and(|ext| ext == "zst") {
        rollout.set_extension("");
    }
    let listing = |err: io::Error| {
        failed(
            cause_of_io(&err),
            format!("listing `{}` failed: {err}", directory.display()),
        )
    };
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        // No directory, no database: the home has not run Codex yet.
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(listing(err)),
    };
    for entry in entries {
        let entry = entry.map_err(listing)?;
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_state_database)
        {
            continue;
        }
        // A link counts when it leads to a file (an account sharing its database with the default
        // account links it), and a dangling one is no database. A link that cannot be followed
        // for another reason fails the switch: it is the database, and its row cannot be checked.
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => continue,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(failed(
                    cause_of_io(&err),
                    format!("inspecting `{}` failed: {err}", path.display()),
                ))
            }
        }
        repoint_in(&path, thread, &rollout, patience).map_err(|err| {
            failed(
                narrowed(Cause::of_database(&err)),
                format!("updating `{}` failed: {err}", path.display()),
            )
        })?;
    }
    Ok(())
}

/// Whether `name` is a Codex thread state database, `state_<n>.sqlite`: the number is Codex's
/// schema generation, which a newer Codex may raise.
pub(crate) fn is_state_database(name: &str) -> bool {
    name.strip_prefix("state_")
        .and_then(|rest| rest.strip_suffix(".sqlite"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Opens an agent's SQLite database for writing, waiting up to `patience` while another process
/// has it busy. Never creates one: a file that is not there, or is not a database, is an error.
/// Foreign keys are off on it, as SQLite leaves them: a caller deleting `threads` rows turns them on
/// first, or the child rows stay and attach to a row later inserted with the same id.
pub(crate) fn open_existing_database(
    database: &Path,
    patience: std::time::Duration,
) -> rusqlite::Result<rusqlite::Connection> {
    let connection = rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(patience)?;
    Ok(connection)
}

/// Where a Codex home keeps its databases: its own directory, unless its `config.toml` sets
/// `sqlite_home`, which wins over the `CODEX_SQLITE_HOME` environment variable (measured with
/// 0.162.1). A relative `sqlite_home` is taken against the home and a leading `~` is the launch
/// environment's `HOME`; a relative `CODEX_SQLITE_HOME` is taken against the directory Codex is
/// started in, which is not known here, so it is refused. Codex also lets an enterprise
/// requirement override both, which is not looked for here.
fn database_directory(
    home: &Path,
    launch_env: &HashMap<String, String>,
) -> Result<PathBuf, DatabaseDirectoryError> {
    let unresolvable = |text: String| DatabaseDirectoryError {
        cause: "sqlite_home",
        text,
    };
    let configured = match std::fs::read_to_string(home.join("config.toml")) {
        Ok(text) => toml::from_str::<toml::Table>(&text)
            .map_err(|err| DatabaseDirectoryError {
                cause: "config_unreadable",
                text: format!("`config.toml` is not valid: {err}"),
            })?
            .get("sqlite_home")
            .and_then(toml::Value::as_str)
            .map(str::to_owned),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(DatabaseDirectoryError {
                cause: match cause_of_io(&err) {
                    "other" => "config_unreadable",
                    cause => cause,
                },
                text: err.to_string(),
            })
        }
    };
    if let Some(dir) = configured {
        let user_home = launch_env.get("HOME").filter(|home| !home.is_empty());
        return match (dir.strip_prefix('~'), user_home) {
            (Some(rest), Some(user_home)) if rest.is_empty() || rest.starts_with('/') => {
                Ok(PathBuf::from(user_home).join(rest.trim_start_matches('/')))
            }
            (Some(_), None) => Err(unresolvable(
                "`sqlite_home` starts with `~` and the launch environment has no HOME".into(),
            )),
            _ => Ok(home.join(dir)),
        };
    }
    match launch_env
        .get("CODEX_SQLITE_HOME")
        .filter(|dir| !dir.is_empty())
    {
        Some(dir) if Path::new(dir).is_absolute() => Ok(PathBuf::from(dir)),
        Some(dir) => Err(unresolvable(format!(
            "`CODEX_SQLITE_HOME` is the relative path `{dir}`, which depends on the directory \
             the session starts in"
        ))),
        None => Ok(home.to_path_buf()),
    }
}

fn repoint_in(
    database: &Path,
    thread: &str,
    rollout: &Path,
    patience: std::time::Duration,
) -> rusqlite::Result<()> {
    use rusqlite::OptionalExtension;
    let connection = open_existing_database(database, patience)?;
    let has_table = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'threads'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_table {
        return Ok(());
    }
    let named: Option<Option<String>> = connection
        .query_row(
            "SELECT rollout_path FROM threads WHERE id = ?1",
            [thread],
            |row| row.get(0),
        )
        .optional()?;
    // A row that already names the rollout, by whatever route, is left as it is.
    // A row with no path names no rollout to compare, so it is left alone.
    if named
        .flatten()
        .is_some_and(|named| resolved(Path::new(&named)) != resolved(rollout))
    {
        connection.execute(
            "UPDATE threads SET rollout_path = ?1 WHERE id = ?2",
            rusqlite::params![rollout.to_string_lossy(), thread],
        )?;
    }
    Ok(())
}

/// The path a rollout is found at, with the links of its directory resolved. The file itself is
/// not looked up, as the path written for a compressed rollout names one that need not exist; a
/// directory that cannot be resolved leaves the path as written.
fn resolved(path: &Path) -> PathBuf {
    match (path.parent().map(std::fs::canonicalize), path.file_name()) {
        (Some(Ok(parent)), Some(name)) => parent.join(name),
        _ => path.to_path_buf(),
    }
}

fn search(
    source: &Path,
    directory: &Path,
    depth: usize,
    layout: &Layout,
    agent_session_id: &str,
    found: &mut Vec<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(source.join(directory)) else {
        return;
    };
    for entry in entries.flatten() {
        // The type of the entry itself: a symbolic link is neither a file nor a directory here,
        // so it is neither followed nor taken for a record.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let Some(name_text) = name.to_str() else {
            continue;
        };
        let relative = directory.join(&name);
        if (layout.is_record)(name_text, file_type, agent_session_id) {
            found.push(relative);
        } else if file_type.is_dir() && depth < layout.depth {
            search(
                source,
                &relative,
                depth + 1,
                layout,
                agent_session_id,
                found,
            );
        }
    }
}

pub(crate) fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// The directory, at the config directory's root, where copies are built before they replace
/// anything. Outside everything an agent reads, so a crash mid-copy leaves nothing among the
/// agent's own records. Each copy works in a subdirectory of its own, named for the record (unique
/// per session for all three agents), and clears only that: several switches can be copying into
/// one account at once, each of a different session, and none may touch another's stage or its
/// set-aside previous record. What a crash leaves is swept at daemon start ([`sweep_staging`]).
/// The final rename is across directories within the config directory, so an unusual layout (a
/// `projects` directory that is a link onto another volume) can make it fail, which is a refused
/// switch and not a lost record.
const STAGING: &str = ".octoboard-switch";

/// Removes the staging directory a crashed daemon left in each of these config directories.
/// Called once at start, before any switch can be running.
pub fn sweep_staging<'a>(config_dirs: impl IntoIterator<Item = &'a Path>) {
    for dir in config_dirs {
        let staging = dir.join(STAGING);
        if let Err(err) = remove_any(&staging) {
            tracing::warn!(path = %staging.display(), %err, "clearing a switch's staging failed");
        }
    }
}

/// Copies the `record` and its `companions` (see [`companions`]) from `source` to the same
/// places in `target`, creating the directories in between, replacing what is already there.
/// Done only once every copy is complete and matches its original, so the target never holds a
/// half-written item: they are all built in [`STAGING`], checked, and only then moved into place,
/// each set aside first what it replaces. A swap that fails puts back every item already swapped
/// and the older ones set aside, so the target holds all of the new items or the older ones, bar
/// empty parent directories the swap may have created. Where an older item cannot be put back,
/// its staging directory is kept (it holds the item) and the error says so; it survives only
/// until the next daemon start ([`sweep_staging`]) or the next switch of this session into this
/// account, which clears it. Anything short of
/// complete is `relocation_failed`, and the caller must not relaunch. An item that is the same
/// file on both sides (two accounts that share a directory) has nothing to copy.
pub fn copy(source: &Path, target: &Path, record: &Path, companions: &[PathBuf]) -> Result<()> {
    let Some(name) = record.file_name() else {
        return Err(CodedError::raised(
            error_code::RELOCATION_FAILED,
            "the record has no name",
            &[
                ("path", &target.to_string_lossy()),
                ("stage", "copy"),
                ("cause", "other"),
                ("detail", "no name"),
            ],
        ));
    };
    let items: Vec<(PathBuf, PathBuf)> = std::iter::once(record)
        .chain(companions.iter().map(PathBuf::as_path))
        .map(|relative| (source.join(relative), target.join(relative)))
        .filter(|(from, to)| !is_same_file(from, to))
        .collect();
    if items.is_empty() {
        return Ok(());
    }
    let shared = target.join(STAGING);
    let staging = shared.join(name);
    // This copy's own subdirectory only, cleared before and after; the shared parent is left for
    // the other copies that may be using it, and removed here only once empty.
    let mut kept = false;
    let outcome = remove_any(&staging)
        .map_err(Failed::from)
        .and_then(|()| stage_and_swap(&items, &staging))
        .map_err(|failed| {
            kept = failed.unrestored;
            let detail = if kept {
                format!(
                    "{}; an older item could not be put back and is kept in `{}` until the next \
                     daemon start or switch of this session into this account",
                    failed.err,
                    staging.display()
                )
            } else {
                failed.err.to_string()
            };
            let cause = if kept {
                "not_restored"
            } else if failed
                .err
                .get_ref()
                .is_some_and(|inner| inner.is::<SymbolicLink>())
            {
                "symlink"
            } else {
                cause_of_io(&failed.err)
            };
            let mut params = vec![
                ("path", target.to_string_lossy().into_owned()),
                (
                    "stage",
                    if kept { "kept_staging" } else { "copy" }.to_string(),
                ),
                ("cause", cause.to_string()),
                ("detail", failed.err.to_string()),
            ];
            if kept {
                params.push(("kept_in", staging.to_string_lossy().into_owned()));
            }
            let params: Vec<(&str, &str)> = params
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect();
            CodedError::raised(
                error_code::RELOCATION_FAILED,
                format!(
                    "copying the conversation into `{}` did not complete: {detail}",
                    target.display()
                ),
                &params,
            )
        });
    if kept {
        tracing::warn!(
            path = %staging.display(),
            "an older item was not put back; staging kept until the next start or switch"
        );
    } else {
        remove_any(&staging).ok();
        std::fs::remove_dir(&shared).ok();
    }
    outcome
}

/// A copy that did not complete, and whether it left an older item set aside in the staging
/// directory instead of back in its place.
struct Failed {
    err: io::Error,
    unrestored: bool,
}

impl From<io::Error> for Failed {
    fn from(err: io::Error) -> Self {
        Self {
            err,
            unrestored: false,
        }
    }
}

fn is_same_file(from: &Path, to: &Path) -> bool {
    match (std::fs::canonicalize(from), std::fs::canonicalize(to)) {
        (Ok(from), Ok(to)) => from == to,
        _ => false,
    }
}

/// Builds each item `(from, to)` in `staging/new/<n>`, checks it, then swaps them all into place.
fn stage_and_swap(items: &[(PathBuf, PathBuf)], staging: &Path) -> Result<(), Failed> {
    let mut staged = Vec::new();
    for (n, (from, to)) in items.iter().enumerate() {
        let stage = staging.join("new").join(n.to_string());
        std::fs::create_dir_all(stage.parent().expect("a stage has a parent"))?;
        copy_tree(from, &stage)?;
        if footprint(from)? != footprint(&stage)? {
            return Err(io::Error::other("the copy differs from the original").into());
        }
        staged.push((
            stage,
            to.as_path(),
            staging.join("previous").join(n.to_string()),
        ));
    }
    swap_into_place(&staged)
}

/// Moves each stage to its place, setting aside what is there first (in the third path, which
/// lies in the staging directory), since a directory cannot be renamed over. If any step fails,
/// everything moved so far is undone: the new items removed and the older ones put back.
fn swap_into_place(items: &[(PathBuf, &Path, PathBuf)]) -> Result<(), Failed> {
    // The items whose place now holds the new one, and whether an older one was set aside for it.
    let mut done: Vec<(&Path, &Path, bool)> = Vec::new();
    for (stage, to, aside) in items {
        let step = (|| {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|err| (err, false))?;
            }
            let replaced = std::fs::symlink_metadata(to).is_ok();
            if replaced {
                std::fs::create_dir_all(aside.parent().expect("an aside has a parent"))
                    .and_then(|()| std::fs::rename(to, aside))
                    .map_err(|err| (err, false))?;
            }
            if let Err(err) = std::fs::rename(stage, to) {
                let put_back = !replaced || std::fs::rename(aside, to).is_ok();
                return Err((err, !put_back));
            }
            Ok(replaced)
        })();
        match step {
            Ok(replaced) => done.push((to, aside, replaced)),
            Err((err, mut unrestored)) => {
                for (to, aside, replaced) in done.into_iter().rev() {
                    unrestored |= remove_any(to).is_err()
                        || (replaced && std::fs::rename(aside, to).is_err());
                }
                return Err(Failed { err, unrestored });
            }
        }
    }
    Ok(())
}

pub(crate) fn remove_any(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Copies a file, or a directory with everything in it. A symbolic link inside a record is not
/// something any agent writes there, and following one could copy from outside it, so it fails the
/// copy.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    let file_type = std::fs::symlink_metadata(from)?.file_type();
    if file_type.is_symlink() {
        return Err(io::Error::other(SymbolicLink(from.display().to_string())));
    }
    if file_type.is_dir() {
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

/// Every file under `path` (or `path` itself, as the empty path) and its size: what a copy has to
/// reproduce to count as complete.
fn footprint(path: &Path) -> io::Result<BTreeMap<PathBuf, u64>> {
    fn walk(root: &Path, path: &Path, into: &mut BTreeMap<PathBuf, u64>) -> io::Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                walk(root, &entry?.path(), into)?;
            }
        } else {
            let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
            into.insert(relative, metadata.len());
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(path, path, &mut files)?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn temp_dir(name: &str) -> ScratchDir {
        ScratchDir::new(&format!("relocate-{name}"))
    }

    fn write(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn stage_of(err: &anyhow::Error) -> String {
        err.downcast_ref::<CodedError>()
            .expect("a coded error")
            .params
            .get("stage")
            .cloned()
            .expect("a stage")
    }

    fn cause_of(err: &anyhow::Error) -> String {
        err.downcast_ref::<CodedError>()
            .expect("a coded error")
            .params
            .get("cause")
            .cloned()
            .expect("a cause")
    }

    fn code_of(err: &anyhow::Error) -> crate::protocol::Code {
        err.downcast_ref::<CodedError>()
            .expect("a coded error")
            .code
    }

    /// The record keeps the path it has relative to the config directory, whatever the agent put
    /// in the middle of it, with the directories in between created; the original stays; and a
    /// neighbour that is not this session's is not copied. Nothing in it derives a slug or an
    /// encoding.
    #[test]
    fn the_record_keeps_its_path_relative_to_the_config_directory_and_is_copied_not_moved() {
        // The record, a file inside it (the record itself unless it is a directory) and a
        // neighbour belonging to another session.
        let cases = [
            (
                Agent::Claude,
                "claude-id",
                "projects/-Users-me-repo/claude-id.jsonl",
                "projects/-Users-me-repo/claude-id.jsonl",
                "projects/-Users-me-repo/other.jsonl",
            ),
            (
                Agent::Codex,
                "codex-id",
                "sessions/2026/10/08/rollout-2026-10-08T01-02-03-codex-id.jsonl",
                "sessions/2026/10/08/rollout-2026-10-08T01-02-03-codex-id.jsonl",
                "sessions/2026/10/08/rollout-2026-10-08T01-02-04-other.jsonl",
            ),
            (
                Agent::Grok,
                "grok-id",
                "sessions/%2FUsers%2Fme%2Frepo/grok-id",
                "sessions/%2FUsers%2Fme%2Frepo/grok-id/messages.jsonl",
                "sessions/%2FUsers%2Fme%2Frepo/other/messages.jsonl",
            ),
        ];
        for (agent, id, record, file, neighbour) in cases {
            let source = temp_dir(&format!("{id}-source"));
            let target = temp_dir(&format!("{id}-target"));
            write(&source, file, "conversation");
            write(&source, neighbour, "someone else's");

            let relative = locate(agent, &source, id).expect("found");
            assert_eq!(relative, Path::new(record), "{agent:?}");

            copy(&source, &target, &relative, &[]).expect("copied");
            assert_eq!(
                std::fs::read_to_string(target.join(file)).unwrap(),
                "conversation",
                "{agent:?}"
            );
            assert!(source.join(file).exists(), "{agent:?} was moved");
            assert!(
                !target.join(neighbour).exists(),
                "{agent:?} copied a neighbour"
            );
        }
    }

    const CLAUDE_ID: &str = "11111111-2222-3333-4444-555555555555";
    const CODEX_ID: &str = "019a0000-0000-7000-8000-000000000001";

    /// A Codex thread that had a turn reverted is carried whole: the head is the record, the
    /// first rollout and the middle segment travel with it as companions, compressed ones as they
    /// are (the bytes stay compressed). A subagent's rollout, named by its own thread id, stays
    /// behind.
    #[test]
    fn a_reverted_codex_thread_is_carried_whole() {
        for compressed in [false, true] {
            let source = temp_dir("codex-source");
            let target = temp_dir("codex-target");
            let chain = crate::test_support::codex_chain(&source, CODEX_ID, compressed);
            let relative = |path: &Path| path.strip_prefix(&*source).unwrap().to_path_buf();
            let child = "019a0000-0000-7000-8000-00000000000a";
            let subagent = format!("sessions/2026/10/01/rollout-2026-10-01T01-05-00-{child}.jsonl");
            write(&source, &subagent, "subagent");

            let located = locate(Agent::Codex, &source, CODEX_ID).expect("found");
            assert_eq!(located, relative(&chain.head), "compressed: {compressed}");
            let mut relatives = companions(Agent::Codex, &source, &located, CODEX_ID);
            relatives.sort();
            assert_eq!(
                relatives,
                [relative(&chain.first), relative(&chain.middle)],
                "compressed: {compressed}"
            );

            copy(&source, &target, &located, &relatives).expect("copied");
            for file in chain.all() {
                assert_eq!(
                    std::fs::read(target.join(relative(&file))).unwrap(),
                    std::fs::read(&file).unwrap(),
                    "{file:?}"
                );
            }
            assert!(
                !target.join(&subagent).exists(),
                "the subagent's was carried"
            );
            assert!(source.join(relative(&chain.head)).is_file(), "moved");
        }
    }

    /// The target's database row of the thread is pointed at the carried head and nothing else of
    /// it or of the database changes; the database is found where the home's `config.toml` or the
    /// environment puts it; a home with no database, or one with no `threads` table, has nothing to
    /// point; a database that stays busy or is no database fails the switch, saying the
    /// conversation was copied.
    #[test]
    fn a_codex_homes_row_of_the_thread_is_pointed_at_the_head_and_nothing_else() {
        use rusqlite::Connection;
        let target = temp_dir("point-row");
        let other = "019a0000-0000-7000-8000-0000000000bb";
        let head = Path::new("sessions/2026/10/03/rollout-2026-10-03T00-00-00-x_y.jsonl.zst");
        let database = target.join("state_5.sqlite");
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT);
                 CREATE TABLE thread_attachments (thread_id TEXT NOT NULL
                     REFERENCES threads(id) ON DELETE CASCADE);
                 INSERT INTO threads VALUES ('{CODEX_ID}', 'old', 'kept'), ('{other}', 'other', 'x');
                 INSERT INTO thread_attachments VALUES ('{CODEX_ID}');"
            ))
            .unwrap();
        let path_of = |id: &str, database: &Path| -> String {
            Connection::open(database)
                .unwrap()
                .query_row(
                    "SELECT rollout_path FROM threads WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .unwrap()
        };
        let patience = std::time::Duration::from_millis(50);
        let point = |target: &Path, thread: &str, env: &[(&str, &str)]| {
            let env = env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            repoint_within(Agent::Codex, target, head, thread, &env, patience)
        };

        let held = Connection::open(&database).unwrap();
        held.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let err = point(&target, CODEX_ID, &[]).expect_err("a busy database");
        assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
        assert_eq!(stage_of(&err), "thread_record");
        assert_eq!(cause_of(&err), "busy");
        assert!(
            err.to_string().contains("conversation was copied"),
            "{err:#}"
        );
        held.execute_batch("ROLLBACK").unwrap();
        assert_eq!(path_of(CODEX_ID, &database), "old");

        point(&target, CODEX_ID, &[]).unwrap();
        // The head's path in the target, without the `.zst` Codex has compressed it to.
        let expected = target
            .join(head)
            .with_extension("")
            .to_string_lossy()
            .into_owned();
        assert_eq!(path_of(CODEX_ID, &database), expected);
        assert_eq!(path_of(other, &database), "other");
        let conn = Connection::open(&database).unwrap();
        let kept: (String, i64) = conn
            .query_row(
                "SELECT (SELECT title FROM threads WHERE id = ?1),
                        (SELECT count(*) FROM thread_attachments)",
                [CODEX_ID],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(kept, ("kept".to_owned(), 1), "more than the path changed");

        // A thread with no row, another agent, no database, no table, no database file.
        point(&target, other.replace("bb", "cc").as_str(), &[]).unwrap();
        repoint_within(
            Agent::Claude,
            &target,
            head,
            CODEX_ID,
            &HashMap::new(),
            patience,
        )
        .unwrap();
        point(&temp_dir("point-none"), CODEX_ID, &[]).unwrap();
        let bare = temp_dir("point-bare");
        Connection::open(bare.join("state_9.sqlite"))
            .unwrap()
            .execute_batch("CREATE TABLE unrelated (id TEXT)")
            .unwrap();
        point(&bare, CODEX_ID, &[]).unwrap();
        let broken = temp_dir("point-broken");
        write(
            &broken,
            "state_5.sqlite",
            "not a database, but long enough to be read as one",
        );
        point(&broken, CODEX_ID, &[]).expect_err("a file that is no database");

        // The database lives where `config.toml` says, else where the environment says; the
        // configuration wins. `~` is the launch environment's HOME.
        let moved = temp_dir("point-moved");
        let elsewhere = moved.join("elsewhere");
        let (elsewhere_text, user_home) = (elsewhere.to_string_lossy(), moved.join("user"));
        let user_home_text = user_home.to_string_lossy();
        // (name, config.toml, launch environment, where the database is)
        type Case<'a> = (&'a str, Option<&'a str>, Vec<(&'a str, &'a str)>, PathBuf);
        let cases: [Case; 4] = [
            (
                "config",
                Some("sqlite_home = \"db\""),
                vec![],
                moved.join("config/db"),
            ),
            (
                "env",
                None,
                vec![("CODEX_SQLITE_HOME", &elsewhere_text)],
                elsewhere.clone(),
            ),
            (
                "both",
                Some("sqlite_home = \"db\""),
                vec![("CODEX_SQLITE_HOME", &elsewhere_text)],
                moved.join("both/db"),
            ),
            (
                "tilde",
                Some("sqlite_home = \"~/tdb\""),
                vec![("HOME", &user_home_text)],
                user_home.join("tdb"),
            ),
        ];
        for (name, config, env, directory) in cases {
            let home = moved.join(name);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::create_dir_all(&home).unwrap();
            if let Some(config) = config {
                write(&home, "config.toml", config);
            }
            let moved_database = directory.join("state_5.sqlite");
            std::fs::copy(&database, &moved_database).unwrap();
            Connection::open(&moved_database)
                .unwrap()
                .execute("UPDATE threads SET rollout_path = 'old'", [])
                .unwrap();
            point(&home, CODEX_ID, &env).unwrap();
            let expected = home
                .join(head)
                .with_extension("")
                .to_string_lossy()
                .into_owned();
            assert_eq!(path_of(CODEX_ID, &moved_database), expected, "{name}");
        }

        // Where the database is cannot be told: not guessed past. Nor can an unlistable
        // directory (the setting names a file) be taken for "no database"; a missing one can.
        for (name, config, env) in [
            ("unreadable", "sqlite_home = [", vec![]),
            ("no-home", "sqlite_home = \"~/db\"", vec![]),
            ("relative-env", "", vec![("CODEX_SQLITE_HOME", "db")]),
            (
                "not-a-directory",
                "sqlite_home = \"state_5.sqlite\"",
                vec![],
            ),
        ] {
            let home = moved.join(name);
            write(&home, "config.toml", config);
            let in_home = home.join("state_5.sqlite");
            if name == "not-a-directory" {
                write(&home, "state_5.sqlite", "a file where a directory is named");
            } else {
                // A valid database in the home itself: a fallback to it would be caught.
                std::fs::copy(&database, &in_home).unwrap();
                Connection::open(&in_home)
                    .unwrap()
                    .execute("UPDATE threads SET rollout_path = 'old'", [])
                    .unwrap();
            }
            point(&home, CODEX_ID, &env).expect_err(name);
            if name != "not-a-directory" {
                assert_eq!(
                    path_of(CODEX_ID, &in_home),
                    "old",
                    "{name} fell back to the home"
                );
            }
        }
        let absent = moved.join("absent");
        write(&absent, "config.toml", "sqlite_home = \"nowhere\"");
        point(&absent, CODEX_ID, &[]).unwrap();
    }

    /// Claude Code's directory beside the transcript and the rewind checkpoints travel with it
    /// when they exist, and are not invented when they do not.
    #[test]
    fn a_claude_transcripts_sidecar_directory_and_rewind_checkpoints_are_carried() {
        let source = temp_dir("claude-sidecar-source");
        let target = temp_dir("claude-sidecar-target");
        let record = format!("projects/-Users-me-repo/{CLAUDE_ID}.jsonl");
        write(&source, &record, "conversation");
        write(
            &source,
            &format!("projects/-Users-me-repo/{CLAUDE_ID}/subagents/agent-1.jsonl"),
            "subagent",
        );
        write(
            &source,
            &format!("projects/-Users-me-repo/{CLAUDE_ID}/tool-results/r.txt"),
            "result",
        );
        write(
            &source,
            &format!("file-history/{CLAUDE_ID}/0123@v1"),
            "checkpoint",
        );
        write(&source, "file-history/another/0123@v1", "someone else's");

        let located = locate(Agent::Claude, &source, CLAUDE_ID).expect("found");
        let relatives = companions(Agent::Claude, &source, &located, CLAUDE_ID);
        assert_eq!(
            relatives,
            [
                PathBuf::from(format!("projects/-Users-me-repo/{CLAUDE_ID}")),
                PathBuf::from(format!("file-history/{CLAUDE_ID}")),
            ]
        );
        copy(&source, &target, &located, &relatives).expect("copied");

        for file in [
            record.clone(),
            format!("projects/-Users-me-repo/{CLAUDE_ID}/subagents/agent-1.jsonl"),
            format!("projects/-Users-me-repo/{CLAUDE_ID}/tool-results/r.txt"),
            format!("file-history/{CLAUDE_ID}/0123@v1"),
        ] {
            assert!(target.join(&file).is_file(), "{file} was not carried");
            assert!(source.join(&file).is_file(), "{file} was moved");
        }
        assert!(!target.join("file-history/another").exists());

        // A session with neither has only its transcript, and an id that is no UUID names no path.
        let bare = temp_dir("claude-sidecar-bare");
        write(&bare, &record, "conversation");
        assert!(companions(Agent::Claude, &bare, Path::new(&record), CLAUDE_ID).is_empty());
        assert!(companions(Agent::Claude, &source, Path::new(&record), "..").is_empty());
    }

    /// The items of one switch are replaced and rolled back together: a copy that cannot complete
    /// on any of them leaves the target as it was, and so does a swap that fails part-way, after
    /// the earlier items were already in place.
    #[test]
    fn the_carried_items_complete_or_roll_back_together() {
        let source = temp_dir("together-source");
        let record = format!("projects/slug/{CLAUDE_ID}.jsonl");
        let sidecar = format!("file-history/{CLAUDE_ID}");
        write(&source, &record, "new conversation");
        write(&source, &format!("{sidecar}/a@v1"), "new checkpoint");
        let (record_path, companions) = (PathBuf::from(&record), [PathBuf::from(&sidecar)]);

        // The second item cannot be copied: the first is not left in the target.
        let target = temp_dir("together-target");
        std::os::unix::fs::symlink("/etc/hosts", source.join(&sidecar).join("link")).unwrap();
        let err =
            copy(&source, &target, &record_path, &companions).expect_err("a link in the sidecar");
        assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
        assert_eq!(stage_of(&err), "copy");
        assert_eq!(cause_of(&err), "symlink");
        assert!(!target.join(&record).exists());
        assert!(!target.join(STAGING).exists());
        std::fs::remove_file(source.join(&sidecar).join("link")).unwrap();

        // The second item cannot be put in place: the first is put back as it was.
        let blocked = temp_dir("together-blocked");
        write(&blocked, &record, "older conversation");
        write(&blocked, "file-history", "a file where a directory must be");
        let err = copy(&source, &blocked, &record_path, &companions).expect_err("blocked");
        assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
        assert_eq!(cause_of(&err), "other");
        assert_eq!(
            std::fs::read_to_string(blocked.join(&record)).unwrap(),
            "older conversation"
        );
        assert!(!blocked.join(STAGING).exists());

        // And with nothing in the way both are in place.
        copy(&source, &target, &record_path, &companions).expect("copied");
        assert!(target.join(&record).is_file() && target.join(&sidecar).join("a@v1").is_file());
    }

    /// Switching back finds an older copy in the account it returns to, and the newer record
    /// replaces it whole — a stale file of the older copy does not survive in a directory record.
    #[test]
    fn a_record_already_in_the_target_is_replaced() {
        let source = temp_dir("replace-source");
        let target = temp_dir("replace-target");
        write(&source, "sessions/enc/grok-id/messages.jsonl", "new");
        write(&target, "sessions/enc/grok-id/messages.jsonl", "old");
        write(&target, "sessions/enc/grok-id/stale.json", "stale");

        copy(&source, &target, Path::new("sessions/enc/grok-id"), &[]).expect("copied");

        assert_eq!(
            std::fs::read_to_string(target.join("sessions/enc/grok-id/messages.jsonl")).unwrap(),
            "new"
        );
        assert!(!target.join("sessions/enc/grok-id/stale.json").exists());

        // A file record is replaced by the rename itself, and nothing of the staging stays.
        write(&source, "projects/slug/claude-id.jsonl", "new");
        write(&target, "projects/slug/claude-id.jsonl", "old");
        copy(
            &source,
            &target,
            Path::new("projects/slug/claude-id.jsonl"),
            &[],
        )
        .expect("copied");
        assert_eq!(
            std::fs::read_to_string(target.join("projects/slug/claude-id.jsonl")).unwrap(),
            "new"
        );
        assert!(!target.join(STAGING).exists());
    }

    #[test]
    fn a_session_with_no_record_in_the_account_is_not_found() {
        let source = temp_dir("not-found");
        write(&source, "projects/slug/another.jsonl", "x");

        let err = locate(Agent::Claude, &source, "claude-id").expect_err("nothing there");
        assert_eq!(code_of(&err), error_code::CONVERSATION_NOT_FOUND);
        let err = locate(Agent::Claude, &source.join("missing"), "claude-id").expect_err("no dir");
        assert_eq!(code_of(&err), error_code::CONVERSATION_NOT_FOUND);
        // A link named like the record is not the record.
        std::os::unix::fs::symlink("/etc/hosts", source.join("projects/slug/claude-id.jsonl"))
            .unwrap();
        let err = locate(Agent::Claude, &source, "claude-id").expect_err("a link");
        assert_eq!(code_of(&err), error_code::CONVERSATION_NOT_FOUND);
    }

    /// Two accounts that share one directory have nothing to copy, and copying onto itself must
    /// not delete the record on the way.
    #[test]
    fn a_shared_directory_is_left_alone() {
        let dir = temp_dir("shared");
        write(&dir, "projects/slug/claude-id.jsonl", "conversation");
        let before = inode(&dir, "projects/slug/claude-id.jsonl");

        copy(&dir, &dir, Path::new("projects/slug/claude-id.jsonl"), &[]).expect("nothing to do");

        assert_eq!(inode(&dir, "projects/slug/claude-id.jsonl"), before);
        assert_eq!(
            std::fs::read_to_string(dir.join("projects/slug/claude-id.jsonl")).unwrap(),
            "conversation"
        );
    }

    /// Makes each of `names` in `account` a link to a directory of the same name in `default`, the
    /// way an account shares its conversation directories with the agent's default account.
    fn share(account: &Path, default: &Path, names: &[&str]) {
        for name in names {
            std::fs::create_dir_all(default.join(name)).unwrap();
            std::os::unix::fs::symlink(default.join(name), account.join(name)).unwrap();
        }
    }

    /// What a switch of the session does to the files: locate, collect the companions, copy.
    fn carry(agent: Agent, source: &Path, target: &Path, id: &str) -> PathBuf {
        let record = locate(agent, source, id).expect("found");
        let companions = companions(agent, source, &record, id);
        copy(source, target, &record, &companions).expect("copied");
        record
    }

    /// The inode a path resolves to: a replaced item, even with the same content, has another.
    fn inode(root: &Path, relative: &str) -> u64 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(root.join(relative)).unwrap().ino()
    }

    fn read(root: &Path, relative: &str) -> String {
        std::fs::read_to_string(root.join(relative)).unwrap()
    }

    /// Claude Code accounts that share `projects/` and `file-history/` hold the one conversation
    /// already: a switch copies nothing and the transcript, sidecar directory and checkpoints stay
    /// as they were. Sharing only `projects/` copies the checkpoints alone.
    #[test]
    fn claude_accounts_sharing_their_directories_copy_only_what_is_not_shared() {
        let record = format!("projects/slug/{CLAUDE_ID}.jsonl");
        let sidecar = format!("projects/slug/{CLAUDE_ID}/tool-results/r.txt");
        let checkpoint = format!("file-history/{CLAUDE_ID}/0123@v1");
        for (shared, checkpoints_copied) in [
            (&["projects", "file-history"][..], false),
            (&["projects"][..], true),
        ] {
            let default = temp_dir("claude-shared-default");
            let source = temp_dir("claude-shared-source");
            let target = temp_dir("claude-shared-target");
            share(&source, &default, shared);
            share(&target, &default, shared);
            write(&source, &record, "conversation");
            write(&source, &sidecar, "result");
            write(&source, &checkpoint, "checkpoint");
            let sidecar_dir = format!("projects/slug/{CLAUDE_ID}");
            let checkpoint_dir = format!("file-history/{CLAUDE_ID}");
            let items = [&record, &sidecar_dir, &checkpoint_dir];
            let before = items.map(|item| inode(&source, item));

            carry(Agent::Claude, &source, &target, CLAUDE_ID);

            assert_eq!(items.map(|item| inode(&source, item)), before, "{shared:?}");
            for (file, content) in [
                (&record, "conversation"),
                (&sidecar, "result"),
                (&checkpoint, "checkpoint"),
            ] {
                assert_eq!(read(&source, file), content, "{shared:?} {file}");
            }
            // Only an unshared account ends up with checkpoints in a directory of its own.
            let own_checkpoints = std::fs::symlink_metadata(target.join("file-history"))
                .is_ok_and(|metadata| metadata.is_dir());
            assert_eq!(own_checkpoints, checkpoints_copied, "{shared:?}");
            if checkpoints_copied {
                assert_eq!(read(&target, &checkpoint), "checkpoint");
            }
            assert!(!target.join(STAGING).exists(), "{shared:?} left staging");
        }
    }

    /// A switch from a sharing account to one that does not share puts the conversation in the
    /// target's own real directories; the other way, it is copied through the target's links into
    /// the default account's directories.
    #[test]
    fn a_switch_between_a_sharing_and_a_non_sharing_account_copies_either_way() {
        let record = format!("projects/slug/{CLAUDE_ID}.jsonl");
        let checkpoint = format!("file-history/{CLAUDE_ID}/0123@v1");
        let names = ["projects", "file-history"];

        let default = temp_dir("mixed-default");
        let sharing = temp_dir("mixed-sharing");
        let own = temp_dir("mixed-own");
        share(&sharing, &default, &names);
        write(&sharing, &record, "conversation");
        write(&sharing, &checkpoint, "checkpoint");
        carry(Agent::Claude, &sharing, &own, CLAUDE_ID);
        for file in [&record, &checkpoint] {
            assert!(own.join(file).is_file(), "{file}");
        }

        let default = temp_dir("mixed-default-back");
        let sharing = temp_dir("mixed-sharing-back");
        share(&sharing, &default, &names);
        carry(Agent::Claude, &own, &sharing, CLAUDE_ID);
        for (file, content) in [(&record, "conversation"), (&checkpoint, "checkpoint")] {
            assert_eq!(read(&default, file), content, "{file}");
        }
        assert!(!sharing.join(STAGING).exists());
    }

    /// Codex accounts that share `sessions/` hold the whole chain of rollouts already, compressed
    /// segment included: nothing is copied and every rollout is as it was.
    #[test]
    fn codex_accounts_sharing_their_sessions_copy_nothing() {
        use std::os::unix::fs::MetadataExt;
        let stamp = |file: &PathBuf| {
            (
                std::fs::read(file).unwrap(),
                std::fs::metadata(file).unwrap().ino(),
            )
        };
        let default = temp_dir("codex-shared-default");
        let source = temp_dir("codex-shared-source");
        let target = temp_dir("codex-shared-target");
        share(&source, &default, &["sessions"]);
        share(&target, &default, &["sessions"]);
        let chain = crate::test_support::codex_chain(&source, CODEX_ID, true);
        let before: Vec<(Vec<u8>, u64)> = chain.all().iter().map(stamp).collect();

        carry(Agent::Codex, &source, &target, CODEX_ID);

        let after: Vec<(Vec<u8>, u64)> = chain.all().iter().map(stamp).collect();
        assert_eq!(before, after);
        assert!(!target.join(STAGING).exists());
    }

    /// Grok Build accounts that share `sessions/` copy nothing and the session directory stays.
    #[test]
    fn grok_accounts_sharing_their_sessions_copy_nothing() {
        let default = temp_dir("grok-shared-default");
        let source = temp_dir("grok-shared-source");
        let target = temp_dir("grok-shared-target");
        share(&source, &default, &["sessions"]);
        share(&target, &default, &["sessions"]);
        write(
            &source,
            "sessions/enc/grok-id/messages.jsonl",
            "conversation",
        );

        let before = inode(&source, "sessions/enc/grok-id");

        carry(Agent::Grok, &source, &target, "grok-id");

        assert_eq!(inode(&source, "sessions/enc/grok-id"), before);
        assert_eq!(
            read(&source, "sessions/enc/grok-id/messages.jsonl"),
            "conversation"
        );
        assert!(!target.join(STAGING).exists());
    }

    fn head_of(home: &Path) -> (crate::test_support::CodexChain, PathBuf) {
        let chain = crate::test_support::codex_chain(home, CODEX_ID, true);
        let relative = chain.head.strip_prefix(home).unwrap().to_path_buf();
        (chain, relative)
    }

    fn row_of(database: &Path) -> Option<String> {
        rusqlite::Connection::open(database)
            .unwrap()
            .query_row(
                "SELECT rollout_path FROM threads WHERE id = ?1",
                [CODEX_ID],
                |row| row.get(0),
            )
            .unwrap()
    }

    /// Makes `path` a database whose row of the thread names `rollout` (no path when `None`).
    fn database_naming(path: &Path, rollout: Option<&str>) {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS threads (id TEXT PRIMARY KEY, rollout_path TEXT)",
            )
            .unwrap();
        connection
            .execute(
                "INSERT OR REPLACE INTO threads VALUES (?1, ?2)",
                rusqlite::params![CODEX_ID, rollout],
            )
            .unwrap();
    }

    fn point_head(target: &Path, head: &Path, env: &[(&str, &Path)]) {
        let env = env
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string_lossy().into_owned()))
            .collect();
        repoint_within(
            Agent::Codex,
            target,
            head,
            CODEX_ID,
            &env,
            std::time::Duration::from_millis(50),
        )
        .unwrap();
    }

    fn uncompressed(path: &Path) -> String {
        path.with_extension("").to_string_lossy().into_owned()
    }

    /// A Codex row is rewritten only when the rollout it names is another file than the head:
    /// left alone when it names the head by another route (both accounts link `sessions/` and the
    /// database file into the default's) or names no path, pointed at the head when it names an
    /// older segment of a shared `sessions/`, and when the database is shared but `sessions/` is
    /// not (two homes given one `CODEX_SQLITE_HOME`).
    #[test]
    fn a_row_is_rewritten_only_when_it_names_another_file_than_the_head() {
        // Both link `sessions/` and the database file: the row names the source's head, which is
        // the target's head.
        let default = temp_dir("row-linked-default");
        let (source, target) = (temp_dir("row-linked-source"), temp_dir("row-linked-target"));
        share(&source, &default, &["sessions"]);
        share(&target, &default, &["sessions"]);
        let (chain, head) = head_of(&source);
        let linked = default.join("state_5.sqlite");
        let named = uncompressed(&chain.head);
        database_naming(&linked, Some(&named));
        for home in [&source, &target] {
            std::os::unix::fs::symlink(&linked, home.join("state_5.sqlite")).unwrap();
        }
        point_head(&target, &head, &[]);
        assert_eq!(row_of(&linked), Some(named));

        // A row with no path is left alone.
        database_naming(&linked, None);
        point_head(&target, &head, &[]);
        assert_eq!(row_of(&linked), None);

        // One database, `sessions/` not shared: the row names the source's file, another file.
        let (source, target) = (temp_dir("row-own-source"), temp_dir("row-own-target"));
        let (chain, head) = head_of(&source);
        head_of(&target);
        let shared_database = temp_dir("row-shared-database");
        let database = shared_database.join("state_5.sqlite");
        database_naming(&database, Some(&uncompressed(&chain.head)));
        point_head(&target, &head, &[("CODEX_SQLITE_HOME", &shared_database)]);
        assert_eq!(row_of(&database), Some(uncompressed(&target.join(&head))));

        // `sessions/` shared, the target's own database names an older segment of the chain.
        let default = temp_dir("row-older-default");
        let (source, target) = (temp_dir("row-older-source"), temp_dir("row-older-target"));
        share(&source, &default, &["sessions"]);
        share(&target, &default, &["sessions"]);
        let (chain, head) = head_of(&source);
        let database = target.join("state_5.sqlite");
        database_naming(&database, Some(&uncompressed(&chain.middle)));
        point_head(&target, &head, &[]);
        assert_eq!(row_of(&database), Some(uncompressed(&target.join(&head))));
    }

    /// Of what a home's database directory holds, a `state_<n>.sqlite` that is a link to a file is
    /// a database (an account sharing its database with the default account links it), and a link
    /// to nothing is not; anything else is never looked at, a link that cannot be followed
    /// included.
    #[test]
    fn the_database_scan_follows_a_linked_file_and_ignores_what_is_not_one() {
        // A source of its own, a target linking `sessions/` and the database file: the chain is
        // carried in through the link, and the row, naming an older segment, is pointed at it.
        let default = temp_dir("scan-default");
        let (source, target) = (temp_dir("scan-source"), temp_dir("scan-target"));
        share(&target, &default, &["sessions"]);
        let (chain, _) = head_of(&source);
        let linked = default.join("state_5.sqlite");
        database_naming(&linked, Some(&uncompressed(&chain.middle)));
        std::os::unix::fs::symlink(&linked, target.join("state_5.sqlite")).unwrap();
        let head = carry(Agent::Codex, &source, &target, CODEX_ID);
        // A link that loops, and is no database, does not fail the switch.
        std::os::unix::fs::symlink("loop", target.join("loop")).unwrap();
        point_head(&target, &head, &[]);
        assert_eq!(row_of(&linked), Some(uncompressed(&target.join(&head))));

        // A link to nothing is no database.
        let dangling = temp_dir("scan-dangling");
        std::os::unix::fs::symlink(
            dangling.join("gone.sqlite"),
            dangling.join("state_5.sqlite"),
        )
        .unwrap();
        point_head(&dangling, &head, &[]);
    }

    /// A copy that cannot complete is a failure with nothing half-written left behind, whether the
    /// target cannot be created or the record holds something that is not copied.
    #[test]
    fn an_incomplete_copy_fails_and_leaves_nothing_in_the_target() {
        let source = temp_dir("incomplete-source");
        write(&source, "sessions/enc/grok-id/messages.jsonl", "x");
        std::os::unix::fs::symlink("/etc/hosts", source.join("sessions/enc/grok-id/link")).unwrap();
        let target = temp_dir("incomplete-target");
        let blocked = temp_dir("incomplete-blocked");
        std::fs::write(blocked.join("sessions"), "a file where a directory must be").unwrap();

        for (target, why) in [(&target, "symlink"), (&blocked, "blocked")] {
            let err = copy(&source, target, Path::new("sessions/enc/grok-id"), &[]).expect_err(why);
            assert_eq!(code_of(&err), error_code::RELOCATION_FAILED, "{why}");
        }
        assert!(!target.join("sessions/enc/grok-id").exists());
        for dir in [&target, &blocked] {
            assert!(!dir.join(STAGING).exists(), "staging left in {dir:?}");
        }

        // An older copy already in the target survives a copy that fails, and a staging directory
        // an earlier crash left behind for this record is cleared rather than trusted.
        write(&target, "sessions/enc/grok-id/messages.jsonl", "older");
        write(
            &target,
            ".octoboard-switch/grok-id/leftover",
            "from a crash",
        );
        copy(&source, &target, Path::new("sessions/enc/grok-id"), &[]).expect_err("symlink");
        assert_eq!(
            std::fs::read_to_string(target.join("sessions/enc/grok-id/messages.jsonl")).unwrap(),
            "older"
        );
        assert!(!target.join(STAGING).exists());
    }

    /// Copies into one account at the same moment each keep to their own stage: a copy that starts
    /// while another is part-way, clears nothing of the other's, and both records are in place
    /// afterwards.
    #[test]
    fn copies_into_one_account_do_not_clear_each_others_staging() {
        let source = temp_dir("concurrent-source");
        let target = temp_dir("concurrent-target");
        write(&source, "projects/slug/a.jsonl", "a");
        write(&source, "sessions/enc/b/messages.jsonl", "b");
        write(&target, "sessions/enc/b/messages.jsonl", "older b");

        // Copy A is part-way: its stage exists. Copy B (a directory over an older directory)
        // runs its whole course meanwhile.
        let a_stage = target.join(STAGING).join("a.jsonl");
        std::fs::create_dir_all(&a_stage).unwrap();
        std::fs::write(a_stage.join("partial"), "a in progress").unwrap();

        copy(&source, &target, Path::new("sessions/enc/b"), &[]).expect("b copied");
        assert_eq!(
            std::fs::read_to_string(target.join("sessions/enc/b/messages.jsonl")).unwrap(),
            "b"
        );
        assert!(
            a_stage.join("partial").exists(),
            "A's stage was cleared by B"
        );
        assert!(!target.join(STAGING).join("b").exists());

        // A then completes its own copy over what is left of its stage.
        copy(&source, &target, Path::new("projects/slug/a.jsonl"), &[]).expect("a copied");
        assert_eq!(
            std::fs::read_to_string(target.join("projects/slug/a.jsonl")).unwrap(),
            "a"
        );
        assert!(!target.join(STAGING).exists());
    }

    /// A swap that fails puts the older record back, and a sweep at start removes whatever a
    /// crash left.
    #[test]
    fn a_failed_swap_restores_the_older_record_and_a_sweep_clears_a_crashs_leftovers() {
        let source = temp_dir("aside-source");
        let target = temp_dir("aside-target");
        write(&source, "sessions/enc/b/messages.jsonl", "b");
        write(&target, "sessions/enc/b/messages.jsonl", "older b");
        let to = target.join("sessions/enc/b");
        let staging = target.join(STAGING).join("b");
        std::fs::create_dir_all(&staging).unwrap();
        // The older record is set aside and the rename into place cannot succeed, as `stage`
        // does not exist.
        let items = [(staging.join("b"), &*to, staging.join("previous"))];
        assert!(swap_into_place(&items).is_err());
        assert_eq!(
            std::fs::read_to_string(to.join("messages.jsonl")).unwrap(),
            "older b"
        );

        write(&target, ".octoboard-switch/b/leftover", "from a crash");
        sweep_staging([&*target]);
        assert!(!target.join(STAGING).exists());
    }
    /// Where a Codex home keeps its thread database is told wrongly in a way the user can fix:
    /// the cause says which, so a client can word the remedy.
    #[test]
    fn a_thread_record_failure_names_a_cause_the_user_can_fix() {
        let head = Path::new("sessions/2026/10/03/rollout-2026-10-03T00-00-00-x_y.jsonl");
        let point = |target: &Path, env: &[(&str, &str)]| {
            let env = env
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            repoint_within(
                Agent::Codex,
                target,
                head,
                CODEX_ID,
                &env,
                std::time::Duration::from_millis(50),
            )
        };

        let invalid = temp_dir("cause-invalid");
        write(&invalid, "config.toml", "sqlite_home = [unterminated");
        let err = point(&invalid, &[]).expect_err("an invalid config.toml");
        assert_eq!(
            (stage_of(&err).as_str(), cause_of(&err).as_str()),
            ("thread_record", "config_unreadable")
        );

        let unreadable = temp_dir("cause-config-dir");
        std::fs::create_dir(unreadable.join("config.toml")).unwrap();
        let err = point(&unreadable, &[]).expect_err("a config.toml that is a directory");
        assert_eq!(cause_of(&err), "config_unreadable");

        let relative = temp_dir("cause-relative");
        let err = point(&relative, &[("CODEX_SQLITE_HOME", "state")]).expect_err("relative");
        assert_eq!(cause_of(&err), "sqlite_home");

        let tilde = temp_dir("cause-tilde");
        write(&tilde, "config.toml", "sqlite_home = \"~/db\"");
        let err = point(&tilde, &[]).expect_err("a ~ with no HOME");
        assert_eq!(cause_of(&err), "sqlite_home");

        let broken = temp_dir("cause-broken");
        write(
            &broken,
            "state_5.sqlite",
            "not a database, but long enough to be read as one",
        );
        let err = point(&broken, &[]).expect_err("not a database");
        assert_eq!(cause_of(&err), "other");
    }

    /// A copy the file system refuses names why, when it is one a user can act on.
    #[test]
    fn a_copy_refused_for_permissions_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let source = temp_dir("cause-perm-source");
        write(&source, "sessions/enc/grok-id/messages.jsonl", "x");
        let target = temp_dir("cause-perm-target");
        std::fs::set_permissions(&*target, std::fs::Permissions::from_mode(0o555)).unwrap();
        // A superuser writes anyway, and then there is nothing to refuse.
        if std::fs::create_dir(target.join("probe")).is_ok() {
            std::fs::set_permissions(&*target, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }
        let err =
            copy(&source, &target, Path::new("sessions/enc/grok-id"), &[]).expect_err("read-only");
        std::fs::set_permissions(&*target, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            (stage_of(&err).as_str(), cause_of(&err).as_str()),
            ("copy", "permission_denied")
        );
    }
}
