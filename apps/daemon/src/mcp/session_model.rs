//! The tool with which an owner changes the model and reasoning effort of a session of its own,
//! `set_session_model`. What the change does and refuses is the coordinator's
//! (`coordinator::set_session_model_with`); what is here is the caller's side: that the session is
//! its own and not archived, and what the result tells it, since the session is not sent a message
//! for the change (only, at times, one to carry on with its turn). It needs no approval: the
//! session stays on its account, so no other account's quota is spent. The change runs on a task
//! of its own (`session_request::run_to_the_end`), so a call dropped mid-way, as when the caller's
//! process ends, never leaves the session half restarted.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::coordinator::{self, LaunchEdit, ModelAsker, ModelChange};
use crate::protocol::{Session, SessionStatus};
use crate::session_request;
use crate::state::AppState;

use super::args::{present_text, required_str};
use super::exec::resolve_owned_session;

/// Changes the model and reasoning effort of a session of the caller's; see
/// `coordinator::set_session_model_with`. The shell environment the default account is read from
/// comes from `snapshot` and the session is relaunched through `launch`:
/// `env_shell::launch_snapshot` and `term::launch` outside the tests, which cannot start an agent.
pub(super) async fn set_session_model(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let edit = LaunchEdit {
        model: present_text(arguments, "model")?,
        effort: present_text(arguments, "effort")?,
    };
    if edit.model.is_none() && edit.effort.is_none() {
        bail!("give `model`, `effort` or both: there is nothing to change otherwise");
    }
    if target.status == SessionStatus::Archived {
        bail!(
            "`{}` is archived; reopen it with `reopen_session`, which takes a `model` and an \
             `effort` of its own",
            target.id
        );
    }
    let run = {
        let (state, caller_id, target_id) = (state.clone(), caller.id.clone(), target.id.clone());
        async move {
            let change = coordinator::set_session_model_with(
                &state,
                &target_id,
                edit,
                ModelAsker::Owner(&caller_id),
                // An owner asks over MCP, which has no window appearance to match.
                None,
                snapshot,
                launch,
            )
            .await?;
            Ok(outcome(&state.session_record(&target_id)?, change))
        }
    };
    session_request::run_to_the_end(state, caller, "set_session_model", run).await
}

/// The call's result for `changed`, the session as `change` left it.
fn outcome(changed: &Session, change: ModelChange) -> Value {
    let note = match change {
        ModelChange::Unchanged => format!(
            "`{}` already had that model and reasoning effort as its own, so nothing was changed \
             and it was not restarted.",
            changed.id
        ),
        ModelChange::Recorded => format!(
            "`{}` is interrupted, so nothing was restarted: it runs with these the next time it is \
             resumed, which `reopen_session` does.",
            changed.id
        ),
        ModelChange::Restarted { continued } => {
            let mut note = format!(
                "`{}` was restarted with these on the same account, keeping its conversation. ",
                changed.id
            );
            note.push_str(if continued {
                "Its turn had been cut off by the restart or stopped on a usage limit, so it was \
                 sent a prompt to continue that task: do not send it one yourself."
            } else {
                "It is back awaiting instructions: a turn it was in the middle of is cut off, so \
                 send it the next step with `instruct_session`."
            });
            if !(changed.has_conversation && changed.agent_session_id.is_some()) {
                note.push_str(" It had no conversation yet, so it starts fresh.");
            }
            note
        }
    };
    let mut result = json!({
        "session": changed.id,
        "agent": changed.agent,
        "restarted": matches!(change, ModelChange::Restarted { .. }),
    });
    if let Some(model) = &changed.model {
        result["model"] = json!(model);
    }
    if let Some(effort) = &changed.effort {
        result["effort"] = json!(effort);
    }
    result["note"] = json!(note);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, peer_state, shared_console_state};
    use crate::protocol::{Agent, OrchestrationRole};
    use crate::term::{Launch, LaunchRequest};
    use crate::test_support::{
        output_containing, running, stand_in_launch, within, Launched, ScratchDir, StandIn,
        PATIENCE,
    };
    use std::sync::Mutex;

    type Launches = Arc<Mutex<Vec<Launched>>>;
    type Started = Arc<Mutex<Vec<StandIn>>>;

    const CONVERSATION: &str = "projects/slug/agent-1.jsonl";

    /// A Claude Code transcript whose last turn a restart cut off on a tool call.
    const CUT_OFF_TURN: &str = concat!(
        r#"{"type":"user","message":{"role":"user","content":"run it"}}"#,
        "\n",
        r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","#,
        r#""content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}"#,
        "\n"
    );

    /// Makes the fixture's console and project start in `dir`, which exists, since a relaunch is
    /// refused at a working directory that does not; offers Claude Code's models; and puts a
    /// conversation of `session` in the default account, whose directory the returned shell
    /// snapshot names.
    fn prepare(
        state: &Arc<AppState>,
        dir: &ScratchDir,
        session: &str,
    ) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
        let mut console = state.store.get_console("console-1").unwrap().unwrap();
        console.workdir = dir.to_string_lossy().into_owned();
        state.store.update_console(&console).unwrap();
        for project in ["project-1", "project-3"] {
            if let Some(mut project) = state.store.get_project(project).unwrap() {
                project.path = dir.to_string_lossy().into_owned();
                state.store.update_project(&project).unwrap();
            }
        }
        crate::test_support::offer_models(
            state,
            Agent::Claude,
            &[("opus", &["high", "low"]), ("sonnet", &["low"])],
        );
        let default = dir.join("default-claude");
        std::fs::create_dir_all(default.join(CONVERSATION).parent().unwrap()).unwrap();
        std::fs::write(default.join(CONVERSATION), "conversation").unwrap();
        let mut record = state.session_record(session).unwrap();
        record.has_conversation = true;
        record.agent_session_id = Some("agent-1".to_string());
        state.store.update_session(&record).unwrap();
        let env = HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_string(),
            default.to_string_lossy().into_owned(),
        )]);
        move || Ok(env)
    }

    /// A stand-in for the session's process whose end the daemon notices, as a restart needs.
    fn live_and_watched(state: &Arc<AppState>, id: &str) -> StandIn {
        let live = running(state, id);
        state.watch_exit(live.clone());
        live
    }

    /// Every session of the store as it is, for what a refusal must leave alone.
    fn sessions_as_stored(state: &Arc<AppState>) -> Value {
        let mut sessions = state.store.list_sessions().unwrap();
        sessions.sort_by(|a, b| a.id.cmp(&b.id));
        json!(sessions)
    }

    /// A console session changes the model of a running session of its own: the session's process
    /// is ended and relaunched on the same account resuming its conversation, the model becomes
    /// its own, and the result says it is back awaiting instructions.
    #[tokio::test]
    async fn a_console_session_restarts_a_session_of_its_own_with_another_model() {
        let (state, dir) = shared_console_state("model-console");
        let snapshot = prepare(&state, &dir, "of-a");
        let _caller = running(&state, "a");
        let original = live_and_watched(&state, "of-a");
        let (launches, started): (Launches, Started) = Default::default();

        let result = set_session_model(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "model": "sonnet", "effort": "" })),
            snapshot,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        assert_eq!(result["session"], "of-a");
        assert_eq!(result["restarted"], true);
        assert_eq!(result["model"], "sonnet");
        assert_eq!(result["effort"], Value::Null);
        assert!(original.poll_exit(), "its process was ended");
        assert_eq!(launches.lock().unwrap().len(), 1, "relaunched once");
        // Its conversation shows no turn in progress, so it is not continued.
        assert_eq!(launches.lock().unwrap()[0].2, None);
        let record = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(record.model, Some(Some("sonnet".to_string())));
        assert_eq!(record.effort, Some(None));
        assert_eq!((record.account_id, record.config_dir), (None, None));
        assert_eq!(record.agent_session_id.as_deref(), Some("agent-1"));
        assert!(!state.is_changing_model("of-a"));
        assert!(result["note"]
            .as_str()
            .unwrap()
            .contains("send it the next step with `instruct_session`"));
    }

    /// A session whose turn the restart cut off on a tool call is relaunched with the prompt to
    /// carry on, and the result tells the caller not to send one.
    #[tokio::test]
    async fn a_restart_that_cut_a_turn_off_continues_it() {
        let (state, dir) = peer_state("model-continue");
        let snapshot = prepare(&state, &dir, "mine");
        std::fs::write(dir.join("default-claude").join(CONVERSATION), CUT_OFF_TURN).unwrap();
        let _caller = running(&state, "starter");
        let _original = live_and_watched(&state, "mine");
        let (launches, started): (Launches, Started) = Default::default();

        let result = set_session_model(
            &state,
            &state.session_record("starter").unwrap(),
            &arguments(json!({ "session": "mine", "effort": "low" })),
            snapshot,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        assert_eq!(
            launches.lock().unwrap()[0].2.as_deref(),
            Some(crate::switch_continue::CONTINUE_PROMPT)
        );
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("do not send it one yourself"), "{note}");
    }

    #[tokio::test]
    async fn a_prompt_that_appears_during_the_shell_snapshot_is_left_for_the_user() {
        let (state, dir) = peer_state("model-late-prompt");
        let snapshot = prepare(&state, &dir, "mine");
        let _caller = running(&state, "starter");
        let original = live_and_watched(&state, "mine");
        let during = state.clone();
        let err = set_session_model(
            &state,
            &state.session_record("starter").unwrap(),
            &arguments(json!({"session":"mine", "model":"sonnet"})),
            move || {
                let mut session = during.session_record("mine")?;
                session.status = SessionStatus::WaitingUser;
                during.store.update_session(&session)?;
                snapshot()
            },
            |_| panic!("the user's prompt must not be ended"),
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.downcast_ref::<crate::protocol::CodedError>()
                .unwrap()
                .code,
            crate::protocol::error_code::SESSION_WAITING_FOR_USER
        );
        assert!(!original.poll_exit());
        let after = state.session_record("mine").unwrap();
        assert_eq!(after.status, SessionStatus::WaitingUser);
        assert!(after.model.is_none());
        assert!(!state.is_changing_model("mine"));
    }

    #[tokio::test]
    async fn a_first_conversation_learned_during_the_snapshot_is_resumed_and_continued() {
        let (state, dir) = peer_state("model-late-conversation");
        let snapshot = prepare(&state, &dir, "mine");
        let mut first = state.session_record("mine").unwrap();
        first.has_conversation = false;
        first.agent_session_id = None;
        state.store.update_session(&first).unwrap();
        std::fs::write(dir.join("default-claude").join(CONVERSATION), CUT_OFF_TURN).unwrap();
        let _caller = running(&state, "starter");
        let _original = live_and_watched(&state, "mine");
        let (launches, started): (Launches, Started) = Default::default();
        let during = state.clone();
        let result = set_session_model(
            &state,
            &state.session_record("starter").unwrap(),
            &arguments(json!({"session":"mine", "effort":"low"})),
            move || {
                let mut session = during.session_record("mine")?;
                session.agent_session_id = Some("agent-1".to_string());
                session.has_conversation = true;
                during.store.update_session(&session)?;
                snapshot()
            },
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        assert_eq!(
            launches.lock().unwrap()[0].2.as_deref(),
            Some(crate::switch_continue::CONTINUE_PROMPT)
        );
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("do not send it one yourself"), "{note}");
        assert!(!note.contains("starts fresh"), "{note}");
    }

    /// Refused, with nothing changed and nothing launched: a session that is not the caller's, an
    /// archived one, one waiting for the user, a model the agent does not offer, and a call that
    /// gives nothing to change.
    #[tokio::test]
    async fn a_model_change_is_refused_with_nothing_changed() {
        let (state, dir) = shared_console_state("model-refused");
        let _ = prepare(&state, &dir, "of-a");
        let mut waiting = state.session_record("of-a").unwrap();
        waiting.status = SessionStatus::WaitingUser;
        state.store.update_session(&waiting).unwrap();
        let _waiting = live_and_watched(&state, "of-a");
        let before = sessions_as_stored(&state);
        let (launches, started): (Launches, Started) = Default::default();
        let mut events = state.subscribe();

        for (input, says) in [
            (json!({ "session": "of-b", "model": "opus" }), "`b`"),
            (
                json!({ "session": "archived-of-a", "model": "opus" }),
                "reopen_session",
            ),
            (
                json!({ "session": "of-a", "model": "opus" }),
                "waiting for the user",
            ),
            (json!({ "session": "of-a" }), "nothing to change"),
        ] {
            let err = set_session_model(
                &state,
                &state.session_record("a").unwrap(),
                &arguments(input.clone()),
                || Ok(HashMap::new()),
                stand_in_launch(&started, &launches, || {}),
            )
            .await
            .expect_err("refused");
            assert!(err.to_string().contains(says), "{input}: {err}");
        }
        let mut idle = state.session_record("of-a").unwrap();
        idle.status = SessionStatus::Idle;
        state.store.update_session(&idle).unwrap();
        let err = set_session_model(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "model": "gpt-9" })),
            || Ok(HashMap::new()),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .expect_err("not offered");
        assert!(err.to_string().contains("gpt-9"), "{err}");
        // Refused after it was shown starting: nothing is left marked, and the last thing clients
        // were told is its own status.
        assert!(!state.is_changing_model("of-a"));
        assert_eq!(
            last_published_status(&mut events, "of-a"),
            Some(SessionStatus::Idle)
        );
        idle.status = SessionStatus::WaitingUser;
        state.store.update_session(&idle).unwrap();

        assert_eq!(sessions_as_stored(&state), before);
        assert!(launches.lock().unwrap().is_empty(), "nothing was launched");
    }

    /// An interrupted session only has the model recorded, for its next resume, and values it
    /// already has change nothing; neither is restarted.
    #[tokio::test]
    async fn an_interrupted_session_takes_the_model_at_its_next_resume() {
        let (state, dir) = shared_console_state("model-interrupted");
        let snapshot = prepare(&state, &dir, "of-a");
        let mut record = state.session_record("of-a").unwrap();
        record.status = SessionStatus::Interrupted;
        state.store.update_session(&record).unwrap();
        let (launches, started): (Launches, Started) = Default::default();
        let caller = state.session_record("a").unwrap();
        let input = arguments(json!({ "session": "of-a", "model": "opus" }));
        let call = || {
            set_session_model(
                &state,
                &caller,
                &input,
                || Ok(HashMap::new()),
                stand_in_launch(&started, &launches, || {}),
            )
        };
        drop(snapshot);

        let recorded = call().await.unwrap();
        assert_eq!(recorded["restarted"], false);
        assert!(recorded["note"].as_str().unwrap().contains("interrupted"));
        let unchanged = call().await.unwrap();
        assert_eq!(unchanged["restarted"], false);
        assert!(unchanged["note"]
            .as_str()
            .unwrap()
            .contains("nothing was changed"));
        let record = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(record.model, Some(Some("opus".to_string())));
        assert_eq!(record.status, SessionStatus::Interrupted);
        assert!(launches.lock().unwrap().is_empty(), "nothing was launched");
    }

    fn code_of(err: &anyhow::Error) -> crate::protocol::Code {
        err.downcast_ref::<crate::protocol::CodedError>()
            .unwrap_or_else(|| panic!("a coded error: {err:#}"))
            .code
    }

    /// The status of the last `session_upserted` for `id` among the events waiting in `events`.
    fn last_published_status(
        events: &mut tokio::sync::broadcast::Receiver<crate::protocol::Event>,
        id: &str,
    ) -> Option<SessionStatus> {
        let mut last = None;
        while let Ok(event) = events.try_recv() {
            if let crate::protocol::Event::SessionUpserted { session } = event {
                if session.id == id {
                    last = Some(session.status);
                }
            }
        }
        last
    }

    /// The change the window asks for: `mine` to Sonnet.
    async fn user_asks(
        state: &Arc<AppState>,
        snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
        launch: impl FnOnce(LaunchRequest) -> Result<Launch> + Send + 'static,
    ) -> Result<ModelChange> {
        coordinator::set_session_model_with(
            state,
            "mine",
            LaunchEdit {
                model: Some(Some("sonnet".to_string())),
                effort: None,
            },
            ModelAsker::User,
            None,
            snapshot,
            launch,
        )
        .await
    }

    /// The user can see what a session waits on, so a change from the window restarts one waiting
    /// for the user, ending that prompt.
    #[tokio::test]
    async fn the_window_restarts_a_session_waiting_for_the_user() {
        let (state, dir) = peer_state("model-window-waiting");
        let snapshot = prepare(&state, &dir, "mine");
        let mut waiting = state.session_record("mine").unwrap();
        waiting.status = SessionStatus::WaitingUser;
        state.store.update_session(&waiting).unwrap();
        let original = live_and_watched(&state, "mine");
        let (launches, started): (Launches, Started) = Default::default();

        let change = user_asks(
            &state,
            snapshot,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        assert_eq!(change, ModelChange::Restarted { continued: false });
        assert!(original.poll_exit(), "its process was ended");
        assert_eq!(launches.lock().unwrap().len(), 1);
        let record = state.session_record("mine").unwrap();
        assert_eq!(record.model, Some(Some("sonnet".to_string())));
    }

    /// Every client is shown the session starting as soon as the change holds its claim, before
    /// the checks that take time are done, and another change asked for meanwhile is refused.
    #[tokio::test]
    async fn a_running_session_is_shown_starting_from_the_moment_the_change_is_claimed() {
        let (state, dir) = peer_state("model-shown-starting");
        let snapshot = prepare(&state, &dir, "mine");
        let _original = live_and_watched(&state, "mine");
        let (launches, started): (Launches, Started) = Default::default();
        let mut events = state.subscribe();
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let held = released.clone();
        let snapshot = move || {
            while !held.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            snapshot()
        };
        let meanwhile = async {
            let marking = state.clone();
            let marked = within(PATIENCE, move || marking.is_changing_model("mine")).await;
            let shown = state
                .wire_session(state.session_record("mine").unwrap())
                .status;
            let mut published = None;
            while let Ok(event) = events.try_recv() {
                if let crate::protocol::Event::SessionUpserted { session } = event {
                    if session.id == "mine" {
                        published = Some(session.status);
                    }
                }
            }
            let second = coordinator::set_session_model_with(
                &state,
                "mine",
                LaunchEdit {
                    model: Some(Some("opus".to_string())),
                    effort: None,
                },
                ModelAsker::User,
                None,
                || Ok(HashMap::new()),
                |_| panic!("the second change launches nothing"),
            )
            .await;
            released.store(true, std::sync::atomic::Ordering::SeqCst);
            (marked, shown, published, second)
        };

        let (change, (marked, shown, published, second)) = tokio::join!(
            user_asks(
                &state,
                snapshot,
                stand_in_launch(&started, &launches, || {})
            ),
            meanwhile
        );

        assert!(marked, "shown starting before the shell snapshot");
        assert_eq!(change.unwrap(), ModelChange::Restarted { continued: false });
        assert_eq!(shown, SessionStatus::Starting);
        assert_eq!(published, Some(SessionStatus::Starting));
        assert_eq!(
            code_of(&second.unwrap_err()),
            crate::protocol::error_code::SESSION_ALREADY_STARTING
        );
        assert!(!state.is_changing_model("mine"));
    }

    /// A process whose end is never seen fails the change with `model_change_did_not_stop`, with
    /// nothing recorded and nothing launched.
    #[tokio::test]
    async fn a_process_that_does_not_stop_fails_the_change_with_nothing_recorded() {
        let (state, dir) = peer_state("model-did-not-stop");
        let snapshot = prepare(&state, &dir, "mine");
        // No exit watcher: nothing records the process down.
        let _original = running(&state, "mine");
        *state.model_change_stop_timeout.lock().unwrap() =
            Some(std::time::Duration::from_millis(200));

        let err = user_asks(&state, snapshot, |_| panic!("nothing is relaunched"))
            .await
            .unwrap_err();

        assert_eq!(
            code_of(&err),
            crate::protocol::error_code::MODEL_CHANGE_DID_NOT_STOP
        );
        let record = state.session_record("mine").unwrap();
        assert!(record.model.is_none(), "nothing was recorded");
        assert!(state.store.observed_boundary("mine").unwrap().is_none());
        assert!(!state.is_changing_model("mine"));
    }

    /// A change of the sharing of the session's account refuses a change of its model, with
    /// nothing changed, but not one that gives the values it has, which succeeds whatever else is
    /// going on, another holder's claim included.
    #[tokio::test]
    async fn a_change_of_the_accounts_sharing_refuses_all_but_a_change_of_nothing() {
        let (state, dir) = peer_state("model-sharing");
        let _snapshot = prepare(&state, &dir, "mine");
        let account = crate::protocol::Account {
            id: "claude-a".to_string(),
            agent: crate::protocol::Agent::Claude,
            name: "Claude A".to_string(),
            config_dir: dir.join("claude-a").to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        };
        state.store.insert_account(&account).unwrap();
        state
            .set_session_account("mine", Some(&account.id), Some(&account.config_dir))
            .unwrap();
        state
            .set_session_launch_overrides("mine", &Some(Some("opus".to_string())), &None)
            .unwrap();
        let _original = live_and_watched(&state, "mine");
        let before = sessions_as_stored(&state);
        let (_, _sharing) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
        let mut events = state.subscribe();
        let change = |model: &str| {
            coordinator::set_session_model_with(
                &state,
                "mine",
                LaunchEdit {
                    model: Some(Some(model.to_string())),
                    effort: None,
                },
                ModelAsker::Owner("starter"),
                None,
                || Ok(HashMap::new()),
                |_| panic!("nothing is launched"),
            )
        };

        let err = change("sonnet").await.unwrap_err();
        assert_eq!(
            code_of(&err),
            crate::protocol::error_code::ACCOUNT_SHARING_IN_PROGRESS
        );
        assert!(!state.is_changing_model("mine"));
        let own = state.session_record("mine").unwrap().status;
        assert_ne!(own, SessionStatus::Starting);
        assert_eq!(last_published_status(&mut events, "mine"), Some(own));
        assert_eq!(change(" opus ").await.unwrap(), ModelChange::Unchanged);
        let _claim = state.begin_switch("mine").unwrap();
        assert_eq!(change("opus").await.unwrap(), ModelChange::Unchanged);
        assert_eq!(sessions_as_stored(&state), before);
    }

    /// Once the process is down, a relaunch refused for an unreachable directory leaves the
    /// session interrupted with the model it had.
    #[tokio::test]
    async fn a_relaunch_the_launch_rules_refuse_leaves_the_model_it_had() {
        let (state, dir) = peer_state("model-relaunch-refused");
        let snapshot = prepare(&state, &dir, "mine");
        let original = live_and_watched(&state, "mine");
        let during = state.clone();
        let gone = dir.join("gone").to_string_lossy().into_owned();
        let snapshot = move || {
            // Its project's folder is no longer there by the time it is relaunched.
            let mut project = during.store.get_project("project-1")?.unwrap();
            project.path = gone;
            during.store.update_project(&project)?;
            snapshot()
        };

        let err = user_asks(&state, snapshot, |request| {
            assert!(!request.cwd.is_dir());
            Err(crate::protocol::CodedError::raised(
                crate::protocol::error_code::DIRECTORY_UNREACHABLE,
                "the launch directory is unreachable",
                &[("path", &request.cwd.to_string_lossy())],
            ))
        })
        .await
        .unwrap_err();

        assert_eq!(
            code_of(&err),
            crate::protocol::error_code::DIRECTORY_UNREACHABLE
        );
        assert!(original.poll_exit(), "its process was ended");
        let record = state.session_record("mine").unwrap();
        assert_eq!(record.status, SessionStatus::Interrupted, "{err:#}");
        assert!(record.model.is_none(), "the model it had: {err:#}");
        assert!(!state.is_changing_model("mine"));
    }

    /// An archive that lands once the restart is under way, before the process is ended or once
    /// it is down, is the user's later word: the change is refused, nothing is launched, and the
    /// session stays archived with the model it had.
    #[tokio::test]
    async fn an_archive_during_the_restart_wins() {
        for after_the_stop in [false, true] {
            let (state, dir) = peer_state(&format!("model-archived-{after_the_stop}"));
            let snapshot = prepare(&state, &dir, "mine");
            let original = running(&state, "mine");
            let archive = {
                let state = state.clone();
                move || {
                    let mut record = state.session_record("mine").unwrap();
                    record.status = SessionStatus::Archived;
                    state.store.update_session(&record).unwrap();
                }
            };
            let result = if after_the_stop {
                // The process is ended, then archived before its end is recorded.
                let watcher = {
                    let (state, live) = (state.clone(), (*original).clone());
                    async move {
                        let ended = live.clone();
                        if !within(PATIENCE, move || ended.poll_exit()).await {
                            return false;
                        }
                        archive();
                        state.watch_exit(live);
                        true
                    }
                };
                let (result, ended) = tokio::join!(
                    user_asks(&state, snapshot, |_| panic!("nothing is launched")),
                    watcher
                );
                assert!(ended, "the process was ended before the archive");
                result
            } else {
                state.watch_exit((*original).clone());
                let snapshot = move || {
                    archive();
                    snapshot()
                };
                user_asks(&state, snapshot, |_| panic!("nothing is launched")).await
            };

            let err = result.unwrap_err();
            assert_eq!(
                code_of(&err),
                crate::protocol::error_code::SESSION_ARCHIVED,
                "after the stop: {after_the_stop}"
            );
            let record = state.session_record("mine").unwrap();
            assert_eq!(record.status, SessionStatus::Archived);
            assert!(record.model.is_none());
            assert_eq!(original.poll_exit(), after_the_stop);
            assert!(!state.is_changing_model("mine"));
        }
    }

    /// The operation checks for itself, under its claim, that the session is still the asking
    /// owner's, whatever its caller checked before.
    #[tokio::test]
    async fn a_session_handed_to_another_owner_is_refused_under_the_claim() {
        let (state, dir) = peer_state("model-owner-moved");
        let snapshot = prepare(&state, &dir, "mine");
        let original = live_and_watched(&state, "mine");
        assert!(state
            .store
            .set_binding(
                "mine",
                Some("starter"),
                "a",
                OrchestrationRole::Worker,
                false,
                false
            )
            .unwrap()
            .is_some());

        let err = coordinator::set_session_model_with(
            &state,
            "mine",
            LaunchEdit {
                model: Some(Some("sonnet".to_string())),
                effort: None,
            },
            ModelAsker::Owner("starter"),
            None,
            snapshot,
            |_| panic!("nothing is launched"),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("no longer yours"), "{err:#}");
        assert!(!original.poll_exit());
        assert!(state.session_record("mine").unwrap().model.is_none());
    }

    /// A Grok Build session whose only wait is its usage-limit dialog is restarted at its owner's
    /// asking, the dialog ending with the process, and the turn the limit stopped is continued.
    #[tokio::test]
    async fn an_owner_restarts_a_session_held_only_by_a_usage_limit_dialog_and_it_continues() {
        const GROK_MODAL: &str = "You hit your weekly limit.\n1 (o) Upgrade tier\n2 (o) Buy more \
                                  credits\n3 (o) Try Again\nEnter :submit / Shift+x :dismiss\n";
        let (state, dir) = peer_state("model-usage-limit");
        let _ = prepare(&state, &dir, "mine");
        crate::test_support::offer_models(
            &state,
            Agent::Grok,
            &[("grok-4.7", &["high"]), ("grok-4.8", &["high"])],
        );
        let mut grok = state.session_record("mine").unwrap();
        grok.agent = Agent::Grok;
        grok.agent_session_id = Some("grok-1".to_string());
        state.store.update_session(&grok).unwrap();
        // Its turn stopped on the limit, as Grok 1.0.70 writes it.
        let record = dir.join("grok-default/sessions/proj/grok-1");
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
        let home = dir.join("grok-default").to_string_lossy().into_owned();
        let live = crate::test_support::fake_live_session("mine", Agent::Grok, 80, 24, "sleep 30");
        state.register_live(live.clone());
        state.watch_exit((*live).clone());
        live.on_output(GROK_MODAL.as_bytes());
        assert!(live
            .look_for_dialog()
            .current
            .is_some_and(|dialog| dialog.is_usage_limit()));
        assert_eq!(
            state.shown_status(&state.session_record("mine").unwrap()),
            SessionStatus::WaitingUser,
            "shown waiting for the user"
        );
        let _caller = running(&state, "starter");
        let (launches, started): (Launches, Started) = Default::default();

        let result = set_session_model(
            &state,
            &state.session_record("starter").unwrap(),
            &arguments(json!({ "session": "mine", "model": "grok-4.8" })),
            move || Ok(HashMap::from([("GROK_HOME".to_string(), home)])),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        assert_eq!(result["restarted"], true);
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("do not send it one yourself"), "{note}");
        assert!(live.poll_exit(), "the dialog ended with its process");
        assert_eq!(launches.lock().unwrap().len(), 1);
    }

    /// A call dropped while the change runs, as when the caller's process ends, does not stop it
    /// half way: the change runs to its end and its outcome is written into the caller's session.
    #[tokio::test]
    async fn a_dropped_call_leaves_the_change_to_finish_and_tells_the_caller() {
        let (state, dir) = peer_state("model-dropped-call");
        let snapshot = prepare(&state, &dir, "mine");
        let caller = running(&state, "starter");
        let original = live_and_watched(&state, "mine");
        let (launches, started): (Launches, Started) = Default::default();
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let held = released.clone();
        let snapshot = move || {
            while !held.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            snapshot()
        };
        let call = {
            let (state, caller) = (state.clone(), state.session_record("starter").unwrap());
            let launch = stand_in_launch(&started, &launches, || {});
            tokio::spawn(async move {
                let input = arguments(json!({ "session": "mine", "model": "sonnet" }));
                set_session_model(&state, &caller, &input, snapshot, launch).await
            })
        };
        let marking = state.clone();
        let marked = within(PATIENCE, move || marking.is_changing_model("mine")).await;

        call.abort();
        let dropped = call.await;
        // Before any assertion: a snapshot left waiting would hold the runtime's shutdown.
        released.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(marked, "the change was under way");
        assert!(dropped.unwrap_err().is_cancelled());

        // Read off the runtime, which the change and the message to the caller still run on.
        let heard = (*caller).clone();
        let told = tokio::task::spawn_blocking(move || {
            output_containing(&heard, "was restarted with these")
        })
        .await
        .unwrap();
        assert!(told.contains("was restarted with these"), "{told}");
        assert!(original.poll_exit());
        assert_eq!(launches.lock().unwrap().len(), 1);
        let record = state.session_record("mine").unwrap();
        assert_eq!(record.model, Some(Some("sonnet".to_string())));
        assert!(!state.is_changing_model("mine"));
    }

    /// A call dropped while the change runs, whose change then fails, has the failure written into
    /// the caller's session, and the session is left as it was.
    #[tokio::test]
    async fn a_dropped_call_whose_change_fails_tells_the_caller_the_failure() {
        let (state, dir) = peer_state("model-dropped-failure");
        let snapshot = prepare(&state, &dir, "mine");
        let caller = running(&state, "starter");
        // No exit watcher: nothing records the process down, so the change fails to stop it.
        let _original = running(&state, "mine");
        *state.model_change_stop_timeout.lock().unwrap() =
            Some(std::time::Duration::from_millis(200));
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let held = released.clone();
        let snapshot = move || {
            while !held.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            snapshot()
        };
        let call = {
            let (state, caller) = (state.clone(), state.session_record("starter").unwrap());
            tokio::spawn(async move {
                let input = arguments(json!({ "session": "mine", "model": "sonnet" }));
                set_session_model(&state, &caller, &input, snapshot, |_| {
                    panic!("nothing is relaunched")
                })
                .await
            })
        };
        let marking = state.clone();
        let marked = within(PATIENCE, move || marking.is_changing_model("mine")).await;

        call.abort();
        let dropped = call.await;
        released.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(marked, "the change was under way");
        assert!(dropped.unwrap_err().is_cancelled());

        let heard = (*caller).clone();
        let told = tokio::task::spawn_blocking(move || {
            output_containing(&heard, "could not be carried out")
        })
        .await
        .unwrap();
        assert!(told.contains("did not end in time"), "{told}");
        assert!(state.session_record("mine").unwrap().model.is_none());
        assert!(!state.is_changing_model("mine"));
    }

    /// A change refused after it was shown starting leaves a quota recovery waiting to resume the
    /// session alone; one that goes through cancels it, the restart cutting it off.
    #[tokio::test]
    async fn only_a_change_that_goes_through_cancels_a_pending_quota_recovery() {
        let (state, dir) = peer_state("model-quota-recovery");
        let snapshot = prepare(&state, &dir, "mine");
        let _original = live_and_watched(&state, "mine");
        let (launches, started): (Launches, Started) = Default::default();
        state.quota_recovery.wait_for_test(&state, "mine");
        assert!(state.quota_recovery.is_waiting_for_test("mine"));

        let ask = |model: &str| {
            coordinator::set_session_model_with(
                &state,
                "mine",
                LaunchEdit {
                    model: Some(Some(model.to_string())),
                    effort: None,
                },
                ModelAsker::User,
                None,
                || Ok(HashMap::new()),
                |_| panic!("nothing is launched"),
            )
        };
        ask("gpt-9").await.unwrap_err();
        assert!(
            state.quota_recovery.is_waiting_for_test("mine"),
            "refused, so the recovery stands"
        );

        user_asks(
            &state,
            snapshot,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        assert!(!state.quota_recovery.is_waiting_for_test("mine"));
    }

    /// Values the session has are not answered as unchanged while a change of its model is under
    /// way, which may still put the old ones back: the request is refused like any claim on it, and
    /// answered once the change is over.
    #[tokio::test]
    async fn no_change_is_not_answered_while_another_change_of_the_model_is_under_way() {
        let (state, dir) = peer_state("model-unchanged-while-changing");
        let snapshot = prepare(&state, &dir, "mine");
        state
            .set_session_launch_overrides("mine", &Some(Some("opus".to_string())), &None)
            .unwrap();
        let _original = live_and_watched(&state, "mine");
        let (launches, started): (Launches, Started) = Default::default();
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let held = released.clone();
        let snapshot = move || {
            while !held.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            snapshot()
        };
        let ask = |model: &str| {
            coordinator::set_session_model_with(
                &state,
                "mine",
                LaunchEdit {
                    model: Some(Some(model.to_string())),
                    effort: None,
                },
                ModelAsker::User,
                None,
                || Ok(HashMap::new()),
                |_| panic!("nothing is launched"),
            )
        };
        let meanwhile = async {
            let marking = state.clone();
            let marked = within(PATIENCE, move || marking.is_changing_model("mine")).await;
            let during = ask("opus").await;
            released.store(true, std::sync::atomic::Ordering::SeqCst);
            (marked, during)
        };

        let (change, (marked, during)) = tokio::join!(
            user_asks(
                &state,
                snapshot,
                stand_in_launch(&started, &launches, || {})
            ),
            meanwhile
        );

        assert!(marked, "the change was under way");
        assert_eq!(change.unwrap(), ModelChange::Restarted { continued: false });
        assert_eq!(
            code_of(&during.unwrap_err()),
            crate::protocol::error_code::SESSION_ALREADY_STARTING
        );
        // Over, the session runs Sonnet, and asking for what it has is answered at once.
        assert_eq!(ask("sonnet").await.unwrap(), ModelChange::Unchanged);
    }
}
