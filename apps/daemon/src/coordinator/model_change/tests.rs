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
