//! What drives an agent's CLI to sign an account in, whichever the agent: the [`Flow`] to run, the
//! [`Process`] it runs in (from `accounts::background_cli`) and what it reports, and asks the
//! user, through [`Progress`]. How each CLI is driven is the provider's own ([`LoginFlow`], in
//! `crate::adapter::<provider>::login`), as measured in `docs/agent-cli-reference.md` ("Signing in
//! a config directory without a terminal"). All run with no terminal of the user's and nothing
//! written to their input beyond what the provider writes: on pipes ([`Process`]), or, for a CLI
//! that asks its question only where it has a terminal, on a pseudo-terminal of the flow's own
//! (`accounts::background_cli::TerminalProcess`).
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
use tokio::sync::{oneshot, watch, Notify};

use super::Key;
use crate::accounts::background_cli::{Process, SigningIn};
use crate::adapter::{self, Unstartable};
use crate::platform::shell::ResolvedBinary;
use crate::protocol::{
    login_failure_code as code, AgentId, LoginAnswer, LoginFailure, LoginQuestion, LoginStatus,
};
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
    /// The serial of the flow's question that is open, waiting on the user: the login's clock
    /// stands still meanwhile ([`Self::question_open`]).
    asking: watch::Sender<Option<u64>>,
}

impl Progress {
    pub(super) fn new(state: Arc<AppState>, key: Key) -> Self {
        Self {
            state,
            key,
            asking: watch::Sender::new(None),
        }
    }

    /// Follows which question [`Self::ask`] put is open, by its serial, for the login's timeout,
    /// which does not count the time the user takes to answer.
    pub(super) fn question_open(&self) -> watch::Receiver<Option<u64>> {
        self.asking.subscribe()
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

    /// Puts `question` of the agent's own to the user and waits for their
    /// answer, however long that takes: nothing answers it but the user, so a flow that needs an
    /// answer does nothing until this returns, and the login's own timeout stands still while the
    /// question is open. While it waits the login shows the question in place of the page and the
    /// code. A login asks one question at a time: a second one while the first is open is refused
    /// as [`AlreadyAsking`], the first staying open. Dropping the future (a flow that is
    /// cancelled, or whose CLI has gone) takes its own question back.
    pub(crate) async fn ask(&self, question: LoginQuestion) -> Result<LoginAnswer, AlreadyAsking> {
        let (answer, answered) = oneshot::channel();
        let logins = &self.state.account_logins;
        let serial = match logins.ask(&self.state, &self.key, question, answer) {
            super::Asked::Open(serial) => serial,
            super::Asked::AlreadyAsking => return Err(AlreadyAsking),
            // The flow is about to be dropped with its login: nothing is ever answered.
            super::Asked::Ended => return std::future::pending().await,
        };
        // Declared before the receiver is moved into the wait below, so that the receiver goes
        // first when the future is dropped: an answer arriving then finds nobody waiting and is
        // refused instead of being taken for a question about to be withdrawn.
        let _open = OpenQuestion {
            progress: self,
            serial,
        };
        self.asking.send_replace(Some(serial));
        match answered.await {
            Ok(answer) => Ok(answer),
            // The login has ended under the flow.
            Err(_) => std::future::pending().await,
        }
    }
}

/// A flow asked a question while its login was still waiting on one.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AlreadyAsking;

/// A question that is open on a login, taken back when its asker stops waiting.
struct OpenQuestion<'a> {
    progress: &'a Progress,
    serial: u64,
}

impl Drop for OpenQuestion<'_> {
    fn drop(&mut self) {
        let Progress { state, key, asking } = self.progress;
        // Answered or taken back, the question no longer holds the login's clock, unless another
        // question has opened since: a flow polling a second `ask` while the first's future has
        // not completed yet.
        asking.send_if_modified(|open| {
            let own = *open == Some(self.serial);
            if own {
                *open = None;
            }
            own
        });
        if !state.account_logins.withdraw_question(key, self.serial) {
            return;
        }
        // Said from a task of its own, a turn later, so that a login ending in the same turn (given
        // up on, or cancelled with nothing left to wait for) announces its ending alone, as a rule,
        // not a question taken back just before it. Outside a runtime the daemon is going down and
        // nobody listens.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (state, key) = (state.clone(), key.clone());
        runtime.spawn(async move {
            tokio::task::yield_now().await;
            state.account_logins.announce(&state, &key);
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

    /// How long the daemon waits for this sign-in before giving up on it, for a flow whose own
    /// wait is not the daemon's (a device code that lives longer, so that the user is shown the
    /// agent's own expiry and not the daemon's timeout); `None`, the default, declares none, and
    /// the flow is given the daemon's own wait.
    fn timeout(&self) -> Option<Duration> {
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
/// leaves nothing behind either, as far as the removal can go: on Windows a CLI process still
/// running down when the flow is dropped can hold a file in it, and the directory then stays.
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

    /// The command that runs the agent's CLI as the account, or how the login ends when the
    /// account's directory is one the agent cannot be given.
    pub(crate) fn command(&self, args: &[&str]) -> Result<std::process::Command, Outcome> {
        adapter::account_command_of(
            &self.binary,
            self.agent,
            &self.shell_env,
            self.config_dir.as_deref(),
            args,
        )
        .map_err(|unstartable| self.failed(&unstartable.into_error().to_string()))
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

/// Whether `text` is a device code as the agents print one: two groups of four letters or digits
/// joined by a hyphen, and nothing else.
pub(crate) fn looks_like_device_code(text: &str) -> bool {
    let mut groups = text.split('-');
    let is_group = |group: Option<&str>| {
        group.is_some_and(|group| {
            group.len() == 4 && group.chars().all(|c| c.is_ascii_alphanumeric())
        })
    };
    is_group(groups.next()) && is_group(groups.next()) && groups.next().is_none()
}

/// The last non-empty line of the file at `path`; empty while it is missing.
pub(crate) fn last_line_of(path: &Path) -> String {
    const URL_LOG_LIMIT: usize = 64 * 1024;
    crate::hostfs::read_regular_text(path, URL_LOG_LIMIT)
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

    /// A device code is two groups of four letters or digits joined by a hyphen, and nothing else.
    #[test]
    fn a_device_code_is_two_groups_of_four_letters_or_digits() {
        for code in ["ABCD-1234", "a1b2-C3D4", "0000-ffff", "XXXX-XXXX"] {
            assert!(looks_like_device_code(code), "{code}");
        }
        for not_a_code in [
            "",
            "Waiting for authorization...",
            "ABCD-12345",
            "ABC-1234",
            "ABCD1234",
            "ABCD-12-34",
            "abcd-12-34",
            "ABCD-1234-",
            "-ABCD-1234",
            "ABCD-",
            "ABCD- 1234",
            "ABCD-123!",
            " ABCD-1234",
            // Four bytes, two letters, and not ASCII.
            "\u{c0}\u{c0}-1234",
        ] {
            assert!(!looks_like_device_code(not_a_code), "{not_a_code:?}");
        }
    }
}
