//! The tool with which an owner moves a session of its own to another account of its agent,
//! `switch_session_account`. What the switch does and refuses is the coordinator's
//! (`coordinator::switch_session_account`); what is here is the caller's side: that the session is
//! its own, the account is read for the session's own agent, the user being asked first unless
//! they chose to have switches approved without asking (`session_request`), and what the result
//! tells it, since the session is not sent a message for the switch (only, at times, one to carry
//! on with its turn).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::coordinator::{self, LaunchEdit};
use crate::protocol::Session;
use crate::session_request::{self, Change, Edits, Perform, Performed};
use crate::state::AppState;

use super::args::required_str;
use super::exec::resolve_owned_session;
use super::settings::resolve_account;

/// Moves a session of the caller's to another account of its own agent, keeping its conversation;
/// see `coordinator::switch_session_account` for what happens to it and what is refused. The
/// session is not told of the switch: it is relaunched on the new account and comes back awaiting
/// instructions, which the result tells the caller to send — unless the relaunch was sent a prompt
/// to carry on with its turn, which the result says instead.
///
/// Unless the user has chosen to have account switches approved without asking, the call
/// waits while the user is asked, who may change the account and the session's model and
/// reasoning effort; the result then names what was done.
///
/// The shell environment the default account is read from comes from `snapshot` and the session is
/// relaunched through `launch`: `env_shell::launch_snapshot` and `term::launch` outside the tests,
/// which cannot start an agent.
pub(super) async fn switch_session_account(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let account = resolve_account(state, target.agent, required_str(arguments, "account")?)?;
    let run = {
        let (state, caller_id, target, account) = (
            state.clone(),
            caller.id.clone(),
            target.clone(),
            account.clone(),
        );
        move |edits| async move {
            perform(
                &state, &caller_id, &target.id, account, edits, snapshot, launch,
            )
            .await
        }
    };
    // Read once: the call acts on it.
    if state.store.auto_approve_account_switches()? {
        return session_request::run_unasked(
            state,
            caller,
            "switch_session_account",
            &target.id,
            run(Edits::default()),
        )
        .await;
    }
    // Where nothing the user changes in the dialog could help.
    coordinator::refuse_if_archived(&target)?;
    coordinator::refuse_if_on_account(&target, &account)?;
    coordinator::refuse_if_account_disabled(state, target.agent, account.as_deref())?;
    let change = Change::Switch {
        target: target.id.clone(),
        account,
        perform: Perform::new(run),
    };
    session_request::ask_to_change(
        state,
        caller,
        &target,
        change,
        session_request::ANSWER_TIME_LIMIT,
    )
    .await
}

/// How a model or effort a switch set is told: its value, or the agent's own default.
fn setting_text(value: &Option<String>) -> String {
    value.as_deref().map_or_else(
        || "the agent's own default".to_string(),
        |text| format!("`{text}`"),
    )
}

/// Switches the session `target_id` to `asked`, or to the account the user chose instead, with the
/// model and reasoning effort the user changed, which become the session's own. What the user
/// changed is put back when the switch fails.
async fn perform(
    state: &Arc<AppState>,
    caller_id: &str,
    target_id: &str,
    asked: Option<String>,
    edits: Edits,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static,
) -> Result<Performed> {
    // As it stands now, which is not as it stood when the user was asked.
    let target = state.session_record(target_id)?;
    let account = edits.account.clone().unwrap_or_else(|| asked.clone());
    let chose_otherwise = account != asked;
    let continued = coordinator::switch_session_account_with(
        state,
        &target.id,
        Some(account),
        Some(caller_id),
        // An owner asks over MCP, which has no window appearance to match.
        None,
        LaunchEdit {
            model: edits.model.clone(),
            effort: edits.effort.clone(),
        },
        snapshot,
        launch,
    )
    .await?;
    let moved = state.session_record(&target.id)?;
    let name = state
        .store
        .account_display_name(moved.agent, moved.account_id.as_deref())?;
    let account = moved
        .account_id
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let mut note = format!(
        "`{}` is now on the {} account `{name}`. ",
        moved.id,
        moved.agent.label(),
    );
    note.push_str(if continued {
        "Its turn had been cut off by the switch or stopped on a usage limit, so it was sent a \
         prompt to continue that task: do not send it one yourself."
    } else {
        "It is back awaiting instructions: a turn it was in the middle of is cut off, so send it \
         the next step with `send_message`."
    });
    if !(target.has_conversation && target.agent_session_id.is_some()) {
        note.push_str(" It had no conversation yet, so it starts fresh there.");
    }
    if chose_otherwise {
        note.push_str(" The user chose this account instead of the one you asked for.");
    }
    let mut result = json!({
        "session": moved.id,
        "agent": moved.agent,
        "account": account,
        "account_name": name,
    });
    if let Some(model) = &moved.model {
        result["model"] = json!(model);
    }
    if let Some(effort) = &moved.effort {
        result["effort"] = json!(effort);
    }
    if let Some(model) = &edits.model {
        note.push_str(&format!(
            " The user set its model to {}.",
            setting_text(model)
        ));
    }
    if let Some(effort) = &edits.effort {
        note.push_str(&format!(
            " The user set its reasoning effort to {}.",
            setting_text(effort)
        ));
    }
    result["note"] = json!(note);
    Ok(Performed {
        session: moved,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, peer_state, shared_console_state};
    use crate::protocol::{
        Account, Agent, Event, SessionRequestAction, SessionStatus, DEFAULT_ACCOUNT_NAME,
    };
    use crate::test_support::{
        heard_after_marker, running, stand_in_launch, Launched, ScratchDir, StandIn,
    };
    use std::path::Path;
    use std::sync::Mutex;

    type Launches = Arc<Mutex<Vec<Launched>>>;
    type Started = Arc<Mutex<Vec<StandIn>>>;
    /// What each relaunch was asked to resume and to run against.
    type Resumed = Arc<Mutex<Vec<(Option<String>, Option<std::path::PathBuf>)>>>;

    const CONVERSATION: &str = "projects/slug/agent-1.jsonl";

    /// Makes the fixture's console and project start in `dir`, which exists, since a relaunch is
    /// refused at a working directory that does not; adds a Claude and a Codex account under it;
    /// and puts a conversation of `session` in the default account, whose directory the shell
    /// snapshot names. Returns that snapshot.
    fn prepare(
        state: &Arc<AppState>,
        dir: &ScratchDir,
        session: &str,
    ) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
        let mut console = state.store.get_console("console-1").unwrap().unwrap();
        console.workdir = dir.to_string_lossy().into_owned();
        state.store.update_console(&console).unwrap();
        let mut project = state.store.get_project("project-1").unwrap().unwrap();
        project.path = dir.to_string_lossy().into_owned();
        state.store.update_project(&project).unwrap();
        for (id, agent) in [("claude-a", Agent::Claude), ("codex-a", Agent::Codex)] {
            state
                .store
                .insert_account(&Account {
                    id: id.to_string(),
                    agent,
                    name: format!("{id} name"),
                    config_dir: dir.join(id).to_string_lossy().into_owned(),
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled: false,
                })
                .unwrap();
        }
        let default = dir.join("default-claude");
        std::fs::create_dir_all(default.join(CONVERSATION).parent().unwrap()).unwrap();
        std::fs::write(default.join(CONVERSATION), "conversation").unwrap();
        let mut record = state.session_record(session).unwrap();
        record.has_conversation = true;
        record.agent_session_id = Some("agent-1".to_string());
        state.store.update_session(&record).unwrap();
        default_env(dir)
    }

    /// The shell environment `prepare` makes the default account's directory come from, for a
    /// switch that reads it again, so that no test reads the real one.
    fn default_env(
        dir: &ScratchDir,
    ) -> impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static {
        let env = HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_string(),
            dir.join("default-claude").to_string_lossy().into_owned(),
        )]);
        move || Ok(env)
    }

    /// A launch that records what it was asked to resume and run against, then starts a stand-in.
    fn recording_launch(
        started: &Started,
        launches: &Launches,
        resumed: &Resumed,
    ) -> impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static
    {
        let (inner, resumed) = (stand_in_launch(started, launches, || {}), resumed.clone());
        move |request| {
            resumed.lock().unwrap().push((
                request.resume_agent_session_id.clone(),
                request.config_dir.clone(),
            ));
            inner(request)
        }
    }

    /// A stand-in for the session's process whose end the daemon notices, as a switch needs.
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

    /// Asserts neither the caller nor the switched session was sent a message for the switch: the
    /// switched session's new process is the one started by the relaunch.
    fn assert_nobody_told(state: &Arc<AppState>, caller: &StandIn, started: &Started) {
        let mut heard = vec![heard_after_marker(state, caller)];
        for process in started.lock().unwrap().iter() {
            heard.push(heard_after_marker(state, process));
        }
        for heard in heard {
            assert!(!heard.contains("Message from Octoboard"), "{heard}");
        }
    }

    fn assert_moved_to_claude_a(state: &Arc<AppState>, dir: &Path, id: &str, resumed: &Resumed) {
        let record = state.store.get_session(id).unwrap().unwrap();
        let target = dir.join("claude-a");
        assert_eq!(record.account_id.as_deref(), Some("claude-a"));
        assert_eq!(record.config_dir.as_deref(), target.to_str());
        assert_eq!(
            std::fs::read_to_string(target.join(CONVERSATION)).unwrap(),
            "conversation",
            "the conversation was copied to the account"
        );
        assert!(
            dir.join("default-claude").join(CONVERSATION).exists(),
            "copied, not moved"
        );
        assert_eq!(
            *resumed.lock().unwrap(),
            [(Some("agent-1".to_string()), Some(target))],
            "relaunched resuming its conversation against the account"
        );
    }

    /// A console session moves a session of its own to another account by name: the conversation
    /// is copied there, the session is relaunched resuming it, the result names the account, and
    /// neither the caller nor the session is sent a message for it.
    #[tokio::test]
    async fn a_console_session_switches_a_session_of_its_own() {
        let (state, dir) = shared_console_state("switch-console");
        state.store.set_auto_approve_account_switches(true).unwrap();
        let snapshot = prepare(&state, &dir, "of-a");
        let caller = running(&state, "a");
        let original = live_and_watched(&state, "of-a");
        let (launches, started, resumed) = Default::default();

        let result = switch_session_account(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "account": "CLAUDE-A name" })),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        )
        .await
        .unwrap();

        assert_eq!(result["session"], "of-a");
        assert_eq!(result["account"], "claude-a");
        assert_eq!(result["account_name"], "claude-a name");
        assert_moved_to_claude_a(&state, &dir, "of-a", &resumed);
        assert_eq!(launches.lock().unwrap().len(), 1);
        let record = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(record.bound_to.as_deref(), Some("a"));
        // The original process's end is the exit watcher's to judge, after the switch returned.
        crate::test_support::exit_settled(&original).await;
        assert_nobody_told(&state, &caller, &started);
        // Its conversation shows no turn in progress, so it is not continued.
        assert_eq!(launches.lock().unwrap()[0].2, None);
        assert!(result["note"]
            .as_str()
            .unwrap()
            .contains("send it the next step with `send_message`"));
    }

    /// Makes the conversation `prepare` put in the default account end on a tool call that never
    /// got its result.
    fn end_on_a_tool_call(dir: &ScratchDir) {
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
    }

    /// A session whose turn the switch cut off on a tool call is relaunched with the prompt to
    /// carry on, and the result tells the caller not to send one.
    #[tokio::test]
    async fn a_switch_that_cut_a_turn_off_continues_it() {
        let (state, dir) = shared_console_state("switch-continue");
        state.store.set_auto_approve_account_switches(true).unwrap();
        let snapshot = prepare(&state, &dir, "of-a");
        end_on_a_tool_call(&dir);
        let _caller = running(&state, "a");
        let _original = live_and_watched(&state, "of-a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();

        let result = switch_session_account(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "account": "claude-a" })),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        )
        .await
        .unwrap();

        assert_eq!(
            launches.lock().unwrap()[0].2.as_deref(),
            Some(crate::switch_continue::CONTINUE_PROMPT)
        );
        let note = result["note"].as_str().unwrap();
        assert!(note.contains("do not send it one yourself"), "{note}");
        assert!(!note.contains("send it the next step"), "{note}");
    }

    /// A session whose process had already ended is not continued, however its turn ended: the
    /// switch did not cut it off.
    #[tokio::test]
    async fn a_switch_of_a_session_already_down_does_not_continue_it() {
        let (state, dir) = shared_console_state("switch-continue-down");
        state.store.set_auto_approve_account_switches(true).unwrap();
        let snapshot = prepare(&state, &dir, "of-a");
        end_on_a_tool_call(&dir);
        let mut record = state.session_record("of-a").unwrap();
        record.status = SessionStatus::Interrupted;
        state.store.update_session(&record).unwrap();
        let _caller = running(&state, "a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();

        let result = switch_session_account(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "account": "claude-a" })),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        )
        .await
        .unwrap();

        assert_eq!(launches.lock().unwrap()[0].2, None);
        assert!(result["note"]
            .as_str()
            .unwrap()
            .contains("send it the next step with `send_message`"));
    }

    /// An unbound project session moves a session of its own back to the default account, which
    /// the result calls `default`.
    #[tokio::test]
    async fn an_unbound_project_session_switches_a_session_of_its_own() {
        let (state, dir) = peer_state("switch-project");
        state.store.set_auto_approve_account_switches(true).unwrap();
        let snapshot = prepare(&state, &dir, "mine");
        let caller = running(&state, "starter");
        let original = live_and_watched(&state, "mine");
        state
            .set_session_account("mine", Some("claude-a"), dir.join("claude-a").to_str())
            .unwrap();
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();
        // The conversation sits in the account it is on.
        let (from, to) = (dir.join("claude-a"), dir.join("default-claude"));
        std::fs::create_dir_all(from.join(CONVERSATION).parent().unwrap()).unwrap();
        std::fs::write(from.join(CONVERSATION), "conversation").unwrap();
        std::fs::remove_file(to.join(CONVERSATION)).unwrap();

        let result = switch_session_account(
            &state,
            &state.session_record("starter").unwrap(),
            &arguments(json!({ "session": "mine", "account": "default" })),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        )
        .await
        .unwrap();

        assert_eq!(result["account"], "default");
        assert_eq!(result["account_name"], DEFAULT_ACCOUNT_NAME);
        let record = state.store.get_session("mine").unwrap().unwrap();
        assert_eq!((record.account_id, record.config_dir), (None, None));
        assert_eq!(
            std::fs::read_to_string(to.join(CONVERSATION)).unwrap(),
            "conversation"
        );
        assert_eq!(
            *resumed.lock().unwrap(),
            [(Some("agent-1".to_string()), None)]
        );
        // The original process's end is the exit watcher's to judge, after the switch returned.
        crate::test_support::exit_settled(&original).await;
        assert_nobody_told(&state, &caller, &started);
    }

    /// What is not the caller's to switch (another owner's session, the caller itself) and what
    /// the switch refuses (an archived session, one case of the coordinator's refusals passed
    /// through) are all refused in prose: nothing is launched, the session keeps its process, and
    /// no session changes.
    #[tokio::test]
    async fn a_switch_that_is_refused_changes_nothing() {
        let (state, dir) = shared_console_state("switch-refused");
        let _snapshot = prepare(&state, &dir, "of-a");
        let _a = running(&state, "a");
        let original = live_and_watched(&state, "of-a");
        let before = sessions_as_stored(&state);
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();
        let caller = state.session_record("a").unwrap();

        for (input, reason) in [
            (
                json!({ "session": "of-b", "account": "claude-a" }),
                "bound to another console session, `b`",
            ),
            (
                json!({ "session": "a", "account": "claude-a" }),
                "your own session",
            ),
            (
                json!({ "session": "archived-of-a", "account": "claude-a" }),
                "archived",
            ),
        ] {
            let snapshot = || -> Result<HashMap<String, String>> {
                panic!("a refused switch reads no shell environment")
            };
            let err = switch_session_account(
                &state,
                &caller,
                &arguments(input.clone()),
                snapshot,
                recording_launch(&started, &launches, &resumed),
            )
            .await
            .expect_err("refused");
            assert!(err.to_string().contains(reason), "{input}: {err}");
        }
        assert!(launches.lock().unwrap().is_empty());
        assert!(!original.poll_exit(), "the process was left running");
        assert!(!dir.join("claude-a").exists());
        assert_eq!(sessions_as_stored(&state), before);
    }

    /// The request a switch left waiting, and the caller that is held on it.
    fn spawn_switch(
        state: &Arc<AppState>,
        caller: &str,
        input: Value,
        snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
        launch: impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static,
    ) -> tokio::task::JoinHandle<Result<Value>> {
        let (state, caller) = (state.clone(), state.session_record(caller).unwrap());
        tokio::spawn(async move {
            switch_session_account(&state, &caller, &arguments(input), snapshot, launch).await
        })
    }

    fn add_second_account(state: &Arc<AppState>, dir: &ScratchDir) {
        state
            .store
            .insert_account(&Account {
                id: "claude-b".to_string(),
                agent: Agent::Claude,
                name: "claude-b name".to_string(),
                config_dir: dir.join("claude-b").to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
    }

    /// With the user not having waived the dialog, the call waits, naming the session and the
    /// account asked for, with the caller's hand up; the account the user picks instead is the one
    /// the session is moved to, the model and effort they set become the session's own, and the
    /// result names what was done.
    #[tokio::test]
    async fn a_switch_waits_for_the_user_and_moves_the_session_where_the_user_chose() {
        let (state, dir) = shared_console_state("switch-asks");
        let snapshot = prepare(&state, &dir, "of-a");
        add_second_account(&state, &dir);
        let _caller = running(&state, "a");
        let _original = live_and_watched(&state, "of-a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();
        let call = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        );

        let Event::SessionRequest {
            request_id,
            session,
            action,
            target,
            account,
            agent,
            ..
        } = crate::mcp::exec::tests::wait_for_request(&state, "a").await
        else {
            panic!("a session request");
        };
        assert_eq!(session, "a");
        assert_eq!(action, SessionRequestAction::Switch);
        assert_eq!(target.as_deref(), Some("of-a"));
        assert_eq!(account.as_deref(), Some("claude-a"));
        assert_eq!(agent, Agent::Claude);
        assert!(state.session_requests.is_waiting_for("a"));
        assert!(
            launches.lock().unwrap().is_empty(),
            "nothing moves before the answer"
        );

        let moved = session_request::answer(
            &state,
            &request_id,
            true,
            Edits {
                account: Some(Some("claude-b".to_string())),
                model: Some(None),
                effort: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        let result = call.await.unwrap().unwrap();
        assert_eq!(moved.id, "of-a");
        assert_eq!(result["account"], "claude-b");
        assert_eq!(result["account_name"], "claude-b name");
        let record = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(record.account_id.as_deref(), Some("claude-b"));
        assert_eq!((record.model, record.effort), (Some(None), Some(None)));
        assert_eq!(launches.lock().unwrap().len(), 1);
    }

    /// A refusal ends the call with an error saying the user refused and nothing changed.
    #[tokio::test]
    async fn a_refused_switch_returns_an_error_and_changes_nothing() {
        let (state, dir) = shared_console_state("switch-refused-by-user");
        let snapshot = prepare(&state, &dir, "of-a");
        let _caller = running(&state, "a");
        let original = live_and_watched(&state, "of-a");
        let before = sessions_as_stored(&state);
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();
        let call = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        );
        let Event::SessionRequest { request_id, .. } =
            crate::mcp::exec::tests::wait_for_request(&state, "a").await
        else {
            panic!("a session request");
        };

        assert!(
            session_request::answer(&state, &request_id, false, Edits::default())
                .await
                .unwrap()
                .is_none()
        );

        let err = call.await.unwrap().expect_err("refused");
        assert!(err.to_string().contains("the user refused"), "{err}");
        assert!(err.to_string().contains("nothing was changed"), "{err}");
        assert!(launches.lock().unwrap().is_empty());
        assert!(!original.poll_exit(), "the process was left running");
        assert!(!state.session_requests.is_waiting_for("a"));
        assert_eq!(sessions_as_stored(&state), before);
    }

    /// A switch the user approves with another account and a model and effort change, whose
    /// relaunch fails, leaves the session on its account with its model and effort as they were.
    #[tokio::test]
    async fn a_failed_approved_switch_puts_back_the_account_model_and_effort() {
        let (state, dir) = shared_console_state("switch-fails-after-approval");
        let snapshot = prepare(&state, &dir, "of-a");
        let _caller = running(&state, "a");
        let _original = live_and_watched(&state, "of-a");
        let call = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            snapshot,
            |_| Err(anyhow::anyhow!("the agent would not start")),
        );
        let Event::SessionRequest { request_id, .. } =
            crate::mcp::exec::tests::wait_for_request(&state, "a").await
        else {
            panic!("a session request");
        };

        session_request::answer(
            &state,
            &request_id,
            true,
            Edits {
                model: Some(None),
                effort: Some(None),
                ..Default::default()
            },
        )
        .await
        .expect_err("the relaunch fails");

        let err = call.await.unwrap().expect_err("could not be carried out");
        assert!(err.to_string().contains("the user approved"), "{err}");
        let record = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!((record.model, record.effort), (None, None));
        assert_eq!(record.account_id, None);
    }

    /// With the switches approved without asking, a call dropped while the switch runs does not
    /// stop it, and the outcome is written into the caller's session.
    #[tokio::test]
    async fn a_dropped_unasked_switch_finishes_and_tells_the_caller() {
        let (state, dir) = shared_console_state("switch-unasked-dropped");
        state.store.set_auto_approve_account_switches(true).unwrap();
        let snapshot = prepare(&state, &dir, "of-a");
        let caller = running(&state, "a");
        let original = live_and_watched(&state, "of-a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();
        let call = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        );
        // Dropped once the switch is under way: it has ended the session's process.
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while !original.poll_exit() {
            assert!(
                std::time::Instant::now() < deadline,
                "the switch never started"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        call.abort();

        // Polled without blocking the runtime the switch runs on.
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        let mut heard = String::new();
        while !heard.contains("is now on the Claude Code account") {
            assert!(
                std::time::Instant::now() < deadline,
                "the caller was never told: {heard}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            heard = String::from_utf8_lossy(&caller.recent_output(8 * 1024)).into_owned();
        }
        assert_moved_to_claude_a(&state, &dir, "of-a", &resumed);
    }

    /// An account the user turned off is refused before the user is asked.
    #[tokio::test]
    async fn a_switch_to_a_turned_off_account_is_refused_without_asking() {
        let (state, dir) = shared_console_state("switch-turned-off");
        let snapshot = prepare(&state, &dir, "of-a");
        add_second_account(&state, &dir);
        state.store.set_account_disabled("claude-b", true).unwrap();
        let _caller = running(&state, "a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();

        let err = switch_session_account(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "account": "claude-b" })),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        )
        .await
        .expect_err("refused");

        assert!(err.to_string().contains("is disabled"), "{err}");
        assert!(!state.session_requests.is_waiting_for("a"));
    }

    /// A switch to the account the session is on is refused before the user is asked.
    #[tokio::test]
    async fn a_switch_to_the_current_account_is_refused_without_asking() {
        let (state, dir) = shared_console_state("switch-current");
        let snapshot = prepare(&state, &dir, "of-a");
        let _caller = running(&state, "a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();

        let err = switch_session_account(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "account": "default" })),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        )
        .await
        .expect_err("refused");

        assert!(err.to_string().contains("on that account already"), "{err}");
        assert!(!state.session_requests.is_waiting_for("a"));
    }

    /// Switches of two different sessions of one owner wait side by side; a second call for the
    /// same session takes the first one's place and leaves the other session's alone.
    #[tokio::test]
    async fn switches_wait_per_session() {
        let (state, dir) = shared_console_state("switch-per-session");
        let snapshot = prepare(&state, &dir, "of-a");
        state
            .store
            .insert_session(&crate::mcp::exec::tests::project_session(
                "of-a-too",
                "console-1",
                Some("a"),
            ))
            .unwrap();
        let _caller = running(&state, "a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();
        let first = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        );
        crate::mcp::exec::tests::wait_for_request(&state, "a").await;
        let other = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a-too", "account": "claude-a" }),
            default_env(&dir),
            recording_launch(&started, &launches, &resumed),
        );
        let waiting = || session_request::pending_events(&state).len();
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while waiting() < 2 {
            assert!(std::time::Instant::now() < deadline, "both never waited");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let again = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            default_env(&dir),
            recording_launch(&started, &launches, &resumed),
        );

        let err = first.await.unwrap().expect_err("replaced");
        assert!(err.to_string().contains("withdrawn"), "{err}");
        assert!(!other.is_finished(), "the other session's request waits on");
        other.abort();
        again.abort();
    }

    /// A switch that runs without asking withdraws the request for the same session that still
    /// waits.
    #[tokio::test]
    async fn an_unasked_switch_withdraws_a_waiting_request_for_the_same_session() {
        let (state, dir) = shared_console_state("switch-unasked-replaces");
        let snapshot = prepare(&state, &dir, "of-a");
        let _caller = running(&state, "a");
        let _original = live_and_watched(&state, "of-a");
        let (launches, started, resumed): (Launches, Started, Resumed) = Default::default();
        let waiting = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            snapshot,
            recording_launch(&started, &launches, &resumed),
        );
        crate::mcp::exec::tests::wait_for_request(&state, "a").await;
        state.store.set_auto_approve_account_switches(true).unwrap();
        let unasked = spawn_switch(
            &state,
            "a",
            json!({ "session": "of-a", "account": "claude-a" }),
            default_env(&dir),
            |_| Err(anyhow::anyhow!("not started")),
        );

        let err = waiting.await.unwrap().expect_err("withdrawn");
        assert!(err.to_string().contains("withdrawn"), "{err}");
        assert!(!state.session_requests.is_waiting_for("a"));
        let _ = unasked.await;
    }
}
