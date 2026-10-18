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
//! The record alone is sufficient for all three agents to resume — no index, registry or
//! per-project entry is copied with it — but it is not the whole conversation, so what the agent
//! keeps beside it travels too ([`companions`]): Claude Code's directory of the same name (subagent
//! transcripts, tool results) and its rewind checkpoints. Nothing else is touched in either
//! directory: the copy only ever adds (or replaces) those items of the one session, besides a
//! transient staging directory at the target's root that it creates and removes again. It copies
//! rather than moves, so a switch that fails afterwards leaves the original where it was and
//! switching back needs no second copy. The measurements are in
//! `docs/agent-cli-reference.md`.

use std::collections::BTreeMap;
use std::fs::FileType;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;

use crate::agent_records::{plausible_id, rollout_stem};
use crate::protocol::{error_code, Agent, CodedError};

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
        // once Codex has compressed it
        Agent::Codex => Layout {
            root: "sessions",
            depth: 6,
            is_record: |name, kind, id| {
                kind.is_file()
                    && name.starts_with("rollout-")
                    && rollout_stem(name).is_some_and(|stem| stem.ends_with(&format!("-{id}")))
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
/// `source`. Refused with `conversation_not_found` when there is none, which the caller meets
/// before it has ended anything. Compared by entry name only, never joined into a path, so an id
/// the agent reported cannot steer the search out of the directory.
pub fn locate(agent: Agent, source: &Path, agent_session_id: &str) -> Result<PathBuf> {
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
    // More than one is not expected; if it happens the one written to last is the live one.
    found
        .into_iter()
        .max_by_key(|relative| modified(&source.join(relative)))
        .ok_or_else(|| {
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
/// exist there, for a switch to carry with it. Claude Code only: the directory of the record's own
/// name beside it (subagent transcripts, tool results, the custom title) and `file-history/<id>`
/// (rewind checkpoints). An id that is no canonical UUID yields nothing, as it is joined into a
/// path here.
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
        // Codex's subagent rollouts (`rollout-<timestamp>-<thread id>_<child id>`) stay behind:
        // a home with no database row for the thread resumes the newest rollout whose name
        // contains the id, which with those present is a subagent's, not the thread's (measured
        // with 0.162.1). Grok keeps everything inside the session directory.
        Agent::Codex | Agent::Grok => Vec::new(),
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

fn modified(path: &Path) -> SystemTime {
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
            &[("path", &target.to_string_lossy()), ("detail", "no name")],
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
            CodedError::raised(
                error_code::RELOCATION_FAILED,
                format!(
                    "copying the conversation into `{}` did not complete: {detail}",
                    target.display()
                ),
                &[("path", &target.to_string_lossy()), ("detail", &detail)],
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

fn remove_any(path: &Path) -> io::Result<()> {
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
        return Err(io::Error::other(format!(
            "`{}` is a symbolic link",
            from.display()
        )));
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

    fn code_of(err: &anyhow::Error) -> &'static str {
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

    /// A Codex rollout Codex has compressed is located like a plain one and copied as it is (the
    /// bytes stay compressed). Its subagent rollouts, plain or compressed, stay behind: a home
    /// with no database row for the thread resumes the newest rollout whose name contains the id
    /// (measured with 0.162.1), which with them present is a subagent's, not the thread's.
    #[test]
    fn a_compressed_codex_rollout_is_carried_alone() {
        let source = temp_dir("codex-source");
        let target = temp_dir("codex-target");
        let record =
            format!("sessions/2026/10/01/rollout-2026-10-01T01-02-03-{CODEX_ID}.jsonl.zst");
        let child = "019a0000-0000-7000-8000-00000000000a";
        let subagents = [
            format!("sessions/2026/10/01/rollout-2026-10-01T01-05-00-{CODEX_ID}_{child}.jsonl.zst"),
            format!("sessions/2026/10/02/rollout-2026-10-02T00-00-01-{CODEX_ID}_{child}.jsonl"),
        ];
        write(&source, &record, "compressed bytes");
        for file in &subagents {
            write(&source, file, "subagent");
        }

        let located = locate(Agent::Codex, &source, CODEX_ID).expect("found");
        assert_eq!(located, Path::new(&record));
        let relatives = companions(Agent::Codex, &source, &located, CODEX_ID);
        assert!(relatives.is_empty());

        copy(&source, &target, &located, &relatives).expect("copied");
        assert_eq!(
            std::fs::read_to_string(target.join(&record)).unwrap(),
            "compressed bytes"
        );
        assert!(source.join(&record).is_file(), "moved");
        for file in &subagents {
            assert!(!target.join(file).exists(), "{file} was carried");
        }
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
        assert!(!target.join(&record).exists());
        assert!(!target.join(STAGING).exists());
        std::fs::remove_file(source.join(&sidecar).join("link")).unwrap();

        // The second item cannot be put in place: the first is put back as it was.
        let blocked = temp_dir("together-blocked");
        write(&blocked, &record, "older conversation");
        write(&blocked, "file-history", "a file where a directory must be");
        let err = copy(&source, &blocked, &record_path, &companions).expect_err("blocked");
        assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
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

        copy(&dir, &dir, Path::new("projects/slug/claude-id.jsonl"), &[]).expect("nothing to do");

        assert_eq!(
            std::fs::read_to_string(dir.join("projects/slug/claude-id.jsonl")).unwrap(),
            "conversation"
        );
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
}
