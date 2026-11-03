use super::*;
use crate::coordinator::handle;
use crate::coordinator::projects::{update_project, ProjectChanges};
use crate::coordinator::test_support::{console, console_workdir_in};
use crate::protocol::{
    error_code, Agent, CodedError, Launch, LaunchChange, PerAgent, Project, ProjectAccount,
    ProjectSource, RequestBody,
};
use crate::session_tools;
use crate::store::LOCAL_HOST_ID;
use std::path::Path;

/// An `update_console` request that changes nothing, then `edit` applied to it.
fn update_console_request(console: &str, edit: impl FnOnce(&mut RequestBody)) -> RequestBody {
    let mut request = RequestBody::UpdateConsole {
        console: console.to_string(),
        name: None,
        default_agent: None,
        project_session_agent: None,
        accounts: Default::default(),
        launch: Default::default(),
        project_session_launch: Default::default(),
        icon: None,
        console_session_prompt: None,
    };
    edit(&mut request);
    request
}

/// A create the store cannot write, because the data folder is read-only, is refused as
/// `store_write_failed` with the `access` cause, and leaves no working directory behind.
#[tokio::test]
async fn a_create_the_store_cannot_write_leaves_no_working_directory() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: a plain query of this process's effective user. Permissions do not stop the
    // superuser, so the store never fails there.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let (state, dir) = crate::test_support::app_state("coordinator-create-console-read-only");
    let consoles = dir.join("consoles");
    std::fs::create_dir_all(&consoles).unwrap();
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let refused = handle(
        &state,
        None,
        RequestBody::CreateConsole {
            name: "Console".to_string(),
            default_agent: Agent::Claude,
            project_session_agent: None,
            accounts: PerAgent {
                claude: None,
                codex: None,
                grok: None,
            },
            launch: Default::default(),
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
        },
    )
    .await;
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (code, params, _) = crate::protocol::error_parts(&refused.expect_err("read-only"));
    assert_eq!(code, error_code::STORE_WRITE_FAILED);
    assert_eq!(params["cause"], "access");
    assert!(state.store.list_consoles().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(&consoles).unwrap().count(), 0);
}

/// A create whose instruction file cannot be written, because the settings it reads cannot be,
/// stores no console and takes its new working directory with it.
#[tokio::test]
async fn a_create_whose_instructions_cannot_be_written_leaves_no_working_directory() {
    let (state, dir) = crate::test_support::app_state("coordinator-create-console-no-instructions");
    state.store.drop_settings().unwrap();
    let consoles = dir.join("consoles");
    std::fs::create_dir_all(&consoles).unwrap();
    let refused = handle(
        &state,
        None,
        RequestBody::CreateConsole {
            name: "Console".to_string(),
            default_agent: Agent::Claude,
            project_session_agent: None,
            accounts: PerAgent {
                claude: None,
                codex: None,
                grok: None,
            },
            launch: Default::default(),
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
        },
    )
    .await;
    refused.expect_err("the settings are gone");
    assert!(state.store.list_consoles().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(&consoles).unwrap().count(), 0);
}

/// A new console stores its default agent and launch settings; a model or effort is trimmed,
/// with blank read as unset.
#[tokio::test]
async fn a_new_console_stores_its_default_agent_and_launch_settings() {
    let (state, dir) = crate::test_support::app_state("coordinator-create-console-agent");
    crate::test_support::offer_models(&state, Agent::Codex, &[("gpt-5", &["high"])]);
    handle(
        &state,
        None,
        RequestBody::CreateConsole {
            name: "Console".to_string(),
            default_agent: Agent::Codex,
            project_session_agent: None,
            accounts: PerAgent {
                claude: None,
                codex: None,
                grok: None,
            },
            launch: PerAgent {
                claude: Launch {
                    model: Some("  ".to_string()),
                    effort: None,
                },
                codex: Launch {
                    model: Some(" gpt-5 ".to_string()),
                    effort: Some("high".to_string()),
                },
                grok: Launch {
                    model: None,
                    effort: None,
                },
            },
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
        },
    )
    .await
    .expect("created");
    let created = state.store.list_consoles().unwrap().remove(0);
    assert!(Path::new(&created.workdir).starts_with(&*dir));
    assert_eq!(created.default_agent, Agent::Codex);
    assert_eq!(created.launch.claude.model, None);
    assert_eq!(created.launch.codex.model.as_deref(), Some("gpt-5"));
    assert_eq!(created.launch.codex.effort.as_deref(), Some("high"));

    // A model the agent does not offer is refused, and no second console is created.
    let err = handle(
        &state,
        None,
        RequestBody::CreateConsole {
            name: "Other".to_string(),
            default_agent: Agent::Codex,
            project_session_agent: None,
            accounts: PerAgent {
                claude: None,
                codex: None,
                grok: None,
            },
            launch: PerAgent {
                claude: Launch {
                    model: None,
                    effort: None,
                },
                codex: Launch {
                    model: Some("gpt-9".to_string()),
                    effort: None,
                },
                grok: Launch {
                    model: None,
                    effort: None,
                },
            },
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
        },
    )
    .await
    .expect_err("refused");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::INVALID_LAUNCH_SETTING);
    assert_eq!(state.store.list_consoles().unwrap().len(), 1);
}

/// The project session agent is stored at creation, left alone by an absent field, set by a
/// value and cleared by `null`.
#[tokio::test]
async fn the_project_session_agent_is_left_set_or_cleared_by_an_update() {
    let (state, dir) = crate::test_support::app_state("coordinator-project-session-agent");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let stored = || state.store.get_console("console-1").unwrap().unwrap();
    let update = |value: Option<Option<Agent>>| {
        update_console_request("console-1", move |request| {
            let RequestBody::UpdateConsole {
                project_session_agent,
                ..
            } = request
            else {
                unreachable!()
            };
            *project_session_agent = value;
        })
    };

    handle(&state, None, update(Some(Some(Agent::Codex))))
        .await
        .unwrap();
    assert_eq!(stored().project_session_agent, Some(Agent::Codex));
    handle(&state, None, update(None)).await.unwrap();
    assert_eq!(stored().project_session_agent, Some(Agent::Codex));
    handle(&state, None, update(Some(None))).await.unwrap();
    assert_eq!(stored().project_session_agent, None);

    handle(
        &state,
        None,
        RequestBody::CreateConsole {
            name: "Other".to_string(),
            default_agent: Agent::Claude,
            project_session_agent: Some(Agent::Grok),
            accounts: Default::default(),
            launch: Default::default(),
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
        },
    )
    .await
    .unwrap();
    let created = state
        .store
        .list_consoles()
        .unwrap()
        .into_iter()
        .find(|console| console.name == "Other")
        .unwrap();
    assert_eq!(created.project_session_agent, Some(Agent::Grok));
}

/// A launch setting is left alone when absent, set (trimmed) by a value, and cleared by `null`
/// or a blank string; each agent's is independent.
#[tokio::test]
async fn a_console_session_launch_setting_is_left_set_or_cleared_by_an_update() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-console-launch");
    let workdir = console_workdir_in(&dir);
    let mut seeded = console(Agent::Claude, &workdir);
    seeded.launch.claude.model = Some("opus".to_string());
    seeded.launch.grok.model = Some("grok-4".to_string());
    state.store.insert_console(&seeded).unwrap();
    let stored = || state.store.get_console("console-1").unwrap().unwrap();

    handle(&state, None, update_console_request("console-1", |_| {}))
        .await
        .unwrap();
    assert_eq!(stored().launch.claude.model.as_deref(), Some("opus"));

    handle(
        &state,
        None,
        update_console_request("console-1", |request| {
            let RequestBody::UpdateConsole { launch, .. } = request else {
                unreachable!()
            };
            launch.claude.model = Some(Some(" sonnet ".to_string()));
            launch.grok.model = Some(None);
        }),
    )
    .await
    .unwrap();
    assert_eq!(stored().launch.claude.model.as_deref(), Some("sonnet"));
    assert_eq!(stored().launch.grok.model, None);

    handle(
        &state,
        None,
        update_console_request("console-1", |request| {
            let RequestBody::UpdateConsole { launch, .. } = request else {
                unreachable!()
            };
            launch.claude.model = Some(Some("  ".to_string()));
        }),
    )
    .await
    .unwrap();
    assert_eq!(stored().launch.claude.model, None);
}

/// The user's addition to the console session's instructions is left alone when absent, set
/// (trimmed) by a value, and cleared by `null` or a blank string, and the instruction file
/// holds it only while there is some.
#[tokio::test]
async fn a_console_session_prompt_is_left_set_or_cleared_by_an_update() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-console-prompt");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let stored = || {
        state
            .store
            .get_console("console-1")
            .unwrap()
            .unwrap()
            .console_session_prompt
    };
    let file = || std::fs::read_to_string(workdir.join("AGENTS.md")).unwrap();
    let update = |prompt: Option<Option<&str>>| {
        update_console_request("console-1", move |request| {
            let RequestBody::UpdateConsole {
                console_session_prompt,
                ..
            } = request
            else {
                unreachable!()
            };
            *console_session_prompt = prompt.map(|text| text.map(str::to_string));
        })
    };

    handle(&state, None, update(Some(Some("  Be brief.  "))))
        .await
        .unwrap();
    assert_eq!(stored().as_deref(), Some("Be brief."));
    assert_eq!(
        file(),
        session_tools::console_instructions::console_session_instructions(
            Some("Be brief."),
            &session_tools::role::RoleOptions {
                ask_as_choices: true,
                ..Default::default()
            }
        )
    );

    handle(&state, None, update(None)).await.unwrap();
    assert_eq!(stored().as_deref(), Some("Be brief."));
    assert_eq!(
        file(),
        session_tools::console_instructions::console_session_instructions(
            Some("Be brief."),
            &session_tools::role::RoleOptions {
                ask_as_choices: true,
                ..Default::default()
            }
        )
    );

    handle(&state, None, update(Some(None))).await.unwrap();
    assert_eq!(stored(), None);
    assert_eq!(
        file(),
        session_tools::console_instructions::console_session_instructions(
            None,
            &session_tools::role::RoleOptions {
                ask_as_choices: true,
                ..Default::default()
            }
        )
    );

    handle(&state, None, update(Some(Some("Answer in French."))))
        .await
        .unwrap();
    assert_eq!(stored().as_deref(), Some("Answer in French."));
    assert_eq!(
        file(),
        session_tools::console_instructions::console_session_instructions(
            Some("Answer in French."),
            &session_tools::role::RoleOptions {
                ask_as_choices: true,
                ..Default::default()
            }
        )
    );

    handle(&state, None, update(Some(Some("  \n "))))
        .await
        .unwrap();
    assert_eq!(stored(), None);
    assert_eq!(
        file(),
        session_tools::console_instructions::console_session_instructions(
            None,
            &session_tools::role::RoleOptions {
                ask_as_choices: true,
                ..Default::default()
            }
        )
    );
    assert!(!file().contains("Answer in French."));

    // The file follows the language the user's interface is set to at the next write.
    state.store.set_interface_language("ja").unwrap();
    handle(&state, None, update(Some(Some("Be brief."))))
        .await
        .unwrap();
    assert!(file().contains("interface to Japanese (ja)."));
}

/// A name that is blank, and a model or effort the agent does not offer, are refused by the
/// function the control socket and the console session's tools share, with nothing written; a
/// value that is not being changed is not checked, so a stored one the agent has since dropped
/// does not stop an unrelated edit, and an agent whose list could not be read takes anything.
#[tokio::test]
async fn an_update_refuses_a_blank_name_and_a_model_the_agent_does_not_offer() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-validation");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let mut project = Project {
        id: "project-1".to_string(),
        console_id: "console-1".to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
        name: "Project".to_string(),
        path: "/tmp/project-1".to_string(),
        default_agent: None,
        source: ProjectSource::Local,
        remote_url: None,
        accounts: PerAgent {
            claude: ProjectAccount::Inherit,
            codex: ProjectAccount::Inherit,
            grok: ProjectAccount::Inherit,
        },
        launch: PerAgent {
            claude: Launch {
                model: Some("retired".to_string()),
                effort: None,
            },
            codex: Launch {
                model: None,
                effort: None,
            },
            grok: Launch {
                model: None,
                effort: None,
            },
        },
        trust_consent: false,
        pinned: false,
        tags: Vec::new(),
        description: None,
        git_workflow: Default::default(),
    };
    state.store.insert_project(&project).unwrap();
    let refused = |err: anyhow::Error| {
        err.downcast_ref::<CodedError>()
            .expect("a coded error")
            .code
    };
    let change = |edit: fn(&mut ProjectChanges)| {
        let mut changes = ProjectChanges::default();
        edit(&mut changes);
        changes
    };
    let launch = |model: &str, effort: Option<&str>| LaunchChange {
        model: Some(Some(model.to_string())),
        effort: effort.map(|effort| Some(effort.to_string())),
    };

    let err = update_project(
        &state,
        "project-1",
        change(|changes| changes.name = Some("  ".to_string())),
    )
    .await
    .expect_err("blank name");
    assert_eq!(refused(err), error_code::FIELD_REQUIRED);

    let mut changes = ProjectChanges::default();
    changes.launch.insert(Agent::Claude, launch("gpt-9", None));
    let err = update_project(&state, "project-1", changes)
        .await
        .expect_err("model not offered");
    assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);

    let mut changes = ProjectChanges::default();
    changes
        .launch
        .insert(Agent::Claude, launch("opus", Some("extreme")));
    let err = update_project(&state, "project-1", changes)
        .await
        .expect_err("effort not offered");
    assert_eq!(refused(err), error_code::INVALID_LAUNCH_SETTING);
    project = state.store.get_project("project-1").unwrap().unwrap();
    assert_eq!(project.name, "Project");
    assert_eq!(project.launch.claude.model.as_deref(), Some("retired"));

    // The stored model is not on the list, and sending it again is not a change.
    let mut changes = ProjectChanges::default();
    changes
        .launch
        .insert(Agent::Claude, launch("retired", None));
    update_project(&state, "project-1", changes)
        .await
        .expect("an unchanged model is not checked");

    // The stored model is not on the list, and saving something else is fine.
    let renamed = update_project(
        &state,
        "project-1",
        change(|changes| changes.name = Some(" Renamed ".to_string())),
    )
    .await
    .expect("renamed");
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.launch.claude.model.as_deref(), Some("retired"));

    // Nothing to check Codex's model against, so it is taken as given.
    crate::test_support::offer_models(&state, Agent::Codex, &[]);
    let mut changes = ProjectChanges::default();
    changes
        .launch
        .insert(Agent::Codex, launch("anything", None));
    let updated = update_project(&state, "project-1", changes)
        .await
        .expect("accepted");
    assert_eq!(updated.launch.codex.model.as_deref(), Some("anything"));

    let err = update_console(
        &state,
        "console-1",
        ConsoleChanges {
            name: Some(" ".to_string()),
            ..ConsoleChanges::default()
        },
    )
    .await
    .expect_err("blank console name");
    assert_eq!(refused(err), error_code::FIELD_REQUIRED);
}

/// The project session pair is stored at creation, left alone by an absent field, set (trimmed)
/// by a value and cleared by `null`, each agent and value independently.
#[tokio::test]
async fn the_project_session_pair_is_left_set_or_cleared_by_an_update() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-project-pair");
    crate::test_support::offer_models(&state, Agent::Codex, &[("gpt-5", &["high"])]);
    crate::test_support::offer_models(&state, Agent::Claude, &[("opus", &["medium"])]);
    let workdir = console_workdir_in(&dir);
    let mut seeded = console(Agent::Claude, &workdir);
    seeded.project_session_launch.claude.model = Some("opus".to_string());
    seeded.project_session_launch.grok.effort = Some("low".to_string());
    state.store.insert_console(&seeded).unwrap();
    let stored = || state.store.get_console("console-1").unwrap().unwrap();

    handle(&state, None, update_console_request("console-1", |_| {}))
        .await
        .unwrap();
    assert_eq!(
        stored().project_session_launch.claude.model.as_deref(),
        Some("opus")
    );

    handle(
        &state,
        None,
        update_console_request("console-1", |request| {
            let RequestBody::UpdateConsole {
                project_session_launch,
                ..
            } = request
            else {
                unreachable!()
            };
            project_session_launch.claude.model = Some(None);
            project_session_launch.claude.effort = Some(Some("  medium ".to_string()));
            project_session_launch.codex.model = Some(Some("gpt-5".to_string()));
        }),
    )
    .await
    .unwrap();
    let updated = stored();
    assert_eq!(updated.project_session_launch.claude.model, None);
    assert_eq!(
        updated.project_session_launch.claude.effort.as_deref(),
        Some("medium")
    );
    assert_eq!(
        updated.project_session_launch.codex.model.as_deref(),
        Some("gpt-5")
    );
    // The console session pair is a separate setting, and the other agents' values stay.
    assert_eq!(updated.launch.claude.model, None);
    assert_eq!(
        updated.project_session_launch.grok.effort.as_deref(),
        Some("low")
    );
}
