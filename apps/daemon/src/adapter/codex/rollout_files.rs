//! The rules for a Codex rollout file's name and for the chain of rollouts one thread is made of:
//! the first rollout, and a later history segment for each turn reverted, each naming the file it
//! continues in its first line.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::providers::codex::{
    CODEX_COMPRESSED_ROLLOUT_SUFFIX, CODEX_ROLLOUT_PREFIX, CODEX_ROLLOUT_SUFFIX,
};
use crate::records::locate::modified;
use crate::records::Stretch;

/// Whether a directory entry is the rollout file of the Codex thread `agent_id`; the name and the
/// id both lowercased.
pub(super) fn is_rollout_of(kind: std::fs::FileType, name: &str, agent_id: &str) -> bool {
    kind.is_file()
        && name.starts_with(CODEX_ROLLOUT_PREFIX)
        && rollout_stem(name).is_some_and(|stem| names_thread(stem, agent_id))
}

/// A Codex rollout's file name without its extension: `.jsonl`, or `.jsonl.zst` once Codex has
/// compressed it (it does so for rollouts older than a week).
pub fn rollout_stem(name: &str) -> Option<&str> {
    name.strip_suffix(CODEX_ROLLOUT_SUFFIX)
        .or_else(|| name.strip_suffix(CODEX_COMPRESSED_ROLLOUT_SUFFIX))
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
pub(super) fn path_stem(path: &Path) -> Option<&str> {
    rollout_stem(path.file_name()?.to_str()?)
}

/// The id a rollout is continued by: the last 36 characters of its [`rollout_stem`], the thread id
/// for a first rollout and the segment id for a later one.
pub(super) fn own_id(stem: &str) -> Option<&str> {
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
    let mut reader = crate::records::line_file::open(path, 0).ok()?;
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::protocol::AgentId;
    use crate::records::fixtures::*;
    use crate::records::locate::record_paths;
    use crate::records::{conversation_branch, presence, Hints, Locator};

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
            let codex = session(AgentId::CODEX, CODEX_ID, Some(&account));

            let mut named = record_paths(&hints, &mut locator, &[], &codex);
            named.sort();
            assert_eq!(named, chain.all(), "compressed: {compressed}");
            assert_eq!(
                locator.current_record(AgentId::CODEX, &account, CODEX_ID),
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
}
