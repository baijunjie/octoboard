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
        &format!("echo {SHELL_PID} > \"$HOME/pid\"; sleep 30"),
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
        r#"sleep 10; echo '{"loggedIn":true}'"#,
    );
    let setup = setup("login-cancel-late", AgentId::CLAUDE, &body);
    let mut events = setup.state.subscribe();
    setup.start(AgentId::CLAUDE, None, LOGIN_TIMEOUT).unwrap();
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
        &format!("echo {SHELL_PID} > \"$HOME/pid\"; sleep 30"),
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
