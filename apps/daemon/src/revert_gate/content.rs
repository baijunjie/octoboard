//! The content rules: which commits a move from `old` to `new` undoes, judged from the files
//! the commits changed and the lines in them. The rules themselves are in the header of the
//! parent module.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use anyhow::{Context, Result};

use super::git::{is_null, is_oid, is_space, lossy, Git};

/// How many commits to look back. A revert usually comes from a branch developed in parallel in
/// the same period, so its base is not far behind; too large a value reads many more blobs on
/// every merge. When a fork point is known, every commit after it is checked as well, but no
/// more than [`MAX_DEPTH`], so that a branch left unmerged for a long time cannot slow the check
/// down enough to stall the merge.
const WINDOW: usize = 50;
/// How many commits to look back when no fork point is known for the move at all (neither
/// recorded nor found in the branch's reflog): nothing then says how far back the branch's base
/// can be, so the window is wider.
const WINDOW_WITHOUT_FORK: usize = 100;
const MAX_DEPTH: usize = 1000;

/// Line-level criteria: the file only counts as restored when the share of C's added lines that
/// disappeared and the share of its deleted lines that came back each reach [`RATIO`]. "Came back
/// in bulk" also requires at least [`MIN_LINES`] recovered lines; a few stray matches are normal.
const RATIO: f64 = 0.5;
const MIN_LINES: usize = 3;

/// Files larger than this are only compared whole, without reading their content, so that big
/// files are not read into memory on every merge.
const MAX_BLOB: usize = 4 * 1024 * 1024;

/// A path, as the bytes git prints it in.
pub(super) type Path = Vec<u8>;

/// A file's blob ids before and after a change; `None` when it was absent.
type Pair = (Option<String>, Option<String>);

/// What a move changed, by path.
type Touched = HashMap<Path, Pair>;

/// One file change of a commit.
struct Change {
    before: Option<String>,
    after: Option<String>,
    path: Path,
}

struct Commit {
    sha: String,
    subject: String,
    changes: Vec<Change>,
}

/// A commit a move undid and the files that show it, each with the reason.
pub(super) struct Finding {
    pub(super) sha: String,
    pub(super) subject: String,
    pub(super) hits: Vec<(Path, String)>,
}

/// The lines of a file as a multiset.
#[derive(Default)]
struct Lines(HashMap<String, usize>);

impl Lines {
    /// The lines of `data` that carry something: split at `\n` only (not at form feed, U+2028 and
    /// the like, which disagree with git's lines), stripped, and without the ones made only of
    /// brackets, punctuation and whitespace, which repeat heavily in any file and would create
    /// false signals. A byte that is not valid UTF-8 stays a character of its own that matches
    /// only itself, so two different invalid lines never merge.
    fn of(data: &[u8]) -> Self {
        let mut lines = HashMap::new();
        for line in data.split(|byte| *byte == b'\n') {
            let line = escaped(line);
            if line.chars().all(is_trivial) {
                continue;
            }
            *lines
                .entry(line.trim_matches(is_space).to_string())
                .or_insert(0) += 1;
        }
        Self(lines)
    }

    /// The lines of `self` beyond the copies `other` has.
    fn minus(&self, other: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter_map(|(line, count)| {
                    let rest = count.saturating_sub(other.0.get(line).copied().unwrap_or(0));
                    (rest > 0).then(|| (line.clone(), rest))
                })
                .collect(),
        )
    }

    /// The copies both have.
    fn and(&self, other: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter_map(|(line, count)| {
                    let both = (*count).min(other.0.get(line).copied().unwrap_or(0));
                    (both > 0).then(|| (line.clone(), both))
                })
                .collect(),
        )
    }

    fn total(&self) -> usize {
        self.0.values().sum()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `line` as text, each byte that is not part of valid UTF-8 as a private-use character of its
/// own (U+10F800 + the byte).
fn escaped(line: &[u8]) -> String {
    let mut text = String::new();
    for chunk in line.utf8_chunks() {
        text.push_str(chunk.valid());
        text.extend(
            chunk
                .invalid()
                .iter()
                .map(|byte| char::from_u32(0x10_F800 + u32::from(*byte)).expect("private use")),
        );
    }
    text
}

fn is_trivial(c: char) -> bool {
    is_space(c) || "{}()[];,.:<>/*#-".contains(c)
}

/// `part` as a share of `whole`; all of it when `whole` is empty.
fn ratio(part: &Lines, whole: &Lines) -> f64 {
    if whole.is_empty() {
        1.0
    } else {
        part.total() as f64 / whole.total() as f64
    }
}

fn os(path: &[u8]) -> OsString {
    OsString::from_vec(path.to_vec())
}

/// One file change starting at field `i` of `-z --raw` output, and the index after it.
fn parse_raw(fields: &[&[u8]], i: usize) -> Result<(Change, usize)> {
    let record = lossy(&fields[i][1..]);
    let parts: Vec<&str> = record.splitn(5, ' ').collect();
    let [_, _, before, after, _] = parts[..] else {
        anyhow::bail!("unexpected raw diff record {record:?}");
    };
    let oid = |oid: &str| (!is_null(oid)).then(|| oid.to_string());
    Ok((
        Change {
            before: oid(before),
            after: oid(after),
            path: fields[i + 1].to_vec(),
        },
        i + 2,
    ))
}

/// The raw records of `-z --raw` output that start at field `i`; the fields end at the first one
/// that is not a record.
fn is_record(fields: &[&[u8]], i: usize) -> bool {
    i + 1 < fields.len() && fields[i].starts_with(b":")
}

/// The files that differ between trees `a` and `b`, with their blob ids in each.
fn tree_diff(git: &Git, a: &str, b: &str) -> Result<Touched> {
    let out = git.read(["diff-tree", "-r", "-z", "--no-renames", a, b])?;
    let fields: Vec<&[u8]> = out.split(|byte| *byte == 0).collect();
    let (mut result, mut i) = (HashMap::new(), 0);
    while is_record(&fields, i) {
        let (change, next) = parse_raw(&fields, i)?;
        result.insert(change.path, (change.before, change.after));
        i = next;
    }
    Ok(result)
}

/// Paths that git identifies as rename sources in a to b: the content moved elsewhere, it was
/// not deleted.
fn rename_sources(git: &Git, a: &str, b: &str) -> Result<HashSet<Path>> {
    let out = git.read_lenient([
        "diff-tree",
        "-r",
        "-z",
        "--find-renames",
        "--diff-filter=R",
        a,
        b,
    ])?;
    let fields: Vec<&[u8]> = out.split(|byte| *byte == 0).collect();
    let (mut sources, mut i) = (HashSet::new(), 0);
    while is_record(&fields, i) {
        // A rename record carries an extra path field: `:<mode>... R<similarity>\0<source>\0<destination>`.
        sources.insert(fields[i + 1].to_vec());
        i += 3;
    }
    Ok(sources)
}

/// The commits to check: the most recent `window` on `old`, united with everything between
/// `old` and the merge base of `old` and each fork point (at most [`MAX_DEPTH`]), non-merge
/// commits only.
fn recent_commits(git: &Git, old: &str, forks: &[String], window: usize) -> Result<Vec<Commit>> {
    let mut ranges = vec![vec![format!("--max-count={window}"), old.to_string()]];
    for fork in forks {
        let base = git.line_lenient(["merge-base", fork, old])?;
        if !base.is_empty() {
            ranges.push(vec![
                format!("--max-count={MAX_DEPTH}"),
                format!("{base}..{old}"),
            ]);
        }
    }
    let mut subjects: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();
    for range in ranges {
        let mut args = vec![
            "log".to_string(),
            "--no-show-signature".into(),
            "--no-merges".into(),
            "--format=%H %s".into(),
        ];
        args.extend(range);
        let listing = lossy(&git.read(args)?);
        for line in listing.split('\n').filter(|line| !line.is_empty()) {
            let (sha, subject) = line.split_once(' ').unwrap_or((line, ""));
            if seen.insert(sha.to_string()) {
                subjects.push((sha.to_string(), subject.to_string()));
            }
        }
    }
    if subjects.is_empty() {
        return Ok(Vec::new());
    }
    // In `--stdin` mode each commit prints its own id first, then its file list; `--root`
    // compares the root commit with the empty tree.
    let input = subjects
        .iter()
        .map(|(sha, _)| format!("{sha}\n"))
        .collect::<String>();
    let out = git.read_with_input(
        ["diff-tree", "--stdin", "--root", "-r", "-z", "--no-renames"],
        input.into_bytes(),
    )?;
    let fields: Vec<&[u8]> = out.split(|byte| *byte == 0).collect();
    let mut commits: Vec<Commit> = subjects
        .into_iter()
        .map(|(sha, subject)| Commit {
            sha,
            subject,
            changes: Vec::new(),
        })
        .collect();
    let index: HashMap<String, usize> = commits
        .iter()
        .enumerate()
        .map(|(at, commit)| (commit.sha.clone(), at))
        .collect();
    let (mut current, mut i) = (None, 0);
    while i < fields.len() {
        let field = lossy(fields[i]);
        if is_oid(field.trim_matches(is_space)) {
            current = index.get(field.trim_matches(is_space)).copied();
            i += 1;
        } else if let (true, Some(at)) = (is_record(&fields, i), current) {
            let (change, next) = parse_raw(&fields, i)?;
            commits[at].changes.push(change);
            i = next;
        } else {
            i += 1;
        }
    }
    Ok(commits)
}

/// The commits that the messages added by the move name as deliberately reverted: lines that
/// start with `Reverts: <sha>` or git revert's `This reverts commit <sha>.`, matched without
/// regard to case, the sha 7 to 64 hex digits. Lower-cased.
fn overridden(git: &Git, old: &str, new: &str) -> Result<Vec<String>> {
    let range = format!("{old}..{new}");
    let messages =
        lossy(&git.read(["log", "--no-show-signature", "--format=%B", range.as_str()])?);
    Ok(declared_reverts(&messages))
}

fn declared_reverts(messages: &str) -> Vec<String> {
    let chars: Vec<char> = messages.chars().collect();
    let (mut found, mut at) = (Vec::new(), 0);
    while at < chars.len() {
        if at > 0 && chars[at - 1] != '\n' {
            at += 1;
            continue;
        }
        match declared_at(&chars, at) {
            Some((sha, end)) => {
                found.push(sha);
                at = end;
            }
            None => at += 1,
        }
    }
    found
}

/// The sha of a declaration starting at `at`, and where it ends.
fn declared_at(chars: &[char], at: usize) -> Option<(String, usize)> {
    let literal = |from: usize, text: &str| {
        let end = from + text.len();
        let matches = chars.get(from..end).is_some_and(|slice| {
            slice
                .iter()
                .zip(text.chars())
                .all(|(c, expected)| c.eq_ignore_ascii_case(&expected))
        });
        matches.then_some(end)
    };
    let mut from = match literal(at, "reverts:") {
        Some(end) => {
            let mut end = end;
            while chars.get(end).is_some_and(|c| is_space(*c)) {
                end += 1;
            }
            end
        }
        None => literal(at, "this reverts commit ")?,
    };
    let start = from;
    while from - start < 64 && chars.get(from).is_some_and(|c| c.is_ascii_hexdigit()) {
        from += 1;
    }
    (from - start >= 7).then(|| {
        (
            chars[start..from].iter().collect::<String>().to_lowercase(),
            from,
        )
    })
}

/// The blob id of each of `paths` in `commit`; `None` when absent (or not a regular file).
fn tree_entries(git: &Git, commit: &str, paths: &[Path]) -> Result<HashMap<Path, Option<String>>> {
    // A request is a line, so a path with a newline in it cannot be asked about.
    let mut wanted: Vec<&Path> = Vec::new();
    let mut seen = HashSet::new();
    for path in paths.iter().filter(|path| !path.contains(&b'\n')) {
        if seen.insert(path) {
            wanted.push(path);
        }
    }
    let lines = batch_check(
        git,
        wanted.iter().map(|path| {
            let mut request = format!("{commit}:").into_bytes();
            request.extend_from_slice(path);
            request
        }),
        "%(objectname) %(objecttype)",
    )?;
    let mut result = HashMap::new();
    for (path, line) in wanted.into_iter().zip(lines) {
        // A miss echoes the request itself plus " missing", and paths may contain spaces, so
        // only the "id + blob" shape is accepted.
        let line = lossy(&line);
        let (oid, kind) = line.split_once(' ').unwrap_or((&line, ""));
        result.insert(
            path.clone(),
            (kind == "blob" && is_oid(oid)).then(|| oid.to_string()),
        );
    }
    Ok(result)
}

/// `cat-file --batch-check`: one output line per request.
fn batch_check(
    git: &Git,
    requests: impl Iterator<Item = Vec<u8>>,
    format: &str,
) -> Result<Vec<Vec<u8>>> {
    let mut input = Vec::new();
    let mut count = 0;
    for request in requests {
        input.extend_from_slice(&request);
        input.push(b'\n');
        count += 1;
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let out = git.read_with_input(
        ["cat-file".to_string(), format!("--batch-check={format}")],
        input,
    )?;
    Ok(out
        .split(|byte| *byte == b'\n')
        .take(count)
        .map(<[u8]>::to_vec)
        .collect())
}

/// The content of blobs by id; blobs over [`MAX_BLOB`] are not read and are absent from the
/// result.
fn read_blobs(git: &Git, oids: &[String]) -> Result<HashMap<String, Vec<u8>>> {
    let sizes = batch_check(
        git,
        oids.iter().map(|oid| oid.clone().into_bytes()),
        "%(objectsize)",
    )?;
    let wanted: Vec<&String> = oids
        .iter()
        .zip(sizes)
        .filter(|(_, size)| {
            lossy(size)
                .parse::<usize>()
                .is_ok_and(|size| size <= MAX_BLOB)
        })
        .map(|(oid, _)| oid)
        .collect();
    if wanted.is_empty() {
        return Ok(HashMap::new());
    }
    let input = wanted
        .iter()
        .map(|oid| format!("{oid}\n"))
        .collect::<String>();
    let out = git.read_with_input(["cat-file", "--batch"], input.into_bytes())?;
    let (mut blobs, mut pos) = (HashMap::new(), 0);
    for oid in wanted {
        let header_end = out
            .get(pos..)
            .context("truncated cat-file output")?
            .iter()
            .position(|byte| *byte == b'\n')
            .context("truncated cat-file output")?
            + pos;
        let header = lossy(&out[pos..header_end]);
        let size: usize = header
            .split_whitespace()
            .next_back()
            .and_then(|size| size.parse().ok())
            .with_context(|| format!("unexpected cat-file header {header:?}"))?;
        let body = out
            .get(header_end + 1..header_end + 1 + size)
            .context("truncated cat-file output")?;
        blobs.insert(oid.clone(), body.to_vec());
        pos = header_end + 1 + size + 1;
    }
    Ok(blobs)
}

/// Traces left by a changed base where the content was never merged: this move changed some
/// file relative to `old` that the branch itself (fork point to `new`) never touched at all —
/// that difference was not written by the author, it was dragged in by the base changing.
///
/// This criterion does not look at whether the content resembles being restored, so it catches a
/// change on the target-branch side regardless of whether it was an add, a delete, or an edit to
/// existing content, including the shape line-level criteria cannot see — a file that only ever
/// gained lines being deleted wholesale. When the author themselves deleted a file that already
/// existed at the fork point, it shows up in the branch's own diff and is not caught here; only
/// content that first appeared after the fork point being deleted counts — a deliberate deletion
/// should still leave a `Reverts:`.
/// Cannot check without a fork point (recorded, or the commit the branch's reflog says it was
/// created at): a squash that changed the base is exactly one
/// that rewrote the parent to the target branch's tip, so merge-base is no longer the fork
/// point. Rename sources do not count; that content just moved elsewhere.
fn base_artifacts(
    git: &Git,
    old: &str,
    new: &str,
    touched: &Touched,
    allowed: &[String],
    bases: &[String],
) -> Result<Vec<Finding>> {
    // A fork point equal to `new` does not count: that branch contributed no commit at all (most
    // likely it was just created and happens to point here); using it to compute the authored
    // files would give an empty set, making every file this move touched a suspect.
    let bases: Vec<&String> = bases.iter().filter(|base| *base != new).collect();
    if bases.is_empty() || touched.is_empty() {
        return Ok(Vec::new());
    }
    let mut authored: HashSet<Path> = HashSet::new();
    for base in bases {
        // Rename-aware: when the branch renamed a file, only the new path counts as authored by
        // it; the old path remains a suspect — if the target branch rewrote the old path's
        // content after the fork point, that is exactly how such a rewrite gets lost.
        // `diff-tree` rather than `diff`, so this is not affected by each clone's
        // `diff.renames` setting.
        let out = git.read_lenient([
            "diff-tree",
            "-r",
            "--find-renames",
            "--name-only",
            "-z",
            base,
            new,
        ])?;
        authored.extend(
            out.split(|byte| *byte == 0)
                .filter(|path| !path.is_empty())
                .map(<[u8]>::to_vec),
        );
    }
    let mut suspects: Vec<&Path> = touched
        .keys()
        .filter(|path| !authored.contains(*path))
        .collect();
    if suspects.is_empty() {
        return Ok(Vec::new());
    }
    let renamed = rename_sources(git, old, new)?;
    suspects.retain(|path| !renamed.contains(*path));
    suspects.sort();
    let mut found: Vec<Finding> = Vec::new();
    for path in suspects {
        let (at_old, at_new) = &touched[path];
        // Attribute it to the commit that last changed this file on the target branch: a
        // deliberate revert should carry a `Reverts:` naming it. Literal, so that a path with a
        // glob character in it is that file.
        let out = git.line_lenient([
            OsString::from("--literal-pathspecs"),
            "log".into(),
            "-1".into(),
            "--no-show-signature".into(),
            "--no-merges".into(),
            "--format=%H %s".into(),
            old.into(),
            "--".into(),
            os(path),
        ])?;
        if out.is_empty() {
            continue;
        }
        let (sha, subject) = out.split_once(' ').unwrap_or((&out, ""));
        if allowed.iter().any(|a| sha.starts_with(a.as_str())) {
            continue;
        }
        let why = if at_new.is_none() {
            "this file was deleted wholesale, and this branch itself never touched it"
        } else if at_old.is_none() {
            // Cannot stand alone: when the resurrected content is close to the deleted
            // version, the content criteria will always hit first.
            "this file was resurrected, and this branch itself never touched it"
        } else {
            "this file was changed back to its version at the fork point, and this branch itself never touched it"
        };
        let hit = (path.clone(), why.to_string());
        match found.iter_mut().find(|finding| finding.sha == sha) {
            Some(finding) => finding.hits.push(hit),
            None => found.push(Finding {
                sha: sha.to_string(),
                subject: subject.to_string(),
                hits: vec![hit],
            }),
        }
    }
    Ok(found)
}

/// The commits that the move `old` to `new` undid, newest first.
///
/// A commit C counts as undone in three cases; the first two compare content, the third compares
/// "who wrote it":
/// - Wholesale: every file C touched and whose C changes were still live at `old` was restored
///   by this move, and the lines C deleted were brought back.
/// - Partial: some existing file C modified or deleted was restored and the lines C deleted came
///   back in bulk — resolving every conflict to your own side during a rebase has exactly this
///   shape.
/// - Changed base (see [`base_artifacts`]): this move changed some file that the branch itself
///   never touched since its fork point. Checking this requires a fork point, recorded or the
///   commit the branch's reflog says it was created at; once it can be checked, add, delete, and
///   edit to existing content are all treated the same.
///
/// The line comparison only looks at the multiset of lines, not at positions, so it still works
/// when the file was changed again by other commits after C. `forks` decides how far back the
/// commits are looked at; `bases` is the branch's actual fork points, and with none of them the
/// window is [`WINDOW_WITHOUT_FORK`] instead of [`WINDOW`].
pub(super) fn find_reverts(
    git: &Git,
    old: &str,
    new: &str,
    forks: &[String],
    bases: &[String],
) -> Result<Vec<Finding>> {
    let touched = tree_diff(git, old, new)?;
    if touched.is_empty() {
        return Ok(Vec::new());
    }
    let allowed = overridden(git, old, new)?;
    let window = if bases.is_empty() {
        WINDOW_WITHOUT_FORK
    } else {
        WINDOW
    };
    let candidates: Vec<Commit> = recent_commits(git, old, forks, window)?
        .into_iter()
        .filter(|commit| {
            commit
                .changes
                .iter()
                .any(|change| touched.contains_key(&change.path))
                && !allowed.iter().any(|a| commit.sha.starts_with(a.as_str()))
        })
        .collect();
    if candidates.is_empty() {
        return base_artifacts(git, old, new, &touched, &allowed, bases);
    }
    // A file this move did not touch is the same in `old` and `new`, so the one in `old` is
    // enough to tell whether C's changes are still live.
    let untouched_paths: Vec<Path> = candidates
        .iter()
        .flat_map(|commit| &commit.changes)
        .filter(|change| !touched.contains_key(&change.path))
        .map(|change| change.path.clone())
        .collect();
    let untouched = tree_entries(git, old, &untouched_paths)?;
    let mut wanted: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for change in candidates.iter().flat_map(|commit| &commit.changes) {
        if let Some((at_old, at_new)) = touched.get(&change.path) {
            for oid in [&change.before, &change.after, at_old, at_new]
                .into_iter()
                .flatten()
            {
                if seen.insert(oid.clone()) {
                    wanted.push(oid.clone());
                }
            }
        }
    }
    let blobs = read_blobs(git, &wanted)?;
    // An absent file reads as empty; a blob that was not read is `None`.
    let content = |oid: &Option<String>| match oid {
        None => Some(&[][..]),
        Some(oid) => blobs.get(oid).map(Vec::as_slice),
    };

    let mut findings: Vec<Finding> = Vec::new();
    for commit in &candidates {
        let (mut live, mut whole, mut restored_any) = (0, true, false);
        let (mut partial, mut reverted): (Vec<(Path, String)>, Vec<Path>) =
            (Vec::new(), Vec::new());
        for Change {
            before,
            after,
            path,
        } in &commit.changes
        {
            let (at_old, at_new) = match touched.get(path) {
                Some(pair) => pair.clone(),
                None => {
                    let both = untouched.get(path).cloned().flatten();
                    (both.clone(), both)
                }
            };
            if &at_old == before {
                continue; // C's changes to this file were already undone before `old`, unrelated to this move
            }
            live += 1;
            if at_new == at_old {
                whole = false;
                continue;
            }
            let exact = &at_new == before;
            let data = [before, after, &at_old, &at_new].map(content);
            let [Some(c_before), Some(c_after), Some(old_data), Some(new_data)] = data else {
                // Content unread (too large, a submodule, or the object missing locally):
                // compare whole files only. What C deleted is unknowable here, so treat it as
                // having changed or deleted existing content.
                if !exact {
                    whole = false;
                    continue;
                }
                reverted.push(path.clone());
                if before.is_some() {
                    restored_any = true;
                    partial.push((
                        path.clone(),
                        "the whole file is back at its previous version".to_string(),
                    ));
                }
                continue;
            };
            let [c_before, c_after, old_lines, new_lines] =
                [c_before, c_after, old_data, new_data].map(Lines::of);
            let alive = c_after.minus(&c_before).and(&old_lines);
            let gone = c_before.minus(&c_after).minus(&old_lines);
            let (lost, back) = (alive.minus(&new_lines), gone.and(&new_lines));
            let restored = exact
                || ((!alive.is_empty() || !gone.is_empty())
                    && ratio(&lost, &alive) >= RATIO
                    && ratio(&back, &gone) >= RATIO);
            if !restored {
                whole = false;
                continue;
            }
            reverted.push(path.clone());
            if !back.is_empty() {
                restored_any = true;
            }
            // Only "deleted lines came back in bulk" counts: when C only adds, taking away what
            // it added (such as deleting temporary experimental code) is a normal change.
            if before.is_some() && back.total() >= MIN_LINES {
                let why = if exact {
                    "the whole file is back at its previous version".to_string()
                } else {
                    format!(
                        "{} of the {} lines it deleted came back, {} of the {} lines it added are gone",
                        back.total(),
                        gone.total(),
                        lost.total(),
                        alive.total()
                    )
                };
                partial.push((path.clone(), why));
            }
        }
        if live > 0 && whole && restored_any {
            findings.push(Finding {
                sha: commit.sha.clone(),
                subject: commit.subject.clone(),
                hits: reverted
                    .into_iter()
                    .map(|path| {
                        (
                            path,
                            "this commit's changes to this file were reverted".to_string(),
                        )
                    })
                    .collect(),
            });
        } else if !partial.is_empty() {
            findings.push(Finding {
                sha: commit.sha.clone(),
                subject: commit.subject.clone(),
                hits: partial,
            });
        }
    }
    for artifact in base_artifacts(git, old, new, &touched, &allowed, bases)? {
        match findings
            .iter_mut()
            .find(|finding| finding.sha == artifact.sha)
        {
            // The same commit matched both categories: merge the file lists, and don't report
            // the same file twice.
            Some(finding) => {
                let shown: HashSet<Path> =
                    finding.hits.iter().map(|(path, _)| path.clone()).collect();
                finding.hits.extend(
                    artifact
                        .hits
                        .into_iter()
                        .filter(|(path, _)| !shown.contains(path)),
                );
            }
            None => findings.push(artifact),
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_reverts_reads_both_forms_at_line_start_without_regard_to_case() {
        let message = "fix\n\nREVERTS: ABCDEF1234\nThis reverts commit 0123456789abcdef.\n\
                       Reverts:\n1234567\nnot Reverts: 7654321\nReverts: abc12\n";
        assert_eq!(
            declared_reverts(message),
            ["abcdef1234", "0123456789abcdef", "1234567"]
        );
    }

    #[test]
    fn lines_split_at_newline_only_and_invalid_bytes_never_merge() {
        let lines = Lines::of(b"  keep  \n}\x0c\n\xff\n\xfe\n\x0c\xe2\x80\xa8end\x0c");
        let mut keys: Vec<&String> = lines.0.keys().collect();
        keys.sort();
        // The form feed and U+2028 trim away only at the line's ends, and each invalid byte is
        // a line of its own.
        assert_eq!(keys.len(), 4);
        assert!(lines.0.contains_key("keep") && lines.0.contains_key("end"));
    }
}
