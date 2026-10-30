//! The tool with which an owner hands a session of its own off to a new session of another agent,
//! `handoff_session`. What moves and what is refused is the operation's
//! (`crate::handoff::operation`); what is here is the caller's side: that the session is its own,
//! the user being asked first unless they chose to have handoffs approved without asking
//! (`session_request`), and what the result tells it, since it is not sent the message that tells
//! an owner of a handoff.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::coordinator;
use crate::handoff::operation;
use crate::protocol::{Agent, Session};
use crate::session_request::{self, Change, Edits, Perform, Performed};
use crate::state::AppState;

use super::args::{optional_string, parse_agent, present_text, required_str};
use super::exec::{account_when_unset, resolve_owned_session};
use super::settings::resolve_account;

/// Hands a session of the caller's off to a new session of another agent, which takes its place and
/// stays the caller's; see `handoff::operation` for what moves and what is refused. The caller is
/// not sent the message that tells an owner of a handoff, since the result tells it.
///
/// Unless the user has chosen to have handoffs approved without asking, the call
/// waits while the user is asked, who may change the agent, the account, the model and the
/// reasoning effort; the result then names what was done.
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
        // A session the caller owns is always in the caller's own console.
        None => account_when_unset(state, caller, agent, true)?,
    };
    let asked = Asked {
        agent,
        account,
        model: present_text(arguments, "model")?,
        effort: present_text(arguments, "effort")?,
        title: optional_string(arguments, "title"),
        note: optional_string(arguments, "note"),
    };
    let target_id = target.id.clone();
    // Read once: the call acts on it.
    if state.store.auto_approve_handoffs()? {
        let (state_for_run, caller_id, target) = (state.clone(), caller.id.clone(), target.clone());
        return session_request::run_unasked(
            state,
            caller,
            "handoff_session",
            &target_id,
            async move {
                perform(
                    &state_for_run,
                    &caller_id,
                    &target,
                    &asked,
                    Edits::default(),
                    launch,
                )
                .await
            },
        )
        .await;
    }
    // Where nothing the user changes in the dialog could help.
    operation::refuse_unless_it_can_be_handed_off(&target, agent)?;
    let open = operation::receiving_request(state, &target, &asked.handoff(&target), None);
    let account = coordinator::check_open_asking(state, &open).await?;
    let change = Change::Handoff {
        target: target_id,
        agent,
        account: account.clone(),
        model: asked.model.clone(),
        effort: asked.effort.clone(),
        perform: Perform::new({
            let (state, caller_id, target) = (state.clone(), caller.id.clone(), target.clone());
            // The account the dialog showed stands, rather than one settled again at approval.
            let asked = Asked {
                account: Some(account),
                ..asked
            };
            move |edits| async move { perform(&state, &caller_id, &target, &asked, edits, launch).await }
        }),
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

/// What the call asked for.
struct Asked {
    agent: Agent,
    account: Option<Option<String>>,
    model: Option<Option<String>>,
    effort: Option<Option<String>>,
    title: Option<String>,
    note: Option<String>,
}

impl Asked {
    /// The operation's request for `target` as asked.
    fn handoff(&self, target: &Session) -> operation::Handoff {
        operation::Handoff {
            session: target.id.clone(),
            agent: self.agent,
            account: self.account.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            title: self.title.clone(),
            note: self.note.clone(),
            asked_by: None,
            terminal_appearance: None,
        }
    }
}

/// How a setting the user chose is told: its value, or the agent's own default.
fn setting_text(value: &Option<String>) -> String {
    value.as_deref().map_or_else(
        || "the agent's own default".to_string(),
        |text| format!("`{text}`"),
    )
}

/// Hands `target` off as `asked`, with what the user changed in the dialog standing in place of it:
/// another agent takes its own account unless the user chose one, and the model and effort the call
/// asked for belong to the agent it asked for.
async fn perform(
    state: &Arc<AppState>,
    caller_id: &str,
    target: &Session,
    asked: &Asked,
    edits: Edits,
    launch: impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static,
) -> Result<Performed> {
    let (edited_model, edited_effort) = (edits.model.is_some(), edits.effort.is_some());
    let agent = edits.agent.unwrap_or(asked.agent);
    let as_asked = agent == asked.agent;
    let account = match edits.account {
        Some(account) => Some(account),
        None if as_asked => asked.account.clone(),
        // Another agent than the call named, and no account chosen: as a call naming that agent.
        None => {
            let caller = state.session_record(caller_id)?;
            account_when_unset(state, &caller, agent, true)?
        }
    };
    let (model, effort) = if as_asked {
        (
            edits.model.or_else(|| asked.model.clone()),
            edits.effort.or_else(|| asked.effort.clone()),
        )
    } else {
        (edits.model, edits.effort)
    };
    let chose_otherwise = !as_asked || account != asked.account;
    let (set_model, set_effort) = (model.clone(), effort.clone());
    let moved = operation::hand_off_with(
        state,
        operation::Handoff {
            session: target.id.clone(),
            agent,
            account,
            model,
            effort,
            title: asked.title.clone(),
            note: asked.note.clone(),
            asked_by: Some(caller_id.to_string()),
            terminal_appearance: None,
        },
        launch,
    )
    .await?;
    let account_name = state
        .store
        .account_display_name(moved.receiving.agent, moved.receiving.account_id.as_deref())?;
    let mut answer = json!({
        "session": moved.receiving.id,
        "agent": moved.receiving.agent,
        "archived": target.id,
        "account": moved.receiving.account_id.clone().unwrap_or_else(|| "default".to_string()),
        "account_name": account_name,
        "owned_sessions": moved.owned.iter().map(|session| &session.id).collect::<Vec<_>>(),
    });
    let mut note = format!(
        "Handed off: `{}` was archived and the work moved to the new session `{}`, a session of \
         {} on the account `{account_name}` that reports to you like the original did. Drive \
         it with the usual tools.",
        target.id,
        moved.receiving.id,
        moved.receiving.agent.label(),
    );
    if let Some(model) = &moved.receiving.model {
        answer["model"] = json!(model);
    }
    if let Some(effort) = &moved.receiving.effort {
        answer["effort"] = json!(effort);
    }
    if chose_otherwise {
        note.push_str(" The user changed the agent or account you asked for.");
    }
    if let Some(model) = set_model.filter(|_| edited_model) {
        note.push_str(&format!(" Its model is {}.", setting_text(&model)));
    }
    if let Some(effort) = set_effort.filter(|_| edited_effort) {
        note.push_str(&format!(
            " Its reasoning effort is {}.",
            setting_text(&effort)
        ));
    }
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
    Ok(Performed {
        session: moved.receiving,
        result: answer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, peer_state, project_session, shared_console_state};
    use crate::protocol::{Agent, Event, SessionRequestAction, SessionStatus};
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
        state.store.set_auto_approve_handoffs(true).unwrap();
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
        state.store.set_auto_approve_handoffs(true).unwrap();
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
        state.store.set_auto_approve_handoffs(true).unwrap();
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: "codex-work".to_string(),
                agent: Agent::Codex,
                name: "Work".to_string(),
                config_dir: "/tmp/codex-work".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
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

    /// With automatic account selection on, a handoff that names no account takes the account
    /// whose weekly window resets soonest over the caller's own.
    #[tokio::test]
    async fn a_handoff_takes_the_soonest_weekly_reset_when_automatic_selection_is_on() {
        let (state, _dir) = shared_console_state("handoff-follows-auto-select");
        state.store.set_auto_approve_handoffs(true).unwrap();
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
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled: false,
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
                email: None,
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
        assert_eq!(record.account_id.as_deref(), Some("sooner"));
    }

    /// An account the user turned off is refused before the user is asked.
    #[tokio::test]
    async fn a_handoff_to_a_turned_off_account_is_refused_without_asking() {
        let (state, _dir) = shared_console_state("handoff-turned-off");
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: "codex-off".to_string(),
                agent: Agent::Codex,
                name: "codex-off".to_string(),
                config_dir: "/tmp/codex-off".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: true,
            })
            .unwrap();
        let _caller = running(&state, "a");
        let _original = running(&state, "of-a");
        let (launches, started) = (Launches::default(), Started::default());

        let err = handoff_session(
            &state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": "of-a", "agent": "codex", "account": "codex-off" })),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .expect_err("refused");

        assert!(err.to_string().contains("is disabled"), "{err}");
        assert!(!state.session_requests.is_waiting_for("a"));
    }

    /// With automatic selection on and no usable account of the agent, a handoff that names none is
    /// refused naming the account that becomes usable soonest; with automatic selection off it
    /// names the inherited one, here the turned-off default account.
    #[tokio::test]
    async fn a_handoff_with_no_usable_account_names_the_soonest_one_when_selection_is_on() {
        let (state, _dir) = shared_console_state("handoff-soonest-available");
        state.store.set_auto_approve_handoffs(true).unwrap();
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap();
        let used_up = |resets_in: i64| crate::protocol::QuotaWindow {
            kind: crate::protocol::QuotaWindowKind::Session,
            label: None,
            used_percent: 100.0,
            resets_at: Some(crate::protocol::now_millis() + resets_in),
            window_minutes: None,
        };
        for (id, resets_in) in [("codex-work", 7_200_000), ("sooner", 3_600_000)] {
            let dir = format!("/tmp/{id}");
            state
                .store
                .insert_account(&crate::protocol::Account {
                    id: id.to_string(),
                    agent: Agent::Codex,
                    name: id.to_string(),
                    config_dir: dir.clone(),
                    share_default_config: false,
                    share_default_config_incomplete: false,
                    disabled: false,
                })
                .unwrap();
            state.account_quotas.hold_in_dir_for_test(
                &crate::protocol::AccountQuota {
                    agent: Agent::Codex,
                    account: Some(id.to_string()),
                    fetched_at: 1,
                    plan: None,
                    email: None,
                    windows: vec![used_up(resets_in)],
                    unavailable: None,
                },
                &dir,
            );
        }
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
        let caller = state.session_record("a").unwrap();
        let input = arguments(json!({ "session": "of-a", "agent": "codex" }));

        let err = handoff_session(
            &state,
            &caller,
            &input,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().starts_with("sooner "), "{err}");

        state.store.set_auto_select_account(false).unwrap();
        let err = handoff_session(
            &state,
            &caller,
            &input,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().starts_with("Default "), "{err}");

        state.store.set_auto_select_account(true).unwrap();
        state.store.set_auto_approve_handoffs(false).unwrap();
        let call = {
            let (state, caller, input) = (state.clone(), caller.clone(), input.clone());
            let launch = stand_in_launch(&started, &launches, || {});
            tokio::spawn(async move { handoff_session(&state, &caller, &input, launch).await })
        };
        let Event::SessionRequest {
            request_id,
            account,
            ..
        } = crate::mcp::exec::tests::wait_for_request(&state, "a").await
        else {
            panic!("a session request");
        };
        assert_eq!(account.as_deref(), Some("sooner"));
        let refused = session_request::answer(&state, &request_id, true, Edits::default())
            .await
            .expect_err("the displayed account is still used up");
        assert!(refused.to_string().starts_with("sooner "), "{refused}");
        let refused = call.await.unwrap().expect_err("refused at approval");
        assert!(
            refused.to_string().contains("sooner has used up"),
            "{refused}"
        );
        assert!(launches.lock().unwrap().is_empty());
        assert_eq!(
            state.session_record("of-a").unwrap().status,
            SessionStatus::Idle
        );
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
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
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

    /// With the user not having waived the dialog, the call waits, the request carrying what the
    /// call asked for with the account settled; the agent the user picks instead is the one
    /// handed off to, and the result names it.
    #[tokio::test]
    async fn a_handoff_waits_for_the_user_and_goes_where_the_user_chose() {
        let (state, _dir) = shared_console_state("handoff-asks");
        let caller = running(&state, "a");
        let _original = running(&state, "of-a");
        let (launches, started) = (Launches::default(), Started::default());
        let call = {
            let (state, caller) = (state.clone(), state.session_record("a").unwrap());
            let launch = stand_in_launch(&started, &launches, || {});
            tokio::spawn(async move {
                handoff_session(
                    &state,
                    &caller,
                    &arguments(json!({
                        "session": "of-a",
                        "agent": "codex",
                        "title": "Fix the build",
                        "note": "Start with the failing test.",
                    })),
                    launch,
                )
                .await
            })
        };

        let Event::SessionRequest {
            request_id,
            action,
            target,
            agent,
            account,
            model,
            effort,
            ..
        } = crate::mcp::exec::tests::wait_for_request(&state, "a").await
        else {
            panic!("a session request");
        };
        assert_eq!(action, SessionRequestAction::Handoff);
        assert_eq!(target.as_deref(), Some("of-a"));
        assert_eq!(agent, Agent::Codex);
        assert_eq!(
            account, None,
            "the default account, settled when the call was made"
        );
        assert_eq!((model, effort), (None, None));
        assert!(
            launches.lock().unwrap().is_empty(),
            "nothing moves before the answer"
        );

        let receiving = session_request::answer(
            &state,
            &request_id,
            true,
            Edits {
                agent: Some(Agent::Grok),
                account: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        let result = call.await.unwrap().unwrap();
        assert_eq!(result["agent"], "grok");
        assert_eq!(result["session"], receiving.id);
        let record = state.store.get_session(&receiving.id).unwrap().unwrap();
        assert_eq!(record.agent, Agent::Grok);
        assert_eq!(record.title, "Fix the build");
        assert_eq!(
            state.store.get_session("of-a").unwrap().unwrap().status,
            SessionStatus::Archived
        );
        assert_caller_not_told(&state, &caller);
    }

    /// A refusal ends the call with an error saying the user refused and nothing changed.
    #[tokio::test]
    async fn a_refused_handoff_returns_an_error_and_changes_nothing() {
        let (state, _dir) = shared_console_state("handoff-refused-by-user");
        let _caller = running(&state, "a");
        let _original = running(&state, "of-a");
        let before = sessions_as_stored(&state);
        let (launches, started) = (Launches::default(), Started::default());
        let call = {
            let (state, caller) = (state.clone(), state.session_record("a").unwrap());
            let launch = stand_in_launch(&started, &launches, || {});
            tokio::spawn(async move {
                handoff_session(
                    &state,
                    &caller,
                    &arguments(json!({ "session": "of-a", "agent": "codex" })),
                    launch,
                )
                .await
            })
        };
        let Event::SessionRequest { request_id, .. } =
            crate::mcp::exec::tests::wait_for_request(&state, "a").await
        else {
            panic!("a session request");
        };

        session_request::answer(&state, &request_id, false, Edits::default())
            .await
            .unwrap();

        let err = call.await.unwrap().expect_err("refused");
        assert!(err.to_string().contains("the user refused"), "{err}");
        assert!(err.to_string().contains("nothing was changed"), "{err}");
        assert!(launches.lock().unwrap().is_empty());
        assert_eq!(sessions_as_stored(&state), before);
    }

    /// An approval that changes the agent and names no account takes the account a call naming
    /// that agent would, and the result says what was done and that the user changed the ask.
    #[tokio::test]
    async fn a_handoff_to_an_agent_the_user_chose_resolves_its_account() {
        let (state, _dir) = shared_console_state("handoff-changed-agent");
        let _caller = running(&state, "a");
        let _original = running(&state, "of-a");
        let (launches, started) = (Launches::default(), Started::default());
        let call = {
            let (state, caller) = (state.clone(), state.session_record("a").unwrap());
            let launch = stand_in_launch(&started, &launches, || {});
            tokio::spawn(async move {
                handoff_session(
                    &state,
                    &caller,
                    &arguments(json!({ "session": "of-a", "agent": "codex" })),
                    launch,
                )
                .await
            })
        };
        let Event::SessionRequest { request_id, .. } =
            crate::mcp::exec::tests::wait_for_request(&state, "a").await
        else {
            panic!("a session request");
        };

        crate::session_request::answer(
            &state,
            &request_id,
            true,
            crate::session_request::Edits {
                agent: Some(Agent::Grok),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let result = call.await.unwrap().unwrap();
        assert_eq!(result["agent"], "grok");
        assert_eq!(result["account"], "default");
        assert!(result["note"].as_str().unwrap().contains("user changed"));
    }
}
