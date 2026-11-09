use super::*;
use crate::protocol::{Account, AccountQuota, Event, QuotaWindow, QuotaWindowKind, Role};
use crate::test_support::{
    app_state, console, insert_project, output_containing, running, stand_in_launch, Launched,
    ScratchDir, StandIn,
};
use crate::time::now_millis;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

type Launches = Arc<Mutex<Vec<Launched>>>;
type Started = Arc<Mutex<Vec<StandIn>>>;

const CONVERSATION: &str = "projects/slug/a0000000-0000-4000-8000-000000000001.jsonl";

/// A project session `w` of Claude Code, owned by the console session `owner` (which has a
/// process that takes messages), with its conversation in the default account's directory,
/// ending on a usage limit, and a process watched for its end; two accounts, `claude-a`
/// ("Work A") and `claude-b`; and the setting that switches sessions automatically on. The
/// default account's weekly quota has 60% left and `claude-a`'s 80%; `claude-b`, which the
/// owner is on, has not been read.
struct Fixture {
    state: Arc<AppState>,
    dir: ScratchDir,
    owner: StandIn,
    worker_process: StandIn,
    launches: Launches,
    started: Started,
}

impl Fixture {
    fn new(label: &str, status: SessionStatus) -> Self {
        Self::with_worker_process(label, status, |state, _| {
            let worker = running(state, "w");
            crate::coordinator::exit_watch::watch_exit(state, worker.clone());
            worker
        })
    }

    /// [`Self::new`] with the worker's process made by `worker_process`, given the state and the
    /// scratch directory.
    fn with_worker_process(
        label: &str,
        status: SessionStatus,
        worker_process: impl FnOnce(&Arc<AppState>, &std::path::Path) -> StandIn,
    ) -> Self {
        let (state, dir) = app_state(label);
        state
            .store
            .insert_console(&crate::protocol::Console {
                workdir: dir.to_string_lossy().into_owned(),
                ..console(&dir)
            })
            .unwrap();
        let project_dir = dir.join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        insert_project(&state, &project_dir);
        state
            .store
            .insert_session(&crate::test_support::console_session(
                "owner",
                SessionStatus::Idle,
            ))
            .unwrap();
        state
            .store
            .insert_session(&Session {
                project_id: Some("p1".to_string()),
                bound_to: Some("owner".to_string()),
                role: Role::Worker,
                has_conversation: true,
                agent_session_id: Some("a0000000-0000-4000-8000-000000000001".to_string()),
                ..crate::test_support::project_session("w", AgentId::CLAUDE, status)
            })
            .unwrap();
        for (id, name) in [("claude-a", "Work A"), ("claude-b", "Work B")] {
            state
                .store
                .insert_account(&Account {
                    id: id.to_string(),
                    agent: AgentId::CLAUDE,
                    name: name.to_string(),
                    config_dir: dir.join(id).to_string_lossy().into_owned(),
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled: false,
                    versions: Default::default(),
                })
                .unwrap();
        }
        std::fs::create_dir_all(
            dir.join("default-claude")
                .join(CONVERSATION)
                .parent()
                .unwrap(),
        )
        .unwrap();
        state.store.set_auto_switch_on_usage_limit(true).unwrap();
        // Not on the account the worker is on, or the owner would be a candidate as well.
        state
            .set_session_account("owner", Some("claude-b"), dir.join("claude-b").to_str())
            .unwrap();
        let fixture = Self {
            owner: running(&state, "owner"),
            worker_process: worker_process(&state, &dir),
            state,
            dir,
            launches: Default::default(),
            started: Default::default(),
        };
        fixture.hold(None, 40.0);
        fixture.hold(Some("claude-a"), 20.0);
        fixture.end_on_usage_limit();
        fixture
    }

    /// Holds a reading of `account` with a weekly window `used` percent used, resetting
    /// tomorrow.
    fn hold(&self, account: Option<&str>, used: f64) {
        let window = QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent: used,
            resets_at: Some(now_millis() + 86_400_000),
            window_minutes: None,
        };
        self.hold_reading(account, vec![window], None);
    }

    fn hold_reading(
        &self,
        account: Option<&str>,
        windows: Vec<QuotaWindow>,
        unavailable: Option<crate::protocol::QuotaUnavailable>,
    ) {
        let quota = AccountQuota {
            reset_credits: None,
            agent: AgentId::CLAUDE,
            account: account.map(str::to_string),
            fetched_at: 1,
            plan: None,
            email: None,
            windows,
            unavailable,
        };
        match account {
            None => self.state.account_quotas.hold_for_test(&quota),
            Some(id) => self
                .state
                .account_quotas
                .hold_in_dir_for_test(&quota, self.dir.join(id).to_str().unwrap()),
        }
    }

    /// Puts a dialog of Claude Code's on the worker's screen, which shows it as waiting for
    /// the user without any hook saying so.
    fn show_dialog(&self) {
        self.worker_process.on_output(
            "\n  Resume this conversation?\n  This conversation has been inactive for 1h and \
             is 42k tokens long.\n  \u{276f} Resume\n    Start a new conversation\n\n  \
             Enter to confirm \u{b7} Esc to dismiss\n"
                .as_bytes(),
        );
        assert!(self.worker_process.look_for_dialog().current.is_some());
    }

    /// Puts Claude Code's view of a background subagent on the worker's screen, which shows
    /// it as dormant: not at a prompt, but no question waits for the user either.
    fn show_agents_view(&self) {
        crate::test_support::show_agents_view(&self.worker_process);
    }

    fn clear_screen(&self) {
        self.worker_process.on_output(b"\x1b[2J\x1b[H");
        assert!(self.worker_process.look_for_dialog().current.is_none());
    }

    fn worker(&self) -> Session {
        self.state.session_record("w").unwrap()
    }

    fn env(&self) -> HashMap<String, String> {
        HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_string(),
            self.dir
                .join("default-claude")
                .to_string_lossy()
                .into_owned(),
        )])
    }

    /// The end of a turn that failed, run as the daemon runs it, with a relaunch that starts
    /// a stand-in: whether the switch moved the session, or resumed it on its own account.
    async fn failed_turn(&self) -> bool {
        let (started, launches) = (self.started.clone(), self.launches.clone());
        let env = self.env();
        switch_after_failed_turn(
            &self.state,
            "w",
            move || Ok(env),
            move |request| stand_in_launch(&started, &launches, || {})(request),
        )
        .await
    }

    /// A relaunch that fails its first `failures` calls and starts a stand-in on the rest.
    fn launch_failing(
        &self,
        failures: usize,
    ) -> impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static {
        let (started, launches) = (self.started.clone(), self.launches.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        move |request| {
            if calls.fetch_add(1, Ordering::SeqCst) < failures {
                anyhow::bail!("the agent cannot be started");
            }
            stand_in_launch(&started, &launches, || {})(request)
        }
    }

    fn launched(&self) -> usize {
        self.launches.lock().unwrap().len()
    }

    /// The prompt the `n`th relaunch was given.
    fn prompt_of(&self, n: usize) -> Option<String> {
        self.launches.lock().unwrap()[n].2.clone()
    }

    /// Makes the conversation end on the plan's usage limit, as Claude Code writes it.
    fn end_on_usage_limit(&self) {
        self.write_conversation(concat!(
            r#"{"type":"user","message":{"role":"user","content":"hello"}}"#,
            "\n",
            r#"{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","#,
            r#""apiError":"usage_limit_reached","message":{"role":"assistant","#,
            r#""stop_reason":"stop_sequence","content":[{"type":"text","text":"limit"}]}}"#,
            "\n"
        ));
    }

    /// Makes the conversation end on a tool call that never got its result, as a turn that is
    /// running does.
    fn end_on_a_tool_call(&self) {
        self.write_conversation(concat!(
            r#"{"type":"user","message":{"role":"user","content":"run it"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","#,
            r#""content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}"#,
            "\n"
        ));
    }

    fn write_conversation(&self, text: &str) {
        std::fs::write(self.dir.join("default-claude").join(CONVERSATION), text).unwrap();
    }
}

impl Fixture {
    async fn waiting_for_quota(&self) {
        self.hold(None, 100.0);
        self.hold(Some("claude-a"), 100.0);
        self.failed_turn().await;
        assert!(self
            .state
            .quota_recovery
            .inner
            .lock()
            .unwrap()
            .waiting
            .contains_key("w"));
        assert_eq!(self.launched(), 0);
        self.install_recovery_launch().await;
    }

    /// Gives the recovery a relaunch that starts a stand-in, and returns once the millisecond
    /// clock has moved past the wait: a probe that started in the same millisecond as the wait is
    /// not evidence of recovery.
    async fn install_recovery_launch(&self) {
        let (started, launches) = (self.started.clone(), self.launches.clone());
        *self.state.quota_recovery.launch.lock().unwrap() = Some(RecoveryLaunch {
            env: self.env(),
            launch: Arc::new(move |request| stand_in_launch(&started, &launches, || {})(request)),
        });
        crate::test_support::next_millisecond().await;
    }

    /// Whether the worker's trigger is parked, and the token it was parked under.
    fn parked(&self) -> Option<(Arc<()>, Option<i64>)> {
        self.state
            .quota_recovery
            .inner
            .lock()
            .unwrap()
            .waiting
            .get("w")
            .map(|wait| (wait.token.clone(), wait.hold_until))
    }

    fn refresh_quota(&self, account: Option<&str>, used: f64) -> tokio::task::JoinHandle<()> {
        let quota = AccountQuota {
            reset_credits: None,
            agent: AgentId::CLAUDE,
            account: account.map(str::to_string),
            fetched_at: now_millis(),
            plan: None,
            email: None,
            unavailable: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: used,
                resets_at: Some(now_millis() + 86_400_000),
                window_minutes: None,
            }],
        };
        crate::accounts::quotas::finish_probe_for_test(
            &self.state,
            quota,
            account
                .map(|id| self.dir.join(id))
                .as_deref()
                .and_then(|path| path.to_str()),
        )
        .unwrap()
    }

    async fn recovered(&self) {
        let launches = self.launches.clone();
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, move || launches
                .lock()
                .unwrap()
                .len()
                == 1)
            .await
        );
        let state = self.state.clone();
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, move || !state
                .is_launching("w"))
            .await
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_successful_probe_recovers_a_waiting_turn_on_another_account_once() {
    let fixture = Fixture::new("quota-recovery-other", SessionStatus::Idle);
    fixture.waiting_for_quota().await;
    // Even a recently exhausted account can be used after a verified fresh reading.
    fixture.state.note_left_for_limit("w", Some("claude-a"));
    let first = fixture.refresh_quota(Some("claude-a"), 20.0);
    let second = fixture.refresh_quota(Some("claude-a"), 20.0);
    first.await.unwrap();
    second.await.unwrap();
    fixture.refresh_quota(None, 10.0).await.unwrap();
    fixture.recovered().await;
    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
    assert_eq!(fixture.launched(), 1);
    assert!(fixture
        .prompt_of(0)
        .unwrap()
        .ends_with(crate::messaging::texts::CONTINUE_AFTER_NOTICE));
}

#[tokio::test]
async fn a_successful_probe_can_resume_the_account_the_turn_was_on() {
    let fixture = Fixture::new("quota-recovery-own", SessionStatus::Idle);
    fixture.waiting_for_quota().await;
    let mut events = fixture.state.subscribe();
    fixture.refresh_quota(None, 5.0).await.unwrap();
    fixture.recovered().await;
    assert_eq!(fixture.worker().account_id, None);
    assert!(fixture.prompt_of(0).unwrap().starts_with(&format!(
        "{NOTICE_PREFIX}: your account's usage quota has recovered"
    )));
    assert!(notices(&mut events, notice_code::ACCOUNT_SWITCHED_AUTOMATICALLY).is_empty());
    let heard = output_containing(&fixture.owner, "resumed automatically");
    assert!(heard.contains("resumed automatically"), "{heard}");
}

#[tokio::test]
async fn manual_input_or_a_new_turn_cancels_quota_recovery() {
    for action in [
        "input",
        "turn",
        "interrupt",
        "model",
        "archive",
        "setting",
        "process",
        "exit",
    ] {
        let fixture = Fixture::new(
            &format!("quota-recovery-cancel-{action}"),
            SessionStatus::Idle,
        );
        fixture.waiting_for_quota().await;
        match action {
            "input" => fixture.worker_process.write_input(b"new task").unwrap(),
            "turn" => fixture.state.turn_started("w", Some("new task")),
            "interrupt" => {
                let _claim = fixture.state.claim_interrupt("w");
            }
            "model" => fixture
                .state
                .set_session_launch_overrides("w", &None, &None)
                .unwrap(),
            "archive" => {
                crate::coordinator::lifecycle::archive_session(&fixture.state, "w").unwrap();
            }
            "setting" => {
                fixture
                    .state
                    .store
                    .set_auto_switch_on_usage_limit(false)
                    .unwrap();
            }
            "process" => crate::coordinator::exit_watch::end_process_for_test(&fixture.state, "w"),
            "exit" => {
                fixture.worker_process.terminate();
                let registered = fixture.state.live_session("w").expect("still registered");
                assert!(Arc::ptr_eq(&registered, &fixture.worker_process));
                assert!(fixture.worker_process.poll_exit());
            }
            _ => unreachable!(),
        }
        fixture.refresh_quota(Some("claude-a"), 20.0).await.unwrap();
        assert_eq!(fixture.launched(), 0, "{action}");
        assert!(!fixture
            .state
            .quota_recovery
            .inner
            .lock()
            .unwrap()
            .waiting
            .contains_key("w"));
    }
}

#[tokio::test]
async fn quota_refresh_never_resumes_an_unrelated_idle_session() {
    let fixture = Fixture::new("quota-recovery-unrelated", SessionStatus::Idle);
    let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
    *fixture.state.quota_recovery.launch.lock().unwrap() = Some(RecoveryLaunch {
        env: fixture.env(),
        launch: Arc::new(move |request| stand_in_launch(&started, &launches, || {})(request)),
    });
    fixture.refresh_quota(Some("claude-a"), 0.0).await.unwrap();
    assert_eq!(fixture.launched(), 0);
}

#[tokio::test]
async fn failed_stale_or_still_exhausted_probes_keep_waiting() {
    let fixture = Fixture::new("quota-recovery-no-proof", SessionStatus::Idle);
    fixture.waiting_for_quota().await;
    let mut quota = fixture
        .state
        .account_quotas
        .held(
            AgentId::CLAUDE,
            Some("claude-a"),
            fixture.dir.join("claude-a").to_str(),
        )
        .unwrap();
    quota.windows[0].resets_at = Some(now_millis() - 1000);
    crate::accounts::quotas::finish_probe_for_test(
        &fixture.state,
        quota.clone(),
        fixture.dir.join("claude-a").to_str(),
    )
    .unwrap()
    .await
    .unwrap();
    quota.windows[0].used_percent = 0.0;
    quota.unavailable = Some(crate::protocol::QuotaUnavailable::new(
        crate::protocol::quota_unavailable_code::TIMED_OUT,
        "no",
        &[],
    ));
    crate::accounts::quotas::finish_probe_for_test(
        &fixture.state,
        quota,
        fixture.dir.join("claude-a").to_str(),
    )
    .unwrap()
    .await
    .unwrap();
    assert_eq!(fixture.launched(), 0);
    assert!(fixture
        .state
        .quota_recovery
        .inner
        .lock()
        .unwrap()
        .waiting
        .contains_key("w"));
    fixture.refresh_quota(Some("claude-a"), 20.0).await.unwrap();
    fixture.recovered().await;
}

#[tokio::test]
async fn input_before_an_async_failure_trigger_is_judged_prevents_registering_a_wait() {
    let fixture = Fixture::new("quota-recovery-stale-trigger", SessionStatus::Idle);
    fixture.hold(Some("claude-a"), 100.0);
    let trigger = fixture
        .state
        .quota_recovery
        .capture(&fixture.state, "w")
        .unwrap();
    fixture.worker_process.write_input(b"user task").unwrap();
    let env = fixture.env();
    switch_after_failed_turn_captured(
        &fixture.state,
        "w",
        trigger,
        move || Ok(env),
        fixture.launch_failing(0),
    )
    .await;
    assert!(!fixture
        .state
        .quota_recovery
        .inner
        .lock()
        .unwrap()
        .waiting
        .contains_key("w"));
    assert_eq!(fixture.launched(), 0);
}

#[tokio::test]
async fn a_probe_started_before_the_wait_or_with_another_exhausted_window_cannot_resume_it() {
    let fixture = Fixture::new("quota-recovery-all-windows", SessionStatus::Idle);
    fixture.waiting_for_quota().await;
    let since = fixture.state.quota_recovery.inner.lock().unwrap().waiting["w"].since;
    fixture.hold(Some("claude-a"), 0.0);
    after_quota_refresh(
        &fixture.state,
        AgentId::CLAUDE,
        Some("claude-a".to_string()),
        Some(fixture.dir.join("claude-a").to_string_lossy().into_owned()),
        since - 1,
    )
    .unwrap()
    .await
    .unwrap();
    assert_eq!(fixture.launched(), 0);
    let mut quota = fixture
        .state
        .account_quotas
        .held(
            AgentId::CLAUDE,
            Some("claude-a"),
            fixture.dir.join("claude-a").to_str(),
        )
        .unwrap();
    quota.windows.push(QuotaWindow {
        kind: QuotaWindowKind::Session,
        label: None,
        used_percent: 100.0,
        resets_at: Some(now_millis() + 1000),
        window_minutes: None,
    });
    crate::accounts::quotas::finish_probe_for_test(
        &fixture.state,
        quota,
        fixture.dir.join("claude-a").to_str(),
    )
    .unwrap()
    .await
    .unwrap();
    assert_eq!(fixture.launched(), 0);
    assert!(fixture
        .state
        .quota_recovery
        .inner
        .lock()
        .unwrap()
        .waiting
        .contains_key("w"));
    fixture.refresh_quota(Some("claude-a"), 20.0).await.unwrap();
    fixture.recovered().await;
}

#[tokio::test]
async fn a_models_window_alone_is_not_evidence_the_account_recovered() {
    let fixture = Fixture::new("quota-recovery-model-only", SessionStatus::Idle);
    fixture.waiting_for_quota().await;
    let quota = AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: Some("claude-a".to_string()),
        fetched_at: now_millis(),
        plan: None,
        email: None,
        unavailable: None,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::WeeklyModel,
            label: Some("Fable".to_string()),
            used_percent: 0.0,
            resets_at: Some(now_millis() + 86_400_000),
            window_minutes: None,
        }],
    };
    crate::accounts::quotas::finish_probe_for_test(
        &fixture.state,
        quota,
        fixture.dir.join("claude-a").to_str(),
    )
    .unwrap()
    .await
    .unwrap();
    assert_eq!(fixture.launched(), 0);
    assert!(fixture
        .state
        .quota_recovery
        .inner
        .lock()
        .unwrap()
        .waiting
        .contains_key("w"));
    fixture.refresh_quota(Some("claude-a"), 20.0).await.unwrap();
    fixture.recovered().await;
}

#[tokio::test]
async fn a_wait_for_the_user_that_appears_during_preparation_prevents_recovery() {
    for screen in [false, true] {
        let fixture = Fixture::new(
            &format!("quota-recovery-late-wait-{screen}"),
            SessionStatus::Idle,
        );
        fixture.waiting_for_quota().await;
        let mut wait = fixture.state.quota_recovery.inner.lock().unwrap().waiting["w"].clone();
        fixture.hold(Some("claude-a"), 20.0);
        wait.verified = Some((
            Some("claude-a".to_string()),
            Some(fixture.dir.join("claude-a").to_string_lossy().into_owned()),
            wait.since,
        ));
        let (state, live, env) = (
            fixture.state.clone(),
            fixture.state.live_session("w").unwrap(),
            fixture.env(),
        );
        switch_to_with_recovery(&fixture.state, &fixture.worker(), Some("claude-a".to_string()),
            move || {
                if screen {
                    live.on_output("\n  Resume this conversation?\n  This conversation has been inactive for 1h and is 42k tokens long.\n  ❯ Resume\n    Start a new conversation\n\n  Enter to confirm · Esc to dismiss\n".as_bytes());
                    assert!(live.look_for_dialog().current.is_some());
                } else {
                    state.apply_hook_status("w", SessionStatus::WaitingUser).unwrap();
                }
                Ok(env)
            }, fixture.launch_failing(0), Some(&wait)).await.unwrap();
        assert_eq!(fixture.launched(), 0);
        assert!(fixture.state.live_session("w").is_some());
        assert!(!fixture.worker_process.was_ended_on_purpose());
    }
}

#[tokio::test]
async fn a_failed_recovery_on_the_same_account_reports_a_launch_failure() {
    let fixture = Fixture::new("quota-recovery-own-failure", SessionStatus::Idle);
    fixture.waiting_for_quota().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = attempts.clone();
    fixture
        .state
        .quota_recovery
        .launch
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .launch = Arc::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        anyhow::bail!("the agent cannot be started")
    });
    let mut events = fixture.state.subscribe();
    let mut failures = fixture.state.subscribe();
    fixture.refresh_quota(None, 0.0).await.unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.worker().status, SessionStatus::Interrupted);
    assert!(fixture.state.live_session("w").is_none());
    let told = notices(&mut events, notice_code::LAUNCH_FAILED);
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].1["title"], fixture.worker().title);
    assert!(told[0].1["reason"].contains("the agent cannot be started"));
    assert!(told[0].1.contains_key("reason_code"));
    assert!(notices(
        &mut failures,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    let heard = output_containing(&fixture.owner, "could not resume after");
    assert!(
        heard.contains("interrupted and its work is stopped"),
        "{heard}"
    );
    assert!(!heard.contains("could neither switch"), "{heard}");
    assert!(!heard.contains("switched it to"), "{heard}");
}

/// The `session_notice` events of `code` among those received, with the session each is
/// about.
fn notices(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    code: crate::protocol::Code,
) -> Vec<(String, crate::protocol::Params)> {
    let mut found = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::SessionNotice {
            session,
            code: sent,
            params,
            ..
        } = event
        {
            if sent == code {
                found.push((session, params));
            }
        }
    }
    found
}

/// What the tracing of the test body wrote, for asserting that nothing was logged.
#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
    type Writer = LogBuffer;

    fn make_writer(&'a self) -> LogBuffer {
        self.clone()
    }
}

impl LogBuffer {
    /// Captures what the current thread logs until the guard drops; a current-thread test
    /// runtime runs the code under test on it.
    fn capture() -> (Self, tracing::subscriber::DefaultGuard) {
        let logs = Self::default();
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(logs.clone())
                .with_ansi(false)
                .finish(),
        );
        (logs, guard)
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// What the owner has been written once a marker has reached it.
async fn owner_heard(fixture: &Fixture) -> String {
    let (state, owner) = (fixture.state.clone(), (*fixture.owner).clone());
    tokio::task::spawn_blocking(move || crate::test_support::heard_after_marker(&state, &owner))
        .await
        .unwrap()
}

/// A session whose turn stopped on the usage limit is switched to another account whatever
/// its quota reading says of its own, working or idle: it resumes its conversation there with
/// one prompt that tells it of the switch and carries on, and the user and its owner are told.
#[tokio::test]
async fn a_session_whose_turn_stopped_on_the_usage_limit_is_switched_and_everyone_told() {
    for (label, status) in [
        ("auto-switch-idle", SessionStatus::Idle),
        ("auto-switch-working", SessionStatus::Working),
    ] {
        let fixture = Fixture::new(label, status);
        let mut events = fixture.state.subscribe();

        assert!(fixture.failed_turn().await, "moved");

        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
        assert_eq!(fixture.launched(), 1);
        let prompt = fixture.prompt_of(0).expect("a prompt to carry on");
        assert_eq!(
            prompt,
            "Message from Octoboard: your Claude Code account `Default` reached its usage \
             limit, so Octoboard switched you to `Work A`.\n\nContinue the task you were \
             working on when it was interrupted; do not repeat work that is already complete."
        );
        let owner = output_containing(&fixture.owner, "switched it to");
        assert!(
            owner.contains("`w`") && owner.contains("switched it to `Work A` by itself"),
            "{owner}"
        );
        assert!(owner.contains("do not send it one yourself"), "{owner}");
        let told = notices(&mut events, notice_code::ACCOUNT_SWITCHED_AUTOMATICALLY);
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].0, "w");
        assert_eq!(told[0].1["from"], "Default");
        assert_eq!(told[0].1["account"], "Work A");
        assert_eq!(told[0].1["account_id"], "claude-a");
        assert_eq!(told[0].1["session"], "w");
        assert!(!told[0].1.contains_key("from_id"));
    }
}

/// A restart that continues the turn the usage limit cut off tells the session, in the prompt that
/// carries it on, after the notice of the switch and before the words to carry on, which trees of
/// those it had running the restart ended, and nothing of those that kept running.
#[tokio::test(flavor = "multi_thread")]
async fn the_prompt_that_carries_a_switched_session_on_lists_the_trees_the_restart_ended() {
    let mut children = Vec::new();
    let fixture = Fixture::with_worker_process(
        "auto-switch-ended-trees",
        SessionStatus::Idle,
        |state, dir| {
            let (worker, survivor, doomed) =
                crate::test_support::running_leaving_two(state, dir, "w");
            children = vec![survivor, doomed];
            worker
        },
    );
    let _guard = crate::test_support::KillOnDrop(children.clone());

    assert!(fixture.failed_turn().await, "moved");

    let prompt = fixture.prompt_of(0).expect("a prompt to carry on");
    let (notice, rest) = prompt.split_once("\n\n").expect("the notice comes first");
    assert!(notice.contains("switched you to `Work A`"), "{prompt}");
    let (ended, words) = rest.split_once("\n\n").expect("the words come last");
    assert!(
        ended.starts_with(&format!(
            "The restart ended these processes you had started: `{}",
            crate::test_support::doomed_command_starts()
        )),
        "{prompt}"
    );
    assert!(
        ended.ends_with("Start again any you still need."),
        "{prompt}"
    );
    assert!(
        !ended.contains(&crate::test_support::survivor_command_starts()),
        "{prompt}"
    );
    assert_eq!(words, crate::messaging::texts::CONTINUE_AFTER_NOTICE);
    assert!(crate::test_support::is_running(children[0]), "kept running");
}

/// What the switch ended of what the session had started goes into the one message of the switch
/// that does not continue, after the note and before the words to carry on.
#[tokio::test]
async fn a_switch_that_does_not_continue_folds_what_it_ended_into_its_one_message() {
    let fixture = Fixture::new("auto-switch-note-ended", SessionStatus::Idle);
    let worker = fixture.worker();
    let (state, note) = (
        fixture.state.clone(),
        note_to_session(AgentId::CLAUDE, "A", "B"),
    );
    let ended = "The restart ended these processes you had started: `serve` in `/w`. Start again \
                 any you still need.";
    tokio::task::spawn_blocking(move || {
        tell_sessions(&state, &worker, "A", "B", &note, false, Some(ended))
    })
    .await
    .unwrap();

    let heard = output_containing(&fixture.worker_process, "Carry on with what");
    let one = heard
        .split("Message from Octoboard")
        .filter(|message| message.contains("switched you to `B`"))
        .collect::<Vec<_>>();
    assert_eq!(one.len(), 1, "{heard}");
    let at = |needle: &str| {
        one[0]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle}: {heard}"))
    };
    assert!(at("switched you to `B`.") < at("The restart ended"));
    assert!(at("The restart ended") < at("Carry on with what"));
}

/// Holds a used-up weekly window of the default account, the worker's, that resets in
/// `minutes`.
fn hold_used_up_resetting_in(fixture: &Fixture, minutes: i64) {
    let window = QuotaWindow {
        kind: QuotaWindowKind::Weekly,
        label: None,
        used_percent: 100.0,
        resets_at: Some(now_millis() + minutes * 60_000),
        window_minutes: None,
    };
    fixture.hold_reading(None, vec![window], None);
}

/// The window that blocks the session's account resetting within minutes is waited out: the
/// session stays put and its trigger is parked for the recovery after the reset. A later reset
/// is not worth waiting for, and the session moves.
#[tokio::test]
async fn a_window_about_to_reset_is_waited_out_instead_of_switching() {
    let soon = Fixture::new("auto-switch-hold-soon", SessionStatus::Idle);
    hold_used_up_resetting_in(&soon, 4);

    assert!(!soon.failed_turn().await, "held, not moved");

    assert_eq!(soon.worker().account_id, None);
    assert_eq!(soon.launched(), 0);
    assert!(soon
        .state
        .quota_recovery
        .inner
        .lock()
        .unwrap()
        .waiting
        .contains_key("w"));

    let later = Fixture::new("auto-switch-hold-later", SessionStatus::Idle);
    hold_used_up_resetting_in(&later, 10);

    assert!(later.failed_turn().await, "moved");

    assert_eq!(later.worker().account_id.as_deref(), Some("claude-a"));
    assert_eq!(later.launched(), 1);
}

/// The window that blocks the account being the worker's own, held to resume it: another
/// account's successful reading in the meantime does not move the session, the own account's
/// reading after the reset resumes it where it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_session_resumes_only_on_its_own_account() {
    let fixture = Fixture::new("auto-switch-hold-own", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    assert!(fixture.parked().unwrap().1.is_some());
    fixture.install_recovery_launch().await;

    fixture.refresh_quota(Some("claude-a"), 20.0).await.unwrap();

    assert_eq!(fixture.worker().account_id, None);
    assert_eq!(fixture.launched(), 0);
    assert!(fixture.parked().is_some());

    fixture.refresh_quota(None, 10.0).await.unwrap();
    fixture.recovered().await;

    assert_eq!(fixture.worker().account_id, None);
    assert!(fixture
        .prompt_of(0)
        .unwrap()
        .contains("usage quota has recovered"));
}

/// When the hold ends without the own account having recovered, the session is judged as any
/// other: moved if an account qualifies, else left waiting for any account's recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_end_of_a_hold_judges_the_session_as_usual() {
    let fixture = Fixture::new("auto-switch-hold-end", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    fixture.install_recovery_launch().await;
    let (token, until) = fixture.parked().unwrap();

    end_hold(&fixture.state, "w", &token, until.unwrap()).await;

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
    assert_eq!(fixture.launched(), 1);

    let stuck = Fixture::new("auto-switch-hold-end-none", SessionStatus::Idle);
    hold_used_up_resetting_in(&stuck, 4);
    stuck.hold(Some("claude-a"), 100.0);
    stuck.failed_turn().await;
    stuck.install_recovery_launch().await;
    let (token, until) = stuck.parked().unwrap();

    end_hold(&stuck.state, "w", &token, until.unwrap()).await;

    assert_eq!(stuck.launched(), 0);
    let (_, hold_until) = stuck.parked().expect("left waiting");
    assert!(hold_until.is_some_and(|until| until <= now_millis()));
    // Any account's reading may resume it now.
    stuck.refresh_quota(Some("claude-a"), 20.0).await.unwrap();
    stuck.recovered().await;
    assert_eq!(stuck.worker().account_id.as_deref(), Some("claude-a"));
}

/// The own account's reading may land while the end of a hold is under way: the session then
/// continues on its own account, not on another one.
#[tokio::test]
async fn the_end_of_a_hold_continues_on_the_own_account_when_it_has_recovered() {
    let fixture = Fixture::new("auto-switch-hold-end-recovered", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    fixture.install_recovery_launch().await;
    let (token, until) = fixture.parked().unwrap();
    fixture.hold(None, 20.0);

    end_hold(&fixture.state, "w", &token, until.unwrap()).await;

    assert_eq!(fixture.worker().account_id, None);
    assert_eq!(fixture.launched(), 1);
    assert!(fixture.parked().is_none());
}

/// A recovered reading whose callback finds the hold already claimed is still used when it
/// arrives during asynchronous switch preparation, before the original process is stopped.
#[tokio::test]
async fn a_held_account_recovering_during_switch_preparation_resumes_on_itself() {
    let fixture = Fixture::new(
        "auto-switch-recovered-during-preparation",
        SessionStatus::Idle,
    );
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    fixture.install_recovery_launch().await;
    let waiting = fixture.state.quota_recovery.inner.lock().unwrap().waiting["w"].clone();
    assert!(claim(&fixture.state, &waiting));
    let trigger = PendingRecovery {
        hold_until: Some(now_millis()),
        ..waiting
    };
    let (state, env) = (fixture.state.clone(), fixture.env());
    let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
    let recovered = AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: None,
        fetched_at: 1,
        plan: None,
        email: None,
        unavailable: None,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent: 20.0,
            resets_at: Some(now_millis() + 86_400_000),
            window_minutes: None,
        }],
    };

    switch_to_with_recovery(
        &fixture.state,
        &fixture.worker(),
        Some("claude-a".to_string()),
        move || {
            assert!(state.is_switching_account("w"));
            assert!(!state
                .quota_recovery
                .inner
                .lock()
                .unwrap()
                .waiting
                .contains_key("w"));
            state.account_quotas.hold_for_test(&recovered);
            Ok(env)
        },
        move |request| stand_in_launch(&started, &launches, || {})(request),
        Some(&trigger),
    )
    .await
    .unwrap();
    fixture.recovered().await;

    assert_eq!(fixture.worker().account_id, None);
    assert_eq!(fixture.launched(), 1);
    assert!(fixture
        .prompt_of(0)
        .unwrap()
        .contains("usage quota has recovered"));
    assert!(!fixture.dir.join("claude-a").join(CONVERSATION).exists());
    assert!(fixture.parked().is_none());
}

/// An older recovery run cannot displace a newer trigger, including while that trigger is
/// claimed and leaves an empty waiting slot.
#[tokio::test]
async fn keeping_a_trigger_waiting_never_replaces_a_newer_one() {
    let fixture = Fixture::new("auto-switch-keep-waiting", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    let capture = || {
        fixture
            .state
            .quota_recovery
            .capture(&fixture.state, "w")
            .unwrap()
            .unwrap()
    };
    let (older, newer) = (capture(), capture());
    assert!(older.seq < newer.seq);
    let waiting = fixture.state.quota_recovery.inner.lock().unwrap().waiting["w"].clone();
    assert!(claim(&fixture.state, &waiting));

    assert!(park(&fixture.state, "w", newer.clone()));
    park(&fixture.state, "w", older.clone());
    assert!(Arc::ptr_eq(&fixture.parked().unwrap().0, &newer.token));

    assert!(claim(&fixture.state, &newer));
    park(&fixture.state, "w", older.clone());
    assert!(
        fixture.parked().is_none(),
        "the newer trigger is still being judged"
    );
    assert!(!park(&fixture.state, "w", older.clone()));
    assert!(!older.still_due(&fixture.state));
    assert!(newer.still_due(&fixture.state));
    park(&fixture.state, "w", newer.clone());
    assert!(Arc::ptr_eq(&fixture.parked().unwrap().0, &newer.token));

    assert!(claim(&fixture.state, &newer));
    park(&fixture.state, "w", newer.clone());
    let held = PendingRecovery {
        hold_until: Some(5),
        ..older.clone()
    };
    park(&fixture.state, "w", held);
    assert_eq!(fixture.parked().unwrap().1, None);
}

/// A hold ends for a trigger that is no longer parked (recovered, cancelled) without doing
/// anything.
#[tokio::test]
async fn the_end_of_a_hold_does_nothing_for_a_trigger_that_is_gone() {
    let fixture = Fixture::new("auto-switch-hold-gone", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    fixture.install_recovery_launch().await;
    let (token, until) = fixture.parked().unwrap();
    fixture.state.quota_recovery.cancel("w");

    end_hold(&fixture.state, "w", &token, until.unwrap()).await;

    assert_eq!(fixture.launched(), 0);
    assert!(fixture.parked().is_none());
}

/// A trigger parked over another, by a later failed turn of the session, is not taken for the
/// first one: claiming the first leaves the second waiting.
#[tokio::test]
async fn a_trigger_parked_over_another_is_not_claimed_as_the_first() {
    let fixture = Fixture::new("auto-switch-replaced", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    let first = fixture.state.quota_recovery.inner.lock().unwrap().waiting["w"].clone();
    let second = fixture
        .state
        .quota_recovery
        .capture(&fixture.state, "w")
        .unwrap()
        .unwrap();
    assert!(park(&fixture.state, "w", second.clone()));

    assert!(!claim(&fixture.state, &first));
    assert!(Arc::ptr_eq(&fixture.parked().unwrap().0, &second.token));
    assert!(claim(&fixture.state, &second));
    assert!(fixture.parked().is_none());
}

/// The timer of an earlier hold does not end the hold of a trigger parked since, nor one that
/// was held to another deadline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_timer_does_not_end_another_hold() {
    let fixture = Fixture::new("auto-switch-stale-timer", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    fixture.install_recovery_launch().await;
    let (token, until) = fixture.parked().unwrap();
    let until = until.unwrap();

    end_hold(&fixture.state, "w", &token, until - 1).await;
    end_hold(&fixture.state, "w", &Arc::new(()), until).await;

    assert_eq!(fixture.launched(), 0);
    assert_eq!(fixture.parked().unwrap().1, Some(until));
}

/// The timer set when a trigger is held ends the hold at its deadline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_scheduled_end_of_a_hold_switches_the_session() {
    let fixture = Fixture::new("auto-switch-timer", SessionStatus::Idle);
    hold_used_up_resetting_in(&fixture, 4);
    fixture.failed_turn().await;
    fixture.install_recovery_launch().await;
    let past = now_millis() - 1;
    let token = {
        let mut inner = fixture.state.quota_recovery.inner.lock().unwrap();
        let wait = inner.waiting.get_mut("w").unwrap();
        wait.hold_until = Some(past);
        wait.token.clone()
    };

    schedule_end_hold(&fixture.state, "w", token, past);
    fixture.recovered().await;

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
}

#[test]
fn a_window_resetting_in_exactly_five_minutes_is_waited_for() {
    let limit = HOLD_FOR_RESET.as_millis() as i64;
    assert!(within_hold(1_000 + limit, 1_000));
    assert!(!within_hold(1_000 + limit + 1, 1_000));
}

/// Nothing is waited for without a reading that can be trusted to say when the account
/// recovers: none held, one that failed to be read again, a window with no reset time, or one
/// that is not used up.
#[tokio::test]
async fn no_hold_without_a_trustworthy_reset_time() {
    let fixture = Fixture::new("auto-switch-hold-none", SessionStatus::Idle);
    let session = fixture.worker();
    hold_used_up_resetting_in(&fixture, 4);
    assert!(hold_deadline(&fixture.state, &session).unwrap().is_some());

    let unreadable = Session {
        account_id: Some("claude-b".to_string()),
        ..session.clone()
    };
    assert!(hold_deadline(&fixture.state, &unreadable)
        .unwrap()
        .is_none());

    let window = |used, resets_at| QuotaWindow {
        kind: QuotaWindowKind::Weekly,
        label: None,
        used_percent: used,
        resets_at,
        window_minutes: None,
    };
    fixture.hold_reading(None, vec![window(100.0, None)], None);
    assert!(hold_deadline(&fixture.state, &session).unwrap().is_none());
    fixture.hold_reading(None, vec![window(50.0, Some(now_millis() + 60_000))], None);
    assert!(hold_deadline(&fixture.state, &session).unwrap().is_none());
    fixture.hold_reading(
        None,
        vec![window(100.0, Some(now_millis() + 60_000))],
        Some(crate::protocol::QuotaUnavailable::new(
            crate::protocol::quota_unavailable_code::TIMED_OUT,
            "x",
            &[("agent", "Claude Code")],
        )),
    );
    assert!(hold_deadline(&fixture.state, &session).unwrap().is_none());
}

/// A reading never moves a session: one whose turn did not stop on the limit stays on an
/// account that reads as used up, a turn that is only running or cut off included.
#[tokio::test]
async fn a_session_on_a_used_up_account_is_not_switched_unless_its_turn_hit_the_limit() {
    crate::coordinator::account_switch::USAGE_LIMIT_RECORD_WAIT_OVERRIDE
        .set(Some(Duration::from_millis(100)));
    let fixture = Fixture::new("auto-switch-reading-only", SessionStatus::Working);
    fixture.hold(None, 100.0);
    fixture.end_on_a_tool_call();

    let switched = fixture.failed_turn().await;
    crate::coordinator::account_switch::USAGE_LIMIT_RECORD_WAIT_OVERRIDE.set(None);
    assert!(!switched, "not a usage limit");

    assert_eq!(fixture.worker().account_id, None);
    assert_eq!(fixture.launched(), 0);
    assert!(
        fixture.state.live_session("w").is_some(),
        "its process was left alone"
    );
}

/// The switch happens only while its own setting is on, which neither automatic selection nor
/// the approval of an agent's requests turns on.
#[tokio::test]
async fn nothing_is_switched_while_the_setting_is_off() {
    let fixture = Fixture::new("auto-switch-off", SessionStatus::Idle);
    fixture
        .state
        .store
        .set_auto_switch_on_usage_limit(false)
        .unwrap();
    fixture.state.store.set_auto_select_account(true).unwrap();
    fixture
        .state
        .store
        .set_auto_approve_account_switches(true)
        .unwrap();

    fixture.failed_turn().await;
    after_failed_turn(&fixture.state, "setting-off");

    assert_eq!(fixture.worker().account_id, None);
    assert_eq!(fixture.launched(), 0);
    assert!(!ASKED.lock().unwrap().contains(&"setting-off".to_string()));
}

/// An account that is clearly unusable is never the target: used up, turned off, logged out,
/// whose quota cannot be read now or has not been read yet, or the one the session is on. With
/// none left the session stays; once one is usable it is switched.
#[tokio::test]
async fn no_account_that_is_clearly_unusable_is_chosen() {
    let fixture = Fixture::new("auto-switch-none", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let unavailable = |code: crate::protocol::Code| {
        Some(crate::protocol::QuotaUnavailable::new(
            code,
            "x",
            &[("agent", "Claude Code")],
        ))
    };
    let weekly = |used: f64| QuotaWindow {
        kind: QuotaWindowKind::Weekly,
        label: None,
        used_percent: used,
        resets_at: Some(now_millis() + 86_400_000),
        window_minutes: None,
    };
    // The default account is the session's own and reads fine; `claude-b` is not read yet.
    fixture.hold(Some("claude-a"), 100.0);
    assert!(!fixture.failed_turn().await, "no account qualifies");
    assert_eq!(fixture.launched(), 0, "used up, and nothing else qualifies");

    fixture.hold(Some("claude-a"), 30.0);
    fixture
        .state
        .store
        .set_account_disabled("claude-a", true)
        .unwrap();
    fixture.failed_turn().await;
    assert_eq!(fixture.launched(), 0, "turned off");
    fixture
        .state
        .store
        .set_account_disabled("claude-a", false)
        .unwrap();

    for code in [
        crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
        crate::protocol::quota_unavailable_code::CONFIG_DIR_MISSING,
        crate::protocol::quota_unavailable_code::COMMAND_FAILED,
    ] {
        fixture.hold_reading(Some("claude-a"), vec![weekly(30.0)], unavailable(code));
        fixture.failed_turn().await;
        assert_eq!(
            fixture.launched(),
            0,
            "{code}: cannot be read now or is logged out"
        );
    }
    assert!(
        notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty(),
        "not switching is not a failure"
    );

    fixture.hold(Some("claude-a"), 30.0);
    fixture.failed_turn().await;
    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
}

/// An account that is gone when the switch is asked for it is a wrong time, not a failure:
/// nothing changes and nobody is told.
#[tokio::test]
async fn an_unknown_target_is_not_a_failure() {
    let fixture = Fixture::new("auto-switch-unknown", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let session = fixture.worker();
    let env = fixture.env();

    switch_to(
        &fixture.state,
        &session,
        Some("no-such-account".to_string()),
        move || Ok(env),
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect("a wrong time is not an error");

    assert_eq!(fixture.worker().account_id, None);
    assert!(fixture.state.live_session("w").is_some());
    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
}

/// Switching a session twice for its limit never goes back to the account it left: with the
/// two other accounts both at their limit it is not swapped between them at every turn, and
/// no wait is kept after a switch that worked.
#[tokio::test]
async fn a_session_is_not_moved_back_onto_the_account_it_just_left() {
    let fixture = Fixture::new("auto-switch-back", SessionStatus::Idle);
    fixture.hold(Some("claude-b"), 50.0);

    fixture.failed_turn().await;
    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
    assert_eq!(
        fixture
            .state
            .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR),
        [None],
        "noted by the switch itself, before its claim was released"
    );

    // The next turn hits the limit on `claude-a` as well, its conversation having come with
    // it. The default account still reads as having quota, but it was left a moment ago, so
    // `claude-b` is the one left.
    fixture.failed_turn().await;

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
}

/// A session waiting for the user (by a hook or by what its screen shows), one a launch or
/// switch holds, and one with no process are left alone.
#[tokio::test]
async fn sessions_that_cannot_be_switched_now_are_left_alone() {
    let fixture = Fixture::new("auto-switch-busy", SessionStatus::WaitingUser);
    fixture.failed_turn().await;
    assert_eq!(fixture.launched(), 0, "a prompt it waits at would end");

    fixture
        .state
        .apply_hook_status("w", SessionStatus::Idle)
        .unwrap();
    fixture.show_dialog();
    fixture.failed_turn().await;
    assert_eq!(fixture.launched(), 0, "a prompt the screen shows would end");
    fixture.clear_screen();

    fixture.show_agents_view();
    assert_eq!(
        fixture.state.status_for_switch(&fixture.worker()),
        SessionStatus::Dormant
    );
    fixture.failed_turn().await;
    assert_eq!(fixture.launched(), 0, "a view the screen shows would end");
    fixture.clear_screen();

    let claim = fixture.state.begin_account_switch("w", false).unwrap();
    fixture.failed_turn().await;
    assert_eq!(fixture.launched(), 0, "a claim is held");
    drop(claim);

    let mut down = fixture.worker();
    down.status = SessionStatus::Interrupted;
    fixture.state.store.update_session(&down).unwrap();
    fixture.failed_turn().await;
    assert_eq!(fixture.launched(), 0, "no process to switch");
    assert_eq!(fixture.worker().account_id, None);
}

/// What the decision on a record read earlier is held to under the switch's claim: a session
/// that is on another account by then, shown as waiting, without a running process, or whose
/// last turn did not stop on the limit is refused with nothing changed.
#[tokio::test]
async fn a_switch_is_refused_under_its_claim_when_the_record_it_was_decided_on_is_stale() {
    let fixture = Fixture::new("auto-switch-stale", SessionStatus::Idle);
    fixture
        .state
        .store
        .insert_session(&Session {
            project_id: Some("p1".to_string()),
            ..crate::test_support::project_session(
                "no-process",
                AgentId::CLAUDE,
                SessionStatus::Idle,
            )
        })
        .unwrap();
    let attempt = |id: &'static str, only_if: SwitchOnlyIf<'static>| {
        let fixture = &fixture;
        let env = fixture.env();
        async move {
            crate::coordinator::account_switch::switch_session_account_if(
                &fixture.state,
                id,
                Some(Some("claude-a".to_string())),
                None,
                only_if,
                None,
                LaunchEdit::default(),
                move || Ok(env),
                |_| anyhow::bail!("not reached"),
            )
            .await
            .expect_err("refused")
        }
    };
    let refusal = |failed: &crate::coordinator::account_switch::SwitchFailed| {
        assert!(!failed.ended_process);
        failed.error.downcast_ref::<NoLongerDue>().map(|due| due.0)
    };

    let moved_by_hand = attempt(
        "w",
        SwitchOnlyIf {
            on_account: Some(Some("claude-b")),
            ..SwitchOnlyIf::default()
        },
    )
    .await;
    assert_eq!(
        refusal(&moved_by_hand),
        Some("the session is on another account now")
    );

    fixture.show_dialog();
    let waiting = attempt(
        "w",
        SwitchOnlyIf {
            not_waiting: true,
            ..SwitchOnlyIf::default()
        },
    )
    .await;
    assert_eq!(
        refusal(&waiting),
        Some("the session waits for the user now")
    );
    fixture.clear_screen();

    fixture.show_agents_view();
    let viewing = attempt(
        "w",
        SwitchOnlyIf {
            not_waiting: true,
            ..SwitchOnlyIf::default()
        },
    )
    .await;
    assert_eq!(
        refusal(&viewing),
        Some("the session waits for the user now")
    );
    fixture.clear_screen();

    fixture.end_on_a_tool_call();
    let limit = SwitchOnlyIf {
        usage_limited: true,
        ..SwitchOnlyIf::default()
    };
    let cut_off = attempt("w", limit).await;
    assert_eq!(
        refusal(&cut_off),
        Some("the session's last turn did not stop on a usage limit")
    );
    fixture.end_on_usage_limit();

    let mut stopped = fixture.worker();
    stopped.status = SessionStatus::Interrupted;
    fixture.state.store.update_session(&stopped).unwrap();
    let running = SwitchOnlyIf {
        running: true,
        ..SwitchOnlyIf::default()
    };
    let stopped = attempt("w", running).await;
    assert_eq!(
        refusal(&stopped),
        Some("the session has no running process now")
    );
    let exited = attempt("no-process", running).await;
    assert_eq!(
        refusal(&exited),
        Some("the session has no running process now")
    );

    assert_eq!(fixture.worker().account_id, None);
    assert_eq!(fixture.launched(), 0);
    assert!(!fixture.state.is_switching_account("w"));
}

/// A switch that fails after the process was ended is undone by Octoboard itself: the session
/// is relaunched on the account it had, as it was and without a prompt, and nobody is told of
/// a failure.
#[tokio::test]
async fn a_failed_switch_is_undone_by_bringing_the_session_back_up() {
    let fixture = Fixture::new("auto-switch-restored", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let env = fixture.env();

    let outcome = try_switch(
        &fixture.state,
        "w",
        move || Ok(env),
        fixture.launch_failing(1),
    )
    .await;

    assert!(!outcome.expect("restored, so not an error"), "not moved");
    assert!(fixture
        .state
        .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR)
        .is_empty());
    let worker = fixture.worker();
    assert_eq!(worker.account_id, None, "on the account it had");
    assert_ne!(worker.status, SessionStatus::Interrupted);
    assert!(fixture.state.live_session("w").is_some(), "it runs again");
    assert_eq!(fixture.launched(), 1, "the one relaunch that worked");
    assert_eq!(
        fixture.prompt_of(0),
        None,
        "no prompt: its turn is not carried on"
    );
    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    assert!(notices(&mut events, notice_code::ACCOUNT_SWITCHED_AUTOMATICALLY).is_empty());
    let heard = owner_heard(&fixture).await;
    assert!(!heard.contains("could neither switch"), "{heard}");
}

/// Only when bringing the session back up fails too is an error raised: the user and the
/// owner are told the session is interrupted on its own account, and why.
#[tokio::test]
async fn a_session_that_cannot_be_brought_back_is_told_of() {
    let fixture = Fixture::new("auto-switch-not-restored", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let env = fixture.env();

    let err = try_switch(
        &fixture.state,
        "w",
        move || Ok(env),
        fixture.launch_failing(usize::MAX),
    )
    .await
    .expect_err("neither the switch nor the restoring worked");

    assert!(
        format!("{err:#}").contains("the agent cannot be started"),
        "{err:#}"
    );
    let worker = fixture.worker();
    assert_eq!(worker.account_id, None);
    assert_eq!(worker.status, SessionStatus::Interrupted);
    let mut told = notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED,
    );
    assert_eq!(told.len(), 1, "the user was told");
    let params = told.remove(0).1;
    assert_eq!(params["agent"], "Claude Code");
    assert_eq!(params["from"], "Default");
    assert_eq!(params["account"], "Work A");
    assert_eq!(params["account_id"], "claude-a");
    assert_eq!(params["session"], "w");
    assert!(!params.contains_key("from_id"));
    assert!(!params["reason"].is_empty());
    assert!(params.contains_key("reason_code"));
    let heard = output_containing(&fixture.owner, "could neither switch");
    assert!(
        heard.contains("could neither switch it to another account"),
        "{heard}"
    );
    assert!(!heard.to_lowercase().contains("retry"), "{heard}");
}

/// A failure before the switch ended the process leaves the session as it was and tells
/// nobody, and nothing is relaunched to restore it.
#[tokio::test]
async fn a_failure_before_the_process_was_ended_tells_nobody() {
    let fixture = Fixture::new("auto-switch-early-failure", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let session = fixture.worker();

    let err = switch_to(
        &fixture.state,
        &session,
        Some("claude-a".to_string()),
        // The login shell cannot be read: before anything is ended.
        || anyhow::bail!("no shell"),
        fixture.launch_failing(usize::MAX),
    )
    .await
    .expect_err("returned to be logged");

    assert!(format!("{err:#}").contains("no shell"), "{err:#}");
    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    assert!(
        fixture.state.live_session("w").is_some(),
        "its process runs on"
    );
    assert_eq!(fixture.worker().account_id, None);
}

/// A process that exits by itself between the running check and the stop was not ended by the
/// switch: a later failure is not the switch's to report, nothing is restored and nobody is
/// told.
#[tokio::test]
async fn a_process_that_ended_by_itself_before_the_stop_is_not_the_switchs_to_report() {
    let fixture = Fixture::new("auto-switch-exits-itself", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let env = fixture.env();
    let (state, worker) = (fixture.state.clone(), fixture.worker_process.clone());
    let snapshot = move || {
        // Runs after the running check and before the stop: the agent exits on its own.
        worker.terminate();
        assert!(
            crate::session::wait::wait_for(crate::test_support::PATIENCE, || {
                state.live_session("w").is_none()
            }),
            "the exit was recorded"
        );
        Ok(env)
    };

    let err = switch_to(
        &fixture.state,
        &fixture.worker(),
        Some("claude-a".to_string()),
        snapshot,
        fixture.launch_failing(usize::MAX),
    )
    .await
    .expect_err("the relaunch failed");

    assert!(
        format!("{err:#}").contains("the agent cannot be started"),
        "{err:#}"
    );
    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    let heard = owner_heard(&fixture).await;
    assert!(!heard.contains("could neither switch"), "{heard}");
}

/// The same when the exit has not been recorded yet by the time the switch stops the process:
/// the process is still registered, but it was gone before the stop.
#[tokio::test]
async fn a_process_that_exited_but_is_still_registered_was_not_ended_by_the_switch() {
    let fixture = Fixture::new("auto-switch-exited-registered", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let env = fixture.env();
    let worker = fixture.worker_process.clone();
    // The agent exits during the plan; the exit watcher has not recorded it by the stop.
    let snapshot = move || {
        worker.terminate();
        Ok(env)
    };

    let _ = switch_to(
        &fixture.state,
        &fixture.worker(),
        Some("claude-a".to_string()),
        snapshot,
        fixture.launch_failing(usize::MAX),
    )
    .await;

    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    let heard = owner_heard(&fixture).await;
    assert!(!heard.contains("could neither switch"), "{heard}");
}

/// The user archiving the session while it is switched is their word on the matter: the
/// switch fails after the process was ended, and nobody is told it did, nor is it restored.
#[tokio::test]
async fn archiving_the_session_during_the_switch_tells_nobody() {
    let fixture = Fixture::new("auto-switch-archived-meanwhile", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let env = fixture.env();
    let (state, archived) = (fixture.state.clone(), fixture.worker());
    let snapshot = move || {
        // Runs before the process is ended.
        let mut archived = archived;
        archived.status = SessionStatus::Archived;
        state.store.update_session(&archived)?;
        Ok(env)
    };

    let err = switch_to(
        &fixture.state,
        &fixture.worker(),
        Some("claude-a".to_string()),
        snapshot,
        fixture.launch_failing(usize::MAX),
    )
    .await
    .expect_err("the session was archived");

    assert!(is_quiet(&err), "{err:#}");
    assert!(fixture.state.live_session("w").is_none());
    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    let heard = owner_heard(&fixture).await;
    assert!(!heard.contains("could neither switch"), "{heard}");
}

/// A session deleted since is skipped without an error.
#[tokio::test]
async fn a_session_deleted_meanwhile_is_skipped_quietly() {
    let fixture = Fixture::new("auto-switch-deleted", SessionStatus::Idle);
    try_switch(
        &fixture.state,
        "gone",
        || Ok(HashMap::new()),
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect("nothing to switch");
}

/// A change of the target's sharing that begins after the switch has ended the session's
/// process refuses the switch too late to be a wrong time: the session is brought back up on
/// the account it had, and nobody is told.
#[tokio::test]
async fn a_refusal_after_the_process_was_ended_is_undone_quietly() {
    let fixture = Fixture::new("auto-switch-late-refusal", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let target = fixture
        .state
        .store
        .get_account("claude-a")
        .unwrap()
        .unwrap();
    let lock = Arc::new(Mutex::new(None));
    let (state, held) = (fixture.state.clone(), lock.clone());
    let env = fixture.env();
    // Runs before the process is ended, after the checks that would have refused.
    let snapshot = move || {
        *held.lock().unwrap() = Some(state.lock_account_sharing(&target, |_, _| Ok(()))?.1);
        Ok(env)
    };

    switch_to(
        &fixture.state,
        &fixture.worker(),
        Some("claude-a".to_string()),
        snapshot,
        fixture.launch_failing(0),
    )
    .await
    .expect("brought back up");

    assert_eq!(fixture.launched(), 1, "the restore launched it");
    assert_eq!(fixture.worker().account_id, None);
    assert!(fixture.state.live_session("w").is_some());
    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    drop(lock);
}

/// When the relaunch did not carry the note (the turn could not be carried on after all), the
/// session is told in a message of its own, which opens a turn that needs no report back.
#[tokio::test]
async fn a_note_written_instead_of_the_prompt_opens_a_turn_that_owes_no_report() {
    let fixture = Fixture::new("auto-switch-note-written", SessionStatus::Idle);
    let worker = fixture.worker();
    fixture.state.turn_started("w", Some("work"));

    let (state, note) = (
        fixture.state.clone(),
        note_to_session(AgentId::CLAUDE, "A", "B"),
    );
    tokio::task::spawn_blocking(move || {
        tell_sessions(&state, &worker, "A", "B", &note, false, None)
    })
    .await
    .unwrap();

    let heard = output_containing(&fixture.worker_process, "switched you to `B`");
    assert!(heard.contains("switched you to `B`."), "{heard}");
    fixture.state.close_turn("w", false);
    fixture
        .state
        .turn_started("w", Some(&note_to_session(AgentId::CLAUDE, "A", "B")));
    assert_eq!(
        fixture.state.close_turn("w", false),
        crate::state::TurnClose::Reported
    );
}

/// The turn the note opens is credited by the note's own opening, so a turn another notice
/// opens first still owes its report and leaves the credit to the note's turn.
#[tokio::test]
async fn another_notices_turn_does_not_spend_the_notes_credit() {
    let fixture = Fixture::new("auto-switch-note-credit", SessionStatus::Idle);
    let worker = fixture.worker();
    fixture.state.turn_started("w", Some("work"));

    let (state, note) = (
        fixture.state.clone(),
        note_to_session(AgentId::CLAUDE, "A", "B"),
    );
    tokio::task::spawn_blocking(move || {
        tell_sessions(&state, &worker, "A", "B", &note, false, None)
    })
    .await
    .unwrap();
    output_containing(&fixture.worker_process, "switched you to `B`");
    fixture.state.close_turn("w", false);

    fixture.state.turn_started(
        "w",
        Some(&format!(
            "{NOTICE_PREFIX}: your parent session has changed."
        )),
    );
    assert_eq!(
        fixture.state.close_turn("w", false),
        crate::state::TurnClose::OwesReport
    );
    fixture
        .state
        .turn_started("w", Some(&note_to_session(AgentId::CLAUDE, "A", "B")));
    assert_eq!(
        fixture.state.close_turn("w", false),
        crate::state::TurnClose::Reported
    );
}

/// An account name with a character the write strips (here a zero-width non-joiner) still
/// credits the note's turn, which reports the stripped text as its prompt.
#[tokio::test]
async fn a_note_naming_an_account_the_write_strips_still_credits_its_turn() {
    let fixture = Fixture::new("auto-switch-note-stripped", SessionStatus::Idle);
    let worker = fixture.worker();
    fixture.state.turn_started("w", Some("work"));
    let from = "Work\u{200C}Two";

    let (state, note) = (
        fixture.state.clone(),
        note_to_session(AgentId::CLAUDE, from, "B"),
    );
    tokio::task::spawn_blocking(move || {
        tell_sessions(&state, &worker, from, "B", &note, false, None)
    })
    .await
    .unwrap();
    output_containing(&fixture.worker_process, "switched you to `B`");
    fixture.state.close_turn("w", false);

    let written = crate::session::term::stripped(&note_to_session(AgentId::CLAUDE, from, "B"));
    assert!(!written.contains('\u{200C}'));
    fixture.state.turn_started("w", Some(&written));
    assert_eq!(
        fixture.state.close_turn("w", false),
        crate::state::TurnClose::Reported
    );
}

/// A Grok Build session whose only wait is its usage-limit dialog is not waiting as far as the
/// switch is concerned, at the first check and under the claim: ending the modal is the point.
/// Another dialog, or the same one on another agent, still is a wait.
#[tokio::test]
async fn a_grok_session_showing_its_usage_limit_dialog_passes_every_wait_check() {
    const GROK_MODAL: &str = "You hit your weekly limit.\n1 (o) Upgrade tier\n2 (o) Buy more \
                              credits\n3 (o) Try Again\nEnter :submit / Shift+x :dismiss\n";
    let fixture = Fixture::new("auto-switch-grok-modal", SessionStatus::Idle);
    let grok_session = Session {
        project_id: Some("p1".to_string()),
        has_conversation: true,
        agent_session_id: Some("b0000000-0000-4000-8000-000000000001".to_string()),
        ..crate::test_support::project_session("g", AgentId::GROK, SessionStatus::Idle)
    };
    // Its turn stopped on the limit, as Grok 1.0.70 writes it.
    let record = fixture
        .dir
        .join("grok-default/sessions/proj/b0000000-0000-4000-8000-000000000001");
    std::fs::create_dir_all(&record).unwrap();
    std::fs::write(
        record.join("updates.jsonl"),
        concat!(
            r#"{"params":{"update":{"sessionUpdate":"user_message_chunk"}}}"#,
            "\n",
            r#"{"params":{"update":{"sessionUpdate":"turn_completed","#,
            r#""stop_reason":"error","agent_result":"API error (status 402)"}}}"#,
            "\n"
        ),
    )
    .unwrap();
    let grok_env = HashMap::from([(
        "GROK_HOME".to_string(),
        fixture
            .dir
            .join("grok-default")
            .to_string_lossy()
            .into_owned(),
    )]);
    fixture.state.store.insert_session(&grok_session).unwrap();
    let grok = crate::test_support::fake_live_session("g", AgentId::GROK, 80, 24, "sleep 30");
    fixture.state.register_live(grok.clone());
    grok.on_output(GROK_MODAL.as_bytes());
    assert!(grok
        .look_for_dialog()
        .current
        .is_some_and(|dialog| dialog.is_usage_limit()));

    fixture
        .state
        .store
        .insert_account(&Account {
            id: "grok-a".to_string(),
            agent: AgentId::GROK,
            name: "Grok A".to_string(),
            config_dir: fixture.dir.join("grok-a").to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        })
        .unwrap();
    fixture.state.account_quotas.hold_in_dir_for_test(
        &AccountQuota {
            reset_credits: None,
            agent: AgentId::GROK,
            account: Some("grok-a".to_string()),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: 10.0,
                resets_at: Some(now_millis() + 86_400_000),
                window_minutes: None,
            }],
            unavailable: None,
        },
        fixture.dir.join("grok-a").to_str().unwrap(),
    );

    assert_eq!(
        fixture.state.shown_status(&grok_session),
        SessionStatus::WaitingUser
    );
    assert_eq!(
        fixture.state.status_for_switch(&grok_session),
        SessionStatus::Idle
    );

    // The whole decision goes on to the switch itself, which refuses the Grok target that is
    // no initialised home: it is the wait that would have stopped it before that.
    let env = grok_env.clone();
    let err = try_switch(
        &fixture.state,
        "g",
        move || Ok(env),
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect_err("went on to the switch");
    assert!(
        format!("{err:#}").contains("is not a Grok home yet"),
        "{err:#}"
    );

    // Under the claim the wait is not what refuses it: the switch goes on to its own checks,
    // and refuses the Grok target there.
    let err = crate::coordinator::account_switch::switch_session_account_if(
        &fixture.state,
        "g",
        Some(Some("grok-a".to_string())),
        None,
        SwitchOnlyIf {
            not_waiting: true,
            running: true,
            usage_limited: true,
            ..SwitchOnlyIf::default()
        },
        None,
        LaunchEdit::default(),
        move || Ok(grok_env),
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect_err("refused further on");
    assert!(
        format!("{:#}", err.error).contains("is not a Grok home yet"),
        "{:#}",
        err.error
    );

    // Without the usage-limit part, and for a dialog of another kind, it is still a wait.
    let plain = crate::coordinator::account_switch::switch_session_account_if(
        &fixture.state,
        "g",
        Some(Some("grok-a".to_string())),
        None,
        SwitchOnlyIf {
            not_waiting: true,
            ..SwitchOnlyIf::default()
        },
        None,
        LaunchEdit::default(),
        || Ok(HashMap::new()),
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect_err("refused");
    assert!(plain.error.is::<NoLongerDue>());
    fixture.show_dialog();
    assert_eq!(
        fixture.state.status_for_switch(&fixture.worker()),
        SessionStatus::WaitingUser,
        "Claude Code's resume dialog is not a usage limit"
    );
}

/// The usage limit may reach the record a moment after the failed turn is reported: the record
/// is read again for a while before the turn is judged not to have stopped on the limit.
#[tokio::test]
async fn a_usage_limit_that_reaches_the_record_late_still_switches() {
    let fixture = Fixture::new("auto-switch-late-record", SessionStatus::Idle);
    let record = fixture.dir.join("default-claude").join(CONVERSATION);
    fixture.end_on_usage_limit();
    let limit_text = std::fs::read_to_string(&record).unwrap();
    fixture.end_on_a_tool_call();

    tokio::join!(fixture.failed_turn(), async {
        // Once every task is idle, the record has been read without the limit and is waited on.
        crate::test_support::runtime_settled().await;
        std::fs::write(record, limit_text).unwrap();
    });

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
}

/// A wait is registered only after confirming the usage limit in the conversation record.
#[tokio::test]
async fn no_target_still_confirms_the_usage_limit_before_registering_a_wait() {
    let fixture = Fixture::new("auto-switch-no-target", SessionStatus::Idle);
    fixture.hold(Some("claude-a"), 100.0);
    let taken = Arc::new(AtomicUsize::new(0));
    let counted = taken.clone();
    let env = fixture.env();

    try_switch(
        &fixture.state,
        "w",
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(env)
        },
        fixture.launch_failing(0),
    )
    .await
    .expect("nothing to do");

    assert_eq!(taken.load(Ordering::SeqCst), 1);
    assert!(fixture
        .state
        .quota_recovery
        .inner
        .lock()
        .unwrap()
        .waiting
        .contains_key("w"));
    assert_eq!(fixture.launched(), 0);
}

/// A session moved to another account while the record was waited for is left alone: the limit
/// was found on the account it is no longer on.
#[tokio::test]
async fn a_session_moved_during_the_wait_is_left_alone() {
    let fixture = Fixture::new("auto-switch-moved-meanwhile", SessionStatus::Idle);
    let record = fixture.dir.join("default-claude").join(CONVERSATION);
    fixture.end_on_usage_limit();
    let limit_text = std::fs::read_to_string(&record).unwrap();
    fixture.end_on_a_tool_call();
    let (state, to) = (fixture.state.clone(), fixture.dir.join("claude-a"));
    let env = fixture.env();

    let (switched, ()) = tokio::join!(
        try_switch(
            &fixture.state,
            "w",
            move || Ok(env),
            fixture.launch_failing(0),
        ),
        async {
            // Once every task is idle, the record has been read without the limit and is waited
            // on. Moved first, then the limit shows: the wait ends on an account that has changed.
            crate::test_support::runtime_settled().await;
            state
                .set_session_account("w", Some("claude-a"), to.to_str())
                .unwrap();
            std::fs::write(record, limit_text).unwrap();
        }
    );
    switched.expect("left alone, not switched from the account it moved to");

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
    assert_eq!(fixture.launched(), 0);
}

/// A relaunch that starts and dies within the settling time is the switch's failure to report;
/// the owner is not also told that the process "ended on its own", only that the session could
/// not be brought back, if it could not.
#[tokio::test]
async fn a_relaunch_that_dies_is_reported_as_the_outcome_not_as_ending_on_its_own() {
    let fixture = Fixture::new("auto-switch-dies", SessionStatus::Idle);
    let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
    let env = fixture.env();
    // The first process starts and dies; the restore's starts and stays.
    let calls = Arc::new(AtomicUsize::new(0));
    let launch = move |request: LaunchRequest| {
        let launched = stand_in_launch(&started, &launches, || {})(request)?;
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            let process = launched.session.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                process.terminate();
            });
        }
        Ok(launched)
    };

    try_switch(&fixture.state, "w", move || Ok(env), launch)
        .await
        .expect("brought back up, so not an error");

    assert_eq!(
        fixture.worker().account_id,
        None,
        "on its own account again"
    );
    assert!(fixture.state.live_session("w").is_some());
    let died = (*fixture.started.lock().unwrap()[0]).clone();
    for process in [&*fixture.worker_process, &died] {
        crate::test_support::exit_settled(process).await;
    }
    let heard = owner_heard(&fixture).await;
    assert!(!heard.contains("ended on its own"), "{heard}");
    assert!(!heard.contains("could neither switch"), "{heard}");
}

/// A turn that failed for another reason than the usage limit (an overload, a network error)
/// is judged before any claim is taken: the session is never shown as switching, which would
/// refuse an interrupt, a handoff or a rebinding for the seconds the record is waited on.
#[tokio::test]
async fn a_failed_turn_that_did_not_hit_the_limit_takes_no_claim() {
    crate::coordinator::account_switch::USAGE_LIMIT_RECORD_WAIT_OVERRIDE
        .set(Some(Duration::from_millis(100)));
    let fixture = Fixture::new("auto-switch-no-claim", SessionStatus::Idle);
    fixture.end_on_a_tool_call();
    let mut events = fixture.state.subscribe();

    fixture.failed_turn().await;
    crate::coordinator::account_switch::USAGE_LIMIT_RECORD_WAIT_OVERRIDE.set(None);

    let mut shown_switching = false;
    while let Ok(event) = events.try_recv() {
        if let Event::SessionUpserted { session } = event {
            shown_switching |= session.switching_account;
        }
    }
    assert!(!shown_switching, "shown as switching its account");
    assert_eq!(fixture.launched(), 0);
    assert_eq!(fixture.worker().account_id, None);
}

/// Only Octoboard's own switch watches the process it launches: a switch someone asked for,
/// whose relaunch dies, still leaves the owner to be told that the session ended on its own.
#[tokio::test]
async fn a_relaunch_that_dies_after_a_switch_asked_for_still_tells_the_owner() {
    let fixture = Fixture::new("auto-switch-dies-by-hand", SessionStatus::Idle);
    let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
    let env = fixture.env();

    let failed = crate::coordinator::account_switch::switch_session_account_if(
        &fixture.state,
        "w",
        Some(Some("claude-a".to_string())),
        None,
        SwitchOnlyIf::default(),
        None,
        LaunchEdit::default(),
        move || Ok(env),
        move |request| {
            let launched = stand_in_launch(&started, &launches, || {})(request)?;
            let process = launched.session.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                process.terminate();
            });
            Ok(launched)
        },
    )
    .await
    .expect_err("the relaunch did not stay up");
    let coded = failed.error.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(coded.code, error_code::SWITCH_DID_NOT_COME_UP);

    let heard = output_containing(&fixture.owner, "ended on its own");
    assert!(
        heard.contains("`w`") && heard.contains("ended on its own"),
        "{heard}"
    );
}

/// A restore that finds another claim on the session waits for it to end rather than giving up
/// and leaving the session down.
#[tokio::test]
async fn a_restore_waits_for_another_claim_on_the_session() {
    let fixture = Fixture::new("auto-switch-restore-claim", SessionStatus::Idle);
    let before = fixture.worker();
    let live = fixture.state.live_session("w").unwrap();
    let ending = tokio::task::spawn_blocking(move || live.end())
        .await
        .unwrap();
    assert!(ending);
    assert!(
        crate::test_support::within(crate::test_support::PATIENCE, {
            let state = fixture.state.clone();
            move || state.live_session("w").is_none()
        })
        .await
    );
    // A transfer of branches holds the session for a moment.
    let claim = fixture.state.begin_switch("w").unwrap();
    let (started, launches) = (fixture.started.clone(), fixture.launches.clone());

    let (restored, ()) = tokio::join!(
        crate::coordinator::account_switch::restore_after_failed_switch(
            &fixture.state,
            &before,
            None,
            None,
            move |request| stand_in_launch(&started, &launches, || {})(request),
        ),
        async {
            // Once every task is idle, the restore has found the claim and waits for it to end.
            crate::test_support::runtime_settled().await;
            drop(claim);
        }
    );
    restored.expect("restored once the other claim ended");

    assert!(fixture.state.live_session("w").is_some());
}

/// A restore finds a session that runs already, or is archived, before it puts anything back:
/// the account it is on stays.
#[tokio::test]
async fn a_restore_touches_nothing_of_a_session_that_is_not_down() {
    let fixture = Fixture::new("auto-switch-restore-running", SessionStatus::Idle);
    let before = Session {
        account_id: Some("claude-b".to_string()),
        ..fixture.worker()
    };

    let err = crate::coordinator::account_switch::restore_after_failed_switch(
        &fixture.state,
        &before,
        None,
        None,
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect_err("it runs");

    assert!(is_already_up(&err), "{err:#}");
    assert_eq!(fixture.worker().account_id, None);
    let mut archived = fixture.worker();
    archived.status = SessionStatus::Archived;
    fixture.state.store.update_session(&archived).unwrap();
    let live = fixture.state.live_session("w").unwrap();
    tokio::task::spawn_blocking(move || live.end())
        .await
        .unwrap();
    assert!(
        crate::test_support::within(crate::test_support::PATIENCE, {
            let state = fixture.state.clone();
            move || state.live_session("w").is_none()
        })
        .await
    );
    let err = crate::coordinator::account_switch::restore_after_failed_switch(
        &fixture.state,
        &before,
        None,
        None,
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect_err("archived");
    assert!(is_quiet(&err), "{err:#}");
    assert_eq!(fixture.worker().account_id, None);
}

/// A restore finds the session on neither the account it had nor the one the failed switch was
/// to move it to: somebody moved it meanwhile, and the restore leaves it as it is.
#[tokio::test]
async fn a_restore_does_not_undo_an_account_change_somebody_else_made() {
    let fixture = Fixture::new("auto-switch-restore-moved", SessionStatus::Idle);
    let before = fixture.worker();
    let live = fixture.state.live_session("w").unwrap();
    tokio::task::spawn_blocking(move || live.end())
        .await
        .unwrap();
    assert!(
        crate::test_support::within(crate::test_support::PATIENCE, {
            let state = fixture.state.clone();
            move || state.live_session("w").is_none()
        })
        .await
    );
    let moved_dir = fixture.dir.join("claude-b");
    fixture
        .state
        .set_session_account("w", Some("claude-b"), moved_dir.to_str())
        .unwrap();

    crate::coordinator::account_switch::restore_after_failed_switch(
        &fixture.state,
        &before,
        Some("claude-a"),
        None,
        |_| anyhow::bail!("not reached"),
    )
    .await
    .expect("nothing to restore");

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
    assert!(fixture.state.live_session("w").is_none());
}

/// A turn that fails while a claim on its session is held (the continued turn of a switch that
/// has just relaunched it) is judged once the claim ends, and moves the session on.
#[tokio::test]
async fn a_turn_that_fails_under_a_claim_is_switched_once_it_ends() {
    let fixture = Fixture::new("auto-switch-claimed", SessionStatus::Idle);
    let claim = fixture.state.begin_switch("w").unwrap();

    tokio::join!(fixture.failed_turn(), async {
        // Once every task is idle, the failed turn has found the claim and waits for it to end.
        crate::test_support::runtime_settled().await;
        drop(claim);
    });

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
    assert_eq!(fixture.launched(), 1);
}

/// Failure params that share a name with ours are overridden, those of the default account's
/// ids included, which are left out.
#[tokio::test]
async fn the_params_of_a_failed_switch_override_those_of_its_failure() {
    let fixture = Fixture::new("auto-switch-params", SessionStatus::Idle);
    let mut events = fixture.state.subscribe();
    let session = fixture.worker();
    let err = CodedError::raised(
        error_code::SWITCH_DID_NOT_COME_UP,
        "down",
        &[
            ("session", "other"),
            ("from_id", "theirs"),
            ("account_id", "theirs"),
            ("detail", "kept"),
        ],
    );

    tell_switch_failed(&fixture.state, &session, None, &err).await;

    let told = notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED,
    );
    let params = &told[0].1;
    assert_eq!(params.get("session").map(String::as_str), Some("w"));
    assert_eq!(params.get("detail").map(String::as_str), Some("kept"));
    assert!(!params.contains_key("from_id") && !params.contains_key("account_id"));
}

/// What is held of the accounts a session was switched off for a limit goes with the session
/// when it is archived.
#[tokio::test]
async fn the_accounts_left_for_a_limit_are_forgotten_with_an_archived_session() {
    let fixture = Fixture::new("auto-switch-forget", SessionStatus::Idle);
    fixture.state.note_left_for_limit("w", None);
    assert_eq!(
        fixture
            .state
            .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR),
        [None]
    );

    crate::coordinator::lifecycle::archive_session(&fixture.state, "w").unwrap();

    assert!(fixture
        .state
        .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR)
        .is_empty());
}

/// While the daemon shuts down nothing is told of a switch that could not be completed or
/// undone, and nothing is started again.
#[tokio::test]
async fn nothing_is_told_of_a_switch_cut_short_by_the_shutdown() {
    let fixture = Fixture::new("auto-switch-shutdown", SessionStatus::Idle);
    let (logs, _guard) = LogBuffer::capture();
    let mut events = fixture.state.subscribe();
    let env = fixture.env();
    let state = fixture.state.clone();
    // The daemon starts to shut down once the switch has ended the process and is relaunching.
    let launch = move |_: LaunchRequest| -> Result<Launch> {
        let stopping = state.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            runtime.block_on(stopping.stop_all_sessions());
        })
        .join()
        .unwrap();
        anyhow::bail!("the agent cannot be started")
    };

    try_switch(&fixture.state, "w", move || Ok(env), launch)
        .await
        .expect("cut short, and nobody is told");

    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    assert!(
        fixture.state.live_session("w").is_none(),
        "nothing was started again"
    );
    let logged = logs.text();
    assert!(!logged.contains("bringing it back up"), "{logged}");
}

/// The daemon shutting down during the restore's own relaunch ends that process too: the
/// session is down, and nobody is told or logged to, since it is resumed at the next start.
#[tokio::test]
async fn nothing_is_told_of_a_restore_cut_short_by_the_shutdown() {
    let fixture = Fixture::new("auto-switch-shutdown-restore", SessionStatus::Idle);
    let (logs, _guard) = LogBuffer::capture();
    let mut events = fixture.state.subscribe();
    let env = fixture.env();
    let (state, calls) = (fixture.state.clone(), Arc::new(AtomicUsize::new(0)));
    let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
    // The switch's relaunch fails; the restore's starts a process as the daemon begins to shut
    // down, which ends it.
    let launch = move |request: LaunchRequest| -> Result<Launch> {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            anyhow::bail!("the agent cannot be started");
        }
        let stopping = state.clone();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(stopping.stop_all_sessions());
        })
        .join()
        .unwrap();
        stand_in_launch(&started, &launches, || {})(request)
    };

    try_switch(&fixture.state, "w", move || Ok(env), launch)
        .await
        .expect("cut short, and nobody is told");

    assert!(notices(
        &mut events,
        notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
    )
    .is_empty());
    // Only the switch's own failure, which came before the shutdown, is logged.
    let logged = logs.text();
    assert_eq!(logged.matches("WARN").count(), 1, "{logged}");
}

/// A turn that failed on one account is not judged against the account somebody moved the
/// session to while it waited for a claim.
#[tokio::test]
async fn a_trigger_is_dropped_when_the_session_was_moved_during_the_claim_wait() {
    let fixture = Fixture::new("auto-switch-moved-in-claim", SessionStatus::Idle);
    let claim = fixture.state.begin_switch("w").unwrap();
    let (state, dir) = (fixture.state.clone(), fixture.dir.join("claude-b"));

    tokio::join!(fixture.failed_turn(), async {
        // Once every task is idle, the failed turn has found the claim and waits for it to end.
        crate::test_support::runtime_settled().await;
        state
            .set_session_account("w", Some("claude-b"), dir.to_str())
            .unwrap();
        drop(claim);
    });

    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
    assert_eq!(fixture.launched(), 0);
}

/// A second trigger that arrives while a real automatic switch still holds its claim waits for
/// it, and then does not pick the account the first one left.
#[tokio::test]
async fn a_second_trigger_under_a_switchs_claim_does_not_pick_the_account_just_left() {
    let fixture = Fixture::new("auto-switch-second", SessionStatus::Idle);
    fixture.hold(Some("claude-b"), 50.0);

    let second = async {
        // The turn it continues fails once the session is relaunched on its new account, while
        // the claim is still held.
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !(fixture.state.is_launching("w")
            && fixture.worker().account_id.as_deref() == Some("claude-a")
            && fixture
                .state
                .live_session("w")
                .is_some_and(|live| !std::ptr::eq(live.as_ref(), fixture.worker_process.as_ref())))
        {
            assert!(
                Instant::now() < deadline,
                "the switched process did not come up"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        fixture.failed_turn().await;
    };
    tokio::join!(fixture.failed_turn(), second);

    // The default account still reads as having quota, but the first switch left it.
    assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
    assert_eq!(fixture.launched(), 2);
}

/// The owner is not told that a session waits for the user while the dialog on its screen is
/// its agent's usage limit, which is no wait; any other dialog is.
#[tokio::test]
async fn the_owner_is_not_told_of_a_wait_by_a_usage_limit_dialog() {
    const GROK_MODAL: &str = "You hit your weekly limit.\n1 (o) Upgrade tier\n2 (o) Buy more \
                              credits\n3 (o) Try Again\nEnter :submit / Shift+x :dismiss\n";
    let fixture = Fixture::new("auto-switch-limit-dialog", SessionStatus::Idle);
    fixture
        .state
        .store
        .insert_session(&Session {
            project_id: Some("p1".to_string()),
            bound_to: Some("owner".to_string()),
            role: Role::Worker,
            ..crate::test_support::project_session("g", AgentId::GROK, SessionStatus::Idle)
        })
        .unwrap();
    let grok = crate::test_support::fake_live_session("g", AgentId::GROK, 80, 24, "sleep 30");
    fixture.state.register_live(grok.clone());
    grok.on_output(GROK_MODAL.as_bytes());
    let limit = grok.look_for_dialog().current;
    assert!(limit.is_some_and(|dialog| dialog.is_usage_limit()));

    crate::session::dialogs::react_to_dialog(&fixture.state, "g", limit);
    // A notice the reaction set off would be taken on a blocking task, which this waits out.
    crate::test_support::runtime_settled().await;

    assert!(
        fixture.state.claim_waiting_notice("g"),
        "no notice was taken for it"
    );
    let heard = owner_heard(&fixture).await;
    assert!(!heard.contains("is waiting for the user"), "{heard}");

    // Another dialog is a wait, and its owner is told.
    fixture.show_dialog();
    let resume = fixture.worker_process.look_for_dialog().current;
    crate::session::dialogs::react_to_dialog(&fixture.state, "w", resume);
    let heard = output_containing(&fixture.owner, "is waiting for the user");
    assert!(heard.contains("is waiting for the user"), "{heard}");
}

/// A name that cannot be read is replaced by the account's id, or by the standard name of the
/// default account, which has none.
#[test]
fn an_account_name_that_cannot_be_read_falls_back_to_its_id() {
    assert_eq!(fallback_account_name(Some("claude-a")), "claude-a");
    assert_eq!(fallback_account_name(None), DEFAULT_ACCOUNT_NAME);
}

/// Only a refusal that says the time was wrong is silent.
#[test]
fn only_a_refusal_of_the_time_is_silent() {
    for code in [
        error_code::SESSION_ALREADY_STARTING,
        error_code::ACCOUNT_SHARING_IN_PROGRESS,
        error_code::ACCOUNT_DISABLED,
        error_code::ACCOUNT_QUOTA_EXHAUSTED,
        error_code::ACCOUNT_NOT_SIGNED_IN,
        error_code::UNKNOWN_ACCOUNT,
    ] {
        assert!(is_wrong_time(&CodedError::raised(code, "busy", &[])));
    }
    assert!(is_wrong_time(&NoLongerDue("moved").into()));
    assert!(!is_wrong_time(&CodedError::raised(
        error_code::SESSION_ARCHIVED,
        "archived",
        &[]
    )));
    assert!(!is_wrong_time(&anyhow::anyhow!(
        "the agent cannot be started"
    )));
}
