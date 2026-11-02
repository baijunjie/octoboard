//! Running an agent's CLI as an account for a short query outside a session — a quota probe, a
//! kick, the use of a reset credit, a model listing, an unarchive, a sign-in's `claude auth
//! status`: the command a session of the account would run ([`adapter::account_command`]),
//! admitted under the account's directory for the whole run. Such a run is held to
//! [`crate::processes::subprocess::Bounds::agent_query`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::accounts::shared_config;
use crate::adapter;
use crate::processes::subprocess::{run_bounded, BoundedFailure, Bounds};
use crate::protocol::Agent;

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
    let command = adapter::account_command(agent, shell_env, config_dir, args)
        .map_err(NotRun::BinaryNotFound)?;
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
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// A shell environment whose `PATH` holds no agent binary.
    fn no_agents(dir: &Path) -> HashMap<String, String> {
        HashMap::from([
            ("PATH".to_string(), dir.to_string_lossy().into_owned()),
            ("HOME".to_string(), dir.to_string_lossy().into_owned()),
        ])
    }

    /// A gone directory is refused before the binary is looked for, so the account's missing
    /// directory is what a caller reports even where the CLI is not installed either.
    #[test]
    fn a_missing_directory_is_refused_before_the_binary_is_resolved() {
        let dir = ScratchDir::new("cli-run-missing");
        let gone = dir.join("gone");
        let refused = admitted_command(Agent::Codex, &no_agents(&dir), Some(&gone), &[])
            .err()
            .expect("refused");
        assert!(matches!(&refused, NotRun::DirMissing(missing) if *missing == gone));
        assert_eq!(
            refused.detail(),
            format!(
                "the account's configuration directory {} does not exist",
                gone.display()
            )
        );
    }

    #[test]
    fn a_binary_off_the_path_is_named() {
        let dir = ScratchDir::new("cli-run-no-binary");
        let refused = admitted_command(Agent::Grok, &no_agents(&dir), Some(&dir), &[])
            .err()
            .expect("refused");
        assert!(matches!(refused, NotRun::BinaryNotFound("grok")));
        assert_eq!(
            refused.detail(),
            "`grok` was not found on PATH in the shell environment"
        );
    }
}
