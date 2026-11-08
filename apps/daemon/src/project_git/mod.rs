//! The git work the daemon does on a project's own repository: the status checks and their branch
//! header ([`status`], [`header`]), fetching and fast-forwarding, pushing ([`push`]), switching
//! branches ([`switch`]), restoring ([`restore`]), the project's remotes ([`remote`],
//! [`remote_url`]), re-reading its refs when the platform reports them changed ([`watch`]) or a
//! session's git reports moving them ([`ref_moves`]) through one debounced scheduler
//! ([`debounced`]), and cloning ([`clone`]). The `git` the others share for a project's
//! repository ([`project_git`]) is here, and so is the lookup of the stored projects in one
//! repository ([`projects_in_repository`]).

pub mod clone;
pub mod debounced;
pub mod header;
pub mod push;
pub mod ref_moves;
pub mod remote;
pub mod remote_url;
pub mod restore;
pub mod status;
pub mod switch;
pub mod watch;

use std::path::Path;
use std::time::Duration;

use crate::git::repo_files::repository_of;
use crate::git::run::{Limits, Policy, Runner};
use crate::host_tools::{is_git_not_found, Pace};
use crate::protocol::{error_code, Code, GitStatus, Project};
use crate::state::AppState;

/// Upper bound on one `git` subprocess other than a push (`fetch`, `status`, `merge`).
/// `GIT_TERMINAL_PROMPT=0` and `ssh -oBatchMode=yes` (see `crate::git::env`) turn a missing
/// credential or an unknown host key into an immediate failure rather than a prompt neither of them
/// ever shows on a headless daemon, but a slow network can still legitimately take tens of seconds
/// on a fetch; two minutes leaves a wide margin above that while still turning a wedged step into a
/// reported error within one sweep instead of leaking a blocking-pool thread, and leaving its
/// project excluded from every later sweep, for the rest of the daemon's life.
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// What each `git` of a project's repository is held to, a push apart: [`GIT_COMMAND_TIMEOUT`],
/// and no output ceiling, its output being small by nature.
const LIMITS: Limits = Limits::deadline(GIT_COMMAND_TIMEOUT);

/// Upper bound on one `git push`. A push runs the repository's `pre-push` hook before anything is
/// sent, and a hook that runs the project's checks (tests, a build) legitimately takes minutes, so
/// the two minutes of [`GIT_COMMAND_TIMEOUT`] would kill it mid-check and the user would never see
/// its output. Half an hour is still a bound: a hook that waits on something that never comes
/// is reported in the end rather than left running for the rest of the daemon's life.
const GIT_PUSH_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// What a `git push` is held to: [`GIT_PUSH_TIMEOUT`] in place of [`LIMITS`]'s bound. Its output,
/// which `git` sends to stderr, the `pre-push` hook's included, is what a failed push reports.
pub(crate) const PUSH_LIMITS: Limits = Limits::deadline(GIT_PUSH_TIMEOUT);

/// The `git` of a project's own repository: the user's own, as the host tools hand it out for
/// `pace` (the stand-in a test puts in its place), under the plain policy and [`LIMITS`]. One that
/// cannot be found is `binary_not_found`, kept as it is: a reply carries it with its code, and a
/// `GitStatus` records it as its `error_code` ([`StepFailure::no_git`]).
///
/// A failed run's error is the verbatim stderr of `git`, the operating-system message of a failure
/// to even start it, or a timeout — carried as is, untranslated, into `GitStatus` or the error of
/// the step that failed.
pub(crate) fn project_git(state: &AppState, pace: Pace) -> anyhow::Result<Runner> {
    state.host_tools.get().git_for(pace).map(project_runner)
}

/// `err`, the failure to have a `git` at all ([`project_git`]), as an action that words its other
/// failures with `failed` replies it: one that cannot be found keeps `binary_not_found`, which a
/// client words as Git not being installed, and any other (a shell snapshot that failed or timed
/// out) is the action's own failure, with the text as its detail.
pub(crate) fn no_git_or(
    err: anyhow::Error,
    failed: impl FnOnce(&str) -> anyhow::Error,
) -> anyhow::Error {
    if is_git_not_found(&err) {
        err
    } else {
        failed(&format!("{err:#}"))
    }
}

/// Why a step of a project's git work failed, as a `GitStatus` records it: the text, and the code
/// of a failure a client words itself.
pub(crate) struct StepFailure {
    message: String,
    code: Option<Code>,
}

impl StepFailure {
    /// The failure to have a `git` at all ([`project_git`]): `binary_not_found` for one that cannot
    /// be found, which a client words as Git not being installed.
    pub(crate) fn no_git(err: &anyhow::Error) -> Self {
        Self {
            message: format!("{err:#}"),
            code: is_git_not_found(err).then_some(error_code::BINARY_NOT_FOUND),
        }
    }

    /// Records it on `status` as the last failed step, in place of whatever failure was there.
    pub(crate) fn record(self, status: &mut GitStatus) {
        status.error = Some(self.message);
        status.error_code = self.code;
    }
}

/// A step's own failure: `git`'s or the system's text, with no code.
impl From<String> for StepFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            code: None,
        }
    }
}

/// Creates a missing directory with an empty repository, leaving existing directories untouched.
/// Blocking: initialization finishes in an owned sibling before the target is created, so a
/// failed initialization cannot turn a retry into the association of an uninitialized directory.
pub(crate) fn create_empty(state: &AppState, path: &Path) -> anyhow::Result<()> {
    use anyhow::Context;

    if crate::hostfs::directory_exists_or_creatable(path)? {
        return Ok(());
    }
    let git = project_git(state, Pace::Person)
        .with_context(|| format!("initializing `{}`", path.display()))?;
    let parent = path
        .parent()
        .expect("an absolute missing path has a parent");
    std::fs::create_dir_all(parent).map_err(|err| {
        crate::hostfs::directory_unreadable(
            path,
            &err,
            format!("creating `{}`: {err}", path.display()),
        )
    })?;
    let temporary = parent.join(format!(".octoboard-new-project-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&temporary).with_context(|| format!("preparing `{}`", path.display()))?;
    let temporary = NewProjectDirectory(temporary);
    // Parent-directory discovery must not mistake the temporary holder for a project.
    let repository = temporary.0.join("repository");
    std::fs::create_dir(&repository).with_context(|| format!("preparing `{}`", path.display()))?;
    git.write(&repository, ["init", "--quiet", "--template="])
        .with_context(|| format!("initializing `{}`", path.display()))?;
    match std::fs::create_dir(path) {
        Ok(()) => {
            if let Err(err) = std::fs::rename(repository.join(".git"), path.join(".git")) {
                // Only an empty leaf is removable; concurrent user files must survive a failure.
                std::fs::remove_dir(path).ok();
                return Err(err).with_context(|| format!("initializing `{}`", path.display()));
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            crate::hostfs::require_directory(path)
        }
        Err(err) => Err(crate::hostfs::directory_unreadable(
            path,
            &err,
            format!("creating `{}`: {err}", path.display()),
        )),
    }
}

struct NewProjectDirectory(std::path::PathBuf);

impl Drop for NewProjectDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// The stored projects whose directory is in the repository with canonical common directory
/// `repository`, the linked worktrees of one repository counting as it. A directory that cannot be
/// traced to a repository is in none. Blocking: it resolves every project's directory.
pub(crate) fn projects_in_repository(
    state: &AppState,
    repository: &Path,
) -> anyhow::Result<Vec<Project>> {
    Ok(in_repository(state, repository)?.collect())
}

/// Whether any stored project is in the repository, as [`projects_in_repository`] counts it,
/// resolving directories only until one is found. Blocking.
pub(crate) fn has_projects_in_repository(
    state: &AppState,
    repository: &Path,
) -> anyhow::Result<bool> {
    Ok(in_repository(state, repository)?.next().is_some())
}

/// The projects [`projects_in_repository`] lists, each directory resolved as it is reached.
fn in_repository<'a>(
    state: &AppState,
    repository: &'a Path,
) -> anyhow::Result<impl Iterator<Item = Project> + 'a> {
    Ok(state
        .store
        .list_projects()?
        .into_iter()
        .filter(move |project| {
            repository_of(Path::new(&project.path)).as_deref() == Some(repository)
        }))
}

/// [`project_git`] over `git`.
fn project_runner(git: crate::git::run::UserGit) -> Runner {
    Runner::new(git, Policy::PLAIN, LIMITS)
}

/// [`project_git`] over the system `git` and [`crate::test_support::isolated_git_env`], for a test
/// that runs one of this module's steps without an `AppState`.
#[cfg(test)]
pub(crate) fn test_git() -> Runner {
    use crate::test_support::{isolated_git_env, SYSTEM_GIT};

    project_runner(crate::git::run::UserGit::new(
        SYSTEM_GIT,
        isolated_git_env(),
    ))
}

#[cfg(test)]
mod tests {
    use super::header::BRANCH_HEADER_ARGS;
    use super::*;
    use crate::git::env::BASE_OPTIONS;
    use crate::git::run::{self, Access, UserGit, WorkDir};
    use crate::test_support::{
        isolated_git_env, redirecting_overrides, repo_pair, repo_with, CommandLine, PATIENCE,
        SYSTEM_GIT,
    };

    /// The command line of a read and of a write, pinned whole: both start with the base options,
    /// the read gets the read guard ahead of the arguments, the write does not, so a push's hook
    /// keeps the user's monitor, and both run in the repository.
    #[test]
    fn a_read_and_a_write_build_their_command_lines() {
        let git = project_runner(UserGit::new(
            "/opt/git",
            crate::test_support::pinned_shell_env(),
        ));
        let read = git.command(
            Access::Read,
            WorkDir::At(Path::new("/repo")),
            BRANCH_HEADER_ARGS,
        );
        let mut args = BASE_OPTIONS.to_vec();
        args.extend([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.autoRefreshIndex=false",
        ]);
        args.extend_from_slice(BRANCH_HEADER_ARGS);
        assert_eq!(
            CommandLine::of(&read),
            CommandLine::pinned(
                "/opt/git",
                &args,
                &[("GIT_OPTIONAL_LOCKS", "0")],
                Some("/repo")
            )
        );
        let write = git.command(
            Access::Write,
            WorkDir::At(Path::new("/repo")),
            ["push", "--porcelain", "origin"],
        );
        let mut args = BASE_OPTIONS.to_vec();
        args.extend(["push", "--porcelain", "origin"]);
        assert_eq!(
            CommandLine::of(&write),
            CommandLine::pinned("/opt/git", &args, &[], Some("/repo"))
        );
    }

    #[test]
    fn environment_overrides_cannot_redirect_a_command_to_another_repository() {
        let (ours, theirs) = repo_pair("git-status");
        let mut env = isolated_git_env();
        env.extend(
            redirecting_overrides(&theirs)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        );
        // A read of the file's content, which says whose index and object store it came from; the
        // branch name would follow `GIT_DIR` alone. The worktree and the inline configuration are
        // not seen by it: the unit tests in `crate::git::env` pin them by name.
        let args = ["cat-file", "blob", ":a.txt"];

        // The positive control: the same command without the filter, over the whole environment,
        // reads the other repository.
        let mut unfiltered = crate::git::env::non_interactive(Path::new(SYSTEM_GIT), &env);
        unfiltered.current_dir(&*ours).args(args);
        assert_eq!(
            run::run_checked(&mut unfiltered, LIMITS, None).expect("the unfiltered read runs"),
            b"theirs\n"
        );

        // One access is enough: the filter is applied before the command tells a read from a
        // write, so a write goes through the very same line.
        let git = project_runner(UserGit::new(SYSTEM_GIT, env));
        assert_eq!(git.read(&ours, args).expect("the read runs"), b"ours\n");
    }

    #[test]
    fn a_command_is_held_to_the_timeout_it_is_given() {
        let repo = repo_with("git-status-timeout", &[(b"a.txt", b"ours\n")]);
        // An alias standing in for a `git` that is slow to finish, as a `pre-push` hook makes a
        // push.
        let args = ["-c", "alias.nap=!sleep 1", "nap"];
        let slow = |timeout| {
            test_git()
                .with_limits(Limits::deadline(timeout))
                .write(&repo, args)
        };
        let err = slow(Duration::from_millis(100))
            .expect_err("the short bound is exceeded")
            .to_string();
        assert_eq!(err, "git did not finish within 100ms");
        slow(PATIENCE).expect("the long bound is not");
    }
}
