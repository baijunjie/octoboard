//! Reading the changes between commits out of `git`'s tree-diff output: the `--raw -z` records of
//! `git diff-tree` (a comparison, in `compare`) and `git log` (a commit listing, in `commits`),
//! each kept as the change it is to the project (`committed_record`), and the budget that output
//! is read under (`tree_diff_stdout`).

use anyhow::Result;

use crate::browse::changes;
use crate::browse::wire_path;
use crate::browse::{budget, git_failed, Located};
use crate::protocol::{ChangeEntry, ChangeSide};

/// One `--raw -z` record of `git diff-tree` or `git log` (`command`, which names it in a failure),
/// `:<old mode> <new mode> <old id> <new id> <status>` followed by its path — or, for a rename
/// (`R<score>`), the path it came from and the one it went to — as the change between the two
/// commits it is to the project, or `None` when neither side is inside it.
pub(super) fn committed_record<'a>(
    command: &str,
    meta: &[u8],
    fields: &mut impl Iterator<Item = &'a [u8]>,
    at: &Located,
    (old_commit, new_commit): (&str, &str),
) -> Result<Option<ChangeEntry>> {
    let malformed = || {
        let shown = String::from_utf8_lossy(meta);
        git_failed(&format!(
            "git {command} printed `{shown}`, which is not a record it writes"
        ))
    };
    let parts: Vec<&[u8]> = meta
        .strip_prefix(b":")
        .ok_or_else(malformed)?
        .split(|&b| b == b' ')
        .collect();
    let [old_mode, new_mode, old_oid, new_oid, status] = parts[..] else {
        return Err(malformed());
    };
    let path = fields.next().ok_or_else(malformed)?;
    // Only `-C` would add copies, which name two paths as well; renames are all there are.
    let to = match status.first() {
        Some(b'R') => Some(fields.next().ok_or_else(malformed)?),
        _ => None,
    };
    let scope = &at.repository.scope;
    let side = |commit: &str, mode: &[u8], oid: &[u8], path: &[u8]| {
        let rel = changes::in_scope(scope, path);
        match rel {
            Some(rel) => changes::head_side(Some(commit), mode, oid, &rel),
            None => ChangeSide::OutOfScope {
                repository_path: wire_path::encode(path),
            },
        }
    };
    // A rename's record names the path it came from first and the one it went to second.
    let (old, new) = (
        side(old_commit, old_mode, old_oid, path),
        side(new_commit, new_mode, new_oid, to.unwrap_or(path)),
    );
    // Kept when a side is inside the project; one of an addition or a deletion outside it is
    // named outside on its other side too, so it is never kept.
    let inside = |side: &ChangeSide| matches!(side, ChangeSide::Present { .. });
    Ok((inside(&old) || inside(&new)).then_some(ChangeEntry::Committed { old, new }))
}

/// The budget of the `git` output behind a comparison (`git diff-tree`) and behind a commit
/// listing (`git log --raw`), both over the whole tree; a test can lower it, as nothing short of
/// tens of thousands of changed files reaches the real one.
pub(super) fn tree_diff_stdout() -> usize {
    #[cfg(test)]
    if let Some(limit) = TREE_DIFF_STDOUT_OVERRIDE.with(std::cell::Cell::get) {
        return limit;
    }
    budget::TREE_DIFF_STDOUT
}

#[cfg(test)]
thread_local! {
    pub(super) static TREE_DIFF_STDOUT_OVERRIDE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}
