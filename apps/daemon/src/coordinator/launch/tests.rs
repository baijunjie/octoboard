use super::*;
use crate::agents::launch_settings::normalize_session_launch_setting;
use crate::coordinator::test_support::{console, project_of};
use crate::protocol::{
    error_code, AgentId, Event, GitWorkflowSettings, GitWorkflowSettingsChanges, Session,
    SessionKind, SessionStatus,
};
use crate::session::term;
use crate::test_support::code_of;
use crate::test_support::{
    bare_session, console_with_project_and_accounts, fake_live_session, idle_stand_in,
    output_containing, project_session,
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
    let mut owner = console(AgentId::CLAUDE, Path::new("/tmp/unused"));
    owner.launch[AgentId::CLAUDE].model = Some("console-opus".to_string());
    owner.launch[AgentId::CODEX].effort = Some("console-high".to_string());
    owner.launch[AgentId::GROK].model = Some("console-grok".to_string());
    owner.launch[AgentId::GROK].effort = Some("console-low".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut configured = project_of("console-1", "configured");
    configured.launch[AgentId::CLAUDE].model = Some("project-sonnet".to_string());
    configured.launch[AgentId::CLAUDE].effort = Some("project-low".to_string());
    configured.launch[AgentId::CODEX].model = Some("project-gpt".to_string());
    state.store.insert_project(&configured).unwrap();
    state
        .store
        .insert_project(&project_of("console-1", "plain"))
        .unwrap();

    let launched = |kind: SessionKind, project_id: Option<&str>, agent: AgentId| {
        let mut session = bare_session("s", kind);
        session.project_id = project_id.map(str::to_string);
        session.agent = agent;
        launch_settings(&state, &session).unwrap()
    };
    let pair = |model: Option<&str>, effort: Option<&str>| {
        (model.map(str::to_string), effort.map(str::to_string))
    };

    assert_eq!(
        launched(SessionKind::Console, None, AgentId::CLAUDE),
        pair(Some("console-opus"), None)
    );
    assert_eq!(
        launched(SessionKind::Console, None, AgentId::CODEX),
        pair(None, Some("console-high"))
    );
    assert_eq!(
        launched(SessionKind::Console, None, AgentId::GROK),
        pair(Some("console-grok"), Some("console-low"))
    );
    assert_eq!(
        launched(SessionKind::Project, Some("configured"), AgentId::CLAUDE),
        pair(Some("project-sonnet"), Some("project-low"))
    );
    assert_eq!(
        launched(SessionKind::Project, Some("configured"), AgentId::CODEX),
        pair(Some("project-gpt"), None)
    );
    assert_eq!(
        launched(SessionKind::Project, Some("configured"), AgentId::GROK),
        pair(None, None)
    );
    assert_eq!(
        launched(SessionKind::Project, Some("plain"), AgentId::CLAUDE),
        pair(None, None)
    );
    assert_eq!(
        launched(SessionKind::Project, Some("plain"), AgentId::CODEX),
        pair(None, None)
    );
}

/// The retired Claude alias `best` is read as no model at every level: a project's falls through
/// to its console's, a console's and a session's own leave the choice open, and another agent's
/// model of that name is left alone.
#[test]
fn a_stored_best_model_is_read_as_unset_for_claude_only() {
    let (state, _dir) = crate::test_support::app_state("coordinator-launch-best");
    let mut owner = console(AgentId::CLAUDE, Path::new("/tmp/unused"));
    owner.launch[AgentId::CLAUDE].model = Some("best".to_string());
    owner.launch[AgentId::CODEX].model = Some("best".to_string());
    owner.project_session_launch[AgentId::CLAUDE].model = Some("sonnet".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut project = project_of("console-1", "project-1");
    project.launch[AgentId::CLAUDE].model = Some("best".to_string());
    state.store.insert_project(&project).unwrap();

    let model =
        |kind: SessionKind, project_id: Option<&str>, agent: AgentId, own: Option<Option<&str>>| {
            let mut session = bare_session("s", kind);
            session.project_id = project_id.map(str::to_string);
            session.agent = agent;
            session.model = own.map(|own| own.map(str::to_string));
            launch_settings(&state, &session).unwrap().0
        };
    let some = |model: &str| Some(model.to_string());
    assert_eq!(
        model(SessionKind::Console, None, AgentId::CLAUDE, None),
        None
    );
    assert_eq!(
        model(SessionKind::Console, None, AgentId::CODEX, None),
        some("best")
    );
    assert_eq!(
        model(
            SessionKind::Project,
            Some("project-1"),
            AgentId::CLAUDE,
            None
        ),
        some("sonnet")
    );
    assert_eq!(
        model(
            SessionKind::Console,
            None,
            AgentId::CLAUDE,
            Some(Some("best"))
        ),
        None
    );
    assert_eq!(
        model(
            SessionKind::Console,
            None,
            AgentId::CLAUDE,
            Some(Some("opus"))
        ),
        some("opus")
    );
}

/// A Claude Code session with no model of its own is launched with the best model the account may
/// use; when Claude Code's own configuration names one (the environment, or the project's
/// settings in the session's directory) nothing is passed and Claude Code applies it. A model the
/// session has of its own stands, and the other agents are left to their own default.
#[tokio::test]
async fn an_unset_claude_model_is_launched_as_the_default_unless_claude_code_names_one() {
    use crate::adapter::claude::models::fixtures::{claude_home, CLAUDE_CATALOGUE};
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;
    let (state, dir) = console_with_project_and_accounts("coordinator-launch-default-model");
    let env = claude_home(&dir, "[]", Some(CLAUDE_CATALOGUE));
    let launch_model =
        |id: &str, agent: AgentId, own: Option<Option<&str>>, env: &HashMap<String, String>| {
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
        launch_model("plain", AgentId::CLAUDE, None, &env).await,
        some("opus")
    );
    assert_eq!(
        launch_model("codex", AgentId::CODEX, None, &env).await,
        None
    );
    assert_eq!(
        launch_model("best", AgentId::CLAUDE, Some(Some("best")), &env).await,
        some("opus")
    );
    assert_eq!(
        launch_model("own", AgentId::CLAUDE, Some(Some("haiku")), &env).await,
        some("haiku")
    );
    let mut named = env.clone();
    named.insert("ANTHROPIC_MODEL".to_string(), "sonnet".to_string());
    assert_eq!(
        launch_model("named", AgentId::CLAUDE, None, &named).await,
        None
    );
    // The project's own settings are only known at launch.
    let session = state.session_record("plain").unwrap();
    let cwd = session_cwd(&state, &session).unwrap();
    std::fs::create_dir_all(cwd.join(".claude")).unwrap();
    std::fs::write(cwd.join(".claude/settings.json"), r#"{"model":"opus"}"#).unwrap();
    assert_eq!(
        launch_model("project", AgentId::CLAUDE, None, &env).await,
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
        .insert_console(&console(AgentId::CLAUDE, Path::new("/tmp/unused")))
        .unwrap();
    let owner = state.store.require_console("console-1").unwrap();
    let checked = check_launch_settings(
        &state,
        AgentId::CLAUDE,
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
    let mut owner = console(AgentId::CLAUDE, Path::new("/tmp/unused"));
    owner.launch[AgentId::CLAUDE].model = Some("console-model".to_string());
    owner.launch[AgentId::CLAUDE].effort = Some("console-effort".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut project = project_of("console-1", "project-1");
    project.launch[AgentId::CLAUDE].model = Some("project-model".to_string());
    project.launch[AgentId::CLAUDE].effort = Some("project-effort".to_string());
    state.store.insert_project(&project).unwrap();
    for (kind, project_id) in [
        (SessionKind::Console, None),
        (SessionKind::Project, Some("project-1")),
    ] {
        let mut session = bare_session("s", kind);
        session.project_id = project_id.map(str::to_string);
        let defaults = if kind == SessionKind::Console {
            (
                owner.launch[AgentId::CLAUDE].model.clone(),
                owner.launch[AgentId::CLAUDE].effort.clone(),
            )
        } else {
            (
                project.launch[AgentId::CLAUDE].model.clone(),
                project.launch[AgentId::CLAUDE].effort.clone(),
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
    let mut inherited = bare_session("legacy", SessionKind::Project);
    inherited.project_id = Some("project-1".to_string());
    state.store.insert_session(&inherited).unwrap();
    project.launch[AgentId::CLAUDE].model = Some("changed-model".to_string());
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
        .insert_console(&console(AgentId::CLAUDE, Path::new("/tmp/unused")))
        .unwrap();
    let mut project = project_of("console-1", "project-1");
    state.store.insert_project(&project).unwrap();
    let mut session = bare_session("s", SessionKind::Project);
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
    let mut console_session = bare_session("c", SessionKind::Console);
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
    let mut owner = console(AgentId::CLAUDE, Path::new("/tmp/unused"));
    owner.launch[AgentId::CLAUDE].model = Some("console-session-model".to_string());
    state.store.insert_console(&owner).unwrap();
    let mut project = project_of("console-1", "project-1");
    state.store.insert_project(&project).unwrap();
    let mut session = bare_session("s", SessionKind::Project);
    session.project_id = Some("project-1".to_string());

    // A console that names no pair: nothing to fall back to.
    assert_eq!(launch_settings(&state, &session).unwrap(), (None, None));

    owner.project_session_launch[AgentId::CLAUDE].model = Some("fallback-model".to_string());
    owner.project_session_launch[AgentId::CLAUDE].effort = Some("fallback-effort".to_string());
    owner.project_session_launch[AgentId::CODEX].model = Some("codex-fallback".to_string());
    state.store.update_console(&owner).unwrap();
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (
            Some("fallback-model".to_string()),
            Some("fallback-effort".to_string())
        )
    );

    // The project's own value wins for that value alone.
    project.launch[AgentId::CLAUDE].model = Some("project-model".to_string());
    state.store.update_project(&project).unwrap();
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (
            Some("project-model".to_string()),
            Some("fallback-effort".to_string())
        )
    );

    // Another agent's pair is its own.
    session.agent = AgentId::CODEX;
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (Some("codex-fallback".to_string()), None)
    );
    session.agent = AgentId::CLAUDE;

    // The session's own choice, an explicit null included, wins over both.
    session.model = normalize_session_launch_setting(Some(None));
    session.effort = normalize_session_launch_setting(Some(Some(" high ".to_string())));
    assert_eq!(
        launch_settings(&state, &session).unwrap(),
        (None, Some("high".to_string()))
    );

    // A console session reads only the console session pair.
    let console_session = bare_session("c", SessionKind::Console);
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
    let up = idle_stand_in("up");
    let gone = fake_live_session(
        "gone",
        AgentId::CLAUDE,
        80,
        24,
        "printf 'fixture-started\\n'; exit 1",
    );
    crate::session::spawn_reader_thread(gone.clone(), 8 * 1024);
    // Exec can stall under parallel load. Start the settling window after the shell has run;
    // only its built-in exit remains, so starting the fixture cannot consume that window.
    let ready = output_containing(&gone, "fixture-started");
    assert!(ready.contains("fixture-started"), "{ready}");

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
            AgentId::CLAUDE,
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

/// A console session's launch writes its console's instruction files before the agent starts, as
/// regular files where the host makes no file links, in place of the links an earlier run left.
#[tokio::test]
async fn a_console_session_launch_writes_its_instruction_files_without_file_links() {
    use crate::test_support::{console_session, stand_in_launch, Launched, StandIn};
    use std::sync::Mutex;
    let (state, dir) = crate::test_support::app_state("coordinator-launch-no-file-links");
    let workdir = crate::coordinator::test_support::console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(AgentId::CODEX, &workdir))
        .unwrap();
    std::fs::write(workdir.join("AGENTS.md"), "an earlier run's text").unwrap();
    crate::test_support::symlink("AGENTS.md", workdir.join("CLAUDE.md")).unwrap();
    state
        .store
        .insert_session(&Session {
            agent: AgentId::CODEX,
            ..console_session("hub", SessionStatus::Interrupted)
        })
        .unwrap();
    crate::coordinator::console_workdir::FILE_LINKS_OVERRIDE.set(Some(false));
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
    let claim = state.begin_launch("hub").unwrap();
    relaunch_session_with(
        &state,
        &claim,
        "hub",
        None,
        false,
        None,
        None,
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();
    assert_eq!(launches.lock().unwrap().len(), 1);
    let regular_file = |name: &str| {
        let path = workdir.join(name);
        assert!(
            std::fs::symlink_metadata(&path).unwrap().is_file(),
            "{name}"
        );
        std::fs::read_to_string(path).unwrap()
    };
    assert_eq!(
        regular_file("AGENTS.md"),
        crate::session_tools::console_instructions::console_session_instructions(
            None,
            &crate::session_tools::role::RoleOptions {
                ask_as_choices: true,
                ..Default::default()
            },
        )
    );
    assert_eq!(regular_file("CLAUDE.md"), "@AGENTS.md\n");
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
            AgentId::CLAUDE,
            "\u{1}/clear it",
            Some(" /clear it"),
            0,
        ),
        ("dashed", AgentId::CLAUDE, "- item", Some(" - item"), 0),
        ("blank", AgentId::CLAUDE, "\u{1} ", None, 0),
        ("codex", AgentId::CODEX, "/clear it", None, 1),
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
                AgentId::CLAUDE,
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

/// Starts a relaunch of the stored session `id` whose launch waits for `holder` to answer, and
/// drops the relaunch once the launch has reached it: the request is gone while the holder has
/// not confirmed the start. The claim the request held is dropped with it.
async fn relaunch_dropped_while_unconfirmed(
    state: &Arc<AppState>,
    id: &str,
    holder: std::sync::mpsc::Receiver<Result<(), &'static str>>,
    start: impl FnOnce(term::LaunchRequest) -> anyhow::Result<term::Launch> + Send + 'static,
) {
    use crate::test_support::{within, PATIENCE};
    let (entered, reached) = std::sync::mpsc::channel::<()>();
    let claim = state.begin_launch(id).unwrap();
    {
        let relaunch =
            relaunch_session_with(state, &claim, id, None, true, None, None, move |request| {
                entered.send(()).ok();
                match holder.recv() {
                    Ok(Ok(())) => start(request),
                    Ok(Err(why)) => Err(anyhow::anyhow!(why)),
                    Err(_) => Err(anyhow::anyhow!("the holder was never heard from")),
                }
            });
        tokio::pin!(relaunch);
        let reached = within(PATIENCE, move || reached.try_recv().is_ok());
        tokio::select! {
            result = &mut relaunch => panic!("the relaunch finished before the holder answered: {result:?}"),
            reached = reached => assert!(reached, "the launch never reached the holder"),
        }
    }
    drop(claim);
}

/// A start the holder confirms after the request that asked for it was dropped is still the
/// session's process: the session stays claimed until then, and is registered live and followed
/// when the confirmation comes.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_confirmation_registers_the_session_after_its_request_was_dropped() {
    use crate::test_support::{stand_in_launch, within, Launched, StandIn, PATIENCE};
    use std::sync::Mutex;
    let (state, _dir) = console_with_project_and_accounts("coordinator-launch-late-confirm");
    state
        .store
        .insert_session(&project_session(
            "late",
            AgentId::CLAUDE,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
    let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
    let (answer, holder) = std::sync::mpsc::channel();
    relaunch_dropped_while_unconfirmed(
        &state,
        "late",
        holder,
        stand_in_launch(&started, &launches, || {}),
    )
    .await;

    let refused = state
        .begin_launch("late")
        .err()
        .expect("the session is claimed");
    assert_eq!(code_of(&refused), error_code::SESSION_ALREADY_STARTING);
    assert!(state.live_session("late").is_none());

    answer.send(Ok(())).unwrap();
    let registered = {
        let state = state.clone();
        within(PATIENCE, move || state.live_session("late").is_some()).await
    };
    assert!(
        registered,
        "the late confirmation never registered the session"
    );
    assert_eq!(
        started.lock().unwrap().len(),
        1,
        "the program was started again"
    );

    // Followed: its end is seen, and the claim is let go with the registration's work done.
    started.lock().unwrap().clear();
    let released = {
        let state = state.clone();
        within(PATIENCE, move || {
            state.live_session("late").is_none() && state.begin_launch("late").is_ok()
        })
        .await
    };
    assert!(released, "the session was never released after its end");
}

/// A start that fails after the request that asked for it was dropped lets the session go: the
/// claim is released, and the session is left interrupted, not starting.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_failure_releases_the_session_after_its_request_was_dropped() {
    use crate::test_support::{within, PATIENCE};
    let (state, _dir) = console_with_project_and_accounts("coordinator-launch-late-failure");
    state
        .store
        .insert_session(&project_session(
            "late",
            AgentId::CLAUDE,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let (answer, holder) = std::sync::mpsc::channel();
    relaunch_dropped_while_unconfirmed(&state, "late", holder, |_| unreachable!()).await;
    assert!(
        state.begin_launch("late").is_err(),
        "the session was let go too soon"
    );

    answer.send(Err("the holder could not start it")).unwrap();
    let released = {
        let state = state.clone();
        within(PATIENCE, move || state.begin_launch("late").is_ok()).await
    };
    assert!(released, "the late failure never released the session");
    assert!(state.live_session("late").is_none());
    let within_status = {
        let state = state.clone();
        within(PATIENCE, move || {
            state.session_record("late").unwrap().status == SessionStatus::Interrupted
        })
        .await
    };
    assert!(
        within_status,
        "the session was left as it was optimistically shown"
    );
}

mod compatibility {
    use super::*;
    use crate::providers::SettingValue;

    #[test]
    fn launch_resolution_obeys_every_level_for_each_agent_and_session_kind() {
        let (state, dir) = crate::test_support::app_state("compatibility-resolution");
        let key = "claude_compatibility";
        let mut console = console(AgentId::CODEX, &dir);
        let mut project = project_of("console-1", "project-1");
        state.store.insert_console(&console).unwrap();
        state.store.insert_project(&project).unwrap();
        // All combinations prove both held values, following, and independence of the two sets.
        for agent in [AgentId::CODEX, AgentId::GROK] {
            for global in [false, true] {
                state
                    .store
                    .set_provider_setting(agent, key, Some(&SettingValue::Switch(global)))
                    .unwrap();
                for console_value in [None, Some(false), Some(true)] {
                    for project_value in [None, Some(false), Some(true)] {
                        for session_value in [None, Some(false), Some(true)] {
                            console.project_session_launch[agent].compatibility = console_value
                                .map(|v| (key.to_string(), v))
                                .into_iter()
                                .collect();
                            console.launch[agent].compatibility =
                                [(key.to_string(), !global)].into();
                            project.launch[agent].compatibility = project_value
                                .map(|v| (key.to_string(), v))
                                .into_iter()
                                .collect();
                            state.store.update_console(&console).unwrap();
                            state.store.update_project(&project).unwrap();
                            let mut session = bare_session("s", SessionKind::Project);
                            session.agent = agent;
                            session.project_id = Some(project.id.clone());
                            session.compatibility = session_value
                                .map(|v| (key.to_string(), v))
                                .into_iter()
                                .collect();
                            assert_eq!(
                                launch_provider_settings(&state, &session)
                                    .unwrap()
                                    .switch(key),
                                session_value
                                    .or(project_value)
                                    .or(console_value)
                                    .unwrap_or(global)
                            );
                            session.kind = SessionKind::Console;
                            session.project_id = None;
                            assert_eq!(
                                launch_provider_settings(&state, &session)
                                    .unwrap()
                                    .switch(key),
                                session_value.unwrap_or(!global)
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn compatibility_following_reads_the_current_agent_setting_at_each_launch() {
        let (state, dir) = crate::test_support::app_state("compatibility-agent-refresh");
        let mut console = console(AgentId::CODEX, &dir);
        let mut project = project_of("console-1", "project-1");
        state.store.insert_console(&console).unwrap();
        state.store.insert_project(&project).unwrap();
        let key = "claude_compatibility";
        for agent in [AgentId::CODEX, AgentId::GROK] {
            for global in [false, true] {
                // A fresh launch reads an edited agent value, with no stale resolved value stored.
                let mut session = bare_session("s", SessionKind::Project);
                session.agent = agent;
                session.project_id = Some(project.id.clone());
                console.project_session_launch[agent].compatibility.clear();
                project.launch[agent].compatibility.clear();
                state.store.update_console(&console).unwrap();
                state.store.update_project(&project).unwrap();
                state
                    .store
                    .set_provider_setting(agent, key, Some(&SettingValue::Switch(!global)))
                    .unwrap();
                assert_eq!(
                    effective_compatibility(&state, &session).unwrap()[key],
                    !global
                );
            }
        }
    }

    #[test]
    fn overrides_do_not_change_choice_settings_or_the_agent_setting() {
        let (state, dir) = crate::test_support::app_state("compatibility-choices");
        state
            .store
            .insert_console(&console(AgentId::CODEX, &dir))
            .unwrap();
        let mut session = bare_session("s", SessionKind::Console);
        session.agent = AgentId::CODEX;
        session
            .compatibility
            .insert("claude_compatibility".into(), true);
        state
            .store
            .set_provider_setting(
                AgentId::CODEX,
                "approval_reviewer",
                Some(&SettingValue::Choice("full_access".into())),
            )
            .unwrap();
        let before = state.store.provider_settings().unwrap();
        let resolved = launch_provider_settings(&state, &session).unwrap();
        assert!(resolved.switch("claude_compatibility"));
        assert_eq!(
            resolved.choice("approval_reviewer"),
            state
                .store
                .provider_settings_of(session.agent)
                .unwrap()
                .choice("approval_reviewer")
        );
        assert_eq!(state.store.provider_settings().unwrap(), before);
    }

    #[tokio::test(start_paused = true)]
    async fn every_relaunch_reads_the_current_chain_into_launch_request_settings() {
        use std::sync::{Arc, Mutex};
        let (state, dir) = crate::test_support::app_state("compatibility-relaunch-read");
        state
            .store
            .insert_console(&console(AgentId::CODEX, &dir))
            .unwrap();
        let mut project = project_of("console-1", "project-1");
        project.path = dir.to_string_lossy().into_owned();
        state.store.insert_project(&project).unwrap();
        let mut session = bare_session("s", SessionKind::Project);
        session.agent = AgentId::CODEX;
        session.project_id = Some(project.id.clone());
        session.status = SessionStatus::Interrupted;
        session
            .compatibility
            .insert("claude_compatibility".into(), false);
        state.store.insert_session(&session).unwrap();
        for (held, inherited) in [
            (Some(false), true),
            (None, true),
            (None, false),
            (Some(true), false),
        ] {
            session.compatibility = held
                .map(|v| ("claude_compatibility".into(), v))
                .into_iter()
                .collect();
            state
                .store
                .set_session_compatibility(&session.id, &session.compatibility)
                .unwrap();
            project.launch[AgentId::CODEX]
                .compatibility
                .insert("claude_compatibility".into(), inherited);
            state.store.update_project(&project).unwrap();
            let observed = Arc::new(Mutex::new(None));
            let seen = observed.clone();
            let claim = state.begin_launch("s").unwrap();
            let result = relaunch_session_with(
                &state,
                &claim,
                "s",
                None,
                false,
                Some(HashMap::new()),
                None,
                move |request| {
                    *seen.lock().unwrap() = Some(request.settings.switch("claude_compatibility"));
                    Err(anyhow::anyhow!("fixture stops before creating a process"))
                },
            )
            .await;
            assert!(result.is_err());
            tokio::time::advance(crate::test_support::PATIENCE).await;
            assert_eq!(*observed.lock().unwrap(), Some(held.unwrap_or(inherited)));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn opening_a_session_records_held_switches_and_launches_with_them() {
        use crate::coordinator::open::open_session_with;
        use crate::coordinator::test_support::{open_request, state_for_opening};
        use std::sync::{Arc, Mutex};
        let (state, _dir) = state_for_opening("compatibility-open");
        let observed = Arc::new(Mutex::new(None));
        let seen = observed.clone();
        let during = state.clone();
        let mut request = open_request();
        request.agent = Some(AgentId::CODEX);
        request
            .compatibility
            .insert("claude_compatibility".into(), Some(true));
        let result = open_session_with(&state, request, move |request| {
            let recorded = during.session_record(&request.session_id).unwrap();
            *seen.lock().unwrap() = Some((
                recorded.compatibility,
                request.settings.switch("claude_compatibility"),
            ));
            Err(anyhow::anyhow!("fixture stops before creating a process"))
        })
        .await;
        assert!(result.is_err());
        assert_eq!(
            *observed.lock().unwrap(),
            Some(([("claude_compatibility".into(), true)].into(), true))
        );
    }

    #[test]
    fn a_hold_for_one_switch_or_agent_does_not_hide_another_switch() {
        let (state, dir) = crate::test_support::app_state("compatibility-independent");
        let mut hub = console(AgentId::GROK, &dir);
        let mut project = project_of("console-1", "project-1");
        hub.project_session_launch[AgentId::GROK]
            .compatibility
            .insert("codex_compatibility".into(), true);
        project.launch[AgentId::GROK]
            .compatibility
            .insert("claude_compatibility".into(), true);
        project.launch[AgentId::CODEX]
            .compatibility
            .insert("claude_compatibility".into(), false);
        state.store.insert_console(&hub).unwrap();
        state.store.insert_project(&project).unwrap();
        let mut session = bare_session("s", SessionKind::Project);
        session.agent = AgentId::GROK;
        session.project_id = Some(project.id);
        session
            .compatibility
            .insert("claude_compatibility".into(), false);
        assert_eq!(
            effective_compatibility(&state, &session).unwrap(),
            [
                ("claude_compatibility".into(), false),
                ("codex_compatibility".into(), true)
            ]
            .into()
        );
        session.compatibility.clear();
        assert!(launch_provider_settings(&state, &session)
            .unwrap()
            .switch("claude_compatibility"));
        session.agent = AgentId::CODEX;
        assert!(!launch_provider_settings(&state, &session)
            .unwrap()
            .switch("claude_compatibility"));
    }
}

#[test]
fn launch_overrides_are_written_and_published_together_once() {
    let (state, dir) = crate::test_support::app_state("launch-overrides-record");
    state
        .store
        .insert_console(&console(AgentId::CODEX, &dir))
        .unwrap();
    let mut session = bare_session("s", SessionKind::Console);
    session.agent = AgentId::CODEX;
    state.store.insert_session(&session).unwrap();
    let audit = rusqlite::Connection::open(dir.join("octoboard.db")).unwrap();
    audit
        .execute_batch(
            "CREATE TABLE launch_writes (id TEXT);
         CREATE TRIGGER count_launch_writes AFTER UPDATE ON sessions
         BEGIN INSERT INTO launch_writes VALUES (NEW.id); END;",
        )
        .unwrap();
    let mut events = state.subscribe();
    let overrides = LaunchOverrides {
        model: Some(Some("model".into())),
        effort: Some(None),
        compatibility: [("claude_compatibility".into(), false)].into(),
    };
    overrides.record(&state, "s").unwrap();
    let count: i64 = audit
        .query_row("SELECT COUNT(*) FROM launch_writes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let crate::protocol::Event::SessionUpserted { session } = events.try_recv().unwrap() else {
        panic!("expected session_upserted");
    };
    assert_eq!(session.model, overrides.model);
    assert_eq!(session.effort, overrides.effort);
    assert_eq!(session.compatibility, overrides.compatibility);
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}
