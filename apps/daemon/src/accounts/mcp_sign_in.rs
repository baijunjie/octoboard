//! Signing an account in to and out of an MCP server through the agent's own commands, run in the
//! background under the account's home (`codex mcp login` and `codex mcp logout`): what a
//! provider's [`McpServerAuth`] says to run, and [`request`], which runs it, one run per account
//! and server at a time. A second request for the same action joins the run under way and gets its
//! outcome; the other action is refused as busy. The agent opens the browser and keeps the token
//! itself, so Octoboard only follows the CLI to its end. Nothing here rereads the inventory: the
//! client does, once it has the answer.
//!
//! The runs are runtime state, held in [`McpServerAuths`] and never stored. A run that is ended
//! while its CLI still runs (cancelled by the user or by the account's removal, given up on after
//! its timeout, or cut short by the daemon's exit) kills the CLI's whole session; a CLI that exits
//! by itself is not followed into whatever it forked, as with an account's own login
//! ([`Process`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

use super::background_cli::{Line, Process, SigningIn};
use super::choice::refuse_if_agent_unavailable;
use super::login::LOGIN_TIMEOUT;
use super::quotas::ShellSource;
use crate::adapter::{self, provider, Unstartable};
use crate::platform::shell::ResolvedBinary;
use crate::protocol::{
    error_code, AgentId, CodedError, McpServerAuthOutcome, McpServerAuthStatus, ProviderOperation,
};
use crate::state::AppState;

/// How long a sign-in may wait for the user in the browser before the daemon gives up: as long as
/// an account's own login.
const SIGN_IN_TIMEOUT: Duration = LOGIN_TIMEOUT;

/// How long a sign-out may take; it only removes a stored token.
const SIGN_OUT_TIMEOUT: Duration = Duration::from_secs(60);

/// The most characters of a CLI's message a failure carries, the `…` that marks a cut included.
const DETAIL_LIMIT: usize = 4096;

/// What is run for a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpAuthAction {
    SignIn,
    SignOut,
}

impl McpAuthAction {
    /// How long the CLI may run.
    fn timeout(self) -> Duration {
        match self {
            Self::SignIn => SIGN_IN_TIMEOUT,
            Self::SignOut => SIGN_OUT_TIMEOUT,
        }
    }
}

/// How one provider's CLI signs an account in to and out of an MCP server.
pub(crate) trait McpServerAuth: Send + Sync {
    /// The arguments of the CLI for `action` on the server named `server`, for the account whose
    /// directory is `config_dir` (`None` for the default account). The name is never read as an
    /// option.
    fn args(&self, action: McpAuthAction, server: &str, config_dir: Option<&Path>) -> Vec<String>;
}

/// Which run: the agent, the account's id (`None` for the default account) and the server's name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    agent: AgentId,
    account: Option<String>,
    server: String,
}

/// What a request for a run that is already going waits on: the outcome, once there is one.
type Outcomes = watch::Receiver<Option<McpServerAuthOutcome>>;

struct Running {
    action: McpAuthAction,
    /// Wakes the run to end itself.
    cancel: Arc<Notify>,
    outcome: Outcomes,
    task: JoinHandle<()>,
}

/// The runs going now. Held in memory only.
#[derive(Default)]
pub struct McpServerAuths {
    running: Mutex<HashMap<Key, Running>>,
}

/// Removes the run of its key when dropped, and only then hands the outcome to whoever waits, so
/// that a request made on hearing it starts a run of its own. A run that panics or is aborted
/// unwinds through this and is told as failed.
struct Ending {
    state: Arc<AppState>,
    key: Key,
    outcome: Option<McpServerAuthOutcome>,
    told: watch::Sender<Option<McpServerAuthOutcome>>,
}

impl Drop for Ending {
    fn drop(&mut self) {
        self.state.mcp_server_auths.lock().remove(&self.key);
        let outcome = self.outcome.take().unwrap_or_else(|| {
            McpServerAuthOutcome::failed("the run stopped unexpectedly".to_string())
        });
        self.told.send_replace(Some(outcome));
    }
}

impl McpServerAuths {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Key, Running>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The outcome receiver of the run of `key` when it is `action`; `Err` when the other action
    /// is running for it, as `mcp_server_auth_busy`.
    fn joined(
        running: &HashMap<Key, Running>,
        key: &Key,
        action: McpAuthAction,
    ) -> Result<Option<Outcomes>> {
        match running.get(key) {
            None => Ok(None),
            Some(held) if held.action == action => Ok(Some(held.outcome.clone())),
            Some(_) => Err(CodedError::raised(
                error_code::MCP_SERVER_AUTH_BUSY,
                format!(
                    "{} is being signed {} for this account; wait for it to end.",
                    key.server,
                    match action {
                        McpAuthAction::SignIn => "out",
                        McpAuthAction::SignOut => "in",
                    }
                ),
                &[("agent", key.agent.label()), ("server", &key.server)],
            )),
        }
    }

    /// Asks the sign-in of `server` for the account to end. Looks at nothing but the runs going,
    /// and at no sign-out, which is not offered for cancelling.
    pub(crate) fn cancel(
        &self,
        agent: AgentId,
        account: Option<String>,
        server: &str,
    ) -> Result<()> {
        let key = Key {
            agent,
            account,
            server: server.to_string(),
        };
        match self.lock().get(&key) {
            Some(held) if held.action == McpAuthAction::SignIn => {
                held.cancel.notify_one();
                Ok(())
            }
            _ => Err(CodedError::raised(
                error_code::MCP_SIGN_IN_NOT_RUNNING,
                format!("{server} is not being signed in to for this account."),
                &[("agent", agent.label()), ("server", server)],
            )),
        }
    }

    /// Asks every run of the account `account` to end, whichever the action.
    pub(crate) fn cancel_account(&self, agent: AgentId, account: &str) {
        for (key, held) in self.lock().iter() {
            if key.agent == agent && key.account.as_deref() == Some(account) {
                held.cancel.notify_one();
            }
        }
    }

    /// Ends the runs of an account and waits, for at most `within`, until they are gone and their
    /// CLIs with them: `true` where none runs for the account any more, including where none ran.
    pub(crate) async fn end_account(
        &self,
        agent: AgentId,
        account: &str,
        within: Duration,
    ) -> bool {
        self.cancel_account(agent, account);
        let gone = async {
            while self
                .lock()
                .keys()
                .any(|key| key.agent == agent && key.account.as_deref() == Some(account))
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(within, gone).await.is_ok()
    }

    /// Ends every run, killing every CLI, and returns once they are gone. For the daemon's own
    /// exit.
    pub async fn stop_all(&self) {
        let tasks: Vec<JoinHandle<()>> = self.lock().drain().map(|(_, held)| held.task).collect();
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
    }
}

/// What a run needs, taken before it is registered.
struct Job {
    agent: AgentId,
    action: McpAuthAction,
    auth: &'static dyn McpServerAuth,
    binary: ResolvedBinary,
    shell_env: HashMap<String, String>,
    /// The account's directory, exported to the CLI; `None` for the default account.
    config_dir: Option<PathBuf>,
    server: String,
}

/// Signs the `agent` account `account` (an account id, `None` for the default account) in to or
/// out of the MCP server `server` as `action` says, in the shell environment `shell` gives, and
/// returns how the CLI ended, after at most `action.timeout()`. Refused, before anything runs,
/// for an agent with no such commands (`provider_unsupported`), one that is not available, a
/// shell environment that cannot be taken, a binary that does not resolve, an account whose
/// removal is under way, and the other action running for the same server. Waits for the run of
/// the same action already going, if any, instead of starting another.
pub(crate) async fn request(
    state: &Arc<AppState>,
    agent: AgentId,
    account: Option<String>,
    server: String,
    action: McpAuthAction,
    shell: ShellSource,
) -> Result<McpServerAuthOutcome> {
    request_with(
        state,
        agent,
        account,
        server,
        action,
        shell,
        action.timeout(),
    )
    .await
}

/// [`request`] with the time the CLI may run given.
async fn request_with(
    state: &Arc<AppState>,
    agent: AgentId,
    account: Option<String>,
    server: String,
    action: McpAuthAction,
    shell: ShellSource,
    timeout: Duration,
) -> Result<McpServerAuthOutcome> {
    let auth = CodedError::require_offered(
        agent,
        ProviderOperation::McpSignIn,
        provider(agent)
            .accounts()
            .and_then(|ops| ops.mcp_server_auth()),
    )?;
    refuse_if_agent_unavailable(state, agent)?;
    let config_dir = state.store.account_dir(&account)?;
    if let (Some(id), None) = (&account, &config_dir) {
        return Err(CodedError::unknown_account(id));
    }
    let key = Key {
        agent,
        account: account.clone(),
        server: server.clone(),
    };
    let auths = &state.mcp_server_auths;
    // A run of the same action already going is joined before anything is taken or resolved.
    let joined = McpServerAuths::joined(&auths.lock(), &key, action)?;
    let mut outcome = match joined {
        Some(outcome) => outcome,
        None => {
            let shell_env = tokio::task::spawn_blocking(move || shell())
                .await?
                .map_err(crate::env_shell::capture_failed)?;
            let binary = adapter::resolve_account_binary(agent, &shell_env)
                .map_err(Unstartable::into_error)?;
            let job = Job {
                agent,
                action,
                auth,
                binary,
                shell_env,
                config_dir: config_dir.clone().map(PathBuf::from),
                server,
            };
            begin(state, key.clone(), job, timeout)?
        }
    };
    // A delete or repoint between the read and the run being registered missed the cancel it
    // makes: that cancel follows its write, so the write is seen here.
    if let Some(id) = &account {
        if state.store.account_dir(&account)? != config_dir {
            auths.cancel_account(agent, id);
        }
    }
    let ended = outcome.wait_for(Option::is_some).await;
    Ok(ended.ok().and_then(|told| told.clone()).unwrap_or_else(|| {
        McpServerAuthOutcome::failed("the run stopped unexpectedly".to_string())
    }))
}

/// Registers the run of `key` and starts it, or joins one that went into the registry since: its
/// outcome receiver either way. Judged under the registry's lock, which the run's end takes as
/// well, so an account whose removal is moving its directory to the Trash has either had its runs
/// ended or refuses this one.
fn begin(state: &Arc<AppState>, key: Key, job: Job, timeout: Duration) -> Result<Outcomes> {
    let mut running = state.mcp_server_auths.lock();
    if let Some(removing) = key
        .account
        .as_deref()
        .and_then(|account| state.account_being_removed(account))
    {
        return Err(super::trash::being_removed(&removing));
    }
    if let Some(outcome) = McpServerAuths::joined(&running, &key, job.action)? {
        return Ok(outcome);
    }
    let cancel = Arc::new(Notify::new());
    let (told, outcomes) = watch::channel(None);
    let action = job.action;
    let task = {
        let (state, key, cancel) = (state.clone(), key.clone(), cancel.clone());
        tokio::spawn(async move {
            let mut ending = Ending {
                state: state.clone(),
                key,
                outcome: None,
                told,
            };
            // Dropping the run when the time is up kills the CLI's whole session.
            let outcome = tokio::select! {
                outcome = execute(&state, job, &cancel) => outcome,
                () = tokio::time::sleep(timeout) => {
                    McpServerAuthOutcome::of(McpServerAuthStatus::TimedOut)
                }
            };
            ending.outcome = Some(outcome);
        })
    };
    running.insert(
        key,
        Running {
            action,
            cancel,
            outcome: outcomes.clone(),
            task,
        },
    );
    Ok(outcomes)
}

/// Runs the CLI until it ends or `cancel` is notified.
async fn execute(state: &Arc<AppState>, job: Job, cancel: &Notify) -> McpServerAuthOutcome {
    // The account's directory must not change sharing under the CLI while it runs. A run waiting
    // for a change being carried out there is still cancelled at once.
    let dir = job.config_dir.clone();
    let waiting = tokio::task::spawn_blocking(move || {
        crate::accounts::shared_config::running_long(dir.as_deref())
    });
    let running = tokio::select! {
        running = waiting => running.ok().flatten(),
        () = cancel.notified() => return McpServerAuthOutcome::of(McpServerAuthStatus::Cancelled),
    };
    let _signing_in = running.map(|run| SigningIn::new(state.clone(), run));
    // A removal may have moved the directory while the run waited, and the CLI would create it
    // again.
    if let Some(dir) = job
        .config_dir
        .as_deref()
        .filter(|dir| crate::accounts::shared_config::is_missing(dir))
    {
        return McpServerAuthOutcome::failed(format!(
            "the account's configuration directory {} does not exist",
            dir.display()
        ));
    }
    let args = job
        .auth
        .args(job.action, &job.server, job.config_dir.as_deref());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = adapter::account_command_of(
        &job.binary,
        job.agent,
        &job.shell_env,
        job.config_dir.as_deref(),
        &args,
    );
    let mut process = match Process::spawn(command, job.agent.label(), false) {
        Ok(process) => process,
        Err(err) => {
            return McpServerAuthOutcome::failed(format!("it could not be started: {err}"));
        }
    };
    let mut said = Said::default();
    loop {
        tokio::select! {
            Some(line) = process.lines.recv() => said.take(line),
            status = process.child.wait() => {
                return match status {
                    Ok(status) if status.success() => {
                        McpServerAuthOutcome::of(McpServerAuthStatus::Succeeded)
                    }
                    Ok(status) => {
                        process.drain(|line| said.take(line)).await;
                        McpServerAuthOutcome::failed(said.failure(status))
                    }
                    Err(err) => McpServerAuthOutcome::failed(err.to_string()),
                };
            }
            () = cancel.notified() => {
                process.stop().await;
                return McpServerAuthOutcome::of(McpServerAuthStatus::Cancelled);
            }
        }
    }
}

/// What the CLI said: its stderr, whole, and the last line of its stdout.
#[derive(Default)]
struct Said {
    stderr: String,
    last_stdout: String,
}

impl Said {
    fn take(&mut self, line: Line) {
        if line.stderr {
            // Lines past the cap are dropped: the start of a message says what failed.
            if self.stderr.len() < DETAIL_LIMIT {
                self.stderr.push('\n');
                self.stderr.push_str(&line.text);
            }
        } else if !line.text.is_empty() {
            self.last_stdout = line.text;
        }
    }

    /// Why a CLI that ended with `status` did not succeed: its stderr, else the last line of its
    /// stdout, else its exit status.
    fn failure(&self, status: std::process::ExitStatus) -> String {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            if stderr.chars().count() <= DETAIL_LIMIT {
                return stderr.to_string();
            }
            let (end, _) = stderr
                .char_indices()
                .nth(DETAIL_LIMIT - 1)
                .expect("a message longer than the limit");
            return format!("{}…", &stderr[..end]);
        }
        if !self.last_stdout.is_empty() {
            return self.last_stdout.clone();
        }
        match status.code() {
            Some(code) => format!("exited with status {code}"),
            None => format!("exited with {status}"),
        }
    }
}

#[cfg(test)]
mod tests;
