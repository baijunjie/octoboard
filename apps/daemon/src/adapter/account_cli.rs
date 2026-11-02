//! Running an agent's CLI the way one of its accounts would: where the account's directory is,
//! and a command for a short query of the binary with the account's environment.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::adapter_for;
use crate::agents::layout::layout;
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
/// A caller running it outside a session or its launch claim holds
/// [`crate::accounts::shared_config::admitted_reading`] for the whole run, as
/// [`crate::accounts::cli_run::admitted_command`] does: otherwise a change to the account's
/// sharing can move what the CLI opens, such as Codex's state database and history, from under it.
pub fn account_command(
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

/// [`account_command`] over a `binary` already resolved by [`resolve_account_binary`], for a caller
/// that makes several commands and wants them all to run the one it found.
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

#[cfg(test)]
mod tests;
