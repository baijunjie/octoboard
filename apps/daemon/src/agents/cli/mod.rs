//! What the daemon knows about an agent's installed CLI beyond launching it: its version, how it
//! was installed, the latest version published and an update of it that runs or ran. See "Agent
//! CLI versions" in `apps/daemon/PROTOCOL.md`.
//!
//! A check ([`check_requested`]) and an update ([`update_requested`], in [`update`]) are both
//! started by a client and neither happens on the daemon's own: a check is asked for by whatever shows the versions,
//! when it shows them, since the daemon cannot know what any window displays, and an update is
//! only ever the user's choice. The statuses are runtime state held in [`AgentClis`], never
//! stored, announced as `agent_cli_status_updated` and replayed in a `snapshot`.
//!
//! How the CLI was installed ([`install`]) decides both what updates it and where the latest
//! version is read from ([`latest`]); an install that is not recognised is shown with its version
//! and is never updated. An update runs the install's own tool with a long timeout while the
//! agent's sessions keep running: a running session keeps the process it has, and a session
//! started afterwards finds the new version on the refreshed shell environment. When it ends the
//! shell environment is taken again, the agent's availability is judged again on it and the status
//! is read afresh, so one `agent_cli_status_updated` carries the outcome and the new versions.
//! The tool writes its output to a file, not to a pipe of the daemon's, so that an update still
//! running when the daemon is stopped is not broken by it. A daemon that has begun to shut down
//! refuses a new update, and one that ends after the shutdown began only records its outcome:
//! it takes no shell environment and checks nothing.

pub(crate) mod install;
pub(crate) mod latest;
#[cfg(test)]
mod tests;
mod update;

pub(crate) use update::update_requested;
#[cfg(test)]
use update::{error_line, prepare};
#[cfg(all(test, unix))]
use update::{run_update_tool, start_update, Refresh, UPDATE_TIMEOUT};

use std::collections::HashMap;
use std::process::Output;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::accounts::choice::agent_is_available;
use crate::adapter::{self, account_cli, AgentVersion};
use crate::platform::proc::Reach;
use crate::platform::shell::ResolvedBinary;
use crate::processes::subprocess::{run_bounded, BoundedFailure, Bounds};
use crate::protocol::{
    agent_cli_code, error_code, AgentCliStatus, AgentCliUpdate, AgentId, CliInstallMethod,
    CliUpdateState, CodedError, Event,
};
use crate::state::AppState;
use crate::time::now_millis;
use install::{Install, ToolCommand};
use latest::Registry;

/// How long `--version` may take where the install does not tell the version.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// How soon after an agent's CLI was last checked a client's request to check it again is left
/// alone. Several clients can be connected and each asks on its own schedule, so an in-flight
/// claim alone would let N of them turn one check per interval into N.
const CHECK_FLOOR: Duration = Duration::from_secs(30);

/// The most of a tool's output kept for the line that says why it failed: the stderr of one the
/// daemon reads through pipes, the tail of an update's output file.
const STDERR_LIMIT: usize = 256 * 1024;

/// The longest failure line carried to clients, in characters.
const DETAIL_LIMIT: usize = 300;

/// How long a stop leaves a tool to exit after `SIGTERM` before it is killed.
const TERM_GRACE: Duration = Duration::from_secs(6);

/// The statuses and what is running, per agent.
#[derive(Default)]
pub struct AgentClis {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    statuses: HashMap<AgentId, AgentCliStatus>,
    held: HashMap<AgentId, Held>,
}

#[derive(Default)]
struct Held {
    checking: bool,
    updating: bool,
    /// When the last check or update of the agent ended.
    last_ended: Option<Instant>,
    /// Moved on whenever an update starts: a check that began before it read versions the update
    /// may have changed, so its result is dropped when it lands after.
    generation: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Work {
    Check,
    Update,
}

/// The right to check or update one agent's CLI, released when dropped: the work runs on a
/// blocking task that can panic, and an agent whose claim stays held would be left out of every
/// later check or update for the rest of the daemon's life. An update's claim that is dropped
/// with the status still `running` (the task panicked) ends it as failed, so no client waits on
/// an update that is no longer running.
pub(crate) struct Claim {
    state: Arc<AppState>,
    agent: AgentId,
    work: Work,
    /// The agent's [`Held::generation`] when the claim was taken.
    generation: u64,
}

impl Drop for Claim {
    fn drop(&mut self) {
        let mut inner = self.state.agent_clis.lock();
        let held = inner.held.entry(self.agent).or_default();
        match self.work {
            Work::Check => held.checking = false,
            Work::Update => held.updating = false,
        }
        held.last_ended = Some(Instant::now());
        if self.work == Work::Update {
            if let Some(status) = inner.statuses.get_mut(&self.agent) {
                let running = status
                    .update
                    .as_mut()
                    .filter(|update| update.state == CliUpdateState::Running);
                if let Some(update) = running {
                    update.state = CliUpdateState::Failed;
                    update.finished_at = Some(now_millis());
                    update.error = Some(agent_cli_code::UPDATE_FAILED);
                    update.detail = Some("internal error".to_owned());
                    let status = status.clone();
                    self.state
                        .broadcast(Event::AgentCliStatusUpdated { status });
                }
            }
        }
    }
}

impl AgentClis {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("agent CLI statuses lock poisoned")
    }

    /// Every status held, in the agents' own order.
    pub fn statuses(&self) -> Vec<AgentCliStatus> {
        let inner = self.lock();
        AgentId::ALL
            .into_iter()
            .filter_map(|agent| inner.statuses.get(&agent).cloned())
            .collect()
    }

    /// The claim to check `agent`, or `None` while it is being checked or updated or was checked
    /// or updated within [`CHECK_FLOOR`].
    fn begin_check(&self, state: &Arc<AppState>, agent: AgentId) -> Option<Claim> {
        let mut inner = self.lock();
        let held = inner.held.entry(agent).or_default();
        let recent = held
            .last_ended
            .is_some_and(|ended| ended.elapsed() < CHECK_FLOOR);
        if held.checking || held.updating || recent {
            return None;
        }
        held.checking = true;
        Some(Claim {
            state: state.clone(),
            agent,
            work: Work::Check,
            generation: held.generation,
        })
    }

    /// The claim to update `agent`, or the refusal that one is running. A check running meanwhile
    /// is left to end, but what it reads is older than the update, so its result is dropped.
    fn begin_update(&self, state: &Arc<AppState>, agent: AgentId) -> Result<Claim> {
        let mut inner = self.lock();
        let held = inner.held.entry(agent).or_default();
        if held.updating {
            return Err(CodedError::raised(
                error_code::AGENT_CLI_UPDATE_RUNNING,
                format!("{} is already being updated.", agent.label()),
                &[("agent", agent.label())],
            ));
        }
        held.updating = true;
        held.generation += 1;
        Ok(Claim {
            state: state.clone(),
            agent,
            work: Work::Update,
            generation: held.generation,
        })
    }

    /// Records the result of a check taken under `claim`, keeping the update the status already
    /// carries, and announces it; dropped when an update started since the claim was taken.
    fn record_checked(&self, state: &AppState, claim: &Claim, mut status: AgentCliStatus) {
        let mut inner = self.lock();
        if inner
            .held
            .get(&claim.agent)
            .is_some_and(|held| held.generation != claim.generation)
        {
            return;
        }
        status.update = inner
            .statuses
            .get(&status.agent)
            .and_then(|held| held.update.clone());
        inner.statuses.insert(status.agent, status.clone());
        state.broadcast(Event::AgentCliStatusUpdated { status });
    }

    /// Records `status` whole, an update's last word included, and announces it.
    fn record(&self, state: &AppState, status: AgentCliStatus) {
        let mut inner = self.lock();
        inner.statuses.insert(status.agent, status.clone());
        state.broadcast(Event::AgentCliStatusUpdated { status });
    }

    /// Marks an update as started on the status held for the agent, or on `found` where none is
    /// held yet, and announces it.
    fn record_update_started(&self, state: &AppState, found: AgentCliStatus) {
        let mut inner = self.lock();
        let mut status = inner.statuses.remove(&found.agent).unwrap_or(found);
        status.update = Some(AgentCliUpdate {
            state: CliUpdateState::Running,
            started_at: now_millis(),
            finished_at: None,
            error: None,
            detail: None,
        });
        inner.statuses.insert(status.agent, status.clone());
        state.broadcast(Event::AgentCliStatusUpdated { status });
    }
}

/// Starts checking the CLI of `agent`, or of every agent that is not known to be missing (one
/// whose availability is not determined yet included), and returns at once: each result is
/// announced when it lands. An agent already being checked or updated, or checked within the last
/// [`CHECK_FLOOR`], is left alone. Where the shell environment cannot be taken nothing is
/// announced for the round.
pub(crate) fn check_requested(state: &Arc<AppState>, agent: Option<AgentId>) {
    let agents: Vec<AgentId> = match agent {
        Some(agent) => vec![agent],
        None => AgentId::ALL
            .into_iter()
            .filter(|agent| agent_is_available(state, *agent))
            .collect(),
    };
    for agent in agents {
        let Some(claim) = state.agent_clis.begin_check(state, agent) else {
            continue;
        };
        let state = state.clone();
        tokio::task::spawn_blocking(move || match check_shell() {
            Ok(shell_env) => {
                let status = check(agent, &shell_env, registry());
                state.agent_clis.record_checked(&state, &claim, status);
            }
            Err(err) => tracing::warn!(%agent, %err, "could not check the agent's CLI"),
        });
    }
}

/// Reads how `agent`'s CLI is installed and what it is, as it resolves on `shell_env`. Blocking.
pub(crate) fn check(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
    registry: &dyn Registry,
) -> AgentCliStatus {
    let packaging = adapter::provider(agent).cli_packaging();
    let mut status = AgentCliStatus {
        agent,
        installed_version: None,
        install_method: CliInstallMethod::Unknown,
        latest_version: None,
        latest_error: None,
        latest_detail: None,
        checked_at: 0,
        update: None,
    };
    if let Some((_, install, installed_version)) = inspect(agent, shell_env) {
        status.installed_version = installed_version;
        status.install_method = install.method;
        match latest::latest_version(agent, packaging, &install, shell_env, registry) {
            Some(Ok(version)) => status.latest_version = Some(version),
            Some(Err(failure)) => {
                status.latest_error = Some(failure.code());
                status.latest_detail = failure.detail();
            }
            None => {}
        }
    }
    status.checked_at = now_millis();
    status
}

/// The CLI of `agent` as `shell_env` resolves it: its binary, how it was installed and its
/// version; `None` where its binary does not resolve. Blocking, since the version may be asked of
/// the CLI.
fn inspect(
    agent: AgentId,
    shell_env: &HashMap<String, String>,
) -> Option<(ResolvedBinary, Install, Option<String>)> {
    let binary = adapter::resolve_binary(agent, shell_env).ok()?;
    let provider = adapter::provider(agent);
    let install = Install::of_binary(provider.cli_packaging(), &binary);
    let version = provider
        .installed_version(&binary.program.to_string_lossy())
        .or_else(|| asked_version(agent, shell_env));
    let version =
        version.map(|(major, minor, patch): AgentVersion| format!("{major}.{minor}.{patch}"));
    Some((binary, install, version))
}

/// What the CLI says its version is, for an install that does not tell it.
fn asked_version(agent: AgentId, shell_env: &HashMap<String, String>) -> Option<AgentVersion> {
    let output = account_cli::query(
        agent,
        shell_env,
        None,
        &["--version"],
        VERSION_TIMEOUT,
        16 * 1024,
        None,
    )
    .ok()?;
    adapter::version_from_output(&String::from_utf8_lossy(&output.stdout))
        .or_else(|| adapter::version_from_output(&String::from_utf8_lossy(&output.stderr)))
}

/// Runs `command` on `shell_env` within `timeout`, its output read through pipes.
fn run_tool(
    command: &ToolCommand,
    shell_env: &HashMap<String, String>,
    timeout: Duration,
    stdout_limit: usize,
) -> std::result::Result<Output, BoundedFailure> {
    let bounds = Bounds {
        timeout,
        stdout_limit,
        stderr_limit: STDERR_LIMIT,
        reach: tool_reach(command),
        term_grace: TERM_GRACE,
    };
    run_bounded(
        &mut command.command(shell_env),
        &bounds,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

/// What stopping `command` reaches: a tool of an agent's own installer is held as an agent's CLI
/// is, to its whole session; any other, to its process group.
fn tool_reach(command: &ToolCommand) -> Reach {
    if command.is_agent_cli() {
        Reach::Session
    } else {
        Reach::Group
    }
}

/// The last non-empty line of `bytes`, trimmed.
fn last_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_owned)
}

/// The last line a failed tool printed, on stderr, else on stdout, else `fallback`.
fn failure_line(output: &Output, fallback: &str) -> String {
    bounded(
        &last_line(&output.stderr)
            .or_else(|| last_line(&output.stdout))
            .unwrap_or_else(|| fallback.to_owned()),
    )
}

/// `text` trimmed and cut to [`DETAIL_LIMIT`] characters.
fn bounded(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(DETAIL_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

/// The shell environment a check runs in, the cached one: a check is paced by a client, not by a
/// person's launch.
#[cfg(not(test))]
fn check_shell() -> Result<HashMap<String, String>> {
    crate::env_shell::cached_snapshot()
}

/// Where the latest version is read from.
#[cfg(not(test))]
fn registry() -> &'static dyn Registry {
    &latest::NpmRegistry
}

/// Under test, no shell, no registry and no update: the host's login shell and the network are
/// not the tests' to depend on, and the CLI that would run is the user's own. A test that checks
/// or updates takes [`check`] and [`start_update`] with an environment and a registry of its own.
#[cfg(test)]
fn check_shell() -> Result<HashMap<String, String>> {
    anyhow::bail!("no shell environment under test")
}

#[cfg(test)]
fn registry() -> &'static dyn Registry {
    &tests::NoRegistry
}
