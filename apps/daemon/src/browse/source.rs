//! Resolving a project to the directories a browse read may touch: the project's own directory
//! with its symbolic links followed, the repository and worktree holding it when there is one, the
//! project's place inside that worktree, and the same place in each of the repository's other
//! worktrees. Every identity handed to a client comes out of here and every one a client hands back
//! is checked here again before anything is read; nothing a client sends is ever used as a
//! directory.

use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::git::{GitEnv, GitError};
use crate::browse::wire_path::{self, RelPath};
use crate::browse::{budget, git_error, Git};
use crate::protocol::{error_code, CodedError};

/// A directory's or a file's identity on its filesystem, as sent on the wire: its device and inode
/// numbers, and its creation time where the filesystem records one. It survives a rename, and
/// changes when the thing is removed and another put in its place — the creation time is what
/// tells the two apart on a filesystem that hands a freed inode number straight out again.
pub fn file_id(metadata: &fs::Metadata) -> String {
    let born = metadata
        .created()
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok());
    match born {
        Some(born) => format!("{}:{}:{}", metadata.dev(), metadata.ino(), born.as_nanos()),
        None => format!("{}:{}", metadata.dev(), metadata.ino()),
    }
}

/// A directory reads are held inside, as it resolves now.
#[derive(Debug, Clone)]
pub struct ProjectRoot {
    pub canonical: PathBuf,
    pub id: String,
}

/// Resolves the project's stored path, following symbolic links, to a directory that exists.
pub fn resolve_root(path: &str) -> Result<ProjectRoot> {
    let canonical = fs::canonicalize(path).map_err(|err| io_failure(Path::new(path), &err))?;
    let metadata = fs::metadata(&canonical).map_err(|err| io_failure(&canonical, &err))?;
    if !metadata.is_dir() {
        return Err(unavailable(&canonical, "not a directory"));
    }
    Ok(ProjectRoot {
        id: file_id(&metadata),
        canonical,
    })
}

/// The repository holding a project's directory.
#[derive(Debug, Clone)]
pub struct Repository {
    pub common_dir: PathBuf,
    pub id: String,
    /// The worktree holding the project's directory.
    pub worktree: String,
    pub worktree_root: PathBuf,
    /// The project's directory relative to `worktree_root`.
    pub scope: RelPath,
}

/// What discovery found for a project's directory.
#[derive(Debug)]
pub enum Discovery {
    /// No `.git` anywhere from the directory up to the ceiling: an ordinary directory.
    NotARepository,
    Repository(Repository),
    /// There is a `.git`, but `git` could not read the repository; the message is its own.
    Unreadable(String),
}

/// The directory discovery never climbs into: the home directory, resolved the way the paths it is
/// compared with are. See [`find_dot_git`].
pub fn home_ceiling() -> Option<PathBuf> {
    crate::paths::known_home_dir().and_then(|home| fs::canonicalize(home).ok())
}

/// The nearest directory from `start` upwards holding a `.git` entry (a directory, or the file a
/// linked worktree or a submodule has), stopping below `ceiling` the way `git` does for
/// `GIT_CEILING_DIRECTORIES`: `start` itself is always looked at, and the climb never steps into
/// `ceiling`.
///
/// The ceiling is the home directory. Without it, a home directory kept under version control —
/// a dotfiles repository — would make every ordinary project under it look like a subdirectory of
/// that repository, and its Git review would show the whole home directory's changes.
pub fn find_dot_git(start: &Path, ceiling: Option<&Path>) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if fs::symlink_metadata(dir.join(".git")).is_ok() {
            return Some(dir.to_path_buf());
        }
        let parent = dir.parent()?;
        if Some(parent) == ceiling {
            return None;
        }
        dir = parent;
    }
}

/// Finds the repository and worktree holding `root`, and `root`'s place in that worktree.
/// `git` is only asked for once the filesystem says there is a repository to ask about, so an
/// ordinary directory costs no subprocess and never depends on `git` being installed.
pub fn discover(root: &ProjectRoot, git: &Git, cancel: &AtomicBool) -> Result<Discovery> {
    if find_dot_git(&root.canonical, git.ceiling()).is_none() {
        return Ok(Discovery::NotARepository);
    }
    let env = match git.env() {
        Ok(env) => env,
        Err(err) => return Ok(Discovery::Unreadable(format!("{err:#}"))),
    };
    let rev_parse = |flag: &str| -> Result<std::result::Result<PathBuf, String>> {
        let args: &[&[u8]] = &[b"--path-format=absolute", flag.as_bytes()];
        let limit = budget::GIT_METADATA_STDOUT;
        match env.run(&root.canonical, "rev-parse", args, limit, cancel) {
            Ok(stdout) => {
                // One value per call, so the single trailing newline is the only one that is not
                // part of the path.
                let path = stdout.strip_suffix(b"\n").unwrap_or(&stdout);
                let path = Path::new(std::ffi::OsStr::from_bytes(path));
                Ok(fs::canonicalize(path).map_err(|err| format!("{}: {err}", path.display())))
            }
            Err(GitError::Failed(failure)) => Ok(Err(failure.stderr)),
            Err(err) => Err(git_error(err)),
        }
    };
    let toplevel = match rev_parse("--show-toplevel")? {
        Ok(path) => path,
        Err(message) => return Ok(Discovery::Unreadable(message)),
    };
    let common_dir = match rev_parse("--git-common-dir")? {
        Ok(path) => path,
        Err(message) => return Ok(Discovery::Unreadable(message)),
    };
    let git_dir = match rev_parse("--git-dir")? {
        Ok(path) => path,
        Err(message) => return Ok(Discovery::Unreadable(message)),
    };
    let Some(scope) = root
        .canonical
        .strip_prefix(&toplevel)
        .ok()
        .and_then(|scope| RelPath::from_bytes(scope.as_os_str().as_bytes().to_vec()))
    else {
        return Ok(Discovery::Unreadable(format!(
            "{} is not inside the worktree at {}",
            root.canonical.display(),
            toplevel.display()
        )));
    };
    let common = fs::metadata(&common_dir).map_err(|err| io_failure(&common_dir, &err))?;
    let admin = fs::metadata(&git_dir).map_err(|err| io_failure(&git_dir, &err))?;
    Ok(Discovery::Repository(Repository {
        id: file_id(&common),
        common_dir,
        worktree: file_id(&admin),
        worktree_root: toplevel,
        scope,
    }))
}

/// One worktree of a repository, as it stands now.
#[derive(Debug, Clone)]
pub struct Worktree {
    /// The identity of the worktree's own git directory: the common directory for the main
    /// worktree, `<common>/worktrees/<name>` for a linked one. A worktree removed and added again
    /// at the same path gets a new one.
    pub id: String,
    pub root: PathBuf,
    pub main: bool,
    pub head: Option<String>,
    /// The checked-out branch, below `refs/heads/`, as raw bytes.
    pub branch: Option<Vec<u8>>,
}

/// The repository's worktrees that are there and still belong to it. A worktree whose directory
/// has gone, or whose `.git` no longer leads back to this repository (its path reused by another
/// checkout), is left out, so an id that named it no longer resolves.
pub fn worktrees(
    env: &GitEnv,
    repository: &Repository,
    cancel: &AtomicBool,
) -> Result<Vec<Worktree>> {
    let stdout = env
        .run(
            &repository.worktree_root,
            "worktree",
            &[b"list", b"--porcelain", b"-z"],
            budget::GIT_METADATA_STDOUT,
            cancel,
        )
        .map_err(git_error)?;
    let linked_parent = repository.common_dir.join("worktrees");
    let mut found = Vec::new();
    for record in parse_worktree_list(&stdout) {
        if record.bare || record.prunable {
            continue;
        }
        let path = Path::new(std::ffi::OsStr::from_bytes(&record.path));
        let Ok(root) = fs::canonicalize(path) else {
            continue;
        };
        let Some(admin) = admin_dir(&root) else {
            continue;
        };
        let main = admin == repository.common_dir;
        if !main && admin.parent() != Some(linked_parent.as_path()) {
            continue;
        }
        let Ok(metadata) = fs::metadata(&admin) else {
            continue;
        };
        found.push(Worktree {
            id: file_id(&metadata),
            root,
            main,
            head: record.head,
            branch: record.branch,
        });
    }
    Ok(found)
}

/// The worktree `id` names, checked against the repository as it stands now.
pub fn find_worktree(
    env: &GitEnv,
    repository: &Repository,
    id: &str,
    cancel: &AtomicBool,
) -> Result<Worktree> {
    worktrees(env, repository, cancel)?
        .into_iter()
        .find(|worktree| worktree.id == id)
        .ok_or_else(|| {
            CodedError::raised(
                error_code::WORKTREE_UNAVAILABLE,
                format!("worktree {id} is not a worktree of this repository any more"),
                &[("worktree", id)],
            )
        })
}

/// The git directory a worktree's `.git` leads to: the directory itself, or the one a `.git` file
/// names (relative to the worktree when it is written relative).
fn admin_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    let metadata = fs::symlink_metadata(&dot_git).ok()?;
    if metadata.is_dir() {
        return fs::canonicalize(&dot_git).ok();
    }
    let contents = fs::read(&dot_git).ok()?;
    let pointer = contents.strip_prefix(b"gitdir: ")?;
    let pointer = pointer.strip_suffix(b"\n").unwrap_or(pointer);
    fs::canonicalize(root.join(std::ffi::OsStr::from_bytes(pointer))).ok()
}

/// One record of `git worktree list --porcelain -z`.
#[derive(Debug, Default, PartialEq, Eq)]
struct WorktreeRecord {
    path: Vec<u8>,
    head: Option<String>,
    branch: Option<Vec<u8>>,
    bare: bool,
    prunable: bool,
}

/// Parses `git worktree list --porcelain -z`: NUL-terminated attribute lines, each record ended
/// by an empty one. Paths are taken as bytes, so a worktree path holding a newline is read whole.
fn parse_worktree_list(stdout: &[u8]) -> Vec<WorktreeRecord> {
    let mut records = Vec::new();
    let mut current: Option<WorktreeRecord> = None;
    for field in stdout.split(|&b| b == 0) {
        if field.is_empty() {
            records.extend(current.take());
            continue;
        }
        let (key, value) = match field.iter().position(|&b| b == b' ') {
            Some(at) => (&field[..at], Some(&field[at + 1..])),
            None => (field, None),
        };
        if key == b"worktree" {
            records.extend(current.take());
            current = Some(WorktreeRecord {
                path: value.unwrap_or_default().to_vec(),
                ..WorktreeRecord::default()
            });
            continue;
        }
        let Some(record) = current.as_mut() else {
            continue;
        };
        match key {
            b"HEAD" => {
                record.head = value
                    .map(|oid| String::from_utf8_lossy(oid).into_owned())
                    .filter(|oid| oid.bytes().any(|b| b != b'0'));
            }
            b"branch" => {
                record.branch = value
                    .and_then(|name| name.strip_prefix(b"refs/heads/"))
                    .map(<[u8]>::to_vec);
            }
            b"bare" => record.bare = true,
            b"prunable" => record.prunable = true,
            _ => {}
        }
    }
    records.extend(current);
    records
}

/// The directory `scope` names inside the worktree at `root`, which must be exactly that: a
/// directory reached without passing through a symbolic link. A link there — `sub` pointing at
/// `.` in one worktree but not in another — would otherwise widen or move the project's scope
/// without anyone choosing it, so a scope that resolves anywhere else is unavailable in that
/// worktree.
pub fn scope_root(root: &Path, scope: &RelPath) -> Result<ProjectRoot> {
    let candidate = root.join(scope.as_path());
    let canonical = match fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(unavailable(
                &candidate,
                "the project's directory is not in this worktree",
            ))
        }
        Err(err) => return Err(io_failure(&candidate, &err)),
    };
    // `candidate` ends in a separator when `scope` is empty; compare components, not text.
    if !canonical.components().eq(candidate.components()) {
        return Err(unavailable(
            &candidate,
            "the project's directory is a link in this worktree",
        ));
    }
    let metadata = fs::metadata(&canonical).map_err(|err| io_failure(&canonical, &err))?;
    if !metadata.is_dir() {
        return Err(unavailable(&candidate, "not a directory"));
    }
    Ok(ProjectRoot {
        id: file_id(&metadata),
        canonical,
    })
}

/// A source directory that cannot be read from: gone, or not a directory any more. Its `path` is
/// absolute, as a wire path.
fn unavailable(path: &Path, detail: &str) -> anyhow::Error {
    let shown = wire_path::encode(path.as_os_str().as_bytes());
    CodedError::raised(
        error_code::SOURCE_UNAVAILABLE,
        format!("{shown} is unavailable: {detail}"),
        &[("path", &shown), ("detail", detail)],
    )
}

/// An I/O failure on a source directory, as the code a client words it by.
fn io_failure(path: &Path, err: &io::Error) -> anyhow::Error {
    let shown = wire_path::encode(path.as_os_str().as_bytes());
    if err.kind() == io::ErrorKind::PermissionDenied {
        return CodedError::raised(
            error_code::PERMISSION_DENIED,
            format!("{shown} could not be read: {err}"),
            &[("path", &shown), ("detail", &err.to_string())],
        );
    }
    unavailable(path, &err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_never_climbs_into_the_ceiling() {
        let home = crate::test_support::ScratchDir::new("source-ceiling");
        let home = fs::canonicalize(&*home).unwrap();
        fs::create_dir_all(home.join(".git")).unwrap();
        fs::create_dir_all(home.join("project/inner")).unwrap();
        assert_eq!(find_dot_git(&home.join("project/inner"), Some(&home)), None);
        assert_eq!(find_dot_git(&home, Some(&home)), Some(home.clone()));
        assert_eq!(
            find_dot_git(&home.join("project/inner"), None),
            Some(home.clone())
        );
    }

    #[test]
    fn worktree_records_are_read_as_bytes() {
        let stdout = [
            &b"worktree /r/main\0HEAD 1111111111111111111111111111111111111111\0"[..],
            b"branch refs/heads/main\0\0",
            b"worktree /r/new\nline\0HEAD 2222222222222222222222222222222222222222\0detached\0\0",
            b"worktree /r/gone\0HEAD 3333333333333333333333333333333333333333\0",
            b"branch refs/heads/caf\xe9\0prunable gitdir file points to non-existent location\0\0",
        ]
        .concat();
        let records = parse_worktree_list(&stdout);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].branch.as_deref(), Some(&b"main"[..]));
        assert_eq!(records[1].path, b"/r/new\nline");
        assert_eq!(records[1].branch, None);
        assert_eq!(records[2].branch.as_deref(), Some(&b"caf\xe9"[..]));
        assert!(records[2].prunable);
    }
}
