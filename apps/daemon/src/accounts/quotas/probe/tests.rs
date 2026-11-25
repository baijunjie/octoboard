use super::test_support::{
    answering_agent, answering_agent_then, fake, fixture_of, probe_of, run_probe, unavailable_code,
};
use super::*;
use crate::test_support::{until_the_shell_ran, ScratchDir, PATIENCE};
use serde_json::json;
use std::path::PathBuf;

/// Long enough for a fake agent to start on a loaded machine, short enough to wait out.
const SHORT: Duration = Duration::from_millis(700);

/// The providers that read a quota by asking their CLI; the others have nothing to probe.
fn probing_providers() -> impl Iterator<Item = AgentId> {
    AgentId::ALL.into_iter().filter(|agent| {
        provider(*agent)
            .accounts()
            .and_then(|ops| ops.quota_probe())
            .is_some()
    })
}

#[test]
fn a_provider_with_no_quota_probe_fails_without_running_anything() {
    let dir = ScratchDir::new("quota-no-probe");
    let ran = dir.join("ran");
    let env = fake(&dir, AgentId::TEST, &format!("touch '{}'", ran.display()));
    let unavailable = probe_of(AgentId::TEST, &env).expect_err("nothing to read");
    assert_eq!(unavailable.code, code::COMMAND_FAILED);
    assert!(!ran.exists(), "the CLI was not run");
}

/// Whether `agent`'s provider makes its reading against a directory of Octoboard's own rather than
/// the account's (`AccountOps::run_home`), asked under a root of the question's own.
fn reads_in_a_home_of_its_own(
    agent: AgentId,
    account: &Path,
    shell_env: &HashMap<String, String>,
) -> bool {
    let asked = ScratchDir::new("quota-run-home-asked");
    let Some(ops) = provider(agent).accounts() else {
        return false;
    };
    let wait = RunWait {
        until: Instant::now() + crate::test_support::PATIENCE,
        cancel: None,
    };
    ops.run_home(
        &asked,
        BackgroundRun::QuotaReading,
        account,
        shell_env,
        &wait,
    )
    .expect("the provider's answer")
    .is_some()
}

/// The agent's own variable names the account's directory to the CLI, or, for a provider that
/// reads against a directory of Octoboard's own, one under the run homes and never the account's.
#[test]
fn an_account_directory_is_exported_under_the_agents_own_variable() {
    for agent in probing_providers().filter(|agent| agent.descriptor().capabilities.owned_accounts)
    {
        let dir = ScratchDir::new("quota-dir");
        let env = answering_agent(&dir, agent, fixture_of(agent).reply);
        let account_dir = dir.join("account");
        std::fs::create_dir(&account_dir).unwrap();
        let run_homes = dir.join("run-homes");
        let own_home = reads_in_a_home_of_its_own(agent, &account_dir, &env);
        let given = || {
            let given = std::fs::read_to_string(dir.join("dir")).unwrap();
            PathBuf::from(given.trim())
        };
        super::probe(
            agent,
            &env,
            Some(&account_dir),
            &run_homes,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
            &mut ProbeTrace::default(),
        )
        .expect("a readable reply");
        if own_home {
            assert!(given().starts_with(&run_homes), "{agent:?}: {:?}", given());
            assert!(
                !given().starts_with(&account_dir),
                "{agent:?}: {:?}",
                given()
            );
        } else {
            assert_eq!(given(), account_dir, "{agent:?}");
        }
        let of_the_account = given();
        // The default account is left to the shell, which here exports nothing; a provider with a
        // directory of its own names one for the default account as well, another than the
        // pinned account's.
        super::probe(
            agent,
            &env,
            None,
            &run_homes,
            crate::test_support::PATIENCE,
            &AtomicBool::new(false),
            &mut ProbeTrace::default(),
        )
        .expect("a readable reply");
        if own_home {
            assert!(given().starts_with(&run_homes), "{agent:?}: {:?}", given());
            assert_ne!(given(), of_the_account, "{agent:?}");
        } else {
            assert_eq!(given(), PathBuf::new(), "{agent:?}");
        }
    }
}

#[test]
fn a_reply_that_is_not_in_a_known_shape_is_unreadable() {
    for agent in probing_providers() {
        let reply = fixture_of(agent).unreadable_reply;
        let dir = ScratchDir::new("quota-unreadable");
        let env = answering_agent(&dir, agent, &format!("{reply}\n"));
        assert_eq!(
            unavailable_code(run_probe(
                agent,
                &env,
                None,
                PATIENCE,
                &mut ProbeTrace::default()
            )),
            code::UNREADABLE_REPLY,
            "{agent:?}"
        );
    }
}

#[test]
fn a_cli_that_never_replies_times_out() {
    for agent in probing_providers() {
        // Answers nothing, as a CLI waiting on a login would; the marker shows it started,
        // since a fake killed before it ran proves nothing.
        let got = until_the_shell_ran(|| {
            let dir = ScratchDir::new("quota-timeout");
            let env = fake(
                &dir,
                agent,
                &format!(
                    "echo started > '{}/started'; exec /bin/sleep 300",
                    dir.display()
                ),
            );
            let result = run_probe(agent, &env, None, SHORT, &mut ProbeTrace::default());
            dir.join("started")
                .exists()
                .then(|| unavailable_code(result))
        });
        assert_eq!(got, code::TIMED_OUT, "{agent:?}");
    }
}

#[test]
fn a_cli_that_exits_without_replying_fails_with_what_it_wrote() {
    for agent in probing_providers() {
        let dir = ScratchDir::new("quota-exit");
        let env = fake(&dir, agent, "echo 'not logged in' >&2; exit 1");
        let unavailable = run_probe(agent, &env, None, PATIENCE, &mut ProbeTrace::default())
            .expect_err("no reply");
        assert_eq!(unavailable.code, code::COMMAND_FAILED, "{agent:?}");
        assert!(
            unavailable.params["detail"].ends_with("not logged in"),
            "{unavailable:?}"
        );
    }
}

/// A read admitted after the account's directory was moved (a removal held it back) runs
/// nothing, which would create the directory again, and reports it missing.
#[test]
fn a_read_admitted_after_the_directory_was_moved_runs_nothing() {
    for agent in probing_providers().filter(|agent| agent.descriptor().capabilities.owned_accounts)
    {
        let dir = ScratchDir::new("quota-probe-moved");
        let ran = dir.join("ran");
        let env = fake(&dir, agent, &format!("touch '{}'", ran.display()));
        let account = dir.join("account");
        let unavailable = run_probe(
            agent,
            &env,
            Some(&account),
            PATIENCE,
            &mut ProbeTrace::default(),
        )
        .expect_err("no reading");
        assert_eq!(unavailable.code, code::CONFIG_DIR_MISSING, "{agent:?}");
        assert!(!ran.exists(), "{agent:?}: the CLI was not run");
        assert!(!account.exists());
    }
}

#[test]
fn a_missing_binary_is_reported_as_such() {
    for agent in probing_providers() {
        let dir = ScratchDir::new("quota-missing");
        let env = HashMap::from([
            ("PATH".to_string(), dir.to_string_lossy().into_owned()),
            ("HOME".to_string(), dir.to_string_lossy().into_owned()),
        ]);
        assert_eq!(
            unavailable_code(run_probe(
                agent,
                &env,
                None,
                SHORT,
                &mut ProbeTrace::default()
            )),
            code::BINARY_NOT_FOUND,
            "{agent:?}"
        );
    }
}

#[test]
fn a_program_that_ignores_the_end_of_its_input_is_killed_and_the_trace_says_how_long_after() {
    for agent in probing_providers() {
        let dir = ScratchDir::new("quota-ignores-eof");
        let env = answering_agent_then(&dir, agent, fixture_of(agent).reply, "exec /bin/sleep 300");
        let mut trace = ProbeTrace::default();
        let reading = run_probe(agent, &env, None, Duration::from_millis(3500), &mut trace);
        assert!(reading.is_ok(), "{agent:?}: {reading:?}");
        let stopped = trace.stopped.expect("the program was stopped");
        assert!(
            stopped.starts_with("stopped ")
                && stopped.ends_with(" after its reply: it did not exit in time"),
            "{stopped}"
        );
        assert!(trace.pid.is_some());
    }
}

#[test]
fn an_error_line_besides_the_reply_is_kept_redacted_in_the_notes() {
    for agent in probing_providers() {
        let Some(error_line) = fixture_of(agent).error_line else {
            continue;
        };
        let accepted = provider(agent)
            .accounts()
            .and_then(|ops| ops.quota_probe())
            .expect("a probing provider")
            .is_error_like(&serde_json::from_str(error_line).expect("a JSON line"));
        assert!(
            accepted,
            "{agent:?}: its own probe does not count {error_line}"
        );
        let dir = ScratchDir::new("quota-error-line");
        std::fs::write(dir.join("reply"), fixture_of(agent).reply).expect("reply");
        let env = fake(
            &dir,
            agent,
            &format!(
                "while read line; do case \"$line\" in *'{marker}'*) \
                 echo '{error_line}'; \
                 echo 'a plain line'; cat '{d}/reply';; esac; done",
                marker = fixture_of(agent).request_marker.expect("a stdin request"),
                d = dir.display()
            ),
        );
        let mut trace = ProbeTrace::default();
        run_probe(agent, &env, None, PATIENCE, &mut trace).expect("a readable reply");
        assert_eq!(trace.notes.len(), 1, "{agent:?}: {:?}", trace.notes);
        assert!(trace.notes[0].contains("refresh failed"));
        assert!(
            !trace.notes[0].contains("me@example.com"),
            "{:?}",
            trace.notes
        );
        assert_eq!(trace.stopped, None);
    }
}

/// A probe that declares nothing beyond what every probe must.
struct StandInProbe;

impl QuotaProbe for StandInProbe {
    fn args(&self) -> &'static [&'static str] {
        &[]
    }

    fn input(&self) -> Vec<String> {
        Vec::new()
    }

    fn is_reply(&self, _value: &Value) -> bool {
        false
    }

    fn read(&self, _reply: &Value, _companion: Option<&Value>) -> Result<Reading, ReplyProblem> {
        Err(ReplyProblem::Unreadable)
    }

    fn signed_out_reason(&self, _reply: Option<&Value>, _companion: Option<&Value>) -> String {
        String::new()
    }

    fn test_fixture(&self) -> ProbeFixture {
        ProbeFixture {
            request_marker: None,
            reply: "",
            unreadable_reply: "",
            error_line: None,
        }
    }
}

/// A probe that does not say which of its lines are errors counts none, whatever a line holds.
#[test]
fn a_probe_that_declares_no_error_lines_counts_none() {
    for line in [
        json!({"type": "error", "message": "refused"}),
        json!({"type": "auth_status"}),
        json!({"error": {"message": "refused"}}),
    ] {
        assert!(!StandInProbe.is_error_like(&line), "{line}");
        assert!(marks_error(&line), "{line}");
    }
}

/// What a probe that overrides nothing gets: its own arguments as the account, one message per
/// line, and its whole input written at the start.
#[test]
fn a_probe_that_overrides_nothing_runs_as_the_account_over_lines_in_one_step() {
    let launch = StandInProbe
        .launch(&ProbeRun {
            account_dir: None,
            own_dir: None,
        })
        .expect("a launch");
    assert_eq!(launch, ProbeLaunch::as_account(&[]));
    assert_eq!(StandInProbe.framing(), Framing::Lines);
    assert_eq!(StandInProbe.steps(), vec![Vec::<String>::new()]);
    assert!(!StandInProbe.advances(&json!({"id": 1, "result": {}})));
}

/// A probe no registered provider has: its CLI runs in a directory of Octoboard's own, takes an
/// argument and a variable made for the run, frames its messages by length, and answers a second
/// request that is written only once the first is answered.
struct OwnDirProbe;

impl QuotaProbe for OwnDirProbe {
    fn args(&self) -> &'static [&'static str] {
        &["serve"]
    }

    fn input(&self) -> Vec<String> {
        Vec::new()
    }

    fn launch(&self, run: &ProbeRun<'_>) -> Result<ProbeLaunch, String> {
        let account = run.account_dir.ok_or("the test probes a pinned account")?;
        if !account.join("signed-in").exists() {
            return Err("the account's directory names no sign-in".to_string());
        }
        let home = run.own_dir.ok_or("the quota home was not prepared")?;
        Ok(ProbeLaunch {
            args: vec![
                "serve".to_string(),
                format!("--log-dir={}", home.join("logs").display()),
            ],
            env: vec![(
                "OCTOBOARD_TEST_PROBE_CACHE".to_string(),
                "own-cache".to_string(),
            )],
        })
    }

    fn framing(&self) -> Framing {
        Framing::ContentLength
    }

    fn steps(&self) -> Vec<Vec<String>> {
        vec![
            vec![json!({"id": 1, "method": "session.create"}).to_string()],
            vec![json!({"id": 2, "method": "session.quota"}).to_string()],
        ]
    }

    fn advances(&self, value: &Value) -> bool {
        json_rpc::is_reply_to(value, 1)
    }

    fn is_reply(&self, value: &Value) -> bool {
        is_second_response(value)
    }

    fn read(&self, reply: &Value, _companion: Option<&Value>) -> Result<Reading, ReplyProblem> {
        let used = reply
            .pointer("/result/used")
            .and_then(Value::as_f64)
            .ok_or(ReplyProblem::Unreadable)?;
        Ok(Reading {
            plan: None,
            windows: vec![QuotaWindow {
                kind: crate::protocol::QuotaWindowKind::CreditPeriod,
                label: None,
                used_percent: used,
                resets_at: None,
                window_minutes: None,
            }],
            reset_credits: None,
        })
    }

    fn signed_out_reason(&self, _reply: Option<&Value>, _companion: Option<&Value>) -> String {
        String::new()
    }

    fn test_fixture(&self) -> ProbeFixture {
        StandInProbe.test_fixture()
    }
}

/// A fake CLI for [`OwnDirProbe`]: records its arguments, the directory variable and the added
/// variable, then answers two length-framed requests in turn, recording each as it reads it.
fn own_dir_agent(dir: &Path) -> HashMap<String, String> {
    let var = crate::providers::layout::layout(AgentId::TEST).config_dir_var;
    let mut env = fake(
        dir,
        AgentId::TEST,
        &format!(
            "echo \"$*\" > '{d}/args'; echo \"${var}\" > '{d}/dir'\n\
             echo \"$OCTOBOARD_TEST_PROBE_CACHE\" > '{d}/cache'\n\
             cr=$(printf '\\r')\n\
             take() {{ IFS=': ' read -r name length; length=${{length%\"$cr\"}}; read -r blank\n\
             body=$(dd bs=1 count=\"$length\" 2>/dev/null); echo \"$body\" >> '{d}/input'; }}\n\
             say() {{ printf 'Content-Length: %d\\r\\n\\r\\n%s' \"${{#1}}\" \"$1\"; }}\n\
             take; say '{{\"id\":1,\"result\":{{}}}}'\n\
             take; say '{{\"method\":\"notice\"}}'; say '{{\"id\":2,\"result\":{{\"used\":40}}}}'\n\
             cat > /dev/null",
            d = dir.display(),
        ),
    );
    env.insert("OCTOBOARD_TEST_QUOTA_HOME".to_owned(), "1".to_owned());
    env
}

/// Runs [`OwnDirProbe`] for `account` with the probes' own directories under `dir/probes`, which
/// stands for the one under Octoboard's data directory, where a test must not write.
fn run_own_dir_probe(
    dir: &Path,
    account: &Path,
    env: &HashMap<String, String>,
) -> Result<Reading, QuotaUnavailable> {
    probe_with(
        AgentId::TEST,
        Probing {
            conversation: &OwnDirProbe,
            probes_dir: &dir.join("probes"),
        },
        env,
        Some(account),
        PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
}

#[test]
fn a_probe_run_in_a_directory_of_its_own_exports_that_directory_and_leaves_the_accounts_alone() {
    let dir = ScratchDir::new("quota-own-dir");
    let env = own_dir_agent(&dir);
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();
    std::fs::write(account.join("signed-in"), "").unwrap();

    let reading = run_own_dir_probe(&dir, &account, &env).expect("a readable reply");

    assert_eq!(reading.windows.len(), 1);
    assert_eq!(reading.windows[0].used_percent, 40.0);
    let recorded = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .unwrap()
            .trim()
            .to_string()
    };
    // The directory the probe was handed for this account, which it made and ran its CLI in.
    let own = PathBuf::from(recorded("dir"));
    assert!(own.starts_with(dir.join("probes")), "{own:?}");
    assert!(!own.exists(), "the probe home is removed after the run");
    assert_eq!(
        recorded("args"),
        format!("serve --log-dir={}", own.join("logs").display())
    );
    assert_eq!(recorded("cache"), "own-cache");
    // Both requests arrived whole under their headers, the second after the first's answer.
    assert_eq!(
        recorded("input"),
        "{\"id\":1,\"method\":\"session.create\"}\n{\"id\":2,\"method\":\"session.quota\"}"
    );
    let left: Vec<_> = std::fs::read_dir(&account)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        left,
        ["signed-in"],
        "nothing was added to the account's directory"
    );
}

#[test]
fn a_launch_the_probe_refuses_fails_the_reading_without_running_anything() {
    let dir = ScratchDir::new("quota-own-dir-refused");
    let env = own_dir_agent(&dir);
    let account = dir.join("account");
    std::fs::create_dir(&account).unwrap();

    let unavailable = run_own_dir_probe(&dir, &account, &env).expect_err("no reading");

    assert_eq!(unavailable.code, code::COMMAND_FAILED);
    assert!(
        unavailable.params["detail"].contains("names no sign-in"),
        "{unavailable:?}"
    );
    assert!(!dir.join("args").exists(), "the CLI was not run");
    assert_eq!(std::fs::read_dir(dir.join("probes")).unwrap().count(), 0);
}

/// The account's directory is still what is admitted: a probe that runs elsewhere is not run for
/// an account whose directory is gone.
#[test]
fn a_probe_run_in_a_directory_of_its_own_is_not_run_for_an_account_whose_directory_is_gone() {
    struct AnywhereProbe;
    impl QuotaProbe for AnywhereProbe {
        fn args(&self) -> &'static [&'static str] {
            &[]
        }
        fn input(&self) -> Vec<String> {
            Vec::new()
        }
        fn is_reply(&self, _value: &Value) -> bool {
            true
        }
        fn read(&self, _: &Value, _: Option<&Value>) -> Result<Reading, ReplyProblem> {
            Err(ReplyProblem::Unreadable)
        }
        fn signed_out_reason(&self, _: Option<&Value>, _: Option<&Value>) -> String {
            String::new()
        }
        fn test_fixture(&self) -> ProbeFixture {
            StandInProbe.test_fixture()
        }
    }
    let dir = ScratchDir::new("quota-own-dir-gone");
    let ran = dir.join("ran");
    let mut env = fake(&dir, AgentId::TEST, &format!("touch '{}'", ran.display()));
    env.insert("OCTOBOARD_TEST_QUOTA_HOME".to_owned(), "1".to_owned());
    let unavailable = probe_with(
        AgentId::TEST,
        Probing {
            conversation: &AnywhereProbe,
            probes_dir: &dir.join("probes"),
        },
        &env,
        Some(&dir.join("account")),
        PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect_err("no reading");
    assert_eq!(unavailable.code, code::CONFIG_DIR_MISSING);
    assert!(!ran.exists(), "the CLI was not run");
}

/// A Grok account with a configuration and a login, and a stand-in that initialises its home as
/// Grok Build does: it appends to the home's `config.toml`, records the home and the login it was
/// given, and answers the billing request.
fn grok_account_and_stand_in(dir: &ScratchDir, users: &str) -> (PathBuf, HashMap<String, String>) {
    use crate::providers::grok::{GROK_CONFIG_FILE, GROK_SIGN_IN_FILE};

    let account = dir.join("account");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(GROK_CONFIG_FILE), users).unwrap();
    std::fs::write(account.join(GROK_SIGN_IN_FILE), "{}").unwrap();
    std::fs::write(dir.join("reply"), fixture_of(AgentId::GROK).reply).unwrap();
    let env = fake(
        dir,
        AgentId::GROK,
        &format!(
            "echo \"$GROK_HOME\" > '{d}/dir'; echo \"$GROK_AUTH_PATH\" > '{d}/login'\n\
             printf '\\n[marketplace]\\ndefault_skills_installs_purged = true\\n' \
             >> \"$GROK_HOME/config.toml\"\n\
             while read line; do case \"$line\" in *'_x.ai/billing'*) cat '{d}/reply';; esac\n\
             done",
            d = dir.display(),
        ),
    );
    (account, env)
}

/// Grok Build's reading is run against a directory of Octoboard's own, never the account's: the
/// CLI is given that directory, with a copy of the account's configuration in it, and the
/// account's own login by its path; whatever the CLI writes there leaves the account's
/// `config.toml` as it was.
#[test]
fn groks_reading_runs_against_a_home_of_its_own_and_leaves_the_accounts_configuration() {
    use crate::providers::grok::{GROK_CONFIG_FILE, GROK_SIGN_IN_FILE};

    let dir = ScratchDir::new("quota-grok-own-home");
    let users = "# mine\n[ui]\nfoo = \"bar\"\n";
    let (account, env) = grok_account_and_stand_in(&dir, users);
    let run_homes = dir.join("homes");

    super::probe(
        AgentId::GROK,
        &env,
        Some(&account),
        &run_homes,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect("a readable reply");

    let home = PathBuf::from(std::fs::read_to_string(dir.join("dir")).unwrap().trim());
    assert!(home.starts_with(&run_homes), "{}", home.display());
    let login = crate::hostfs::canonical(account.join(GROK_SIGN_IN_FILE)).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("login")).unwrap().trim(),
        login.to_string_lossy()
    );
    assert_eq!(
        std::fs::read_to_string(account.join(GROK_CONFIG_FILE)).unwrap(),
        users,
        "the account's configuration is as it was"
    );
    assert!(std::fs::read_to_string(home.join(GROK_CONFIG_FILE))
        .unwrap()
        .starts_with(users));
}

/// Where the directory of Octoboard's own cannot be made ready, the reading fails and the CLI is
/// not run at all: the account's own directory is never fallen back to.
#[test]
fn a_grok_reading_whose_home_cannot_be_prepared_fails_and_runs_nothing() {
    let dir = ScratchDir::new("quota-grok-no-home");
    let users = "# mine\n";
    let (account, env) = grok_account_and_stand_in(&dir, users);
    let run_homes = dir.join("homes");
    std::fs::write(&run_homes, "a file where the directories go").unwrap();

    let unavailable = super::probe(
        AgentId::GROK,
        &env,
        Some(&account),
        &run_homes,
        crate::test_support::PATIENCE,
        &AtomicBool::new(false),
        &mut ProbeTrace::default(),
    )
    .expect_err("a failed reading");
    assert_eq!(unavailable.code, code::COMMAND_FAILED);
    assert!(!dir.join("dir").exists(), "the CLI was not run");
    assert_eq!(
        std::fs::read_to_string(account.join(crate::providers::grok::GROK_CONFIG_FILE)).unwrap(),
        users
    );
}
