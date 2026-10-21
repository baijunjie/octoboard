//! The tool with which an owner moves a session of its own to another account of its agent,
//! `switch_session_account`. What the switch does and refuses is the coordinator's
//! (`coordinator::switch_session_account`); what is here is the caller's side: that the session is
//! its own, the account is read for the session's own agent, and what the result tells it, since
//! the session is not sent a message for the switch.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::coordinator;
use crate::protocol::Session;
use crate::state::AppState;

use super::args::required_str;
use super::exec::resolve_owned_session;
use super::settings::resolve_account;

/// Moves a session of the caller's to another account of its own agent, keeping its conversation;
/// see `coordinator::switch_session_account` for what happens to it and what is refused. The
/// session is not told: it is relaunched on the new account and comes back awaiting instructions,
/// which the result tells the caller to send.
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
    coordinator::switch_session_account_with(
        state,
        &target.id,
        Some(account),
        Some(&caller.id),
        // An owner asks over MCP, which has no window appearance to match.
        None,
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
        "`{}` is now on the {} account `{name}`. It is back awaiting instructions: a turn it was \
         in the middle of is cut off, so send it the next step with `send_message`.",
        moved.id,
        moved.agent.label(),
    );
    if !(target.has_conversation && target.agent_session_id.is_some()) {
        note.push_str(" It had no conversation yet, so it starts fresh there.");
    }
    Ok(json!({
        "session": moved.id,
        "agent": moved.agent,
        "account": account,
        "account_name": name,
        "note": note,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, peer_state, shared_console_state};
    use crate::protocol::{Account, Agent, DEFAULT_ACCOUNT_NAME};
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
                    approval_reviewer: None,
                    approval_mode: None,
                    share_default_config: false,
                    share_default_config_pending: false,
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
        let env = HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_string(),
            default.to_string_lossy().into_owned(),
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
        let snapshot = prepare(&state, &dir, "of-a");
        let caller = running(&state, "a");
        let _original = live_and_watched(&state, "of-a");
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
        assert_nobody_told(&state, &caller, &started);
    }

    /// An unbound project session moves a session of its own back to the default account, which
    /// the result calls `default`.
    #[tokio::test]
    async fn an_unbound_project_session_switches_a_session_of_its_own() {
        let (state, dir) = peer_state("switch-project");
        let snapshot = prepare(&state, &dir, "mine");
        let caller = running(&state, "starter");
        let _original = live_and_watched(&state, "mine");
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
        assert_nobody_told(&state, &caller, &started);
    }

    /// What is not the caller's to switch (another owner's session, the caller itself), an account
    /// of another agent, and what the switch refuses (the account the session is on, an archived
    /// session) are all refused in prose: nothing is launched, the session keeps its process, and
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
                json!({ "session": "of-a", "account": "codex-a" }),
                "codex-a",
            ),
            (
                json!({ "session": "of-a", "account": "default" }),
                "on that account already",
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
}
