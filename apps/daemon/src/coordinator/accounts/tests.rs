use super::*;
use crate::accounts::dirs::normalize_account_dir;
use crate::accounts::login;
use crate::coordinator::handle;
use crate::coordinator::test_support::{
    console, create_account_request, only_account, pins, project_of, set_availability, temp_dir,
};
use crate::paths;
use crate::protocol::{
    error_code, error_parts, AgentId, Availability, CodedError, Event, ProjectAccount, RequestBody,
};
use crate::state::AppState;
use crate::test_support::console_with_project_and_accounts;
use crate::test_support::{code_of, host_path};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

/// An account directory is stored absolute and lexically normalised or not at all; unlike a
/// console's pinned path before accounts existed, it is accepted even when it does not exist,
/// since it is created on first launch (Grok Build excepted, checked at launch instead).
#[test]
fn an_account_dir_is_stored_absolute_or_not_at_all_and_existence_is_not_checked() {
    let dir = temp_dir("config-dir");
    let text = dir.to_string_lossy().into_owned();
    let normalize = |text: &str| normalize_account_dir(AgentId::CODEX, text);

    assert_eq!(normalize(&format!("  {text} ")).unwrap(), text.clone());
    assert_eq!(normalize(&format!("{text}/")).unwrap(), text.clone());
    assert_eq!(
        normalize(&format!("{text}/./sub/..")).unwrap(),
        text.clone()
    );

    // Now intentionally accepted, where a console's pinned path used to refuse it.
    let missing = dir.join("missing").to_string_lossy().into_owned();
    assert_eq!(normalize(&missing).unwrap(), missing);

    // The refusal names the agent whose field it was.
    let relative = normalize(".codex-alt").unwrap_err();
    assert!(relative.to_string().contains("absolute"), "{relative}");
    assert!(relative.to_string().contains("Codex"), "{relative}");
}

/// An account always requires a non-blank directory.
#[test]
fn an_account_directory_may_not_be_blank() {
    assert!(normalize_account_dir(AgentId::CODEX, "").is_err());
    assert!(normalize_account_dir(AgentId::CODEX, "   ").is_err());
}

/// Turning an account off or on broadcasts the settings only when the flag really changed, and
/// an id that is no account of the agent is refused.
#[tokio::test]
async fn turning_an_account_off_broadcasts_only_a_real_change() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-disabled-broadcast");
    let mut events = state.subscribe();
    let toggle = |agent, account: Option<&str>, disabled| RequestBody::SetAccountDisabled {
        agent,
        account: account.map(str::to_string),
        disabled,
    };
    let flags = |event: Event| match event {
        Event::SettingsUpdated { settings } => (
            settings.accounts.iter().any(|account| account.disabled),
            settings.disabled_default_accounts.len(),
        ),
        other => panic!("unexpected {other:?}"),
    };

    handle(
        &state,
        None,
        toggle(AgentId::CLAUDE, Some("claude-a"), true),
    )
    .await
    .unwrap();
    assert_eq!(flags(events.try_recv().unwrap()), (true, 0));
    handle(
        &state,
        None,
        toggle(AgentId::CLAUDE, Some("claude-a"), true),
    )
    .await
    .unwrap();
    assert!(events.try_recv().is_err(), "a repeat changes nothing");

    handle(&state, None, toggle(AgentId::CLAUDE, None, true))
        .await
        .unwrap();
    assert_eq!(flags(events.try_recv().unwrap()), (true, 1));
    handle(&state, None, toggle(AgentId::CLAUDE, None, true))
        .await
        .unwrap();
    assert!(events.try_recv().is_err());

    for (agent, account) in [(AgentId::CLAUDE, "nope"), (AgentId::CODEX, "claude-a")] {
        let err = handle(&state, None, toggle(agent, Some(account), true))
            .await
            .expect_err("not an account of the agent");
        assert_eq!(code_of(&err), error_code::UNKNOWN_ACCOUNT);
    }
    assert!(events.try_recv().is_err());
    assert!(
        state
            .store
            .get_account("claude-a")
            .unwrap()
            .unwrap()
            .disabled
    );
}

/// Turning an account off drops the reading held for it, and only that one, while a session
/// running on it keeps it; turning it back on reads it at once with no failures held against it.
#[tokio::test]
async fn turning_an_account_off_drops_its_held_reading_unless_a_session_runs_on_it() {
    use crate::protocol::{AccountQuota, SessionStatus};

    let (state, _dir) = console_with_project_and_accounts("coordinator-disabled-reading");
    set_availability(&state, AgentId::CLAUDE, Availability::Available);
    let quotas = &state.account_quotas;
    let reading = |account: Option<&str>| AccountQuota {
        reset_credits: None,
        agent: AgentId::CLAUDE,
        account: account.map(str::to_string),
        fetched_at: 1,
        plan: None,
        email: None,
        windows: Vec::new(),
        unavailable: None,
    };
    let dir = state
        .store
        .account_dir(&Some("claude-a".to_string()))
        .unwrap()
        .unwrap();
    quotas.hold_in_dir_for_test(&reading(Some("claude-a")), &dir);
    quotas.hold_for_test(&reading(None));
    let held = || {
        quotas
            .list()
            .into_iter()
            .map(|quota| quota.account)
            .collect::<Vec<_>>()
    };

    set_account_disabled(&state, AgentId::CLAUDE, Some("claude-a"), true).unwrap();
    assert_eq!(held(), [None], "only the turned-off account's reading goes");
    set_account_disabled(&state, AgentId::CLAUDE, Some("claude-a"), false).unwrap();

    // With a session running on it, the reading stays through the turning off, with the
    // failures it has built up, and the turning back on wipes them and makes it due.
    let mut session = crate::test_support::sessions::project_session(
        "s1",
        AgentId::CLAUDE,
        SessionStatus::Working,
    );
    session.account_id = Some("claude-a".to_string());
    state.store.insert_session(&session).unwrap();
    quotas.hold_in_dir_for_test(&reading(Some("claude-a")), &dir);
    quotas.set_failures_for_test(AgentId::CLAUDE, Some("claude-a"), 4);
    set_account_disabled(&state, AgentId::CLAUDE, Some("claude-a"), true).unwrap();
    assert_eq!(held().len(), 2, "a running session keeps the reading");
    set_account_disabled(&state, AgentId::CLAUDE, Some("claude-a"), false).unwrap();
    // The current-thread runtime has not yet run the background read turning it back on spawned.
    assert_eq!(
        quotas.failures_for_test(AgentId::CLAUDE, Some("claude-a")),
        0
    );
    assert!(quotas.is_due_for_test(AgentId::CLAUDE, Some("claude-a")));
}

/// `~` is the home directory of the host the daemon runs on, like every other path it takes.
#[test]
fn a_leading_tilde_in_a_config_dir_is_expanded() {
    // `$HOME` always exists, so this needs no fixture and no environment edit.
    let home = paths::home_dir();
    assert_eq!(
        normalize_account_dir(AgentId::GROK, "~").unwrap(),
        home.to_string_lossy().into_owned()
    );
}

/// Adding is refused for an agent determined not installed, for one whose availability is not
/// determined yet, and for one already added; each leaves the list as it was.
#[tokio::test]
async fn adding_an_agent_is_refused_unless_it_is_installed_and_not_yet_added() {
    let (state, _dir) = crate::test_support::app_state("coordinator-add-agent");
    for agent in AgentId::ALL {
        set_availability(&state, agent, Availability::Unavailable);
    }
    set_availability(&state, AgentId::CLAUDE, Availability::Available);
    state.store.set_added_agents_if_undecided(&[]).unwrap();

    let add = |agent| handle(&state, None, RequestBody::AddAgent { agent });
    let refusal = |result: Result<Option<Event>>| {
        let err = result.expect_err("refused");
        let (code, params, _) = error_parts(&err);
        (code, params.get("agent").cloned())
    };
    assert_eq!(
        refusal(add(AgentId::CODEX).await),
        (error_code::AGENT_NOT_INSTALLED, Some("Codex".to_string()))
    );
    let mut events = state.subscribe();
    add(AgentId::CLAUDE).await.expect("added");
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings })
            if settings.added_agents == Some(vec![AgentId::CLAUDE])
    ));
    assert_eq!(
        refusal(add(AgentId::CLAUDE).await).0,
        error_code::AGENT_ALREADY_ADDED
    );
    assert_eq!(
        state.store.added_agents().unwrap(),
        Some(vec![AgentId::CLAUDE])
    );

    let (pending, _dir) = crate::test_support::app_state("coordinator-add-agent-pending");
    assert_eq!(
        refusal(
            handle(
                &pending,
                None,
                RequestBody::AddAgent {
                    agent: AgentId::CLAUDE
                }
            )
            .await
        )
        .0,
        error_code::AGENT_AVAILABILITY_PENDING
    );
    assert_eq!(pending.store.added_agents().unwrap(), None);
}

/// An add that arrives once availability is known but before the fill ran settles the list
/// first, so the agents with accounts are not left off it and the agent asked for, being
/// installed, is found already in it.
#[tokio::test]
async fn adding_an_agent_first_settles_an_undecided_list() {
    let (state, _dir) = crate::test_support::app_state("coordinator-add-agent-fill");
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("created");
    for agent in AgentId::ALL {
        set_availability(&state, agent, Availability::Unavailable);
    }
    set_availability(&state, AgentId::CLAUDE, Availability::Available);

    let err = handle(
        &state,
        None,
        RequestBody::AddAgent {
            agent: AgentId::CLAUDE,
        },
    )
    .await
    .expect_err("already in the filled list");

    assert_eq!(error_parts(&err).0, error_code::AGENT_ALREADY_ADDED);
    assert_eq!(
        state.store.added_agents().unwrap(),
        Some(vec![AgentId::CLAUDE, AgentId::CODEX])
    );
}

/// Reordering takes a permutation of the decided list, persists and broadcasts it, answers a
/// repeat without a broadcast, and refuses a missing, repeated or not-added agent.
#[tokio::test]
async fn reordering_agents_takes_only_a_permutation_of_the_list() {
    let (state, _dir) = crate::test_support::app_state("coordinator-reorder-agents");
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE, AgentId::CODEX, AgentId::GROK])
        .unwrap();
    let mut events = state.subscribe();
    let reorder =
        |agents: Vec<AgentId>| handle(&state, None, RequestBody::ReorderAgents { agents });

    reorder(vec![AgentId::GROK, AgentId::CLAUDE, AgentId::CODEX])
        .await
        .expect("reordered");
    let order = Some(vec![AgentId::GROK, AgentId::CLAUDE, AgentId::CODEX]);
    assert_eq!(state.store.added_agents().unwrap(), order);
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings }) if settings.added_agents == order
    ));

    reorder(vec![AgentId::GROK, AgentId::CLAUDE, AgentId::CODEX])
        .await
        .expect("an unchanged order is accepted");
    assert!(events.try_recv().is_err(), "nothing changed");

    for bad in [
        vec![AgentId::GROK, AgentId::CLAUDE],
        vec![AgentId::GROK, AgentId::GROK, AgentId::CLAUDE],
        vec![
            AgentId::GROK,
            AgentId::CLAUDE,
            AgentId::CODEX,
            AgentId::CODEX,
        ],
        vec![],
    ] {
        let err = reorder(bad).await.expect_err("refused");
        assert_eq!(error_parts(&err).0, error_code::AGENT_ORDER_INVALID);
    }
    state.store.remove_added_agent(AgentId::CODEX).unwrap();
    let err = reorder(vec![AgentId::GROK, AgentId::CODEX])
        .await
        .expect_err("a not-added agent is refused");
    assert_eq!(error_parts(&err).0, error_code::AGENT_ORDER_INVALID);
    assert_eq!(
        state.store.added_agents().unwrap(),
        Some(vec![AgentId::GROK, AgentId::CLAUDE])
    );
}

#[tokio::test]
async fn account_order_broadcasts_only_a_changed_valid_order() {
    let (state, _dir) = crate::test_support::app_state("coordinator-reorder-accounts");
    let mut events = state.subscribe();
    let reorder = |accounts| {
        handle(
            &state,
            None,
            RequestBody::ReorderAccounts {
                agent: AgentId::CLAUDE,
                accounts,
            },
        )
    };
    reorder(vec![None]).await.unwrap();
    assert!(
        matches!(events.try_recv(), Ok(Event::SettingsUpdated { settings })
        if settings.account_order["claude"] == vec![None])
    );
    reorder(vec![None]).await.unwrap();
    assert!(events.try_recv().is_err());
    let err = reorder(vec![]).await.unwrap_err();
    assert_eq!(error_parts(&err).0, error_code::ACCOUNT_ORDER_INVALID);
    assert!(events.try_recv().is_err());
}

/// An undecided list is filled first when availability is known, and refused otherwise.
#[tokio::test]
async fn reordering_agents_settles_an_undecided_list_or_is_refused() {
    let (state, _dir) = crate::test_support::app_state("coordinator-reorder-undecided");
    let err = handle(&state, None, RequestBody::ReorderAgents { agents: vec![] })
        .await
        .expect_err("undecided");
    assert_eq!(error_parts(&err).0, error_code::ADDED_AGENTS_UNDECIDED);
    let err = handle(
        &state,
        None,
        RequestBody::RemoveAgent {
            agent: AgentId::CLAUDE,
        },
    )
    .await
    .expect_err("undecided");
    assert_eq!(error_parts(&err).0, error_code::ADDED_AGENTS_UNDECIDED);

    for agent in AgentId::ALL {
        set_availability(&state, agent, Availability::Unavailable);
    }
    set_availability(&state, AgentId::GROK, Availability::Available);
    set_availability(&state, AgentId::CLAUDE, Availability::Available);
    handle(
        &state,
        None,
        RequestBody::ReorderAgents {
            agents: vec![AgentId::GROK, AgentId::CLAUDE],
        },
    )
    .await
    .expect("filled, then reordered");
    assert_eq!(
        state.store.added_agents().unwrap(),
        Some(vec![AgentId::GROK, AgentId::CLAUDE])
    );
}

/// Removing an agent takes it off the list and nothing else: its accounts stay, and clients
/// are told only when the list changed.
#[tokio::test]
async fn removing_an_agent_keeps_its_accounts() {
    let (state, _dir) = crate::test_support::app_state("coordinator-remove-agent");
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CLAUDE, AgentId::CODEX])
        .unwrap();
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("created");
    let account = only_account(&state);
    let mut events = state.subscribe();

    handle(
        &state,
        None,
        RequestBody::RemoveAgent {
            agent: AgentId::CODEX,
        },
    )
    .await
    .expect("removed");

    assert_eq!(
        state.store.added_agents().unwrap(),
        Some(vec![AgentId::CLAUDE])
    );
    assert_eq!(only_account(&state), account);
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { settings })
            if settings.added_agents == Some(vec![AgentId::CLAUDE]) && settings.accounts == [account]
    ));

    handle(
        &state,
        None,
        RequestBody::RemoveAgent {
            agent: AgentId::CODEX,
        },
    )
    .await
    .expect("removing it again is not an error");
    assert!(events.try_recv().is_err(), "nothing changed");
}

/// Creating, renaming, repointing and removing an account over the protocol, each change
/// visible afterwards through the settings record the account list travels with.
#[tokio::test]
async fn an_account_can_be_created_renamed_repointed_and_removed_over_the_protocol() {
    let (state, _dir) = crate::test_support::app_state("coordinator-account-crud");

    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("created");
    let created = only_account(&state);
    assert_eq!(created.name, "Work");
    assert_eq!(created.config_dir, host_path("/home/u/.codex-work"));
    assert_eq!(created.agent, AgentId::CODEX);

    handle(
        &state,
        None,
        RequestBody::UpdateAccount {
            account: created.id.clone(),
            name: Some("Work (renamed)".to_string()),
            config_dir: None,
        },
    )
    .await
    .expect("renamed");
    let renamed = only_account(&state);
    assert_eq!(renamed.name, "Work (renamed)");
    assert_eq!(
        renamed.config_dir,
        host_path("/home/u/.codex-work"),
        "untouched"
    );

    handle(
        &state,
        None,
        RequestBody::UpdateAccount {
            account: created.id.clone(),
            name: None,
            config_dir: Some(host_path("/home/u/.codex-work-2")),
        },
    )
    .await
    .expect("repointed");
    let repointed = only_account(&state);
    assert_eq!(repointed.name, "Work (renamed)", "untouched");
    assert_eq!(repointed.config_dir, host_path("/home/u/.codex-work-2"));

    handle(
        &state,
        None,
        RequestBody::DeleteAccount {
            account: created.id.clone(),
            trash_directory: false,
        },
    )
    .await
    .expect("removed");
    assert!(state.store.list_accounts().unwrap().is_empty());
}

/// A name colliding within its agent is refused, naming the account it collides with — against
/// an existing account, against the default account's own name, and not at all across agents.
#[tokio::test]
async fn a_colliding_account_name_is_refused_within_its_agent_only() {
    let (state, _dir) = crate::test_support::app_state("coordinator-account-name-collision");
    handle(
        &state,
        None,
        create_account_request(AgentId::CLAUDE, "Work", &host_path("/home/u/.claude-work")),
    )
    .await
    .expect("created");

    // Trimmed and case-insensitive, against an existing account of the same agent.
    let refused = handle(
        &state,
        None,
        create_account_request(
            AgentId::CLAUDE,
            "  work ",
            &host_path("/home/u/.claude-other"),
        ),
    )
    .await
    .expect_err("collides");
    let coded = refused.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(coded.code, error_code::ACCOUNT_NAME_TAKEN);
    assert_eq!(coded.params.get("name").map(String::as_str), Some("Work"));

    // The default account's own name takes part in the comparison too.
    let refused_default = handle(
        &state,
        None,
        create_account_request(
            AgentId::CLAUDE,
            "default",
            &host_path("/home/u/.claude-other"),
        ),
    )
    .await
    .expect_err("collides with the default account");
    let coded = refused_default.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(coded.code, error_code::ACCOUNT_NAME_TAKEN);

    // The same name is free for another agent: names are not compared across agents.
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("a different agent, so no collision");
    assert_eq!(state.store.list_accounts().unwrap().len(), 2);
}

/// A directory another account of the agent already uses is refused on create and on update,
/// however it is written (`~/`, a trailing slash, `./`), naming that account; the default
/// account's resolved directory counts as taken; an account may keep its own directory; and
/// another agent may use the same path.
#[tokio::test]
async fn a_taken_account_directory_is_refused_within_its_agent_only() {
    let (state, _dir) = crate::test_support::app_state("coordinator-account-dir-collision");
    let home = crate::paths::home_dir();
    let home_dir = home.to_string_lossy().into_owned();
    // Before availability is determined, the conventional default directory takes part.
    let before = handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Early", "~/.codex/"),
    )
    .await
    .expect_err("the conventional default directory is taken");
    let coded = before.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(coded.code, error_code::CONFIG_DIR_TAKEN);
    assert_eq!(
        coded.params.get("name").map(String::as_str),
        Some("Default")
    );
    state.set_agent_availability(vec![crate::protocol::AgentAvailability {
        agent: AgentId::CLAUDE,
        availability: Availability::Available,
        default_account_dir: Some(format!("{home_dir}/.claude")),
    }]);
    handle(
        &state,
        None,
        create_account_request(AgentId::CLAUDE, "Work", &host_path("/home/u/.claude-work")),
    )
    .await
    .expect("created");
    handle(
        &state,
        None,
        create_account_request(AgentId::CLAUDE, "Home", "~/.claude-home"),
    )
    .await
    .expect("created");

    let taken_by = |err: anyhow::Error| {
        let coded = err.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::CONFIG_DIR_TAKEN);
        assert_eq!(
            coded.params.get("agent").map(String::as_str),
            Some("Claude Code")
        );
        coded.params.get("name").cloned().expect("name")
    };
    for (dir, owner) in [
        (host_path("/home/u/.claude-work").as_str(), "Work"),
        (host_path("/home/u/.claude-work/").as_str(), "Work"),
        (
            format!("  {}  ", host_path("/home/u/./.claude-work")).as_str(),
            "Work",
        ),
        (host_path("/home/u/x/../.claude-work").as_str(), "Work"),
        ("~/.claude-home", "Home"),
        (&format!("{home_dir}/.claude-home"), "Home"),
        ("~/.claude", "Default"),
        ("~/.claude/", "Default"),
    ] {
        let err = handle(
            &state,
            None,
            create_account_request(AgentId::CLAUDE, "New", dir),
        )
        .await
        .expect_err(dir);
        assert_eq!(taken_by(err), owner, "{dir}");
    }
    assert_eq!(state.store.list_accounts().unwrap().len(), 2);

    // Another agent may use the same path.
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.claude-work")),
    )
    .await
    .expect("a different agent, so no collision");

    // An update into a taken directory is refused; keeping its own directory is not.
    let accounts = state.store.list_accounts().unwrap();
    let work = accounts
        .iter()
        .find(|a| a.agent == AgentId::CLAUDE && a.name == "Work")
        .unwrap();
    for dir in ["~/.claude-home", "~/.claude"] {
        let err = handle(
            &state,
            None,
            RequestBody::UpdateAccount {
                account: work.id.clone(),
                name: None,
                config_dir: Some(dir.to_string()),
            },
        )
        .await
        .expect_err(dir);
        taken_by(err);
    }
    handle(
        &state,
        None,
        RequestBody::UpdateAccount {
            account: work.id.clone(),
            name: Some("Work 2".to_string()),
            config_dir: Some(host_path("/home/u/.claude-work/")),
        },
    )
    .await
    .expect("its own directory is not a collision");
    assert_eq!(
        state.store.get_account(&work.id).unwrap().unwrap().name,
        "Work 2"
    );
}

/// Two creates racing on one name, and on one directory, let exactly one through: the checks
/// and the insert are a single critical section. The names differ in a non-ASCII letter's case
/// on purpose: the table's `LOWER` index folds ASCII only, so a plain "Work"/"work" pair would
/// be caught by it even without the lock, and only the coordinator's check under the lock
/// catches this one. Threads are released together by a barrier
/// and each runs the synchronous `create_account` directly, as concurrent requests on the
/// runtime's worker threads do.
#[test]
fn racing_account_creates_let_exactly_one_through() {
    for (first, second) in [
        (
            ("Équipe", host_path("/home/u/.claude-a").as_str()),
            ("équipe", host_path("/home/u/.claude-b").as_str()),
        ),
        (
            ("Work", host_path("/home/u/.claude-c").as_str()),
            ("Other", host_path("/home/u/.claude-c/").as_str()),
        ),
    ] {
        for round in 0..20 {
            let (state, _dir) = crate::test_support::app_state("coordinator-account-create-race");
            let barrier = std::sync::Barrier::new(2);
            let results: Vec<Result<()>> = std::thread::scope(|scope| {
                [first, second]
                    .map(|(name, dir)| {
                        let (state, barrier) = (&state, &barrier);
                        scope.spawn(move || {
                            barrier.wait();
                            create_account(
                                state,
                                AgentId::CLAUDE,
                                name.to_string(),
                                dir.to_string(),
                            )
                        })
                    })
                    .into_iter()
                    .map(|handle| handle.join().expect("no panic"))
                    .collect()
            });
            assert_eq!(
                results.iter().filter(|result| result.is_ok()).count(),
                1,
                "round {round}: {results:?}"
            );
            assert_eq!(state.store.list_accounts().unwrap().len(), 1);
        }
    }
}

/// Where the host's file system folds case (macOS, Windows) the directory comparison does too;
/// elsewhere it is byte-exact. The stored text stays as typed either way.
#[tokio::test]
async fn account_directories_fold_case_where_the_host_does() {
    let (state, _dir) = crate::test_support::app_state("coordinator-account-dir-case");
    handle(
        &state,
        None,
        create_account_request(AgentId::CLAUDE, "Work", &host_path("/home/u/.Claude-Work")),
    )
    .await
    .expect("created");
    let second = handle(
        &state,
        None,
        create_account_request(AgentId::CLAUDE, "Other", &host_path("/home/u/.claude-work")),
    )
    .await;
    if crate::platform::names::FOLDS_CASE {
        let coded = second.expect_err("same directory on a case-insensitive file system");
        let coded = coded.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::CONFIG_DIR_TAKEN);
    } else {
        second.expect("a different directory on a case-sensitive file system");
    }
    assert_eq!(
        state.store.list_accounts().unwrap()[0].config_dir,
        host_path("/home/u/.Claude-Work"),
        "stored as typed"
    );
}

#[tokio::test]
async fn refreshing_quotas_refuses_a_foreign_or_unknown_account_and_acks_otherwise() {
    let (state, _dir) = crate::test_support::app_state("coordinator-refresh-quotas");
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("created");
    let codex_account = only_account(&state).id;

    for (agent, account) in [
        (Some(AgentId::CLAUDE), Some(codex_account.clone())),
        (None, Some("no-such-account".to_string())),
    ] {
        let refused = handle(
            &state,
            None,
            RequestBody::RefreshAccountQuotas { agent, account },
        )
        .await
        .expect_err("not an account of that agent");
        let coded = refused.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
    }

    // Nothing is available until the shell has been read, so nothing is run.
    for (agent, account) in [
        (None, None),
        (Some(AgentId::CODEX), None),
        (None, Some(codex_account.clone())),
        (Some(AgentId::CODEX), Some(codex_account)),
    ] {
        let reply = handle(
            &state,
            Some("req-1".to_string()),
            RequestBody::RefreshAccountQuotas { agent, account },
        )
        .await
        .expect("accepted");
        assert!(reply.is_none());
    }
}

#[tokio::test]
async fn a_login_is_refused_for_a_foreign_account_and_a_cancel_for_a_login_that_is_not_running() {
    let (state, _dir) = crate::test_support::app_state("coordinator-account-login");
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("created");
    let codex_account = only_account(&state).id;

    let refused = handle(
        &state,
        None,
        RequestBody::StartAccountLogin {
            agent: AgentId::CLAUDE,
            account: Some(codex_account.clone()),
        },
    )
    .await
    .expect_err("not an account of that agent");
    assert_eq!(
        refused.downcast_ref::<CodedError>().unwrap().code,
        error_code::UNKNOWN_ACCOUNT
    );
    for account in [None, Some(codex_account)] {
        let refused = handle(
            &state,
            None,
            RequestBody::CancelAccountLogin {
                agent: AgentId::CODEX,
                account,
            },
        )
        .await
        .expect_err("no login is running");
        assert_eq!(
            refused.downcast_ref::<CodedError>().unwrap().code,
            error_code::LOGIN_NOT_RUNNING
        );
    }
}

/// A sign-in for a provider with no flow is refused with `provider_unsupported`, for the default
/// account and an account of its own, before any login is registered and before the shell
/// environment is taken: under test there is none, so a capture coming first would fail instead.
#[tokio::test]
async fn a_sign_in_is_refused_for_a_provider_with_no_login_flow() {
    let (state, _dir) = crate::test_support::app_state("coordinator-login-unsupported");
    handle(
        &state,
        None,
        create_account_request(
            AgentId::TEST,
            "Work",
            &host_path("/home/u/.test-agent-work"),
        ),
    )
    .await
    .expect("created");
    let account = only_account(&state).id;
    for account in [None, Some(account)] {
        let refused = handle(
            &state,
            None,
            RequestBody::StartAccountLogin {
                agent: AgentId::TEST,
                account,
            },
        )
        .await
        .expect_err("no flow");
        let coded = refused.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PROVIDER_UNSUPPORTED);
        assert_eq!(coded.params["agent"], "test_agent");
        assert_eq!(coded.params["operation"], "sign_in");
        assert!(state.account_logins.running().is_empty());
    }
}

#[tokio::test]
async fn the_default_account_takes_a_name_that_no_account_of_its_agent_shares_and_clears_it() {
    let (state, _dir) = crate::test_support::app_state("coordinator-default-name");
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("created");
    let rename = |agent, name: &str| RequestBody::SetDefaultAccountName {
        agent,
        name: name.to_string(),
    };
    let code_of = |err: anyhow::Error| err.downcast_ref::<CodedError>().unwrap().code;
    let names = |state: &Arc<AppState>| state.store.get_settings().unwrap().default_account_names;

    // The default cannot take an owned account's name, in any letter case.
    let refused = handle(&state, None, rename(AgentId::CODEX, " work "))
        .await
        .expect_err("taken");
    assert_eq!(code_of(refused), error_code::ACCOUNT_NAME_TAKEN);
    // Another agent's accounts are not in the way.
    handle(&state, None, rename(AgentId::CLAUDE, "Work"))
        .await
        .unwrap();
    handle(&state, None, rename(AgentId::CODEX, "  Personal "))
        .await
        .unwrap();
    assert_eq!(names(&state)["codex"], "Personal");
    assert_eq!(names(&state)["claude"], "Work");

    // An owned account cannot take it either, and a rename to its own name is no collision.
    let refused = handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "personal", &host_path("/home/u/.codex-p")),
    )
    .await
    .expect_err("taken by the default");
    assert_eq!(code_of(refused), error_code::ACCOUNT_NAME_TAKEN);
    handle(&state, None, rename(AgentId::CODEX, "Personal"))
        .await
        .unwrap();
    // Nor is a change of letter case alone.
    handle(&state, None, rename(AgentId::CODEX, "PERSONAL"))
        .await
        .unwrap();
    assert_eq!(names(&state)["codex"], "PERSONAL");
    // The word the daemon reserves for the default stays the default's alone to keep.
    let refused = handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "default", &host_path("/home/u/.codex-d")),
    )
    .await
    .expect_err("reserved");
    assert_eq!(code_of(refused), error_code::ACCOUNT_NAME_TAKEN);

    // A blank name clears it back, for that agent only, and frees the name.
    let mut events = state.subscribe();
    handle(&state, None, rename(AgentId::CODEX, "  "))
        .await
        .unwrap();
    assert!(!names(&state).contains_key("codex"));
    assert_eq!(names(&state)["claude"], "Work");
    assert!(matches!(
        events.try_recv(),
        Ok(Event::SettingsUpdated { .. })
    ));
    // Clearing what is not set changes nothing and says nothing.
    handle(&state, None, rename(AgentId::CODEX, ""))
        .await
        .unwrap();
    assert!(events.try_recv().is_err());
    // The word for an unnamed default, in any letter case, clears a name like a blank does.
    handle(&state, None, rename(AgentId::CODEX, "Personal"))
        .await
        .unwrap();
    handle(&state, None, rename(AgentId::CODEX, " DEFAULT "))
        .await
        .unwrap();
    assert!(!names(&state).contains_key("codex"));
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "personal", &host_path("/home/u/.codex-p")),
    )
    .await
    .expect("free again");
}

#[tokio::test]
async fn deleting_or_repointing_an_account_ends_its_running_login_as_cancelled() {
    let (state, dir) = crate::test_support::app_state("coordinator-login-ends");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let env = crate::test_support::fake_agent(&bin, "claude", "sleep 30");
    for repoint in [true, false] {
        let home = dir.join("claude-work");
        handle(
            &state,
            None,
            create_account_request(AgentId::CLAUDE, "Work", &home.to_string_lossy()),
        )
        .await
        .expect("created");
        let account = only_account(&state).id;
        let mut events = state.subscribe();
        login::AccountLogins::start_for_test(
            &state,
            login::Start {
                agent: AgentId::CLAUDE,
                account: Some(account.clone()),
                config_dir: Some(home.to_string_lossy().into_owned()),
                resolved_dir: Some(home.to_string_lossy().into_owned()),
            },
            env.clone(),
            dir.join("run"),
        )
        .unwrap();
        assert_eq!(state.account_logins.running().len(), 1);

        let request = if repoint {
            RequestBody::UpdateAccount {
                account: account.clone(),
                name: None,
                config_dir: Some(dir.join("elsewhere").to_string_lossy().into_owned()),
            }
        } else {
            RequestBody::DeleteAccount {
                account: account.clone(),
                trash_directory: false,
            }
        };
        handle(&state, None, request).await.expect("accepted");
        let ended = tokio::time::timeout(crate::test_support::PATIENCE, async {
            loop {
                if let Ok(Event::AccountLoginUpdated { login }) = events.recv().await {
                    if login.status == crate::protocol::LoginStatus::Cancelled {
                        return login;
                    }
                }
            }
        })
        .await
        .expect("the login was cancelled");
        assert_eq!(ended.account.as_deref(), Some(account.as_str()));
        assert!(state.account_logins.running().is_empty());
        if repoint {
            handle(
                &state,
                None,
                RequestBody::DeleteAccount {
                    account,
                    trash_directory: false,
                },
            )
            .await
            .expect("deleted");
        }
    }
}

#[tokio::test]
async fn repointing_or_deleting_an_account_drops_its_quota_reading() {
    let (state, _dir) = crate::test_support::app_state("coordinator-forget-quotas");
    handle(
        &state,
        None,
        create_account_request(AgentId::CODEX, "Work", &host_path("/home/u/.codex-work")),
    )
    .await
    .expect("created");
    let account = only_account(&state).id;
    let hold = |state: &Arc<AppState>| {
        state
            .account_quotas
            .hold_for_test(&crate::protocol::AccountQuota {
                reset_credits: None,
                agent: AgentId::CODEX,
                account: Some(account.clone()),
                fetched_at: 1,
                plan: None,
                email: None,
                windows: Vec::new(),
                unavailable: None,
            });
    };

    hold(&state);
    assert_eq!(state.account_quotas.list().len(), 1);
    handle(
        &state,
        None,
        RequestBody::UpdateAccount {
            account: account.clone(),
            name: Some("Renamed".to_string()),
            config_dir: None,
        },
    )
    .await
    .expect("updated");
    // A rename leaves the reading alone; repointing the account drops it.
    assert_eq!(state.account_quotas.list().len(), 1);
    handle(
        &state,
        None,
        RequestBody::UpdateAccount {
            account: account.clone(),
            name: None,
            config_dir: Some(host_path("/home/u/.codex-other")),
        },
    )
    .await
    .expect("repointed");
    assert!(state.account_quotas.list().is_empty());

    hold(&state);
    handle(
        &state,
        None,
        RequestBody::DeleteAccount {
            account: account.clone(),
            trash_directory: false,
        },
    )
    .await
    .expect("deleted");
    assert!(state.account_quotas.list().is_empty());
}

/// A non-absolute directory is refused with nothing saved; a directory that does not exist at
/// all is accepted — the two rules this milestone changes about a config directory.
#[tokio::test]
async fn a_non_absolute_account_dir_is_refused_and_a_non_existent_one_is_accepted() {
    let (state, _dir) = crate::test_support::app_state("coordinator-account-dir-rules");

    let refused = handle(
        &state,
        None,
        create_account_request(AgentId::CLAUDE, "Relative", "relative/path"),
    )
    .await
    .expect_err("not absolute");
    let coded = refused.downcast_ref::<CodedError>().expect("coded");
    assert_eq!(coded.code, error_code::CONFIG_DIR_NOT_ABSOLUTE);
    assert!(
        state.store.list_accounts().unwrap().is_empty(),
        "nothing saved"
    );

    handle(
        &state,
        None,
        create_account_request(
            AgentId::CLAUDE,
            "Not yet created",
            &host_path("/tmp/octoboard-does-not-exist"),
        ),
    )
    .await
    .expect("a missing directory is accepted");
    assert_eq!(
        only_account(&state).config_dir,
        host_path("/tmp/octoboard-does-not-exist")
    );
}

/// Removing an account publishes the projects whose reference it cleared, and only those.
#[tokio::test]
async fn removing_an_account_publishes_only_the_projects_it_cleared() {
    let (state, _dir) =
        crate::test_support::app_state("coordinator-account-delete-publishes-projects");
    handle(
        &state,
        None,
        create_account_request(AgentId::CLAUDE, "Work", &host_path("/home/u/.claude-work")),
    )
    .await
    .expect("created");
    let account_id = only_account(&state).id;

    let mut referring = console(AgentId::CLAUDE, Path::new("/tmp/unused"));
    referring.id = "console-referring".to_string();
    state.store.insert_console(&referring).unwrap();
    let mut referring_project = project_of("console-referring", "project-referring");
    referring_project.accounts[AgentId::CLAUDE] = pins(&account_id);
    state.store.insert_project(&referring_project).unwrap();
    let mut defaulting_project = project_of("console-referring", "project-defaulting");
    defaulting_project.accounts[AgentId::CLAUDE] = ProjectAccount::Default;
    state.store.insert_project(&defaulting_project).unwrap();
    let inheriting_project = project_of("console-referring", "project-inheriting");
    state.store.insert_project(&inheriting_project).unwrap();
    let mut events = state.subscribe();

    handle(
        &state,
        None,
        RequestBody::DeleteAccount {
            account: account_id,
            trash_directory: false,
        },
    )
    .await
    .expect("removed");

    let mut broadcast_projects = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::ProjectUpserted { project } = event {
            broadcast_projects.push((project.id, project.accounts[AgentId::CLAUDE].clone()));
        }
    }
    assert_eq!(
        broadcast_projects,
        [("project-referring".to_string(), ProjectAccount::Inherit)],
        "the cleared project is published, and only it"
    );
}
