//! The tool with which an owner changes the model and reasoning effort of a session of its own,
//! `set_session_model`. What the change does and refuses is the coordinator's
//! (`coordinator::set_session_model_with`); what is here is the caller's side: that the session is
//! its own and not archived, and what the result tells it, since the session is not sent a message
//! for the change (only, at times, one to carry on with its turn). It needs no approval: the
//! session stays on its account, so no other account's quota is spent.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::coordinator::{self, LaunchEdit, ModelAsker, ModelChange};
use crate::protocol::{Session, SessionStatus};
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
    let change = coordinator::set_session_model_with(
        state,
        &target.id,
        edit,
        ModelAsker::Owner(&caller.id),
        // An owner asks over MCP, which has no window appearance to match.
        None,
        snapshot,
        launch,
    )
    .await?;
    let changed = state.session_record(&target.id)?;
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
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, peer_state, shared_console_state};
    use crate::protocol::Agent;
    use crate::test_support::{running, stand_in_launch, Launched, ScratchDir, StandIn};
    use std::sync::Mutex;

    type Launches = Arc<Mutex<Vec<Launched>>>;
    type Started = Arc<Mutex<Vec<StandIn>>>;

    const CONVERSATION: &str = "projects/slug/agent-1.jsonl";

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
        std::fs::write(
            dir.join("default-claude").join(CONVERSATION),
            concat!(
                r#"{"type":"user","message":{"role":"user","content":"run it"}}"#,
                "\n",
                r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","#,
                r#""content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}"#,
                "\n"
            ),
        )
        .unwrap();
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
        std::fs::write(dir.join("default-claude").join(CONVERSATION), concat!(
            r#"{"type":"user","message":{"role":"user","content":"run it"}}"#, "\n",
            r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}"#, "\n"
        )).unwrap();
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
}
