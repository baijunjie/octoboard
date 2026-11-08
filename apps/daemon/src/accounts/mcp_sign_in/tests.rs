use super::*;
use crate::protocol::Account;
use crate::test_support::{
    code_of, fake_agent, is_running, read_pid_file, ScratchDir, PATIENCE, SHELL_PID,
};

struct Setup {
    state: Arc<AppState>,
    dir: ScratchDir,
    env: HashMap<String, String>,
}

/// A daemon state with a stored Codex account `work` whose directory exists, and a fake Codex
/// running `body` (its pid file `bin/pid`, its arguments `bin/args`), all under one scratch
/// directory.
fn setup(name: &str, body: &str) -> Setup {
    let (state, dir) = crate::test_support::app_state(name);
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env = fake_agent(&bin, AgentId::CODEX.descriptor().binary, body);
    let home = dir.join("work");
    std::fs::create_dir(&home).unwrap();
    state
        .store
        .insert_account(&Account {
            id: "work".to_string(),
            agent: AgentId::CODEX,
            name: "Work".to_string(),
            config_dir: home.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        })
        .unwrap();
    Setup { state, dir, env }
}

impl Setup {
    fn shell(&self) -> ShellSource {
        let env = self.env.clone();
        Arc::new(move || Ok(env.clone()))
    }

    async fn run(
        &self,
        account: Option<&str>,
        server: &str,
        action: McpAuthAction,
        timeout: Duration,
    ) -> Result<McpServerAuthOutcome> {
        request_with(
            &self.state,
            AgentId::CODEX,
            account.map(str::to_string),
            server.to_string(),
            action,
            self.shell(),
            timeout,
        )
        .await
    }

    /// Starts a request on a task of its own, as the control socket does.
    fn spawn(
        &self,
        account: Option<&str>,
        server: &str,
        action: McpAuthAction,
        timeout: Duration,
    ) -> JoinHandle<Result<McpServerAuthOutcome>> {
        let (state, shell) = (self.state.clone(), self.shell());
        let (account, server) = (account.map(str::to_string), server.to_string());
        tokio::spawn(async move {
            request_with(
                &state,
                AgentId::CODEX,
                account,
                server,
                action,
                shell,
                timeout,
            )
            .await
        })
    }

    fn cli_file(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.join("bin").join(name)).unwrap()
    }
}

async fn until_pid(path: &Path) -> i32 {
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
    .expect("the CLI kept running");
}

fn succeeded() -> McpServerAuthOutcome {
    McpServerAuthOutcome::of(McpServerAuthStatus::Succeeded)
}

const RECORDS_ITS_LAUNCH: &str = r#"
    echo "$*" > "$HOME/args"
    echo "${CODEX_HOME-unset}" > "$HOME/home"
    exit 0
"#;

fn sleeping() -> String {
    format!("echo {SHELL_PID} > \"$HOME/pid\"; sleep 30")
}

#[tokio::test]
async fn a_non_default_account_signs_in_under_its_own_home_with_its_own_credential_store() {
    let setup = setup("mcp-auth-account", RECORDS_ITS_LAUNCH);
    let outcome = setup
        .run(Some("work"), "-odd", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap();
    assert_eq!(outcome, succeeded());
    assert_eq!(
        setup.cli_file("args").trim(),
        "mcp login -c mcp_oauth_credentials_store=\"file\" -- -odd"
    );
    assert_eq!(
        setup.cli_file("home").trim(),
        setup.dir.join("work").to_string_lossy()
    );
    assert!(setup.state.mcp_server_auths.lock().is_empty());
}

#[tokio::test]
async fn the_default_account_keeps_codex_s_default_store_and_pins_no_home() {
    let setup = setup("mcp-auth-default", RECORDS_ITS_LAUNCH);
    let outcome = setup
        .run(None, "linear", McpAuthAction::SignOut, PATIENCE)
        .await
        .unwrap();
    assert_eq!(outcome, succeeded());
    assert_eq!(setup.cli_file("args").trim(), "mcp logout -- linear");
    assert_eq!(setup.cli_file("home").trim(), "unset");
}

#[tokio::test]
async fn a_failing_cli_is_reported_with_its_whole_stderr() {
    let setup = setup(
        "mcp-auth-failed",
        "echo 'Authorize at http://x' ; printf 'Error: Metadata error: discovery failed for http://h\\n  Caused by: connection refused\\n' >&2; exit 1",
    );
    let outcome = setup
        .run(Some("work"), "s", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        McpServerAuthOutcome::failed(
            "Error: Metadata error: discovery failed for http://h\nCaused by: connection refused"
                .to_string()
        )
    );
}

#[tokio::test]
async fn without_stderr_the_failure_is_the_last_stdout_line_and_else_the_exit_status() {
    let setup = setup("mcp-auth-stdout", "echo first; echo 'last words'; exit 1");
    let outcome = setup
        .run(None, "s", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        McpServerAuthOutcome::failed("last words".to_string())
    );

    let setup = self::setup("mcp-auth-silent", "exit 3");
    let outcome = setup
        .run(None, "s", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        McpServerAuthOutcome::failed("exited with status 3".to_string())
    );
}

#[tokio::test]
async fn a_long_stderr_is_capped() {
    let setup = setup(
        "mcp-auth-capped",
        "i=0; while [ $i -lt 2000 ]; do echo 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' >&2; i=$((i+1)); done; exit 1",
    );
    let outcome = setup
        .run(None, "s", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap();
    let detail = outcome.detail.unwrap();
    assert_eq!(detail.chars().count(), DETAIL_LIMIT);
    assert!(detail.starts_with("xxxx") && detail.ends_with('…'));
}

#[tokio::test]
async fn cancelling_a_sign_in_kills_the_cli_and_answers_cancelled() {
    let setup = setup("mcp-auth-cancel", &sleeping());
    let sign_in = setup.spawn(Some("work"), "s", McpAuthAction::SignIn, PATIENCE);
    let pid = until_pid(&setup.dir.join("bin/pid")).await;

    setup
        .state
        .mcp_server_auths
        .cancel(AgentId::CODEX, Some("work".to_string()), "s")
        .unwrap();
    let outcome = sign_in.await.unwrap().unwrap();
    assert_eq!(outcome.status, McpServerAuthStatus::Cancelled);
    assert_eq!(outcome.detail, None);
    // Answered only once the CLI has gone.
    assert!(!is_running(pid));
    assert!(setup.state.mcp_server_auths.lock().is_empty());
}

#[tokio::test]
async fn cancelling_with_nothing_running_or_only_a_sign_out_is_refused() {
    let setup = setup("mcp-auth-cancel-none", &sleeping());
    let cancel = |server: &str| {
        setup
            .state
            .mcp_server_auths
            .cancel(AgentId::CODEX, Some("work".to_string()), server)
    };
    assert_eq!(
        code_of(&cancel("s").unwrap_err()),
        error_code::MCP_SIGN_IN_NOT_RUNNING
    );

    let sign_out = setup.spawn(Some("work"), "s", McpAuthAction::SignOut, PATIENCE);
    until_pid(&setup.dir.join("bin/pid")).await;
    assert_eq!(
        code_of(&cancel("s").unwrap_err()),
        error_code::MCP_SIGN_IN_NOT_RUNNING
    );
    setup
        .state
        .mcp_server_auths
        .cancel_account(AgentId::CODEX, "work");
    assert_eq!(
        sign_out.await.unwrap().unwrap().status,
        McpServerAuthStatus::Cancelled
    );
}

#[tokio::test]
async fn a_sign_in_that_outlasts_its_time_is_killed_and_answers_timed_out() {
    let setup = setup("mcp-auth-timeout", &sleeping());
    let pid_file = setup.dir.join("bin/pid");
    // A loaded machine can kill the fake CLI before it records its pid, which tests nothing: the
    // whole run is made again until it records one.
    let deadline = tokio::time::Instant::now() + PATIENCE;
    let (outcome, pid) = loop {
        let _ = std::fs::remove_file(&pid_file);
        let outcome = setup
            .run(
                Some("work"),
                "s",
                McpAuthAction::SignIn,
                Duration::from_millis(600),
            )
            .await
            .unwrap();
        if let Some(pid) = read_pid_file(&pid_file) {
            break (outcome, pid);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the fake CLI never wrote its pid"
        );
    };
    assert_eq!(
        outcome,
        McpServerAuthOutcome::of(McpServerAuthStatus::TimedOut)
    );
    until_gone(pid).await;
    assert!(setup.state.mcp_server_auths.lock().is_empty());
}

#[tokio::test]
async fn a_second_request_for_the_same_action_joins_the_run_and_gets_its_outcome() {
    let setup = setup(
        "mcp-auth-join",
        &format!("echo x >> \"$HOME/runs\"; echo {SHELL_PID} > \"$HOME/pid\"; sleep 1; echo late >&2; exit 1"),
    );
    let first = setup.spawn(Some("work"), "s", McpAuthAction::SignIn, PATIENCE);
    until_pid(&setup.dir.join("bin/pid")).await;
    let second = setup.spawn(Some("work"), "s", McpAuthAction::SignIn, PATIENCE);
    let (first, second) = (
        first.await.unwrap().unwrap(),
        second.await.unwrap().unwrap(),
    );
    assert_eq!(first, McpServerAuthOutcome::failed("late".to_string()));
    assert_eq!(first, second);
    assert_eq!(setup.cli_file("runs").lines().count(), 1);

    // Once it is over the next request runs again, and another server is a run of its own.
    setup
        .run(Some("work"), "s", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap();
    assert_eq!(setup.cli_file("runs").lines().count(), 2);
}

#[tokio::test]
async fn the_other_action_for_the_same_server_is_refused_busy_and_another_server_is_not() {
    let setup = setup("mcp-auth-busy", &sleeping());
    let sign_in = setup.spawn(Some("work"), "s", McpAuthAction::SignIn, PATIENCE);
    until_pid(&setup.dir.join("bin/pid")).await;

    let refused = setup
        .run(Some("work"), "s", McpAuthAction::SignOut, PATIENCE)
        .await
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::MCP_SERVER_AUTH_BUSY);
    let other = setup.spawn(Some("work"), "t", McpAuthAction::SignOut, PATIENCE);
    let other_default = setup.spawn(None, "s", McpAuthAction::SignOut, PATIENCE);
    tokio::time::timeout(PATIENCE, async {
        while setup.state.mcp_server_auths.lock().len() < 3 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("three runs of their own");

    setup.state.mcp_server_auths.stop_all().await;
    for run in [sign_in, other, other_default] {
        let _ = run.await;
    }
}

#[tokio::test]
async fn an_agent_without_the_commands_a_missing_binary_and_a_failed_shell_are_refused_first() {
    let setup = setup("mcp-auth-refused", "exit 0");
    let grok = request_with(
        &setup.state,
        AgentId::GROK,
        None,
        "s".to_string(),
        McpAuthAction::SignIn,
        setup.shell(),
        PATIENCE,
    )
    .await
    .unwrap_err();
    let coded = grok.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.code, error_code::PROVIDER_UNSUPPORTED);
    assert_eq!(coded.params["operation"], "mcp_sign_in");

    let none = HashMap::from([("PATH".to_string(), setup.dir.display().to_string())]);
    let missing = request_with(
        &setup.state,
        AgentId::CODEX,
        None,
        "s".to_string(),
        McpAuthAction::SignIn,
        Arc::new(move || Ok(none.clone())),
        PATIENCE,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&missing), error_code::BINARY_NOT_FOUND);

    let no_shell = request_with(
        &setup.state,
        AgentId::CODEX,
        None,
        "s".to_string(),
        McpAuthAction::SignIn,
        Arc::new(|| anyhow::bail!("no shell")),
        PATIENCE,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&no_shell), error_code::SHELL_ENVIRONMENT_TIMEOUT);
    assert!(setup.state.mcp_server_auths.lock().is_empty());
}

#[tokio::test]
async fn a_missing_account_directory_fails_and_nothing_is_run() {
    let setup = setup("mcp-auth-dir-missing", RECORDS_ITS_LAUNCH);
    std::fs::remove_dir(setup.dir.join("work")).unwrap();
    let outcome = setup
        .run(Some("work"), "s", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap();
    assert_eq!(outcome.status, McpServerAuthStatus::Failed);
    assert!(outcome.detail.unwrap().contains("does not exist"));
    assert!(!setup.dir.join("bin/args").exists());
}

#[tokio::test]
async fn removing_an_account_ends_its_runs_first_and_refuses_new_ones() {
    let setup = setup("mcp-auth-removal", &sleeping());
    let sign_in = setup.spawn(Some("work"), "s", McpAuthAction::SignIn, PATIENCE);
    let pid = until_pid(&setup.dir.join("bin/pid")).await;

    assert!(
        setup
            .state
            .mcp_server_auths
            .end_account(AgentId::CODEX, "work", PATIENCE)
            .await
    );
    assert_eq!(
        sign_in.await.unwrap().unwrap().status,
        McpServerAuthStatus::Cancelled
    );
    assert!(!is_running(pid));

    let account = setup.state.store.get_account("work").unwrap().unwrap();
    let _removal = setup.state.begin_account_removal(&account);
    let refused = setup
        .run(Some("work"), "s", McpAuthAction::SignIn, PATIENCE)
        .await
        .unwrap_err();
    assert_eq!(code_of(&refused), error_code::ACCOUNT_BEING_REMOVED);
}

#[tokio::test]
async fn the_daemon_s_exit_kills_the_running_clis() {
    let setup = setup("mcp-auth-stop-all", &sleeping());
    let sign_in = setup.spawn(None, "s", McpAuthAction::SignIn, PATIENCE);
    let pid = until_pid(&setup.dir.join("bin/pid")).await;
    setup.state.mcp_server_auths.stop_all().await;
    until_gone(pid).await;
    let _ = sign_in.await;
    assert!(setup.state.mcp_server_auths.lock().is_empty());
}
