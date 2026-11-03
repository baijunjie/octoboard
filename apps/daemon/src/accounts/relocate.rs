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
//! keeps beside it travels too (its companions, [`crate::records::carried`]): Claude Code's
//! directory of the same name (subagent transcripts, tool results) and its rewind checkpoints. For
//! Codex the record alone is not sufficient: a thread that had a turn reverted is a chain of
//! rollouts, the record is the chain's head, and the first rollout and the segments between are its
//! companions. Nothing else is touched in either directory: the copy only ever adds (or replaces)
//! those items of the one session, besides a transient staging directory at the target's root that
//! it creates and removes again, and, for a provider that keeps a thread registry (Codex's
//! database), the one row it keeps of the thread ([`repoint_thread_row`]). It copies rather than
//! moves, so a switch that fails afterwards leaves the original where it was and switching back
//! needs no second copy. The measurements are in `docs/agent-cli-reference.md`.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::accounts::thread_registry::{thread_registry, RowCause, RowError, ThreadRegistry};
use crate::hostfs::{is_same_path, remove_any};
use crate::protocol::{error_code, AgentId, CodedError, SharingCause};
use crate::records::carried::locate_record;
use crate::records::{plausible_id, Locator};

/// The path, relative to `source`, of the session's record in the account whose directory is
/// `source` ([`locate_record`]). Refused with `conversation_not_found` when there is none, which
/// the caller meets before it has ended anything.
pub fn locate(agent: AgentId, source: &Path, agent_session_id: &str) -> Result<PathBuf> {
    locate_with(&mut Locator::default(), agent, source, agent_session_id)
}

/// [`locate`], listing each records directory through `locator`, which keeps what it listed for
/// later lookups: one directory several accounts share is listed once for all of them.
pub fn locate_with(
    locator: &mut Locator,
    agent: AgentId,
    source: &Path,
    agent_session_id: &str,
) -> Result<PathBuf> {
    locate_record(locator, agent, source, agent_session_id).ok_or_else(|| {
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

/// Points the account's own record of `thread` in its provider's thread registry at the record the
/// switch just carried in (`head`, relative to `target`), for a provider that keeps one: the one
/// write a switch makes to an agent's own state beside the copy. What is pointed and when is the
/// provider's [`ThreadRegistry::point_thread_at`]'s.
///
/// `launch_env` is the environment the relaunch will see, which can say where the account keeps
/// its registry. A switch fails (`relocation_failed`, before anything is relaunched) when where the
/// registry is cannot be determined, or it cannot be written to or stays busy: a row left standing
/// would resume the stale branch with no sign of it.
pub fn repoint_thread_row(
    agent: AgentId,
    target: &Path,
    head: &Path,
    thread: &str,
    launch_env: &HashMap<String, String>,
) -> Result<()> {
    let Some(registry) = thread_registry(agent) else {
        return Ok(());
    };
    repoint_within(
        registry,
        agent,
        target,
        head,
        thread,
        launch_env,
        registry.patience(),
    )
}

/// Why a conversation could not be moved, in the words of the wire's `cause` param on
/// `relocation_failed`: the file system's own causes ([`SharingCause`]), which a user can act on,
/// and the ones of this move alone — `config_unreadable` (the provider's configuration that says
/// where its thread registry is cannot be read or understood), `sqlite_home` (where that registry
/// is cannot be told; the word names Codex's setting, the first provider to have such a
/// registry), `symlink` (a record holds a symbolic link, which is not copied) and `not_restored`
/// (an older item could not be put back). `other` is everything else. Not found and conflicts are
/// told only as `other`.
fn cause_of_io(err: &io::Error) -> &'static str {
    narrowed(SharingCause::of(err))
}

/// The wire word for `cause` where this move tells it apart, else `other`.
fn narrowed(cause: SharingCause) -> &'static str {
    match cause {
        SharingCause::PermissionDenied | SharingCause::Busy | SharingCause::NoSpace => {
            cause.as_str()
        }
        _ => "other",
    }
}

/// The wire word for why the account's row of the thread was not pointed (see [`cause_of_io`]).
fn cause_of_row(err: &RowError) -> &'static str {
    match err.cause {
        RowCause::Configuration(cause) => match narrowed(cause) {
            "other" => "config_unreadable",
            cause => cause,
        },
        RowCause::Unlocatable => "sqlite_home",
        RowCause::Registry(cause) => narrowed(cause),
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
    registry: &dyn ThreadRegistry,
    agent: AgentId,
    target: &Path,
    head: &Path,
    thread: &str,
    launch_env: &HashMap<String, String>,
    patience: std::time::Duration,
) -> Result<()> {
    if !plausible_id(thread) {
        return Ok(());
    }
    registry
        .point_thread_at(target, &target.join(head), thread, launch_env, patience)
        .map_err(|err| {
            let detail = err.to_string();
            CodedError::raised(
                error_code::RELOCATION_FAILED,
                format!(
                    "the conversation was copied but {}'s record of the thread was not \
                     updated: {detail}",
                    agent.label()
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

/// Copies the `record` and its `companions` (see [`crate::records::carried`]) from `source` to the same
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
