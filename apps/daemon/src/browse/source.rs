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

use crate::browse::{budget, git_error, BrowseGit};
use crate::git::isolated::IsolatedGit;
use crate::git::layout::{admin_dir, find_dot_git};
use crate::git::run::GitError;
use crate::git::worktree_list::parse_worktree_list;
use crate::protocol::{error_code, CodedError};
use crate::wire_path::{self, RelPath};

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
        return Err(unavailable(&canonical, NOT_A_DIRECTORY, "not a directory"));
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

/// Finds the repository and worktree holding `root`, and `root`'s place in that worktree.
/// `git` is only asked for once the filesystem says there is a repository to ask about, so an
/// ordinary directory costs no subprocess and never depends on `git` being installed.
pub fn discover(root: &ProjectRoot, git: &BrowseGit, cancel: &AtomicBool) -> Result<Discovery> {
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
            Err(GitError::Exited { stderr, .. }) => Ok(Err(stderr)),
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
    env: &IsolatedGit,
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
    env: &IsolatedGit,
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
                MISSING,
                "the project's directory is not in this worktree",
            ))
        }
        Err(err) => return Err(io_failure(&candidate, &err)),
    };
    // `candidate` ends in a separator when `scope` is empty; compare components, not text.
    if !canonical.components().eq(candidate.components()) {
        return Err(unavailable(
            &candidate,
            LINK,
            "the path to the project's directory goes through a link in this worktree",
        ));
    }
    let metadata = fs::metadata(&canonical).map_err(|err| io_failure(&canonical, &err))?;
    if !metadata.is_dir() {
        return Err(unavailable(&candidate, NOT_A_DIRECTORY, "not a directory"));
    }
    Ok(ProjectRoot {
        id: file_id(&metadata),
        canonical,
    })
}

// The `cause` of a `source_unavailable`, which a client picks its remedy by: the directory is not
// there, is reached through a symbolic link where none may be, is something other than a directory,
// or could not be read for another cause, which only `detail` names.
const MISSING: &str = "missing";
const LINK: &str = "link";
const NOT_A_DIRECTORY: &str = "not_a_directory";
const OTHER: &str = "other";

/// A source directory that cannot be read from, for the `cause` given. Its `path` is absolute, as a
/// wire path.
fn unavailable(path: &Path, cause: &str, detail: &str) -> anyhow::Error {
    let shown = wire_path::encode(path.as_os_str().as_bytes());
    CodedError::raised(
        error_code::SOURCE_UNAVAILABLE,
        format!("{shown} is unavailable: {detail}"),
        &[("path", &shown), ("cause", cause), ("detail", detail)],
    )
}

/// Why a source directory was refused with `err`: the `cause` `source_unavailable` gives,
/// `permission_denied` for a read the system denied, and `other` for any other refusal.
pub fn unavailable_cause(err: &anyhow::Error) -> &'static str {
    let Some(coded) = err.downcast_ref::<CodedError>() else {
        return OTHER;
    };
    if coded.code == error_code::PERMISSION_DENIED {
        return error_code::PERMISSION_DENIED.as_str();
    }
    if coded.code != error_code::SOURCE_UNAVAILABLE {
        return OTHER;
    }
    match coded.params.get("cause").map(String::as_str) {
        Some(MISSING) => MISSING,
        Some(LINK) => LINK,
        Some(NOT_A_DIRECTORY) => NOT_A_DIRECTORY,
        _ => OTHER,
    }
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
    let cause = if err.kind() == io::ErrorKind::NotFound {
        MISSING
    } else {
        OTHER
    };
    unavailable(path, cause, &err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_scope_is_told_by_its_cause_a_denied_read_apart_and_any_other_refusal_is_other() {
        let path = Path::new("/nowhere");
        assert_eq!(unavailable_cause(&unavailable(path, MISSING, "x")), MISSING);
        assert_eq!(unavailable_cause(&unavailable(path, LINK, "x")), LINK);
        assert_eq!(
            unavailable_cause(&unavailable(path, NOT_A_DIRECTORY, "x")),
            NOT_A_DIRECTORY
        );
        assert_eq!(unavailable_cause(&unavailable(path, OTHER, "x")), OTHER);
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(
            unavailable_cause(&io_failure(path, &denied)),
            error_code::PERMISSION_DENIED
        );
        assert_eq!(unavailable_cause(&anyhow::anyhow!("not coded")), OTHER);
    }
}
