//! The dispatcher's cross-area tests, and the tests of what is kept for an interrupted owner and
//! written in at its relaunch, through the coordinator fixtures.

use crate::coordinator::consoles::{update_console, ConsoleChanges};
use crate::coordinator::handle;
use crate::coordinator::launch::relaunch_session_with;
use crate::coordinator::lifecycle::archive_session;
use crate::coordinator::open::{check_open, OpenRequest};
use crate::coordinator::projects::{
    add_project, update_project, AddProjectRequest, NewProjectSettings, ProjectChanges,
};
use crate::coordinator::test_support::{
    binding_fixture, bound_session, console, console_workdir_in, create_account_request,
    no_process_with_conversation, only_account, open_request, project_of, relaunch_stand_in,
    set_availability,
};
use crate::protocol::DefaultModelSource;
use crate::protocol::{
    error_code, Agent, Availability, CodedError, Event, Launch, LaunchChange, PerAgent, Project,
    ProjectSource, RequestBody, Session, SessionStatus,
};
use crate::state::AppState;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::code_of;
use crate::test_support::{console_session, project_session};
use std::sync::Arc;
use std::time::Duration;

/// Every request that removes a session's record drops what the daemon holds for it as well —
/// its saved output, the accounts it left for a usage limit, the hand a session request raised —
/// including the two where the record goes by cascade; and a console takes its stalled
/// directory pass with it.
#[tokio::test]
async fn a_session_record_takes_what_is_held_for_it_with_it() {
    let requests = [
        RequestBody::DeleteSession {
            session: "s".to_string(),
            trash_agent_records: false,
            only_if_unused: false,
            force_remove: Vec::new(),
            keep: Vec::new(),
        },
        RequestBody::DeleteProject {
            project: "project-1".to_string(),
            stop_sessions: false,
            trash_directory: false,
            force_remove: Vec::new(),
            inspection: None,
        },
        RequestBody::DeleteConsole {
            console: "console-1".to_string(),
        },
    ];
    for request in requests {
        let (state, dir) = crate::test_support::app_state("coordinator-saved-output-deleted");
        let workdir = console_workdir_in(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: "/tmp/project-1".to_string(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                accounts: Default::default(),
                launch: Default::default(),
                trust_consent: false,
                pinned: false,
                tags: Vec::new(),
                description: None,
                git_workflow: Default::default(),
            })
            .unwrap();
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        // Raised while it ran, and left behind by the archive.
        state.raise_request_hand("s").unwrap();
        state.archive_session_record("s").unwrap();
        assert!(state.has_request_hand("s"));
        let saved = dir.join("output").join("s");
        std::fs::create_dir_all(saved.parent().unwrap()).unwrap();
        std::fs::write(&saved, b"last output").unwrap();
        assert_eq!(state.saved_output("s"), Some(b"last output".to_vec()));
        state.note_left_for_limit("s", Some("account-1"));
        let stuck = Arc::new(());
        state.stalled_directory_passes.record_from(
            "console-1".to_string(),
            Arc::downgrade(&stuck),
            std::time::Instant::now() - Duration::from_secs(1),
        );
        assert!(state.stalled_directory_passes.is_stuck("console-1"));

        let label = format!("{request:?}");
        let console_goes = matches!(request, RequestBody::DeleteConsole { .. });
        handle(&state, None, request).await.unwrap();
        assert!(state.store.get_session("s").unwrap().is_none(), "{label}");
        assert!(!saved.exists(), "{label}");
        assert!(
            state
                .recently_left_for_limit("s", Duration::from_secs(3600))
                .is_empty(),
            "{label}"
        );
        assert!(!state.has_request_hand("s"), "{label}");
        assert_eq!(
            state.stalled_directory_passes.is_stuck("console-1"),
            !console_goes,
            "{label}"
        );
    }
}

/// `list_agent_models` answers with the request echoed (Claude Code's list needs no
/// subprocess), and refuses an account that does not exist or belongs to another agent: the
/// only way it ever errors, since anything wrong with asking the agent is an empty answer.
#[tokio::test]
async fn listing_agent_models_echoes_the_request_and_refuses_a_foreign_account() {
    let (state, _dir) = crate::test_support::app_state("coordinator-list-agent-models");
    handle(
        &state,
        Some("req-1".to_string()),
        create_account_request(Agent::Codex, "Work", "/home/u/.codex-work"),
    )
    .await
    .expect("created");
    let codex_account = only_account(&state).id;

    let reply = handle(
        &state,
        Some("req-2".to_string()),
        RequestBody::ListAgentModels {
            agent: Agent::Claude,
            account: None,
        },
    )
    .await
    .expect("answered");
    match reply {
        Some(Event::AgentModels {
            id,
            agent,
            account,
            models,
            default_model,
            default_model_source,
            unavailable,
        }) => {
            assert_eq!(id.as_deref(), Some("req-2"));
            assert_eq!((agent, account), (Agent::Claude, None));
            assert_eq!(models.len(), 4);
            // The test home holds no catalogue, so nothing is known to be usable.
            assert_eq!((default_model, default_model_source), (None, None));
            assert!(unavailable.is_none());
        }
        other => panic!("expected agent_models, got {other:?}"),
    }

    for account in [codex_account, "no-such-account".to_string()] {
        let refused = handle(
            &state,
            None,
            RequestBody::ListAgentModels {
                agent: Agent::Claude,
                account: Some(account),
            },
        )
        .await
        .expect_err("not a Claude account");
        let coded = refused.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
    }
}

/// The reply carries the model an unset picker resolves to: the best model the account may use,
/// or the one Claude Code's own configuration names.
#[tokio::test]
async fn the_agent_models_reply_names_the_default_model_of_the_account() {
    use crate::agents::models::claude_fixtures::{write_claude_account, CLAUDE_CATALOGUE};
    let (state, dir) = crate::test_support::app_state("coordinator-agent-models-default");
    let pinned = dir.join("pinned");
    std::fs::create_dir_all(&pinned).unwrap();
    write_claude_account(
        &pinned.join(".claude.json"),
        &pinned,
        "[]",
        Some(CLAUDE_CATALOGUE),
    );
    handle(
        &state,
        None,
        create_account_request(Agent::Claude, "Work", &pinned.to_string_lossy()),
    )
    .await
    .expect("created");
    let account = only_account(&state).id;
    let default_model = || async {
        let reply = handle(
            &state,
            None,
            RequestBody::ListAgentModels {
                agent: Agent::Claude,
                account: Some(account.clone()),
            },
        )
        .await
        .expect("answered");
        match reply {
            Some(Event::AgentModels {
                default_model,
                default_model_source,
                models,
                ..
            }) => {
                assert_eq!(models[1].available, Some(true));
                (default_model, default_model_source)
            }
            other => panic!("expected agent_models, got {other:?}"),
        }
    };
    assert_eq!(
        default_model().await,
        (
            Some("opus".to_string()),
            Some(DefaultModelSource::BestAvailable)
        )
    );
    std::fs::write(pinned.join("settings.json"), r#"{"model":"claude-x-1"}"#).unwrap();
    assert_eq!(
        default_model().await,
        (
            Some("claude-x-1".to_string()),
            Some(DefaultModelSource::Configured)
        )
    );
}

#[tokio::test]
async fn using_a_reset_credit_is_refused_for_a_foreign_or_unknown_account() {
    let (state, _dir) = crate::test_support::app_state("coordinator-use-reset-credit");
    handle(
        &state,
        None,
        create_account_request(Agent::Claude, "Work", "/home/u/.claude-work"),
    )
    .await
    .expect("created");
    let claude_account = only_account(&state).id;
    let use_on = |account: Option<String>| RequestBody::UseCodexResetCredit { account };
    let code_of = |refused: anyhow::Error| refused.downcast_ref::<CodedError>().unwrap().code;

    // Refused before any shell is asked for; `reset_credit_shell` has none under test either,
    // so a request that got through would fail on that instead of reaching Codex.
    set_availability(&state, Agent::Codex, Availability::Available);
    for account in [Some(claude_account), Some("no-such-account".to_string())] {
        let refused = handle(&state, None, use_on(account)).await.unwrap_err();
        assert_eq!(code_of(refused), error_code::UNKNOWN_ACCOUNT);
    }
}

/// A project session that sets no model takes the console's project session one, so an effort
/// given alone is checked against that model's levels wherever it is given: a project update,
/// an opened session, and the console's own project session pair.
#[tokio::test]
async fn an_effort_given_alone_is_checked_against_the_model_it_inherits_from_the_console() {
    let (state, dir) = crate::test_support::app_state("coordinator-inherited-model-check");
    crate::test_support::offer_models(
        &state,
        Agent::Claude,
        &[("fallback", &["low"]), ("other", &["high"])],
    );
    let workdir = console_workdir_in(&dir);
    let mut owner = console(Agent::Claude, &workdir);
    owner.project_session_launch.claude.model = Some("fallback".to_string());
    state.store.insert_console(&owner).unwrap();
    state
        .store
        .insert_project(&project_of("console-1", "project-1"))
        .unwrap();
    let refused = |err: anyhow::Error| err.downcast_ref::<CodedError>().expect("coded").code;

    // A project given only an effort the console's model does not take.
    let effort_only = |effort: &str| LaunchChange {
        model: None,
        effort: Some(Some(effort.to_string())),
    };
    let mut changes = ProjectChanges::default();
    changes.launch.insert(Agent::Claude, effort_only("high"));
    let err = update_project(&state, "project-1", changes)
        .await
        .expect_err("effort of another model");
    assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
    let mut changes = ProjectChanges::default();
    changes.launch.insert(Agent::Claude, effort_only("low"));
    update_project(&state, "project-1", changes)
        .await
        .expect("the inherited model's own effort");

    // A project added with only an effort is checked the same way.
    let added = |name: &str, effort: &str| {
        let repo = dir.join(name);
        std::fs::create_dir(&repo).expect("repo");
        add_project(
            &state,
            AddProjectRequest {
                console_id: "console-1".to_string(),
                source: ProjectSource::Local,
                path: Some(repo.to_string_lossy().into_owned()),
                remote_url: None,
                name: None,
                default_agent: None,
                settings: NewProjectSettings {
                    launch: PerAgent {
                        claude: Launch {
                            model: None,
                            effort: Some(effort.to_string()),
                        },
                        ..Default::default()
                    },
                    ..NewProjectSettings::default()
                },
                detect_default_agent: false,
                tags: None,
                description: None,
                check_launch: true,
                check_text_limits: true,
            },
        )
    };
    let err = added("rejected", "high")
        .await
        .expect_err("effort of another model");
    assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
    added("accepted", "low").await.expect("accepted");

    // A project session opened with only that effort.
    let open = |effort: &str| OpenRequest {
        project_id: Some("project-1".to_string()),
        effort: Some(Some(effort.to_string())),
        ..open_request()
    };
    let err = check_open(&state, &open("high"))
        .await
        .expect_err("effort of another model");
    assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
    check_open(&state, &open("low")).await.expect("accepted");

    // The console's own project session pair is checked like any other setting.
    let mut changes = ConsoleChanges::default();
    changes.project_launch.insert(
        Agent::Claude,
        LaunchChange {
            model: Some(Some("unlisted".to_string())),
            effort: None,
        },
    );
    let err = update_console(&state, "console-1", changes)
        .await
        .expect_err("model not offered");
    assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
}

/// Information shared with an interrupted session stays in the store through a launch that
/// fails, and through a successful one until its message has been written into the session:
/// a queue lost before then (the process ends, the daemon exits) leaves the rows for the next
/// resume, which queues each message once, in the order the information arrived. Archiving or
/// deleting the session drops what is kept.
#[tokio::test]
async fn information_kept_for_an_interrupted_session_is_deleted_only_once_it_is_written() {
    use SessionStatus::Idle;
    let interrupt = |state: &Arc<AppState>, id: &str| {
        let mut session = state.session_record(id).unwrap();
        session.status = SessionStatus::Interrupted;
        state.store.update_session(&session).unwrap();
    };
    let (state, _dir, _live) = binding_fixture(
        "kept-info",
        vec![
            console_session("hub", Idle),
            no_process_with_conversation("codex", Agent::Codex, "hub"),
            no_process_with_conversation("archived-later", Agent::Codex, "hub"),
            no_process_with_conversation("deleted-later", Agent::Codex, "hub"),
        ],
        &["hub"],
    );
    let share = |target: &str, text: &str| {
        let (delivery, _) =
            crate::messaging::sharing::deliver_info(&state, "hub", target, text).unwrap();
        assert_eq!(delivery, crate::messaging::delivery::Delivery::Kept);
    };
    share("codex", "first news");
    share("codex", "second news");
    assert!(state.take_queued("codex").is_empty());

    relaunch_stand_in(&state, "codex", true)
        .await
        .expect_err("the launch fails");
    assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);
    assert_eq!(state.queued_count("codex"), 0);

    // A launch that succeeds queues them, and keeps the rows until they are written.
    let (_, queued) = relaunch_stand_in(&state, "codex", false).await.unwrap();
    assert_eq!(queued.len(), 2, "{queued:?}");
    assert!(queued[0].contains("first news"), "{queued:?}");
    assert!(queued[1].contains("second news"), "{queued:?}");
    assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);

    // The queue is lost before it drained, as when the daemon exits or the process ends: the
    // next resume queues each message once.
    crate::coordinator::exit_watch::end_process_for_test(&state, "codex");
    interrupt(&state, "codex");
    assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);
    let (_, queued) = relaunch_stand_in(&state, "codex", false).await.unwrap();
    assert_eq!(queued.len(), 2, "{queued:?}");

    // Written into the session, the rows go.
    crate::coordinator::exit_watch::end_process_for_test(&state, "codex");
    interrupt(&state, "codex");
    let started = Arc::new(std::sync::Mutex::new(Vec::new()));
    let launches = Arc::new(std::sync::Mutex::new(Vec::new()));
    let claim = state.begin_launch("codex").unwrap();
    relaunch_session_with(
        &state,
        &claim,
        "codex",
        None,
        true,
        None,
        None,
        crate::test_support::stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();
    drop(claim);
    assert_eq!(state.store.kept_messages("codex").unwrap().len(), 2);
    let live = state.live_session("codex").expect("running");
    state.flush_outbox("codex", SessionStatus::Idle);
    let seen = crate::test_support::output_containing(&live, "second news");
    assert!(seen.contains("first news"), "{seen}");
    assert!(state.store.kept_messages("codex").unwrap().is_empty());
    assert_eq!(state.queued_count("codex"), 0);

    // A share that meets a launch under way keeps nothing: the launch may already have read
    // what was kept.
    let claim = state.begin_launch("archived-later").unwrap();
    assert!(state
        .keep_message_if_not_running("archived-later", "too late")
        .unwrap()
        .is_none());
    drop(claim);
    assert!(state
        .store
        .kept_messages("archived-later")
        .unwrap()
        .is_empty());

    share("archived-later", "stale news");
    share("deleted-later", "stale news");
    archive_session(&state, "archived-later").unwrap();
    assert!(state
        .store
        .kept_messages("archived-later")
        .unwrap()
        .is_empty());
    state.discard_session("deleted-later").unwrap();
    assert!(state
        .store
        .kept_messages("deleted-later")
        .unwrap()
        .is_empty());
}

/// An owner that is interrupted, with no process running.
fn interrupted_owner(id: &str) -> Session {
    Session {
        title: id.to_string(),
        agent_session_id: Some(format!("conversation-{id}")),
        has_conversation: true,
        ..project_session(id, Agent::Claude, SessionStatus::Interrupted)
    }
}

async fn report(
    state: &Arc<AppState>,
    session_id: &str,
    summary: &str,
    status: crate::messaging::reporting::ReportStatus,
) -> anyhow::Result<(String, crate::messaging::delivery::Delivery)> {
    crate::messaging::reporting::deliver_own_report(
        state,
        session_id,
        summary.to_string(),
        status,
        Vec::new(),
        Vec::new(),
    )
    .await
}

/// A report to an owner that is interrupted is kept for it, and a `done` one archives the
/// reporter as a delivered one would; the owner's resume writes it in, dated, in the order it
/// came among the information kept for it.
#[tokio::test]
async fn a_report_to_an_interrupted_owner_is_kept_for_its_resume_in_order() {
    use crate::messaging::reporting::ReportStatus;
    let (state, _dir, _live) = binding_fixture(
        "kept-report",
        vec![
            interrupted_owner("starter"),
            bound_session("worker", SessionStatus::Idle, "starter"),
            bound_session("asker", SessionStatus::Idle, "starter"),
            console_session("hub", SessionStatus::Idle),
        ],
        &["hub"],
    );
    let share = |text: &str| {
        crate::messaging::sharing::deliver_info(&state, "hub", "starter", text).unwrap();
    };

    share("first news");
    let (note, _) = report(&state, "worker", "all done", ReportStatus::Done)
        .await
        .expect("kept");
    assert!(
        note.contains("kept for the project session `starter`"),
        "{note}"
    );
    assert!(note.contains("This session is now archived"), "{note}");
    assert_eq!(
        state.session_record("worker").unwrap().status,
        SessionStatus::Archived
    );
    let (note, _) = report(&state, "asker", "which cache?", ReportStatus::NeedsDecision)
        .await
        .expect("kept");
    assert!(!note.contains("archived"), "{note}");
    assert_eq!(
        state.session_record("asker").unwrap().status,
        SessionStatus::Idle
    );
    share("second news");

    let (_, queued) = relaunch_stand_in(&state, "starter", false).await.unwrap();
    assert_eq!(queued.len(), 4, "{queued:?}");
    assert!(queued[0].contains("first news"), "{queued:?}");
    assert!(
        queued[1].starts_with("Report from session worker"),
        "{queued:?}"
    );
    assert!(queued[1].contains("all done"), "{queued:?}");
    assert!(
        queued[1].contains("Kept for you while you were interrupted; it dates from "),
        "{queued:?}"
    );
    assert!(queued[2].contains("which cache?"), "{queued:?}");
    assert!(queued[3].contains("second news"), "{queued:?}");
}

/// A report to an archived owner is refused, keeps nothing and leaves the reporter as it was,
/// and the refusal names the owner rather than reading as the reporter's own state; a reporter
/// archived with its owner is told that too.
#[tokio::test]
async fn a_report_to_an_archived_owner_is_refused_naming_the_owner() {
    use crate::messaging::reporting::ReportStatus;
    let (state, _dir, _live) = binding_fixture(
        "report-archived-owner",
        vec![
            Session {
                status: SessionStatus::Archived,
                ..interrupted_owner("starter")
            },
            bound_session("left-out", SessionStatus::Idle, "starter"),
            bound_session("archived-with-it", SessionStatus::Archived, "starter"),
        ],
        &[],
    );

    let err = report(&state, "left-out", "done", ReportStatus::Done)
        .await
        .expect_err("refused");
    assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED, "{err}");
    assert!(
        err.to_string()
            .contains("your owner, the project session `starter`, is archived"),
        "{err}"
    );
    assert!(err.to_string().contains("not delivered"), "{err}");
    assert_eq!(
        state.session_record("left-out").unwrap().status,
        SessionStatus::Idle
    );

    let err = report(&state, "archived-with-it", "done", ReportStatus::Done)
        .await
        .expect_err("refused");
    assert!(
        err.to_string().contains("your owner `starter` is archived"),
        "{err}"
    );
    assert!(state.store.kept_messages("starter").unwrap().is_empty());
}

/// A notice Octoboard writes to an owner that is interrupted is kept for its resume and
/// delivered dated; a kept notice that a session waits for the user gives way to anything
/// later about that session, a later wait included. What is kept for an owner archived before
/// its resume is dropped.
#[tokio::test]
async fn a_notice_to_an_interrupted_owner_is_kept_and_an_out_of_date_wait_dropped() {
    use crate::messaging::owner_notices::{tell_owner_it_ended, tell_owner_it_waits};
    let (state, _dir, _live) = binding_fixture(
        "kept-notice",
        vec![
            interrupted_owner("starter"),
            bound_session("worker", SessionStatus::WaitingUser, "starter"),
            bound_session("blocked", SessionStatus::WaitingUser, "starter"),
            interrupted_owner("archived-later"),
            bound_session("its-worker", SessionStatus::Interrupted, "archived-later"),
        ],
        &[],
    );

    tell_owner_it_waits(&state, "worker");
    state.turn_started("worker", Some("go on"));
    tell_owner_it_waits(&state, "worker");
    tell_owner_it_waits(&state, "blocked");
    assert_eq!(state.store.kept_messages("starter").unwrap().len(), 2);
    let worker = state.session_record("worker").unwrap();
    tell_owner_it_ended(&state, &worker);

    let (_, queued) = relaunch_stand_in(&state, "starter", false).await.unwrap();
    assert_eq!(queued.len(), 2, "{queued:?}");
    assert!(queued[0].contains("`blocked`"), "{queued:?}");
    assert!(queued[0].contains("is waiting for the user"), "{queued:?}");
    assert!(queued[1].contains("`worker`"), "{queued:?}");
    assert!(queued[1].contains("ended on its own"), "{queued:?}");
    for notice in &queued {
        assert!(
            notice.contains("Kept for you while you were interrupted"),
            "{notice}"
        );
    }

    let its_worker = state.session_record("its-worker").unwrap();
    tell_owner_it_ended(&state, &its_worker);
    assert_eq!(
        state.store.kept_messages("archived-later").unwrap().len(),
        1
    );
    archive_session(&state, "archived-later").unwrap();
    assert!(state
        .store
        .kept_messages("archived-later")
        .unwrap()
        .is_empty());
}

/// A kept notice that a session waits for the user, which nothing later superseded, is not
/// written into the owner at its resume once the session no longer waits.
#[tokio::test]
async fn a_kept_wait_that_is_over_is_not_delivered_at_the_resume() {
    use crate::messaging::owner_notices::tell_owner_it_waits;
    let (state, _dir, _live) = binding_fixture(
        "kept-wait-over",
        vec![
            interrupted_owner("starter"),
            bound_session("over", SessionStatus::WaitingUser, "starter"),
            bound_session("still", SessionStatus::WaitingUser, "starter"),
        ],
        &[],
    );
    tell_owner_it_waits(&state, "over");
    tell_owner_it_waits(&state, "still");
    assert_eq!(state.store.kept_messages("starter").unwrap().len(), 2);
    let mut over = state.session_record("over").unwrap();
    over.status = SessionStatus::Idle;
    state.store.update_session(&over).unwrap();

    let (_, queued) = relaunch_stand_in(&state, "starter", false).await.unwrap();

    assert_eq!(queued.len(), 1, "{queued:?}");
    assert!(queued[0].contains("`still`"), "{queued:?}");
    assert_eq!(state.store.kept_messages("starter").unwrap().len(), 1);
}
