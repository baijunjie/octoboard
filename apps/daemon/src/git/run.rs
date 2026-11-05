//! Where every `git` the daemon runs in a user's repository, or against a user's remote, is built
//! and run, so that none of them misses part of what [`crate::git::env`] settles for all of them.
//!
//! A command is the user's own `git` ([`UserGit`]): their binary, run with their shell
//! environment cut down to their own configuration and unable to prompt. On top of that comes the
//! fixed part of its call site's family ([`Policy`]): the global options ahead of the subcommand
//! and the variables that family sets. Whether it only reads the repository ([`Access`]) decides
//! the index guard, and the caller adds the rest — its own `-c` items, its subcommand and
//! arguments, a variable to drop — to the `Command` it is handed.
//!
//! Running it holds it to a deadline, and to output ceilings where the caller has them
//! ([`Limits`]); a stop kills the whole process group, which also reaps the `ssh` a fetch forked.
//! [`run_checked`] turns a failing exit into a [`GitError`]; [`run`] hands back the output
//! whatever the status, for a caller that reads it either way.
//!
//! A family of call sites runs its `git` through a [`Runner`]: the user's `git` with the family's
//! `Policy` and `Limits`, both constants defined beside the family's users, so its commands are
//! built and held alike and fail with the same [`GitError`].
//!
//! This is the one runner for every `git` the daemon starts, except one that runs inside the
//! user's hook with the hook's own environment.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::Result;

use crate::env_shell;
use crate::git::env::{self as git_env, SshConfiguration};
use crate::platform::names;
use crate::platform::proc::Reach;
use crate::processes::subprocess::{self, BoundedFailure, Bounds};

/// The user's `git` binary and the shell environment it runs with.
#[derive(Debug, Clone)]
pub struct UserGit {
    program: String,
    env: HashMap<String, String>,
}

impl UserGit {
    /// The user's `git`, as their login shell's `PATH` resolves it, in that shell's environment,
    /// from the cached snapshot every repeating `git` caller uses.
    pub fn cached_shell() -> Result<Self> {
        Self::from_env(env_shell::cached_snapshot()?)
    }

    /// The `git` that `env`'s `PATH` resolves, run in `env`.
    pub fn from_env(env: HashMap<String, String>) -> Result<Self> {
        let program = env_shell::resolve_program("git", &env)?;
        Ok(Self { program, env })
    }

    /// The `git` at `program`, run in `env`.
    pub fn new(program: impl Into<String>, env: HashMap<String, String>) -> Self {
        Self {
            program: program.into(),
            env,
        }
    }

    /// The command `git <base options> <policy's options> [<read-only guard>]` in `dir`, for the
    /// caller to add its own `-c` items, subcommand and arguments to. The environment is the
    /// user's, cut down to their own configuration, unable to prompt, with the policy's variables
    /// set and, for a read, the read guard. The base options are [`git_env::BASE_OPTIONS`].
    pub fn command(&self, policy: &Policy, access: Access, dir: WorkDir<'_>) -> Command {
        let mut command =
            git_env::non_interactive(&self.program, git_env::user_configuration_only(&self.env));
        command.envs(policy.env.iter().copied());
        command.args(git_env::BASE_OPTIONS);
        command.args(policy.leading);
        if matches!(access, Access::Read) {
            git_env::read_only(&mut command);
        }
        if let WorkDir::At(dir) = dir {
            command.current_dir(dir);
        }
        command
    }

    /// [`Self::command`] for a `git` that may reach a remote — a fetch, a push, a clone — with the
    /// user's own ssh in place of the plain one, `configured` being what the configuration it runs
    /// under says of `core.sshCommand` (see [`git_env::reach_remote`]).
    pub fn remote_command(
        &self,
        policy: &Policy,
        access: Access,
        dir: WorkDir<'_>,
        configured: &SshConfiguration,
    ) -> Command {
        let mut command = self.command(policy, access, dir);
        git_env::reach_remote(&mut command, &self.env, configured);
        command
    }

    /// What `core.sshCommand` is for a `git` run in `dir`, read with `git config` in the same
    /// environment, for [`Self::remote_command`]; [`SshConfiguration::Unset`], without running
    /// `git`, where the user's own `GIT_SSH_COMMAND` wins over it anyway. A caller that already
    /// reads the configuration takes the value from that read instead.
    pub fn ssh_configuration(&self, dir: &Path) -> SshConfiguration {
        if !git_env::ssh_configuration_matters(&self.env) {
            return SshConfiguration::Unset;
        }
        let mut command = self.command(&Policy::PLAIN, Access::Read, WorkDir::At(dir));
        command.args(["config", "-z", "--get", "core.sshCommand"]);
        match run_checked(
            &mut command,
            Limits::deadline(SSH_CONFIGURATION_TIMEOUT),
            None,
        ) {
            Ok(stdout) => {
                let value = stdout.strip_suffix(b"\0").unwrap_or(&stdout);
                SshConfiguration::of(Some(String::from_utf8_lossy(value).into_owned()))
            }
            // `git config --get` says "not set" with exit status 1, and only with it.
            Err(GitError::Exited { code: Some(1), .. }) => SshConfiguration::Unset,
            Err(err) => {
                tracing::debug!(%err, "reading core.sshCommand failed");
                SshConfiguration::Unknown
            }
        }
    }
}

/// How long the read of `core.sshCommand` may take: it reads configuration files, so this guards
/// against a wedged file system, not a network.
const SSH_CONFIGURATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether a `git` only reads the repository it runs in, or is meant to write to it (a fetch, a
/// fast-forward, a branch switch, a push, a restore, a worktree removal) or reads no repository at
/// all (a clone, a question about the installed `git`). A read is held back from writing the
/// index, which it otherwise does behind the caller's back — see [`git_env::read_only`]; a write
/// is not, since it legitimately takes the locks it needs. A read whose only interesting output
/// is its exit status — a dirty-worktree `git diff --quiet`, say — is still a read: it is exactly
/// such a command that the index guard matters most for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// Where a command runs.
#[derive(Debug, Clone, Copy)]
pub enum WorkDir<'a> {
    /// In this directory, which is the repository `git` discovers from it.
    At(&'a Path),
    /// In the daemon's own working directory: for a command that names every path it works on.
    Inherited,
}

/// The fixed part of a family of call sites: the global options ahead of anything the call adds,
/// after the base ones every command gets, and the variables it sets.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    leading: &'static [&'static str],
    env: &'static [(&'static str, &'static str)],
}

impl Policy {
    /// Nothing beyond the base.
    pub const PLAIN: Self = Self::new(&[], &[]);

    /// `leading` goes ahead of everything else on the command line but the base options, the
    /// read-only guard's `-c` included, so a `-c` in it stands before the subcommand as it has to.
    pub const fn new(
        leading: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self { leading, env }
    }
}

/// What a run is held to: a deadline, the most stdout it may print (one byte more stops it) and
/// the most stderr kept.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub timeout: Duration,
    pub stdout: usize,
    pub stderr: usize,
}

impl Limits {
    /// A deadline and no output ceiling, for a command whose output is small by nature.
    pub const fn deadline(timeout: Duration) -> Self {
        Self {
            timeout,
            stdout: usize::MAX,
            stderr: usize::MAX,
        }
    }
}

/// How a `git` run ended short of an output to use.
///
/// Its message ([`Display`](std::fmt::Display)) is the one wording of the failure wherever it is
/// shown: `git`'s own message for a run that failed, and for one that was stopped what stopped it
/// with `git` as the subject (`git did not finish within 120s`), which a stop's own message does
/// not name.
#[derive(Debug)]
pub enum GitError {
    /// `git` ran and failed. `code` is its exit status, absent when a signal killed it; `stderr`
    /// is its own message, trimmed.
    Exited { code: Option<i32>, stderr: String },
    /// The run was stopped: a timeout, too much output, a cancel, or `git` never started.
    Stopped(BoundedFailure),
}

impl GitError {
    /// The exit status of a `git` that ran to an exit; none for one a signal killed or that was
    /// stopped, which is no answer from `git` at all.
    pub fn code(&self) -> Option<i32> {
        match self {
            Self::Exited { code, .. } => *code,
            Self::Stopped(_) => None,
        }
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exited { stderr, .. } => formatter.write_str(stderr),
            // A failure to run has a message of its own ("spawning the command: …"), which reads
            // as what running `git` came to rather than as something `git` did.
            Self::Stopped(BoundedFailure::Failed(err)) => write!(formatter, "running git: {err:#}"),
            Self::Stopped(stopped) => write!(formatter, "git {stopped}"),
        }
    }
}

impl std::error::Error for GitError {}

/// The `git` of a family of call sites: the user's own ([`UserGit`]) under the family's
/// [`Policy`], every run held to the family's [`Limits`]. The family defines both as constants
/// beside its users; a run of the family that needs another bound takes [`Runner::with_limits`].
#[derive(Debug, Clone)]
pub struct Runner {
    git: UserGit,
    policy: Policy,
    limits: Limits,
}

impl Runner {
    pub fn new(git: UserGit, policy: Policy, limits: Limits) -> Self {
        Self {
            git,
            policy,
            limits,
        }
    }

    /// The same `git` and policy, held to `limits` instead.
    pub fn with_limits(&self, limits: Limits) -> Self {
        Self {
            limits,
            ..self.clone()
        }
    }

    /// The user's `git` underneath, for a read that is built another way (a browse read, the read
    /// of `core.sshCommand`).
    pub fn git(&self) -> &UserGit {
        &self.git
    }

    /// The command `git <args>` in `dir`, built by [`UserGit::command`] under the family's policy.
    pub fn command<I>(&self, access: Access, dir: WorkDir<'_>, args: I) -> Command
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        let mut command = self.git.command(&self.policy, access, dir);
        command.args(args);
        command
    }

    /// [`Runner::command`] for a `git` that may reach a remote, built by
    /// [`UserGit::remote_command`] with `ssh`, what its configuration says of `core.sshCommand`.
    pub fn remote_command<I>(
        &self,
        access: Access,
        dir: WorkDir<'_>,
        args: I,
        ssh: &SshConfiguration,
    ) -> Command
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        let mut command = self.git.remote_command(&self.policy, access, dir, ssh);
        command.args(args);
        command
    }

    /// Runs `command`, built by this runner and then adjusted, within the family's limits, and
    /// returns its stdout.
    pub fn run(&self, command: &mut Command) -> Result<Vec<u8>, GitError> {
        run_checked(command, self.limits, None)
    }

    /// Runs `git <args>` in `dir` as a read ([`Access::Read`]) and returns its stdout. A read
    /// whose only interesting output is its exit status runs here too and drops what it returns.
    pub fn read<I>(&self, dir: &Path, args: I) -> Result<Vec<u8>, GitError>
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        self.run(&mut self.command(Access::Read, WorkDir::At(dir), args))
    }

    /// Runs `git <args>` in `dir` as a write ([`Access::Write`]) that reaches no remote, and
    /// returns its stdout.
    pub fn write<I>(&self, dir: &Path, args: I) -> Result<Vec<u8>, GitError>
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        self.run(&mut self.command(Access::Write, WorkDir::At(dir), args))
    }

    /// Runs `git <args>` in `dir` as a write that may reach a remote — a fetch, a push, a clone —
    /// with the user's ssh, `ssh` being what its configuration says of `core.sshCommand`, and
    /// returns its stdout.
    pub fn write_remote<I>(
        &self,
        dir: WorkDir<'_>,
        args: I,
        ssh: &SshConfiguration,
    ) -> Result<Vec<u8>, GitError>
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        self.run(&mut self.remote_command(Access::Write, dir, args, ssh))
    }
}

/// `bytes` as one command-line argument of `git`: the file name they stand for, or, on a host that
/// cannot spell them (Windows, for bytes that are not WTF-8), their text with replacement
/// characters, which names nothing there, so that git is asked for a path that does not exist
/// rather than for a different one. On Windows an unpaired surrogate in WTF-8 bytes reaches `git`
/// as that surrogate in its UTF-16 command line.
pub(crate) fn arg_from_bytes(bytes: &[u8]) -> OsString {
    names::name_from_bytes(bytes)
        .unwrap_or_else(|| OsString::from(String::from_utf8_lossy(bytes).into_owned()))
}

/// Runs `command` to completion within `limits`, and hands back what it printed and how it
/// exited, whatever the status. `cancel`, when there is one, stops the run once it is set.
pub fn run(
    command: &mut Command,
    limits: Limits,
    cancel: Option<&AtomicBool>,
) -> Result<Output, BoundedFailure> {
    static NEVER: AtomicBool = AtomicBool::new(false);
    let bounds = Bounds {
        timeout: limits.timeout,
        stdout_limit: limits.stdout,
        stderr_limit: limits.stderr,
        reach: Reach::Group,
        term_grace: Duration::ZERO,
    };
    subprocess::run_bounded(command, &bounds, cancel.unwrap_or(&NEVER))
}

/// [`run`], returning the stdout of a run that succeeded and the message of one that failed.
pub fn run_checked(
    command: &mut Command,
    limits: Limits,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>, GitError> {
    let output = run(command, limits, cancel).map_err(GitError::Stopped)?;
    if !output.status.success() {
        return Err(GitError::Exited {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        git, isolated_git_env, make_executable, pinned_shell_env, repo_with, CommandLine,
        ScratchDir, PATIENCE, SYSTEM_GIT,
    };

    /// A policy with a leading option and a variable of its own, to show where each goes.
    const FAMILY: Policy = Policy::new(
        &["-c", "core.quotePath=false"],
        &[("GIT_NO_LAZY_FETCH", "1")],
    );

    /// The command line of a read and of a write, pinned whole: both start with the base options
    /// and then the family's, and only the read gets the read guard.
    #[test]
    fn a_runners_read_and_write_build_their_command_lines() {
        let runner = Runner::new(
            UserGit::new("/opt/git", pinned_shell_env()),
            FAMILY,
            Limits::deadline(PATIENCE),
        );
        let read = runner.command(
            Access::Read,
            WorkDir::At(Path::new("/repo")),
            ["worktree", "list"],
        );
        assert_eq!(
            CommandLine::of(&read),
            CommandLine::pinned(
                "/opt/git",
                &[
                    "--no-pager",
                    "-c",
                    "core.quotePath=false",
                    "-c",
                    "core.fsmonitor=false",
                    "-c",
                    "diff.autoRefreshIndex=false",
                    "worktree",
                    "list"
                ],
                &[("GIT_NO_LAZY_FETCH", "1"), ("GIT_OPTIONAL_LOCKS", "0")],
                Some("/repo")
            )
        );
        let write = runner.command(Access::Write, WorkDir::Inherited, ["worktree", "prune"]);
        assert_eq!(
            CommandLine::of(&write),
            CommandLine::pinned(
                "/opt/git",
                &[
                    "--no-pager",
                    "-c",
                    "core.quotePath=false",
                    "worktree",
                    "prune"
                ],
                &[("GIT_NO_LAZY_FETCH", "1")],
                None
            )
        );
    }

    /// A repository configured with a filesystem monitor hook has it run by a write, which keeps
    /// the user's configuration for the hooks it runs, and not by a read.
    #[test]
    fn a_read_starts_no_configured_filesystem_monitor_and_a_write_keeps_it() {
        let repo = repo_with("run-fsmonitor", &[(b"a.txt", b"one\n")]);
        let bin = ScratchDir::new("run-fsmonitor-hook");
        let marker = bin.join("ran");
        let hook = bin.join("fsmonitor");
        std::fs::write(
            &hook,
            format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
        )
        .unwrap();
        make_executable(&hook);
        // Git runs the hook through its shell, which takes a path the POSIX way.
        let hook = crate::platform::shell::posix_path(&hook);
        git(&repo, &["config", "core.fsmonitor", &hook]);
        let runner = Runner::new(
            UserGit::new(SYSTEM_GIT, isolated_git_env()),
            Policy::PLAIN,
            Limits::deadline(PATIENCE),
        );
        let status = ["status", "--porcelain"];

        runner.read(&repo, status).expect("the read runs");
        assert!(!marker.exists(), "a read ran the monitor");

        // The positive control, and the write's own rule: the same status as a write runs it.
        runner.write(&repo, status).expect("the write runs");
        assert!(marker.exists(), "the write ran no monitor");
    }

    /// A stop reads with `git` as its subject, and a `git` that could not start says so; a failed
    /// run is `git`'s own message, with its exit status.
    #[test]
    fn a_runs_failure_has_one_wording() {
        let repo = repo_with("run-wording", &[(b"a.txt", b"one\n")]);
        let runner = Runner::new(
            UserGit::new(SYSTEM_GIT, isolated_git_env()),
            Policy::PLAIN,
            Limits::deadline(Duration::from_millis(100)),
        );
        let stopped = runner
            .write(&repo, ["-c", "alias.nap=!sleep 1", "nap"])
            .expect_err("it is stopped");
        assert_eq!(stopped.code(), None);
        assert_eq!(stopped.to_string(), "git did not finish within 100ms");

        let missing = Runner::new(
            UserGit::new("/nonexistent/git", isolated_git_env()),
            Policy::PLAIN,
            Limits::deadline(PATIENCE),
        )
        .read(&repo, ["status"])
        .expect_err("it cannot start");
        assert!(
            missing
                .to_string()
                .starts_with("running git: spawning the command"),
            "{missing}"
        );

        let failed = runner
            .with_limits(Limits::deadline(PATIENCE))
            .read(&repo, ["rev-parse", "--verify", "refs/heads/none"])
            .expect_err("there is no such branch");
        assert_eq!(failed.code(), Some(128));
        assert_eq!(failed.to_string(), "fatal: Needed a single revision");
    }

    /// A program named `ssh` in a scratch directory that writes the arguments it was started with,
    /// one a line, to `args` beside it, and fails as an unreachable host would.
    fn recording_ssh(dir: &Path) -> std::path::PathBuf {
        let ssh = dir.join("ssh");
        std::fs::write(
            &ssh,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 255\n",
                dir.join("args").display()
            ),
        )
        .unwrap();
        make_executable(&ssh);
        ssh
    }

    /// A fetch built as a remote command in a repository whose own configuration sets
    /// `core.sshCommand` runs that ssh, with the batch option ahead of the user's own arguments.
    #[test]
    fn a_fetch_runs_the_repositorys_ssh_command_unable_to_prompt() {
        let repo = repo_with("remote-ssh-repo", &[(b"a.txt", b"one\n")]);
        let bin = ScratchDir::new("remote-ssh-bin");
        // Git runs the command through its shell, which takes a path the POSIX way.
        let ssh = crate::platform::shell::posix_path(&recording_ssh(&bin));
        git(
            &repo,
            &["config", "core.sshCommand", &format!("{ssh} -i /k")],
        );
        git(
            &repo,
            &["remote", "add", "origin", "ssh://git@example.invalid/r.git"],
        );
        let user = UserGit::new(SYSTEM_GIT, isolated_git_env());
        let configured = user.ssh_configuration(&repo);
        assert_eq!(
            configured,
            SshConfiguration::Command(format!("{ssh} -i /k"))
        );
        let mut fetch = user.remote_command(
            &Policy::PLAIN,
            Access::Write,
            WorkDir::At(&repo),
            &configured,
        );
        fetch.args(["fetch", "--quiet", "origin"]);
        let output = run(&mut fetch, Limits::deadline(PATIENCE), None).expect("the fetch runs");
        assert!(!output.status.success());
        let args = std::fs::read_to_string(bin.join("args")).expect("the ssh ran");
        let args: Vec<&str> = args.lines().collect();
        assert_eq!(args[..3], ["-oBatchMode=yes", "-i", "/k"], "{args:?}");
        assert!(args.contains(&"git@example.invalid"), "{args:?}");
    }

    /// Where nothing sets `core.sshCommand`, the read says so; where the user's own
    /// `GIT_SSH_COMMAND` is set, nothing is read, since it wins.
    #[test]
    fn the_ssh_configuration_is_unset_or_not_read() {
        let repo = repo_with("remote-ssh-unset", &[(b"a.txt", b"one\n")]);
        let user = UserGit::new(SYSTEM_GIT, isolated_git_env());
        assert_eq!(user.ssh_configuration(&repo), SshConfiguration::Unset);
        // A `git` that cannot be run would make the read unknown, were it run.
        let mut env = isolated_git_env();
        assert_eq!(
            UserGit::new("/nonexistent/git", env.clone()).ssh_configuration(&repo),
            SshConfiguration::Unknown
        );
        env.insert("GIT_SSH_COMMAND".to_string(), "ssh -i k".to_string());
        assert_eq!(
            UserGit::new("/nonexistent/git", env).ssh_configuration(&repo),
            SshConfiguration::Unset
        );
    }
}
