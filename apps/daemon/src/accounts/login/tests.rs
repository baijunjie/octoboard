use super::*;
use crate::test_support::{fake_agent, is_running, read_pid_file, ScratchDir, PATIENCE, SHELL_PID};

struct Setup {
    state: Arc<AppState>,
    dir: ScratchDir,
    env: HashMap<String, String>,
}

/// A daemon state and a fake CLI of `agent` running `body`, all under one scratch directory.
fn setup(name: &str, agent: AgentId, body: &str) -> Setup {
    let (state, dir) = crate::test_support::app_state(name);
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env = fake_agent(&bin, agent.descriptor().binary, body);
    Setup { state, dir, env }
}

impl Setup {
    fn start(&self, agent: AgentId, account: Option<&str>, timeout: Duration) -> Result<()> {
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

/// A flow that runs no CLI and waits until it is cancelled, declaring the timeout it holds.
struct WaitsForCancel(Option<Duration>);

impl flows::LoginFlow for WaitsForCancel {
    fn run<'a>(
        &'a self,
        _flow: &'a Flow,
        _progress: &'a flows::Progress,
        cancel: &'a Notify,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            cancel.notified().await;
            Outcome::Cancelled
        })
    }

    fn timeout(&self) -> Option<Duration> {
        self.0
    }
}

impl Setup {
    /// Starts the default account's login of `agent` over `login` in place of its provider's
    /// flow, with `undeclared_timeout` for a flow that declares none.
    fn start_flow(
        &self,
        agent: AgentId,
        login: &'static dyn flows::LoginFlow,
        undeclared_timeout: Duration,
    ) -> Result<()> {
        AccountLogins::start_flow(
            &self.state,
            Start {
                agent,
                account: None,
                config_dir: None,
                resolved_dir: None,
            },
            login,
            self.env.clone(),
            self.dir.join("run"),
            undeclared_timeout,
        )
    }
}

/// A flow that declares its own timeout is given up on after that long, not after the daemon's
/// own wait, whichever of the two is the longer.
#[tokio::test]
async fn a_flow_that_declares_its_timeout_is_given_that_long() {
    static SHORT: WaitsForCancel = WaitsForCancel(Some(Duration::from_millis(200)));
    static LONG: WaitsForCancel = WaitsForCancel(Some(Duration::from_secs(15 * 60)));

    let setup = setup("login-declared-timeout", AgentId::CLAUDE, "exit 0");
    let mut events = setup.state.subscribe();
    setup
        .start_flow(AgentId::CLAUDE, &SHORT, LOGIN_TIMEOUT)
        .unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::TimedOut);

    // A code that lives longer than the daemon's own wait outlasts it.
    let undeclared = Duration::from_millis(200);
    setup
        .start_flow(AgentId::CLAUDE, &LONG, undeclared)
        .unwrap();
    tokio::time::sleep(undeclared * 4).await;
    assert_eq!(
        setup.state.account_logins.running().len(),
        1,
        "the login was given up on after the daemon's own wait"
    );
    setup
        .state
        .account_logins
        .cancel(AgentId::CLAUDE, None)
        .unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Cancelled);
}

/// A flow that declares none is given the daemon's own wait, which is ten minutes.
#[tokio::test]
async fn a_flow_that_declares_no_timeout_is_given_the_daemons_wait() {
    static UNDECLARED: WaitsForCancel = WaitsForCancel(None);
    assert_eq!(LOGIN_TIMEOUT, Duration::from_secs(10 * 60));

    let setup = setup("login-undeclared-timeout", AgentId::CLAUDE, "exit 0");
    let mut events = setup.state.subscribe();
    setup
        .start_flow(AgentId::CLAUDE, &UNDECLARED, Duration::from_millis(200))
        .unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::TimedOut);
}

/// Every update broadcast until the login ends, the ending included.
async fn until_ended(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<AccountLogin> {
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

fn failure_code(login: &AccountLogin) -> crate::protocol::Code {
    login.failure.as_ref().expect("a failure").code
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
    let setup = setup("login-claude", AgentId::CLAUDE, CLAUDE_SIGNS_IN);
    let mut events = setup.state.subscribe();
    setup
        .start(AgentId::CLAUDE, Some("a"), LOGIN_TIMEOUT)
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
        Some("https://claude.test/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback")
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
    let setup = setup("login-claude-unconfirmed", AgentId::CLAUDE, &body);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Failed);
    assert_eq!(
        failure_code(&ended),
        crate::protocol::login_failure_code::NOT_CONFIRMED
    );
}

/// `claude auth status` is read within a bound like every other short query of an agent CLI:
/// one that floods its output confirms nothing.
#[tokio::test]
async fn a_sign_in_check_that_floods_its_output_confirms_nothing() {
    let body = CLAUDE_SIGNS_IN.replace(
        r#"echo '{"loggedIn":true}'; exit 0;;"#,
        &format!(
            r#"echo '{{"loggedIn":true}}'; head -c {} /dev/zero | tr '\0' ' '; exit 0;;"#,
            64 * 1024
        ),
    );
    let setup = setup("login-claude-flooded-status", AgentId::CLAUDE, &body);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
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
        AgentId::CLAUDE,
        "echo 'Login failed: Request failed with status code 400' >&2; exit 1",
    );
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
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
    let setup = setup("login-twice", AgentId::CLAUDE, "sleep 30");
    setup
        .start(AgentId::CLAUDE, Some("a"), LOGIN_TIMEOUT)
        .unwrap();
    let refused = setup
        .start(AgentId::CLAUDE, Some("a"), LOGIN_TIMEOUT)
        .expect_err("already running");
    let coded = refused.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::LOGIN_ALREADY_RUNNING);
    // Another account of the same agent is not in the way.
    setup
        .start(AgentId::CLAUDE, Some("b"), LOGIN_TIMEOUT)
        .unwrap();
    assert_eq!(setup.state.account_logins.running().len(), 2);
    setup.state.account_logins.stop_all().await;
}

#[tokio::test]
async fn cancelling_ends_the_login_and_kills_the_cli() {
    let setup = setup(
        "login-cancel",
        AgentId::CLAUDE,
        &crate::test_support::records_pid_then_waits("\"$HOME/pid\""),
    );
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
    let pid = until_pid(&setup.dir.join("bin/pid")).await;

    setup
        .state
        .account_logins
        .cancel(AgentId::CLAUDE, None)
        .unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Cancelled);
    // Announced only once the CLI has gone, not while it is still dying.
    assert!(!is_running(pid));
    // And its scratch directory is gone with it.
    assert_eq!(std::fs::read_dir(setup.dir.join("run")).unwrap().count(), 0);

    let refused = setup
        .state
        .account_logins
        .cancel(AgentId::CLAUDE, None)
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
        AgentId::CLAUDE,
        &format!("{} & echo {SHELL_PID} > \"$HOME/pid\"; wait", cli_child()),
    );
    let (pid, child_pid) = (setup.dir.join("bin/pid"), setup.dir.join("bin/child.pid"));
    // A CLI that is slow to exec can be given up on before it has written anything, which
    // proves nothing: that attempt is made again.
    for _ in 0..5 {
        let mut events = setup.state.subscribe();
        setup
            .start(AgentId::CLAUDE, None, Duration::from_millis(1500))
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
    let setup = setup("login-ending-guard", AgentId::CLAUDE, "sleep 30");
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
    // What a panic in the task unwinds through.
    drop(Ending::new(
        setup.state.clone(),
        Key {
            agent: AgentId::CLAUDE,
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
        r#"touch "$HOME/checking"; sleep 30; echo '{"loggedIn":true}'"#,
    );
    let setup = setup("login-cancel-late", AgentId::CLAUDE, &body);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
    // Cancelled once the sign-in check has begun, which the fake marks before it answers.
    let checking = setup.dir.join("bin/checking");
    assert!(
        crate::test_support::within(PATIENCE, move || checking.exists()).await,
        "the sign-in was never checked"
    );
    setup
        .state
        .account_logins
        .cancel(AgentId::CLAUDE, None)
        .unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Cancelled);
}

#[tokio::test]
async fn a_codex_that_exits_reports_what_it_said_last() {
    let setup = setup(
        "login-codex-exits",
        AgentId::CODEX,
        "echo 'Error: bad configuration' >&2; exit 1",
    );
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CODEX, None, LOGIN_TIMEOUT).unwrap();
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
        AgentId::CODEX,
        &codex_server(true),
    );
    let mut events = setup.state.subscribe();
    let home = setup.dir.join("resolved-codex-home");
    AccountLogins::start_with(
        &setup.state,
        Start {
            agent: AgentId::CODEX,
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
    let setup = setup("login-due", AgentId::CLAUDE, "exit 1");
    let mut events = setup.state.subscribe();
    let quota = crate::protocol::AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: Some("a".to_string()),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: Vec::new(),
        unavailable: None,
    };
    setup.state.account_quotas.hold_for_test(&quota);
    setup
        .start(AgentId::CLAUDE, Some("a"), LOGIN_TIMEOUT)
        .unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Failed);
    assert!(setup
        .state
        .account_quotas
        .is_due_for_test(AgentId::CLAUDE, Some("a")));
}

#[tokio::test]
async fn stopping_every_login_kills_its_cli_without_announcing_anything() {
    let setup = setup(
        "login-stop-all",
        AgentId::CLAUDE,
        &crate::test_support::records_pid_then_waits("\"$HOME/pid\""),
    );
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
    let pid = until_pid(&setup.dir.join("bin/pid")).await;
    while events.try_recv().is_ok() {}

    setup.state.account_logins.stop_all().await;
    until_gone(pid).await;
    assert!(setup.state.account_logins.running().is_empty());
    assert!(events.try_recv().is_err());
}

/// A subshell of a fake CLI's script that runs a child writing its pid to `$HOME/child.pid` and
/// sleeping 30 s. On Windows the child is a fixture process (`test_support::fixture`), whose pid is
/// Windows' own: under Git Bash `$!` is a pid of its own numbering.
fn cli_child() -> String {
    #[cfg(unix)]
    return "(sleep 30 & echo $! > \"$HOME/child.pid\"; wait)".to_string();
    #[cfg(windows)]
    return format!(
        "(cd \"$HOME\" && {})",
        crate::test_support::fixture::Steps::new()
            .write_pid(std::path::Path::new("child.pid"))
            .sleep(30)
            .program("child")
            .posix_line()
    );
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
        while is_running(pid) {
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
    let setup = setup("login-grok", AgentId::GROK, GROK_SIGNS_IN);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::GROK, None, LOGIN_TIMEOUT).unwrap();

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
    let setup = setup("login-grok-unconfirmed", AgentId::GROK, &body);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::GROK, None, LOGIN_TIMEOUT).unwrap();
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
    echo {SHELL_PID} > "$HOME/pid"
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
    let setup = setup("login-codex", AgentId::CODEX, &codex_server(true));
    let mut events = setup.state.subscribe();
    setup
        .start(AgentId::CODEX, Some("a"), LOGIN_TIMEOUT)
        .unwrap();

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
    let setup = setup("login-codex-busy", AgentId::CODEX, &codex_server(false));
    setup
        .start(AgentId::CODEX, Some("a"), LOGIN_TIMEOUT)
        .unwrap();
    let refused = setup
        .start(AgentId::CODEX, Some("b"), LOGIN_TIMEOUT)
        .expect_err("busy");
    assert_eq!(
        refused.downcast_ref::<CodedError>().unwrap().code,
        crate::protocol::Code::of_provider(crate::providers::codex::LOGIN_BUSY)
    );
    setup.state.account_logins.stop_all().await;
}

#[tokio::test]
async fn cancelling_a_codex_login_asks_the_app_server_to_cancel_it() {
    let setup = setup("login-codex-cancel", AgentId::CODEX, &codex_server(false));
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CODEX, None, LOGIN_TIMEOUT).unwrap();
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
        .cancel(AgentId::CODEX, None)
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
    let setup = setup("login-codex-denied", AgentId::CODEX, &body);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CODEX, None, LOGIN_TIMEOUT).unwrap();
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
    let setup = setup("login-codex-timed-out", AgentId::CODEX, &body);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CODEX, None, LOGIN_TIMEOUT).unwrap();
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
            agent: AgentId::GROK,
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

/// A provider that declares no login flow is refused a login request as `provider_unsupported`,
/// by `AccountLogins::start` before the shell environment is taken, with nothing announced.
#[tokio::test]
async fn a_provider_without_a_login_flow_is_refused_a_login_request() {
    let (state, _dir) = crate::test_support::app_state("login-no-flow");
    assert!(!AgentId::TEST.descriptor().capabilities.login_flow);
    let mut events = state.subscribe();
    let refused = AccountLogins::start(
        &state,
        Start {
            agent: AgentId::TEST,
            account: Some("a".to_string()),
            config_dir: Some("/tmp/a".to_string()),
            resolved_dir: None,
        },
    )
    .await
    .expect_err("no login flow");
    let coded = refused.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::PROVIDER_UNSUPPORTED);
    assert_eq!(coded.params["operation"], "sign_in");
    assert!(events.try_recv().is_err());
    assert!(state.account_logins.running().is_empty());
}

#[tokio::test]
async fn a_login_in_progress_replays_in_the_snapshot_list_with_its_page() {
    let setup = setup(
        "login-running-list",
        AgentId::GROK,
        &GROK_SIGNS_IN.replace("sleep 0.4", "sleep 30"),
    );
    let mut events = setup.state.subscribe();
    setup
        .start(AgentId::GROK, Some("a"), LOGIN_TIMEOUT)
        .unwrap();
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

/// The question the stand-in flows below ask: a provider's own, which this layer only carries.
fn question() -> LoginQuestion {
    LoginQuestion::new(
        "keep_token_in_a_file",
        "Keep the token in a file at /tmp/token?",
        &[("path", "/tmp/token")],
    )
}

/// A provider's sign-in that shows a page and a code, then asks its question and does nothing until
/// it is answered: a yes signs in, a no ends the login with the flow's own reason.
struct AsksFirst;

impl flows::LoginFlow for AsksFirst {
    fn run<'a>(
        &'a self,
        flow: &'a flows::Flow,
        progress: &'a flows::Progress,
        cancel: &'a Notify,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            progress.waiting(
                Some("https://test.example/authorize".to_string()),
                Some("ABCD-1234".to_string()),
                false,
            );
            tokio::select! {
                answer = progress.ask(question()) => match answer {
                    Ok(LoginAnswer::Yes) => Outcome::Succeeded,
                    Ok(LoginAnswer::No) => flow.failed("the token was not kept"),
                    _ => flow.failed("the login question did not receive a choice"),
                },
                () = cancel.notified() => Outcome::Cancelled,
            }
        })
    }
}

/// A sign-in that asks a second question while its first is still open, and goes by the first
/// one's answer once the second has been refused; a second question that is taken fails the login.
struct AsksTwice;

impl flows::LoginFlow for AsksTwice {
    fn run<'a>(
        &'a self,
        flow: &'a flows::Flow,
        progress: &'a flows::Progress,
        cancel: &'a Notify,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            let first = progress.ask(question());
            tokio::pin!(first);
            let second = LoginQuestion::new("another_question", "Another question?", &[]);
            // The first is polled first, which opens it, and still waits when the second asks.
            tokio::select! {
                biased;
                _ = &mut first => return flow.failed("the first question was answered unasked"),
                second = progress.ask(second) => {
                    if second != Err(flows::AlreadyAsking) {
                        return flow.failed("the second question was taken");
                    }
                }
            }
            tokio::select! {
                answer = first => match answer {
                    Ok(LoginAnswer::Yes) => Outcome::Succeeded,
                    _ => flow.failed("the first question was not answered yes"),
                },
                () = cancel.notified() => Outcome::Cancelled,
            }
        })
    }
}

/// A sign-in that stops waiting for its answer once the test says so, as one whose CLI went on by
/// itself does, and then waits to be cancelled.
struct StopsAsking {
    stop: Notify,
}

impl flows::LoginFlow for StopsAsking {
    fn run<'a>(
        &'a self,
        _flow: &'a flows::Flow,
        progress: &'a flows::Progress,
        cancel: &'a Notify,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            tokio::select! {
                _ = progress.ask(question()) => {}
                () = self.stop.notified() => {}
            }
            cancel.notified().await;
            Outcome::Cancelled
        })
    }
}

/// Starts the login of the test provider's default account with `flow`, which its descriptor does
/// not offer: the question is proven on the layer that carries it, with no provider's own flow.
fn start_asking(
    name: &str,
    flow: &'static dyn flows::LoginFlow,
    timeout: Duration,
) -> (Setup, tokio::sync::broadcast::Receiver<Event>) {
    let setup = setup(name, AgentId::TEST, "sleep 30");
    let events = setup.state.subscribe();
    setup.start_flow(AgentId::TEST, flow, timeout).unwrap();
    (setup, events)
}

/// The next update of a login, which must still be running.
async fn next_update(events: &mut tokio::sync::broadcast::Receiver<Event>) -> AccountLogin {
    tokio::time::timeout(PATIENCE, async {
        loop {
            if let Ok(Event::AccountLoginUpdated { login }) = events.recv().await {
                return login;
            }
        }
    })
    .await
    .expect("the login said nothing more")
}

/// Every update up to the one that carries the question.
async fn until_asked(events: &mut tokio::sync::broadcast::Receiver<Event>) -> AccountLogin {
    loop {
        let login = next_update(events).await;
        if login.question.is_some() {
            return login;
        }
        assert!(
            matches!(login.status, LoginStatus::Starting | LoginStatus::Waiting),
            "the login ended before it asked: {login:?}"
        );
    }
}

const NOT_RUNNING: &str = "Test Agent is not being signed in for this account.";
const NOT_OPEN: &str = "The sign-in of this Test Agent account is not waiting on that question.";

/// The code and the message of a refusal.
fn refusal(refused: &anyhow::Error) -> (crate::protocol::Code, &str) {
    let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
    (coded.code, coded.message.as_str())
}

fn answer(setup: &Setup, question: &str, answer: LoginAnswer) -> Result<()> {
    setup
        .state
        .account_logins
        .answer(&setup.state, AgentId::TEST, None, question, answer)
}

#[tokio::test]
async fn a_flows_question_waits_on_the_login_and_a_yes_lets_the_flow_go_on() {
    let (setup, mut events) = start_asking("login-question-yes", &AsksFirst, LOGIN_TIMEOUT);

    let asked = until_asked(&mut events).await;
    assert_eq!(asked.status, LoginStatus::Waiting);
    assert_eq!(asked.question, Some(question()));
    // A login taken up later (a reload, another window) finds the question too.
    assert_eq!(setup.state.account_logins.running(), [asked]);

    answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).unwrap();
    let answered = next_update(&mut events).await;
    assert_eq!(answered.status, LoginStatus::Waiting);
    assert_eq!(answered.question, None);
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Succeeded);
    assert_eq!(ended.question, None);
}

#[tokio::test]
async fn a_no_reaches_the_flow_which_ends_the_login_with_its_reason() {
    let (setup, mut events) = start_asking("login-question-no", &AsksFirst, LOGIN_TIMEOUT);
    until_asked(&mut events).await;

    answer(&setup, "keep_token_in_a_file", LoginAnswer::No).unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Failed);
    assert_eq!(ended.question, None);
    assert_eq!(
        ended
            .failure
            .unwrap()
            .params
            .get("detail")
            .map(String::as_str),
        Some("the token was not kept")
    );
}

/// Every login update broadcast so far, without waiting for more.
fn updates_so_far(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<AccountLogin> {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            Event::AccountLoginUpdated { login } => Some(login),
            _ => None,
        })
        .collect()
}

/// The daemon never answers a question itself, and the time it stays open is not counted against
/// the login's timeout ([`flows::Progress::ask`] waits however long the user takes): the login
/// waits on it past that time, unanswered and not timed out, and an answer still reaches the flow
/// then.
#[tokio::test]
async fn a_question_nobody_answers_holds_its_login_past_the_timeout_unanswered() {
    let (setup, mut events) = start_asking("login-question-timeout", &AsksFirst, LOGIN_TIMEOUT);
    until_asked(&mut events).await;

    // Past the whole timeout: a clock still counting fires within it, and its ending is announced
    // before this returns.
    crate::test_support::runtime_advanced(LOGIN_TIMEOUT).await;
    // Nothing moved on meanwhile: no update followed the question, which is still open.
    assert_eq!(updates_so_far(&mut events), []);
    assert_eq!(
        setup.state.account_logins.running()[0].question,
        Some(question())
    );

    answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Succeeded);
}

/// A sign-in that waits for the user to authorize, asks its question once the test says so and,
/// once it is answered, waits again until it is cancelled: what the login's timeout does before
/// and after a question.
struct AsksWhenTold {
    ask: Notify,
}

impl flows::LoginFlow for AsksWhenTold {
    fn run<'a>(
        &'a self,
        _flow: &'a flows::Flow,
        progress: &'a flows::Progress,
        cancel: &'a Notify,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            tokio::select! {
                () = self.ask.notified() => {}
                () = cancel.notified() => return Outcome::Cancelled,
            }
            tokio::select! {
                _ = progress.ask(question()) => {}
                () = cancel.notified() => return Outcome::Cancelled,
            }
            cancel.notified().await;
            Outcome::Cancelled
        })
    }
}

/// Whether any of `updates` ends its login.
fn any_ended(updates: &[AccountLogin]) -> bool {
    updates
        .iter()
        .any(|login| !matches!(login.status, LoginStatus::Starting | LoginStatus::Waiting))
}

/// Once its question is answered, the login's timeout goes on with what was left of it when the
/// question opened, neither the time the question stayed open nor a fresh timeout: a login left
/// waiting after the answer ends as timed out once the rest has passed.
#[tokio::test]
async fn the_timeout_goes_on_with_what_was_left_once_the_question_is_answered() {
    let flow: &'static AsksWhenTold = Box::leak(Box::new(AsksWhenTold { ask: Notify::new() }));
    let (setup, mut events) = start_asking("login-question-clock", flow, LOGIN_TIMEOUT);
    // A part of the timeout passes before the flow asks.
    let before = LOGIN_TIMEOUT / 3;
    crate::test_support::runtime_settled().await;
    crate::test_support::runtime_advanced(before).await;
    flow.ask.notify_one();
    until_asked(&mut events).await;

    // However long the question stays open, it is not counted.
    crate::test_support::runtime_advanced(LOGIN_TIMEOUT).await;
    answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).unwrap();
    // The flow takes the answer, which lets the clock go again, before the clock moves on.
    crate::test_support::runtime_settled().await;

    let left = LOGIN_TIMEOUT - before;
    crate::test_support::runtime_advanced(left - Duration::from_secs(1)).await;
    let updates = updates_so_far(&mut events);
    assert!(!any_ended(&updates), "{updates:?}");
    assert_eq!(setup.state.account_logins.running().len(), 1);

    crate::test_support::runtime_advanced(Duration::from_secs(2)).await;
    let updates = until_ended(&mut events).await;
    let ended = updates.last().unwrap();
    assert_eq!(ended.status, LoginStatus::TimedOut, "{updates:?}");
    assert_eq!(ended.question, None);
    assert_eq!(ended.failure, None);
}

/// A sign-in whose first question is answered and which asks a second before the first's wait has
/// completed, as a flow polling two `ask` futures can, once the test says the first was answered;
/// a yes to the second signs in.
struct AsksAgainAsTheFirstIsAnswered {
    answered: Notify,
}

impl flows::LoginFlow for AsksAgainAsTheFirstIsAnswered {
    fn run<'a>(
        &'a self,
        flow: &'a flows::Flow,
        progress: &'a flows::Progress,
        cancel: &'a Notify,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            let first = progress.ask(question());
            tokio::pin!(first);
            // Polling the first opens it; the test's signal comes once it has been answered.
            tokio::select! {
                biased;
                () = self.answered.notified() => {}
                _ = &mut first => return flow.failed("the first wait completed unpolled"),
            }
            let second = progress.ask(LoginQuestion::new(
                "another_question",
                "Another question?",
                &[],
            ));
            tokio::pin!(second);
            // The second is polled first, which opens it, and then the first's wait completes.
            tokio::select! {
                biased;
                _ = &mut second => return flow.failed("the second question was answered unasked"),
                first = &mut first => if first != Ok(LoginAnswer::Yes) {
                    return flow.failed("the first question was not answered yes");
                },
            }
            tokio::select! {
                answer = second => match answer {
                    Ok(LoginAnswer::Yes) => Outcome::Succeeded,
                    _ => flow.failed("the second question was not answered yes"),
                },
                () = cancel.notified() => Outcome::Cancelled,
            }
        })
    }
}

/// The question that holds the clock is the one that is open: the first question's wait completing
/// after a second has opened does not let the clock run while the second waits on the user.
#[tokio::test]
async fn a_second_question_opened_as_the_first_is_answered_holds_the_clock() {
    let flow: &'static AsksAgainAsTheFirstIsAnswered =
        Box::leak(Box::new(AsksAgainAsTheFirstIsAnswered {
            answered: Notify::new(),
        }));
    let (setup, mut events) = start_asking("login-question-again", flow, LOGIN_TIMEOUT);
    until_asked(&mut events).await;
    answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).unwrap();
    flow.answered.notify_one();
    let asked = until_asked(&mut events).await;
    assert_eq!(
        asked.question.map(|question| question.id),
        Some("another_question".to_string())
    );
    crate::test_support::runtime_settled().await;

    // Past the whole timeout: a clock let go by the first's wait fires within it.
    crate::test_support::runtime_advanced(LOGIN_TIMEOUT).await;
    let updates = updates_so_far(&mut events);
    assert!(!any_ended(&updates), "{updates:?}");

    answer(&setup, "another_question", LoginAnswer::Yes).unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Succeeded, "{ended:?}");
}

/// What closing the login dialog without an answer does: the login is cancelled, unanswered.
#[tokio::test]
async fn cancelling_a_login_that_is_asking_ends_it_as_cancelled() {
    let (setup, mut events) = start_asking("login-question-cancel", &AsksFirst, LOGIN_TIMEOUT);
    until_asked(&mut events).await;
    setup
        .state
        .account_logins
        .cancel(AgentId::TEST, None)
        .unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Cancelled);
    assert_eq!(ended.question, None);
    assert_eq!(ended.failure, None);
    let refused = answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).expect_err("no login");
    assert_eq!(
        refusal(&refused),
        (error_code::LOGIN_NOT_RUNNING, NOT_RUNNING)
    );
}

#[tokio::test]
async fn only_an_answer_to_the_question_that_is_open_is_taken() {
    let (setup, mut events) = start_asking("login-question-other", &AsksFirst, LOGIN_TIMEOUT);
    until_asked(&mut events).await;

    let refused = answer(&setup, "another_question", LoginAnswer::Yes).expect_err("not that one");
    assert_eq!(
        refusal(&refused),
        (error_code::LOGIN_QUESTION_NOT_OPEN, NOT_OPEN)
    );
    // The question is still there to answer, and the login still waits on it.
    assert_eq!(
        setup.state.account_logins.running()[0].question,
        Some(question())
    );

    answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).unwrap();
    // A second answer, as a second window's would be, finds the question gone or the login over.
    let refused = answer(&setup, "keep_token_in_a_file", LoginAnswer::No).expect_err("answered");
    let refused = refusal(&refused);
    assert!(
        refused == (error_code::LOGIN_QUESTION_NOT_OPEN, NOT_OPEN)
            || refused == (error_code::LOGIN_NOT_RUNNING, NOT_RUNNING),
        "{refused:?}"
    );
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Succeeded);
}

#[tokio::test]
async fn a_flow_that_stops_waiting_for_its_answer_takes_the_question_back() {
    let flow: &'static StopsAsking = Box::leak(Box::new(StopsAsking {
        stop: Notify::new(),
    }));
    let (setup, mut events) = start_asking("login-question-withdrawn", flow, LOGIN_TIMEOUT);
    until_asked(&mut events).await;
    flow.stop.notify_one();
    let withdrawn = next_update(&mut events).await;
    assert_eq!(withdrawn.status, LoginStatus::Waiting);
    assert_eq!(withdrawn.question, None);
    let refused = answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).expect_err("withdrawn");
    assert_eq!(
        refusal(&refused),
        (error_code::LOGIN_QUESTION_NOT_OPEN, NOT_OPEN)
    );
    setup.state.account_logins.stop_all().await;
}

/// A login asks one question at a time: a second while the first is open is refused to the flow,
/// and the first stays open for its answer.
#[tokio::test]
async fn a_second_question_is_refused_while_the_first_is_open() {
    let (setup, mut events) = start_asking("login-question-second", &AsksTwice, LOGIN_TIMEOUT);
    let asked = until_asked(&mut events).await;
    assert_eq!(asked.question, Some(question()));
    let refused = answer(&setup, "another_question", LoginAnswer::Yes).expect_err("never opened");
    assert_eq!(
        refusal(&refused),
        (error_code::LOGIN_QUESTION_NOT_OPEN, NOT_OPEN)
    );
    answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).unwrap();
    let ended = until_ended(&mut events).await.pop().unwrap();
    assert_eq!(ended.status, LoginStatus::Succeeded, "{ended:?}");
}

/// The key of the test provider's default account, whose login `start_asking` starts.
fn test_key() -> Key {
    Key {
        agent: AgentId::TEST,
        account: None,
    }
}

/// An asker that stops waiting after its question was answered, and after another was asked,
/// takes back nothing: the later question is not its own.
#[tokio::test]
async fn an_asker_takes_back_only_its_own_question() {
    let flow: &'static WaitsForCancel = Box::leak(Box::new(WaitsForCancel(None)));
    let (setup, _events) = start_asking("login-question-serial", flow, LOGIN_TIMEOUT);
    let logins = &setup.state.account_logins;
    let ask = |question: LoginQuestion| {
        let (answer, answered) = oneshot::channel();
        match logins.ask(&setup.state, &test_key(), question, answer) {
            Asked::Open(serial) => (serial, answered),
            _ => panic!("the question was not opened"),
        }
    };

    let (first, _first_answered) = ask(question());
    answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).unwrap();
    let later = LoginQuestion::new("another_question", "Another question?", &[]);
    let (second, _second_answered) = ask(later.clone());

    assert!(!logins.withdraw_question(&test_key(), first));
    assert_eq!(logins.running()[0].question, Some(later));
    assert!(logins.withdraw_question(&test_key(), second));
    assert_eq!(logins.running()[0].question, None);
    logins.stop_all().await;
}

/// An answer for a question whose asker has stopped waiting is refused, not acknowledged and
/// lost: the question is about to be taken back.
#[tokio::test]
async fn an_answer_nobody_waits_for_is_refused() {
    let flow: &'static WaitsForCancel = Box::leak(Box::new(WaitsForCancel(None)));
    let (setup, _events) = start_asking("login-question-gone", flow, LOGIN_TIMEOUT);
    let logins = &setup.state.account_logins;
    let (answer_to, answered) = oneshot::channel();
    assert!(matches!(
        logins.ask(&setup.state, &test_key(), question(), answer_to),
        Asked::Open(_)
    ));
    drop(answered);

    let refused = answer(&setup, "keep_token_in_a_file", LoginAnswer::Yes).expect_err("nobody");
    assert_eq!(
        refusal(&refused),
        (error_code::LOGIN_QUESTION_NOT_OPEN, NOT_OPEN)
    );
    // The question is the asker's to take back, not the refusal's.
    assert_eq!(logins.running()[0].question, Some(question()));
    logins.stop_all().await;
}

mod asking_on_a_terminal {
    use super::*;

    enum TerminalScenario {
        Question,
        GivesUp,
        Redraws,
    }

    /// Shell text for a fake CLI that waits until the test creates `$HOME/<name>`, for as long as
    /// the test's patience at most, so that it moves on when the test says and not after a time.
    #[cfg(unix)]
    fn until_released(name: &str) -> String {
        format!(
            "n=0; while [ ! -e \"$HOME/{name}\" ] && [ $n -lt {polls} ]; do sleep 0.05; n=$((n+1)); done",
            polls = PATIENCE.as_millis() / 50
        )
    }

    /// A fake login CLI that does `after_asking` once it has printed its prompt.
    #[cfg(unix)]
    fn asks_and_then(after_asking: &str) -> String {
        format!(
            r#"echo {SHELL_PID} > "$HOME/pid"
if [ -t 0 ] && [ -t 1 ]; then
  printf 'Keep the token in a file? (y/N) '
  {after_asking}
  if [ "$answer" = y ]; then echo kept > "$HOME/token"; exit 0; fi
  echo 'The token was not kept.'
  exit 1
fi
echo 'No terminal to ask on.'
exit 1
"#
        )
    }

    #[cfg(unix)]
    fn terminal_setup(name: &str, scenario: TerminalScenario) -> Setup {
        let body = match scenario {
            TerminalScenario::Question => asks_and_then("read answer"),
            TerminalScenario::GivesUp => asks_and_then(&format!(
                "{}; echo 'Gave up waiting.'; exit 1",
                until_released("give-up")
            )),
            TerminalScenario::Redraws => format!(
                r#"echo {SHELL_PID} > "$HOME/pid"
printf 'Keep the token in a file? (y/N) \rKeep the token in a file? (y/N) '
read answer
{}
if [ "$answer" = y ]; then echo kept > "$HOME/token"; exit 0; fi
exit 1
"#,
                until_released("release")
            ),
        };
        setup(name, AgentId::TEST, &body)
    }

    #[cfg(windows)]
    fn terminal_setup(name: &str, scenario: TerminalScenario) -> Setup {
        use crate::test_support::{fake_native_agent, fixture::Steps};

        let (state, dir) = crate::test_support::app_state(name);
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let steps = Steps::new().write_pid(&bin.join("pid"));
        let prompt = "Keep the token in a file? (y/N) ";
        let yes = Steps::new().write_file(&bin.join("token"), "kept").exit();
        let no = Steps::new().print("The token was not kept.").fail();
        let pipes = Steps::new().say("No terminal to ask on.").fail();
        let steps = match scenario {
            TerminalScenario::Question => steps.ask(prompt, None, yes, no, pipes),
            TerminalScenario::GivesUp => steps
                .print(prompt)
                .wait_for(&bin.join("give-up"))
                .print("\r\nGave up waiting.")
                .fail(),
            TerminalScenario::Redraws => steps.print(&format!("{prompt}\r")).ask(
                prompt,
                None,
                Steps::new()
                    .wait_for(&bin.join("release"))
                    .write_file(&bin.join("token"), "kept")
                    .exit(),
                no,
                pipes,
            ),
        };
        let env = fake_native_agent(&bin, AgentId::TEST.descriptor().binary, steps);
        Setup { state, dir, env }
    }

    /// Starts the login of the test provider's default account with its terminal flow over the fake
    /// CLI above.
    fn start_on_a_terminal(
        name: &str,
        timeout: Duration,
    ) -> (Setup, tokio::sync::broadcast::Receiver<Event>) {
        let setup = terminal_setup(name, TerminalScenario::Question);
        let events = setup.state.subscribe();
        setup
            .start_flow(
                AgentId::TEST,
                &crate::adapter::test_provider::AsksOnATerminal,
                timeout,
            )
            .unwrap();
        (setup, events)
    }

    const TERMINAL_QUESTION: &str = crate::adapter::test_provider::TERMINAL_QUESTION;

    /// The question of a CLI that asks only a terminal reaches the user, and their yes reaches the
    /// CLI, which then does what it asked about.
    #[tokio::test]
    async fn a_question_a_cli_asks_only_on_a_terminal_reaches_the_user_and_a_yes_reaches_the_cli() {
        let (setup, mut events) = start_on_a_terminal("login-terminal-yes", LOGIN_TIMEOUT);
        let asked = until_asked(&mut events).await;
        assert_eq!(
            asked.question.map(|question| question.id),
            Some(TERMINAL_QUESTION.to_string())
        );
        // Nothing was answered for the user meanwhile.
        assert!(!setup.dir.join("bin/token").exists());

        answer(&setup, TERMINAL_QUESTION, LoginAnswer::Yes).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Succeeded, "{ended:?}");
        assert_eq!(
            std::fs::read_to_string(setup.dir.join("bin/token"))
                .unwrap()
                .trim(),
            "kept"
        );
    }

    #[tokio::test]
    async fn a_no_to_a_terminal_question_reaches_the_cli_and_the_login_ends_with_its_words() {
        let (setup, mut events) = start_on_a_terminal("login-terminal-no", LOGIN_TIMEOUT);
        until_asked(&mut events).await;

        answer(&setup, TERMINAL_QUESTION, LoginAnswer::No).unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert_eq!(
            ended
                .failure
                .unwrap()
                .params
                .get("detail")
                .map(String::as_str),
            Some("The token was not kept.")
        );
        assert!(!setup.dir.join("bin/token").exists());
    }

    /// A terminal question nobody answers is never answered for the user: the CLI waits at its
    /// prompt with the question open, and cancelling the login ends the CLI before the cancellation
    /// is announced. That the time the question stays open is not counted against the login's
    /// timeout is the clock's rule, the same for every flow, and proven without a CLI on the path
    /// by `a_question_nobody_answers_holds_its_login_past_the_timeout_unanswered`.
    #[tokio::test]
    async fn a_terminal_question_nobody_answers_keeps_its_cli_waiting_until_the_login_is_cancelled()
    {
        let (setup, mut events) = start_on_a_terminal("login-terminal-cancel", LOGIN_TIMEOUT);
        until_asked(&mut events).await;
        let pid = until_pid(&setup.dir.join("bin/pid")).await;
        assert!(is_running(pid));
        assert_eq!(
            setup.state.account_logins.running()[0]
                .question
                .as_ref()
                .map(|question| question.id.as_str()),
            Some(TERMINAL_QUESTION)
        );
        assert!(!setup.dir.join("bin/token").exists());

        setup
            .state
            .account_logins
            .cancel(AgentId::TEST, None)
            .unwrap();
        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Cancelled);
        assert_eq!(ended.question, None);
        assert!(!is_running(pid));
        assert!(!setup.dir.join("bin/token").exists());
    }

    /// A CLI that gives up at its prompt by itself leaves the user no question to answer: the login
    /// ends with the CLI's words, the question taken back.
    #[tokio::test]
    async fn a_cli_that_exits_while_its_terminal_question_is_open_ends_the_login() {
        let setup = terminal_setup("login-terminal-gone", TerminalScenario::GivesUp);
        let mut events = setup.state.subscribe();
        setup
            .start_flow(
                AgentId::TEST,
                &crate::adapter::test_provider::AsksOnATerminal,
                LOGIN_TIMEOUT,
            )
            .unwrap();
        until_asked(&mut events).await;
        // The CLI gives up only now, with the question open.
        std::fs::write(setup.dir.join("bin/give-up"), "").unwrap();

        let ended = until_ended(&mut events).await.pop().unwrap();
        assert_eq!(ended.status, LoginStatus::Failed);
        assert_eq!(ended.question, None);
        assert_eq!(
            ended
                .failure
                .unwrap()
                .params
                .get("detail")
                .map(String::as_str),
            Some("Gave up waiting.")
        );
        let refused = answer(&setup, TERMINAL_QUESTION, LoginAnswer::Yes).expect_err("no login");
        assert_eq!(refusal(&refused).0, error_code::LOGIN_NOT_RUNNING);
    }

    /// A prompt the terminal shows twice before the answer, as a CLI redrawing it in place or a
    /// host repainting its row does, puts one question to the user, and the one answer ends the
    /// login: the CLI is held after it has read the answer, so that it stays alive while the flow
    /// takes the next prompt line, and a second question would be broadcast before the login
    /// ends.
    #[tokio::test]
    async fn a_prompt_drawn_twice_puts_one_question_and_one_answer_ends_the_login() {
        let setup = terminal_setup("login-terminal-twice", TerminalScenario::Redraws);
        let mut events = setup.state.subscribe();
        setup
            .start_flow(
                AgentId::TEST,
                &crate::adapter::test_provider::AsksOnATerminal,
                LOGIN_TIMEOUT,
            )
            .unwrap();
        until_asked(&mut events).await;
        answer(&setup, TERMINAL_QUESTION, LoginAnswer::Yes).unwrap();

        // Every update from the answer's own on: none of them, while the CLI was held or after,
        // asks again.
        std::fs::write(setup.dir.join("bin/release"), "").unwrap();
        let updates = until_ended(&mut events).await;
        assert!(
            updates.iter().all(|login| login.question.is_none()),
            "{updates:?}"
        );
        assert_eq!(updates.last().unwrap().status, LoginStatus::Succeeded);
    }
}
