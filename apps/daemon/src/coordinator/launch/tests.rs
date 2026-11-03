use super::*;
use crate::agents::launch_settings::normalize_session_launch_setting;
use crate::coordinator::test_support::{console, project_of};
use crate::protocol::{
    error_code, Agent, Event, GitWorkflowSettings, GitWorkflowSettingsChanges, Role, Session,
    SessionStatus,
};
use crate::session::term;
use crate::test_support::code_of;
use crate::test_support::{
    bare_session, console_with_project_and_accounts, fake_live_session, idle_stand_in,
    project_session,
};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// A project session reads its project's pair for its own agent and never the console's; a
/// console session reads the console's pair and never a project's. A project that sets nothing
/// leaves its sessions launching exactly as the agent would unasked, whatever the console sets.
#[test]
fn a_session_launches_with_the_pair_of_the_record_that_owns_it() {
    let (state, _dir) = crate::test_support::app_state("coordinator-launch-settings");
    let mut owner = console(Agent::Claude, Path::new("/tmp/unused"));
    owner.launch.claude.model = Some("console-opus".to_string());
    owner.launch.codex.effort = Some("console-high".to_string());
    owner.launch.grok.model = Some("console-grok".to_string());
    owner.launch.grok.effort = Some("console-low".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut configured = project_of("console-1", "configured");
    configured.launch.claude.model = Some("project-sonnet".to_string());
    configured.launch.claude.effort = Some("project-low".to_string());
    configured.launch.codex.model = Some("project-gpt".to_string());
    state.store.insert_project(&configured).unwrap();
    state
        .store
        .insert_project(&project_of("console-1", "plain"))
        .unwrap();

    let launched = |role: Role, project_id: Option<&str>, agent: Agent| {
        let mut session = bare_session("s", role);
        session.project_id = project_id.map(str::to_string);
        session.agent = agent;
        launch_settings(&state, &session).unwrap()
    };
    let pair = |model: Option<&str>, effort: Option<&str>| {
        (model.map(str::to_string), effort.map(str::to_string))
    };

    assert_eq!(
        launched(Role::Console, None, Agent::Claude),
        pair(Some("console-opus"), None)
    );
    assert_eq!(
        launched(Role::Console, None, Agent::Codex),
        pair(None, Some("console-high"))
    );
    assert_eq!(
        launched(Role::Console, None, Agent::Grok),
        pair(Some("console-grok"), Some("console-low"))
    );
    assert_eq!(
        launched(Role::Project, Some("configured"), Agent::Claude),
        pair(Some("project-sonnet"), Some("project-low"))
    );
    assert_eq!(
        launched(Role::Project, Some("configured"), Agent::Codex),
        pair(Some("project-gpt"), None)
    );
    assert_eq!(
        launched(Role::Project, Some("configured"), Agent::Grok),
        pair(None, None)
    );
    assert_eq!(
        launched(Role::Project, Some("plain"), Agent::Claude),
        pair(None, None)
    );
    assert_eq!(
        launched(Role::Project, Some("plain"), Agent::Codex),
        pair(None, None)
    );
}

/// The retired Claude alias `best` is read as no model at every level: a project's falls through
/// to its console's, a console's and a session's own leave the choice open, and another agent's
/// model of that name is left alone.
#[test]
fn a_stored_best_model_is_read_as_unset_for_claude_only() {
    let (state, _dir) = crate::test_support::app_state("coordinator-launch-best");
    let mut owner = console(Agent::Claude, Path::new("/tmp/unused"));
    owner.launch.claude.model = Some("best".to_string());
    owner.launch.codex.model = Some("best".to_string());
    owner.project_session_launch.claude.model = Some("sonnet".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut project = project_of("console-1", "project-1");
    project.launch.claude.model = Some("best".to_string());
    state.store.insert_project(&project).unwrap();

    let model = |role: Role, project_id: Option<&str>, agent: Agent, own: Option<Option<&str>>| {
        let mut session = bare_session("s", role);
        session.project_id = project_id.map(str::to_string);
        session.agent = agent;
        session.model = own.map(|own| own.map(str::to_string));
        launch_settings(&state, &session).unwrap().0
    };
    let some = |model: &str| Some(model.to_string());
    assert_eq!(model(Role::Console, None, Agent::Claude, None), None);
    assert_eq!(model(Role::Console, None, Agent::Codex, None), some("best"));
    assert_eq!(
        model(Role::Project, Some("project-1"), Agent::Claude, None),
        some("sonnet")
    );
    assert_eq!(
        model(Role::Console, None, Agent::Claude, Some(Some("best"))),
        None
    );
    assert_eq!(
        model(Role::Console, None, Agent::Claude, Some(Some("opus"))),
        some("opus")
    );
}

/// A Claude Code session with no model of its own is launched with the best model the account may
/// use; when Claude Code's own configuration names one (the environment, or the project's
/// settings in the session's directory) nothing is passed and Claude Code applies it. A model the
/// session has of its own stands, and the other agents are left to their own default.
#[tokio::test]
async fn an_unset_claude_model_is_launched_as_the_default_unless_claude_code_names_one() {
    use crate::agents::models::claude_fixtures::{claude_home, CLAUDE_CATALOGUE};
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;
    let (state, dir) = console_with_project_and_accounts("coordinator-launch-default-model");
    let env = claude_home(&dir, "[]", Some(CLAUDE_CATALOGUE));
    let launch_model =
        |id: &str, agent: Agent, own: Option<Option<&str>>, env: &HashMap<String, String>| {
            let session = Session {
                agent_session_id: Some(format!("conversation-{id}")),
                has_conversation: true,
                model: own.map(|own| own.map(str::to_string)),
                ..project_session(id, agent, SessionStatus::Interrupted)
            };
            state.store.insert_session(&session).unwrap();
            let (state, env, id) = (state.clone(), env.clone(), id.to_string());
            async move {
                let models = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
                let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
                let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
                let claim = state.begin_launch(&id).unwrap();
                let launch = stand_in_launch(&started, &launches, || {});
                let seen = models.clone();
                relaunch_session_with(
                    &state,
                    &claim,
                    &id,
                    None,
                    true,
                    Some(env),
                    None,
                    move |request| {
                        seen.lock().unwrap().push(request.model.clone());
                        launch(request)
                    },
                )
                .await
                .unwrap();
                let model = models.lock().unwrap()[0].clone();
                model
            }
        };
    let some = |model: &str| Some(model.to_string());
    assert_eq!(
        launch_model("plain", Agent::Claude, None, &env).await,
        some("opus")
    );
    assert_eq!(launch_model("codex", Agent::Codex, None, &env).await, None);
    assert_eq!(
        launch_model("best", Agent::Claude, Some(Some("best")), &env).await,
        some("opus")
    );
    assert_eq!(
        launch_model("own", Agent::Claude, Some(Some("haiku")), &env).await,
        some("haiku")
    );
    let mut named = env.clone();
    named.insert("ANTHROPIC_MODEL".to_string(), "sonnet".to_string());
    assert_eq!(
        launch_model("named", Agent::Claude, None, &named).await,
        None
    );
    // The project's own settings are only known at launch.
    let session = state.session_record("plain").unwrap();
    let cwd = session_cwd(&state, &session).unwrap();
    std::fs::create_dir_all(cwd.join(".claude")).unwrap();
    std::fs::write(cwd.join(".claude/settings.json"), r#"{"model":"opus"}"#).unwrap();
    assert_eq!(
        launch_model("project", Agent::Claude, None, &env).await,
        None
    );
}

/// A stored `best` is no model, so it is no change when only the effort of a session that holds
/// it is edited: it is not checked against what the agent offers, and is kept as unset.
#[tokio::test]
async fn a_stored_best_model_is_not_refused_when_only_the_effort_changes() {
    let (state, _dir) = crate::test_support::app_state("coordinator-launch-best-effort");
    state
        .store
        .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
        .unwrap();
    let owner = state.store.require_console("console-1").unwrap();
    let checked = check_launch_settings(
        &state,
        Agent::Claude,
        None,
        None,
        &owner,
        Some(Some("best".to_string())),
        Some(Some("high".to_string())),
        SettingsChecked::All,
    )
    .await
    .unwrap();
    assert_eq!(checked, (Some(None), Some(Some("high".to_string()))));
}

/// Every launch reads inherited defaults live, independently per field, while a session's
/// explicit value or explicit null takes precedence over its parent's defaults.
#[test]
fn session_launch_overrides_take_precedence_over_live_defaults() {
    let (state, _dir) = crate::test_support::app_state("coordinator-session-overrides");
    let mut owner = console(Agent::Claude, Path::new("/tmp/unused"));
    owner.launch.claude.model = Some("console-model".to_string());
    owner.launch.claude.effort = Some("console-effort".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut project = project_of("console-1", "project-1");
    project.launch.claude.model = Some("project-model".to_string());
    project.launch.claude.effort = Some("project-effort".to_string());
    state.store.insert_project(&project).unwrap();
    for (role, project_id) in [(Role::Console, None), (Role::Project, Some("project-1"))] {
        let mut session = bare_session("s", role);
        session.project_id = project_id.map(str::to_string);
        let defaults = if role == Role::Console {
            (
                owner.launch.claude.model.clone(),
                owner.launch.claude.effort.clone(),
            )
        } else {
            (
                project.launch.claude.model.clone(),
                project.launch.claude.effort.clone(),
            )
        };
        assert_eq!(launch_settings(&state, &session).unwrap(), defaults);
        session.model = normalize_session_launch_setting(Some(Some(" custom ".to_string())));
        assert_eq!(
            launch_settings(&state, &session).unwrap(),
            (Some("custom".to_string()), defaults.1)
        );
        session.effort = normalize_session_launch_setting(Some(Some(" high ".to_string())));
        assert_eq!(
            launch_settings(&state, &session).unwrap(),
            (Some("custom".to_string()), Some("high".to_string()))
        );
        session.effort = normalize_session_launch_setting(Some(Some("  ".to_string())));
        assert_eq!(
            launch_settings(&state, &session).unwrap(),
            (Some("custom".to_string()), None)
        );
        session.model = normalize_session_launch_setting(Some(None));
        assert_eq!(launch_settings(&state, &session).unwrap(), (None, None));
    }
    let mut inherited = bare_session("legacy", Role::Project);
    inherited.project_id = Some("project-1".to_string());
    state.store.insert_session(&inherited).unwrap();
    project.launch.claude.model = Some("changed-model".to_string());
    state.store.update_project(&project).unwrap();
    let reread = state.session_record("legacy").unwrap();
    assert_eq!(
        launch_settings(&state, &reread).unwrap().0.as_deref(),
        Some("changed-model")
    );
    assert_eq!(normalize_session_launch_setting(None), None);
}

/// A project session launches with its project's git workflow settings as they are at that
/// launch, the project's own value winning; a console session, which is given no rule, with
/// none.
#[test]
fn a_project_session_launches_with_its_projects_git_workflow_settings() {
    let (state, _dir) = crate::test_support::app_state("coordinator-git-workflow-settings");
    state
        .store
        .insert_console(&console(Agent::Claude, Path::new("/tmp/unused")))
        .unwrap();
    let mut project = project_of("console-1", "project-1");
    state.store.insert_project(&project).unwrap();
    let mut session = bare_session("s", Role::Project);
    session.project_id = Some("project-1".to_string());
    assert_eq!(
        launch_git_workflow_settings(&state, &session).unwrap(),
        Some(GitWorkflowSettings::default())
    );
    state
        .store
        .change_git_workflow_settings(&GitWorkflowSettingsChanges {
            delivery: Some(crate::protocol::GitDelivery::Merge),
            ..GitWorkflowSettingsChanges::default()
        })
        .unwrap();
    project.git_workflow.development = Some(crate::protocol::GitDevelopment::InPlace);
    state.store.update_project(&project).unwrap();
    assert_eq!(
        launch_git_workflow_settings(&state, &session).unwrap(),
        Some(GitWorkflowSettings {
            development: crate::protocol::GitDevelopment::InPlace,
            delivery: crate::protocol::GitDelivery::Merge,
            ..GitWorkflowSettings::default()
        })
    );
    let mut console_session = bare_session("c", Role::Console);
    console_session.project_id = Some("project-1".to_string());
    assert_eq!(
        launch_git_workflow_settings(&state, &console_session).unwrap(),
        None
    );
}

/// A project session takes each value from its project, else from its console's project
/// session pair, else leaves it to the agent, independently per value and per agent; a
/// console that names no pair leaves it to the agent, and a console session never reads it.
#[test]
fn a_project_session_falls_back_to_the_consoles_project_session_pair_per_value() {
    let (state, _dir) = crate::test_support::app_state("coordinator-project-session-pair");
    let mut owner = console(Agent::Claude, Path::new("/tmp/unused"));
    owner.launch.claude.model = Some("console-session-model".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut project = project_of("console-1", "project-1");
    state.store.insert_project(&project).unwrap();
    let mut session = bare_session("s", Role::Project);
    session.project_id = Some("project-1".to_string());

    // A console that names no pair: nothing to fall back to.
    assert_eq!(launch_settings(&state, &session).unwrap(), (None, None));

    owner.project_session_launch.claude.model = Some("fallback-model".to_string());
    owner.project_session_launch.claude.effort = Some("fallback-effort".to_string());
    owner.project_session_launch.codex.model = Some("codex-fallback".to_string());
    state.store.update_console(&owner).unwrap();
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (
            Some("fallback-model".to_string()),
            Some("fallback-effort".to_string())
        )
    );

    // The project's own value wins for that value alone.
    project.launch.claude.model = Some("project-model".to_string());
    state.store.update_project(&project).unwrap();
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (
            Some("project-model".to_string()),
            Some("fallback-effort".to_string())
        )
    );

    // Another agent's pair is its own.
    session.agent = Agent::Codex;
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (Some("codex-fallback".to_string()), None)
    );
    session.agent = Agent::Claude;

    // The session's own choice, an explicit null included, wins over both.
    session.model = normalize_session_launch_setting(Some(None));
    session.effort = normalize_session_launch_setting(Some(Some(" high ".to_string())));
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (None, Some("high".to_string()))
    );

    // A console session reads only the console session pair.
    let console_session = bare_session("c", Role::Console);
    assert_eq!(
        launch_settings(&state, &console_session).unwrap(),
        (Some("console-session-model".to_string()), None)
    );
}

/// A relaunched process that has ended by the end of the settling time did not come up; one
/// still running did.
#[tokio::test]
async fn a_relaunched_process_that_ends_at_once_did_not_come_up() {
    let settle = Duration::from_millis(400);
    let gone = fake_live_session("gone", Agent::Claude, 80, 24, "exit 1");
    let up = idle_stand_in("up");

    assert!(!came_up(&gone, settle).await);
    assert!(came_up(&up, settle).await);
}

/// An archive that lands before the relaunch writes anything is not undone by the switch: the
/// relaunch refuses an archived session unless it is a resume reopening one, and leaves it
/// archived.
#[tokio::test]
async fn a_relaunch_for_a_switch_never_reopens_an_archived_session() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-switch-archived-late");
    state
        .store
        .insert_session(&project_session(
            "s",
            Agent::Claude,
            SessionStatus::Archived,
        ))
        .unwrap();
    let claim = state.begin_switch("s").unwrap();

    let err = relaunch_session_with(&state, &claim, "s", None, false, None, None, term::launch)
        .await
        .expect_err("archived");
    assert_eq!(code_of(&err), error_code::SESSION_ARCHIVED);
    assert_eq!(
        state.store.get_session("s").unwrap().unwrap().status,
        SessionStatus::Archived
    );
}

/// A Claude Code resume takes the instruction as its launch prompt, shaped as a written message
/// is, and queues nothing: written into the session it would meet the "Resume this
/// conversation?" dialog. Another agent has it queued, as before.
#[tokio::test]
async fn a_claude_resume_takes_its_instruction_as_the_launch_prompt() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;
    let (state, _dir) = console_with_project_and_accounts("coordinator-resume-prompt");
    for (id, agent, instruction, prompt, queued) in [
        (
            "claude",
            Agent::Claude,
            "\u{1}/clear it",
            Some(" /clear it"),
            0,
        ),
        ("dashed", Agent::Claude, "- item", Some(" - item"), 0),
        ("blank", Agent::Claude, "\u{1} ", None, 0),
        ("codex", Agent::Codex, "/clear it", None, 1),
    ] {
        state
            .store
            .insert_session(&Session {
                agent_session_id: Some(format!("conversation-{id}")),
                has_conversation: true,
                ..project_session(id, agent, SessionStatus::Interrupted)
            })
            .unwrap();
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
        let claim = state.begin_launch(id).unwrap();
        relaunch_session_with(
            &state,
            &claim,
            id,
            Some(instruction),
            true,
            None,
            None,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        let launches = launches.lock().unwrap();
        assert_eq!(launches[0].2.as_deref(), prompt, "{id}");
        assert_eq!(state.queued_count(id), queued, "{id}");
    }
}

/// Clients are shown a relaunch as handing the agent a prompt exactly while it is under way and
/// given an instruction (a carried-on turn, say), and not once its claim is released.
#[tokio::test]
async fn a_relaunch_shows_as_prompting_only_while_it_is_given_an_instruction() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;
    let (state, _dir) = console_with_project_and_accounts("coordinator-relaunch-prompting");
    for (id, instruction) in [("carried", Some("carry on")), ("plain", None)] {
        state
            .store
            .insert_session(&project_session(
                id,
                Agent::Claude,
                SessionStatus::Interrupted,
            ))
            .unwrap();
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
        let shown = Arc::new(Mutex::new(None));
        let claim = state.begin_launch(id).unwrap();
        relaunch_session_with(
            &state,
            &claim,
            id,
            instruction,
            true,
            None,
            None,
            stand_in_launch(&started, &launches, {
                let (state, shown, id) = (state.clone(), shown.clone(), id.to_string());
                move || {
                    let record = state.session_record(&id).unwrap();
                    *shown.lock().unwrap() = Some(state.wire_session(record).prompt_on_launch);
                }
            }),
        )
        .await
        .unwrap();
        assert_eq!(*shown.lock().unwrap(), Some(instruction.is_some()), "{id}");
        let mut events = state.subscribe();
        drop(claim);
        // Releasing the claim announces the session with the flag cleared, when it was set.
        let mut cleared = false;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionUpserted { session } = event {
                if session.id == id {
                    assert!(!session.prompt_on_launch, "{id}");
                    cleared = true;
                }
            }
        }
        assert_eq!(cleared, instruction.is_some(), "{id}");
        assert!(
            !state
                .wire_session(state.session_record(id).unwrap())
                .prompt_on_launch,
            "{id}"
        );
    }
}
