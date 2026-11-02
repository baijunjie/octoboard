use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::accounts::quotas::tests::{key, quota_of, window, NOW};
use crate::protocol::{
    quota_unavailable_code as code, AccountQuota, Agent, Availability, Event, QuotaUnavailable,
    QuotaWindow, QuotaWindowKind,
};
use crate::state::AppState;
use crate::time::now_millis;

#[test]
fn the_wait_before_another_attempt_doubles_with_failures_up_to_a_cap() {
    assert_eq!(wait_after(0), POLL_INTERVAL);
    assert_eq!(wait_after(1), POLL_INTERVAL * 2);
    assert_eq!(wait_after(3), POLL_INTERVAL * 8);
    assert_eq!(wait_after(40), POLL_INTERVAL * 8);
}

/// A scope that leaves out the accounts the user turned off selects the others only, the
/// default account among them, while the default scope selects every account.
#[tokio::test]
async fn a_scope_can_leave_out_the_accounts_the_user_turned_off() {
    let (state, dir) = crate::test_support::app_state("quota-scope-enabled");
    state.set_agent_availability(vec![crate::protocol::AgentAvailability {
        agent: Agent::Codex,
        availability: Availability::Available,
        default_account_dir: None,
    }]);
    for (id, disabled) in [("on", false), ("off", true)] {
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: id.to_string(),
                agent: Agent::Codex,
                name: id.to_string(),
                config_dir: dir.join(id).to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled,
            })
            .unwrap();
    }
    let selected = |scope: Scope| {
        let mut accounts: Vec<Option<String>> = state
            .account_quotas
            .claim_scope(&state, &scope, Instant::now(), Trigger::Poll)
            .unwrap()
            .into_iter()
            .map(|claim| claim.key.account.clone())
            .collect();
        // Let the next selection claim them again.
        state.account_quotas.lock().in_flight.clear();
        accounts.sort();
        accounts
    };

    assert_eq!(
        selected(Scope::default()),
        [None, Some("off".to_string()), Some("on".to_string())]
    );
    assert_eq!(
        selected(Scope {
            enabled_only: true,
            ..Scope::default()
        }),
        [None, Some("on".to_string())]
    );
}

/// A missing directory is reported without running anything, and read for real once it exists.
#[tokio::test]
async fn a_missing_config_directory_is_a_state_found_before_the_cli_is_run() {
    let (state, dir) = crate::test_support::app_state("quota-missing-dir");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(
        bin.join("codex.reply"),
        include_str!("../../../../testdata/codex_rate_limits_reply.json"),
    )
    .unwrap();
    let env = crate::test_support::fake_agent(
        &bin,
        "codex",
        &format!(
            "echo run >> '{d}/runs'\nwhile read line; do case \"$line\" in *rateLimits*) cat '{d}/codex.reply';; esac; done",
            d = bin.display()
        ),
    );
    let shell: ShellSource = Arc::new(move || Ok(env.clone()));
    state.set_agent_availability(vec![crate::protocol::AgentAvailability {
        agent: Agent::Codex,
        availability: Availability::Available,
        default_account_dir: None,
    }]);
    let config_dir = dir.join("codex-work");
    let account = crate::protocol::Account {
        id: "work".to_string(),
        agent: Agent::Codex,
        name: "Work".to_string(),
        config_dir: config_dir.to_string_lossy().into_owned(),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
    };
    state.store.insert_account(&account).unwrap();
    let scope = Scope {
        agent: Some(Agent::Codex),
        account: Some("work".to_string()),
        ..Scope::default()
    };

    refresh_with(&state, &scope, Trigger::Poll, shell.clone()).await;
    let held = state.account_quotas.list();
    let unavailable = held[0].unavailable.as_ref().expect("a state");
    assert_eq!(unavailable.code, code::CONFIG_DIR_MISSING);
    assert_eq!(
        unavailable.params.get("dir").map(String::as_str),
        config_dir.to_str()
    );
    assert!(!bin.join("runs").exists());
    assert!(!config_dir.exists());

    std::fs::create_dir(&config_dir).unwrap();
    state.account_quotas.make_due(Agent::Codex, Some("work"));
    refresh_with(&state, &scope, Trigger::Poll, shell).await;
    assert!(state.account_quotas.list()[0].unavailable.is_none());
    assert!(bin.join("runs").exists());
}

#[test]
fn a_directory_that_cannot_be_judged_missing_is_left_to_the_cli() {
    let dir = crate::test_support::ScratchDir::new("quota-dir-is-a-file");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env = crate::test_support::fake_agent(
        &bin,
        "codex",
        &format!("echo run >> '{}/runs'; exit 1", bin.display()),
    );
    // A regular file where the directory should be is not `NotFound`.
    let file = dir.join("not-a-dir");
    std::fs::write(&file, "x").unwrap();
    let key = Key {
        agent: Agent::Codex,
        account: Some("a".to_string()),
        config_dir: Some(file.to_string_lossy().into_owned()),
    };
    let shell = move || Ok(env.clone());
    let quota = read(&key, &shell, &AtomicBool::new(false)).0;
    assert_ne!(quota.unavailable.unwrap().code, code::CONFIG_DIR_MISSING);
    assert!(bin.join("runs").exists());
}

/// A Codex account directory under `dir` whose `auth.json` carries an ID token for
/// `me@example.com`, and the key of the owned account over it.
fn codex_account_with_email(dir: &crate::test_support::ScratchDir) -> Key {
    use base64::Engine;
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();
    let claims =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"email":"me@example.com"}"#);
    std::fs::write(
        account.join("auth.json"),
        format!(r#"{{"tokens":{{"id_token":"e30.{claims}.sig"}}}}"#),
    )
    .unwrap();
    Key {
        agent: Agent::Codex,
        account: Some("a".to_string()),
        config_dir: Some(account.to_string_lossy().into_owned()),
    }
}

#[test]
fn the_email_is_read_when_the_quota_could_not_be() {
    let dir = crate::test_support::ScratchDir::new("quota-email-failed");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env = crate::test_support::fake_agent(&bin, "codex", "exit 1");
    let key = codex_account_with_email(&dir);
    let shell = move || Ok(env.clone());
    let quota = read(&key, &shell, &AtomicBool::new(false)).0;
    assert_eq!(quota.unavailable.unwrap().code, code::COMMAND_FAILED);
    assert_eq!(quota.email.as_deref(), Some("me@example.com"));
}

#[test]
fn a_signed_out_account_has_no_email_whatever_a_leftover_record_says() {
    let dir = crate::test_support::ScratchDir::new("quota-email-signed-out");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env = crate::test_support::fake_agent(
        &bin,
        "codex",
        r#"while read line; do case "$line" in
*rateLimits*) echo '{"id":2,"error":{"code":-32600,"message":"codex account authentication required to read rate limits"}}';;
*account/read*) echo '{"id":3,"result":{"account":null,"requiresOpenaiAuth":true}}';;
esac; done"#,
    );
    let key = codex_account_with_email(&dir);
    let shell = move || Ok(env.clone());
    let quota = read(&key, &shell, &AtomicBool::new(false)).0;
    assert_eq!(quota.unavailable.unwrap().code, code::NOT_SIGNED_IN);
    assert_eq!(quota.email, None);
}

#[test]
fn a_missing_config_dir_has_no_email() {
    let dir = crate::test_support::ScratchDir::new("quota-email-missing");
    let mut key = codex_account_with_email(&dir);
    std::fs::remove_dir_all(dir.join("account")).unwrap();
    key.account = Some("gone".to_string());
    let shell = || Ok(HashMap::new());
    let quota = read(&key, &shell, &AtomicBool::new(false)).0;
    assert_eq!(quota.unavailable.unwrap().code, code::CONFIG_DIR_MISSING);
    assert_eq!(quota.email, None);
}

#[test]
fn without_a_shell_environment_only_an_owned_account_has_an_email() {
    let dir = crate::test_support::ScratchDir::new("quota-email-no-shell");
    let owned = codex_account_with_email(&dir);
    let shell = || Err(anyhow::anyhow!("no shell"));
    let quota = read(&owned, &shell, &AtomicBool::new(false)).0;
    assert_eq!(
        quota.unavailable.unwrap().code,
        code::SHELL_ENVIRONMENT_FAILED
    );
    assert_eq!(quota.email.as_deref(), Some("me@example.com"));
    // The default account's directory follows the user's own variables, which are not known.
    let default = Key {
        account: None,
        config_dir: None,
        ..owned
    };
    let quota = read(&default, &shell, &AtomicBool::new(false)).0;
    assert_eq!(quota.email, None);
}

/// Runs `refresh_with` over fake Codex and Grok CLIs that count their runs under `dir`.
#[tokio::test]
async fn a_refresh_reads_what_its_scope_selects_once_and_announces_each_reading() {
    let (state, dir) = crate::test_support::app_state("quota-refresh");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let reply = |name: &str, text: &str| std::fs::write(bin.join(name), text).unwrap();
    reply(
        "codex.reply",
        include_str!("../../../../testdata/codex_rate_limits_reply.json"),
    );
    reply(
        "grok.reply",
        include_str!("../../../../testdata/grok_billing_reply.json"),
    );
    let counting = |agent: &str| {
        format!(
            "echo run >> '{d}/{agent}.runs'\nwhile read line; do case \"$line\" in *rateLimits*|*billing*) cat '{d}/{agent}.reply';; esac; done",
            d = bin.display()
        )
    };
    crate::test_support::fake_agent(&bin, "codex", &counting("codex"));
    let env = crate::test_support::fake_agent(&bin, "grok", &counting("grok"));
    let runs = |agent: &str| {
        std::fs::read_to_string(bin.join(format!("{agent}.runs")))
            .map_or(0, |text| text.lines().count())
    };
    let shell: ShellSource = Arc::new(move || Ok(env.clone()));

    state.set_agent_availability(
        [Agent::Codex, Agent::Grok]
            .into_iter()
            .map(|agent| crate::protocol::AgentAvailability {
                agent,
                availability: Availability::Available,
                default_account_dir: None,
            })
            .collect(),
    );
    let codex_work = dir.join("codex-work");
    std::fs::create_dir(&codex_work).unwrap();
    let account = crate::protocol::Account {
        id: "work".to_string(),
        agent: Agent::Codex,
        name: "Work".to_string(),
        config_dir: codex_work.to_string_lossy().into_owned(),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
    };
    state.store.insert_account(&account).unwrap();
    let mut events = state.subscribe();
    let held = |state: &Arc<AppState>| -> Vec<(Agent, Option<String>)> {
        state
            .account_quotas
            .list()
            .into_iter()
            .map(|quota| (quota.agent, quota.account))
            .collect()
    };

    // One account of one agent: only that one is read, and its reading is announced.
    let scope = Scope {
        agent: Some(Agent::Codex),
        account: Some("work".to_string()),
        ..Scope::default()
    };
    refresh_with(&state, &scope, Trigger::Poll, shell.clone()).await;
    assert_eq!(held(&state), [(Agent::Codex, Some("work".to_string()))]);
    assert_eq!((runs("codex"), runs("grok")), (1, 0));
    let mut announced = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::AccountQuotasUpdated { account_quotas } = event {
            announced.push(account_quotas.len());
        }
    }
    assert_eq!(announced, [1]);

    // One agent: its default account joins; the other agent stays unread.
    let scope = Scope {
        agent: Some(Agent::Grok),
        account: None,
        ..Scope::default()
    };
    refresh_with(&state, &scope, Trigger::Poll, shell.clone()).await;
    assert_eq!((runs("codex"), runs("grok")), (1, 1));

    // Everything: only what was never read is read, and what is not available is left out.
    refresh_with(&state, &Scope::default(), Trigger::Poll, shell).await;
    assert_eq!((runs("codex"), runs("grok")), (2, 1));
    assert_eq!(
        held(&state),
        [
            (Agent::Codex, None),
            (Agent::Codex, Some("work".to_string())),
            (Agent::Grok, None),
        ]
    );
    assert!(state
        .account_quotas
        .list()
        .iter()
        .all(|quota| quota.unavailable.is_none()));
}

/// A default Codex account over a fake CLI: its quota requests are answered from `reply`
/// and each run counted in `runs`; `exec` is a kick, logged with its arguments in `kicks`,
/// failing while `kick-fails` exists, not ending while `kick-sleeps` exists, and otherwise
/// leaving `after.reply` as the reply.
struct KickFixture {
    state: Arc<AppState>,
    bin: std::path::PathBuf,
    shell: ShellSource,
    _dir: crate::test_support::ScratchDir,
}

/// Codex's reply with its 5-hour window started or not.
fn codex_reply(started: bool) -> String {
    let reply = include_str!("../../../../testdata/codex_rate_limits_reply.json");
    if started {
        reply.to_string()
    } else {
        reply.replace(r#""resetsAt":1792261723"#, r#""resetsAt":null"#)
    }
}

impl KickFixture {
    fn new(label: &str, setting: bool) -> Self {
        let (state, dir) = crate::test_support::app_state(label);
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let d = bin.display();
        let env = crate::test_support::fake_agent(
            &bin,
            "codex",
            &format!(
                "case \"$1\" in\n\
                 exec) echo \"$*\" >> '{d}/kicks'; [ -e '{d}/kick-fails' ] && exit 1\n\
                 while [ -e '{d}/kick-sleeps' ]; do sleep 0.02; done\n\
                 cp '{d}/after.reply' '{d}/reply';;\n\
                 *) echo run >> '{d}/runs'\n\
                 while [ -e '{d}/probe-sleeps' ]; do sleep 0.02; done\n\
                 while read line; do case \"$line\" in *rateLimits*) cat '{d}/reply';; \
                 esac; done;;\n\
                 esac"
            ),
        );
        state.set_agent_availability(vec![crate::protocol::AgentAvailability {
            agent: Agent::Codex,
            availability: Availability::Available,
            default_account_dir: None,
        }]);
        state.store.set_auto_start_quota_windows(setting).unwrap();
        let fixture = Self {
            state,
            bin,
            shell: Arc::new(move || Ok(env.clone())),
            _dir: dir,
        };
        fixture.serve(&codex_reply(false));
        fixture.after_kick(&codex_reply(true));
        fixture
    }

    fn serve(&self, reply: &str) {
        std::fs::write(self.bin.join("reply"), reply).unwrap();
    }

    fn after_kick(&self, reply: &str) {
        std::fs::write(self.bin.join("after.reply"), reply).unwrap();
    }

    /// Makes the account's last attempt `age` old.
    fn age_attempt(&self, age: Duration) {
        let mut inner = self.state.account_quotas.lock();
        for entry in inner.entries.values_mut() {
            entry.attempted = Instant::now() - age;
        }
    }

    fn count(&self, file: &str) -> usize {
        std::fs::read_to_string(self.bin.join(file)).map_or(0, |text| text.lines().count())
    }

    /// A due reading of the account, with the kick it leads to and the reading after that,
    /// awaited.
    async fn refresh(&self) {
        self.state.account_quotas.make_due(Agent::Codex, None);
        refresh_with(
            &self.state,
            &Scope::default(),
            Trigger::Poll,
            self.shell.clone(),
        )
        .await;
        self.settled().await;
    }

    /// Waits for the work a refresh left in the background: the account is claimed for as long
    /// as a kick or the reading after it is running.
    async fn settled(&self) {
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !self.state.account_quotas.lock().in_flight.is_empty() {
            assert!(Instant::now() < deadline, "the kick did not end");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn session_reset(&self) -> Option<i64> {
        let held = self.state.account_quotas.list();
        held[0].windows[0].resets_at
    }
}

/// A refresh on demand reads an account whose last attempt is over a minute old though it is
/// not due, and starts no window itself, the setting that starts them being on; the poller's
/// next tick does, without the account having to wait for its ordinary interval.
#[tokio::test]
async fn a_refresh_on_demand_rereads_an_account_and_leaves_the_kick_to_the_poller() {
    let fixture = KickFixture::new("quota-demand-reads", true);
    fixture.refresh().await;
    // The poll kicked it, as the setting asks; the reading of that is the baseline.
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
    fixture.serve(&codex_reply(false));
    fixture.age_attempt(Duration::from_secs(61));

    refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
    fixture.settled().await;

    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));
    assert_eq!(fixture.session_reset(), None);
    assert!(fixture
        .state
        .account_quotas
        .is_due_for_test(Agent::Codex, None));

    // The mark does not let a session's request past the limit: two more in a row read nothing.
    for _ in 0..2 {
        refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
    }
    fixture.settled().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));

    // The next poll tick reads the account again, nothing but the mark having made it due, and
    // kicks it.
    refresh_with(
        &fixture.state,
        &Scope::default(),
        Trigger::Poll,
        fixture.shell.clone(),
    )
    .await;
    fixture.settled().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (5, 2));
    assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
}

/// Within a minute of the last attempt the held reading stands, whoever took it.
#[tokio::test]
async fn a_refresh_on_demand_within_a_minute_of_an_attempt_keeps_the_reading() {
    let fixture = KickFixture::new("quota-demand-limited", false);
    fixture.refresh().await;
    assert_eq!(fixture.count("runs"), 1);

    fixture.age_attempt(Duration::from_secs(59));
    refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
    assert_eq!(fixture.count("runs"), 1);

    fixture.age_attempt(Duration::from_secs(61));
    refresh_on_demand(&fixture.state, &Scope::default(), fixture.shell.clone()).await;
    assert_eq!(fixture.count("runs"), 2);
}

#[tokio::test]
async fn a_window_with_no_reset_time_is_left_alone_while_the_setting_is_off() {
    let fixture = KickFixture::new("quota-kick-off", false);
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (1, 0));
    assert_eq!(fixture.session_reset(), None);
}

/// A reading is done once `refresh_with` returns: the kick it calls for goes on without it, so
/// the readings of other accounts are not held up behind a model round trip.
#[tokio::test]
async fn a_refresh_returns_without_waiting_for_the_kick_it_starts() {
    let fixture = KickFixture::new("quota-kick-detached", true);
    std::fs::write(fixture.bin.join("kick-sleeps"), "").unwrap();
    fixture.state.account_quotas.make_due(Agent::Codex, None);
    refresh_with(
        &fixture.state,
        &Scope::default(),
        Trigger::Poll,
        fixture.shell.clone(),
    )
    .await;
    assert_eq!(fixture.session_reset(), None);
    assert!(!fixture.state.account_quotas.lock().in_flight.is_empty());
    std::fs::remove_file(fixture.bin.join("kick-sleeps")).unwrap();
    fixture.settled().await;
    assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
}

#[tokio::test]
async fn a_window_with_no_reset_time_is_kicked_once_and_the_account_read_again() {
    let fixture = KickFixture::new("quota-kick-once", true);
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
    assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
    let kick = std::fs::read_to_string(fixture.bin.join("kicks")).unwrap();
    assert!(kick.starts_with("exec --ephemeral "), "{kick}");
}

#[tokio::test]
async fn a_window_still_with_no_reset_time_after_a_kick_is_not_kicked_again() {
    let fixture = KickFixture::new("quota-kick-still-null", true);
    fixture.after_kick(&codex_reply(false));
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
    fixture.refresh().await;
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (4, 1));
    assert_eq!(fixture.session_reset(), None);
}

#[tokio::test]
async fn a_failed_kick_is_not_retried_within_its_episode() {
    let fixture = KickFixture::new("quota-kick-fails", true);
    std::fs::write(fixture.bin.join("kick-fails"), "").unwrap();
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));
}

#[tokio::test]
async fn a_window_is_kicked_again_once_a_reading_has_shown_its_reset_time() {
    let fixture = KickFixture::new("quota-kick-again", true);
    fixture.after_kick(&codex_reply(false));
    fixture.refresh().await;
    assert_eq!(fixture.count("kicks"), 1);

    // A reading with the reset time ends the episode, so the same window going unstarted again
    // is a new one.
    fixture.serve(&codex_reply(true));
    fixture.refresh().await;
    assert_eq!(fixture.count("kicks"), 1);
    fixture.serve(&codex_reply(false));
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (5, 2));
}

/// A disabled account, the default one included, has no window started on it, and one that is
/// turned back on is kicked again.
#[tokio::test]
async fn a_disabled_account_has_no_window_kicked() {
    let fixture = KickFixture::new("quota-kick-disabled", true);
    fixture
        .state
        .store
        .set_default_account_disabled(Agent::Codex, true)
        .unwrap();
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (1, 0));
    assert!(!fixture
        .state
        .account_quotas
        .is_kicked_for_test(Agent::Codex, None));

    fixture
        .state
        .store
        .set_default_account_disabled(Agent::Codex, false)
        .unwrap();
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (3, 1));
}

#[tokio::test]
async fn turning_the_setting_on_kicks_the_readings_already_held() {
    let fixture = KickFixture::new("quota-kick-held", false);
    fixture.refresh().await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (1, 0));

    kick_held_with(&fixture.state, fixture.shell.clone()).await;
    assert_eq!(fixture.count("kicks"), 0);

    fixture
        .state
        .store
        .set_auto_start_quota_windows(true)
        .unwrap();
    kick_held_with(&fixture.state, fixture.shell.clone()).await;
    assert_eq!((fixture.count("runs"), fixture.count("kicks")), (2, 1));
    assert_eq!(fixture.session_reset(), Some(1_792_261_723_000));
}

/// Holds the default Codex account's unstarted window, claims its kick and runs
/// [`kick_and_read`] on it in the background, with `kick-sleeps` keeping the kick going, until
/// the fake CLI has been started for the kick.
async fn kick_in_progress(fixture: &KickFixture) -> tokio::task::JoinHandle<()> {
    let key = key(Agent::Codex, None);
    let unstarted = AccountQuota {
        windows: vec![window(QuotaWindowKind::Session, 0.0, None)],
        ..quota_of(&key, false)
    };
    let quotas = &fixture.state.account_quotas;
    quotas.recorded(&fixture.state, key.clone(), unstarted);
    std::fs::write(fixture.bin.join("kick-sleeps"), "").unwrap();
    let claim = quotas.claim_kick(&fixture.state, &key).expect("a kick");
    let (state, shell) = (fixture.state.clone(), fixture.shell.clone());
    let running = tokio::spawn(async move { kick_and_read(&state, &shell, claim).await });
    let deadline = Instant::now() + crate::test_support::PATIENCE;
    while fixture.count("kicks") == 0 {
        assert!(Instant::now() < deadline, "the kick did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    running
}

/// A kick whose account was made due (a login ended) while it ran has no claim any more, so
/// the reading after it, which would be discarded, is not taken.
#[tokio::test]
async fn a_kick_whose_claim_was_dropped_is_not_followed_by_a_reading() {
    let fixture = KickFixture::new("quota-kick-dropped", true);
    let running = kick_in_progress(&fixture).await;
    fixture.state.account_quotas.make_due(Agent::Codex, None);
    std::fs::remove_file(fixture.bin.join("kick-sleeps")).unwrap();
    running.await.unwrap();
    assert_eq!(fixture.count("runs"), 0);
}

/// The daemon shutting down stops the CLI of a kick at once and takes no reading after it.
#[tokio::test]
async fn a_shutdown_stops_a_running_kick_and_the_reading_after_it() {
    let fixture = KickFixture::new("quota-kick-shutdown", true);
    let running = kick_in_progress(&fixture).await;
    fixture.state.account_quotas.stop_cli_runs();
    // The fake CLI is still told to sleep: only the cancel can end the kick.
    tokio::time::timeout(crate::test_support::PATIENCE, running)
        .await
        .expect("the kick was not stopped")
        .unwrap();
    assert_eq!(fixture.count("runs"), 0);
}

/// The daemon shutting down stops the CLI of a probe at once and records no reading for it.
#[tokio::test]
async fn a_shutdown_stops_a_running_probe_and_records_nothing() {
    let fixture = KickFixture::new("quota-probe-shutdown", false);
    // The fake CLI never answers while this exists: only the cancel can end the probe.
    std::fs::write(fixture.bin.join("probe-sleeps"), "").unwrap();
    let claim = fixture
        .state
        .account_quotas
        .claim_due(
            vec![key(Agent::Codex, None)],
            Instant::now() + POLL_INTERVAL,
        )
        .pop()
        .expect("a claim");
    let (state, shell) = (fixture.state.clone(), fixture.shell.clone());
    let running = tokio::spawn(async move { read_account(&state, &shell, claim, true).await });
    let deadline = Instant::now() + crate::test_support::PATIENCE;
    while fixture.count("runs") == 0 {
        assert!(Instant::now() < deadline, "the probe did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    fixture.state.account_quotas.stop_cli_runs();
    let next = tokio::time::timeout(crate::test_support::PATIENCE, running)
        .await
        .expect("the probe was not stopped")
        .unwrap();
    assert!(next.is_none());
    assert!(fixture.state.account_quotas.lock().entries.is_empty());
}

/// A kick claimed or started once the daemon is shutting down is refused, so its CLI is never
/// spawned, and no window counts as kicked.
#[tokio::test]
async fn no_kick_is_started_once_the_daemon_is_shutting_down() {
    let fixture = KickFixture::new("quota-kick-refused", true);
    let key = key(Agent::Codex, None);
    let unstarted = AccountQuota {
        windows: vec![window(QuotaWindowKind::Session, 0.0, None)],
        ..quota_of(&key, false)
    };
    let quotas = &fixture.state.account_quotas;
    quotas.recorded(&fixture.state, key.clone(), unstarted);
    quotas.stop_cli_runs();
    assert!(quotas.claim_kick(&fixture.state, &key).is_none());
    let shell_asked = AtomicBool::new(false);
    let shell = || {
        shell_asked.store(true, Ordering::Relaxed);
        (*fixture.shell)()
    };
    let err = start_windows(&key, &shell, &AtomicBool::new(true)).unwrap_err();
    assert!(err.to_string().contains("before it started"), "{err}");
    assert!(!shell_asked.load(Ordering::Relaxed));
    assert_eq!(fixture.count("kicks"), 0);
    // Nothing was counted as kicked, so the window is claimed once the flag is clear again.
    quotas.stopping.store(false, Ordering::Relaxed);
    assert!(quotas.claim_kick(&fixture.state, &key).is_some());
}

/// A kick would make the CLI recreate an account's directory the user deleted, so it is not
/// run; the reading after it is what records the directory as missing.
#[test]
fn an_account_whose_directory_is_gone_is_not_kicked() {
    let fixture = KickFixture::new("quota-kick-dir-missing", true);
    let gone = fixture.bin.join("deleted-account");
    let key = Key::of(Agent::Codex, Some("a"), gone.to_str());
    let err = start_windows(&key, &*fixture.shell, &AtomicBool::new(false)).unwrap_err();
    assert!(err.to_string().contains("does not exist"), "{err}");
    assert_eq!(fixture.count("kicks"), 0);
    assert!(matches!(
        read(&key, &*fixture.shell, &AtomicBool::new(false))
            .0
            .unavailable,
        Some(unavailable) if unavailable.code == code::CONFIG_DIR_MISSING
    ));
}

#[test]
fn an_account_made_due_is_claimed_at_once_whatever_the_clock_says() {
    let state = crate::test_support::app_state("quota-make-due").0;
    let quotas = &state.account_quotas;
    let codex = key(Agent::Codex, Some("a"));
    quotas.recorded(
        &state,
        codex.clone(),
        unavailable_quota(
            &codex,
            QuotaUnavailable::new(code::COMMAND_FAILED, "failed", &[]),
        ),
    );
    // It was just read, and failed, so it waits.
    let now = Instant::now();
    assert!(quotas.claim_due(vec![codex.clone()], now).is_empty());
    quotas.make_due(Agent::Codex, Some("a"));
    assert_eq!(quotas.claim_due(vec![codex.clone()], now).len(), 1);
}

#[test]
fn an_account_is_due_once_a_held_window_has_reset_and_the_grace_has_passed() {
    let (state, _dir) = crate::test_support::app_state("quota-reset");
    let quotas = &state.account_quotas;
    let key = key(Agent::Codex, Some("a"));
    let mut held = quota_of(&key, false);
    held.windows = vec![window(QuotaWindowKind::Session, 100.0, Some(NOW))];
    quotas.recorded(&state, key.clone(), held);
    let set_attempt = |at: i64| {
        quotas.lock().entries.get_mut(&key).unwrap().attempted_at = at;
    };
    let grace = RESET_GRACE.as_millis() as i64;
    let now = Instant::now();
    let due = |wall: i64| quotas.claim_due_at(vec![key.clone()], now, wall).len();

    set_attempt(NOW - 1_000);
    assert_eq!(due(NOW + grace - 1), 0);
    assert_eq!(due(NOW + grace), 1);

    // A reset that came before the last attempt was already seen by it.
    set_attempt(NOW + 1);
    assert_eq!(due(NOW + grace + 1), 0);
}

#[test]
fn a_failed_re_read_after_a_reset_falls_back_to_the_ordinary_wait() {
    let (state, _dir) = crate::test_support::app_state("quota-reset-failed");
    let quotas = &state.account_quotas;
    let key = key(Agent::Codex, Some("a"));
    let mut held = quota_of(&key, false);
    held.windows = vec![window(
        QuotaWindowKind::Session,
        100.0,
        Some(now_millis() - 60_000),
    )];
    quotas.recorded(&state, key.clone(), held);
    quotas.lock().entries.get_mut(&key).unwrap().attempted_at = now_millis() - 120_000;
    let claims = quotas.claim_due(vec![key.clone()], Instant::now());
    assert_eq!(claims.len(), 1);

    // The re-read fails, keeping the old windows: it began after the reset, so no refire.
    drop(claims);
    quotas.recorded(&state, key.clone(), quota_of(&key, true));
    assert!(quotas.claim_due(vec![key], Instant::now()).is_empty());
}

#[test]
fn only_accounts_never_read_or_past_their_wait_are_due_and_a_running_one_is_not_claimed_twice() {
    let (state, _dir) = crate::test_support::app_state("quota-due");
    let quotas = AccountQuotas::default();
    let (fresh, failing, new) = (
        key(Agent::Claude, None),
        key(Agent::Codex, Some("a")),
        key(Agent::Grok, None),
    );
    quotas.recorded(&state, fresh.clone(), quota_of(&fresh, false));
    quotas.recorded(&state, failing.clone(), quota_of(&failing, true));
    let all = vec![fresh.clone(), failing.clone(), new.clone()];

    let due = |quotas: &AccountQuotas, now: Instant| -> Vec<Key> {
        let claims = quotas.claim_due(all.clone(), now);
        claims.iter().map(|claim| claim.key.clone()).collect()
    };
    let now = Instant::now();
    let first = quotas.claim_due(all.clone(), now);
    assert_eq!(first[0].key, new);
    assert_eq!(first.len(), 1);
    // Claimed once; not again while its probe runs.
    assert!(due(&quotas, now).is_empty());

    // After the interval the healthy account is due; the failing one waits twice as long.
    let later = now + POLL_INTERVAL + Duration::from_secs(1);
    let second = quotas.claim_due(all.clone(), later);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].key, fresh);
    // `fresh` is running now, so only the failing one can be claimed.
    let much_later = now + POLL_INTERVAL * 2 + Duration::from_secs(1);
    assert_eq!(due(&quotas, much_later), [failing]);

    // A claim that is dropped without finishing releases its account.
    drop(first);
    assert_eq!(due(&quotas, now).len(), 1);
}

#[test]
fn a_probe_running_for_a_forgotten_or_repointed_account_cannot_put_its_reading_back() {
    let (state, _dir) = crate::test_support::app_state("quota-forgotten");
    let mut events = state.subscribe();
    let quotas = &state.account_quotas;
    let (deleted, repointed) = (key(Agent::Codex, Some("a")), key(Agent::Codex, Some("b")));
    let now = Instant::now();
    let claims = quotas.claim_due(vec![deleted.clone(), repointed.clone()], now);
    assert_eq!(claims.len(), 2);

    quotas.forget_account(&state, "a");
    // The repointed account's old key is no longer among the targets.
    quotas.change(&state, |inner| {
        retain_locked(inner, &[key(Agent::Codex, None)])
    });
    for claim in claims {
        let quota = quota_of(&claim.key, false);
        quotas.finish(&state, claim, quota);
    }
    assert!(quotas.list().is_empty());
    assert!(events.try_recv().is_err());
    // Nothing is left marked as running either.
    assert_eq!(quotas.claim_due(vec![deleted, repointed], now).len(), 2);
}

fn no_windows(key: &Key) -> AccountQuota {
    AccountQuota {
        unavailable: Some(QuotaUnavailable::new(
            code::NO_WINDOWS,
            "none",
            &[("agent", "Claude Code")],
        )),
        ..quota_of(key, false)
    }
}

fn with_windows(key: &Key) -> AccountQuota {
    AccountQuota {
        fetched_at: 7,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Session,
            label: None,
            used_percent: 15.0,
            resets_at: None,
            window_minutes: None,
        }],
        ..quota_of(key, false)
    }
}

#[test]
fn no_quota_to_report_is_not_backed_off_but_still_keeps_the_windows_it_held() {
    let (state, _dir) = crate::test_support::app_state("quota-no-windows");
    let quotas = &state.account_quotas;
    let key = key(Agent::Claude, None);
    for _ in 0..3 {
        quotas.recorded(&state, key.clone(), no_windows(&key));
    }
    let later = Instant::now() + POLL_INTERVAL + Duration::from_secs(1);
    assert_eq!(quotas.claim_due(vec![key.clone()], later).len(), 1);

    let good = with_windows(&key);
    quotas.recorded(&state, key.clone(), good.clone());
    quotas.recorded(&state, key.clone(), no_windows(&key));
    let held = &quotas.list()[0];
    assert_eq!(held.windows, good.windows);
    assert_eq!(
        held.unavailable.as_ref().map(|u| u.code),
        Some(code::NO_WINDOWS)
    );
}

#[test]
fn a_request_reads_an_account_that_may_have_signed_in_since_early_but_not_within_the_minimum() {
    let (state, _dir) = crate::test_support::app_state("quota-request-reread");
    let quotas = &state.account_quotas;
    let (signed_out, none, windows, failing, missing) = (
        key(Agent::Claude, None),
        key(Agent::Claude, Some("a")),
        key(Agent::Codex, None),
        key(Agent::Codex, Some("b")),
        key(Agent::Grok, None),
    );
    let not_signed_in = AccountQuota {
        unavailable: Some(QuotaUnavailable::new(code::NOT_SIGNED_IN, "no", &[])),
        ..quota_of(&signed_out, false)
    };
    quotas.recorded(&state, signed_out.clone(), not_signed_in);
    quotas.recorded(&state, none.clone(), no_windows(&none));
    // Stale windows beside a NO_WINDOWS reason: there is something to show.
    quotas.recorded(&state, windows.clone(), with_windows(&windows));
    quotas.recorded(&state, windows.clone(), no_windows(&windows));
    quotas.recorded(&state, failing.clone(), quota_of(&failing, true));
    let dir_missing = AccountQuota {
        unavailable: Some(QuotaUnavailable::new(code::CONFIG_DIR_MISSING, "gone", &[])),
        ..quota_of(&missing, false)
    };
    quotas.recorded(&state, missing.clone(), dir_missing);
    let all = vec![signed_out.clone(), none.clone(), windows, failing, missing];
    let keys =
        |claims: &[Claim]| -> Vec<Key> { claims.iter().map(|claim| claim.key.clone()).collect() };

    let now = Instant::now();
    // Just attempted: nothing is claimed, for a request either.
    assert!(quotas.claim_requested(all.clone(), now).is_empty());
    // Past the minimum but not the interval: only the accounts that may have signed in since, and only on request.
    let soon = now + MIN_REREAD_INTERVAL + Duration::from_secs(1);
    assert!(quotas.claim_due(all.clone(), soon).is_empty());
    let claimed = quotas.claim_requested(all.clone(), soon);
    assert_eq!(keys(&claimed), [signed_out, none]);
    // Those are running now, so a second request starts nothing.
    assert!(quotas.claim_requested(all, soon).is_empty());
}

#[test]
fn an_account_state_drops_stale_windows_is_not_backed_off_and_a_login_makes_it_due() {
    let (state, _dir) = crate::test_support::app_state("quota-account-state");
    let key = key(Agent::Codex, Some("a"));
    let good = AccountQuota {
        fetched_at: 7,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Session,
            label: None,
            used_percent: 15.0,
            resets_at: None,
            window_minutes: None,
        }],
        ..quota_of(&key, false)
    };
    let signed_out = AccountQuota {
        unavailable: Some(QuotaUnavailable::new(
            code::NOT_SIGNED_IN,
            "no",
            &[("agent", "Codex")],
        )),
        ..quota_of(&key, false)
    };
    state.account_quotas.recorded(&state, key.clone(), good);
    state
        .account_quotas
        .recorded(&state, key.clone(), signed_out.clone());
    // Unlike a failure, the account's state replaces what it held.
    let held = state.account_quotas.list();
    assert!(held[0].windows.is_empty());
    assert_eq!(
        held[0].unavailable.as_ref().map(|u| u.code),
        Some(code::NOT_SIGNED_IN)
    );
    // And is read again after the ordinary interval, however long it has been so.
    state
        .account_quotas
        .recorded(&state, key.clone(), signed_out);
    let later = Instant::now() + POLL_INTERVAL + Duration::from_secs(1);
    assert_eq!(
        state
            .account_quotas
            .claim_due(vec![key.clone()], later)
            .len(),
        1
    );

    // A login that ended makes it due at once, and a probe running meanwhile is discarded.
    let running = state
        .account_quotas
        .claim_due(vec![key.clone()], Instant::now() + POLL_INTERVAL * 100);
    assert_eq!(running.len(), 1);
    state.account_quotas.make_due(Agent::Codex, Some("a"));
    let again = state
        .account_quotas
        .claim_due(vec![key.clone()], Instant::now());
    assert_eq!(again.len(), 1);
    state.account_quotas.finish(
        &state,
        running.into_iter().next().unwrap(),
        quota_of(&key, false),
    );
    assert!(state.account_quotas.list()[0].unavailable.is_some());
}

/// An account that is being read, whose reading failed or holds no windows, or that is gone
/// is not kicked, and its episodes go with it.
#[test]
fn only_a_successful_reading_of_an_idle_account_with_an_unstarted_window_is_kicked() {
    let (state, _dir) = crate::test_support::app_state("quota-kick-claims");
    state.store.set_auto_start_quota_windows(true).unwrap();
    let quotas = AccountQuotas::default();
    let key = key(Agent::Codex, Some("a"));
    let unstarted = AccountQuota {
        windows: vec![window(QuotaWindowKind::Session, 0.0, None)],
        ..quota_of(&key, false)
    };

    assert!(quotas.claim_kick(&state, &key).is_none());
    quotas.recorded(&state, key.clone(), quota_of(&key, true));
    assert!(quotas.claim_kick(&state, &key).is_none());
    quotas.recorded(&state, key.clone(), quota_of(&key, false));
    assert!(quotas.claim_kick(&state, &key).is_none());

    quotas.recorded(&state, key.clone(), unstarted.clone());
    let running = quotas.claim_due(vec![key.clone()], Instant::now() + POLL_INTERVAL);
    assert!(quotas.claim_kick(&state, &key).is_none());
    drop(running);
    let claim = quotas.claim_kick(&state, &key).expect("a kick");
    assert!(quotas.claim_kick(&state, &key).is_none());
    drop(claim);
    assert!(quotas.claim_kick(&state, &key).is_none());

    quotas.forget_account(&state, "a");
    quotas.recorded(&state, key.clone(), unstarted);
    assert!(quotas.claim_kick(&state, &key).is_some());
}

/// A kick goes to one cheap model, so it cannot start the window of another model.
#[test]
fn a_models_own_weekly_window_with_no_reset_time_is_not_kicked() {
    let (state, _dir) = crate::test_support::app_state("quota-kick-weekly-model");
    state.store.set_auto_start_quota_windows(true).unwrap();
    let quotas = AccountQuotas::default();
    let key = key(Agent::Claude, None);
    let mut model = window(QuotaWindowKind::WeeklyModel, 0.0, None);
    model.label = Some("Fable".to_string());
    let mut reading = quota_of(&key, false);
    reading.windows = vec![model, window(QuotaWindowKind::Weekly, 40.0, Some(NOW))];
    quotas.recorded(&state, key.clone(), reading.clone());
    assert!(quotas.claim_kick(&state, &key).is_none());

    reading.windows[1] = window(QuotaWindowKind::Session, 0.0, None);
    quotas.recorded(&state, key.clone(), reading);
    assert!(quotas.claim_kick(&state, &key).is_some());
}
