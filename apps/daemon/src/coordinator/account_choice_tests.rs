//! The tests of `crate::accounts::choice`, which open sessions through the coordinator.

use crate::accounts::choice::*;
use crate::coordinator::account_switch::switch_session_account;
use crate::coordinator::accounts::set_account_disabled;
use crate::coordinator::open::{
    check_open, open_session, open_session_detached, resolve_open, resolve_open_with, OpenRequest,
};
use crate::coordinator::test_support::{
    console, console_workdir_in, open_request, pins, project_of, shell_env, state_for_opening,
};
use crate::protocol::{
    error_code, Account, AgentId, Availability, CodedError, Origin, Project, ProjectAccount,
    SessionStatus,
};
use crate::state::AppState;
use crate::test_support::code_of;
use crate::test_support::{console_with_project_and_accounts, project_session};
use crate::time::now_millis;
use std::path::Path;
use std::sync::Arc;

/// A session whose resolved agent has been determined unavailable is refused before anything
/// is launched or written — no record, no claim, no process.
#[tokio::test]
async fn opening_a_session_is_refused_when_its_resolved_agent_is_unavailable() {
    let (state, dir) = crate::test_support::app_state("coordinator-open-session-agent-unavailable");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, &workdir))
        .unwrap();
    state.set_agent_availability(
        [AgentId::CLAUDE, AgentId::CODEX]
            .into_iter()
            .map(|agent| crate::protocol::AgentAvailability {
                agent,
                availability: Availability::Unavailable,
                default_account_dir: None,
            })
            .collect(),
    );
    for (chosen, expected) in [
        (None, AgentId::CLAUDE),
        (Some(AgentId::CODEX), AgentId::CODEX),
    ] {
        let err = open_session(
            &state,
            OpenRequest {
                compatibility: Default::default(),
                console_id: "console-1".to_string(),
                project_id: None,
                agent: chosen,
                account: None,
                task: None,
                title: None,
                model: None,
                effort: None,
                origin: Origin::User,
                bound_to: None,
                resume_agent_session_id: None,
                session_id: None,
                terminal_appearance: None,
            },
        )
        .await
        .expect_err("refused");
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::AGENT_NOT_AVAILABLE);
        assert_eq!(coded.params["agent"], expected.label());
        assert!(state.store.list_sessions().unwrap().is_empty());
    }
}

/// An account whose held quota reading has a window used up is refused with the window and
/// its reset time when known, for the default account and a stored one alike; an account with
/// no reading is not.
#[test]
fn opening_a_session_is_refused_on_an_account_whose_quota_is_used_up() {
    use crate::protocol::{AccountQuota, Params, QuotaWindow, QuotaWindowKind};

    let (state, dir) = crate::test_support::app_state("coordinator-open-quota-exhausted");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, &workdir))
        .unwrap();
    state
        .store
        .insert_account(&Account {
            id: "claude-acct".to_string(),
            agent: AgentId::CLAUDE,
            name: "Claude work".to_string(),
            config_dir: "/home/u/.claude-alt".to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        })
        .unwrap();
    let reset = now_millis() + 60_000;
    let used_up = |account: Option<&str>, resets_at| AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: account.map(str::to_string),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent: 100.0,
            resets_at,
            window_minutes: None,
        }],
        unavailable: None,
    };
    let request = |account: Option<String>| OpenRequest {
        compatibility: Default::default(),
        console_id: "console-1".to_string(),
        project_id: None,
        agent: None,
        account: Some(account),
        task: None,
        title: None,
        model: None,
        effort: None,
        origin: Origin::User,
        bound_to: None,
        resume_agent_session_id: None,
        session_id: None,
        terminal_appearance: None,
    };
    let refusal =
        |account: Option<&str>| resolve_open(&state, &request(account.map(str::to_string))).err();
    let coded = |err: &anyhow::Error| -> (crate::protocol::Code, Params) {
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        (coded.code, coded.params.clone())
    };

    // Nothing read yet: usable.
    assert!(refusal(Some("claude-acct")).is_none());
    assert!(refusal(None).is_none());

    state.account_quotas.hold_in_dir_for_test(
        &used_up(Some("claude-acct"), Some(reset)),
        "/home/u/.claude-alt",
    );
    let (code, params) = coded(&refusal(Some("claude-acct")).expect("refused"));
    assert_eq!(code, error_code::ACCOUNT_QUOTA_EXHAUSTED);
    assert_eq!(params["agent"], AgentId::CLAUDE.label());
    assert_eq!(params["account"], "Claude work");
    assert_eq!(params["window"], "Weekly");
    assert_eq!(params["window_kind"], "weekly");
    assert!(!params.contains_key("window_minutes"));
    assert!(!params.contains_key("model_weekly"));
    assert_eq!(params["resets_at"], reset.to_string());

    // The default account, whose reading does not say when it resets.
    state.account_quotas.hold_for_test(&used_up(None, None));
    let (code, params) = coded(&refusal(None).expect("refused"));
    assert_eq!(code, error_code::ACCOUNT_QUOTA_EXHAUSTED);
    assert_eq!(params["account"], crate::protocol::DEFAULT_ACCOUNT_NAME);
    assert!(!params.contains_key("resets_at"));

    // An account the reading found not logged in is refused too, the default one included.
    let signed_out = |account: Option<&str>| AccountQuota {
        windows: Vec::new(),
        unavailable: Some(crate::protocol::QuotaUnavailable::new(
            crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
            "x",
            &[("agent", "Claude Code")],
        )),
        ..used_up(account, None)
    };
    state
        .account_quotas
        .hold_in_dir_for_test(&signed_out(Some("claude-acct")), "/home/u/.claude-alt");
    state.account_quotas.hold_for_test(&signed_out(None));
    for account in [Some("claude-acct"), None] {
        let (code, params) = coded(&refusal(account).expect("refused"));
        assert_eq!(code, error_code::ACCOUNT_NOT_SIGNED_IN);
        assert_eq!(params["agent"], AgentId::CLAUDE.label());
    }
}

/// A disabled account, the default one included, is refused for a new session whether the
/// request names it or the console inherits it, is passed over by automatic selection and by
/// `account_can_open`, and turning it back on undoes all of that. A session already on the
/// account keeps it.
#[tokio::test]
async fn a_disabled_account_is_skipped_and_refused_for_new_sessions_but_not_for_running_ones() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-account-disabled");
    let alt_dir = state
        .store
        .account_dir(&Some("claude-a".to_string()))
        .unwrap()
        .unwrap();
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CLAUDE,
            SessionStatus::Working,
        ))
        .unwrap();
    state
        .store
        .set_session_account("s", Some("claude-a"), Some(&alt_dir))
        .unwrap();
    let request = |account: Option<Option<String>>| OpenRequest {
        compatibility: Default::default(),
        console_id: "console-1".to_string(),
        project_id: None,
        agent: None,
        account,
        task: None,
        title: None,
        model: None,
        effort: None,
        origin: Origin::User,
        bound_to: None,
        resume_agent_session_id: None,
        session_id: None,
        terminal_appearance: None,
    };
    let refused = |account: Option<Option<String>>| {
        resolve_open(&state, &request(account)).err().map(|err| {
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            (coded.code, coded.params.clone())
        })
    };
    assert!(refused(Some(Some("claude-a".to_string()))).is_none());

    // An id that is no account of the agent cannot be turned off.
    let err = set_account_disabled(&state, AgentId::CODEX, Some("claude-a"), true)
        .expect_err("another agent's account");
    assert_eq!(code_of(&err), error_code::UNKNOWN_ACCOUNT);

    set_account_disabled(&state, AgentId::CLAUDE, Some("claude-a"), true).unwrap();
    set_account_disabled(&state, AgentId::CLAUDE, None, true).unwrap();
    let settings = state.store.get_settings().unwrap();
    assert!(settings.accounts[0].disabled);
    assert!(settings.disabled_default_accounts.contains("claude"));

    for account in [Some(Some("claude-a".to_string())), Some(None)] {
        let (code, params) = refused(account).expect("refused");
        assert_eq!(code, error_code::ACCOUNT_DISABLED);
        assert_eq!(params["agent"], AgentId::CLAUDE.label());
    }
    // Inherited from the console as well as named.
    assert_eq!(refused(None).unwrap().0, error_code::ACCOUNT_DISABLED);
    assert!(!account_can_open(&state, AgentId::CLAUDE, Some("claude-a")).unwrap());
    assert!(!account_can_open(&state, AgentId::CLAUDE, None).unwrap());
    // Another agent's default account is unaffected.
    assert!(account_can_open(&state, AgentId::CODEX, None).unwrap());

    // Automatic selection passes over both, even with a reading that ranks them.
    let weekly = |account: Option<&str>| crate::protocol::AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: account.map(str::to_string),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: vec![crate::protocol::QuotaWindow {
            kind: crate::protocol::QuotaWindowKind::Weekly,
            label: None,
            used_percent: 10.0,
            resets_at: Some(now_millis() + 3_600_000),
            window_minutes: None,
        }],
        unavailable: None,
    };
    state
        .account_quotas
        .hold_in_dir_for_test(&weekly(Some("claude-a")), &alt_dir);
    state.account_quotas.hold_for_test(&weekly(None));
    assert_eq!(
        preferred_weekly_account(&state, AgentId::CLAUDE).unwrap(),
        None
    );

    // The running session keeps its account.
    let running = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(running.account_id.as_deref(), Some("claude-a"));
    assert_eq!(running.status, SessionStatus::Working);

    // Turning one back on lets it be picked again; the other stays off.
    set_account_disabled(&state, AgentId::CLAUDE, Some("claude-a"), false).unwrap();
    assert!(refused(Some(Some("claude-a".to_string()))).is_none());
    assert_eq!(
        preferred_weekly_account(&state, AgentId::CLAUDE).unwrap(),
        Some(Some("claude-a".to_string()))
    );
    set_account_disabled(&state, AgentId::CLAUDE, None, false).unwrap();
    assert!(state
        .store
        .get_settings()
        .unwrap()
        .disabled_default_accounts
        .is_empty());
}

/// An account the last quota reading found not logged in is refused for a switch and passed
/// over by `account_can_open` and by the choice of a target, the signed-out reasons both
/// included; a reading that failed for another reason refuses and passes over nothing there
/// (the choice of a target for the automatic switch alone asks for a readable one).
#[tokio::test]
async fn an_account_found_not_logged_in_is_refused_and_a_failed_reading_is_not() {
    use crate::protocol::{
        quota_unavailable_code as code, AccountQuota, QuotaUnavailable, QuotaWindow,
        QuotaWindowKind,
    };

    let (state, dir) = console_with_project_and_accounts("coordinator-switch-signed-out");
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CLAUDE,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let alt_dir = state
        .store
        .account_dir(&Some("claude-a".to_string()))
        .unwrap()
        .unwrap();
    let reading = |unavailable: Option<QuotaUnavailable>| AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: Some("claude-a".to_string()),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent: 10.0,
            resets_at: Some(now_millis() + 60_000),
            window_minutes: None,
        }],
        unavailable,
    };
    let switch = || {
        switch_session_account(
            &state,
            "s",
            Some(Some("claude-a".to_string())),
            None,
            shell_env(&dir),
        )
    };

    for reason in [code::NOT_SIGNED_IN, code::CONFIG_DIR_MISSING] {
        let held = reading(Some(QuotaUnavailable::new(
            reason,
            "x",
            &[("agent", "Claude")],
        )));
        state.account_quotas.hold_in_dir_for_test(&held, &alt_dir);
        let err = switch().await.expect_err("refused");
        assert_eq!(code_of(&err), error_code::ACCOUNT_NOT_SIGNED_IN, "{reason}");
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.params["agent"], AgentId::CLAUDE.label());
        assert!(!account_can_open(&state, AgentId::CLAUDE, Some("claude-a")).unwrap());
        assert_eq!(
            preferred_weekly_account(&state, AgentId::CLAUDE).unwrap(),
            None,
            "{reason}: not picked for a new session either"
        );
        assert_eq!(
            state.store.get_session("s").unwrap().unwrap().account_id,
            None
        );
    }

    // A reading that failed for a passing reason still counts for what it holds.
    let failed = reading(Some(QuotaUnavailable::new(
        code::COMMAND_FAILED,
        "x",
        &[("agent", "Claude"), ("detail", "network")],
    )));
    state.account_quotas.hold_in_dir_for_test(&failed, &alt_dir);
    assert!(account_can_open(&state, AgentId::CLAUDE, Some("claude-a")).unwrap());
    assert_eq!(
        preferred_weekly_account(&state, AgentId::CLAUDE).unwrap(),
        Some(Some("claude-a".to_string()))
    );
    assert_eq!(
        preferred_weekly_account_except(&state, AgentId::CLAUDE, &[], true).unwrap(),
        None,
        "the automatic switch wants an account it can read now"
    );
    let err = switch()
        .await
        .expect_err("fails further on, for want of a process");
    assert_ne!(code_of(&err), error_code::ACCOUNT_NOT_SIGNED_IN);
}

/// An account the daemon holds no quota reading of yet (right after a start, before the first
/// probe finishes) opens a session; once the reading says it is not logged in, a new open on it is
/// refused and records nothing, leaving a session already on the account on record. Only the open
/// is checked.
#[tokio::test]
async fn an_account_not_read_yet_opens_and_is_refused_once_read_as_not_logged_in() {
    use crate::coordinator::open::open_session_with;
    use crate::protocol::{
        quota_unavailable_code as code, AccountQuota, QuotaUnavailable, QuotaWindow,
        QuotaWindowKind,
    };

    let (state, _dir) = console_with_project_and_accounts("coordinator-open-before-first-read");
    let alt_dir = state
        .store
        .account_dir(&Some("claude-a".to_string()))
        .unwrap()
        .unwrap();
    let request = || OpenRequest {
        agent: Some(AgentId::CLAUDE),
        account: Some(Some("claude-a".to_string())),
        ..open_request()
    };
    let reading = |unavailable: Option<QuotaUnavailable>| AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: Some("claude-a".to_string()),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent: 10.0,
            resets_at: None,
            window_minutes: None,
        }],
        unavailable,
    };

    // No reading yet: the open is accepted. A session is already on the account.
    assert!(resolve_open(&state, &request()).is_ok());
    let mut existing = project_session("existing", AgentId::CLAUDE, SessionStatus::Idle);
    existing.account_id = Some("claude-a".to_string());
    state.store.insert_session(&existing).unwrap();

    for reason in [code::NOT_SIGNED_IN, code::CONFIG_DIR_MISSING] {
        let held = reading(Some(QuotaUnavailable::new(
            reason,
            "x",
            &[("agent", "Claude")],
        )));
        state.account_quotas.hold_in_dir_for_test(&held, &alt_dir);

        // The refusal comes before any launch; the closure only keeps a regression from starting
        // a real agent.
        let err = open_session_with(&state, request(), |_| {
            anyhow::bail!("an open that is refused must not launch")
        })
        .await
        .expect_err("refused");
        assert_eq!(code_of(&err), error_code::ACCOUNT_NOT_SIGNED_IN, "{reason}");
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.params["agent"], AgentId::CLAUDE.label());
        assert_eq!(coded.params["account"], "claude-a");
        assert_eq!(
            crate::test_support::session_ids(&state),
            ["existing"],
            "{reason}: the refused open recorded nothing"
        );
    }
}

#[test]
fn automatic_open_choice_uses_another_agent_only_after_all_preferred_accounts() {
    let (state, _dir) = state_for_opening("automatic-open-agent-fallback");
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE, AgentId::GROK, AgentId::CODEX])
        .unwrap();
    set_account_disabled(&state, AgentId::CLAUDE, None, true).unwrap();
    let resolved = resolve_open(&state, &open_request()).unwrap();
    assert_eq!(resolved.agent, AgentId::GROK);
    assert_eq!(resolved.account_id, None);

    let explicit = OpenRequest {
        agent: Some(AgentId::CLAUDE),
        ..open_request()
    };
    assert_eq!(
        code_of(&resolve_open(&state, &explicit).err().unwrap()),
        error_code::ACCOUNT_DISABLED
    );
    let explicit = OpenRequest {
        account: Some(None),
        ..open_request()
    };
    assert_eq!(
        code_of(&resolve_open(&state, &explicit).err().unwrap()),
        error_code::ACCOUNT_DISABLED
    );

    state
        .store
        .insert_account(&Account {
            id: "claude-unread".to_string(),
            agent: AgentId::CLAUDE,
            name: "Unread".to_string(),
            config_dir: "/tmp/claude-unread".to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        })
        .unwrap();
    let resolved = resolve_open(&state, &open_request()).unwrap();
    assert_eq!(resolved.agent, AgentId::CLAUDE);
    assert_eq!(resolved.account_id.as_deref(), Some("claude-unread"));
    let account = state.store.get_account("claude-unread").unwrap().unwrap();
    let (_, lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
    assert_eq!(
        resolve_open(&state, &open_request()).unwrap().agent,
        AgentId::GROK
    );
    drop(lock);
    state.store.set_auto_select_account(false).unwrap();
    assert_eq!(
        code_of(&resolve_open(&state, &open_request()).err().unwrap()),
        error_code::ACCOUNT_DISABLED
    );
}

#[test]
fn automatic_open_choice_uses_saved_account_order_for_unranked_accounts() {
    let (state, _dir) = state_for_opening("automatic-open-account-order");
    state.store.set_auto_select_account(true).unwrap();
    set_account_disabled(&state, AgentId::CLAUDE, None, true).unwrap();
    for id in ["first", "second"] {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent: AgentId::CLAUDE,
                name: id.to_string(),
                config_dir: format!("/tmp/{id}"),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();
    }
    state
        .store
        .reorder_accounts(
            AgentId::CLAUDE,
            &[Some("second".to_string()), None, Some("first".to_string())],
        )
        .unwrap();
    assert_eq!(
        resolve_open(&state, &open_request())
            .unwrap()
            .account_id
            .as_deref(),
        Some("second")
    );
}

/// Accounts an automatic choice finds alike are taken in the order the user keeps them in: a
/// weekly tie, and a tie on when a used-up quota resets.
#[test]
fn automatic_choices_break_a_tie_in_the_users_account_order() {
    use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
    let (state, _dir) = state_for_opening("automatic-tie-account-order");
    for id in ["first", "second"] {
        insert_claude_account(&state, id, false);
    }
    let resets_at = now_millis() + 60_000;
    for id in ["first", "second"] {
        state.account_quotas.hold_in_dir_for_test(
            &AccountQuota {
                reset_credits: None,
                agent: AgentId::CLAUDE,
                account: Some(id.to_string()),
                fetched_at: 1,
                plan: None,
                email: None,
                windows: vec![QuotaWindow {
                    kind: QuotaWindowKind::Weekly,
                    label: None,
                    used_percent: 10.0,
                    resets_at: Some(resets_at),
                    window_minutes: None,
                }],
                unavailable: None,
            },
            &format!("/tmp/{id}"),
        );
    }
    assert_eq!(
        preferred_weekly_account(&state, AgentId::CLAUDE).unwrap(),
        Some(Some("first".to_string()))
    );
    state
        .store
        .reorder_accounts(
            AgentId::CLAUDE,
            &[Some("second".to_string()), None, Some("first".to_string())],
        )
        .unwrap();
    assert_eq!(
        preferred_weekly_account(&state, AgentId::CLAUDE).unwrap(),
        Some(Some("second".to_string()))
    );

    // One reset time for all three, so the tie does not depend on the clock ticking between holds.
    let resets_at = now_millis() + 3_600_000;
    for account in [None, Some("first"), Some("second")] {
        hold_used_up_at(&state, account, Some(resets_at), false);
    }
    assert_eq!(
        soonest_available_account(&state, AgentId::CLAUDE).unwrap(),
        Some(Some("second".to_string()))
    );
}

#[test]
fn automatic_open_choice_keeps_a_weekly_preference_within_its_agent() {
    use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
    let (state, _dir) = state_for_opening("automatic-open-weekly");
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .insert_account(&Account {
            id: "claude-weekly".to_string(),
            agent: AgentId::CLAUDE,
            name: "Weekly".to_string(),
            config_dir: "/tmp/claude-weekly".to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
            versions: Default::default(),
        })
        .unwrap();
    state.account_quotas.hold_in_dir_for_test(
        &AccountQuota {
            reset_credits: None,
            agent: AgentId::CLAUDE,
            account: Some("claude-weekly".to_string()),
            fetched_at: 1,
            plan: None,
            email: None,
            unavailable: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: 70.0,
                resets_at: Some(now_millis() + 60_000),
                window_minutes: None,
            }],
        },
        "/tmp/claude-weekly",
    );
    assert_eq!(
        resolve_open(&state, &open_request())
            .unwrap()
            .account_id
            .as_deref(),
        Some("claude-weekly")
    );
    assert_eq!(
        resolve_open(
            &state,
            &OpenRequest {
                account: Some(None),
                ..open_request()
            }
        )
        .unwrap()
        .account_id,
        None
    );
}

/// Holds a reading of a Claude account whose session window is used up, resetting in
/// `resets_in` milliseconds (`None` no reset time), as `signed_out` or not.
fn hold_used_up(
    state: &Arc<AppState>,
    account: Option<&str>,
    resets_in: Option<i64>,
    signed_out: bool,
) {
    hold_used_up_at(
        state,
        account,
        resets_in.map(|span| now_millis() + span),
        signed_out,
    );
}

/// `hold_used_up` with an absolute reset time `resets_at` (`None` no reset time).
fn hold_used_up_at(
    state: &Arc<AppState>,
    account: Option<&str>,
    resets_at: Option<i64>,
    signed_out: bool,
) {
    use crate::protocol::{AccountQuota, QuotaUnavailable, QuotaWindow, QuotaWindowKind};
    let quota = AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: account.map(str::to_string),
        fetched_at: 1,
        plan: None,
        email: None,
        unavailable: signed_out.then(|| {
            QuotaUnavailable::new(
                crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
                "not signed in",
                &[],
            )
        }),
        windows: vec![QuotaWindow {
            kind: QuotaWindowKind::Session,
            label: None,
            used_percent: 100.0,
            resets_at,
            window_minutes: None,
        }],
    };
    match account {
        None => state.account_quotas.hold_for_test(&quota),
        Some(id) => state
            .account_quotas
            .hold_in_dir_for_test(&quota, &format!("/tmp/{id}")),
    }
}

fn insert_claude_account(state: &Arc<AppState>, id: &str, disabled: bool) {
    state
        .store
        .insert_account(&Account {
            id: id.to_string(),
            agent: AgentId::CLAUDE,
            name: id.to_string(),
            config_dir: format!("/tmp/{id}"),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled,
            versions: Default::default(),
        })
        .unwrap();
}

/// With automatic selection on and no usable account, the session opens on the account of the
/// preferred agent that becomes usable soonest, so the refusal names it; an account that is
/// turned off or logged out is not chosen, a missing reset time counts as never, and an equal
/// time keeps the default account.
#[test]
fn automatic_open_without_a_usable_account_is_refused_on_the_soonest_one() {
    let (state, _dir) = state_for_opening("automatic-open-soonest");
    state.store.set_auto_select_account(true).unwrap();
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE])
        .unwrap();
    for (id, disabled) in [
        ("later", false),
        ("soonest", false),
        ("off", true),
        ("out", false),
    ] {
        insert_claude_account(&state, id, disabled);
    }
    let hour = 3_600_000;
    hold_used_up(&state, None, Some(5 * hour), false);
    hold_used_up(&state, Some("later"), Some(3 * hour), false);
    hold_used_up(&state, Some("soonest"), Some(2 * hour), false);
    hold_used_up(&state, Some("off"), Some(hour), false);
    hold_used_up(&state, Some("out"), Some(hour / 2), true);

    let refused = resolve_open(&state, &open_request()).err().unwrap();
    let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::ACCOUNT_QUOTA_EXHAUSTED);
    assert!(coded.message.starts_with("soonest "), "{}", coded.message);
    assert_eq!(
        soonest_available_account(&state, AgentId::CLAUDE).unwrap(),
        Some(Some("soonest".to_string()))
    );

    // A reset time that is missing counts as never; equal times keep the earlier candidate.
    hold_used_up(&state, Some("soonest"), None, false);
    let mut tied = state
        .account_quotas
        .held(AgentId::CLAUDE, None, None)
        .unwrap();
    tied.account = Some("later".to_string());
    state
        .account_quotas
        .hold_in_dir_for_test(&tied, "/tmp/later");
    assert_eq!(
        soonest_available_account(&state, AgentId::CLAUDE).unwrap(),
        Some(None)
    );
    assert_eq!(
        resolve_open_with(&state, &open_request(), false)
            .unwrap()
            .account_id,
        None
    );

    // A usable account of the agent, or automatic selection off, leaves it as it was.
    insert_claude_account(&state, "fresh", false);
    assert_eq!(
        soonest_available_account(&state, AgentId::CLAUDE).unwrap(),
        None
    );
    assert_eq!(
        resolve_open(&state, &open_request())
            .unwrap()
            .account_id
            .as_deref(),
        Some("fresh")
    );
    state.store.delete_account("fresh").unwrap();
    state.store.set_auto_select_account(false).unwrap();
    let refused = resolve_open(&state, &open_request()).err().unwrap();
    assert!(
        refused
            .downcast_ref::<CodedError>()
            .unwrap()
            .message
            .starts_with("Default "),
        "{refused}"
    );
}

/// A session is opened only on an added agent, wherever the agent comes from: the request's
/// own choice or the console's default. An undecided list refuses nothing.
#[tokio::test]
async fn opening_a_session_is_refused_when_its_resolved_agent_is_not_added() {
    let (state, _dir) = state_for_opening("coordinator-open-session-agent-not-added");
    let refused = |err: anyhow::Error, agent: AgentId| {
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::AGENT_NOT_ADDED);
        assert_eq!(coded.params["agent"], agent.label());
        assert!(coded.message.contains("Settings → Agent accounts"));
    };

    assert!(check_open(&state, &open_request()).await.is_ok());
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CODEX])
        .unwrap();

    // The console's default agent, Claude, is not added.
    refused(
        check_open(&state, &open_request()).await.unwrap_err(),
        AgentId::CLAUDE,
    );
    let err = open_session(&state, open_request()).await.unwrap_err();
    refused(err, AgentId::CLAUDE);
    let err = open_session_detached(
        &state,
        OpenRequest {
            agent: Some(AgentId::GROK),
            ..open_request()
        },
    )
    .await
    .map(|_| ())
    .unwrap_err();
    refused(err, AgentId::GROK);
    assert!(state.store.list_sessions().unwrap().is_empty());

    let codex = OpenRequest {
        agent: Some(AgentId::CODEX),
        ..open_request()
    };
    assert_eq!(check_open(&state, &codex).await.unwrap(), AgentId::CODEX);
}

/// A list emptied on purpose is decided, so it refuses every agent.
#[tokio::test]
async fn an_empty_added_list_refuses_every_agent() {
    let (state, _dir) = state_for_opening("coordinator-open-session-nothing-added");
    state.store.set_added_agents_if_undecided(&[]).unwrap();
    for agent in AgentId::ALL {
        let err = check_open(
            &state,
            &OpenRequest {
                agent: Some(agent),
                ..open_request()
            },
        )
        .await
        .unwrap_err();
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::AGENT_NOT_ADDED);
    }
}

/// A session's account is its own pick, else the project's reference for its agent, else the
/// console's, else the default account: a project's reference overrides the console's, and one
/// left unset falls back to it, agent by agent. Another agent's account is never borrowed.
#[test]
fn a_sessions_account_is_its_own_pick_else_its_projects_else_its_consoles() {
    let (state, _dir) = crate::test_support::app_state("coordinator-session-account");
    for (id, agent, name, dir) in [
        (
            "claude-acct",
            AgentId::CLAUDE,
            "Claude work",
            "/home/u/.claude-alt",
        ),
        (
            "claude-other",
            AgentId::CLAUDE,
            "Claude other",
            "/home/u/.claude-other",
        ),
        (
            "claude-project",
            AgentId::CLAUDE,
            "Claude project",
            "/home/u/.claude-project",
        ),
        (
            "codex-acct",
            AgentId::CODEX,
            "Codex work",
            "/home/u/.codex-alt",
        ),
        (
            "grok-project",
            AgentId::GROK,
            "Grok project",
            "/home/u/.grok-project",
        ),
    ] {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent,
                name: name.to_string(),
                config_dir: dir.to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();
    }
    let mut console = console(AgentId::CLAUDE, Path::new("/tmp/unused"));
    console.accounts[AgentId::CLAUDE] = Some("claude-acct".to_string());
    console.accounts[AgentId::CODEX] = Some("codex-acct".to_string());

    let resolved = |agent: AgentId, project: Option<&Project>, chosen: Option<Option<&str>>| {
        session_account(
            &state,
            agent,
            &console,
            project,
            chosen.map(|choice| choice.map(str::to_string)),
        )
        .unwrap()
    };
    let account = |id: &str, dir: &str| (Some(id.to_string()), Some(dir.to_string()));
    let project = |claude: ProjectAccount, grok: ProjectAccount| {
        let mut project = project_of("console-1", "project-1");
        project.accounts[AgentId::CLAUDE] = claude;
        project.accounts[AgentId::GROK] = grok;
        Some(project)
    };
    let claude_acct = || account("claude-acct", "/home/u/.claude-alt");
    let claude_other = || account("claude-other", "/home/u/.claude-other");
    let claude_project = || account("claude-project", "/home/u/.claude-project");
    let codex_acct = || account("codex-acct", "/home/u/.codex-alt");

    // (agent, the session's project, the choice made for the session, what it launches under)
    let cases = [
        // No project and no choice: the console's account for the session's own agent.
        (AgentId::CLAUDE, None, None, claude_acct()),
        (AgentId::CODEX, None, None, codex_acct()),
        // Nothing referenced for Grok, and another agent's account is never borrowed for it:
        // the default account, which pins nothing.
        (AgentId::GROK, None, None, (None, None)),
        // A choice wins over the console's reference...
        (
            AgentId::CLAUDE,
            None,
            Some(Some("claude-other")),
            claude_other(),
        ),
        // ...and a choice of the default account wins over it too.
        (AgentId::CLAUDE, None, Some(None), (None, None)),
        // The project's reference wins over the console's...
        (
            AgentId::CLAUDE,
            project(pins("claude-project"), pins("grok-project")),
            None,
            claude_project(),
        ),
        // ...a project that sets nothing for the agent falls back to the console's...
        (
            AgentId::CODEX,
            project(pins("claude-project"), pins("grok-project")),
            None,
            codex_acct(),
        ),
        // ...and to the default account when the console sets nothing either; the project's
        // reference for another agent is never borrowed.
        (
            AgentId::GROK,
            project(pins("claude-project"), pins("grok-project")),
            None,
            account("grok-project", "/home/u/.grok-project"),
        ),
        (
            AgentId::GROK,
            project(pins("claude-project"), ProjectAccount::Inherit),
            None,
            (None, None),
        ),
        // A project that says the default account ends the chain there, where the console would
        // have named an account; the agents it says nothing for still inherit.
        (
            AgentId::CLAUDE,
            project(ProjectAccount::Default, ProjectAccount::Inherit),
            None,
            (None, None),
        ),
        (
            AgentId::CODEX,
            project(ProjectAccount::Default, ProjectAccount::Inherit),
            None,
            codex_acct(),
        ),
        // The session's own pick wins over the project's reference, the default account
        // included, and over a project that says the default account.
        (
            AgentId::CLAUDE,
            project(ProjectAccount::Default, ProjectAccount::Inherit),
            Some(Some("claude-other")),
            claude_other(),
        ),
        (
            AgentId::CLAUDE,
            project(pins("claude-project"), ProjectAccount::Inherit),
            Some(Some("claude-other")),
            claude_other(),
        ),
        (
            AgentId::CLAUDE,
            project(pins("claude-project"), ProjectAccount::Inherit),
            Some(None),
            (None, None),
        ),
    ];
    for (index, (agent, project, chosen, expected)) in cases.into_iter().enumerate() {
        assert_eq!(
            resolved(agent, project.as_ref(), chosen),
            expected,
            "case {index}: {agent:?} {chosen:?}"
        );
    }

    // A choice naming no account, or an account of another agent, is refused.
    for chosen in ["missing", "codex-acct"] {
        let err = session_account(
            &state,
            AgentId::CLAUDE,
            &console,
            None,
            Some(Some(chosen.to_string())),
        )
        .expect_err(chosen);
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT, "{chosen}");
    }
}

/// A project session falls back to the project's own default, then the console's project
/// session agent, then (for a console that names none) `default_agent`; a console session
/// always takes `default_agent`.
#[test]
fn a_session_agent_falls_back_through_the_project_to_the_project_session_default() {
    let workdir = Path::new("/tmp/console-1");
    let mut console = console(AgentId::CLAUDE, workdir);
    let mut project = project_of("console-1", "p1");
    assert_eq!(
        session_agent(None, Some(&project), &console),
        AgentId::CLAUDE
    );

    console.project_session_agent = Some(AgentId::CODEX);
    assert_eq!(
        session_agent(None, Some(&project), &console),
        AgentId::CODEX
    );
    assert_eq!(session_agent(None, None, &console), AgentId::CLAUDE);

    project.default_agent = Some(AgentId::GROK);
    assert_eq!(session_agent(None, Some(&project), &console), AgentId::GROK);
    assert_eq!(
        session_agent(Some(AgentId::CLAUDE), Some(&project), &console),
        AgentId::CLAUDE
    );
}

/// What a listing says of an account's reason: turned off or gone first, then logged out, then a
/// used-up quota, and a running sharing change last, so an account found logged out is not named
/// for a quota it also happens to have used up.
#[test]
fn why_an_account_cannot_open_is_judged_in_a_fixed_order() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-cannot-open");
    let why = |account: Option<&str>| cannot_open(&state, AgentId::CLAUDE, account).unwrap();
    assert!(why(None).is_none());

    hold_used_up(&state, None, Some(60_000), false);
    assert!(matches!(why(None), Some(CannotOpen::QuotaUsedUp(_))));
    hold_used_up(&state, None, Some(60_000), true);
    assert!(matches!(why(None), Some(CannotOpen::SignedOut)));

    insert_claude_account(&state, "off", true);
    insert_claude_account(&state, "on", false);
    assert!(matches!(why(Some("off")), Some(CannotOpen::Unusable)));
    assert!(matches!(why(Some("gone")), Some(CannotOpen::Unusable)));
    assert!(why(Some("on")).is_none());
    let account = state.store.get_account("on").unwrap().unwrap();
    let (_, lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
    assert!(matches!(
        why(Some("on")),
        Some(CannotOpen::SharingInProgress)
    ));
    drop(lock);
    assert!(why(Some("on")).is_none());
}

/// The accounts of an agent as a listing shows them: the default account first under the name the
/// user gave it, then the stored ones, each with its directory and whether it is turned off.
#[test]
fn the_candidate_accounts_are_listed_with_their_names_and_directories() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-candidate-accounts");
    insert_claude_account(&state, "off", true);
    state
        .store
        .set_default_account_name(AgentId::CLAUDE, Some("Mine"))
        .unwrap();

    let listed: Vec<_> = candidate_accounts(&state, AgentId::CLAUDE)
        .unwrap()
        .into_iter()
        .map(|candidate| {
            (
                candidate.id,
                candidate.config_dir,
                candidate.name,
                candidate.disabled,
            )
        })
        .collect();

    let dir_of = |id: &str| state.store.get_account(id).unwrap().unwrap().config_dir;
    assert_eq!(
        listed,
        [
            (None, None, "Mine".to_string(), false),
            (
                Some("claude-a".to_string()),
                Some(dir_of("claude-a")),
                "claude-a".to_string(),
                false
            ),
            (
                Some("off".to_string()),
                Some("/tmp/off".to_string()),
                "off".to_string(),
                true
            ),
        ]
    );
}
