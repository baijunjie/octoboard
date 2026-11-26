//! The early-exit relaunch at the coordinator, with stand-in launches chosen by the tier each
//! request may use: one that prints a refusal and exits for the upper tier, one that comes up for
//! the lower. "No relaunch happened" is read from the launches recorded and the session's status
//! once the exit watcher is done, never from a wait.

use super::*;

/// The decision on an ended process, apart from any process: only an end by itself, of the
/// session's current process, inside the window, with nothing shown and nothing typed, and with a
/// tier left below its own, calls for a relaunch, one tier down.
#[test]
fn only_an_unshown_untyped_exit_by_itself_inside_the_window_goes_one_tier_down() {
    let early = Ending {
        by_itself: true,
        current: true,
        recorded: true,
        lived: Duration::from_secs(1),
        window: EARLY_EXIT_WINDOW,
        shown: false,
        typed: false,
        tier: LaunchTier::Enhanced,
    };
    assert_eq!(tier_after(&early), Some(LaunchTier::Reduced));
    assert_eq!(
        tier_after(&Ending {
            tier: LaunchTier::Reduced,
            ..early
        }),
        Some(LaunchTier::Bare)
    );
    for not_early in [
        Ending {
            tier: LaunchTier::Bare,
            ..early
        },
        Ending {
            by_itself: false,
            ..early
        },
        Ending {
            current: false,
            ..early
        },
        Ending {
            recorded: false,
            ..early
        },
        Ending {
            lived: EARLY_EXIT_WINDOW + Duration::from_millis(1),
            ..early
        },
        Ending {
            shown: true,
            ..early
        },
        Ending {
            typed: true,
            ..early
        },
    ] {
        assert_eq!(tier_after(&not_early), None, "{not_early:?}");
    }
}

/// An output within both bounds is kept whole; a longer one keeps the whole lines of its start and
/// of its end within their bounds, with the elision on a line between them, and a line longer than
/// its bound alone is cut at a character boundary.
#[test]
fn a_long_output_keeps_whole_lines_of_its_head_and_its_tail() {
    assert_eq!(head_and_tail("one\ntwo", 4, 4), "one\ntwo");
    let text = "refused: --x\nusage line one\nusage line two\nlast";
    assert_eq!(head_and_tail(text, 14, 6), "refused: --x\n…\nlast");
    assert_eq!(
        head_and_tail(text, 14, 20),
        "refused: --x\n…\nusage line two\nlast"
    );
    let long = "ééééé\nmiddle\nééééé";
    assert_eq!(head_and_tail(long, 3, 3), "é\n…\né");
}

/// Qwen Code 0.28.3 refusing an option prints its refusal first and 17 KB of usage after it: the
/// refusal is kept, as is the end of the usage, within the bounds.
#[test]
fn a_refusal_printed_before_a_long_usage_text_is_kept() {
    let capture = include_bytes!("../../../testdata/qwen_early_exit_unknown_option_0.28.3.bin");
    let plain = crate::terminal::plain_text::strip_escapes(&String::from_utf8_lossy(capture));
    let kept = head_and_tail(plain.trim(), OUTPUT_HEAD_KEPT, OUTPUT_TAIL_KEPT);
    assert!(
        kept.lines()
            .next()
            .is_some_and(|line| line.starts_with("Unknown arguments: octoboard-unknown-flag")),
        "{kept}"
    );
    assert!(kept.contains("Show help"), "{kept}");
    assert!(kept.len() <= OUTPUT_HEAD_KEPT + OUTPUT_TAIL_KEPT + OUTPUT_ELIDED.len() + 2);
}

/// A relaunch after an early exit that fails leaves the session interrupted and, last, puts back
/// what the exited process left of a sharing account's registries, from the record as it ran;
/// with no such record, nothing is asked of the registries.
#[tokio::test]
async fn a_failed_relaunch_puts_a_sharing_accounts_registries_back() {
    use crate::protocol::{AgentId, SessionStatus};
    let (state, dir) = crate::test_support::app_state("early-exit-relaunch-failed-registries");
    let kept = |id: &str| {
        crate::accounts::shared_config::KEPT_AFTER_EXIT
            .lock()
            .unwrap()
            .iter()
            .any(|kept| kept == id)
    };
    for (id, with_record) in [
        ("early-exit-failed-kept", true),
        ("early-exit-failed-unrecorded", false),
    ] {
        crate::state::test_support::insert_record(&state, &dir, id, SessionStatus::Starting);
        let ran_as = with_record.then(|| {
            crate::test_support::project_session(id, AgentId::TEST, SessionStatus::Starting)
        });
        relaunch_failed(&state, id, ran_as, false).await;
        assert_eq!(
            state.session_record(id).unwrap().status,
            SessionStatus::Interrupted,
            "{id}"
        );
        assert_eq!(kept(id), with_record, "{id}");
    }
}

#[cfg(unix)]
mod relaunch {
    use std::sync::{Arc, Mutex};

    use crate::adapter::LaunchTier;
    use crate::coordinator::launch::{relaunch_session_telling, stop_process};
    use crate::protocol::{notice_code, AgentId, Event, SessionStatus};
    use crate::session::LiveSession;
    use crate::state::AppState;
    use crate::test_support::{
        console_with_project_and_accounts, exit_settled, project_session, tiered_stand_in_launch,
        within, ScratchDir, StandIn, TieredLaunches, PATIENCE,
    };

    /// A CLI that refuses an option Octoboard gave it and exits.
    const REFUSES: &str = "printf 'error: unexpected argument --hooks\\n'; exit 2";
    /// A CLI that comes up: it completes a terminal title, as an interactive program does.
    const COMES_UP: &str = "printf '\\033]0;agent\\007'; exec sleep 300";
    /// A CLI that shows nothing and exits once a line reaches it.
    const EXITS_ON_A_LINE: &str = "read line; exit 2";

    struct Fixture {
        state: Arc<AppState>,
        _dir: ScratchDir,
        started: Arc<Mutex<Vec<StandIn>>>,
        launches: TieredLaunches,
        events: tokio::sync::broadcast::Receiver<Event>,
    }

    impl Fixture {
        /// The process of the `n`th launch.
        fn live(&self, n: usize) -> Arc<LiveSession> {
            (*self.started.lock().unwrap()[n]).clone()
        }

        fn launch_count(&self) -> usize {
            self.launches.lock().unwrap().len()
        }

        fn status(&self) -> SessionStatus {
            self.state.session_record("s").unwrap().status
        }

        /// Waits until the session is recorded without a process and the exit watcher is done
        /// with the `n`th launch's end.
        async fn until_interrupted_after(&self, n: usize) {
            let state = self.state.clone();
            assert!(
                within(PATIENCE, move || state.session_record("s").is_ok_and(
                    |session| session.status == SessionStatus::Interrupted
                ))
                .await,
                "the exit was never recorded"
            );
            exit_settled(&self.live(n)).await;
        }

        /// Whether any client was told the session was interrupted.
        fn told_interrupted(&mut self) -> bool {
            let mut told = false;
            while let Ok(event) = self.events.try_recv() {
                if let Event::SessionUpserted { session } = event {
                    told |= session.id == "s" && session.status == SessionStatus::Interrupted;
                }
            }
            told
        }

        fn retried_plain_raised(&self) -> bool {
            self.state
                .standing_notices_of("s")
                .iter()
                .any(|notice| notice.code == notice_code::LAUNCH_RETRIED_PLAIN.as_str())
        }
    }

    /// Relaunches the interrupted session `s` through stand-ins running what `script_for` gives
    /// each tier, launched under a claim that `claim` makes (a plain launch claim when `None`),
    /// which is dropped once the launch is registered unless the caller keeps it.
    async fn launched(
        name: &str,
        script_for: impl Fn(LaunchTier) -> String + Send + Sync + 'static,
    ) -> Fixture {
        let (fixture, claim) = launched_under(name, script_for, false, Vec::new()).await;
        drop(claim);
        fixture
    }

    /// [`launched`], under a claim that follows the launch when `following` says so, telling the
    /// session `notices` as a resume tells it what it found.
    async fn launched_under(
        name: &str,
        script_for: impl Fn(LaunchTier) -> String + Send + Sync + 'static,
        following: bool,
        notices: Vec<crate::protocol::Notice>,
    ) -> (Fixture, crate::state::LaunchClaim) {
        let (state, dir) = console_with_project_and_accounts(name);
        state
            .store
            .insert_session(&project_session(
                "s",
                AgentId::TEST,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let started = Arc::new(Mutex::new(Vec::new()));
        let launches = TieredLaunches::default();
        let relauncher = tiered_stand_in_launch(&started, &launches, script_for);
        *state.relauncher.lock().unwrap() = Some(relauncher.clone());
        let events = state.subscribe();
        let claim = if following {
            state.begin_switch("s").unwrap().following()
        } else {
            state.begin_launch("s").unwrap()
        };
        relaunch_session_telling(
            &state,
            &claim,
            "s",
            None,
            true,
            None,
            None,
            notices,
            move |request| relauncher(request),
        )
        .await
        .unwrap();
        (
            Fixture {
                state,
                _dir: dir,
                started,
                launches,
                events,
            },
            claim,
        )
    }

    fn refuses_above_bare(tier: LaunchTier) -> String {
        match tier {
            LaunchTier::Enhanced => REFUSES,
            LaunchTier::Reduced | LaunchTier::Bare => COMES_UP,
        }
        .to_string()
    }

    /// A CLI that refuses the enhanced launch's options is started once more, at the tier below,
    /// handed what it wrote; the session is never recorded interrupted, and once the bare launch
    /// is seen to come up the user is told with a standing `launch_retried_plain`.
    #[tokio::test]
    async fn a_cli_that_exits_as_it_starts_is_relaunched_once_one_tier_down_and_the_user_told() {
        let mut fixture = launched("early-exit-relaunched", refuses_above_bare).await;
        let state = fixture.state.clone();
        assert!(
            within(PATIENCE, move || state.standing_notices_of("s").iter().any(
                |notice| notice.code == notice_code::LAUNCH_RETRIED_PLAIN.as_str()
            ))
            .await,
            "the user was never told"
        );
        let launches = fixture.launches.lock().unwrap().clone();
        assert_eq!(launches.len(), 2, "{launches:?}");
        assert_eq!(launches[0], (LaunchTier::Enhanced, None));
        assert_eq!(launches[1].0, LaunchTier::Reduced);
        let exit = launches[1]
            .1
            .clone()
            .expect("the relaunch follows the early exit");
        assert!(
            exit.output.contains("unexpected argument --hooks"),
            "{exit:?}"
        );
        let current = fixture.state.live_session("s").expect("the relaunch runs");
        assert!(Arc::ptr_eq(&current, &fixture.live(1)));
        assert_eq!(fixture.status(), SessionStatus::Idle);
        assert!(!fixture.told_interrupted());
    }

    /// A lower tier that stays up after an early exit is remembered for the program and version
    /// the launch started, with the early exit of the tier above it, which a later launch of the
    /// same program and version starts from.
    #[tokio::test]
    async fn a_lower_tier_that_stays_up_is_remembered_for_its_program_and_version() {
        use crate::session::relaunch::LowerTierKey;
        let (state, dir) = console_with_project_and_accounts("early-exit-remembered");
        state
            .store
            .insert_session(&project_session(
                "s",
                AgentId::TEST,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let key = LowerTierKey {
            agent: AgentId::TEST,
            program: std::path::PathBuf::from("/bin/agent"),
            version: Some((1, 2, 3)),
        };
        let started = Arc::new(Mutex::new(Vec::new()));
        let launches = TieredLaunches::default();
        let inner = tiered_stand_in_launch(&started, &launches, refuses_above_bare);
        // What `term::launch` notes of the program it started.
        let relauncher: crate::session::term::Relauncher = {
            let key = key.clone();
            Arc::new(
                move |request: crate::session::term::LaunchRequest| -> anyhow::Result<_> {
                    let launch = inner(request)?;
                    launch.session.startup.note_remembered_as(key.clone());
                    Ok(launch)
                },
            )
        };
        *state.relauncher.lock().unwrap() = Some(relauncher.clone());
        let claim = state.begin_launch("s").unwrap();
        relaunch_session_telling(
            &state,
            &claim,
            "s",
            None,
            true,
            None,
            None,
            Vec::new(),
            move |request| relauncher(request),
        )
        .await
        .unwrap();
        drop(claim);
        let _fixture = Fixture {
            state: state.clone(),
            _dir: dir,
            started,
            launches,
            events: state.subscribe(),
        };

        let remembered = state.clone();
        let wanted = key.clone();
        assert!(
            within(PATIENCE, move || remembered
                .lower_tiers
                .recalled(&wanted)
                .is_some())
            .await,
            "the tier that stayed up was never remembered"
        );
        let recalled = state.lower_tiers.recalled(&key).unwrap();
        assert_eq!(recalled.tier, LaunchTier::Bare);
        assert!(
            recalled.exit.output.contains("unexpected argument --hooks"),
            "{:?}",
            recalled.exit
        );
        let other_version = LowerTierKey {
            version: Some((1, 2, 4)),
            ..key
        };
        assert_eq!(state.lower_tiers.recalled(&other_version), None);
    }

    /// What the caller told the launch (a resume's record not found) is told by the launch one
    /// tier down too, with `launch_retried_plain`, once it comes up.
    #[tokio::test]
    async fn a_relaunch_one_tier_down_keeps_what_the_caller_told_the_launch() {
        let not_found = crate::protocol::Notice::new(
            notice_code::CONVERSATION_RECORD_NOT_FOUND,
            "the record was not found",
            &[("agent", AgentId::TEST.label())],
        )
        .standing();
        let (fixture, claim) = launched_under(
            "early-exit-keeps-notices",
            refuses_above_bare,
            false,
            vec![not_found],
        )
        .await;
        drop(claim);
        let state = fixture.state.clone();
        assert!(
            within(PATIENCE, move || state.standing_notices_of("s").iter().any(
                |notice| notice.code == notice_code::LAUNCH_RETRIED_PLAIN.as_str()
            ))
            .await,
            "the relaunch never came up"
        );
        assert_eq!(fixture.launch_count(), 2);
        let codes: Vec<String> = fixture
            .state
            .standing_notices_of("s")
            .into_iter()
            .map(|notice| notice.code)
            .collect();
        assert!(
            codes.contains(
                &notice_code::CONVERSATION_RECORD_NOT_FOUND
                    .as_str()
                    .to_string()
            ),
            "{codes:?}"
        );
    }

    /// Codex refusing a `default_permissions` profile turns the alternate screen and bracketed
    /// paste on before it prints its refusal and exits (the capture, split right after it enters
    /// the alternate screen so that a read sees both on): neither mode is a sign of a program that
    /// started, so the launch is relaunched one tier down, handed the refusal.
    #[tokio::test]
    async fn modes_turned_on_before_an_exit_at_start_are_no_sign_of_starting() {
        let capture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/codex_exit_unknown_profile.bin"
        );
        let refuses = format!("head -c 77 '{capture}'; sleep 0.3; tail -c +78 '{capture}'; exit 1");
        let fixture = launched("early-exit-modes-on", move |tier| match tier {
            LaunchTier::Enhanced => refuses.clone(),
            LaunchTier::Reduced | LaunchTier::Bare => COMES_UP.to_string(),
        })
        .await;
        let launches = fixture.launches.clone();
        assert!(
            within(PATIENCE, move || launches.lock().unwrap().len() == 2).await,
            "no relaunch"
        );
        let launches = fixture.launches.lock().unwrap().clone();
        assert_eq!(launches[1].0, LaunchTier::Reduced);
        let exit = launches[1]
            .1
            .clone()
            .expect("the relaunch follows the early exit");
        assert!(
            exit.output
                .contains("default_permissions refers to unknown built-in profile"),
            "{exit:?}"
        );
    }

    /// When the bare launch exits as it starts too, nothing is tried a third time: two launches,
    /// then the session is interrupted, and nothing is raised for the relaunch.
    #[tokio::test]
    async fn when_the_bare_launch_exits_too_the_session_ends_after_two_launches() {
        let fixture = launched("early-exit-both-tiers", |_| REFUSES.to_string()).await;
        let launches = fixture.launches.clone();
        assert!(
            within(PATIENCE, move || launches.lock().unwrap().len() == 2).await,
            "no relaunch"
        );
        fixture.until_interrupted_after(1).await;
        assert_eq!(fixture.launch_count(), 2);
        assert!(!fixture.retried_plain_raised());
    }

    /// A key a client typed before the exit, or a title, a hook or a status report the program
    /// made, says the CLI got past its start: its exit is recorded as any, with no relaunch.
    #[tokio::test]
    async fn an_exit_after_a_typed_key_or_any_startup_evidence_is_not_relaunched() {
        type Before = fn(&Arc<AppState>, &Arc<LiveSession>);
        let cases: [(&str, &str, Before); 4] = [
            ("typed", EXITS_ON_A_LINE, |_, live| {
                live.write_terminal_input(b"q\n").unwrap();
            }),
            (
                "titled",
                "printf '\\033]0;agent\\007'; read line; exit 2",
                |_, live| {
                    live.write_input(b"\n").unwrap();
                },
            ),
            ("hooked", EXITS_ON_A_LINE, |state, live| {
                state.note_launch_shown("s", Some(&live.launch_id));
                live.write_input(b"\n").unwrap();
            }),
            ("reported", EXITS_ON_A_LINE, |_, live| {
                use crate::terminal::program_status::{Record, ReportedState};
                live.note_hook_program_status(Record {
                    state: ReportedState::Working,
                    kind: None,
                    progress: None,
                    app: None,
                    title: None,
                    msg: None,
                });
                live.write_input(b"\n").unwrap();
            }),
        ];
        for (name, script, before) in cases {
            let script = script.to_string();
            let fixture = launched(&format!("early-exit-not-{name}"), move |tier| match tier {
                LaunchTier::Enhanced => script.clone(),
                _ => COMES_UP.to_string(),
            })
            .await;
            let live = fixture.live(0);
            if name == "titled" {
                let shown = live.clone();
                assert!(
                    within(PATIENCE, move || shown.startup.has_shown()).await,
                    "{name}: the title was never read"
                );
            }
            before(&fixture.state, &live);
            fixture.until_interrupted_after(0).await;
            assert_eq!(fixture.launch_count(), 1, "{name}");
        }
    }

    /// An exit that comes after the window is the CLI's own, however little it showed.
    #[tokio::test]
    async fn an_exit_after_the_window_is_not_relaunched() {
        let _over = super::window_for_test(std::time::Duration::ZERO);
        let fixture = launched("early-exit-after-window", refuses_above_bare).await;
        fixture.until_interrupted_after(0).await;
        assert_eq!(fixture.launch_count(), 1);
    }

    /// A process ended on purpose inside the window is not relaunched.
    #[tokio::test]
    async fn a_process_ended_on_purpose_inside_the_window_is_not_relaunched() {
        let fixture = launched("early-exit-on-purpose", |tier| match tier {
            LaunchTier::Enhanced => "exec sleep 300".to_string(),
            _ => COMES_UP.to_string(),
        })
        .await;
        assert!(stop_process(&fixture.state, fixture.live(0)).await.unwrap());
        fixture.until_interrupted_after(0).await;
        assert_eq!(fixture.launch_count(), 1);
    }

    /// An exit at start while another operation holds the session's claim is not relaunched: it
    /// is recorded as any exit.
    #[tokio::test]
    async fn an_exit_while_another_operation_holds_the_claim_is_not_relaunched() {
        let _no_wait = super::claim_wait_for_test(std::time::Duration::ZERO);
        let fixture = launched("early-exit-claim-held", |tier| match tier {
            LaunchTier::Enhanced => EXITS_ON_A_LINE.to_string(),
            _ => COMES_UP.to_string(),
        })
        .await;
        let held = fixture.state.begin_switch("s").unwrap();
        fixture.live(0).write_input(b"\n").unwrap();
        fixture.until_interrupted_after(0).await;
        assert_eq!(fixture.launch_count(), 1);
        drop(held);
    }

    /// An operation that follows its launch come up under its claim (an account switch, a change
    /// of the model) is handed the early exit and relaunches one tier down under that claim, and
    /// sees the relaunch come up.
    #[tokio::test]
    async fn an_operation_following_its_launch_relaunches_it_under_its_claim() {
        let (fixture, claim) =
            launched_under("early-exit-followed", refuses_above_bare, true, Vec::new()).await;
        assert!(
            crate::coordinator::restart::came_up_under_claim(&fixture.state, &claim, "s").await
        );
        assert_eq!(fixture.launch_count(), 2);
        let current = fixture.state.live_session("s").expect("the relaunch runs");
        assert!(Arc::ptr_eq(&current, &fixture.live(1)));
        assert_eq!(current.integration().tier, LaunchTier::Bare);
        drop(claim);
    }
}
