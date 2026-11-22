//! Running an agent's CLI as one of its accounts: where the account's directory is, the command a
//! session of the account would run, and the short queries outside a session — a quota probe, a
//! kick, the use of a reset credit, a model listing, an unarchive, a sign-in's `claude auth
//! status` — admitted under the account's directory for the whole run and held to
//! [`Bounds::agent_query`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::accounts::shared_config;
use crate::platform::proc::Reach;
use crate::platform::shell::ResolvedBinary;
use crate::processes::subprocess::{run_bounded, run_to_file, BoundedFailure, Bounds};
use crate::protocol::{AgentId, CodedError};
use crate::providers::layout::layout;
use crate::text::truncate;

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
pub fn default_account_dir(agent: AgentId, shell_env: &HashMap<String, String>) -> PathBuf {
    let layout = layout(agent);
    exported_dir(shell_env, layout.config_dir_var)
        .unwrap_or_else(|| snapshot_home(shell_env).join(layout.default_dir))
}

/// The directory of an account of `agent` as a session of it would see it: `pinned`, the account's
/// own, else the default account's ([`default_account_dir`]) resolved from `shell_env`.
pub fn account_dir(
    agent: AgentId,
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
/// configuration of whatever directory the daemon started in takes part. `Err` is why the binary
/// cannot start the CLI.
///
/// [`admitted_command`] is the way to run it.
fn account_command(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> Result<std::process::Command, Unstartable> {
    let binary = resolve_account_binary(agent, shell_env)?;
    Ok(account_command_of(
        &binary, agent, shell_env, config_dir, args,
    ))
}

/// Why an agent's binary cannot start its CLI.
#[derive(Debug)]
pub enum Unstartable {
    /// The binary does not resolve on the shell environment's `PATH`; carries its name.
    NotFound(&'static str),
    /// The binary is a launcher whose native program is missing, which the provider does not start
    /// the CLI through (`AgentProvider::native_binary`); carries the provider's coded error.
    Refused(CodedError),
}

impl Unstartable {
    /// The refusal a request answers with: `binary_not_found` naming the binary, or the provider's
    /// own coded error.
    pub(crate) fn into_error(self) -> anyhow::Error {
        match self {
            Self::NotFound(binary) => CodedError::raised(
                crate::protocol::error_code::BINARY_NOT_FOUND,
                format!("`{binary}` was not found on PATH in the shell environment."),
                &[("binary", binary)],
            ),
            Self::Refused(refusal) => anyhow::Error::new(refusal),
        }
    }
}

/// What starts `agent`'s CLI as resolved on `shell_env`'s `PATH`, as a launch resolves it
/// (`super::resolve_binary`).
pub fn resolve_account_binary(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
) -> Result<ResolvedBinary, Unstartable> {
    let name = agent.descriptor().binary;
    let binary = crate::env_shell::resolve_binary(name, shell_env)
        .map_err(|_| Unstartable::NotFound(name))?;
    super::starting_program(agent, binary).map_err(|err| Unstartable::Refused(CodedError::of(&err)))
}

/// `account_command` over a `binary` already resolved by [`resolve_account_binary`], for a caller
/// that makes several commands and wants them all to run the one it found. The caller keeps the
/// account's directory admitted for the run (`shared_config::admitted_reading`, or `running_long`
/// for a long one), so sharing cannot move what the CLI opens from under it.
pub fn account_command_of(
    binary: &ResolvedBinary,
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> std::process::Command {
    let mut command = binary.command();
    command.args(args).env_clear().envs(shell_env);
    if let Some(dir) = config_dir {
        command.env(layout(agent).config_dir_var, dir);
    }
    command.current_dir(crate::paths::home_dir());
    command
}

/// The runs of an agent's CLI that Octoboard makes on its own, outside a session, for which a
/// provider may give a directory of Octoboard's own ([`RunHome`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundRun {
    QuotaReading,
    ModelListing,
    Kick,
}

/// How long a run may wait for the directory it is made against, where another run holds it: until
/// the run's own deadline, so that the wait comes out of the time the run is given rather than on
/// top of it, and no longer than the caller goes on wanting the run.
pub(crate) struct RunWait<'a> {
    pub(crate) until: Instant,
    /// Set to give the wait up at once, as it stops the run itself.
    pub(crate) cancel: Option<&'a AtomicBool>,
}

/// A directory of Octoboard's own that a run of an agent's CLI is made against in place of the
/// account's, for a CLI measured to change the account's configuration when it is run against the
/// account's own directory: the provider builds it from the account's
/// (`AccountOps::run_home`) and says which variables reach the account's sign-in from it. The
/// account's directory is still what the run is admitted under, and the home is built inside that
/// admission, so no change of the account's sharing moves what it is built from meanwhile.
pub(crate) struct RunHome {
    pub(crate) dir: PathBuf,
    /// Set on top of the shell snapshot, after the agent's own directory variable.
    pub(crate) env: Vec<(String, String)>,
    /// What the provider does to the directory once the run has ended, while it is still this
    /// run's alone: removing what a run leaves there that nothing reads again.
    pub(crate) after_run: Option<Box<dyn FnOnce() + Send>>,
    /// What keeps the directory this run's alone, held until the run has ended: two runs of one
    /// kind for one account would otherwise rewrite the same files under each other.
    pub(crate) _lock: Option<std::fs::File>,
}

impl Drop for RunHome {
    /// The run has ended, or was never made: the provider's work for after it is done before the
    /// lock is let go, which the fields' own drop does next.
    fn drop(&mut self) {
        if let Some(after_run) = self.after_run.take() {
            after_run();
        }
    }
}

/// A command running an agent's CLI as an account, and the account's directory admitted for it:
/// no change of the account's sharing moves what the CLI opens, such as Codex's state database
/// and history, from under it until this is dropped.
pub(crate) struct AdmittedCommand {
    pub(crate) command: Command,
    _reading: Option<shared_config::Run>,
    /// The directory of Octoboard's own the command runs against, where the provider gave one,
    /// kept with its lock for as long as the command is.
    home: Option<RunHome>,
}

impl AdmittedCommand {
    /// The directory of Octoboard's own the command runs against in place of the account's, where
    /// the provider gave one ([`RunHome`]).
    pub(crate) fn home_dir(&self) -> Option<&Path> {
        self.home.as_ref().map(|home| home.dir.as_path())
    }

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

    /// Runs the command to its end with its output in the file `output`, held to `timeout`, and
    /// leaves alone what it started when it ends ([`Reach::Group`], and no pipe for a descendant to
    /// hold open): for a CLI that hands its work to an application that must outlive it, which
    /// [`Self::run`], reaching the CLI's whole session, would end along with it.
    pub(crate) fn run_leaving_descendants(
        &mut self,
        output: &Path,
        timeout: Duration,
    ) -> Result<ExitStatus, BoundedFailure> {
        run_to_file(
            &mut self.command,
            output,
            timeout,
            Reach::Group,
            Duration::ZERO,
        )
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
    /// The agent's binary is a launcher the provider refuses to start the CLI through; carries the
    /// provider's coded error ([`Unstartable::Refused`]).
    BinaryRefused(CodedError),
    /// The directory of Octoboard's own the run was to be made against could not be made ready
    /// ([`RunHome`]); carries why. The account's own directory is never fallen back to.
    HomeNotPrepared(String),
}

impl From<Unstartable> for NotRun {
    fn from(unstartable: Unstartable) -> Self {
        match unstartable {
            Unstartable::NotFound(binary) => Self::BinaryNotFound(binary),
            Unstartable::Refused(err) => Self::BinaryRefused(err),
        }
    }
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
            Self::BinaryRefused(refusal) => refusal.message.clone(),
            Self::HomeNotPrepared(why) => {
                format!("the directory the command runs in could not be prepared: {why}")
            }
        }
    }
}

/// `agent`'s CLI with `args`, run as the account whose directory is `config_dir` (`None` the
/// default account) would run it, admitted under that directory. Blocking: the admission waits
/// for a change of the account's sharing that is moving files there.
pub(crate) fn admitted_command(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> Result<AdmittedCommand, NotRun> {
    let reading = shared_config::admitted_reading(config_dir)
        .map_err(|missing| NotRun::DirMissing(missing.0))?;
    let command = account_command(agent, shell_env, config_dir, args)?;
    Ok(AdmittedCommand {
        command,
        _reading: reading,
        home: None,
    })
}

/// [`admitted_command`] for one of the runs Octoboard makes on its own ([`BackgroundRun`]): where
/// the provider gives a directory of Octoboard's own for it under `run_homes`
/// (`AccountOps::run_home`), the command runs against that directory, built once the account's
/// directory is admitted; elsewhere against the account's. A directory that cannot be made ready,
/// or that another run still holds when `wait` runs out, refuses the run
/// ([`NotRun::HomeNotPrepared`]). The account stays admitted while the directory is waited for,
/// which `wait` bounds by the run's own deadline.
pub(crate) fn admitted_command_in(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    run: BackgroundRun,
    run_homes: &Path,
    wait: &RunWait<'_>,
    args: &[&str],
) -> Result<AdmittedCommand, NotRun> {
    let reading = shared_config::admitted_reading(config_dir)
        .map_err(|missing| NotRun::DirMissing(missing.0))?;
    // Before the directory is made: a CLI that is not there leaves none behind.
    let binary = resolve_account_binary(agent, shell_env)?;
    let home = match super::provider(agent).accounts() {
        Some(ops) => ops
            .run_home(
                run_homes,
                run,
                &account_dir(agent, config_dir, shell_env),
                shell_env,
                wait,
            )
            .map_err(|err| NotRun::HomeNotPrepared(format!("{err:#}")))?,
        None => None,
    };
    let command = match &home {
        Some(home) => {
            let mut command = account_command_of(&binary, agent, shell_env, Some(&home.dir), args);
            command.envs(home.env.iter().map(|(name, value)| (name, value)));
            command
        }
        None => account_command_of(&binary, agent, shell_env, config_dir, args),
    };
    Ok(AdmittedCommand {
        command,
        _reading: reading,
        home,
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
    agent: AgentId,
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

/// The most characters of one stream's line that [`failure_detail`] keeps.
const DETAIL_LINE_LIMIT: usize = 1000;

/// What says why a query failed: the last line of its stdout and of its stderr, whichever it
/// printed, each cut to [`DETAIL_LINE_LIMIT`] characters and passed through `line`; empty when it
/// printed neither. Claude Code writes some failures to stdout while its stderr ends in unrelated
/// warnings (settings an untrusted directory ignores), and which stream holds the reason is not
/// known for every CLI, so neither is dropped. Stdout is left out when it, or its last line, is
/// JSON: that is the query's own answer (a status object, a server listing with its headers and
/// environment), not a reason, and may carry what the caller never means to show.
pub(crate) fn failure_detail(output: &Output, line: impl Fn(String) -> String) -> String {
    let stdout = last_line(&output.stdout);
    let stdout = if is_json(&stdout) || is_json(&String::from_utf8_lossy(&output.stdout)) {
        String::new()
    } else {
        line(truncate(&stdout, DETAIL_LINE_LIMIT))
    };
    let stderr = line(truncate(&last_line(&output.stderr), DETAIL_LINE_LIMIT));
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout} (stderr: {stderr})"),
        (false, true) => stdout,
        _ => stderr,
    }
}

/// The last line of a stream an agent CLI printed, trimmed; empty when it printed none.
fn last_line(stream: &[u8]) -> String {
    String::from_utf8_lossy(stream)
        .trim()
        .lines()
        .next_back()
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// Whether `text` is one JSON object or array.
fn is_json(text: &str) -> bool {
    let text = text.trim();
    text.starts_with(['{', '[']) && serde_json::from_str::<serde_json::Value>(text).is_ok()
}

#[cfg(test)]
mod tests;
