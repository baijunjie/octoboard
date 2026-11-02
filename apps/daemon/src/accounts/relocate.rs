//! Copying one session's conversation record from one account's config directory into another's,
//! which is what lets a switch of the session's account resume the conversation instead of
//! starting a new one: every agent keeps a session's record inside its config directory, so a
//! resume under another directory does not find it.
//!
//! The record is found where the agent writes its records, by the rules `crate::records` finds a
//! session's records by, and copied to the same path *relative to the config directory* in the
//! target, creating the directories in between. Nothing here derives Claude Code's project slug or
//! Grok's encoding of a working directory: the source directory already holds the path the agent
//! wrote, and a second implementation of that rule would be a guess at something the agent already
//! decided. For Grok the directories are the account's own (the source home a per-session home is
//! built from), never a per-session home, which is discarded with its process.
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
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::accounts::codex_threads::{self, RowError};
use crate::accounts::files::{is_same_path, modified, remove_any};
use crate::accounts::shared_config::FailureCause as Cause;
use crate::agents::layout::layout;
use crate::protocol::{error_code, Agent, CodedError};
use crate::records::{conversation_files_in, plausible_id, rollout_head, Locator};

/// The path, relative to `source`, of the session's record in the account whose directory is
/// `source`: for Codex the head of the thread's chain of rollouts, otherwise the copy written to
/// last when the agent keeps more than one. Refused with `conversation_not_found` when there is
/// none, which the caller meets before it has ended anything.
pub fn locate(agent: Agent, source: &Path, agent_session_id: &str) -> Result<PathBuf> {
    locate_with(&mut Locator::default(), agent, source, agent_session_id)
}

/// [`locate`], listing each records directory through `locator`, which keeps what it listed for
/// later lookups: one directory several accounts share is listed once for all of them.
pub fn locate_with(
    locator: &mut Locator,
    agent: Agent,
    source: &Path,
    agent_session_id: &str,
) -> Result<PathBuf> {
    let records = records_of(locator, agent, source, agent_session_id);
    let record = if agent == Agent::Codex {
        let absolute: Vec<PathBuf> = records
            .iter()
            .map(|relative| source.join(relative))
            .collect();
        rollout_head(&absolute)
            .and_then(|head| head.strip_prefix(source).ok().map(Path::to_path_buf))
    } else {
        // The record is a file for Claude Code (its sidecar directory sits beside it) and a
        // directory for Grok Build. More than one is not expected; if it happens the one written
        // to last is the live one.
        records
            .into_iter()
            .filter(|relative| {
                std::fs::symlink_metadata(source.join(relative)).is_ok_and(|metadata| {
                    if agent == Agent::Claude {
                        metadata.is_file()
                    } else {
                        metadata.is_dir()
                    }
                })
            })
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

/// What the agent keeps for the session besides its record, as paths relative to `source` that
/// exist there, for a switch to carry with it. Claude Code: the directory of the record's own
/// name beside it (subagent transcripts, tool results, the custom title) and `file-history/<id>`
/// (rewind checkpoints). Codex: the other rollouts of the thread's chain, which with `record` (the
/// head) are the whole conversation: a target given the head alone is refused, and one given the
/// first rollout alone resumes a stale branch. A subagent's rollout is named by its own thread id
/// and stays behind; Grok keeps everything inside the session directory.
pub fn companions(
    agent: Agent,
    source: &Path,
    record: &Path,
    agent_session_id: &str,
) -> Vec<PathBuf> {
    let files = conversation_of(&mut Locator::default(), agent, source, agent_session_id);
    let records_dir = Path::new(layout(agent).records_dir);
    match agent {
        Agent::Claude => {
            let sidecar = record.with_extension("");
            files
                .into_iter()
                .filter(|relative| *relative == sidecar || !relative.starts_with(records_dir))
                .collect()
        }
        Agent::Codex => files
            .into_iter()
            .filter(|relative| relative.starts_with(records_dir) && relative != record)
            .collect(),
        Agent::Grok => Vec::new(),
    }
}

/// Every file the agent keeps of the conversation in `source` that a switch may carry, relative to
/// `source`: what `crate::records` finds of it, less the rollouts `codex archive` moved aside,
/// which a resume does not read. An id that is not a canonical UUID has none.
fn conversation_of(
    locator: &mut Locator,
    agent: Agent,
    source: &Path,
    agent_session_id: &str,
) -> Vec<PathBuf> {
    let archived = layout(agent).archived_records_dir.map(Path::new);
    conversation_files_in(locator, agent, source, agent_session_id)
        .into_iter()
        .filter_map(|path| path.strip_prefix(source).ok().map(Path::to_path_buf))
        .filter(|relative| !archived.is_some_and(|archived| relative.starts_with(archived)))
        .collect()
}

/// The records of the session in `source` and what sits beside them in the records directory, as
/// paths relative to `source`.
fn records_of(
    locator: &mut Locator,
    agent: Agent,
    source: &Path,
    agent_session_id: &str,
) -> Vec<PathBuf> {
    let records_dir = Path::new(layout(agent).records_dir);
    conversation_of(locator, agent, source, agent_session_id)
        .into_iter()
        .filter(|relative| relative.starts_with(records_dir))
        .collect()
}

/// Points a Codex home's own record of `thread` at the rollout the switch just carried in
/// (`head`, relative to `target`), which is the one write a switch makes to an agent's own state
/// beside the copy; what is pointed and when is [`codex_threads::point_thread_at`]'s.
///
/// `launch_env` is the environment the relaunch will see, which with the home's `config.toml` says
/// where the home keeps its database. A switch fails (`relocation_failed`, before anything is
/// relaunched) when where the database is cannot be determined (a `config.toml` that cannot be
/// read or parsed, a relative `CODEX_SQLITE_HOME`, a directory that cannot be listed) or it cannot
/// be written to or stays busy: a row left standing would resume the stale branch with no sign of
/// it.
pub fn repoint_thread_row(
    agent: Agent,
    target: &Path,
    head: &Path,
    thread: &str,
    launch_env: &HashMap<String, String>,
) -> Result<()> {
    repoint_within(
        agent,
        target,
        head,
        thread,
        launch_env,
        codex_threads::PATIENCE,
    )
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

/// The wire word for why a Codex home's row of the thread was not pointed (see [`cause_of_io`]).
fn cause_of_row(err: &RowError) -> &'static str {
    match err {
        RowError::ConfigUnreadable(err) => match cause_of_io(err) {
            "other" => "config_unreadable",
            cause => cause,
        },
        RowError::ConfigInvalid(_) => "config_unreadable",
        RowError::Unresolvable(_) => "sqlite_home",
        RowError::Listing(_, err) | RowError::Inspecting(_, err) => cause_of_io(err),
        RowError::Database(_, err) => narrowed(Cause::of_database(err)),
    }
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
    codex_threads::point_thread_at(target, &target.join(head), thread, launch_env, patience)
        .map_err(|err| {
            let detail = err.to_string();
            CodedError::raised(
                error_code::RELOCATION_FAILED,
                format!(
                    "the conversation was copied but Codex's record of the thread was not \
                     updated: {detail}"
                ),
                &[
                    ("path", &target.to_string_lossy()),
                    ("stage", "thread_record"),
                    ("cause", cause_of_row(&err)),
                    ("detail", &detail),
                ],
            )
        })
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
        .filter(|(from, to)| !is_same_path(from, to))
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
mod tests;
