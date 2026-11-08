//! What drives an agent's CLI to sign an account in, whichever the agent: the [`Flow`] to run, the
//! [`Process`] it runs in (from `accounts::background_cli`) and what it reports through
//! [`Progress`]. How each CLI is driven is the provider's own ([`LoginFlow`], in
//! `crate::adapter::<provider>::login`), as measured in `docs/agent-cli-reference.md` ("Signing in
//! a config directory without a terminal"). All run with no terminal and nothing written to their
//! stdin beyond what the provider writes.
//!
//! Dropping a flow's future kills the CLI's whole session (see [`Process`]), which is how a timeout
//! and the daemon's exit end one, and removes the flow's scratch directory.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Notify;

use super::Key;
use crate::accounts::background_cli::{Process, SigningIn};
use crate::adapter::{self, Unstartable};
use crate::platform::shell::ResolvedBinary;
use crate::protocol::{login_failure_code as code, AgentId, LoginFailure, LoginStatus};
use crate::providers::ProviderCode;
use crate::state::AppState;

/// How a login ended.
pub(crate) enum Outcome {
    Succeeded,
    Failed(LoginFailure),
    Cancelled,
    TimedOut,
}

/// How long a CLI that could not be written to is given to exit, so that its exit, and what it
/// said, is what the login reports.
const WRITE_FAILURE_GRACE: Duration = Duration::from_secs(1);

/// Reports a login's progress to the clients.
pub(crate) struct Progress {
    state: Arc<AppState>,
    key: Key,
}

impl Progress {
    pub(super) fn new(state: Arc<AppState>, key: Key) -> Self {
        Self { state, key }
    }

    /// The user has something to do: authorize on `url`, confirming `device_code` there if there
    /// is one. `opened_by_agent` is the CLI having opened the page itself.
    pub(crate) fn waiting(
        &self,
        url: Option<String>,
        device_code: Option<String>,
        opened_by_agent: bool,
    ) {
        self.state
            .account_logins
            .update(&self.state, &self.key, |login| {
                login.status = LoginStatus::Waiting;
                login.authorization_url = url.or(login.authorization_url.take());
                login.device_code = device_code.or(login.device_code.take());
                login.opened_by_agent = opened_by_agent;
            });
    }
}

/// How one provider's CLI is driven to sign an account in.
pub(crate) trait LoginFlow: Send + Sync {
    /// Runs the CLI's sign-in for the account `flow` describes (binary, shell environment,
    /// directories, scratch), reporting what the user has to do through `progress`, until it ends
    /// or `cancel` is notified. A boxed future because the crate has no `async-trait`.
    fn run<'a>(
        &'a self,
        flow: &'a Flow,
        progress: &'a Progress,
        cancel: &'a Notify,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;

    /// The code a sign-in is refused with while another account of this provider is being signed
    /// in, for a provider whose sign-in only one login at a time can use (one that listens on a
    /// fixed local port); `None`, the default, lets its logins run side by side.
    fn busy_code(&self) -> Option<ProviderCode> {
        None
    }
}

/// One agent's login, ready to run.
pub(crate) struct Flow {
    pub(crate) agent: AgentId,
    /// How the agent's sign-in is driven.
    login: &'static dyn LoginFlow,
    /// Where the agent's binary resolved, once, so that every command of the flow runs the same.
    binary: ResolvedBinary,
    pub(crate) shell_env: HashMap<String, String>,
    /// The account's directory, exported to the CLI; `None` for the default account.
    pub(crate) config_dir: Option<PathBuf>,
    /// Where the credential ends up, if known: the pinned directory or the agent's own default.
    pub(crate) resolved_dir: Option<PathBuf>,
    /// This login's own scratch directory, removed when the flow is dropped.
    scratch: Scratch,
}

/// A scratch directory removed on drop, so that a flow dropped by a timeout or the daemon's exit
/// leaves nothing behind either.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Flow {
    /// `Err` is why the agent's binary cannot start its CLI in `shell_env`.
    pub(super) fn new(
        agent: AgentId,
        login: &'static dyn LoginFlow,
        shell_env: HashMap<String, String>,
        config_dir: Option<PathBuf>,
        resolved_dir: Option<PathBuf>,
        scratch: PathBuf,
    ) -> Result<Self, Unstartable> {
        let binary = adapter::resolve_account_binary(agent, &shell_env)?;
        Ok(Self {
            agent,
            login,
            binary,
            shell_env,
            config_dir,
            resolved_dir,
            scratch: Scratch(scratch),
        })
    }

    pub(super) async fn run(self, progress: &Progress, cancel: &Notify) -> Outcome {
        // The account's directory must not change sharing under the CLI while it signs in. A login
        // waiting for a change being carried out there is still cancelled at once.
        let dir = self.config_dir.clone();
        let waiting = tokio::task::spawn_blocking(move || {
            crate::accounts::shared_config::running_long(dir.as_deref())
        });
        let running = tokio::select! {
            running = waiting => running.ok().flatten(),
            () = cancel.notified() => return Outcome::Cancelled,
        };
        let _signing_in = running.map(|run| SigningIn::new(progress.state.clone(), run));
        self.login.run(&self, progress, cancel).await
    }

    /// A flow of `agent` for a test, its binary not resolved.
    #[cfg(test)]
    pub(crate) fn unresolved(agent: AgentId, scratch: PathBuf) -> Self {
        Self {
            agent,
            login: adapter::provider(agent)
                .accounts()
                .and_then(|ops| ops.login())
                .expect("the agent has a sign-in flow"),
            binary: ResolvedBinary::default(),
            shell_env: HashMap::new(),
            config_dir: None,
            resolved_dir: None,
            scratch: Scratch(scratch),
        }
    }

    /// This login's own scratch directory, created by whatever needs it.
    pub(crate) fn scratch_dir(&self) -> &Path {
        &self.scratch.0
    }

    pub(crate) fn command(&self, args: &[&str]) -> std::process::Command {
        adapter::account_command_of(
            &self.binary,
            self.agent,
            &self.shell_env,
            self.config_dir.as_deref(),
            args,
        )
    }

    pub(crate) fn failed(&self, detail: &str) -> Outcome {
        Outcome::Failed(LoginFailure::new(
            code::FAILED,
            format!("{} could not sign in: {detail}", self.agent.label()),
            &[("agent", self.agent.label()), ("detail", detail)],
        ))
    }

    pub(crate) fn not_confirmed(&self) -> Outcome {
        Outcome::Failed(LoginFailure::new(
            code::NOT_CONFIRMED,
            format!(
                "{} finished, but is still not signed in for this account",
                self.agent.label()
            ),
            &[("agent", self.agent.label())],
        ))
    }

    /// How a CLI that ended without signing in is reported: its last words, else its exit status.
    pub(crate) fn ended(&self, status: std::process::ExitStatus, last_words: &str) -> Outcome {
        if last_words.is_empty() {
            self.failed(&format!("it ended with {status}"))
        } else {
            self.failed(last_words)
        }
    }

    /// Writes `request` to the CLI, or says how the login ends when that fails. A CLI that exits
    /// at once, on a bad configuration say, can close its stdin before it is written to, and what
    /// it said is why the login failed, not the broken pipe: a failed write is reported as the
    /// CLI's exit, with `last_words` (what it has said so far) brought up to date. Only a CLI still
    /// running after [`WRITE_FAILURE_GRACE`] is reported as not taking the write.
    pub(crate) async fn send(
        &self,
        process: &mut Process,
        request: &Value,
        last_words: &mut String,
    ) -> Result<(), Outcome> {
        let Err(err) = process.send(request).await else {
            return Ok(());
        };
        match tokio::time::timeout(WRITE_FAILURE_GRACE, process.child.wait()).await {
            Ok(Ok(status)) => {
                process.last_words(last_words).await;
                Err(self.ended(status, last_words))
            }
            _ => Err(self.failed(&format!("it could not be written to: {err}"))),
        }
    }
}

/// The last non-empty line of the file at `path`; empty while it is missing.
pub(crate) fn last_line_of(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            text.lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ScratchDir, PATIENCE};
    use serde_json::json;

    #[tokio::test]
    async fn a_write_to_a_cli_that_has_exited_reports_what_it_said_last() {
        let scratch = ScratchDir::new("login-write-after-exit");
        let flow = Flow::unresolved(AgentId::CODEX, scratch.to_path_buf());
        let mut command = std::process::Command::new(crate::test_support::bash());
        command.args(["-c", "echo 'Error: bad configuration' >&2; exit 1"]);
        let mut process = Process::spawn(command, "stand-in", true).expect("the stand-in starts");
        // Exited, found out without reaping it, so the write below meets a closed pipe.
        let child = process.handle();
        let deadline = tokio::time::Instant::now() + PATIENCE;
        while child.exited_unreaped() != Some(true) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the stand-in never exited"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // The pipe can still have a reader for a moment: a process forked by another thread
        // while this one was spawning the stand-in inherited both ends, and holds them until it
        // execs. A write that lands meanwhile is retried, the stand-in still unreaped, until one
        // meets the closed pipe.
        let mut last_words = String::new();
        let outcome = loop {
            match flow
                .send(&mut process, &json!({"id": 1}), &mut last_words)
                .await
            {
                Err(outcome) => break outcome,
                Ok(()) => {
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "something kept reading the stand-in's stdin"
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        };
        let Outcome::Failed(failure) = outcome else {
            panic!("the login did not fail");
        };
        assert_eq!(
            failure.params.get("detail").map(String::as_str),
            Some("Error: bad configuration")
        );
    }
}
