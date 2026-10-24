//! Which entries of a project's directory Git itself would ignore: every `.gitignore` level,
//! `.git/info/exclude` and the user's global excludes, as `git check-ignore` and
//! `git ls-files --ignored` apply them. A tracked file that matches a pattern is not ignored, and
//! anything inside an ignored folder is.
//!
//! A listing asks `git check-ignore` about its entries, one run (more only for a listing of
//! thousands of long names) and none at all for a folder whose entries and ignore sources are as
//! they were at the last listing. A search asks `git ls-files --others --ignored --directory` once
//! for the project's repository and once more for each repository nested in it (a submodule, a
//! clone inside the project) that its traversal enters, since the listing of a folder there is
//! answered by that nested repository's rules.
//!
//! Both are advisory: a project outside a repository costs no subprocess, and a `git` that cannot
//! be run, fails, or runs past its own short deadline ([`budget::IGNORE_CHECK_TIMEOUT`],
//! [`budget::IGNORED_PATHS_TIMEOUT`]) leaves everything "not ignored" rather than failing the read
//! it decorates. Only a cancel ends the read.
//!
//! Every run switches `core.precomposeUnicode` off, so that `git` prints the names the filesystem
//! holds (macOS would otherwise print a decomposed name precomposed, and no byte comparison with
//! the directory's own listing would find it). The price is that a pattern with non-ASCII text
//! matches only a name in the same Unicode form as the pattern.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};
use std::time::{Instant, SystemTime};

use anyhow::Result;

use crate::browse::git::{GitEnv, GitError};
use crate::browse::{budget, source, wire_path, Cancelled, Git};
use crate::protocol::BrowseEntry;
use crate::subprocess::BoundedFailure;

/// The most name bytes one `git check-ignore` run is given on its command line, so a listing of
/// thousands of long names still fits the system's argument limit; a longer listing takes more runs.
const CHECK_BATCH_BYTES: usize = 96 * 1024;

/// The most names one `git check-ignore` run is given, so that its output (a line of source, line
/// number, pattern and name for each) stays within [`budget::GIT_METADATA_STDOUT`].
const CHECK_BATCH_NAMES: usize = 1000;

/// The configuration every run is given; see the module documentation.
const NAMES_AS_STORED: &str = "core.precomposeUnicode=false";

/// What a file's modification time and size say about it, or `None` when it is not there.
type Stamp = Option<(SystemTime, u64)>;

/// What the daemon remembers of one folder's last listing: the entries it had, the files that
/// decide what Git ignores among them as they were, and which of the entries were ignored.
struct Remembered {
    names: u64,
    sources: Vec<Stamp>,
    at: Instant,
    ignored: Vec<bool>,
}

/// The folders (canonical paths) remembered, at most [`budget::IGNORE_CACHE_FOLDERS`].
static REMEMBERED: LazyLock<Mutex<HashMap<PathBuf, Remembered>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn remembered() -> std::sync::MutexGuard<'static, HashMap<PathBuf, Remembered>> {
    REMEMBERED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn stamp(path: &Path) -> Stamp {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// The repository's own directory and its common directory (the same, apart from a linked
/// worktree), for the worktree whose `.git` entry is in `top`.
fn git_dirs(top: &Path) -> Option<(PathBuf, PathBuf)> {
    let dot_git = top.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let text = std::fs::read(&dot_git).ok()?;
        let target = text.strip_prefix(b"gitdir:")?.trim_ascii();
        top.join(OsStr::from_bytes(target))
    };
    let common_dir = match std::fs::read(git_dir.join("commondir")) {
        Ok(text) => git_dir.join(OsStr::from_bytes(text.trim_ascii())),
        Err(_) => git_dir.clone(),
    };
    Some((git_dir, common_dir))
}

/// The files that decide what Git ignores in `dir` and what it tracks, as they stand: the index,
/// `info/exclude`, and the `.gitignore` of each folder from the worktree's top `top` down to `dir`.
/// The global excludes file is not among them (naming it takes the configuration, which only
/// `git` reads); [`budget::IGNORE_CACHE_TTL`] bounds how long a change there goes unseen. `None`
/// when the repository's directories cannot be found, which leaves the folder unremembered.
fn ignore_sources(top: &Path, dir: &Path) -> Option<Vec<Stamp>> {
    let (git_dir, common_dir) = git_dirs(top)?;
    let mut sources = vec![
        stamp(&git_dir.join("index")),
        stamp(&common_dir.join("info/exclude")),
        stamp(&top.join(".gitignore")),
    ];
    let mut level = top.to_path_buf();
    for component in dir.strip_prefix(top).ok()?.components() {
        level.push(component);
        sources.push(stamp(&level.join(".gitignore")));
    }
    Some(sources)
}

/// A digest of the entries' names and kinds, which is all else that decides whether they are
/// ignored.
fn names_digest(names: &[Vec<u8>], entries: &[BrowseEntry]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (name, entry) in names.iter().zip(entries) {
        name.hash(&mut hasher);
        (entry.kind as u8).hash(&mut hasher);
    }
    hasher.finish()
}

/// Whether `git` has a repository to be asked about for `dir`, found on the filesystem alone.
fn repository_top(dir: &Path, git: &Git) -> Option<PathBuf> {
    source::find_dot_git(dir, git.ceiling())
}

/// Sets [`BrowseEntry::ignored`] on the entries of the listing of `dir` (canonical) that Git
/// ignores.
pub fn mark_listing(
    git: &Git,
    dir: &Path,
    entries: &mut [BrowseEntry],
    cancel: &AtomicBool,
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let Some(top) = repository_top(dir, git) else {
        return Ok(());
    };
    let names: Vec<Vec<u8>> = entries
        .iter()
        .map(|entry| wire_path::decode(&entry.name).unwrap_or_default())
        .collect();
    let digest = names_digest(&names, entries);
    // Read before `git` runs, so a source changed meanwhile fails the next comparison.
    let sources = ignore_sources(&top, dir);
    if let Some(sources) = &sources {
        let cache = remembered();
        if let Some(known) = cache.get(dir) {
            if known.names == digest
                && known.sources == *sources
                && known.at.elapsed() < budget::IGNORE_CACHE_TTL
            {
                for (entry, &ignored) in entries.iter_mut().zip(&known.ignored) {
                    entry.ignored = ignored;
                }
                return Ok(());
            }
        }
    }
    let env = match git.env() {
        Ok(env) => env,
        Err(err) => {
            tracing::debug!(%err, "ignored entries are unknown: no git environment");
            return Ok(());
        }
    };
    let Some(ignored) = check_ignore(env, dir, &names, cancel)? else {
        return Ok(());
    };
    for (entry, &ignored) in entries.iter_mut().zip(&ignored) {
        entry.ignored = ignored;
    }
    if let Some(sources) = sources {
        let mut cache = remembered();
        if cache.len() >= budget::IGNORE_CACHE_FOLDERS && !cache.contains_key(dir) {
            let oldest = cache
                .iter()
                .min_by_key(|(_, known)| known.at)
                .map(|(path, _)| path.clone());
            if let Some(oldest) = oldest {
                cache.remove(&oldest);
            }
        }
        cache.insert(
            dir.to_path_buf(),
            Remembered {
                names: digest,
                sources,
                at: Instant::now(),
                ignored,
            },
        );
    }
    Ok(())
}

/// The ranges of `given` that one run may be given.
fn batches(given: &[Vec<u8>]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < given.len() {
        let mut end = start;
        let mut bytes = 0;
        while end < given.len()
            && end - start < CHECK_BATCH_NAMES
            && (end == start || bytes + given[end].len() <= CHECK_BATCH_BYTES)
        {
            bytes += given[end].len();
            end += 1;
        }
        ranges.push(start..end);
        start = end;
    }
    ranges
}

/// Whether `name` holds a character that a pathspec reads as a pattern.
fn has_glob(name: &[u8]) -> bool {
    name.iter()
        .any(|byte| matches!(byte, b'*' | b'?' | b'[' | b'\\'))
}

/// Which of `names` (the entries of `dir`) Git ignores, in their order; `None` when `git` could
/// not say.
///
/// `check-ignore` takes each argument as a pathspec too, and matches it against the index before
/// the rules, so a name holding a glob character can be answered for by a tracked sibling that
/// the pattern matches (a file named `*.txt`, and a tracked `x.txt`). Escaping the character does
/// not help, as the rules are then matched against the escaped text. Such names are judged by the
/// rules alone (`--no-index`), and the ones that are tracked are taken out afterwards with a
/// `git ls-files` that reads its arguments literally.
fn check_ignore(
    env: &GitEnv,
    dir: &Path,
    names: &[Vec<u8>],
    cancel: &AtomicBool,
) -> Result<Option<Vec<bool>>> {
    let deadline = Instant::now() + budget::IGNORE_CHECK_TIMEOUT;
    // `./` keeps a name that starts with `:(` from being read as pathspec magic, which
    // `check-ignore` refuses.
    let given = |name: &Vec<u8>| [b"./".as_slice(), name].concat();
    let plain: Vec<usize> = (0..names.len())
        .filter(|&at| !has_glob(&names[at]))
        .collect();
    let globbed: Vec<usize> = (0..names.len())
        .filter(|&at| has_glob(&names[at]))
        .collect();
    let mut ignored = vec![false; names.len()];
    let plain_given: Vec<Vec<u8>> = plain.iter().map(|&at| given(&names[at])).collect();
    let Some(verdicts) = check_batches(env, dir, &[], &plain_given, deadline, cancel)? else {
        return Ok(None);
    };
    for (&at, verdict) in plain.iter().zip(verdicts) {
        ignored[at] = verdict;
    }
    if globbed.is_empty() {
        return Ok(Some(ignored));
    }
    let globbed_given: Vec<Vec<u8>> = globbed.iter().map(|&at| given(&names[at])).collect();
    let Some(verdicts) =
        check_batches(env, dir, &[b"--no-index"], &globbed_given, deadline, cancel)?
    else {
        return Ok(None);
    };
    let Some(tracked) = tracked_among(env, dir, &globbed_given, deadline, cancel)? else {
        return Ok(None);
    };
    for ((&at, verdict), given) in globbed.iter().zip(verdicts).zip(&globbed_given) {
        ignored[at] = verdict && !tracked.contains(given.as_slice());
    }
    Ok(Some(ignored))
}

/// The time left until `deadline`, if any.
fn left_until(deadline: Instant) -> Option<std::time::Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
}

/// `git check-ignore -v -n <flags> -- <given>` in batches: whether each of `given` is ignored, in
/// order; `None` when `git` could not say.
fn check_batches(
    env: &GitEnv,
    dir: &Path,
    flags: &[&[u8]],
    given: &[Vec<u8>],
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<Option<Vec<bool>>> {
    let mut ignored = Vec::with_capacity(given.len());
    for range in batches(given) {
        let Some(left) = left_until(deadline) else {
            tracing::debug!("ignored entries are unknown: check-ignore ran out of time");
            return Ok(None);
        };
        // `-v -n` answers once for every path, in the order given, so the answer is read by
        // position and never by comparing the names `git` prints back.
        let mut args: Vec<&[u8]> = vec![b"-v", b"-n"];
        args.extend_from_slice(flags);
        args.push(b"--");
        args.extend(given[range.clone()].iter().map(Vec::as_slice));
        let mut command = env.command_configured(dir, &[NAMES_AS_STORED], "check-ignore", &args);
        // `check-ignore` refuses the literal-pathspec mode the command is otherwise run in.
        command.env_remove("GIT_LITERAL_PATHSPECS");
        match env.run_command(&mut command, budget::GIT_METADATA_STDOUT, left, cancel) {
            Ok(stdout) => match verdicts(&stdout, range.len()) {
                Some(batch) => ignored.extend(batch),
                None => {
                    tracing::debug!("ignored entries are unknown: check-ignore answered oddly");
                    return Ok(None);
                }
            },
            // Exit status 1 is `check-ignore` saying none of the paths is ignored.
            Err(GitError::Failed(failed)) if failed.code == Some(1) => {
                ignored.extend(std::iter::repeat_n(false, range.len()));
            }
            Err(GitError::Stopped(BoundedFailure::Cancelled)) => {
                return Err(anyhow::Error::new(Cancelled));
            }
            Err(err) => {
                tracing::debug!(%err, "ignored entries are unknown: check-ignore failed");
                return Ok(None);
            }
        }
    }
    Ok(Some(ignored))
}

/// Those of `given` (`./name`) that the index holds as files, found with `git ls-files --cached`
/// whose arguments are literal paths; `None` when `git` could not say.
fn tracked_among(
    env: &GitEnv,
    dir: &Path,
    given: &[Vec<u8>],
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<Option<HashSet<Vec<u8>>>> {
    let mut tracked = HashSet::new();
    for range in batches(given) {
        let Some(left) = left_until(deadline) else {
            tracing::debug!("ignored entries are unknown: ls-files ran out of time");
            return Ok(None);
        };
        let mut args: Vec<&[u8]> = vec![b"-z", b"--cached", b"--"];
        args.extend(given[range].iter().map(Vec::as_slice));
        let mut command = env.command_configured(dir, &[NAMES_AS_STORED], "ls-files", &args);
        match env.run_command(&mut command, budget::GIT_METADATA_STDOUT, left, cancel) {
            // The paths come relative to `dir`: a name for a file, `name/...` for what a folder of
            // that name holds. Either tracks the entry, so its first component is kept, as
            // `./name`.
            Ok(stdout) => tracked.extend(
                stdout
                    .split(|&byte| byte == 0)
                    .filter(|path| !path.is_empty())
                    .map(|path| {
                        let first = path.split(|&byte| byte == b'/').next().unwrap_or(path);
                        [b"./".as_slice(), first].concat()
                    }),
            ),
            Err(GitError::Stopped(BoundedFailure::Cancelled)) => {
                return Err(anyhow::Error::new(Cancelled));
            }
            Err(err) => {
                tracing::debug!(%err, "ignored entries are unknown: ls-files failed");
                return Ok(None);
            }
        }
    }
    Ok(Some(tracked))
}

/// The `count` answers of a `check-ignore -v -n`, a line for each path: `<source>:<line>:<pattern>`
/// and a tab before the path, or `::` for a path no pattern matches. A pattern that starts with
/// `!` matched the path and un-ignored it. `None` when there are not `count` lines.
fn verdicts(stdout: &[u8], count: usize) -> Option<Vec<bool>> {
    let lines: Vec<&[u8]> = stdout
        .strip_suffix(b"\n")?
        .split(|&byte| byte == b'\n')
        .collect();
    if lines.len() != count {
        return None;
    }
    Some(lines.into_iter().map(ignores).collect())
}

fn ignores(line: &[u8]) -> bool {
    // The path comes last and is quoted when it holds a tab, which the source and the pattern may.
    let Some(tab) = line.iter().rposition(|&byte| byte == b'\t') else {
        return false;
    };
    let matched = &line[..tab];
    // The pattern starts after the first `:<digits>:`; a source path that holds one itself (a
    // global excludes file in a folder named like `a:1:b`) is misread, and the entry then counts
    // as not ignored.
    let pattern = (0..matched.len()).find_map(|at| {
        if matched[at] != b':' {
            return None;
        }
        let digits = matched[at + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        (digits > 0 && matched.get(at + 1 + digits) == Some(&b':'))
            .then(|| &matched[at + 2 + digits..])
    });
    pattern.is_some_and(|pattern| !pattern.starts_with(b"!"))
}

/// The ignored paths below a search's scope directory, and below the repositories nested in it.
///
/// A path is held relative to the scope, as raw bytes: an untracked ignored file, or an ignored
/// folder as one path without what is inside it. A folder that is a repository of its own (a
/// submodule, a clone inside the project) is a boundary: what lies below it is judged by its own
/// rules, as the listing of a folder there is, and not by those of the repository around it.
pub struct IgnoredPaths<'a> {
    git: &'a Git<'a>,
    deadline: Instant,
    paths: HashSet<Vec<u8>>,
    repositories: HashSet<Vec<u8>>,
    stdout_bytes: usize,
}

impl<'a> IgnoredPaths<'a> {
    /// An empty set, whose `git` runs must all end by `deadline`.
    pub fn new(git: &'a Git<'a>, deadline: Instant) -> Self {
        Self {
            git,
            deadline,
            paths: HashSet::new(),
            repositories: HashSet::new(),
            stdout_bytes: 0,
        }
    }

    /// Whether Git ignores `path` (relative to the scope): it is in the set, or a folder above it
    /// is, without a repository of its own in between. The empty path in the set is the scope
    /// itself, which makes everything in it ignored.
    pub fn covers(&self, path: &[u8]) -> bool {
        if self.paths.contains(path) {
            return true;
        }
        let mut end = path.len();
        while let Some(slash) = path[..end].iter().rposition(|&byte| byte == b'/') {
            let above = &path[..slash];
            if self.repositories.contains(above) {
                return false;
            }
            if self.paths.contains(above) {
                return true;
            }
            end = slash;
        }
        self.paths.contains(b"".as_slice())
    }

    /// Reads the ignored paths of the repository holding `scope` (canonical), if it is in one.
    pub fn read_scope(&mut self, scope: &Path, cancel: &AtomicBool) -> Result<()> {
        if repository_top(scope, self.git).is_none() {
            return Ok(());
        }
        self.read_repository(scope, b"", cancel)
    }

    /// Reads the ignored paths of the repository whose root is the folder `dir` (canonical) at
    /// `at` below the scope, which the traversal has just entered, and makes it a boundary.
    pub fn enter_repository(&mut self, dir: &Path, at: &[u8], cancel: &AtomicBool) -> Result<()> {
        // Past the deadline the repository around it keeps judging what is below.
        if self.time_left().is_none() {
            tracing::debug!("ignored paths are unknown: out of time");
            return Ok(());
        }
        self.repositories.insert(at.to_vec());
        self.read_repository(dir, at, cancel)
    }

    fn time_left(&self) -> Option<std::time::Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
    }

    /// `git ls-files --others --ignored --directory` in `dir`, whose paths are `at` below the
    /// scope. (Not with `--no-empty-directory`: it keeps `git` from reporting the ignored files
    /// inside an untracked folder that also holds an unignored one.) A folder that is itself
    /// ignored makes `git` print `./`, or refuse to run from there at all; either is the whole
    /// folder ignored.
    fn read_repository(&mut self, dir: &Path, at: &[u8], cancel: &AtomicBool) -> Result<()> {
        let Some(left) = self.time_left() else {
            tracing::debug!("ignored paths are unknown: out of time");
            return Ok(());
        };
        let env = match self.git.env() {
            Ok(env) => env,
            Err(err) => {
                tracing::debug!(%err, "ignored paths are unknown: no git environment");
                return Ok(());
            }
        };
        let args: &[&[u8]] = &[
            b"-z",
            b"--others",
            b"--ignored",
            b"--exclude-standard",
            b"--directory",
        ];
        let mut command = env.command_configured(dir, &[NAMES_AS_STORED], "ls-files", args);
        let limit = budget::IGNORED_PATHS_STDOUT.saturating_sub(self.stdout_bytes);
        if limit == 0 {
            tracing::debug!("ignored paths are unknown: out of output budget");
            return Ok(());
        }
        match env.run_command(&mut command, limit, left, cancel) {
            Ok(stdout) => {
                self.charge(&stdout, at);
                self.add(&stdout, at);
            }
            Err(GitError::Stopped(BoundedFailure::Cancelled)) => {
                return Err(anyhow::Error::new(Cancelled));
            }
            Err(GitError::Failed(_)) => {
                if is_itself_ignored(env, dir, self.time_left(), cancel)? {
                    self.paths.insert(at.to_vec());
                }
            }
            Err(err) => tracing::debug!(%err, "ignored paths are unknown: ls-files failed"),
        }
        Ok(())
    }

    /// Counts a run's output toward the budget, and the prefix every path of a nested repository
    /// gets, which is held too.
    fn charge(&mut self, stdout: &[u8], at: &[u8]) {
        let prefix = if at.is_empty() { 0 } else { at.len() + 1 };
        let found = stdout.iter().filter(|&&byte| byte == 0).count();
        self.stdout_bytes = self
            .stdout_bytes
            .saturating_add(stdout.len() + found * prefix);
    }

    fn add(&mut self, stdout: &[u8], at: &[u8]) {
        let found = stdout.iter().filter(|&&byte| byte == 0).count();
        if self.paths.len() + found > budget::MAX_IGNORED_PATHS {
            tracing::debug!("ignored paths are unknown: too many");
            return;
        }
        for path in stdout
            .split(|&byte| byte == 0)
            .filter(|path| !path.is_empty())
        {
            let path = path.strip_suffix(b"/").unwrap_or(path);
            let path = if path == b"." {
                at.to_vec()
            } else if at.is_empty() {
                path.to_vec()
            } else {
                [at, b"/", path].concat()
            };
            self.paths.insert(path);
        }
    }
}

/// Whether Git ignores the folder `dir` itself (`git check-ignore ./`), which is how a folder
/// below an ignored one is found out when `git ls-files` refuses to run in it.
fn is_itself_ignored(
    env: &GitEnv,
    dir: &Path,
    left: Option<std::time::Duration>,
    cancel: &AtomicBool,
) -> Result<bool> {
    let Some(left) = left else {
        return Ok(false);
    };
    let mut command = env.command_configured(
        dir,
        &[NAMES_AS_STORED],
        "check-ignore",
        &[b"-q".as_slice(), b"--", b"./"],
    );
    command.env_remove("GIT_LITERAL_PATHSPECS");
    match env.run_command(&mut command, budget::GIT_METADATA_STDOUT, left, cancel) {
        Ok(_) => Ok(true),
        Err(GitError::Stopped(BoundedFailure::Cancelled)) => Err(anyhow::Error::new(Cancelled)),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::{ignores, IgnoredPaths};
    use crate::browse::Git;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    #[test]
    fn a_check_ignore_line_ignores_only_for_a_pattern_that_does_not_negate() {
        assert!(ignores(b".gitignore:1:*.log\t./a.log"));
        assert!(ignores(b"sub/.gitignore:12:a:b/*\t\"./x\\ty\""));
        assert!(ignores(b"/home/u/.config/git/ignore:3:build/\t./build"));
        assert!(!ignores(b"::\t./a.log"));
        assert!(!ignores(b".gitignore:2:!keep.log\t./keep.log"));
        assert!(!ignores(b"no tab at all"));
    }

    #[test]
    fn the_output_budget_saturates_and_counts_a_prefix_only_for_a_nested_repository() {
        let git = Git::new(&|| anyhow::bail!("no git in this test"));
        let mut paths = IgnoredPaths::new(&git, Instant::now());
        paths.charge(b"a\0b\0", b"");
        assert_eq!(paths.stdout_bytes, 4);
        paths.charge(b"a\0", b"in");
        assert_eq!(paths.stdout_bytes, 4 + 2 + 3);
        paths.stdout_bytes = usize::MAX - 1;
        paths.charge(b"a\0b\0", b"inner");
        assert_eq!(paths.stdout_bytes, usize::MAX);
    }

    #[test]
    fn a_repository_found_after_the_deadline_leaves_the_judging_to_the_one_around_it() {
        let git = Git::new(&|| anyhow::bail!("no git in this test"));
        let past = Instant::now() - Duration::from_secs(1);
        let mut paths = IgnoredPaths::new(&git, past);
        paths.paths.insert(b"inner".to_vec());
        paths
            .enter_repository(
                std::path::Path::new("/nowhere"),
                b"inner",
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(paths.covers(b"inner/anything"), "no boundary was made");
    }
}
