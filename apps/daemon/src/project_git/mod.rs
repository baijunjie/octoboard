//! The git work the daemon does on a project's own repository: the status checks and their branch
//! header ([`status`], [`header`]), fetching and fast-forwarding, pushing ([`push`]), switching
//! branches ([`switch`]), restoring ([`restore`]), the project's remotes ([`remote`],
//! [`remote_url`]), watching its refs ([`watch`]) and cloning ([`clone`]). The runners the others
//! share for a project's repository are here.

pub mod clone;
pub mod header;
pub mod push;
pub mod remote;
pub mod remote_url;
pub mod restore;
pub mod status;
pub mod switch;
pub mod watch;

use std::path::Path;
use std::time::Duration;

use crate::git::run::{self, Access, GitError, Limits, Policy, UserGit, WorkDir};

/// Upper bound on one `git` subprocess other than a push (`fetch`, `status`, `merge`).
/// `GIT_TERMINAL_PROMPT=0` and `ssh -oBatchMode=yes` (see `crate::git::env`) turn a missing
/// credential or an unknown host key into an immediate failure rather than a prompt neither of them
/// ever shows on a headless daemon, but a slow network can still legitimately take tens of seconds
/// on a fetch; two minutes leaves a wide margin above that while still turning a wedged step into a
/// reported error within one sweep instead of leaking a blocking-pool thread, and leaving its
/// project excluded from every later sweep, for the rest of the daemon's life.
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// Upper bound on one `git push`. A push runs the repository's `pre-push` hook before anything is
/// sent, and a hook that runs the project's checks (tests, a build) legitimately takes minutes, so
/// the two minutes of [`GIT_COMMAND_TIMEOUT`] would kill it mid-check and the user would never see
/// its output. Half an hour is still a bound: a hook that waits on something that never comes
/// is reported in the end rather than left running for the rest of the daemon's life.
const GIT_PUSH_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Runs `git` with `args` in `path` and returns its stdout as UTF-8, reading the repository and
/// nothing more. A read whose only interesting output is its exit status still goes through here
/// and drops what it returns, never through [`run_git_write`]: see [`Access`].
pub(crate) fn run_git_read(
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
) -> Result<String, String> {
    run_git_access(path, args, Access::Read, GIT_COMMAND_TIMEOUT)
}

/// Runs `git` with `args` in `path` and discards its stdout, for a step that writes to the
/// repository: a fetch, a fast-forward or a branch switch.
pub(crate) fn run_git_write(
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
) -> Result<(), String> {
    run_git_access(path, args, Access::Write, GIT_COMMAND_TIMEOUT).map(|_| ())
}

/// [`run_git_write`] for `git push`, under [`GIT_PUSH_TIMEOUT`] in place of the shorter bound of
/// the other writes: the `pre-push` hook it runs may take minutes, and its output, which `git`
/// sends to stderr, is what a failed push reports.
pub(crate) fn run_git_push(
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
) -> Result<(), String> {
    run_git_access(path, args, Access::Write, GIT_PUSH_TIMEOUT).map(|_| ())
}

/// What all of the above run: the user's `git` from the cached shell environment, and the command
/// [`git_command`] builds over it. The error is the verbatim stderr of a failed `git`, the
/// operating-system message of a failure to even start it, or a timeout past `timeout` — carried
/// as is, untranslated, into `GitStatus` or the error of the step that failed.
fn run_git_access(
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
    access: Access,
    timeout: Duration,
) -> Result<String, String> {
    let git = UserGit::cached_shell().map_err(|err| format!("{err:#}"))?;
    run_git_command(&mut git_command(&git, path, args, access), timeout)
}

/// The `git <args>` command to run in `path`, with the `git` passed in rather than looked up, so a
/// test can point it at something of its own instead of the user's login shell.
///
/// Only the `GIT_*` variables of the user's shell that say which of their own material to use are
/// kept (see `crate::git::env::user_configuration_only`): a `GIT_DIR` or `GIT_WORK_TREE` exported
/// in the login shell would otherwise send the branch read, the fetch and the fast-forward alike to
/// another repository, and the remote name picked from this project's repository would be fetched
/// there.
fn git_command(
    git: &UserGit,
    path: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
    access: Access,
) -> std::process::Command {
    let mut command = git.command(&Policy::PLAIN, access, WorkDir::At(path));
    command.args(args);
    command
}

/// Runs a command [`git_command`] built, to completion within `timeout`, and returns its stdout as
/// UTF-8.
fn run_git_command(
    command: &mut std::process::Command,
    timeout: Duration,
) -> Result<String, String> {
    run::run_checked(command, Limits::deadline(timeout), None)
        .map(|stdout| String::from_utf8_lossy(&stdout).into_owned())
        .map_err(GitError::into_message)
}

#[cfg(test)]
mod tests {
    use super::header::BRANCH_HEADER_ARGS;
    use super::*;
    use crate::test_support::{
        isolated_git_env, redirecting_overrides, repo_pair, repo_with, CommandLine, PATIENCE,
        SYSTEM_GIT,
    };

    /// The command line of a read and of a write, pinned whole: the read gets the index guard ahead
    /// of the arguments, the write does not, and both run in the repository.
    #[test]
    fn a_read_and_a_write_build_their_command_lines() {
        let git = UserGit::new("/opt/git", crate::test_support::pinned_shell_env());
        let read = git_command(&git, Path::new("/repo"), BRANCH_HEADER_ARGS, Access::Read);
        let mut args = vec!["-c", "diff.autoRefreshIndex=false"];
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
        let write = git_command(
            &git,
            Path::new("/repo"),
            &["push", "--porcelain", "origin"],
            Access::Write,
        );
        assert_eq!(
            CommandLine::of(&write),
            CommandLine::pinned(
                "/opt/git",
                &["push", "--porcelain", "origin"],
                &[],
                Some("/repo")
            )
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
            run_git_command(&mut unfiltered, GIT_COMMAND_TIMEOUT)
                .expect("the unfiltered read runs"),
            "theirs\n"
        );

        // One access is enough: the filter is applied before `git_command` tells a read from a
        // write, so a write goes through the very same line.
        let mut command = git_command(&UserGit::new(SYSTEM_GIT, env), &ours, &args, Access::Read);
        assert_eq!(
            run_git_command(&mut command, GIT_COMMAND_TIMEOUT).expect("the read runs"),
            "ours\n"
        );
    }

    #[test]
    fn a_command_is_held_to_the_timeout_it_is_given() {
        let repo = repo_with("git-status-timeout", &[(b"a.txt", b"ours\n")]);
        // An alias standing in for a `git` that is slow to finish, as a `pre-push` hook makes a
        // push.
        let args = ["-c", "alias.nap=!sleep 1", "nap"];
        let slow = |timeout| {
            let mut command = git_command(
                &UserGit::new(SYSTEM_GIT, isolated_git_env()),
                &repo,
                &args,
                Access::Write,
            );
            run_git_command(&mut command, timeout)
        };
        let err = slow(Duration::from_millis(100)).expect_err("the short bound is exceeded");
        assert!(err.contains("did not finish"), "{err}");
        slow(PATIENCE).expect("the long bound is not");
    }
}
