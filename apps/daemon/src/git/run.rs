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
//! This is the one runner for every `git` the daemon starts, except one that runs inside the
//! user's hook with the hook's own environment.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::Result;

use crate::env_shell;
use crate::git::env as git_env;
use crate::processes::subprocess::{self, BoundedFailure, Bounds, Reach};

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
        let program = env_shell::resolve_binary("git", &env)?;
        Ok(Self { program, env })
    }

    /// The `git` at `program`, run in `env`.
    pub fn new(program: impl Into<String>, env: HashMap<String, String>) -> Self {
        Self {
            program: program.into(),
            env,
        }
    }

    /// The command `git <policy's options> [<read-only guard>]` in `dir`, for the caller to add
    /// its own `-c` items, subcommand and arguments to. The environment is the user's, cut down
    /// to their own configuration, unable to prompt, with the policy's variables set and, for a
    /// read, the index guard.
    pub fn command(&self, policy: &Policy, access: Access, dir: WorkDir<'_>) -> Command {
        let mut command =
            git_env::non_interactive(&self.program, git_env::user_configuration_only(&self.env));
        command.envs(policy.env.iter().copied());
        command.args(policy.leading);
        if matches!(access, Access::Read) {
            git_env::read_only(&mut command);
        }
        if let WorkDir::At(dir) = dir {
            command.current_dir(dir);
        }
        command
    }
}

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
/// and the variables it sets.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    leading: &'static [&'static str],
    env: &'static [(&'static str, &'static str)],
}

impl Policy {
    /// Nothing beyond the base.
    pub const PLAIN: Self = Self::new(&[], &[]);

    /// `leading` goes ahead of everything else on the command line, the read-only guard's `-c`
    /// included, so a `-c` in it stands before the subcommand as it has to.
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
#[derive(Debug)]
pub enum GitError {
    /// `git` ran and failed. `code` is its exit status, absent when a signal killed it; `stderr`
    /// is its own message, trimmed.
    Exited { code: Option<i32>, stderr: String },
    /// The run was stopped: a timeout, too much output, a cancel, or `git` never started.
    Stopped(BoundedFailure),
}

impl GitError {
    /// `git`'s own message for a failed run, and for a stopped one what stopped it with `git` as
    /// the subject, which a stop's own message does not name.
    pub fn into_message(self) -> String {
        match self {
            Self::Exited { stderr, .. } => stderr,
            Self::Stopped(stopped) => format!("running git: {stopped}"),
        }
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exited { stderr, .. } => formatter.write_str(stderr),
            Self::Stopped(stopped) => write!(formatter, "git {stopped}"),
        }
    }
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
        reach: Reach::ProcessGroup,
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
