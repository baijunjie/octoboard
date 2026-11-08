//! Which project, directory and worktree a browse request works in: the stored project and the
//! path a request names, the repository a project is in and where it keeps its worktrees, the
//! directory a live read is scoped to (`scope_for`), the worktree a Git read works in (`locate`),
//! the Git read of one file (`read_from_git`), and the resolution of a path for a host action
//! under the same scope as live browsing.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::browse::source::{self, Discovery, ProjectRoot, Repository};
use crate::browse::{blob, live, BrowseGit};
use crate::git::isolated::IsolatedGit;
use crate::host_tools::is_git_not_found;
use crate::platform::names;
use crate::protocol::{
    error_code, CodedError, ContentSource, GitSourceInfo, Project, ProjectSourceInfo, WorktreeInfo,
};
use crate::state::AppState;
use crate::wire_path::{self, RelPath};

/// Resolves a live path for a host action under the same scope as live browsing.
pub fn resolve_project_path(
    state: &AppState,
    project: &str,
    worktree: Option<&str>,
    path: &str,
    git: &dyn Fn() -> Result<IsolatedGit>,
) -> Result<std::path::PathBuf> {
    let rel = parse_path(path)?;
    let record = stored_project(state, project)?;
    let git = BrowseGit::new(git);
    let scope = scope_for(&record, worktree, &git, &AtomicBool::new(false))?;
    live::resolve(&scope.canonical, &rel)?;
    if rel.is_root() {
        return Ok(scope.canonical);
    }
    // Keep the final entry's name so revealing a symlink selects that entry, while both its
    // target and its containing directory must stay inside the project scope.
    let lexical = scope.canonical.join(
        rel.host_path()
            .expect("a path that resolved has a name on the disk"),
    );
    let parent = rel.parent().expect("a relative entry has a parent");
    let directory = live::resolve(&scope.canonical, &parent)?;
    Ok(directory.join(lexical.file_name().expect("a non-root entry has a name")))
}

/// Resolves a live path that must be a folder, for a host action that starts something in it, to
/// the canonical folder under the same scope as live browsing. A final symbolic link is followed,
/// so what starts there starts in the folder that was checked, not wherever a link at that name
/// points later. Refuses what is not a folder with `path_not_a_directory`.
pub fn resolve_project_directory(
    state: &AppState,
    project: &str,
    worktree: Option<&str>,
    path: &str,
    git: &dyn Fn() -> Result<IsolatedGit>,
) -> Result<std::path::PathBuf> {
    let rel = parse_path(path)?;
    let record = stored_project(state, project)?;
    let git = BrowseGit::new(git);
    let scope = scope_for(&record, worktree, &git, &AtomicBool::new(false))?;
    let canonical = live::resolve(&scope.canonical, &rel)?;
    if !canonical.is_dir() {
        return Err(crate::hostfs::not_a_directory(&canonical));
    }
    Ok(canonical)
}

/// One live directory a project's paths can name: where it is, and the worktree id that reads it
/// (`worktree` of the browse requests); none for the project's own directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRoot {
    pub root: std::path::PathBuf,
    pub worktree: Option<String>,
}

/// Every live directory a project's paths can name: the project's own directory first, then its
/// place in each other worktree of its repository. A project outside a repository, or one whose
/// worktrees cannot be listed, has its own directory alone.
pub fn project_live_roots(
    state: &AppState,
    project: &str,
    git: &dyn Fn() -> Result<IsolatedGit>,
) -> Result<Vec<LiveRoot>> {
    let record = stored_project(state, project)?;
    let root = source::resolve_root(&record.path)?;
    let mut roots = vec![LiveRoot {
        root: root.canonical.clone(),
        worktree: None,
    }];
    let git = BrowseGit::new(git);
    let cancel = AtomicBool::new(false);
    let Ok(Discovery::Repository(repository)) = source::discover(&root, &git, &cancel) else {
        return Ok(roots);
    };
    let Ok(env) = git.env() else {
        return Ok(roots);
    };
    let Ok(worktrees) = source::worktrees(env, &repository, &cancel) else {
        return Ok(roots);
    };
    for worktree in worktrees {
        if let Ok(scope) = source::scope_root(&worktree.root, &repository.scope) {
            if !roots.iter().any(|known| known.root == scope.canonical) {
                roots.push(LiveRoot {
                    root: scope.canonical,
                    worktree: Some(worktree.id),
                });
            }
        }
    }
    Ok(roots)
}

pub(super) fn parse_path(path: &str) -> Result<RelPath> {
    RelPath::parse(path).ok_or_else(|| {
        CodedError::raised(
            error_code::INVALID_PATH,
            format!("{path} is not a path inside a project"),
            &[("path", path)],
        )
    })
}

pub(super) fn stored_project(state: &AppState, project: &str) -> Result<Project> {
    Ok(state.store.require_project(project)?)
}

pub(super) fn project_source(
    record: &Project,
    git: &BrowseGit,
    cancel: &AtomicBool,
) -> Result<ProjectSourceInfo> {
    let root = source::resolve_root(&record.path)?;
    let mut info = ProjectSourceInfo {
        project: record.id.clone(),
        root: wire_path::encode(record.path.as_bytes()),
        resolved_root: wire_path::encode(&names::name_bytes(root.canonical.as_os_str())),
        root_id: root.id.clone(),
        git: None,
        git_error: None,
        git_error_code: None,
    };
    let repository = match source::discover(&root, git, cancel)? {
        Discovery::NotARepository => return Ok(info),
        Discovery::Unreadable(message) => {
            info.git_error = Some(message);
            return Ok(info);
        }
        Discovery::NoGit(err) => {
            info.git_error = Some(format!("{err:#}"));
            info.git_error_code = is_git_not_found(&err).then_some(error_code::BINARY_NOT_FOUND);
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
        .map(|worktree| {
            let scope = source::scope_root(&worktree.root, &repository.scope);
            WorktreeInfo {
                scope_present: scope.is_ok(),
                scope_cause: scope.err().map(|err| source::unavailable_cause(&err)),
                id: worktree.id,
                root: wire_path::encode(&names::name_bytes(worktree.root.as_os_str())),
                main: worktree.main,
                head: worktree.head,
                branch: worktree.branch.as_deref().map(wire_path::encode),
            }
        })
        .collect();
    info.git = Some(GitSourceInfo {
        repository: repository.id,
        common_dir: wire_path::encode(&names::name_bytes(repository.common_dir.as_os_str())),
        worktree: repository.worktree,
        scope: repository.scope.to_wire(),
        worktrees,
    });
    Ok(info)
}

/// The project's repository, refusing a project that is in none, or one there is no `git` to read
/// with: as `binary_not_found` for a `git` that cannot be found, which a client words as Git not
/// being installed, else as `git_unavailable`, as for a repository `git` cannot read.
fn repository(
    root: &ProjectRoot,
    project: &str,
    git: &BrowseGit,
    cancel: &AtomicBool,
) -> Result<Repository> {
    match source::discover(root, git, cancel)? {
        Discovery::Repository(repository) => Ok(repository),
        Discovery::NotARepository => Err(CodedError::raised(
            error_code::NOT_A_GIT_REPOSITORY,
            "the project is not in a git repository",
            &[("project", project)],
        )),
        Discovery::NoGit(err) if is_git_not_found(&err) => Err(err),
        Discovery::NoGit(err) => Err(git_unavailable(&format!("{err:#}"))),
        Discovery::Unreadable(message) => Err(git_unavailable(&message)),
    }
}

fn git_unavailable(message: &str) -> anyhow::Error {
    CodedError::raised(error_code::GIT_UNAVAILABLE, message, &[("detail", message)])
}

/// The directory a live read or listing is scoped to: the project's own, or — with `worktree` —
/// the same place in that worktree of the project's repository, revalidated now.
pub(super) fn scope_for(
    record: &Project,
    worktree: Option<&str>,
    git: &BrowseGit,
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

pub(super) fn locate(
    record: &Project,
    worktree: Option<&str>,
    git: &BrowseGit,
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

/// The sources `read_from_git` reads: `ReadFrom` without the file on disk.
pub(super) enum GitRead {
    Index,
    Branch(String),
    Commit(String),
}

/// Reads `path` from the index or a commit, returning the source identity the bytes came from.
pub(super) fn read_from_git(
    record: &Project,
    worktree: Option<&str>,
    path: &RelPath,
    from: GitRead,
    git: &BrowseGit,
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
