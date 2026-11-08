//! The update of an agent's CLI: judging what runs, claiming the agent, running the install's own
//! tool to a file and recording how it ended. The whole feature is described in the module doc of
//! [`super`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::install::ToolCommand;
use super::latest::Registry;
use super::{bounded, check, inspect, registry, tool_reach, Claim, STDERR_LIMIT, TERM_GRACE};
use crate::accounts::choice::refuse_unless_agent_added_to;
use crate::adapter::{self, CliPackaging};
use crate::paths;
use crate::processes::subprocess::{run_to_file, BoundedFailure};
use crate::protocol::{
    agent_cli_code, error_code, AgentCliStatus, AgentCliUpdate, AgentId, CliUpdateState,
    CodedError, ProviderOperation,
};
use crate::state::AppState;
use crate::time::now_millis;

/// How long an update may run before it is stopped.
pub(super) const UPDATE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How the shell environment is taken again once an update ends.
pub(super) type Refresh = Box<dyn FnOnce() -> Result<HashMap<String, String>> + Send>;

/// What an update needs, worked out before anything is claimed or run.
pub(crate) struct Prepared {
    pub(super) agent: AgentId,
    pub(super) shell_env: HashMap<String, String>,
    pub(super) found: AgentCliStatus,
    pub(super) command: ToolCommand,
}

/// Judges whether `agent`'s CLI can be updated on `shell_env` and with what. Blocking.
pub(crate) fn prepare(agent: AgentId, shell_env: HashMap<String, String>) -> Result<Prepared> {
    let packaging = offered_update(agent)?;
    let Some((binary, install, installed_version)) = inspect(agent, &shell_env) else {
        return Err(CodedError::raised(
            error_code::AGENT_NOT_INSTALLED,
            format!(
                "{} is not installed: its binary was not found on PATH in the shell environment.",
                agent.label()
            ),
            &[("agent", agent.label())],
        ));
    };
    let Some(command) = install.update_command(packaging, &binary, &shell_env) else {
        return Err(CodedError::raised(
            error_code::AGENT_CLI_UPDATE_UNSUPPORTED,
            format!(
                "{} cannot be updated from here: it was not installed in a way that is known to \
                 update it, or the tool that updates it (npm or brew) was not found on PATH.",
                agent.label()
            ),
            &[("agent", agent.label())],
        ));
    };
    Ok(Prepared {
        agent,
        found: AgentCliStatus {
            agent,
            installed_version,
            install_method: install.method,
            latest_version: None,
            latest_error: None,
            latest_detail: None,
            checked_at: now_millis(),
            update: None,
        },
        shell_env,
        command,
    })
}

/// How `agent`'s CLI is packaged, or the refusal that its provider knows no way to update it.
fn offered_update(agent: AgentId) -> Result<&'static CliPackaging> {
    let packaging = adapter::provider(agent).cli_packaging();
    if packaging.is_none() {
        return Err(CodedError::provider_unsupported(
            agent,
            ProviderOperation::CliUpdate,
        ));
    }
    Ok(packaging)
}

/// Starts updating the CLI of `agent` and returns once it has started: its progress and outcome
/// are announced as `agent_cli_status_updated`.
pub(crate) async fn update_requested(state: &Arc<AppState>, agent: AgentId) -> Result<()> {
    state.refuse_while_shutting_down()?;
    refuse_unless_agent_added_to(state, agent, "update its CLI")?;
    // Refused before the shell environment is taken: a provider with nothing to update needs none,
    // and a login shell that cannot be captured must not hide that it has none.
    offered_update(agent)?;
    let shell_env = tokio::task::spawn_blocking(update_shell)
        .await?
        .map_err(crate::env_shell::capture_failed)?;
    let prepared = tokio::task::spawn_blocking(move || prepare(agent, shell_env)).await??;
    start_update(
        state,
        prepared,
        UPDATE_TIMEOUT,
        registry(),
        Box::new(refreshed_shell),
        &paths::agent_cli_updates_dir(),
    )
}

/// [`update_requested`] after its judging: claims the agent, marks the update started and runs it
/// on a blocking task, with the time it may take, where the latest version is read from, how the
/// shell environment is taken again once it ends, and the directory the tool's output file goes
/// in.
pub(crate) fn start_update(
    state: &Arc<AppState>,
    prepared: Prepared,
    timeout: Duration,
    registry: &'static dyn Registry,
    refresh: Refresh,
    output_dir: &Path,
) -> Result<()> {
    state.refuse_while_shutting_down()?;
    let claim = state.agent_clis.begin_update(state, prepared.agent)?;
    state
        .agent_clis
        .record_update_started(state, prepared.found.clone());
    tracing::info!(agent = %prepared.agent, command = %prepared.command.display(), "updating the agent's CLI");
    let state = state.clone();
    let output = output_dir.join(format!("{}.log", prepared.agent));
    tokio::task::spawn_blocking(move || {
        run_update(&state, claim, prepared, timeout, registry, refresh, &output)
    });
    Ok(())
}

/// Runs the update, then takes the shell environment again (the install may have changed `PATH`
/// or what it finds), judges the agent's availability on it and reads the status afresh, which is
/// announced with the update's outcome. A daemon that has begun to shut down while the update ran
/// does none of that, and records the outcome on the status it holds. The tool's output goes to
/// the file `output`, one per agent and overwritten by each run. Blocking.
pub(super) fn run_update(
    state: &Arc<AppState>,
    claim: Claim,
    prepared: Prepared,
    timeout: Duration,
    registry: &dyn Registry,
    refresh: Refresh,
    output: &Path,
) {
    let started_at = state
        .agent_clis
        .lock()
        .statuses
        .get(&prepared.agent)
        .and_then(|status| status.update.as_ref().map(|update| update.started_at))
        .unwrap_or_else(now_millis);
    let ran = run_update_tool(&prepared.command, &prepared.shell_env, timeout, output);
    let outcome = match ran {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err((
            agent_cli_code::UPDATE_FAILED,
            Some(output_failure_line(
                output,
                &format!("the update ended with {status}"),
            )),
        )),
        Err(BoundedFailure::TimedOut(_)) => Err((agent_cli_code::UPDATE_TIMED_OUT, None)),
        Err(other) => Err((
            agent_cli_code::UPDATE_FAILED,
            Some(bounded(&other.to_string())),
        )),
    };
    let mut status = if state.is_shutting_down() {
        let held = state
            .agent_clis
            .lock()
            .statuses
            .get(&prepared.agent)
            .cloned();
        held.unwrap_or_else(|| prepared.found.clone())
    } else {
        let shell_env = refresh().unwrap_or_else(|err| {
            tracing::warn!(agent = %prepared.agent, %err, "could not take the shell environment again after the update");
            prepared.shell_env.clone()
        });
        crate::agents::availability::redetermine(state, prepared.agent, &shell_env);
        check(prepared.agent, &shell_env, registry)
    };
    status.update = Some(match outcome {
        Ok(()) => AgentCliUpdate {
            state: CliUpdateState::Succeeded,
            started_at,
            finished_at: Some(now_millis()),
            error: None,
            detail: None,
        },
        Err((code, detail)) => AgentCliUpdate {
            state: CliUpdateState::Failed,
            started_at,
            finished_at: Some(now_millis()),
            error: Some(code),
            detail,
        },
    });
    state.agent_clis.record(state, status);
    drop(claim);
}

/// Runs the update `command` on `shell_env` within `timeout`, its output written to the file
/// `output`. Held as [`run_tool`] holds a tool, but with no pipe to the daemon, so that it is not
/// broken by the daemon being stopped while it runs, and with nothing that cancels it.
pub(super) fn run_update_tool(
    command: &ToolCommand,
    shell_env: &HashMap<String, String>,
    timeout: Duration,
    output: &Path,
) -> std::result::Result<std::process::ExitStatus, BoundedFailure> {
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| BoundedFailure::Failed(anyhow::Error::new(err)))?;
    }
    run_to_file(
        &mut command.command(shell_env),
        output,
        timeout,
        tool_reach(command),
        TERM_GRACE,
    )
}

/// The prefixes npm puts on every line of a failure, by version.
const NPM_PREFIXES: [&str; 2] = ["npm error", "npm ERR!"];

/// What npm's closing pointer to its own log says; it names no cause.
const LOG_POINTERS: [&str; 2] = [
    "A complete log of this run can be found in",
    "Log files were not written",
];

/// How [`error_line`] reads one line of an update's output.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// A line of npm's failure that carries something: its `code` line, a field, a sentence.
    Npm,
    /// A line of npm's failure that carries nothing: a bare prefix, the pointer to its log and,
    /// in npm 6 to 8, the path of the log on the line after it.
    NpmFiller,
    /// Any other line that reads as an error, such as Homebrew's `Error: ...`.
    Error,
    Plain,
}

/// The line of `bytes` that says what went wrong. npm prints a failure as a block, every line
/// prefixed, that opens with its code (`npm error code EACCES`) and goes on to fields and advice
/// that make no sense alone, so for npm it is the first line of the last such block that carries
/// something; for a tool that prints one line, such as Homebrew, the last that reads as an error.
/// Where none does, the last non-empty line.
pub(super) fn error_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let mut after_pointer = false;
    let readings: Vec<Reading> = lines
        .iter()
        .map(|line| {
            let Some(rest) = NPM_PREFIXES
                .iter()
                .find_map(|prefix| line.strip_prefix(prefix))
            else {
                after_pointer = false;
                let reads_as_error = line.starts_with("Error:") || line.starts_with("error:");
                return if reads_as_error {
                    Reading::Error
                } else {
                    Reading::Plain
                };
            };
            let is_pointer = LOG_POINTERS.iter().any(|pointer| rest.contains(pointer));
            let filler = rest.trim().is_empty() || is_pointer || after_pointer;
            after_pointer = is_pointer && rest.trim_end().ends_with(':');
            if filler {
                Reading::NpmFiller
            } else {
                Reading::Npm
            }
        })
        .collect();
    let said = readings
        .iter()
        .rposition(|reading| matches!(reading, Reading::Npm | Reading::Error));
    let line = match said {
        Some(last) if readings[last] == Reading::Npm => {
            let mut first = last;
            while first > 0 && matches!(readings[first - 1], Reading::Npm | Reading::NpmFiller) {
                first -= 1;
            }
            (first..=last).find(|&at| readings[at] == Reading::Npm)?
        }
        Some(last) => last,
        None => lines.len().checked_sub(1)?,
    };
    Some(lines[line].to_owned())
}

/// The line in the tail of the output file `path` an update wrote that says what went wrong (see
/// [`error_line`]), else `fallback`.
fn output_failure_line(path: &Path, fallback: &str) -> String {
    use std::io::{Read, Seek, SeekFrom};

    let tail = || -> std::io::Result<Vec<u8>> {
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        file.seek(SeekFrom::Start(len.saturating_sub(STDERR_LIMIT as u64)))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    bounded(
        &tail()
            .ok()
            .as_deref()
            .and_then(error_line)
            .unwrap_or_else(|| fallback.to_owned()),
    )
}

/// The shell environment an update runs in, as a launch takes it.
#[cfg(not(test))]
fn update_shell() -> Result<HashMap<String, String>> {
    crate::env_shell::launch_snapshot()
}

/// The shell environment taken afresh once an update ends, which the launches after it then use.
#[cfg(not(test))]
fn refreshed_shell() -> Result<HashMap<String, String>> {
    crate::env_shell::snapshot()
}

#[cfg(test)]
fn update_shell() -> Result<HashMap<String, String>> {
    anyhow::bail!("no shell environment under test")
}

#[cfg(test)]
fn refreshed_shell() -> Result<HashMap<String, String>> {
    anyhow::bail!("no shell environment under test")
}
