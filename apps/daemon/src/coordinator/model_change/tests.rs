use super::*;
use crate::coordinator::handle;
use crate::coordinator::launch::stop_process;
use crate::coordinator::launch::LaunchEdit;
use crate::coordinator::lifecycle::archive_record;
use crate::coordinator::test_support::{shell_env, status_of};
use crate::protocol::{
    error_code, AgentId, Event, ModelChangeOutcome, RequestBody, Session, SessionStatus,
};
use crate::session::term;
use crate::test_support::code_of;
use crate::test_support::{
    console_session, console_with_project_and_accounts, fake_live_session, idle_stand_in,
    project_session,
};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

/// Clients are shown a session a change of its model restarts as starting for as long as the
/// change's claim lasts, and a mark without that claim shows nothing.
#[tokio::test]
async fn a_session_shows_as_starting_while_a_model_change_restarts_it() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-model-change-shown");
    state
        .store
        .insert_session(&project_session("s", AgentId::CLAUDE, SessionStatus::Idle))
        .unwrap();
    let mut events = state.subscribe();
    let shown = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
        let mut last = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionUpserted { session } = event {
                last = Some(session.status);
            }
        }
        last
    };

    state.mark_model_change("s");
    assert_eq!(shown(&mut events), None, "no claim, no mark");
    let claim = state.begin_switch("s").unwrap();
    state.mark_model_change("s");
    assert_eq!(shown(&mut events), Some(SessionStatus::Starting));
    drop(claim);
    assert!(!state.is_changing_model("s"));
    assert_eq!(shown(&mut events), Some(SessionStatus::Idle));
}

/// An owner's change of a model is refused for a session waiting for the user and for one shown
/// dormant over a view of its background agents, and goes ahead for one at its prompt; the user's
/// own change is never refused for it.
#[tokio::test]
async fn an_owner_cannot_change_the_model_of_a_session_showing_its_background_agents() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-model-change-view");
    state
        .store
        .insert_session(&project_session("s", AgentId::CLAUDE, SessionStatus::Idle))
        .unwrap();
    let live = idle_stand_in("s");
    state.register_live(live.clone());
    let session = || state.session_record("s").unwrap();
    assert!(refuse_if_owner_must_wait(&state, ModelAsker::Owner("hub"), &session()).is_ok());

    crate::test_support::show_agents_view(&live);
    assert_eq!(
        state.status_for_model_change(&session()),
        SessionStatus::Dormant
    );
    let refused =
        refuse_if_owner_must_wait(&state, ModelAsker::Owner("hub"), &session()).unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_WAITING_FOR_USER);
    assert!(
        refused.to_string().contains("background-agents view"),
        "{refused}"
    );
    assert!(refuse_if_owner_must_wait(&state, ModelAsker::User, &session()).is_ok());

    // A hand a hook raised wins over the view: the refusal is the plain wait for the user.
    let mut waiting = session();
    waiting.status = SessionStatus::WaitingUser;
    state.save_session(&waiting).unwrap();
    let refused =
        refuse_if_owner_must_wait(&state, ModelAsker::Owner("hub"), &session()).unwrap_err();
    assert_eq!(code_of(&refused), error_code::SESSION_WAITING_FOR_USER);
    assert!(
        refused.to_string().contains("waiting for the user"),
        "{refused}"
    );
    assert!(refuse_if_owner_must_wait(&state, ModelAsker::User, &session()).is_ok());
}

/// A launch that must never run, for a change that records or refuses.
fn no_launch(_: term::LaunchRequest) -> Result<term::Launch> {
    panic!("nothing is launched")
}

/// A model change asks for something to change, and is refused, with nothing written, for an
/// archived session, one being started and a value the agent does not offer; an interrupted
/// session only has the values recorded, and values it already has change nothing.
#[tokio::test]
async fn a_model_change_is_checked_and_an_interrupted_session_only_records_it() {
    let (state, dir) = console_with_project_and_accounts("coordinator-model-change-checks");
    for (id, status) in [
        ("archived", SessionStatus::Archived),
        ("starting", SessionStatus::Starting),
        ("down", SessionStatus::Interrupted),
    ] {
        state
            .store
            .insert_session(&Session {
                model: Some(Some("sonnet".to_string())),
                ..project_session(id, AgentId::CLAUDE, status)
            })
            .unwrap();
    }
    let change = |id: &'static str, model: Option<&str>| {
        set_session_model_with(
            &state,
            id,
            LaunchEdit {
                compatibility: Default::default(),
                model: model.map(|model| Some(model.to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            no_launch,
        )
    };

    let err = change("down", None).await.expect_err("nothing given");
    assert_eq!(code_of(&err), error_code::FIELD_REQUIRED);
    let err = change("archived", Some("opus"))
        .await
        .expect_err("archived");
    assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED);
    let err = change("starting", Some("opus"))
        .await
        .expect_err("starting");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    let err = change("down", Some("gpt-9"))
        .await
        .expect_err("not offered");
    assert_eq!(code_of(&err), error_code::INVALID_LAUNCH_SETTING);
    for id in ["archived", "starting", "down"] {
        let after = state.store.get_session(id).unwrap().unwrap();
        assert_eq!(after.model, Some(Some("sonnet".to_string())), "{id}");
    }

    assert_eq!(
        change("down", Some("sonnet")).await.unwrap(),
        ModelChange::Unchanged
    );
    assert_eq!(
        change("down", Some("opus")).await.unwrap(),
        ModelChange::Recorded
    );
    let after = state.store.get_session("down").unwrap().unwrap();
    assert_eq!(after.model, Some(Some("opus".to_string())));
    assert_eq!(after.status, SessionStatus::Interrupted);
}

/// The control request is answered with what the change did.
#[tokio::test]
async fn a_set_session_model_request_is_answered_with_its_outcome() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-model-change-reply");
    crate::test_support::offer_models(&state, AgentId::CLAUDE, &[("opus", &["high"])]);
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CLAUDE,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let ask = || RequestBody::SetSessionModel {
        compatibility: Default::default(),
        session: "s".to_string(),
        model: Some(Some("opus".to_string())),
        effort: None,
        terminal_appearance: None,
    };
    let outcome = |reply: Option<Event>| match reply {
        Some(Event::SessionModelSet {
            id,
            session,
            outcome,
            continued,
        }) => {
            assert_eq!((id.as_deref(), session.as_str()), (Some("r"), "s"));
            (outcome, continued)
        }
        other => panic!("answered with {other:?}"),
    };

    let reply = handle(&state, Some("r".to_string()), ask()).await.unwrap();
    assert_eq!(outcome(reply), (ModelChangeOutcome::Recorded, None));
    let reply = handle(&state, Some("r".to_string()), ask()).await.unwrap();
    assert_eq!(outcome(reply), (ModelChangeOutcome::Unchanged, None));
}

#[tokio::test]
async fn an_unchanged_model_no_longer_in_the_catalogue_is_not_revalidated() {
    let (state, dir) = console_with_project_and_accounts("model-unlisted-noop");
    crate::test_support::offer_models(&state, AgentId::CLAUDE, &[("sonnet", &["low"])]);
    state
        .store
        .insert_session(&Session {
            model: Some(Some("retired-model".to_string())),
            effort: Some(Some("retired-effort".to_string())),
            ..project_session("s", AgentId::CLAUDE, SessionStatus::Interrupted)
        })
        .unwrap();
    let changed = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some(" retired-model ".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        no_launch,
    )
    .await
    .unwrap();
    assert_eq!(changed, ModelChange::Unchanged);
    // A changed field is checked without revalidating the unrelated legacy override.
    let changed = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("sonnet".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        no_launch,
    )
    .await
    .unwrap();
    assert_eq!(changed, ModelChange::Recorded);
    let after = state.session_record("s").unwrap();
    assert_eq!(
        after.effort.as_ref().and_then(|value| value.as_deref()),
        Some("retired-effort")
    );
}

#[tokio::test]
async fn an_interrupted_session_archived_during_model_validation_is_not_changed() {
    let (state, dir) = console_with_project_and_accounts("model-late-archive");
    crate::test_support::offer_models(&state, AgentId::CLAUDE, &[("sonnet", &["low"])]);
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CLAUDE,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let change = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("sonnet".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        no_launch,
    );
    // Model validation awaits a blocking catalogue read, even for a seeded catalogue.
    let (result, ()) = tokio::join!(change, async { archive_record(&state, "s").unwrap() });
    assert_eq!(code_of(&result.unwrap_err()), error_code::SESSION_ARCHIVED);
    let after = state.session_record("s").unwrap();
    assert_eq!(after.status, SessionStatus::Archived);
    assert!(after.model.is_none());
}

/// A running session whose relaunch ends at once is left interrupted with the model and effort
/// it had, on its account, with its earlier process ended; the observed model the records
/// showed is to be read afresh.
#[tokio::test]
async fn a_model_change_whose_relaunch_does_not_come_up_keeps_the_model_it_had() {
    let (state, dir) = console_with_project_and_accounts("coordinator-model-change-down");
    let mut project = state.store.get_project("project-1").unwrap().unwrap();
    project.path = dir.to_string_lossy().into_owned();
    state.store.update_project(&project).unwrap();
    state
        .store
        .insert_session(&Session {
            effort: Some(Some("high".to_string())),
            ..project_session("s", AgentId::CLAUDE, SessionStatus::Idle)
        })
        .unwrap();
    let live = idle_stand_in("s");
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(&state, live.clone());
    let launch = |request: term::LaunchRequest| {
        let gone = fake_live_session(&request.session_id, request.agent, 80, 24, "exit 1");
        Ok(term::Launch {
            session: (*gone).clone(),
            agent_session_id: None,
            notice: None,
        })
    };

    let err = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("opus".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        launch,
    )
    .await
    .expect_err("the relaunch ended at once");

    assert_eq!(code_of(&err), error_code::MODEL_CHANGE_DID_NOT_COME_UP);
    assert!(live.poll_exit(), "its process was ended");
    let after = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Interrupted);
    assert_eq!(
        (after.model, after.effort),
        (None, Some(Some("high".to_string())))
    );
    assert_eq!((after.account_id, after.config_dir), (None, None));
    assert!(!state.is_changing_model("s"));
    assert!(state.store.observed_boundary("s").unwrap().is_some());
}

/// What the process of a session whose model is changed started is carried over to the
/// relaunch and keeps running, listed under the session.
#[tokio::test(flavor = "multi_thread")]
async fn a_model_change_carries_what_the_process_started_over_to_the_relaunch() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    let (state, dir) = console_with_project_and_accounts("coordinator-model-carry-over");
    state
        .store
        .insert_session(&project_session("s", AgentId::CLAUDE, SessionStatus::Idle))
        .unwrap();
    let (first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
    let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));

    set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("opus".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();

    let relaunched = state.live_session("s").expect("relaunched");
    assert_ne!(relaunched.launch_id, first.launch_id);
    state.process_ledger.tick_now();
    assert!(
        crate::test_support::is_running(child),
        "stopped with the process restarted"
    );
    let trees = state
        .process_ledger
        .trees_of_sessions(&["s".to_string()], Duration::ZERO);
    assert_eq!(trees.len(), 1, "{trees:?}");
    assert!(trees[0].id.starts_with(&format!("{child}:")));

    stop_process(&state, relaunched).await.unwrap();
    assert!(
        crate::test_support::stops_running(&state, child).await,
        "kept running past the relaunch"
    );
}

/// What was queued for a session when its model is changed, and what a hold for the usage limit
/// kept back, is not lost with the process the change ends: it is written into the relaunched one,
/// which no such hold holds.
#[tokio::test(flavor = "multi_thread")]
async fn a_model_change_writes_what_was_queued_into_the_relaunched_process() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    let (state, dir) = console_with_project_and_accounts("coordinator-model-queue-carried");
    state
        .store
        .insert_session(&project_session("s", AgentId::CLAUDE, SessionStatus::Idle))
        .unwrap();
    let first = crate::test_support::running(&state, "s");
    crate::coordinator::exit_watch::watch_exit(&state, first.clone());
    let judgement = first.usage_limit.begin_judging(1);
    first.usage_limit.judged(judgement, true);
    state.queue_message("s", "held-for-the-limit");
    let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
    let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));

    set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("opus".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();

    let relaunched = state.live_session("s").expect("relaunched");
    assert_ne!(relaunched.launch_id, first.launch_id);
    assert!(!relaunched.usage_limit.holds());
    let heard = crate::test_support::output_containing(&relaunched, "held-for-the-limit");
    assert!(heard.contains("held-for-the-limit"), "{heard}");
    assert!(!String::from_utf8_lossy(&first.recent_output(8 * 1024)).contains("held-for-the-limit"));
    stop_process(&state, relaunched).await.unwrap();
}

/// A model change that relaunches nothing drops what was queued as the queue of any ended process
/// goes, with nothing announced to the user.
#[tokio::test]
async fn a_model_change_that_relaunches_nothing_drops_the_queue_quietly() {
    let (state, dir) = console_with_project_and_accounts("coordinator-model-queue-dropped");
    let mut project = state.store.get_project("project-1").unwrap().unwrap();
    project.path = dir.to_string_lossy().into_owned();
    state.store.update_project(&project).unwrap();
    state
        .store
        .insert_session(&project_session("s", AgentId::CLAUDE, SessionStatus::Idle))
        .unwrap();
    let live = idle_stand_in("s");
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(&state, live.clone());
    state.queue_message("s", "never delivered");
    let mut events = state.subscribe();
    let launch = |request: term::LaunchRequest| {
        let gone = fake_live_session(&request.session_id, request.agent, 80, 24, "exit 1");
        Ok(term::Launch {
            session: (*gone).clone(),
            agent_session_id: None,
            notice: None,
        })
    };

    set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("opus".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        launch,
    )
    .await
    .expect_err("the relaunch ended at once");

    assert_eq!(state.outbox.len("s"), 0);
    while let Ok(event) = events.try_recv() {
        if let Event::SessionNotice { code, .. } = event {
            assert!(
                code != crate::protocol::notice_code::HANDOFF_MESSAGES_DROPPED
                    && code != crate::protocol::notice_code::SHARING_MESSAGES_DROPPED,
                "announced {code:?}"
            );
        }
    }
}

/// A model change that ended some of the trees the session had running tells it so: in the prompt
/// that carries a cut-off turn on, else as a notice written once it is back.
#[tokio::test(flavor = "multi_thread")]
async fn a_model_change_tells_the_session_which_trees_it_ended() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    for continued in [true, false] {
        let (state, dir) =
            console_with_project_and_accounts(&format!("coordinator-model-ended-{continued}"));
        let record = dir
            .join("default-claude")
            .join("projects/slug/a0000000-0000-4000-8000-000000000001.jsonl");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        let ending = if continued {
            concat!(
                r#"{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","#,
                r#""apiError":"usage_limit_reached","message":{"role":"assistant","#,
                r#""stop_reason":"stop_sequence","content":[{"type":"text","text":"limit"}]}}"#
            )
        } else {
            r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}"#
        };
        std::fs::write(
            &record,
            format!(
                "{}\n{ending}\n",
                r#"{"type":"user","message":{"role":"user","content":"hello"}}"#
            ),
        )
        .unwrap();
        state
            .store
            .insert_session(&Session {
                has_conversation: true,
                agent_session_id: Some("a0000000-0000-4000-8000-000000000001".to_string()),
                ..project_session("s", AgentId::CLAUDE, SessionStatus::Idle)
            })
            .unwrap();
        let (_first, survivor, doomed) =
            crate::test_support::running_leaving_two(&state, &dir, "s");
        let _guard = crate::test_support::KillOnDrop(vec![survivor, doomed]);
        let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
        let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));

        let outcome = set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                compatibility: Default::default(),
                model: Some(Some("opus".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, ModelChange::Restarted { continued: c } if c == continued));
        let prompt = launches.lock().unwrap()[0].2.clone();
        let relaunched = state.live_session("s").expect("relaunched");
        let heard = crate::test_support::heard_after_marker(&state, &relaunched);
        if continued {
            let prompt = prompt.expect("a prompt");
            assert!(
                prompt.starts_with(&format!(
                    "Message from Octoboard: your session was restarted. The restart ended these \
                     processes you had started: `{}",
                    crate::test_support::doomed_command_starts()
                )),
                "{prompt}"
            );
            assert!(prompt.ends_with(crate::messaging::texts::CONTINUE_AFTER_NOTICE));
            assert_eq!(heard.matches("The restart ended").count(), 0, "{heard}");
        } else {
            assert_eq!(prompt, None);
            assert!(
                heard.contains("The restart ended these processes"),
                "{heard}"
            );
            // The turn the notice opens owes no report, and an unrelated one still does.
            state.close_turn("s", false);
            let notice = crate::messaging::texts::restart_ended_notice(
                "The restart ended these processes you had started: `x`.",
            );
            state.turn_started("s", Some(&notice));
            assert_eq!(
                state.close_turn("s", false),
                crate::state::TurnClose::Reported
            );
            state.turn_started("s", Some("unrelated work"));
            assert_eq!(
                state.close_turn("s", false),
                crate::state::TurnClose::OwesReport
            );
        }
        assert!(crate::test_support::is_running(survivor));
        stop_process(&state, relaunched).await.unwrap();
    }
}

/// A model change that fails once it has ended the session's process leaves the session
/// interrupted, and what its process had started running and kept among its project's
/// processes.
#[tokio::test(flavor = "multi_thread")]
async fn a_model_change_that_fails_keeps_what_the_process_started_under_its_project() {
    let (state, dir) = console_with_project_and_accounts("coordinator-model-carry-failed");
    state
        .store
        .insert_session(&project_session("s", AgentId::CLAUDE, SessionStatus::Idle))
        .unwrap();
    let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
    let _guard = crate::test_support::KillOnDrop(vec![child]);

    set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("opus".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        |_| Err(anyhow::anyhow!("the agent could not be started")),
    )
    .await
    .expect_err("the relaunch failed");

    assert_eq!(status_of(&state, "s"), Some(SessionStatus::Interrupted));
    state.process_ledger.tick_now();
    assert!(
        crate::test_support::is_running(child),
        "stopped with a failed model change"
    );
    let listed = state.process_ledger.trees_of_project(&[], "project-1", &[]);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert!(listed[0].id.starts_with(&format!("{child}:")));
    assert!(listed[0].kept && listed[0].session_id.is_none());
}

/// A console session keeps nothing: what its process had started is stopped when its model
/// change fails.
#[tokio::test(flavor = "multi_thread")]
async fn a_console_session_whose_model_change_fails_stops_what_its_process_started() {
    let (state, dir) = console_with_project_and_accounts("coordinator-model-console-failed");
    state
        .store
        .insert_session(&console_session("hub", SessionStatus::Idle))
        .unwrap();
    let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "hub");
    let _guard = crate::test_support::KillOnDrop(vec![child]);

    set_session_model_with(
        &state,
        "hub",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("opus".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        |_| Err(anyhow::anyhow!("the agent could not be started")),
    )
    .await
    .expect_err("the relaunch failed");

    assert!(
        crate::test_support::stops_running(&state, child).await,
        "kept running past the failed model change"
    );
}

#[tokio::test(start_paused = true)]
async fn compatibility_only_session_changes_hold_either_boolean() {
    let (state, dir) = console_with_project_and_accounts("compatibility-session-edit");
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CODEX,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let audit = rusqlite::Connection::open(dir.join("octoboard.db")).unwrap();
    audit
        .execute_batch(
            "CREATE TABLE launch_writes (id TEXT);
         CREATE TRIGGER launch_writes AFTER UPDATE OF
             model, effort, model_override, effort_override, compatibility ON sessions
         BEGIN INSERT INTO launch_writes VALUES (NEW.id); END;",
        )
        .unwrap();
    for (prior, value) in [(None, Some(true)), (Some(true), Some(false))] {
        let held = prior
            .map(|v| ("claude_compatibility".into(), v))
            .into_iter()
            .collect();
        state.store.set_session_compatibility("s", &held).unwrap();
        audit.execute("DELETE FROM launch_writes", []).unwrap();
        let expected = ModelChange::Recorded;
        let outcome = set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                compatibility: [("claude_compatibility".into(), value)].into(),
                ..LaunchEdit::default()
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            no_launch,
        )
        .await
        .unwrap();
        assert_eq!(outcome, expected);
        let writes: i64 = audit
            .query_row("SELECT COUNT(*) FROM launch_writes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(writes, 1);
        let session = state.session_record("s").unwrap();
        assert_eq!(
            session.compatibility.get("claude_compatibility").copied(),
            value
        );
        assert!(session.model.is_none() && session.effort.is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn compatibility_only_unchanged_session_switch_does_not_restart() {
    let (state, dir) = console_with_project_and_accounts("compatibility-session-edit");
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CODEX,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    state
        .store
        .set_session_compatibility("s", &[("claude_compatibility".into(), true)].into())
        .unwrap();
    let value = Some(true);
    let expected = ModelChange::Unchanged;
    let outcome = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: [("claude_compatibility".into(), value)].into(),
            ..LaunchEdit::default()
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        no_launch,
    )
    .await
    .unwrap();
    assert_eq!(outcome, expected);
    let session = state.session_record("s").unwrap();
    assert_eq!(
        session.compatibility.get("claude_compatibility").copied(),
        value
    );
    assert!(session.model.is_none() && session.effort.is_none());
}

#[tokio::test(start_paused = true)]
async fn compatibility_only_session_switch_can_return_to_following() {
    let (state, dir) = console_with_project_and_accounts("compatibility-session-edit");
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CODEX,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    state
        .store
        .set_session_compatibility("s", &[("claude_compatibility".into(), false)].into())
        .unwrap();
    let value = None;
    let expected = ModelChange::Recorded;
    let outcome = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: [("claude_compatibility".into(), value)].into(),
            ..LaunchEdit::default()
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        no_launch,
    )
    .await
    .unwrap();
    assert_eq!(outcome, expected);
    let session = state.session_record("s").unwrap();
    assert_eq!(
        session.compatibility.get("claude_compatibility").copied(),
        value
    );
    assert!(session.model.is_none() && session.effort.is_none());
}

#[tokio::test]
async fn compatibility_invalid_session_changes_are_refused_without_partial_writes() {
    let (state, dir) = console_with_project_and_accounts("compatibility-invalid-session-edit");
    state
        .store
        .insert_session(&project_session(
            "s",
            AgentId::CODEX,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    for setting in ["unknown_switch", "approval_reviewer"] {
        let err = set_session_model_with(
            &state,
            "s",
            LaunchEdit {
                compatibility: [
                    ("claude_compatibility".into(), Some(true)),
                    (setting.into(), Some(false)),
                ]
                .into(),
                ..LaunchEdit::default()
            },
            ModelAsker::User,
            None,
            shell_env(&dir),
            no_launch,
        )
        .await
        .unwrap_err();
        assert_eq!(code_of(&err), error_code::UNREADABLE_REQUEST);
        assert!(state.session_record("s").unwrap().compatibility.is_empty());
    }
}

#[tokio::test]
async fn compatibility_only_change_relaunches_running_session_and_failure_restores_switches() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (state, dir) = console_with_project_and_accounts("compatibility-relaunch-failure");
    let mut project = state.store.require_project("project-1").unwrap();
    project.path = dir.to_string_lossy().into_owned();
    state.store.update_project(&project).unwrap();
    let mut session = project_session("s", AgentId::CODEX, SessionStatus::Idle);
    session
        .compatibility
        .insert("claude_compatibility".into(), false);
    state.store.insert_session(&session).unwrap();
    let live = crate::test_support::idle_stand_in_of("s", AgentId::CODEX);
    state.register_live(live.clone());
    crate::coordinator::exit_watch::watch_exit(&state, live.clone());
    let audit = rusqlite::Connection::open(dir.join("octoboard.db")).unwrap();
    audit
        .execute_batch(
            "CREATE TABLE launch_writes (id TEXT);
         CREATE TRIGGER launch_writes AFTER UPDATE OF
             model, effort, model_override, effort_override, compatibility ON sessions
         BEGIN INSERT INTO launch_writes VALUES (NEW.id); END;",
        )
        .unwrap();
    let called = Arc::new(AtomicBool::new(false));
    let seen = called.clone();
    let err = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: [("claude_compatibility".into(), Some(true))].into(),
            ..LaunchEdit::default()
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        move |request| {
            assert!(request.settings.switch("claude_compatibility"));
            seen.store(true, Ordering::SeqCst);
            Err(anyhow::anyhow!("fixture launch refusal"))
        },
    )
    .await
    .unwrap_err();
    tokio::time::pause();
    tokio::time::advance(crate::test_support::PATIENCE).await;
    assert!(
        called.load(Ordering::SeqCst),
        "compatibility alone must relaunch: {err:#}"
    );
    assert_eq!(
        state.session_record("s").unwrap().compatibility,
        session.compatibility
    );
    assert!(!state.is_changing_model("s"));
    let writes: i64 = audit
        .query_row("SELECT COUNT(*) FROM launch_writes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(writes, 2, "one change and one rollback");
}

/// One hook event of session `s`, applied the way the hook route applies it: queued, then
/// drained.
fn hook(state: &Arc<AppState>, payload: serde_json::Value) {
    state.hook_queues.push("s", payload.to_string().into());
    crate::hooks::apply::drain_hook_queue(state, "s");
}

/// The background subagent Claude Code (2.1.322) lists in a `Stop` or `SubagentStop`, as it
/// lists it.
fn listed_sleeper() -> serde_json::Value {
    serde_json::json!([{
        "id": "a645cb2f11b0d2884",
        "type": "subagent",
        "status": "running",
        "description": "slow sleeper",
        "agent_type": "general-purpose",
    }])
}

/// What clients were last shown of session `s`, as its status and the ids of its background
/// tasks, and whether any of what they were shown was a plain idle.
fn last_shown(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
) -> (Option<(SessionStatus, Vec<String>)>, bool) {
    let (mut last, mut plain_idle) = (None, false);
    while let Ok(event) = events.try_recv() {
        if let Event::SessionUpserted { session } = event {
            let ids: Vec<String> = session
                .background_tasks
                .iter()
                .map(|task| task.id.clone())
                .collect();
            plain_idle |= session.status == SessionStatus::Idle && ids.is_empty();
            last = Some((session.status, ids));
        }
    }
    (last, plain_idle)
}

/// A model change ends a process that left a subagent working in the background, which drops the
/// list with the process; the relaunch's continue prompt revives the subagent (Claude Code does not
/// on its own), and the turn's end that holds it shows the session idle with it running, under the
/// change's claim and once the claim is let go, through the subagent's own events and a helper
/// agent's, until the turn its hand-back opens ends with nothing left. The events are the ones
/// Claude Code 2.1.322 sends for that sequence.
#[tokio::test]
async fn a_subagent_revived_after_a_model_change_shows_as_background_work() {
    let (state, dir) = console_with_project_and_accounts("coordinator-model-revived-subagent");
    let record = dir
        .join("default-claude")
        .join("projects/slug/a0000000-0000-4000-8000-000000000001.jsonl");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    // A turn a usage limit stopped, so the relaunch carries it on with Octoboard's continue prompt.
    std::fs::write(
        &record,
        concat!(
            r#"{"type":"user","message":{"role":"user","content":"hello"}}"#,
            "\n",
            r#"{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","#,
            r#""apiError":"usage_limit_reached","message":{"role":"assistant","#,
            r#""stop_reason":"stop_sequence","content":[{"type":"text","text":"limit"}]}}"#,
            "\n",
        ),
    )
    .unwrap();
    state
        .store
        .insert_session(&Session {
            has_conversation: true,
            agent_session_id: Some("a0000000-0000-4000-8000-000000000001".to_string()),
            ..project_session("s", AgentId::CLAUDE, SessionStatus::Idle)
        })
        .unwrap();
    let first = idle_stand_in("s");
    state.register_live(first.clone());
    crate::coordinator::exit_watch::watch_exit(&state, first.clone());
    // The first process's turn ends holding the subagent.
    hook(
        &state,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "prompt_id": "t0", "prompt": "go"}),
    );
    hook(
        &state,
        serde_json::json!({"hook_event_name": "Stop", "prompt_id": "t0", "background_tasks": listed_sleeper()}),
    );
    assert_eq!(state.background_tasks_of("s").len(), 1);

    let started = Arc::new(std::sync::Mutex::new(
        Vec::<crate::test_support::StandIn>::new(),
    ));
    let launches = Arc::new(std::sync::Mutex::new(
        Vec::<crate::test_support::Launched>::new(),
    ));
    // The claim stays held after the relaunch until the new process's turn has ended.
    let settled = Arc::new(tokio::sync::Notify::new());
    crate::coordinator::restart::SWITCH_SETTLED.with(|slot| slot.replace(Some(settled.clone())));
    let mut events = state.subscribe();

    let change = set_session_model_with(
        &state,
        "s",
        LaunchEdit {
            compatibility: Default::default(),
            model: Some(Some("opus".to_string())),
            effort: None,
        },
        ModelAsker::User,
        None,
        shell_env(&dir),
        crate::test_support::stand_in_launch(&started, &launches, || {}),
    );
    let relaunched = async {
        let (watching, previous) = (state.clone(), first.clone());
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, move || {
                watching.is_launching("s")
                    && watching
                        .live_session("s")
                        .is_some_and(|live| !std::ptr::eq(live.as_ref(), previous.as_ref()))
            })
            .await,
            "the relaunched process did not come up"
        );
        // The list went with the process that ended.
        assert!(state.background_tasks_of("s").is_empty());
        assert!(state.is_prompted_on_launch("s"));
        hook(
            &state,
            serde_json::json!({"hook_event_name": "SessionStart", "source": "resume"}),
        );
        hook(
            &state,
            serde_json::json!({
                "hook_event_name": "UserPromptSubmit",
                "prompt_id": "t1",
                "prompt": crate::messaging::texts::continue_prompt(None),
            }),
        );
        let send = serde_json::json!({
            "tool_name": "SendMessage",
            "tool_input": {"to": "a645cb2f11b0d2884", "message": "continue"},
            "tool_use_id": "toolu_send",
            "prompt_id": "t1",
        });
        let event = |name: &str, base: &serde_json::Value| {
            let mut payload = base.clone();
            payload["hook_event_name"] = name.into();
            payload
        };
        hook(&state, event("PreToolUse", &send));
        // Claude Code posts the two in the same millisecond, in either order; both fall inside the
        // turn.
        hook(
            &state,
            serde_json::json!({
                "hook_event_name": "SubagentStart",
                "prompt_id": "t1",
                "agent_id": "a645cb2f11b0d2884",
                "agent_type": "general-purpose",
            }),
        );
        hook(&state, event("PostToolUse", &send));
        hook(
            &state,
            serde_json::json!({"hook_event_name": "PostToolBatch", "prompt_id": "t1"}),
        );
        hook(
            &state,
            serde_json::json!({
                "hook_event_name": "Stop",
                "prompt_id": "t1",
                "background_tasks": listed_sleeper(),
            }),
        );
        let record = state.session_record("s").unwrap();
        assert_eq!(record.status, SessionStatus::Idle);
        assert_eq!(state.background_tasks_of("s").len(), 1);
        // Shown starting while the change still has its claim.
        assert_eq!(state.wire_session(record).status, SessionStatus::Starting);
        settled.notify_one();
    };
    let (outcome, ()) = tokio::join!(change, relaunched);
    crate::coordinator::restart::SWITCH_SETTLED.with(|slot| slot.replace(None));
    assert!(matches!(
        outcome.unwrap(),
        ModelChange::Restarted { continued: true }
    ));
    assert!(!state.is_launching("s"));
    let sleeper = vec!["a645cb2f11b0d2884".to_string()];
    let (shown, _) = last_shown(&mut events);
    assert_eq!(shown, Some((SessionStatus::Idle, sleeper.clone())));

    // The subagent's own tool calls, and a helper agent's stop that lists it, leave it so.
    let call = serde_json::json!({
        "prompt_id": "t1",
        "agent_id": "a645cb2f11b0d2884",
        "agent_type": "general-purpose",
        "tool_name": "Bash",
        "tool_input": {"command": "sleep 15"},
        "tool_use_id": "toolu_sleep",
    });
    for name in ["PreToolUse", "PostToolUse", "PostToolBatch"] {
        let mut payload = call.clone();
        payload["hook_event_name"] = name.into();
        hook(&state, payload);
    }
    hook(
        &state,
        serde_json::json!({
            "hook_event_name": "SubagentStop",
            "prompt_id": "t1",
            "agent_id": "ad9f0566a1c4dd04e",
            "agent_type": "",
            "background_tasks": listed_sleeper(),
        }),
    );
    let record = state.session_record("s").unwrap();
    assert_eq!(record.status, SessionStatus::Idle);
    let shown_now = state.wire_session(record);
    assert_eq!(shown_now.status, SessionStatus::Idle);
    assert_eq!(shown_now.background_tasks.len(), 1);
    let (_, plain_idle) = last_shown(&mut events);
    assert!(!plain_idle, "shown idle with nothing running");

    // The hand-back opens a turn; the subagent's stop inside it changes nothing, and the turn's end
    // lists nothing left.
    hook(
        &state,
        serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "prompt_id": "t2",
            "prompt": "Another Claude session sent a message:\n<agent-message from=\"a645cb2f11b0d2884\">\n\
                       [Subagent hand-back] The report follows:\n  DONE\n</agent-message>",
        }),
    );
    assert_eq!(status_of(&state, "s"), Some(SessionStatus::Working));
    hook(
        &state,
        serde_json::json!({
            "hook_event_name": "SubagentStop",
            "prompt_id": "t2",
            "agent_id": "a645cb2f11b0d2884",
            "agent_type": "general-purpose",
            "background_tasks": listed_sleeper(),
        }),
    );
    assert_eq!(status_of(&state, "s"), Some(SessionStatus::Working));
    let (_, plain_idle) = last_shown(&mut events);
    assert!(!plain_idle, "shown idle before the hand-back's turn ended");
    hook(
        &state,
        serde_json::json!({"hook_event_name": "Stop", "prompt_id": "t2", "background_tasks": []}),
    );
    assert_eq!(status_of(&state, "s"), Some(SessionStatus::Idle));
    assert!(state.background_tasks_of("s").is_empty());
    let (shown, _) = last_shown(&mut events);
    assert_eq!(shown, Some((SessionStatus::Idle, Vec::new())));

    stop_process(&state, state.live_session("s").unwrap())
        .await
        .unwrap();
}
