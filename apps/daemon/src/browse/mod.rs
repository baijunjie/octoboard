//! Browsing a project's files and their Git versions, for the browse requests in
//! `apps/daemon/PROTOCOL.md`'s "Browsing a project": where a project's files are (`source`), the
//! files on disk (`live`), the files in Git (`blob`, through the isolated invocation in `git`), a
//! worktree's uncommitted changes and one change's diff (`changes`), the reading of one change
//! that it and the two below share (`change_read`), the repository's local branches and the
//! changes between two of their tips (`compare`), the commits a branch made since it forked and the
//! changes of each (`commits`), the tree-diff output both read their changes from (`tree_diff`),
//! which entries Git ignores (`ignored`), the limits all of it is held to (`budget`), and the
//! per-connection lane its replies take back to the client (`lane`). Which project, directory and
//! worktree a request works in is resolved in `scope`, and a body is put on the wire in `content`.
//! Paths travel on the wire in `crate::wire_path`'s form.
//!
//! Every request re-resolves its project from the store and its worktree from the repository; a
//! reply is never built from a directory a client named.

pub mod blob;
pub mod budget;
pub mod change_read;
pub mod changes;
pub mod commits;
pub mod compare;
pub mod content;
pub mod ignored;
pub mod lane;
pub mod line_counts;
pub mod live;
pub mod scope;
pub mod source;
pub mod tree_diff;

#[cfg(test)]
mod body_tests;
#[cfg(test)]
mod change_tests;
#[cfg(test)]
mod commits_tests;
#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod line_count_tests;
#[cfg(test)]
mod nested_tests;
#[cfg(test)]
pub(crate) mod tests;

use std::cell::OnceCell;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::git::isolated::IsolatedGit;
use crate::git::run::GitError;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::{error_code, BrowseBody, CodedError, ContentSource, Event, ReadFrom};
use crate::state::AppState;
use change_read::Expected;
use content::file_content;
use scope::{
    locate, parse_path, project_source, read_from_git, scope_for, stored_project, GitRead,
};

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
        GitError::Exited { stderr, .. } => git_failed(&stderr),
        GitError::Stopped(BoundedFailure::Cancelled) => anyhow::Error::new(Cancelled),
        GitError::Stopped(BoundedFailure::StdoutExceeded(limit)) => {
            let max = limit.to_string();
            CodedError::raised(
                error_code::LIMIT_EXCEEDED,
                format!("git printed more than the {limit}-byte limit"),
                &[("limit", "git_output"), ("max", &max)],
            )
        }
        stopped @ GitError::Stopped(_) => git_failed(&stopped.to_string()),
    }
}

/// `git_failed`, carrying `message` — `git`'s own, or why it did not get as far as one.
pub(crate) fn git_failed(message: &str) -> anyhow::Error {
    CodedError::raised(error_code::GIT_FAILED, message, &[("detail", message)])
}

/// The `git` environment of one request, built the first time the request needs it and reused
/// after that, so a request that never reaches `git` never takes the shell snapshot either.
pub struct BrowseGit<'a> {
    make: &'a dyn Fn() -> Result<IsolatedGit>,
    env: OnceCell<IsolatedGit>,
    ceiling: Option<std::path::PathBuf>,
}

impl<'a> BrowseGit<'a> {
    pub fn new(make: &'a dyn Fn() -> Result<IsolatedGit>) -> Self {
        Self {
            make,
            env: OnceCell::new(),
            ceiling: crate::git::layout::home_ceiling(),
        }
    }

    /// The directory repository discovery never climbs into; see
    /// `crate::git::layout::find_dot_git`.
    pub fn ceiling(&self) -> Option<&std::path::Path> {
        self.ceiling.as_deref()
    }

    #[cfg(test)]
    pub fn with_ceiling(mut self, ceiling: Option<std::path::PathBuf>) -> Self {
        self.ceiling = ceiling;
        self
    }

    pub fn env(&self) -> Result<&IsolatedGit> {
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
    git: &dyn Fn() -> Result<IsolatedGit>,
    cancel: &AtomicBool,
) -> Result<Event> {
    let git = BrowseGit::new(git);
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
            let listing = live::list_dir(&scope.canonical, &rel, &git, cancel)?;
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
        BrowseBody::SearchProjectFiles {
            project,
            query,
            include_ignored,
        } => {
            let record = stored_project(state, &project)?;
            let scope = scope_for(&record, None, &git, cancel)?;
            let matches =
                live::search_files(&scope.canonical, &query, include_ignored, &git, cancel)?;
            Ok(Event::ProjectFileMatches {
                id,
                project,
                query,
                paths: matches.paths,
                ignored: matches.ignored,
                complete: matches.complete,
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
            let mut list = compare::branches(git.env()?, &at, cancel)?;
            crate::branch_origin::annotate(state, &at.repository.common_dir, &mut list);
            Ok(Event::ProjectBranches {
                id,
                project,
                branches: list.branches,
                complete: list.complete,
                upstreams_complete: list.upstreams_complete,
                default_branch: list.default_branch,
            })
        }
        BrowseBody::CompareProjectBranches {
            project,
            left,
            left_remote,
            left_empty,
            right,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let base = match (left_empty, left_remote) {
                (true, _) => compare::Base::Empty,
                (false, true) => compare::Base::Remote(&left),
                (false, false) => compare::Base::Local(&left),
            };
            let comparison = compare::list(git.env()?, &at, base, &right, cancel)?;
            Ok(Event::ProjectComparison {
                id,
                project,
                left: comparison.left,
                right: comparison.right,
                base: comparison.base,
                changes: comparison.changes,
                complete: comparison.complete,
            })
        }
        BrowseBody::ReadProjectComparisonChange {
            project,
            left,
            right,
            base,
            change,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let read = compare::read(git.env()?, &at, (&left, &right, &base), &change, cancel)?;
            Ok(Event::ProjectComparisonChange {
                id,
                project,
                left,
                right,
                base,
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
            let expected = Expected {
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
            base,
            change,
            old,
            new,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let expected = Expected {
                old: &old,
                new: &new,
            };
            let read = compare::read_bodies(
                git.env()?,
                &at,
                (&left, &right, &base),
                &change,
                &expected,
                cancel,
            )?;
            Ok(Event::ProjectComparisonChangeBodies {
                id,
                project,
                left,
                right,
                base,
                old: Box::new(read.old),
                new: Box::new(read.new),
            })
        }
        // As a comparison, a commit's changes concern the repository's history rather than any
        // one worktree, and are read in the worktree holding the project's directory.
        BrowseBody::ListProjectCommits { project, base, tip } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let list = commits::list(git.env()?, &at, (&base, &tip), cancel)?;
            Ok(Event::ProjectCommits {
                id,
                project,
                base,
                tip,
                commits: list.commits,
                complete: list.complete,
                commits_complete: list.commits_complete,
            })
        }
        BrowseBody::ReadProjectCommitChange {
            project,
            commit,
            parent,
            change,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let read = commits::read(
                git.env()?,
                &at,
                (&commit, parent.as_deref()),
                &change,
                cancel,
            )?;
            Ok(Event::ProjectCommitChange {
                id,
                project,
                commit,
                parent,
                old: Box::new(read.old),
                new: Box::new(read.new),
                patch: read.patch.map(file_content),
            })
        }
        BrowseBody::ReadProjectCommitChangeBodies {
            project,
            commit,
            parent,
            change,
            old,
            new,
        } => {
            let record = stored_project(state, &project)?;
            let at = locate(&record, None, &git, cancel)?;
            let expected = Expected {
                old: &old,
                new: &new,
            };
            let read = commits::read_bodies(
                git.env()?,
                &at,
                (&commit, parent.as_deref()),
                &change,
                &expected,
                cancel,
            )?;
            Ok(Event::ProjectCommitChangeBodies {
                id,
                project,
                commit,
                parent,
                old: Box::new(read.old),
                new: Box::new(read.new),
            })
        }
    }
}
