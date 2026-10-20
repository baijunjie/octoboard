//! Bringing an account's configuration directory in line with its sharing choice, item by item
//! over the agent's catalog (`crate::shared_items`): linking each item into the default account's
//! directory, undoing that, and repairing the links an agent replaced. Every step is safe to run
//! again over a directory a failed run left half done, which is what a retry does, the database
//! and prompt-history work included; nothing is rolled back, and each item that could not be
//! brought in line is reported with the state it was left in.
//!
//! A link always names the default entry's fully resolved target, never the entry itself: when
//! the default directory's own entry is a link (to a dotfiles copy, say), an agent saving through
//! a chain of two links replaces the first one with a plain file, and the copy it led to stops
//! being updated.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::codex_state::{self, Repoint};
use super::moves::{
    append_lines, backup_name, backup_path, is_link, link, link_target, lstat, merge_move,
    move_database, move_path, resolve, resolve_existing, resolve_or_spelled,
};
use super::registries;
use super::take_back::{take_back, Conversation};
use crate::protocol::Agent;
use crate::relocate::is_state_database;
use crate::shared_items::{shared_items, ItemClass, ItemKind, SharedItem};

/// The two directories an operation works between.
pub(super) struct Dirs {
    /// The account's directory as the account records it, which is the spelling the agent runs
    /// under and so writes into the paths it records.
    pub account: PathBuf,
    /// The same directory with its links resolved, the other spelling a recorded path can have.
    pub account_real: PathBuf,
    /// The default account's directory, fully resolved where it exists.
    pub default: PathBuf,
    /// When turning sharing off: the agent's current default directory, if it is not `default`,
    /// the one the links were made into. A link into either is one sharing put there.
    pub current_default: Option<PathBuf>,
}

/// Why [`Dirs`] could not be set up.
#[derive(Debug)]
pub(super) enum DirsError {
    /// The account's directory is the default account's own, under another spelling: linking
    /// would make every item a link to itself.
    SameAsDefault,
    Io(io::Error),
}

impl From<io::Error> for DirsError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl Dirs {
    /// For turning sharing on and repairing it, into `default`, which has to exist. `create`
    /// makes the account's directory when it does not exist yet, as turning sharing on does for
    /// an account that has never run; otherwise a missing account directory is `None`, there
    /// being nothing in it to bring in line.
    pub fn new(account: &Path, default: &Path, create: bool) -> Result<Option<Self>, DirsError> {
        let default = std::fs::canonicalize(default).map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("the default directory `{}`: {err}", default.display()),
            )
        })?;
        if create {
            std::fs::create_dir_all(account)?;
        }
        Self::over(account, default)
    }

    /// For turning sharing off, out of `default`, the directory the links were made into, which
    /// may be gone by now: its links are still recognised by where they lead. `current` is the
    /// agent's default directory now.
    pub fn for_undoing(
        account: &Path,
        default: &Path,
        current: &Path,
    ) -> Result<Option<Self>, DirsError> {
        let default = resolve_or_spelled(&crate::hostfs::lexically_normalise(default));
        let current = resolve_or_spelled(&crate::hostfs::lexically_normalise(current));
        let mut dirs = Self::over(account, default)?;
        if let Some(dirs) = dirs.as_mut() {
            dirs.current_default = (current != dirs.default).then_some(current);
        }
        Ok(dirs)
    }

    fn over(account: &Path, default: PathBuf) -> Result<Option<Self>, DirsError> {
        let Some(account_real) = resolve(account)? else {
            return Ok(None);
        };
        if account_real == default {
            return Err(DirsError::SameAsDefault);
        }
        Ok(Some(Self {
            account: account.to_path_buf(),
            account_real,
            default,
            current_default: None,
        }))
    }
}

/// What an item was left as by an operation that could not finish it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Left {
    /// As it was before the operation.
    Unchanged,
    /// Renamed to its backup, with nothing linked in its place.
    BackedUp,
    /// Some of its conversation history moved into the default directory, the rest still in the
    /// account's own directory.
    PartlyMoved,
    /// Still a link into the default directory.
    Linked,
    /// Not there at all.
    Absent,
}

/// An item an operation could not bring in line.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Failure {
    /// An entry of the account's directory, or for a conversation the agent's own id of it.
    pub item: String,
    pub left: Left,
    pub detail: String,
    /// The Octoboard sessions a conversation belongs to, for a failure about one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<String>,
}

impl Failure {
    fn new(item: &SharedItem, left: Left, err: impl std::fmt::Display) -> Self {
        Self {
            item: item.name.to_string(),
            left,
            detail: err.to_string(),
            sessions: Vec::new(),
        }
    }
}

/// The backup turning sharing on made of `item` under `stamp`, the one turning it off restores.
fn turn_on_backup(dirs: &Dirs, item: &SharedItem, stamp: &str) -> PathBuf {
    backup_name(&dirs.account.join(item.name), stamp)
}

/// Whether the link `entry` is one sharing put there at some point: one leading into the default
/// directory (the one the links were made into, or the agent's current one), or to where a
/// default entry leads now.
fn is_sharing_link(entry: &Path, item: &SharedItem, dirs: &Dirs) -> bool {
    let Some(leads) = link_target(entry) else {
        return false;
    };
    let resolved = resolve(entry).ok().flatten();
    std::iter::once(&dirs.default)
        .chain(dirs.current_default.as_ref())
        .any(|default| {
            leads.starts_with(default)
                || resolved.is_some()
                    && resolve(&default.join(item.name)).ok().flatten() == resolved
        })
}

/// Prompt histories and Codex's thread-name index: one line per entry, merged into the default
/// directory's copy rather than set aside, so the recall and the names of the account's earlier
/// conversations survive. A name that is lost makes Codex start a new session where a resume by
/// that name was asked for.
fn is_line_history(name: &str) -> bool {
    matches!(name, "history.jsonl" | "session_index.jsonl")
}

/// Merges the account's `entry` into `into`, the default's copy: the account's file is first set
/// aside under the operation's exact backup name, its lines appended from there and the aside
/// removed. An aside left by a run that stopped part-way is merged first, and lines already
/// appended are not appended again.
fn merge_lines(entry: &Path, into: &Path, stamp: &str) -> io::Result<()> {
    let aside = backup_name(entry, stamp);
    if lstat(&aside)?.is_some() {
        append_lines(&aside, into)?;
        std::fs::remove_file(&aside)?;
    }
    if lstat(entry)?.is_some_and(|metadata| metadata.is_file()) {
        std::fs::rename(entry, &aside)?;
        append_lines(&aside, into)?;
        std::fs::remove_file(&aside)?;
    }
    Ok(())
}

/// How a Codex database's rows name rollouts through the account's directory, by either of its
/// spellings, and where those rollouts are once shared.
fn codex_repoints(dirs: &Dirs) -> Vec<Repoint> {
    let mut repoints = Vec::new();
    for sub in ["sessions", "archived_sessions"] {
        let to = resolve_or_spelled(&dirs.default.join(sub));
        for from in [dirs.account.join(sub), dirs.account_real.join(sub)] {
            if repoints.iter().all(|seen: &Repoint| seen.from != from) {
                repoints.push(Repoint {
                    from,
                    to: to.clone(),
                });
            }
        }
    }
    repoints
}

/// The work an item that already links where it should may still owe: the lines of a prompt
/// history a stopped run set aside, and, on a retry of turning sharing on that has not completed
/// once yet (`retry_database`), the rows of a Codex state database, carried over from the
/// account's own one set aside under `stamp` or, when the database itself was moved, repointed.
/// Both are safe to repeat until turning sharing on completes; after that, the rows carried may
/// have been taken back or forgotten, and are not carried again.
fn finish_linked(
    item: &SharedItem,
    dirs: &Dirs,
    shared: &Path,
    stamp: &str,
    retry_database: bool,
) -> Result<(), Failure> {
    let entry = dirs.account.join(item.name);
    if is_line_history(item.name) {
        return merge_lines(&entry, shared, stamp)
            .map_err(|err| Failure::new(item, Left::Linked, err));
    }
    if !(retry_database && is_state_database(item.name)) {
        return Ok(());
    }
    let aside = turn_on_backup(dirs, item, stamp);
    let carried = if std::fs::symlink_metadata(&aside).is_ok() {
        codex_state::bring_rows(shared, &aside, &codex_repoints(dirs)).map(|_| ())
    } else {
        codex_state::repoint_rows(shared, &codex_repoints(dirs))
    };
    carried.map_err(|err| Failure::new(item, Left::Linked, format!("rows not carried over: {err}")))
}

/// Brings `item` under the default directory: the item as the account holds it is backed up
/// (configuration) or carried into the default directory (conversation history), and replaced by
/// a link to the default entry's resolved target. A directory the default directory lacks is
/// created there, empty; a configuration file it lacks is skipped, and so is a history file
/// neither directory has. `stamp` names the backups.
fn bring_in(
    item: &SharedItem,
    dirs: &Dirs,
    stamp: &str,
    retry_database: bool,
) -> Result<(), Failure> {
    let entry = dirs.account.join(item.name);
    let default_entry = dirs.default.join(item.name);
    let unchanged = |err: io::Error| Failure::new(item, Left::Unchanged, err);
    let present = lstat(&entry).map_err(unchanged)?;
    let target = resolve(&default_entry).map_err(unchanged)?;
    if let Some(target) = &target {
        if is_link(&present) && link_target(&entry).as_ref() == Some(target) {
            return finish_linked(item, dirs, target, stamp, retry_database);
        }
        // The default directory's entry leading back into the account's own: moving or backing
        // it up would take the very thing the link is to lead to.
        if !is_link(&present)
            && present.is_some()
            && resolve(&entry).map_err(unchanged)?.as_ref() == Some(target)
        {
            return Err(unchanged(io::Error::other(format!(
                "`{}` leads into the account's own directory",
                default_entry.display()
            ))));
        }
    }
    let history = item.class == ItemClass::ConversationHistory;
    let sharing_link = is_link(&present) && is_sharing_link(&entry, item, dirs);
    if history && is_link(&present) && !sharing_link {
        // Conversation history the account keeps elsewhere through a link of its own: what it
        // leads to is not the account's to move, and setting the link aside would hide it.
        return Err(unchanged(io::Error::other(format!(
            "`{}` is a link of the account's own",
            entry.display()
        ))));
    }
    let holds_own = present.is_some() && !is_link(&present);
    if target.is_none() && item.kind == ItemKind::File && !(history && holds_own) {
        return Ok(());
    }
    // From here the account's own entry is what is in the way.
    let mut left = Left::Unchanged;
    if sharing_link {
        std::fs::remove_file(&entry).map_err(unchanged)?;
        left = Left::Absent;
    } else if is_link(&present) {
        std::fs::rename(&entry, backup_path(&entry, stamp)).map_err(unchanged)?;
        left = Left::BackedUp;
    }
    let failed = |left: Left, err: io::Error| Failure::new(item, left, err);
    if holds_own {
        let is_dir = present.as_ref().is_some_and(|metadata| metadata.is_dir());
        match (item.class, item.kind, &target) {
            (ItemClass::ConversationHistory, ItemKind::Directory, None) if is_dir => {
                move_path(&entry, &default_entry).map_err(|err| failed(left, err))?;
                left = Left::Absent;
            }
            (ItemClass::ConversationHistory, ItemKind::Directory, Some(into)) if is_dir => {
                if !into.is_dir() {
                    return Err(failed(
                        left,
                        io::Error::other(format!(
                            "`{}` is not a directory",
                            default_entry.display()
                        )),
                    ));
                }
                merge_move(&entry, into, &backup_path(&entry, stamp))
                    .map_err(|err| failed(Left::PartlyMoved, err))?;
                left = Left::Absent;
            }
            (ItemClass::ConversationHistory, ItemKind::File, None) if !is_dir => {
                let database = is_state_database(item.name);
                let moved = if database {
                    move_database(&entry, &default_entry)
                } else {
                    move_path(&entry, &default_entry)
                };
                moved.map_err(|err| failed(left, err))?;
                let shared =
                    resolve_existing(&default_entry).map_err(|err| failed(Left::Absent, err))?;
                link(&entry, &shared).map_err(|err| failed(Left::Absent, err))?;
                if database {
                    // The rows name the rollouts by the account's own spelling, and the
                    // rollouts are in the default directory now.
                    return codex_state::repoint_rows(&shared, &codex_repoints(dirs)).map_err(
                        |err| {
                            Failure::new(item, Left::Linked, format!("rows not repointed: {err}"))
                        },
                    );
                }
                return Ok(());
            }
            (ItemClass::ConversationHistory, ItemKind::File, Some(into))
                if !is_dir && is_line_history(item.name) =>
            {
                merge_lines(&entry, into, stamp).map_err(|err| failed(left, err))?;
                left = Left::Absent;
            }
            (ItemClass::ConversationHistory, ItemKind::File, Some(shared))
                if !is_dir && is_state_database(item.name) =>
            {
                let aside = backup_path(&entry, stamp);
                move_database(&entry, &aside).map_err(|err| failed(left, err))?;
                link(&entry, shared).map_err(|err| failed(Left::BackedUp, err))?;
                return codex_state::bring_rows(shared, &aside, &codex_repoints(dirs))
                    .map(|_| ())
                    .map_err(|err| {
                        Failure::new(item, Left::Linked, format!("rows not carried over: {err}"))
                    });
            }
            _ => {
                std::fs::rename(&entry, backup_path(&entry, stamp))
                    .map_err(|err| failed(left, err))?;
                left = Left::BackedUp;
            }
        }
    }
    let target = match target {
        Some(target) => target,
        None => {
            // A directory item the default directory lacks: created there, empty, so whatever
            // an account installs later lands where every sharing account sees it.
            std::fs::create_dir_all(&default_entry).map_err(|err| failed(left, err))?;
            resolve_existing(&default_entry).map_err(|err| failed(left, err))?
        }
    };
    link(&entry, &target).map_err(|err| failed(left, err))?;
    finish_linked(item, dirs, &target, stamp, retry_database)
}

/// Shares every item of `agent`'s catalog the default directory can take, backing up what the
/// account had under `stamp`. A retry before turning sharing on has `completed` once also redoes
/// the database work an earlier attempt may have left undone.
pub(super) fn turn_on(agent: Agent, dirs: &Dirs, stamp: &str, completed: bool) -> Vec<Failure> {
    shared_items(agent)
        .iter()
        .filter_map(|item| bring_in(item, dirs, stamp, !completed).err())
        .collect()
}

/// Undoes [`turn_on`]: removes each link sharing put there and restores the configuration
/// backup made under `stamp`, the one turning sharing on recorded, if there is one (an item
/// without one is left absent). The account gets empty conversation directories of its own, and
/// `conversations`, those of the account's own sessions, are taken back into them
/// ([`take_back`]), so the sessions stay resumable under the account; the rest of the
/// conversation history stays in the default directory. An entry that is not a link sharing put
/// there is the account's own and is left alone.
///
/// Before anything is unlinked, what the default directory records by the account's spelling is
/// pointed at the default directory's own: Codex's thread rows, which would otherwise name
/// rollouts through an empty directory for every account still sharing, and the plugin
/// registries. An item whose records could not be repointed stays linked, so nothing it holds
/// is left unreachable.
pub(super) fn turn_off(
    agent: Agent,
    dirs: &Dirs,
    stamp: &str,
    conversations: &[Conversation],
) -> Vec<Failure> {
    let items = shared_items(agent);
    let mut failures = Vec::new();
    let mut kept: Vec<&str> = Vec::new();
    for (name, file) in registries::registries(agent) {
        let to = resolve_or_spelled(&dirs.default.join(name));
        let from = [dirs.account.join(name), dirs.account_real.join(name)];
        if let Err(err) = registries::repoint(&dirs.default.join(name).join(file), &from, &to) {
            if let Some(item) = items.iter().find(|item| item.name == *name) {
                failures.push(Failure::new(
                    item,
                    Left::Linked,
                    format!("`{file}` not repointed: {err}"),
                ));
            }
            kept.push(name);
        }
    }
    for item in items.iter().filter(|item| is_state_database(item.name)) {
        let repointed = resolve(&dirs.default.join(item.name)).and_then(|database| {
            database.map_or(Ok(()), |database| {
                codex_state::repoint_rows(&database, &codex_repoints(dirs))
                    .map_err(io::Error::other)
            })
        });
        if let Err(err) = repointed {
            failures.push(Failure::new(
                item,
                Left::Linked,
                format!("rows not repointed: {err}"),
            ));
            kept.extend([item.name, "sessions", "archived_sessions"]);
        }
    }
    for item in items.iter().filter(|item| !kept.contains(&item.name)) {
        if let Err(failure) = take_out(item, dirs, stamp) {
            failures.push(failure);
        }
    }
    // Each directory is listed once for all the conversations taken back.
    let mut locator = crate::agent_records::Locator::default();
    for conversation in conversations {
        if let Err(failure) = take_back(&mut locator, agent, dirs, conversation, stamp) {
            failures.push(failure);
        }
    }
    failures
}

fn take_out(item: &SharedItem, dirs: &Dirs, stamp: &str) -> Result<(), Failure> {
    let entry = dirs.account.join(item.name);
    let backup = turn_on_backup(dirs, item, stamp);
    let has_backup = std::fs::symlink_metadata(&backup).is_ok();
    let present = lstat(&entry).map_err(|err| Failure::new(item, Left::Linked, err))?;
    match &present {
        Some(_)
            if is_link(&present)
                && ((has_backup && item.class == ItemClass::Configuration)
                    || is_sharing_link(&entry, item, dirs)) =>
        {
            std::fs::remove_file(&entry).map_err(|err| Failure::new(item, Left::Linked, err))?;
        }
        // The account's own entry, or a link of the user's.
        Some(_) => return Ok(()),
        // Left absent by a run that failed after unlinking it.
        None if has_backup && item.class == ItemClass::Configuration => {}
        None => return Ok(()),
    }
    let absent = |err: io::Error| Failure::new(item, Left::Absent, err);
    match (item.class, item.kind) {
        (ItemClass::Configuration, _) if has_backup => {
            std::fs::rename(backup, &entry).map_err(absent)?;
        }
        (ItemClass::ConversationHistory, ItemKind::Directory) => {
            std::fs::create_dir(&entry).map_err(absent)?;
        }
        _ => {}
    }
    Ok(())
}

/// What a repair found that it could not, or must not, fix.
pub(super) struct Repaired {
    pub failures: Vec<Failure>,
    /// Codex state databases in the account's directory that the catalog does not name: a Codex
    /// that moved to a new schema generation creates one of its own in every account, and from
    /// then on the shared database is no longer used.
    pub unshared_databases: Vec<PathBuf>,
}

/// Before a sharing account's session starts: every file item that is no longer a link to the
/// default entry's resolved target is brought in again (an agent saving a linked file may have
/// replaced the link with a plain file), and so is a directory item that is missing or links
/// elsewhere. A directory the account holds as its own is left alone. Backups are named with
/// `stamp`, the one turning sharing on used, a later one getting a numbered name beside it.
pub(super) fn repair(agent: Agent, dirs: &Dirs, stamp: &str) -> Repaired {
    let mut failures = Vec::new();
    for item in shared_items(agent) {
        let entry = dirs.account.join(item.name);
        let present = match lstat(&entry) {
            Ok(present) => present,
            Err(err) => {
                failures.push(Failure::new(item, Left::Unchanged, err));
                continue;
            }
        };
        let needs = match (&present, item.kind) {
            (Some(_), _) if is_link(&present) => match resolve(&dirs.default.join(item.name)) {
                Ok(Some(target)) => link_target(&entry) != Some(target),
                // A link to an entry the default directory no longer has: an agent saving
                // through it creates the entry there.
                Ok(None) => item.kind == ItemKind::Directory,
                Err(_) => false,
            },
            (Some(_), ItemKind::File) => true,
            (Some(_), ItemKind::Directory) => false,
            (None, _) => true,
        };
        if needs {
            if let Err(failure) = bring_in(item, dirs, stamp, false) {
                failures.push(failure);
            }
        }
    }
    let unshared_databases = if agent == Agent::Codex {
        unshared_databases(dirs)
    } else {
        Vec::new()
    };
    Repaired {
        failures,
        unshared_databases,
    }
}

fn unshared_databases(dirs: &Dirs) -> Vec<PathBuf> {
    let catalogued: Vec<&str> = shared_items(Agent::Codex)
        .iter()
        .map(|item| item.name)
        .filter(|name| is_state_database(name))
        .collect();
    let Ok(entries) = std::fs::read_dir(&dirs.account) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            is_state_database(&name)
                && !catalogued.contains(&name.as_ref())
                && entry.file_type().is_ok_and(|kind| kind.is_file())
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}
