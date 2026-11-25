//! Signing an account in, with the agent's own CLI run in the background against the account's
//! configuration directory: no terminal and no window. Octoboard never reads or writes the
//! credential; the CLI does, and the daemon only starts it, relays what it says the user has to do
//! (the page to authorize on, and for a device-code flow the code), carries a yes/no question of
//! the agent's own to the user and the answer back, and learns how it ended. The daemon never
//! answers such a question, and the time one stays open is not counted against the login's
//! timeout: a question nobody answers keeps its login waiting until the user cancels it or the
//! flow ends by itself. How each CLI is driven is its provider's ([`flows::LoginFlow`]), measured
//! in `docs/agent-cli-reference.md` ("Signing in a config directory without a terminal"); this
//! module keeps the logins running and decides how each one ends.
//!
//! Logins are runtime state: held in [`AccountLogins`], one per account, never stored. Every change
//! is broadcast as `account_login_updated`, and the ones still running replay in a `snapshot`. A
//! login ends in one of four ways: signed in (the account's quota is then read at once), failed,
//! cancelled by the user, or given up on by the daemon after [`LOGIN_TIMEOUT`], since Claude Code
//! has no timeout of its own, or after the time the provider's flow declares
//! ([`flows::LoginFlow::timeout`]), neither counting the time a question waits on the user. A
//! login that is ended while its CLI still runs (cancelled, given up on, or cut short by the
//! daemon's exit) kills the CLI's whole session; a CLI that exits by itself is not followed into
//! whatever it forked.

pub(crate) mod flows;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{oneshot, Notify};
use tokio::task::JoinHandle;

use crate::accounts::quotas;
use crate::adapter::{provider, Unstartable};
use crate::protocol::{
    error_code, login_failure_code, AccountLogin, AgentId, Code, CodedError, Event, LoginAnswer,
    LoginFailure, LoginQuestion, LoginStatus, ProviderOperation,
};
use crate::state::AppState;
use flows::{Flow, Outcome};

/// How long a login may wait for the user to authorize before the daemon gives up, where the
/// provider's flow declares no time of its own ([`flows::LoginFlow::timeout`]). Codex's own
/// browser flow gives up at the same ten minutes (its login server reports it as a timeout); Claude
/// Code and Grok Build do not time out on their own.
pub(crate) const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Which login: the agent and the account's id, `None` for the default account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    agent: AgentId,
    account: Option<String>,
}

struct Running {
    login: AccountLogin,
    /// Wakes the flow to cancel itself.
    cancel: Arc<Notify>,
    /// The flow waiting on the question the login shows; `None` while no question is open.
    asker: Option<Asker>,
    /// How many questions the login has asked, the last one's serial.
    asked: u64,
    task: JoinHandle<()>,
}

/// The flow that asked the question a login waits on.
struct Asker {
    /// Which of the login's questions it asked, so that an asker that stops waiting takes back its
    /// own question and never a later one.
    serial: u64,
    /// Takes the user's answer to it.
    answer: oneshot::Sender<LoginAnswer>,
}

/// What became of a flow's wish to ask a question.
pub(crate) enum Asked {
    /// The question is open on the login, under this serial.
    Open(u64),
    /// The login already waits on a question: a login asks one at a time.
    AlreadyAsking,
    /// The login has ended.
    Ended,
}

/// The logins running now.
#[derive(Default)]
pub struct AccountLogins {
    running: Mutex<HashMap<Key, Running>>,
}

/// Ends the login of its key when dropped, with the outcome set by then. A login task that panics
/// unwinds through this, so the login is announced as failed instead of staying in the list for
/// the rest of the daemon's life; one aborted by `stop_all` finds its login already gone.
struct Ending {
    state: Arc<AppState>,
    key: Key,
    outcome: Option<Outcome>,
}

impl Ending {
    fn new(state: Arc<AppState>, key: Key) -> Self {
        Self {
            state,
            key,
            outcome: None,
        }
    }
}

impl Drop for Ending {
    fn drop(&mut self) {
        let outcome = self.outcome.take().unwrap_or_else(|| {
            let agent = self.key.agent.label();
            let detail = "the sign-in stopped unexpectedly";
            Outcome::Failed(LoginFailure::new(
                login_failure_code::FAILED,
                format!("{agent} could not sign in: {detail}"),
                &[("agent", agent), ("detail", detail)],
            ))
        });
        self.state
            .account_logins
            .finish(&self.state, &self.key, outcome);
    }
}

/// What a login needs to know about its account.
pub struct Start {
    pub agent: AgentId,
    /// The account's id; `None` for the default account.
    pub account: Option<String>,
    /// The account's directory, exported to the CLI under the agent's own variable; `None` for the
    /// default account, which pins nothing.
    pub config_dir: Option<String>,
    /// Where the account's credential ends up: the pinned directory, else the one the agent
    /// resolves on its own, if known.
    pub resolved_dir: Option<String>,
}

impl AccountLogins {
    /// The logins that have not ended, in a stable order.
    pub fn running(&self) -> Vec<AccountLogin> {
        let mut logins: Vec<AccountLogin> = self
            .lock()
            .values()
            .map(|running| running.login.clone())
            .collect();
        logins.sort_by_key(|login| {
            (
                AgentId::ALL.iter().position(|agent| *agent == login.agent),
                login.account.clone(),
            )
        });
        logins
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Key, Running>> {
        self.running.lock().expect("account logins lock poisoned")
    }

    /// Records `change` to a running login and broadcasts it, under the lock so that the order
    /// clients see is the order the changes happened in. Nothing once the login has ended.
    fn update(&self, state: &AppState, key: &Key, change: impl FnOnce(&mut AccountLogin)) {
        let mut running = self.lock();
        let Some(entry) = running.get_mut(key) else {
            return;
        };
        let before = entry.login.clone();
        change(&mut entry.login);
        if entry.login != before {
            state.broadcast(Event::AccountLoginUpdated {
                login: entry.login.clone(),
            });
        }
    }

    /// Opens `question` on the login of `key`, which then waits for the user: `answer` is where
    /// their answer goes. A login asks one question at a time: while one is open another is
    /// refused, and the open one stays as it is. Nothing once the login has ended.
    fn ask(
        &self,
        state: &AppState,
        key: &Key,
        question: LoginQuestion,
        answer: oneshot::Sender<LoginAnswer>,
    ) -> Asked {
        let mut running = self.lock();
        let Some(entry) = running.get_mut(key) else {
            return Asked::Ended;
        };
        if entry.asker.is_some() {
            return Asked::AlreadyAsking;
        }
        entry.asked += 1;
        entry.login.status = LoginStatus::Waiting;
        entry.login.question = Some(question);
        entry.asker = Some(Asker {
            serial: entry.asked,
            answer,
        });
        state.broadcast(Event::AccountLoginUpdated {
            login: entry.login.clone(),
        });
        Asked::Open(entry.asked)
    }

    /// Takes the question `serial` of the login of `key` back, unanswered: its asker has stopped
    /// waiting. Says whether it was the one open; a question answered already, or one asked since,
    /// is left alone. Nothing is broadcast here: an asker stops waiting as its login ends too, and
    /// the ending is then the only update ([`Self::announce`] is for the login that goes on).
    fn withdraw_question(&self, key: &Key, serial: u64) -> bool {
        let mut running = self.lock();
        let Some(entry) = running.get_mut(key) else {
            return false;
        };
        if entry.asker.as_ref().map(|asker| asker.serial) != Some(serial) {
            return false;
        }
        entry.asker = None;
        entry.login.question = None;
        true
    }

    /// Broadcasts the login of `key` as it stands. Nothing once the login has ended.
    fn announce(&self, state: &AppState, key: &Key) {
        if let Some(entry) = self.lock().get(key) {
            state.broadcast(Event::AccountLoginUpdated {
                login: entry.login.clone(),
            });
        }
    }

    /// Brings the user's `answer` to the question `question` the login of an account waits on,
    /// and the login goes on with it. This is the only way a question is answered. Refused when
    /// the account has no login running, or its login is not waiting on that question: another
    /// question is open, none is, or the flow that asked has stopped waiting for the answer.
    pub fn answer(
        &self,
        state: &AppState,
        agent: AgentId,
        account: Option<String>,
        question: &str,
        answer: LoginAnswer,
    ) -> Result<()> {
        let mut running = self.lock();
        let Some(entry) = running.get_mut(&Key { agent, account }) else {
            return Err(not_running(agent));
        };
        let open = entry
            .login
            .question
            .as_ref()
            .is_some_and(|open| open.id == question && open.accepts(&answer));
        // An asker that has stopped waiting is about to take its question back: an answer taken
        // now would be acknowledged and reach nobody.
        let waiting = entry
            .asker
            .as_ref()
            .is_some_and(|asker| !asker.answer.is_closed());
        let Some(asker) = (open && waiting).then(|| entry.asker.take()).flatten() else {
            return Err(CodedError::raised(
                error_code::LOGIN_QUESTION_NOT_OPEN,
                format!(
                    "The sign-in of this {} account is not waiting on that question.",
                    agent.label()
                ),
                &[("agent", agent.label())],
            ));
        };
        entry.login.question = None;
        state.broadcast(Event::AccountLoginUpdated {
            login: entry.login.clone(),
        });
        // Not received only by an asker that stops waiting between the check above and here; its
        // flow then goes on, or ends, without an answer.
        let _ = asker.answer.send(answer);
        Ok(())
    }

    /// Ends the login of `key` with `outcome`: the last update is broadcast and the account's
    /// quota is made due, since even a login that did not finish may have created the directory;
    /// for a success it is read again at once.
    fn finish(&self, state: &Arc<AppState>, key: &Key, outcome: Outcome) {
        let login = {
            let mut running = self.lock();
            let Some(entry) = running.remove(key) else {
                return;
            };
            let (status, failure) = match outcome {
                Outcome::Succeeded => (LoginStatus::Succeeded, None),
                Outcome::Failed(failure) => (LoginStatus::Failed, Some(failure)),
                Outcome::Cancelled => (LoginStatus::Cancelled, None),
                Outcome::TimedOut => (LoginStatus::TimedOut, None),
            };
            let login = AccountLogin {
                status,
                failure,
                question: None,
                ..entry.login
            };
            state.broadcast(Event::AccountLoginUpdated {
                login: login.clone(),
            });
            login
        };
        state
            .account_quotas
            .make_due(login.agent, login.account.as_deref());
        if login.status == LoginStatus::Succeeded {
            let (state, scope) = (
                state.clone(),
                quotas::Scope {
                    agent: Some(login.agent),
                    account: login.account.clone(),
                },
            );
            tokio::spawn(async move { quotas::refresh(&state, &scope).await });
        }
    }

    /// Starts the login of an account and returns once the CLI is running, or refuses it, as
    /// `account_being_removed` while the account's removal is moving its directory to the Trash.
    /// What follows is announced as `account_login_updated`.
    pub async fn start(state: &Arc<AppState>, start: Start) -> Result<()> {
        // Refused before the shell environment is taken: a provider with no flow needs none, and a
        // login shell that cannot be captured must not hide that it has none.
        offered_login(start.agent)?;
        // Waited for as a person's launch is: the first login may be the first thing to need it.
        let shell_env = tokio::task::spawn_blocking(login_shell)
            .await?
            .map_err(crate::env_shell::capture_failed)?;
        Self::start_with(
            state,
            start,
            shell_env,
            crate::paths::run_dir(),
            LOGIN_TIMEOUT,
        )
    }

    /// [`Self::start_with`] for the tests of other modules, which run a fake CLI.
    #[cfg(test)]
    pub(crate) fn start_for_test(
        state: &Arc<AppState>,
        start: Start,
        shell_env: HashMap<String, String>,
        scratch_root: PathBuf,
    ) -> Result<()> {
        Self::start_with(state, start, shell_env, scratch_root, LOGIN_TIMEOUT)
    }

    /// [`Self::start`] over a shell environment already taken, with the directory its scratch files
    /// go under and the time a flow that declares none may wait for the user.
    fn start_with(
        state: &Arc<AppState>,
        start: Start,
        shell_env: HashMap<String, String>,
        scratch_root: PathBuf,
        undeclared_timeout: Duration,
    ) -> Result<()> {
        let login = offered_login(start.agent)?;
        Self::start_flow(
            state,
            start,
            login,
            shell_env,
            scratch_root,
            undeclared_timeout,
        )
    }

    /// [`Self::start_with`] over the provider's flow, already found.
    fn start_flow(
        state: &Arc<AppState>,
        start: Start,
        login: &'static dyn flows::LoginFlow,
        shell_env: HashMap<String, String>,
        scratch_root: PathBuf,
        undeclared_timeout: Duration,
    ) -> Result<()> {
        let key = Key {
            agent: start.agent,
            account: start.account.clone(),
        };
        let timeout = login.timeout().unwrap_or(undeclared_timeout);
        let logins = &state.account_logins;
        let mut running = logins.lock();
        // Judged under the logins' lock, which `end` takes as well: an account whose removal is
        // moving its directory to the Trash has either ended a login started before it was marked
        // or refuses this one, so none starts writing into the directory.
        if let Some(removing) = start
            .account
            .as_deref()
            .and_then(|account| state.account_being_removed(account))
        {
            return Err(crate::accounts::trash::being_removed(&removing));
        }
        if running.contains_key(&key) {
            return Err(CodedError::raised(
                error_code::LOGIN_ALREADY_RUNNING,
                format!(
                    "{} is already being signed in for this account.",
                    start.agent.label()
                ),
                &[("agent", start.agent.label())],
            ));
        }
        // A sign-in that listens on a fixed local port is one two logins cannot share.
        let busy = login
            .busy_code()
            .filter(|_| running.keys().any(|other| other.agent == start.agent));
        if let Some(busy) = busy {
            let label = start.agent.label();
            return Err(CodedError::raised(
                Code::of_provider(busy),
                format!(
                    "Another {label} account is being signed in; {label} signs in one account at a time."
                ),
                &[],
            ));
        }
        let flow = Flow::new(
            start.agent,
            login,
            shell_env,
            start.config_dir.map(PathBuf::from),
            start.resolved_dir.map(PathBuf::from),
            scratch_root.join(format!("login-{}", uuid::Uuid::new_v4())),
        )
        .map_err(Unstartable::into_error)?;

        let login = AccountLogin {
            agent: start.agent,
            account: start.account,
            status: LoginStatus::Starting,
            authorization_url: None,
            device_code: None,
            opened_by_agent: false,
            question: None,
            failure: None,
        };
        let cancel = Arc::new(Notify::new());
        let task = {
            let (state, key, cancel) = (state.clone(), key.clone(), cancel.clone());
            tokio::spawn(async move {
                let progress = flows::Progress::new(state.clone(), key.clone());
                let mut ending = Ending::new(state.clone(), key.clone());
                let clock = login_clock(timeout, progress.question_open());
                // Dropping the flow when the time is up kills the CLI's whole session.
                ending.outcome = Some(tokio::select! {
                    outcome = flow.run(&progress, &cancel) => outcome,
                    () = clock => Outcome::TimedOut,
                });
            })
        };
        state.broadcast(Event::AccountLoginUpdated {
            login: login.clone(),
        });
        running.insert(
            key,
            Running {
                login,
                cancel,
                asker: None,
                asked: 0,
                task,
            },
        );
        Ok(())
    }

    /// Asks the login of an account to end. It is announced as `cancelled` once the CLI has gone.
    /// Looks at nothing but the logins running, so that a login whose account has just gone can
    /// still be cancelled by its key.
    pub fn cancel(&self, agent: AgentId, account: Option<String>) -> Result<()> {
        let running = self.lock();
        let Some(entry) = running.get(&Key { agent, account }) else {
            return Err(not_running(agent));
        };
        entry.cancel.notify_one();
        Ok(())
    }

    /// Ends the login of an account and waits, for at most `within`, until it is gone and the CLI
    /// with it: `true` where none runs for the account any more, including where none ran.
    pub async fn end(&self, agent: AgentId, account: &str, within: Duration) -> bool {
        if self.cancel(agent, Some(account.to_string())).is_err() {
            return true;
        }
        let key = Key {
            agent,
            account: Some(account.to_string()),
        };
        let gone = async {
            while self.lock().contains_key(&key) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(within, gone).await.is_ok()
    }

    /// Ends every login, killing every CLI, and returns once they are gone. For the daemon's own
    /// exit: nothing is announced.
    pub async fn stop_all(&self) {
        let tasks: Vec<JoinHandle<()>> = self
            .lock()
            .drain()
            .map(|(_, running)| running.task)
            .collect();
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
    }
}

/// Runs out once a login has waited `timeout` for the user with no question open: the time a
/// question of the flow's stays open (`question_open`) is not counted, since
/// [`flows::Progress::ask`] waits for the user's answer however long it takes, and the clock goes
/// on with what was left once the question is answered or taken back.
async fn login_clock(
    timeout: Duration,
    mut question_open: tokio::sync::watch::Receiver<Option<u64>>,
) {
    let mut left = timeout;
    loop {
        // The flow's progress has gone, and nothing can open a question any more: the rest runs.
        if question_open.wait_for(Option::is_none).await.is_err() {
            return tokio::time::sleep(left).await;
        }
        let counting = tokio::time::Instant::now();
        tokio::select! {
            () = tokio::time::sleep(left) => return,
            // Only whether it failed is kept: the value it borrows is not held across a wait.
            gone = async { question_open.wait_for(Option::is_some).await.is_err() } => {
                left = left.saturating_sub(counting.elapsed());
                if gone {
                    return tokio::time::sleep(left).await;
                }
            }
        }
    }
}

fn not_running(agent: AgentId) -> anyhow::Error {
    CodedError::raised(
        error_code::LOGIN_NOT_RUNNING,
        format!("{} is not being signed in for this account.", agent.label()),
        &[("agent", agent.label())],
    )
}

/// The sign-in flow `agent`'s provider offers, or its refusal as `provider_unsupported`.
fn offered_login(agent: AgentId) -> Result<&'static dyn flows::LoginFlow> {
    CodedError::require_offered(
        agent,
        ProviderOperation::SignIn,
        provider(agent).accounts().and_then(|ops| ops.login()),
    )
}

/// The shell environment a login started by [`AccountLogins::start`] runs in.
#[cfg(not(test))]
fn login_shell() -> Result<HashMap<String, String>> {
    crate::env_shell::launch_snapshot()
}

/// Under test, none: the host's login shell is not the tests' to depend on, and a refusal that
/// comes first is told whatever the shell is. Tests that run a login take
/// [`AccountLogins::start_for_test`].
#[cfg(test)]
fn login_shell() -> Result<HashMap<String, String>> {
    anyhow::bail!("no shell environment under test")
}

#[cfg(test)]
mod tests;
