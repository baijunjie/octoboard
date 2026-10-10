//! Browsing a project's files and their Git versions, for the browse requests in
//! `apps/daemon/PROTOCOL.md`'s "Browsing a project": where a project's files are (`source`), the
//! files on disk (`live`), the files in Git (`blob`, through the isolated invocation in `git`), a
//! worktree's uncommitted changes and one change's diff (`changes`), the repository's local
//! branches and the changes between two of their tips (`compare`), how a path travels on the wire
//! (`wire_path`), the limits all of it is held to (`budget`), and the per-connection lane its
//! replies take back to the client (`lane`).
//!
//! Every request re-resolves its project from the store and its worktree from the repository; a
//! reply is never built from a directory a client named.

pub mod blob;
pub mod budget;
pub mod changes;
pub mod compare;
pub mod git;
pub mod lane;
pub mod live;
pub mod source;
pub mod wire_path;

#[cfg(test)]
mod body_tests;
#[cfg(test)]
mod change_tests;
#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod tests;

use std::cell::OnceCell;
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
use base64::Engine as _;

use crate::protocol::{
    error_code, BrowseBody, CodedError, ContentKind, ContentSource, Event, FileContent,
    GitSourceInfo, Project, ProjectSourceInfo, ReadFrom, WorktreeInfo,
};
use crate::state::AppState;
use crate::subprocess::BoundedFailure;
use git::{GitEnv, GitError};
use source::{Discovery, ProjectRoot, Repository};
use wire_path::RelPath;

/// The marker a read that was cancelled ends with, so the lane can tell it from a failure.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the read was cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// A `git` run that did not succeed, as what ends the request: `git_failed` with `git`'s own
/// message, `limit_exceeded` for output past its budget, or [`Cancelled`].
pub(crate) fn git_error(err: GitError) -> anyhow::Error {
    match err {
        GitError::Failed(failure) => git_failed(&failure.stderr),
        GitError::Stopped(BoundedFailure::Cancelled) => anyhow::Error::new(Cancelled),
        GitError::Stopped(BoundedFailure::StdoutExceeded(limit)) => {
            let max = limit.to_string();
            CodedError::raised(
                error_code::LIMIT_EXCEEDED,
                format!("git printed more than the {limit}-byte limit"),
                &[("limit", "git_output"), ("max", &max)],
            )
        }
        GitError::Stopped(stopped) => git_failed(&format!("git {stopped}")),
    }
}

/// `git_failed`, carrying `message` — `git`'s own, or why it did not get as far as one.
pub(crate) fn git_failed(message: &str) -> anyhow::Error {
    CodedError::raised(error_code::GIT_FAILED, message, &[("detail", message)])
}

/// The `git` environment of one request, built the first time the request needs it and reused
/// after that, so a request that never reaches `git` never takes the shell snapshot either.
pub struct Git<'a> {
    make: &'a dyn Fn() -> Result<GitEnv>,
    env: OnceCell<GitEnv>,
    ceiling: Option<std::path::PathBuf>,
}

impl<'a> Git<'a> {
    pub fn new(make: &'a dyn Fn() -> Result<GitEnv>) -> Self {
        Self {
            make,
            env: OnceCell::new(),
            ceiling: source::home_ceiling(),
        }
    }

    /// The directory repository discovery never climbs into; see `source::find_dot_git`.
    pub fn ceiling(&self) -> Option<&std::path::Path> {
        self.ceiling.as_deref()
    }

    #[cfg(test)]
    pub fn with_ceiling(mut self, ceiling: Option<std::path::PathBuf>) -> Self {
        self.ceiling = ceiling;
        self
    }

    pub fn env(&self) -> Result<&GitEnv> {
        if let Some(env) = self.env.get() {
            return Ok(env);
        }
        let env = (self.make)()?;
        Ok(self.env.get_or_init(|| env))
    }
}

/// Runs one browse request to its reply. Blocking: it reads files and waits on `git`.
pub fn serve(
    state: &AppState,
    id: Option<String>,
    body: BrowseBody,
    git: &dyn Fn() -> Result<GitEnv>,
    cancel: &AtomicBool,
) -> Result<Event> {
    let git = Git::new(git);
    match body {
        BrowseBody::GetProjectSource { project } => {
            let source = project_source(&stored_project(state, &project)?, &git, cancel)?;
            Ok(Event::ProjectSource { id, source })
        }
        BrowseBody::ListProjectDir {
            project,
            worktree,
            path,
        } => {
            let rel = parse_path(&path)?;
            let record = stored_project(state, &project)?;
            let scope = scope_for(&record, worktree.as_deref(), &git, cancel)?;
            let listing = live::list_dir(&scope.canonical, &rel, cancel)?;
            Ok(Event::ProjectDir {
                id,
                project,
                worktree,
                path,
                root_id: scope.id,
                entries: listing.entries,
                complete: listing.complete,
            })
        }
        BrowseBody::ReadProjectFile {
            project,
            worktree,
            path,
            from,
        } => {
            let rel = parse_path(&path)?;
            let record = stored_project(state, &project)?;
            let (source, bytes) = match from {
                ReadFrom::Live => {
                    let scope = scope_for(&record, worktree.as_deref(), &git, cancel)?;
                    let file =
                        live::read_file(&scope.canonical, &rel, budget::MAX_FILE_BYTES, cancel)?;
                    let source = ContentSource::Live {
                        root_id: scope.id,
                        version: file.version,
                    };
                    (source, file.bytes)
                }
                ReadFrom::Index => read_from_git(
                    &record,
                    worktree.as_deref(),
                    &rel,
                    GitRead::Index,
                    &git,
                    cancel,
                )?,
                ReadFrom::Branch { branch } => {
                    let read = GitRead::Branch(branch);
                    read_from_git(&record, worktree.as_deref(), &rel, read, &git, cancel)?
                }
                ReadFrom::Commit { commit } => {
                    let read = GitRead::Commit(commit);
                    read_from_git(&record, worktree.as_deref(), &rel, read, &git, cancel)?
                }
            };
            Ok(Event::ProjectFile {
                id,
                project,
                worktree,
                path,
                source,
                file: file_content(bytes),
            })
        }
        BrowseBody::ListProjectChanges { project, worktree } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, worktree.as_deref(), &git, cancel)?;
            let list = changes::list(git.env()?, &at, cancel)?;
            Ok(Event::ProjectChanges {
                id,
                project,
                worktree,
                head: list.head,
                changes: list.changes,
                complete: list.complete,
            })
        }
        BrowseBody::ReadProjectChange {
            project,
            worktree,
            change,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, worktree.as_deref(), &git, cancel)?;
            let read = changes::read(git.env()?, &at, &change, cancel)?;
            Ok(Event::ProjectChange {
                id,
                project,
                worktree,
                group: change.group,
                head: read.head,
                old: Box::new(read.old),
                new: Box::new(read.new),
                patch: read.patch.map(file_content),
            })
        }
        // A comparison concerns the repository's branches, not any one worktree's checkout or
        // index: it is read in the worktree holding the project's directory, which has the same
        // branches and objects as every other.
        BrowseBody::ListProjectBranches { project } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let list = compare::branches(git.env()?, &at, cancel)?;
            Ok(Event::ProjectBranches {
                id,
                project,
                branches: list.branches,
                complete: list.complete,
            })
        }
        BrowseBody::CompareProjectBranches {
            project,
            left,
            right,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let comparison = compare::list(git.env()?, &at, &left, &right, cancel)?;
            Ok(Event::ProjectComparison {
                id,
                project,
                left: comparison.left,
                right: comparison.right,
                changes: comparison.changes,
                complete: comparison.complete,
            })
        }
        BrowseBody::ReadProjectComparisonChange {
            project,
            left,
            right,
            change,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let read = compare::read(git.env()?, &at, &left, &right, &change, cancel)?;
            Ok(Event::ProjectComparisonChange {
                id,
                project,
                left,
                right,
                old: Box::new(read.old),
                new: Box::new(read.new),
                patch: read.patch.map(file_content),
            })
        }
        BrowseBody::ReadProjectChangeBodies {
            project,
            worktree,
            change,
            old,
            new,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, worktree.as_deref(), &git, cancel)?;
            let expected = changes::Expected {
                old: &old,
                new: &new,
            };
            let read = changes::read_bodies(git.env()?, &at, &change, &expected, cancel)?;
            Ok(Event::ProjectChangeBodies {
                id,
                project,
                worktree,
                group: change.group,
                old: Box::new(read.old),
                new: Box::new(read.new),
            })
        }
        BrowseBody::ReadProjectComparisonChangeBodies {
            project,
            left,
            right,
            change,
            old,
            new,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let expected = changes::Expected {
                old: &old,
                new: &new,
            };
            let read =
                compare::read_bodies(git.env()?, &at, &left, &right, &change, &expected, cancel)?;
            Ok(Event::ProjectComparisonChangeBodies {
                id,
                project,
                left,
                right,
                old: Box::new(read.old),
                new: Box::new(read.new),
            })
        }
    }
}

fn parse_path(path: &str) -> Result<RelPath> {
    RelPath::parse(path).ok_or_else(|| {
        CodedError::raised(
            error_code::INVALID_PATH,
            format!("{path} is not a path inside a project"),
            &[("path", path)],
        )
    })
}

fn stored_project(state: &AppState, project: &str) -> Result<Project> {
    state
        .store
        .get_project(project)?
        .ok_or_else(|| CodedError::unknown_project(project))
}

fn project_source(record: &Project, git: &Git, cancel: &AtomicBool) -> Result<ProjectSourceInfo> {
    let root = source::resolve_root(&record.path)?;
    let mut info = ProjectSourceInfo {
        project: record.id.clone(),
        root: wire_path::encode(record.path.as_bytes()),
        resolved_root: wire_path::encode(root.canonical.as_os_str().as_bytes()),
        root_id: root.id.clone(),
        git: None,
        git_error: None,
    };
    let repository = match source::discover(&root, git, cancel)? {
        Discovery::NotARepository => return Ok(info),
        Discovery::Unreadable(message) => {
            info.git_error = Some(message);
            return Ok(info);
        }
        Discovery::Repository(repository) => repository,
    };
    // A repository whose worktrees cannot be listed is reported like one `git` cannot read at
    // all: the files can still be browsed, and Git review shows the reason it is unavailable.
    let worktrees = match source::worktrees(git.env()?, &repository, cancel) {
        Ok(worktrees) => worktrees,
        Err(err) if err.downcast_ref::<CodedError>().is_some() => {
            info.git_error = Some(format!("{err:#}"));
            return Ok(info);
        }
        Err(err) => return Err(err),
    };
    let worktrees = worktrees
        .into_iter()
        .map(|worktree| WorktreeInfo {
            scope_present: source::scope_root(&worktree.root, &repository.scope).is_ok(),
            id: worktree.id,
            root: wire_path::encode(worktree.root.as_os_str().as_bytes()),
            main: worktree.main,
            head: worktree.head,
            branch: worktree.branch.as_deref().map(wire_path::encode),
        })
        .collect();
    info.git = Some(GitSourceInfo {
        repository: repository.id,
        common_dir: wire_path::encode(repository.common_dir.as_os_str().as_bytes()),
        worktree: repository.worktree,
        scope: repository.scope.to_wire(),
        worktrees,
    });
    Ok(info)
}

/// The project's repository, refusing a project that is in none.
fn repository(
    root: &ProjectRoot,
    project: &str,
    git: &Git,
    cancel: &AtomicBool,
) -> Result<Repository> {
    match source::discover(root, git, cancel)? {
        Discovery::Repository(repository) => Ok(repository),
        Discovery::NotARepository => Err(CodedError::raised(
            error_code::NOT_A_GIT_REPOSITORY,
            "the project is not in a git repository",
            &[("project", project)],
        )),
        Discovery::Unreadable(message) => Err(CodedError::raised(
            error_code::GIT_UNAVAILABLE,
            message.clone(),
            &[("detail", &message)],
        )),
    }
}

/// The directory a live read or listing is scoped to: the project's own, or — with `worktree` —
/// the same place in that worktree of the project's repository, revalidated now.
fn scope_for(
    record: &Project,
    worktree: Option<&str>,
    git: &Git,
    cancel: &AtomicBool,
) -> Result<ProjectRoot> {
    let root = source::resolve_root(&record.path)?;
    let Some(worktree) = worktree else {
        return Ok(root);
    };
    let repository = repository(&root, &record.id, git, cancel)?;
    let found = source::find_worktree(git.env()?, &repository, worktree, cancel)?;
    source::scope_root(&found.root, &repository.scope)
}

/// The repository a Git read of a project works in and the worktree it reads: the one holding the
/// project's directory, or — with `worktree` — that worktree of the same repository, checked
/// against it now.
pub struct Located {
    pub repository: Repository,
    /// The worktree's id, as `project_source` reports it.
    pub worktree: String,
    /// The worktree's root directory.
    pub root: std::path::PathBuf,
}

fn locate(
    record: &Project,
    worktree: Option<&str>,
    git: &Git,
    cancel: &AtomicBool,
) -> Result<Located> {
    let root = source::resolve_root(&record.path)?;
    let repository = repository(&root, &record.id, git, cancel)?;
    let (worktree, root) = match worktree {
        None => (
            repository.worktree.clone(),
            repository.worktree_root.clone(),
        ),
        Some(id) => {
            let found = source::find_worktree(git.env()?, &repository, id, cancel)?;
            (found.id, found.root)
        }
    };
    Ok(Located {
        repository,
        worktree,
        root,
    })
}

/// The sources `read_from_git` reads: [`ReadFrom`] without the file on disk.
enum GitRead {
    Index,
    Branch(String),
    Commit(String),
}

/// Reads `path` from the index or a commit, returning the source identity the bytes came from.
fn read_from_git(
    record: &Project,
    worktree: Option<&str>,
    path: &RelPath,
    from: GitRead,
    git: &Git,
    cancel: &AtomicBool,
) -> Result<(ContentSource, Vec<u8>)> {
    // The project's own directory is no file in any source; asking `git` about it would pass an
    // empty pathspec, which `git` refuses as an error of its own.
    if path.is_root() {
        return Err(live::unsupported(path, "directory"));
    }
    let Located {
        repository,
        worktree: worktree_id,
        root: worktree_root,
    } = locate(record, worktree, git, cancel)?;
    let env = git.env()?;
    let entry = blob::Entry {
        scope: &repository.scope,
        path,
    };
    let (source, blob) = match from {
        GitRead::Index => {
            let blob = blob::read_index(env, &worktree_root, entry, cancel)?;
            let source = ContentSource::Index {
                worktree: worktree_id,
                blob: blob.oid.clone(),
            };
            (source, blob)
        }
        GitRead::Branch(branch) => {
            let commit = blob::resolve_branch(env, &worktree_root, &branch, cancel)?;
            let blob = blob::read_commit(env, &worktree_root, &commit, entry, cancel)?;
            let source = ContentSource::Commit {
                commit,
                branch: Some(branch),
                blob: blob.oid.clone(),
            };
            (source, blob)
        }
        GitRead::Commit(commit) => {
            blob::verify_commit(env, &worktree_root, &commit, cancel)?;
            let blob = blob::read_commit(env, &worktree_root, &commit, entry, cancel)?;
            let source = ContentSource::Commit {
                commit,
                branch: None,
                blob: blob.oid.clone(),
            };
            (source, blob)
        }
    };
    Ok((source, blob.bytes))
}

/// How long `text` becomes once JSON-escaped: `"`, `\` and the five control characters JSON has a
/// short escape for take two bytes, every other control character six (`\u00XX`).
pub(crate) fn json_escaped_len(text: &[u8]) -> usize {
    text.iter()
        .map(|&byte| match byte {
            b'"' | b'\\' | 0x08 | 0x09 | 0x0a | 0x0c | 0x0d => 2,
            0x00..=0x1f => 6,
            _ => 1,
        })
        .sum()
}

/// A body as the wire carries it. Text is valid UTF-8 with no NUL whose JSON escaping at most
/// doubles it — so a file of control characters, which escaping would sextuple, travels as
/// binary — and is sent as a string; anything else is base64-encoded and never decoded as text
/// here. That doubling is the ceiling the reply's memory reservation is sized by.
pub(crate) fn file_content(bytes: Vec<u8>) -> FileContent {
    let size = bytes.len() as u64;
    let textual = !bytes.contains(&0) && json_escaped_len(&bytes) <= 2 * bytes.len();
    let bytes = if textual {
        match String::from_utf8(bytes) {
            Ok(text) => {
                return FileContent {
                    size,
                    kind: ContentKind::Text,
                    media_type: None,
                    text: Some(text),
                    data: None,
                }
            }
            Err(err) => err.into_bytes(),
        }
    } else {
        bytes
    };
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    FileContent {
        size,
        kind: ContentKind::Binary,
        media_type: sniff_image(&bytes),
        text: None,
        data: Some(data),
    }
}

/// The image type `bytes` starts like, by its magic number.
fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if starts(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if starts(b"BM") {
        Some("image/bmp")
    } else if starts(b"\x00\x00\x01\x00") {
        Some("image/x-icon")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        Some("image/avif")
    } else {
        None
    }
}
