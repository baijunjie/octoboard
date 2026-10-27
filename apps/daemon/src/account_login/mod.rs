//! Signing an account in, with the agent's own CLI run in the background against the account's
//! configuration directory: no terminal and no window. Octoboard never reads or writes the
//! credential; the CLI does, and the daemon only starts it, relays what it says the user has to do
//! (the page to authorize on, and for a device-code flow the code) and learns how it ended. How
//! each CLI is driven is `flows.rs`'s, measured in `docs/agent-cli-reference.md` ("Signing in a
//! config directory without a terminal"); this module keeps the logins running and decides how
//! each one ends.
//!
//! Logins are runtime state: held in [`AccountLogins`], one per account, never stored. Every change
//! is broadcast as `account_login_updated`, and the ones still running replay in a `snapshot`. A
//! login ends in one of four ways: signed in (the account's quota is then read at once), failed,
//! cancelled by the user, or given up on by the daemon after [`LOGIN_TIMEOUT`], since Claude Code
//! has no timeout of its own. A login that is ended while its CLI still runs (cancelled, given up
//! on, or cut short by the daemon's exit) kills the CLI's whole process group; a CLI that exits by
//! itself is not followed into whatever it forked.

mod flows;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::account_quotas;
use crate::protocol::{
    error_code, login_failure_code, AccountLogin, Agent, CodedError, Event, LoginFailure,
    LoginStatus,
};
use crate::state::AppState;
use flows::{Flow, Outcome};

/// How long a login may wait for the user to authorize before the daemon gives up. Codex's own
/// browser flow gives up at the same ten minutes (its login server reports it as a timeout); Claude
/// Code and Grok Build do not time out on their own.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Which login: the agent and the account's id, `None` for the default account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    agent: Agent,
    account: Option<String>,
}

struct Running {
    login: AccountLogin,
    /// Wakes the flow to cancel itself.
    cancel: Arc<Notify>,
    task: JoinHandle<()>,
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
    pub agent: Agent,
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
                Agent::ALL.iter().position(|agent| *agent == login.agent),
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
                account_quotas::Scope {
                    agent: Some(login.agent),
                    account: login.account.clone(),
                    ..Default::default()
                },
            );
            tokio::spawn(async move { account_quotas::refresh(&state, &scope).await });
        }
    }

    /// Starts the login of an account and returns once the CLI is running, or refuses it. What
    /// follows is announced as `account_login_updated`.
    pub async fn start(state: &Arc<AppState>, start: Start) -> Result<()> {
        // Waited for as a person's launch is: the first login may be the first thing to need it.
        let shell_env = tokio::task::spawn_blocking(crate::env_shell::launch_snapshot)
            .await?
            .map_err(|err| {
                let detail = format!("{err:#}");
                CodedError::raised(
                    error_code::SHELL_ENVIRONMENT_TIMEOUT,
                    format!("The shell environment could not be captured: {detail}"),
                    &[("detail", &detail)],
                )
            })?;
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
    /// go under and the time it may wait for the user.
    fn start_with(
        state: &Arc<AppState>,
        start: Start,
        shell_env: HashMap<String, String>,
        scratch_root: PathBuf,
        timeout: Duration,
    ) -> Result<()> {
        let key = Key {
            agent: start.agent,
            account: start.account.clone(),
        };
        let logins = &state.account_logins;
        let mut running = logins.lock();
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
        // Codex's browser flow listens on a fixed local port, which two logins cannot share.
        if start.agent == Agent::Codex && running.keys().any(|other| other.agent == Agent::Codex) {
            return Err(CodedError::raised(
                error_code::LOGIN_CODEX_BUSY,
                "Another Codex account is being signed in; Codex signs in one account at a time.",
                &[],
            ));
        }
        let flow = Flow::new(
            start.agent,
            shell_env,
            start.config_dir.map(PathBuf::from),
            start.resolved_dir.map(PathBuf::from),
            scratch_root.join(format!("login-{}", uuid::Uuid::new_v4())),
        )
        .map_err(|binary| {
            CodedError::raised(
                error_code::BINARY_NOT_FOUND,
                format!("`{binary}` was not found on PATH in the shell environment."),
                &[("binary", binary)],
            )
        })?;

        let login = AccountLogin {
            agent: start.agent,
            account: start.account,
            status: LoginStatus::Starting,
            authorization_url: None,
            device_code: None,
            opened_by_agent: false,
            failure: None,
        };
        let cancel = Arc::new(Notify::new());
        let task = {
            let (state, key, cancel) = (state.clone(), key.clone(), cancel.clone());
            tokio::spawn(async move {
                let progress = flows::Progress::new(state.clone(), key.clone());
                let mut ending = Ending::new(state.clone(), key.clone());
                // Dropping the flow when the time is up kills the CLI's process group.
                ending.outcome = Some(tokio::select! {
                    outcome = flow.run(&progress, &cancel) => outcome,
                    () = tokio::time::sleep(timeout) => Outcome::TimedOut,
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
                task,
            },
        );
        Ok(())
    }

    /// Asks the login of an account to end. It is announced as `cancelled` once the CLI has gone.
    /// Looks at nothing but the logins running, so that a login whose account has just gone can
    /// still be cancelled by its key.
    pub fn cancel(&self, agent: Agent, account: Option<String>) -> Result<()> {
        let running = self.lock();
        let Some(entry) = running.get(&Key { agent, account }) else {
            return Err(CodedError::raised(
                error_code::LOGIN_NOT_RUNNING,
                format!("{} is not being signed in for this account.", agent.label()),
                &[("agent", agent.label())],
            ));
        };
        entry.cancel.notify_one();
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, read_pid_file, ScratchDir, PATIENCE};

    struct Setup {
        state: Arc<AppState>,
        dir: ScratchDir,
        env: HashMap<String, String>,
    }

    /// A daemon state and a fake CLI of `agent` running `body`, all under one scratch directory.
    fn setup(name: &str, agent: Agent, body: &str) -> Setup {
        let (state, dir) = crate::test_support::app_state(name);
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let env = fake_agent(&bin, crate::adapter::adapter_for(agent).binary(), body);
        Setup { state, dir, env }
    }

    impl Setup {
        fn start(&self, agent: Agent, account: Option<&str>, timeout: Duration) -> Result<()> {
            let config_dir = self.dir.join(format!("{}-home", agent.wire_name()));
            AccountLogins::start_with(
                &self.state,
                Start {
                    agent,
                    account: account.map(str::to_string),
                    config_dir: Some(config_dir.to_string_lossy().into_owned()),
                    resolved_dir: Some(config_dir.to_string_lossy().into_owned()),
                },
                self.env.clone(),
                self.dir.join("run"),
                timeout,
            )
        }
    }

    /// Every update broadcast until the login ends, the ending included.
    async fn until_ended(
        events: &mut tokio::sync::broadcast::Receiver<Event>,
    ) -> Vec<AccountLogin> {
        let mut updates = Vec::new();
        let ended = tokio::time::timeout(PATIENCE, async {
            loop {
                let Ok(Event::AccountLoginUpdated { login }) = events.recv().await else {
                    continue;
                };
                let done = !matches!(login.status, LoginStatus::Starting | LoginStatus::Waiting);
                updates.push(login);
                if done {
                    break;
                }
            }
        })
        .await;
        assert!(ended.is_ok(), "the login never ended: {updates:?}");
        updates
    }

    fn failure_code(login: &AccountLogin) -> &str {
        &login.failure.as_ref().expect("a failure").code
    }

    const CLAUDE_SIGNS_IN: &str = r#"
        case "$1 $2" in
          "auth status") echo '{"loggedIn":true}'; exit 0;;
        esac
        echo "pid $$" > "$HOME/claude.pid"
        echo "$CLAUDE_CONFIG_DIR" > "$HOME/claude.dir"
        echo 'Opening browser to sign in…'
        "$BROWSER" 'https://claude.test/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback'
        sleep 0.4
        echo 'Login successful.'
        exit 0
    "#;

    #[tokio::test]
    async fn claude_hands_the_browser_url_to_the_daemon_and_signs_in() {
        let setup = setup("login-claude", Agent::Claude, CLAUDE_SIGNS_IN);
        let mut events = setup.state.subscribe();
        setup
            .start(Agent::Claude, Some("a"), LOGIN_TIMEOUT)
            .unwrap();

        let updates = until_ended(&mut events).await;
        let statuses: Vec<_> = updates.iter().map(|login| login.status).collect();
        assert_eq!(
            statuses,
            [
                LoginStatus::Starting,
                LoginStatus::Waiting,
                LoginStatus::Succeeded
            ]
        );
        assert_eq!(
            updates[1].authorization_url.as_deref(),
            Some(
                "https://claude.test/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback"
            )
        );
        assert!(!updates[1].opened_by_agent);
        assert_eq!(updates[2].account.as_deref(), Some("a"));
        // The account's directory reached the CLI under its own variable.
        let dir = std::fs::read_to_string(setup.dir.join("bin/claude.dir")).unwrap();
        assert_eq!(dir.trim(), setup.dir.join("claude-home").to_string_lossy());
        assert!(setup.state.account_logins.running().is_empty());
    }

    #[tokio::test]
    async fn claude_exiting_zero_without_a_sign_in_is_not_a_success() {
        let body = CLAUDE_SIGNS_IN.replace(
            r#"echo '{"loggedIn":true}'"#,
            r#"echo '{"loggedIn":false}'"#,
        );
        let setup = setup("login-claude-unconfirmed", Agent::Claude, &body);
        let mut events = setup.state.subscribe();
        setup.start(Agent::Claude, None, LOGIN_TIMEOUT).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert_eq!(
            failure_code(&ended),
            crate::protocol::login_failure_code::NOT_CONFIRMED
        );
    }

    #[tokio::test]
    async fn a_cli_that_fails_reports_its_last_line() {
        let setup = setup(
            "login-claude-fails",
            Agent::Claude,
            "echo 'Login failed: Request failed with status code 400' >&2; exit 1",
        );
        let mut events = setup.state.subscribe();
        setup.start(Agent::Claude, None, LOGIN_TIMEOUT).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        let failure = ended.failure.unwrap();
        assert_eq!(failure.code, crate::protocol::login_failure_code::FAILED);
        assert_eq!(
            failure.params.get("detail").map(String::as_str),
            Some("Login failed: Request failed with status code 400")
        );
    }

    #[tokio::test]
    async fn one_login_runs_per_account_and_a_second_is_refused_with_a_code() {
        let setup = setup("login-twice", Agent::Claude, "sleep 30");
        setup
            .start(Agent::Claude, Some("a"), LOGIN_TIMEOUT)
            .unwrap();
        let refused = setup
            .start(Agent::Claude, Some("a"), LOGIN_TIMEOUT)
            .expect_err("already running");
        let coded = refused.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::LOGIN_ALREADY_RUNNING);
        // Another account of the same agent is not in the way.
        setup
            .start(Agent::Claude, Some("b"), LOGIN_TIMEOUT)
            .unwrap();
        assert_eq!(setup.state.account_logins.running().len(), 2);
        setup.state.account_logins.stop_all().await;
    }

    #[tokio::test]
    async fn cancelling_ends_the_login_and_kills_the_cli() {
        let setup = setup(
            "login-cancel",
            Agent::Claude,
            "echo $$ > \"$HOME/pid\"; sleep 30",
        );
        let mut events = setup.state.subscribe();
        setup.start(Agent::Claude, None, LOGIN_TIMEOUT).unwrap();
        let pid = until_pid(&setup.dir.join("bin/pid")).await;

        setup
            .state
            .account_logins
            .cancel(Agent::Claude, None)
            .unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Cancelled);
        // Announced only once the CLI has gone, not while it is still dying.
        // SAFETY: signal 0 only probes whether the pid exists.
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
        // And its scratch directory is gone with it.
        assert_eq!(std::fs::read_dir(setup.dir.join("run")).unwrap().count(), 0);

        let refused = setup
            .state
            .account_logins
            .cancel(Agent::Claude, None)
            .expect_err("nothing to cancel");
        assert_eq!(
            refused.downcast_ref::<CodedError>().unwrap().code,
            error_code::LOGIN_NOT_RUNNING
        );
    }

    #[tokio::test]
    async fn a_login_nobody_authorizes_is_given_up_on_and_its_process_group_killed() {
        // A child of the CLI, as `sh` forks for `sleep`, is in the group too.
        let setup = setup(
            "login-timeout",
            Agent::Claude,
            "(sleep 30 & echo $! > \"$HOME/child.pid\"; wait) & echo $$ > \"$HOME/pid\"; wait",
        );
        let (pid, child_pid) = (setup.dir.join("bin/pid"), setup.dir.join("bin/child.pid"));
        // A CLI that is slow to exec can be given up on before it has written anything, which
        // proves nothing: that attempt is made again.
        for _ in 0..5 {
            let mut events = setup.state.subscribe();
            setup
                .start(Agent::Claude, None, Duration::from_millis(1500))
                .unwrap();
            let ended = until_ended(&mut events).await.pop().unwrap();
            assert_eq!(ended.status, LoginStatus::TimedOut);
            if let (Some(pid), Some(child_pid)) = (read_pid_file(&pid), read_pid_file(&child_pid)) {
                until_gone(pid).await;
                until_gone(child_pid).await;
                return;
            }
        }
        panic!("the fake CLI never got as far as writing its pids");
    }

    #[tokio::test]
    async fn a_login_task_that_is_dropped_without_an_outcome_ends_the_login_as_failed() {
        let setup = setup("login-ending-guard", Agent::Claude, "sleep 30");
        let mut events = setup.state.subscribe();
        setup.start(Agent::Claude, None, LOGIN_TIMEOUT).unwrap();
        // What a panic in the task unwinds through.
        drop(Ending::new(
            setup.state.clone(),
            Key {
                agent: Agent::Claude,
                account: None,
            },
        ));
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert_eq!(failure_code(&ended), login_failure_code::FAILED);
        assert!(setup.state.account_logins.running().is_empty());
        setup.state.account_logins.stop_all().await;
    }

    #[tokio::test]
    async fn a_cancel_during_claudes_sign_in_check_ends_the_login_as_cancelled() {
        let body = CLAUDE_SIGNS_IN.replace(
            r#"echo '{"loggedIn":true}'"#,
            r#"sleep 10; echo '{"loggedIn":true}'"#,
        );
        let setup = setup("login-cancel-late", Agent::Claude, &body);
        let mut events = setup.state.subscribe();
        setup.start(Agent::Claude, None, LOGIN_TIMEOUT).unwrap();
        loop {
            let Ok(Event::AccountLoginUpdated { login }) = events.recv().await else {
                continue;
            };
            if login.status == LoginStatus::Waiting {
                break;
            }
        }
        // The CLI exits 0.4 seconds after the page, and its last words are waited for after that.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        setup
            .state
            .account_logins
            .cancel(Agent::Claude, None)
            .unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Cancelled);
    }

    #[tokio::test]
    async fn a_codex_that_exits_reports_what_it_said_last() {
        let setup = setup(
            "login-codex-exits",
            Agent::Codex,
            "echo 'Error: bad configuration' >&2; exit 1",
        );
        let mut events = setup.state.subscribe();
        setup.start(Agent::Codex, None, LOGIN_TIMEOUT).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert_eq!(
            ended
                .failure
                .unwrap()
                .params
                .get("detail")
                .map(String::as_str),
            Some("Error: bad configuration")
        );
    }

    #[tokio::test]
    async fn the_default_accounts_codex_home_is_created_when_it_is_missing() {
        let setup = setup(
            "login-codex-default-home",
            Agent::Codex,
            &codex_server(true),
        );
        let mut events = setup.state.subscribe();
        let home = setup.dir.join("resolved-codex-home");
        AccountLogins::start_with(
            &setup.state,
            Start {
                agent: Agent::Codex,
                account: None,
                config_dir: None,
                resolved_dir: Some(home.to_string_lossy().into_owned()),
            },
            setup.env.clone(),
            setup.dir.join("run"),
            LOGIN_TIMEOUT,
        )
        .unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Succeeded);
        assert!(home.is_dir());
    }

    #[tokio::test]
    async fn every_end_of_a_login_makes_the_accounts_reading_due() {
        let setup = setup("login-due", Agent::Claude, "exit 1");
        let mut events = setup.state.subscribe();
        let quota = crate::protocol::AccountQuota {
            agent: Agent::Claude,
            account: Some("a".to_string()),
            fetched_at: 1,
            plan: None,
            windows: Vec::new(),
            unavailable: None,
        };
        setup.state.account_quotas.hold_for_test(&quota);
        setup
            .start(Agent::Claude, Some("a"), LOGIN_TIMEOUT)
            .unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert!(setup
            .state
            .account_quotas
            .is_due_for_test(Agent::Claude, Some("a")));
    }

    #[tokio::test]
    async fn stopping_every_login_kills_its_cli_without_announcing_anything() {
        let setup = setup(
            "login-stop-all",
            Agent::Claude,
            "echo $$ > \"$HOME/pid\"; sleep 30",
        );
        let mut events = setup.state.subscribe();
        setup.start(Agent::Claude, None, LOGIN_TIMEOUT).unwrap();
        let pid = until_pid(&setup.dir.join("bin/pid")).await;
        while events.try_recv().is_ok() {}

        setup.state.account_logins.stop_all().await;
        until_gone(pid).await;
        assert!(setup.state.account_logins.running().is_empty());
        assert!(events.try_recv().is_err());
    }

    async fn until_pid(path: &std::path::Path) -> i32 {
        tokio::time::timeout(PATIENCE, async {
            loop {
                if let Some(pid) = read_pid_file(path) {
                    return pid;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the fake CLI never wrote its pid")
    }

    async fn until_gone(pid: i32) {
        tokio::time::timeout(PATIENCE, async {
            // SAFETY: signal 0 only probes whether the pid exists.
            while unsafe { libc::kill(pid, 0) } == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the process was still running");
    }

    const GROK_SIGNS_IN: &str = r#"
        echo "$*" > "$HOME/args"
        echo 'To sign in, open this URL in your browser:' >&2
        echo 'https://accounts.x.ai/oauth2/device?user_code=ABCD-1234' >&2
        echo 'Confirm this code in your browser:' >&2
        echo 'ABCD-1234' >&2
        echo 'Waiting for authorization...' >&2
        sleep 0.4
        mkdir -p "$GROK_HOME"
        echo '{}' > "$GROK_HOME/auth.json"
        echo '✓ Signed in as someone@example.com' >&2
        exit 0
    "#;

    #[tokio::test]
    async fn grok_shows_the_device_code_and_leaves_opening_the_page_to_the_cli() {
        let setup = setup("login-grok", Agent::Grok, GROK_SIGNS_IN);
        let mut events = setup.state.subscribe();
        setup.start(Agent::Grok, None, LOGIN_TIMEOUT).unwrap();

        let updates = until_ended(&mut events).await;
        let waiting: Vec<_> = updates
            .iter()
            .filter(|login| login.status == LoginStatus::Waiting)
            .collect();
        let last = waiting.last().expect("a waiting update");
        assert_eq!(
            last.authorization_url.as_deref(),
            Some("https://accounts.x.ai/oauth2/device?user_code=ABCD-1234")
        );
        assert_eq!(last.device_code.as_deref(), Some("ABCD-1234"));
        assert!(last.opened_by_agent);
        assert_eq!(updates.last().unwrap().status, LoginStatus::Succeeded);
        let args = std::fs::read_to_string(setup.dir.join("bin/args")).unwrap();
        assert_eq!(args.trim(), "login --device-auth");
    }

    #[tokio::test]
    async fn grok_exiting_zero_without_its_credential_is_not_a_success() {
        let body = GROK_SIGNS_IN.replace("echo '{}' > \"$GROK_HOME/auth.json\"", "true");
        let setup = setup("login-grok-unconfirmed", Agent::Grok, &body);
        let mut events = setup.state.subscribe();
        setup.start(Agent::Grok, None, LOGIN_TIMEOUT).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert_eq!(
            failure_code(&ended),
            crate::protocol::login_failure_code::NOT_CONFIRMED
        );
    }

    /// A Codex app server that answers the login start with an authorization URL and, unless told
    /// to wait, completes it after a moment.
    fn codex_server(completes: bool) -> String {
        format!(
            r#"
        echo "$CODEX_HOME" > "$HOME/home"
        echo $$ > "$HOME/pid"
        while read line; do
          case "$line" in
            *'"initialize"'*) echo '{{"id":1,"result":{{"userAgent":"fake"}}}}';;
            *'account/login/start'*)
              echo "$line" > "$HOME/start"
              echo '{{"id":2,"result":{{"type":"chatgpt","loginId":"L1","authUrl":"https://auth.test/authorize"}}}}'
              if {completes}; then
                sleep 0.3
                echo '{{"method":"account/login/completed","params":{{"loginId":"L1","success":true,"error":null}}}}'
              fi;;
            *'account/login/cancel'*)
              echo '{{"id":4,"result":{{"status":"canceled"}}}}'
              echo '{{"method":"account/login/completed","params":{{"loginId":"L1","success":false,"error":"Login was not completed"}}}}';;
            *'account/read'*) echo '{{"id":3,"result":{{"account":{{"type":"chatgpt","email":"a@b.c","planType":"plus"}},"requiresOpenaiAuth":true}}}}';;
          esac
        done
    "#
        )
    }

    #[tokio::test]
    async fn codex_logs_in_through_its_app_server_in_a_directory_created_first() {
        let setup = setup("login-codex", Agent::Codex, &codex_server(true));
        let mut events = setup.state.subscribe();
        setup.start(Agent::Codex, Some("a"), LOGIN_TIMEOUT).unwrap();

        let updates = until_ended(&mut events).await;
        let waiting = updates
            .iter()
            .find(|login| login.status == LoginStatus::Waiting)
            .expect("a waiting update");
        assert_eq!(
            waiting.authorization_url.as_deref(),
            Some("https://auth.test/authorize")
        );
        assert!(!waiting.opened_by_agent);
        assert_eq!(updates.last().unwrap().status, LoginStatus::Succeeded);
        let start = std::fs::read_to_string(setup.dir.join("bin/start")).unwrap();
        assert!(start.contains(r#""type":"chatgpt""#), "{start}");
        // Codex refuses a missing home.
        assert!(setup.dir.join("codex-home").is_dir());
    }

    #[tokio::test]
    async fn codex_allows_one_login_at_a_time_across_accounts() {
        let setup = setup("login-codex-busy", Agent::Codex, &codex_server(false));
        setup.start(Agent::Codex, Some("a"), LOGIN_TIMEOUT).unwrap();
        let refused = setup
            .start(Agent::Codex, Some("b"), LOGIN_TIMEOUT)
            .expect_err("busy");
        assert_eq!(
            refused.downcast_ref::<CodedError>().unwrap().code,
            error_code::LOGIN_CODEX_BUSY
        );
        setup.state.account_logins.stop_all().await;
    }

    #[tokio::test]
    async fn cancelling_a_codex_login_asks_the_app_server_to_cancel_it() {
        let setup = setup("login-codex-cancel", Agent::Codex, &codex_server(false));
        let mut events = setup.state.subscribe();
        setup.start(Agent::Codex, None, LOGIN_TIMEOUT).unwrap();
        let pid = until_pid(&setup.dir.join("bin/pid")).await;
        // Wait for the URL, so the cancel has a login id to name.
        loop {
            let Ok(Event::AccountLoginUpdated { login }) = events.recv().await else {
                continue;
            };
            if login.status == LoginStatus::Waiting {
                break;
            }
        }
        setup
            .state
            .account_logins
            .cancel(Agent::Codex, None)
            .unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Cancelled);
        until_gone(pid).await;
    }

    #[tokio::test]
    async fn a_codex_login_that_ends_unsuccessfully_reports_the_servers_words() {
        let body = codex_server(true).replace(
            r#""success":true,"error":null"#,
            r#""success":false,"error":"Login server error: denied""#,
        );
        let setup = setup("login-codex-denied", Agent::Codex, &body);
        let mut events = setup.state.subscribe();
        setup.start(Agent::Codex, None, LOGIN_TIMEOUT).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert_eq!(
            ended
                .failure
                .unwrap()
                .params
                .get("detail")
                .map(String::as_str),
            Some("Login server error: denied")
        );
    }

    #[tokio::test]
    async fn a_codex_login_the_server_timed_out_is_a_timeout() {
        let body = codex_server(true).replace(
            r#""success":true,"error":null"#,
            r#""success":false,"error":"Login timed out""#,
        );
        let setup = setup("login-codex-timed-out", Agent::Codex, &body);
        let mut events = setup.state.subscribe();
        setup.start(Agent::Codex, None, LOGIN_TIMEOUT).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::TimedOut);
    }

    #[tokio::test]
    async fn a_missing_binary_refuses_the_login_before_anything_is_announced() {
        let (state, dir) = crate::test_support::app_state("login-no-binary");
        let mut events = state.subscribe();
        let refused = AccountLogins::start_with(
            &state,
            Start {
                agent: Agent::Grok,
                account: None,
                config_dir: None,
                resolved_dir: None,
            },
            HashMap::from([(
                "PATH".to_string(),
                dir.join("empty").to_string_lossy().into_owned(),
            )]),
            dir.join("run"),
            LOGIN_TIMEOUT,
        )
        .expect_err("no grok");
        assert_eq!(
            refused.downcast_ref::<CodedError>().unwrap().code,
            error_code::BINARY_NOT_FOUND
        );
        assert!(events.try_recv().is_err());
        assert!(state.account_logins.running().is_empty());
    }

    #[tokio::test]
    async fn a_login_in_progress_replays_in_the_snapshot_list_with_its_page() {
        let setup = setup(
            "login-running-list",
            Agent::Grok,
            &GROK_SIGNS_IN.replace("sleep 0.4", "sleep 30"),
        );
        let mut events = setup.state.subscribe();
        setup.start(Agent::Grok, Some("a"), LOGIN_TIMEOUT).unwrap();
        loop {
            let Ok(Event::AccountLoginUpdated { login }) = events.recv().await else {
                continue;
            };
            if login.device_code.is_some() {
                break;
            }
        }
        let running = setup.state.account_logins.running();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].status, LoginStatus::Waiting);
        setup.state.account_logins.stop_all().await;
    }
}
