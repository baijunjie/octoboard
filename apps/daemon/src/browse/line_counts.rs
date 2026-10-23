//! The lines each change of a listing adds and removes, which a client shows beside the change
//! without reading its patch.
//!
//! Counts come from `git diff --numstat` run beside the listing's own read, over the project's
//! directory alone: for a worktree's staged and unstaged changes, for a comparison, and for the
//! commits of a commit listing. They are advisory, so a run that fails, runs past its own short
//! deadline ([`budget::LINE_COUNT_TIMEOUT`]) or prints past its budget leaves the listing without
//! counts instead of failing it; only a cancel is passed on. A change is counted only when each
//! side it has is a file inside the project: a link, a submodule and a side outside the project (a
//! rename across its boundary) have no count, nor does a binary file, which `git` reports as `-`.
//!
//! The listing and the count run detect renames separately, the listing over the whole tree and
//! the count run over the project's directory alone, so a count is matched to a change by its
//! shape too: a rename's count only to a change that has the same source and destination, and any
//! other count only to a change that is no rename.
//!
//! An untracked file has no `git` diff, so it is counted from the disk (`count_file`): every line
//! is an addition. A file past [`budget::MAX_FILE_BYTES`], one that looks binary the way `git`
//! judges it (a NUL in its first 8000 bytes), and every file after a listing has read
//! [`budget::UNTRACKED_COUNT_BYTES`] are left uncounted.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::git::{GitEnv, GitError};
use crate::browse::wire_path::RelPath;
use crate::browse::{budget, changes, git_error, line_count, live, Cancelled, Located};
use crate::protocol::{ChangeEntry, ChangeSide, LineStats, SideKind};
use crate::subprocess::BoundedFailure;

/// What `git diff --numstat` counted for one text file: its repository path's wire form is the key
/// of [`Counts`].
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Record {
    /// The wire path the file was renamed from, when `git` counted it as a rename.
    from: Option<String>,
    stats: LineStats,
}

/// The records of a run, by the project-relative wire path of each file's new side (its only
/// side, for a deletion).
pub(super) type Counts = HashMap<String, Record>;

/// The wire paths of the changes a run counts for; the records of any other path are dropped as
/// they are read, so what a run holds is bounded by the listing.
pub(super) type Wanted = HashSet<String>;

/// The paths `entries` are counted by.
pub(super) fn wanted<'a>(entries: impl Iterator<Item = &'a ChangeEntry>) -> Wanted {
    entries.filter_map(shape).map(|shape| shape.path).collect()
}

/// The counts of `git <subcommand> <args>`, which prints a single diff (`diff` or `diff-tree`),
/// for the paths in `wanted`. See [`read`].
pub(super) fn read_diff(
    env: &GitEnv,
    at: &Located,
    subcommand: &str,
    args: &[&[u8]],
    wanted: &Wanted,
    cancel: &AtomicBool,
) -> Result<Counts> {
    let only = |id: &[u8]| id.is_empty().then_some(wanted);
    let groups = read(env, at, subcommand, args, &only, cancel)?;
    Ok(groups
        .into_iter()
        .next()
        .map(|(_, counts)| counts)
        .unwrap_or_default())
}

/// The counts of `git log <args>`, whose format must be the commit's id and a NUL alone: one group
/// per commit of `wanted`, by its id, holding the records of that commit's paths alone.
pub(super) fn read_log(
    env: &GitEnv,
    at: &Located,
    args: &[&[u8]],
    wanted: &HashMap<Vec<u8>, Wanted>,
    cancel: &AtomicBool,
) -> Result<Vec<(Vec<u8>, Counts)>> {
    read(env, at, "log", args, &|id| wanted.get(id), cancel)
}

/// The `-z --numstat` counts of `git <subcommand> <args>`, run in the worktree `at` names and, for
/// a project below the repository's root, limited to its directory. `args` holds everything but the
/// pathspec, and ends with the revisions. The counts are in groups: one per commit header of a
/// `git log` (its `%H%x00` format being the only header), and one, under an empty key, for what
/// comes before any header, which is all a `git diff` or `git diff-tree` prints.
fn read<'w>(
    env: &GitEnv,
    at: &Located,
    subcommand: &str,
    args: &[&[u8]],
    wanted: &dyn Fn(&[u8]) -> Option<&'w Wanted>,
    cancel: &AtomicBool,
) -> Result<Vec<(Vec<u8>, Counts)>> {
    let scope = &at.repository.scope;
    let mut all: Vec<&[u8]> = vec![b"-z", b"--numstat", b"--no-relative"];
    all.extend_from_slice(args);
    if !scope.is_root() {
        // A literal pathspec (`GitEnv::run` sets `GIT_LITERAL_PATHSPECS`): the scope's directory
        // and everything below it.
        all.extend_from_slice(&[b"--", scope.as_bytes()]);
    }
    let run = env.run_within(
        &at.root,
        subcommand,
        &all,
        count_stdout(),
        budget::LINE_COUNT_TIMEOUT,
        cancel,
    );
    match run {
        Ok(stdout) => Ok(parse(&stdout, scope, wanted)),
        Err(err @ GitError::Stopped(BoundedFailure::Cancelled)) => Err(git_error(err)),
        Err(_) => Ok(Vec::new()),
    }
}

/// `-z --numstat` records: `<added>\t<removed>\t<path>`, or for a rename the same with an empty
/// path followed by the path it came from and the one it went to, each a field of its own. A
/// binary file counts `-` for both. Anything else is a header, which opens a group. The first
/// record of a group is preceded by the newline `git log` puts between a format and its diff.
/// `wanted` gives the paths asked for in the group of an id (the empty one before any header); a
/// group it has none for is left out with its records, and only a record of a path asked for is
/// kept.
fn parse<'w>(
    stdout: &[u8],
    scope: &RelPath,
    wanted: &dyn Fn(&[u8]) -> Option<&'w Wanted>,
) -> Vec<(Vec<u8>, Counts)> {
    let wire = |path: &[u8]| changes::in_scope(scope, path).map(|rel| rel.to_wire());
    let mut current = wanted(b"");
    let mut groups = Vec::new();
    if current.is_some() {
        groups.push((Vec::new(), Counts::new()));
    }
    let mut fields = stdout.split(|&b| b == 0);
    while let Some(field) = fields.next() {
        let field = field.strip_prefix(b"\n").unwrap_or(field);
        if field.is_empty() {
            continue;
        }
        let Some((stats, path)) = record(field) else {
            current = wanted(field);
            if current.is_some() {
                groups.push((field.to_vec(), Counts::new()));
            }
            continue;
        };
        let (from, path) = if path.is_empty() {
            let from = fields.next().unwrap_or_default();
            match fields.next() {
                Some(new) => (Some(from), new),
                None => break,
            }
        } else {
            (None, path)
        };
        let (Some(stats), Some(wanted), Some((_, counts))) = (stats, current, groups.last_mut())
        else {
            continue;
        };
        let Some(path) = wire(path).filter(|path| wanted.contains(path)) else {
            continue;
        };
        // A rename from outside the project can match no change that is counted.
        let from = match from {
            Some(from) => match wire(from) {
                Some(from) => Some(from),
                None => continue,
            },
            None => None,
        };
        counts.insert(path, Record { from, stats });
    }
    groups
}

/// A record's counts (`None` for a binary file's) and the rest of it, or `None` when `field` is no
/// record at all.
fn record(field: &[u8]) -> Option<(Option<LineStats>, &[u8])> {
    let mut parts = field.splitn(3, |&b| b == b'\t');
    let (added, removed, path) = (parts.next()?, parts.next()?, parts.next()?);
    let number = |digits: &[u8]| -> Option<Option<u64>> {
        if digits == b"-" {
            return Some(None);
        }
        let text = std::str::from_utf8(digits).ok()?;
        text.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| text.parse().ok())
            .flatten()
            .map(Some)
    };
    let stats = match (number(added)?, number(removed)?) {
        (Some(added), Some(removed)) => Some(LineStats { added, removed }),
        _ => None,
    };
    Some((stats, path))
}

/// Sets the counts of the entries of `entries` that `counts` has a record of the same shape for.
pub(super) fn apply<'a>(entries: impl Iterator<Item = &'a mut ChangeEntry>, counts: &Counts) {
    for entry in entries {
        let Some(shape) = shape(entry) else {
            continue;
        };
        let record = counts.get(&shape.path).filter(|r| r.from == shape.from);
        if let Some(record) = record {
            set(entry, Some(record.stats));
        }
    }
}

/// How an entry is told from the others in a count: the wire path it is counted by, and the one it
/// was renamed from.
pub(super) struct Shape {
    pub path: String,
    from: Option<String>,
}

/// The shape of `entry`, when it is a change of files inside the project.
pub(super) fn shape(entry: &ChangeEntry) -> Option<Shape> {
    let (old, new) = match entry {
        ChangeEntry::Staged { old, new, .. }
        | ChangeEntry::Unstaged { old, new, .. }
        | ChangeEntry::Untracked { old, new, .. }
        | ChangeEntry::Committed { old, new, .. } => (old, new),
        ChangeEntry::Conflicted { .. } => return None,
    };
    let file = |side: &ChangeSide| match side {
        ChangeSide::Present { kind, .. } => *kind == SideKind::File,
        ChangeSide::Absent => true,
        ChangeSide::OutOfScope { .. } => false,
    };
    if !file(old) || !file(new) {
        return None;
    }
    match (old, new) {
        (ChangeSide::Present { path: from, .. }, ChangeSide::Present { path, .. }) => Some(Shape {
            path: path.clone(),
            from: (from != path).then(|| from.clone()),
        }),
        (_, ChangeSide::Present { path, .. }) | (ChangeSide::Present { path, .. }, _) => {
            Some(Shape {
                path: path.clone(),
                from: None,
            })
        }
        _ => None,
    }
}

/// Sets the count of `entry`, which a conflict has none of.
pub(super) fn set(entry: &mut ChangeEntry, counts: Option<LineStats>) {
    if let ChangeEntry::Staged { stats, .. }
    | ChangeEntry::Unstaged { stats, .. }
    | ChangeEntry::Untracked { stats, .. }
    | ChangeEntry::Committed { stats, .. } = entry
    {
        *stats = counts;
    }
}

/// The lines of the file at `path`, all of them additions, read through the one reading rule
/// ([`live::read_resolved`]: no link, nothing but a regular file, a cancel honoured) against
/// `remaining` bytes of the listing's budget; `None` for a file that is too large, looks binary,
/// changed meanwhile or cannot be read. `rel` is the file's path in the project, and `path` the
/// place on disk it was resolved to.
pub(super) fn count_file(
    path: &Path,
    rel: &RelPath,
    remaining: &mut usize,
    cancel: &AtomicBool,
) -> Result<Option<LineStats>> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.is_file() || usize::try_from(metadata.len()).map_or(true, |len| len > *remaining) {
        return Ok(None);
    }
    let file = match live::read_resolved(path, rel, budget::MAX_FILE_BYTES, cancel) {
        Ok(file) => file,
        Err(err) if err.is::<Cancelled>() => return Err(err),
        Err(_) => return Ok(None),
    };
    *remaining = remaining.saturating_sub(file.bytes.len());
    if file.bytes.iter().take(BINARY_PROBE).any(|&b| b == 0) {
        return Ok(None);
    }
    Ok(Some(LineStats {
        added: line_count(&file.bytes),
        removed: 0,
    }))
}

/// How much of a file's start `git` looks at for a NUL to take it for binary.
const BINARY_PROBE: usize = 8000;

/// The bytes of untracked files a listing may read to count their lines.
pub(super) fn untracked_budget() -> usize {
    #[cfg(test)]
    if let Some(limit) = UNTRACKED_BUDGET_OVERRIDE.with(std::cell::Cell::get) {
        return limit;
    }
    budget::UNTRACKED_COUNT_BYTES
}

#[cfg(test)]
thread_local! {
    pub(super) static UNTRACKED_BUDGET_OVERRIDE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

/// The most stdout a count run may print.
fn count_stdout() -> usize {
    #[cfg(test)]
    if let Some(limit) = COUNT_STDOUT_OVERRIDE.with(std::cell::Cell::get) {
        return limit;
    }
    budget::LINE_COUNT_STDOUT
}

// A test can lower the budget of a count run, to see a run past it leave the listing without
// counts.
#[cfg(test)]
thread_local! {
    pub(super) static COUNT_STDOUT_OVERRIDE: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(added: u64, removed: u64) -> LineStats {
        LineStats { added, removed }
    }

    fn parsed(out: &[u8], wanted: &[&str]) -> Vec<(Vec<u8>, Counts)> {
        let wanted: Wanted = wanted.iter().map(|path| path.to_string()).collect();
        let scope = RelPath::from_bytes(Vec::new()).unwrap();
        parse(out, &scope, &|_| Some(&wanted))
    }

    #[test]
    fn parses_records_renames_and_binaries() {
        let out = b"1\t0\tadded.txt\0-\t-\tbin.dat\x002\t1\tf.txt\x001\t0\t\0old.txt\0new.txt\0";
        let groups = parsed(
            out,
            &["added.txt", "bin.dat", "f.txt", "new.txt", "old.txt"],
        );
        assert_eq!(groups.len(), 1);
        let counts = &groups[0].1;
        assert_eq!(counts["added.txt"].stats, stats(1, 0));
        assert_eq!(counts["f.txt"].stats, stats(2, 1));
        assert_eq!(counts["new.txt"].stats, stats(1, 0));
        assert_eq!(counts["new.txt"].from.as_deref(), Some("old.txt"));
        assert_eq!(counts["f.txt"].from, None);
        assert!(!counts.contains_key("bin.dat"));
        assert!(!counts.contains_key("old.txt"));
    }

    #[test]
    fn splits_a_log_into_commits() {
        let id = "a".repeat(40);
        let next = "b".repeat(40);
        let out = format!("{id}\0\0\n3\t0\ta\0{next}\0\0{id}\0\0\n-\t-\tz\0");
        let groups = parsed(out.as_bytes(), &["a", "z"]);
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[1].0, id.as_bytes());
        assert_eq!(groups[1].1["a"].stats, stats(3, 0));
        assert!(groups[2].1.is_empty());
        assert!(groups[3].1.is_empty());
    }

    #[test]
    fn a_log_keeps_only_the_commits_and_paths_asked_for() {
        let (id, other) = ("a".repeat(40), "b".repeat(40));
        let out = format!("{id}\0\n1\t0\tx\02\t0\ty\0{other}\0\n3\t0\tx\0");
        let wanted: HashMap<Vec<u8>, Wanted> =
            HashMap::from([(id.clone().into_bytes(), Wanted::from(["x".to_string()]))]);
        let scope = RelPath::from_bytes(Vec::new()).unwrap();
        let groups = parse(out.as_bytes(), &scope, &|id| wanted.get(id));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, id.as_bytes());
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[0].1["x"].stats, stats(1, 0));
    }

    #[test]
    fn a_path_may_hold_tabs_and_a_leading_newline() {
        let groups = parsed(b"1\t2\ta\tb\0\n4\t0\t\nc\0", &["a\tb", "\nc"]);
        let counts = &groups[0].1;
        assert_eq!(counts["a\tb"].stats, stats(1, 2));
        assert_eq!(counts["\nc"].stats, stats(4, 0));
    }
}
