//! The tool with which an owner hands a session of its own off to a new session of another agent,
//! `handoff_session`. What moves and what is refused is the operation's
//! (`crate::handoff::operation`); what is here is the caller's side: that the session is its own,
//! and what the result tells it, since it is not sent the message that tells an owner of a handoff.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::handoff::operation;
use crate::protocol::Session;
use crate::state::AppState;

use super::args::{optional_string, parse_agent, present_text, required_str};
use super::exec::{callers_account, resolve_owned_session};
use super::settings::resolve_account;

/// Hands a session of the caller's off to a new session of another agent, which takes its place and
/// stays the caller's; see `handoff::operation` for what moves and what is refused. The caller is
/// not sent the message that tells an owner of a handoff, since the result tells it.
///
/// The new session is launched through `launch`: `term::launch` outside the tests, which cannot
/// start an agent.
pub(super) async fn handoff_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    launch: impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let agent = parse_agent(required_str(arguments, "agent")?)?;
    let account = match optional_string(arguments, "account") {
        Some(wanted) => Some(resolve_account(state, agent, &wanted)?),
        None => callers_account(state, caller, agent)?,
    };
    let moved = operation::hand_off_with(
        state,
        operation::Handoff {
            session: target.id.clone(),
            agent,
            account,
            model: present_text(arguments, "model")?,
            effort: present_text(arguments, "effort")?,
            title: optional_string(arguments, "title"),
            note: optional_string(arguments, "note"),
            asked_by: Some(caller.id.clone()),
        },
        launch,
    )
    .await?;
    let mut answer = json!({
        "session": moved.receiving.id,
        "agent": moved.receiving.agent,
        "archived": target.id,
        "owned_sessions": moved.owned.iter().map(|session| &session.id).collect::<Vec<_>>(),
    });
    let mut note = format!(
        "Handed off: `{}` was archived and the work moved to the new session `{}`, a session of \
         {} that reports to you like the original did. Drive it with the usual tools.",
        target.id,
        moved.receiving.id,
        moved.receiving.agent.label(),
    );
    match moved.branches {
        Ok(branches) => answer["branches"] = json!(branches),
        Err(reason) => {
            answer["branches"] = json!([]);
            note.push_str(&format!(
                " The branches the original was responsible for could not be moved ({reason}); \
                 they are still named after it."
            ));
        }
    }
    answer["note"] = json!(note);
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, peer_state, project_session, shared_console_state};
    use crate::protocol::{Agent, SessionStatus};
    use crate::test_support::{heard_after_marker, running, stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;

    type Launches = Arc<Mutex<Vec<Launched>>>;
    type Started = Arc<Mutex<Vec<StandIn>>>;

    /// Every session of the store with its status and owner, for what a refusal must leave alone.
    fn sessions_as_stored(state: &Arc<AppState>) -> Vec<(String, SessionStatus, Option<String>)> {
        let mut sessions: Vec<_> = state
            .store
            .list_sessions()
            .unwrap()
            .into_iter()
            .map(|session| (session.id, session.status, session.bound_to))
            .collect();
        sessions.sort_by(|a, b| a.0.cmp(&b.0));
        sessions
    }

    /// Asserts the caller was sent no message for the handoff.
    fn assert_caller_not_told(state: &Arc<AppState>, caller: &StandIn) {
        let heard = heard_after_marker(state, caller);
        assert!(!heard.contains("handed off"), "{heard}");
    }

    /// A console session hands off a session of its own: the new session, of the agent asked for,
    /// is the console session's like the original was, with the note in its opening prompt; the
    /// original is archived; the caller is told what moved by the result, and is sent no message
    /// of its own for it.
    #[tokio::test]
    async fn a_console_session_hands_off_a_session_of_its_own() {
        let (state, _dir) = shared_console_state("handoff-console");
        let caller = running(&state, "a");
        let _original = running(&state, "of-a");
        let (launches, started) = (Launches::default(), Started::default());

        let result = handoff_session(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({
                "session": "of-a",
                "agent": "codex",
                "note": "Start with the failing test.",
            })),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        let receiving = result["session"].as_str().unwrap();
        assert_eq!(result["agent"], "codex");
        assert_eq!(result["archived"], "of-a");
        assert_eq!(result["owned_sessions"], json!([]));
        let record = state.store.get_session(receiving).unwrap().unwrap();
        assert_eq!(record.agent, Agent::Codex);
        assert_eq!(record.bound_to.as_deref(), Some("a"));
        assert_eq!(record.project_id.as_deref(), Some("project-1"));
        assert_eq!(
            state.store.get_session("of-a").unwrap().unwrap().status,
            SessionStatus::Archived
        );
        let launched = launches.lock().unwrap().clone();
        assert_eq!(launched.len(), 1);
        let opening = launched[0].2.as_deref().unwrap();
        assert!(
            opening.contains("> Start with the failing test."),
            "{opening}"
        );
        assert_caller_not_told(&state, &caller);
    }

    /// An unbound project session hands off a session of its own, and what that session owned is
    /// the new session's from then on.
    #[tokio::test]
    async fn an_unbound_project_session_hands_off_a_session_of_its_own() {
        let (state, _dir) = peer_state("handoff-project");
        let caller = running(&state, "starter");
        let _mine = running(&state, "mine");
        let mut owned = project_session("owned-by-mine", "console-1", Some("mine"));
        owned.status = SessionStatus::Archived;
        state.store.insert_session(&owned).unwrap();
        let (launches, started) = (Launches::default(), Started::default());

        let result = handoff_session(
            &state,
            &state.session_record("starter").unwrap(),
            &arguments(json!({ "session": "mine", "agent": "grok" })),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        let receiving = result["session"].as_str().unwrap();
        assert_eq!(result["owned_sessions"], json!(["owned-by-mine"]));
        assert!(result["note"].as_str().unwrap().contains("Handed off"));
        let record = state.store.get_session(receiving).unwrap().unwrap();
        assert_eq!(record.agent, Agent::Grok);
        assert_eq!(record.bound_to.as_deref(), Some("starter"));
        assert_eq!(
            state
                .store
                .get_session("owned-by-mine")
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref(),
            Some(receiving)
        );
        assert_caller_not_told(&state, &caller);
    }

    /// The new session takes the title the caller gives it, and, when it runs on the caller's own
    /// agent and no account is named, the caller's own account.
    #[tokio::test]
    async fn a_handoff_to_the_callers_agent_takes_the_callers_account_and_the_title_given() {
        let (state, _dir) = shared_console_state("handoff-callers-account");
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: "codex-work".to_string(),
                agent: Agent::Codex,
                name: "Work".to_string(),
                config_dir: "/tmp/codex-work".to_string(),
                approval_reviewer: None,
                approval_mode: None,
                share_default_config: false,
                share_default_config_pending: false,
            })
            .unwrap();
        state
            .save_session(&Session {
                agent: Agent::Codex,
                ..state.session_record("a").unwrap()
            })
            .unwrap();
        state
            .store
            .set_session_account("a", Some("codex-work"), Some("/tmp/codex-work"))
            .unwrap();
        let _caller = running(&state, "a");
        let _original = running(&state, "of-a");
        let (launches, started) = (Launches::default(), Started::default());

        let result = handoff_session(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "agent": "codex", "title": "修复构建" })),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        let record = state
            .store
            .get_session(result["session"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(record.agent, Agent::Codex);
        assert_eq!(record.account_id.as_deref(), Some("codex-work"));
        assert_eq!(record.title, "修复构建");
    }

    /// Automatic account selection does not move a handoff off the caller's account, even when
    /// another account of the same agent has a weekly window that resets sooner.
    #[tokio::test]
    async fn a_handoff_keeps_the_callers_account_when_automatic_selection_is_on() {
        let (state, _dir) = shared_console_state("handoff-ignores-auto-select");
        state.store.set_auto_select_account(true).unwrap();
        for (id, dir) in [
            ("codex-work", "/tmp/codex-work"),
            ("sooner", "/tmp/codex-sooner"),
        ] {
            state
                .store
                .insert_account(&crate::protocol::Account {
                    id: id.to_string(),
                    agent: Agent::Codex,
                    name: id.to_string(),
                    config_dir: dir.to_string(),
                    approval_reviewer: None,
                    approval_mode: None,
                    share_default_config: false,
                    share_default_config_pending: false,
                })
                .unwrap();
        }
        let soon = crate::protocol::now_millis() + 60_000;
        let weekly = |used: f64, resets_at: i64| crate::protocol::QuotaWindow {
            kind: crate::protocol::QuotaWindowKind::Weekly,
            label: None,
            used_percent: used,
            resets_at: Some(resets_at),
            window_minutes: None,
        };
        let reading =
            |id: &str, window: crate::protocol::QuotaWindow| crate::protocol::AccountQuota {
                agent: Agent::Codex,
                account: Some(id.to_string()),
                fetched_at: 1,
                plan: None,
                windows: vec![window],
                unavailable: None,
            };
        state
            .account_quotas
            .hold_in_dir_for_test(&reading("sooner", weekly(10.0, soon)), "/tmp/codex-sooner");
        state.account_quotas.hold_in_dir_for_test(
            &reading("codex-work", weekly(80.0, soon + 3_600_000)),
            "/tmp/codex-work",
        );
        state
            .save_session(&Session {
                agent: Agent::Codex,
                ..state.session_record("a").unwrap()
            })
            .unwrap();
        state
            .store
            .set_session_account("a", Some("codex-work"), Some("/tmp/codex-work"))
            .unwrap();
        let _caller = running(&state, "a");
        let _original = running(&state, "of-a");
        let (launches, started) = (Launches::default(), Started::default());

        let result = handoff_session(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "agent": "codex" })),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        let record = state
            .store
            .get_session(result["session"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(record.account_id.as_deref(), Some("codex-work"));
    }

    /// What is not the caller's to hand off (another owner's session, the caller itself), what the
    /// operation refuses (a session in the middle of a turn), a bad agent and an account of
    /// another agent are all refused in prose: nothing is launched and no session changes.
    #[tokio::test]
    async fn a_handoff_that_is_refused_changes_nothing() {
        let (state, _dir) = shared_console_state("handoff-refused");
        let mut working = project_session("working-of-a", "console-1", Some("a"));
        working.status = SessionStatus::Working;
        state.store.insert_session(&working).unwrap();
        let _a = running(&state, "a");
        let _of_a = running(&state, "of-a");
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: "claude-account".to_string(),
                agent: Agent::Claude,
                name: "Work".to_string(),
                config_dir: "/tmp/claude-account".to_string(),
                approval_reviewer: None,
                approval_mode: None,
                share_default_config: false,
                share_default_config_pending: false,
            })
            .unwrap();
        let before = sessions_as_stored(&state);
        let (launches, started) = (Launches::default(), Started::default());
        let caller = state.session_record("a").unwrap();

        for (input, reason) in [
            (
                json!({ "session": "of-b", "agent": "codex" }),
                "bound to another console session, `b`",
            ),
            (
                json!({ "session": "a", "agent": "codex" }),
                "your own session",
            ),
            (
                json!({ "session": "working-of-a", "agent": "codex" }),
                "in the middle of a turn",
            ),
            (
                json!({ "session": "of-a", "agent": "cursor" }),
                "is not one of",
            ),
            (
                json!({ "session": "of-a", "agent": "codex", "account": "claude-account" }),
                "claude-account",
            ),
        ] {
            let err = handoff_session(
                &state,
                &caller,
                &arguments(input.clone()),
                stand_in_launch(&started, &launches, || {}),
            )
            .await
            .expect_err("refused");
            assert!(err.to_string().contains(reason), "{input}: {err}");
        }
        assert!(launches.lock().unwrap().is_empty());
        assert_eq!(sessions_as_stored(&state), before);
    }
}
