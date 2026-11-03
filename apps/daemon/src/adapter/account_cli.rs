//! Running an agent's CLI as one of its accounts: where the account's directory is, the command a
//! session of the account would run, and the short queries outside a session — a quota probe, a
//! kick, the use of a reset credit, a model listing, an unarchive, a sign-in's `claude auth
//! status` — admitted under the account's directory for the whole run and held to
//! [`Bounds::agent_query`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::adapter_for;
use crate::accounts::shared_config;
use crate::agents::layout::layout;
use crate::processes::subprocess::{run_bounded, BoundedFailure, Bounds};
use crate::protocol::Agent;

/// The home directory the launch's shell snapshot carries, else the daemon's own.
pub(crate) fn snapshot_home(shell_env: &HashMap<String, String>) -> PathBuf {
    shell_env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(crate::paths::home_dir)
}

/// The directory the shell snapshot exports `var` as, when it exports one; an empty value counts as
/// unset.
pub(crate) fn exported_dir(shell_env: &HashMap<String, String>, var: &str) -> Option<PathBuf> {
    shell_env
        .get(var)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// The directory this agent's default account resolves to — what "pinning nothing" means at
/// launch: the directory its own variable is exported to in `shell_env`, else the agent's own
/// usual default under the home directory the snapshot carries, else the daemon's own. For
/// Grok Build this is the *source* home a session's per-session home is built from
/// (`grok::home::build_grok_home`), never the per-session home itself, which is Octoboard's own and
/// exists only for the duration of a process.
///
/// Shared by `crate::agents::availability`, which shows it as the default account's directory
/// before any session has opened, and by [`account_dir`], which a launch with no pinned account
/// resolves its directory through.
pub fn default_account_dir(agent: Agent, shell_env: &HashMap<String, String>) -> PathBuf {
    let layout = layout(agent);
    exported_dir(shell_env, layout.config_dir_var)
        .unwrap_or_else(|| snapshot_home(shell_env).join(layout.default_dir))
}

/// The directory of an account of `agent` as a session of it would see it: `pinned`, the account's
/// own, else the default account's ([`default_account_dir`]) resolved from `shell_env`.
pub fn account_dir(
    agent: Agent,
    pinned: Option<&Path>,
    shell_env: &HashMap<String, String>,
) -> PathBuf {
    pinned.map_or_else(|| default_account_dir(agent, shell_env), Path::to_path_buf)
}

/// A command running `agent`'s binary with `args` the way a session of the account would run it,
/// for a short query of the CLI rather than a launch: the binary resolved on `shell_env`'s
/// `PATH`, the environment the snapshot's and no more, the account's directory exported under
/// the agent's own variable when `config_dir` names one (left as the shell has it for the
/// default account), and the home directory as working directory, so no project-level
/// configuration of whatever directory the daemon started in takes part. `Err` carries the
/// binary's name when it does not resolve.
///
/// [`admitted_command`] is the way to run it.
fn account_command(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> Result<std::process::Command, &'static str> {
    let binary = resolve_account_binary(agent, shell_env)?;
    Ok(account_command_of(
        &binary, agent, shell_env, config_dir, args,
    ))
}

/// Where `agent`'s binary resolves on `shell_env`'s `PATH`; `Err` carries the binary's name.
pub fn resolve_account_binary(
    agent: Agent,
    shell_env: &HashMap<String, String>,
) -> Result<String, &'static str> {
    let binary_name = adapter_for(agent).binary();
    crate::env_shell::resolve_binary(binary_name, shell_env).map_err(|_| binary_name)
}

/// `account_command` over a `binary` already resolved by [`resolve_account_binary`], for a caller
/// that makes several commands and wants them all to run the one it found. The caller keeps the
/// account's directory admitted for the run (`shared_config::admitted_reading`, or `running_long`
/// for a long one), so sharing cannot move what the CLI opens from under it.
pub fn account_command_of(
    binary: &str,
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> std::process::Command {
    let mut command = std::process::Command::new(binary);
    command.args(args).env_clear().envs(shell_env);
    if let Some(dir) = config_dir {
        command.env(layout(agent).config_dir_var, dir);
    }
    command.current_dir(crate::paths::home_dir());
    command
}

/// A command running an agent's CLI as an account, and the account's directory admitted for it:
/// no change of the account's sharing moves what the CLI opens, such as Codex's state database
/// and history, from under it until this is dropped.
pub(crate) struct AdmittedCommand {
    pub(crate) command: Command,
    _reading: Option<shared_config::Run>,
}

impl AdmittedCommand {
    /// Runs the command as a short query ([`run_query`]), the account's directory held until this
    /// returns.
    pub(crate) fn run(
        &mut self,
        timeout: Duration,
        limit: usize,
        cancel: Option<&AtomicBool>,
    ) -> Result<Output, BoundedFailure> {
        run_query(&mut self.command, timeout, limit, cancel)
    }
}

/// Why a short query was not run.
#[derive(Debug)]
pub(crate) enum NotRun {
    /// The account's directory is gone once the run is admitted: a removal of the account may
    /// have moved it while the run waited, and the CLI would create it again (Grok) or refuse it
    /// (Codex).
    DirMissing(PathBuf),
    /// The agent's binary does not resolve on the shell environment's `PATH`; carries its name.
    BinaryNotFound(&'static str),
}

impl NotRun {
    /// What went wrong, for the failure a caller reports.
    pub(crate) fn detail(&self) -> String {
        match self {
            Self::DirMissing(dir) => format!(
                "the account's configuration directory {} does not exist",
                dir.display()
            ),
            Self::BinaryNotFound(binary) => {
                format!("`{binary}` was not found on PATH in the shell environment")
            }
        }
    }
}

/// `agent`'s CLI with `args`, run as the account whose directory is `config_dir` (`None` the
/// default account) would run it, admitted under that directory. Blocking: the admission waits
/// for a change of the account's sharing that is moving files there.
pub(crate) fn admitted_command(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> Result<AdmittedCommand, NotRun> {
    let reading = shared_config::admitted_reading(config_dir)
        .map_err(|missing| NotRun::DirMissing(missing.0))?;
    let command =
        account_command(agent, shell_env, config_dir, args).map_err(NotRun::BinaryNotFound)?;
    Ok(AdmittedCommand {
        command,
        _reading: reading,
    })
}

/// Why [`query`] produced no output.
#[derive(Debug)]
pub(crate) enum QueryFailure {
    /// The command was not started.
    NotRun(NotRun),
    /// The run was stopped or could not be carried out.
    Bounded(BoundedFailure),
}

/// Runs `command` as a short query of an agent CLI: held to [`Bounds::agent_query`] with `timeout`
/// and a stdout `limit`, and stopped by setting `cancel` (`None` never stops it). Blocking. It
/// does not admit the command's directory, so it is only for a command whose directory is already
/// held (a sign-in's, for one); [`query`] is for the rest.
pub(crate) fn run_query(
    command: &mut Command,
    timeout: Duration,
    limit: usize,
    cancel: Option<&AtomicBool>,
) -> Result<Output, BoundedFailure> {
    let never = AtomicBool::new(false);
    run_bounded(
        command,
        &Bounds::agent_query(timeout, limit),
        cancel.unwrap_or(&never),
    )
}

/// [`admitted_command`] with `args` run as a short query ([`run_query`]). Blocking.
pub(crate) fn query(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
    timeout: Duration,
    limit: usize,
    cancel: Option<&AtomicBool>,
) -> Result<Output, QueryFailure> {
    admitted_command(agent, shell_env, config_dir, args)
        .map_err(QueryFailure::NotRun)?
        .run(timeout, limit, cancel)
        .map_err(QueryFailure::Bounded)
}

/// The last line of what a query printed on stderr, trimmed; empty when it printed none.
pub(crate) fn last_stderr_line(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .trim()
        .lines()
        .next_back()
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests;
