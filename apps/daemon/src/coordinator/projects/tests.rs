use super::*;
use crate::coordinator::handle;
use crate::coordinator::test_support::{
    bound_session, console, console_workdir_in, lead_session, pins, project_of, status_of,
};
use crate::hostfs;
use crate::protocol::{
    error_code, Account, Agent, Availability, Change, CodedError, Event, Launch, OrchestrationRole,
    Origin, PerAgent, Project, ProjectAccount, ProjectSource, RequestBody, Role, Session,
    SessionStatus,
};
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    console_session, console_with_project_and_accounts, idle_stand_in, session_ids,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A project's deletion can be told to stop its sessions, but never goes ahead under a session
/// that is being launched; with nothing in flight it removes the project and its sessions.
#[tokio::test]
async fn deleting_a_project_with_stop_sessions_refuses_a_launch_in_flight() {
    let (state, dir) = crate::test_support::app_state("coordinator-delete-project-stop");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    state
        .store
        .insert_project(&Project {
            id: "project-1".to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: "Project".to_string(),
            path: "/tmp/project-1".to_string(),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            accounts: Default::default(),
            launch: Default::default(),
            trust_consent: false,
            pinned: false,
            tags: Vec::new(),
            description: None,
            git_workflow: Default::default(),
        })
        .unwrap();
    state
        .store
        .insert_session(&Session {
            id: "project".to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: Some("project-1".to_string()),
            host_id: LOCAL_HOST_ID.to_string(),
            role: Role::Project,
            origin: Origin::User,
            title: "Project".to_string(),
            status: SessionStatus::Archived,
            terminal_title: None,
            record_titles: Default::default(),
            background_tasks: Vec::new(),
            switching_account: false,
            paused: false,
            prompt_on_launch: false,
            default_effort: None,
            orchestration_role: OrchestrationRole::Coordinator,
            context: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: false,
            bound_to: None,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            locked: false,
            started_at: 0,
            ended_at: None,
        })
        .unwrap();
    let delete = || RequestBody::DeleteProject {
        project: "project-1".to_string(),
        stop_sessions: true,
        trash_directory: false,
        force_remove: Vec::new(),
        inspection: None,
    };

    let claim = state.begin_launch("project").unwrap();
    let refused = handle(&state, None, delete()).await.expect_err("launching");
    let coded = refused.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::PROJECT_HAS_RUNNING_SESSIONS);
    drop(claim);

    handle(&state, None, delete()).await.unwrap();
    assert!(state.store.get_project("project-1").unwrap().is_none());
    assert!(state.store.get_session("project").unwrap().is_none());
}

/// A clone reads nothing, and a directory source reads only the directories not yet
/// associated, keeping the URLs the reader finds.
#[tokio::test]
async fn remote_urls_are_read_for_unassociated_directories_of_the_directory_sources_only() {
    let (state, dir) = crate::test_support::app_state("coordinator-directory-remote-urls");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let (taken, fresh, bare) = (dir.join("taken"), dir.join("fresh"), dir.join("bare"));
    let mut associated = project_of("console-1", "taken");
    associated.path = taken.to_string_lossy().into_owned();
    state.store.insert_project(&associated).unwrap();
    let directories: Vec<_> = [&taken, &fresh, &bare]
        .into_iter()
        .map(|path| hostfs::identity::Resolved::of(path.clone()))
        .collect();

    let reads = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let reader = |reads: &std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>| {
        let reads = reads.clone();
        move |path: &Path| {
            reads.lock().unwrap().push(path.to_path_buf());
            path.ends_with("fresh")
                .then(|| "https://example.com/f.git".to_string())
        }
    };

    let associated = associated_directories(&state, "console-1").await.unwrap();
    let urls = directory_remote_urls(
        &associated,
        ProjectSource::Git,
        &directories,
        reader(&reads),
    )
    .await
    .unwrap();
    assert!(urls.is_empty() && reads.lock().unwrap().is_empty());

    for source in [ProjectSource::Local, ProjectSource::Parent] {
        reads.lock().unwrap().clear();
        let urls = directory_remote_urls(&associated, source, &directories, reader(&reads))
            .await
            .unwrap();
        let mut read = reads.lock().unwrap().clone();
        read.sort();
        assert_eq!(read, vec![bare.clone(), fresh.clone()]);
        assert_eq!(
            urls,
            HashMap::from([(fresh.clone(), "https://example.com/f.git".to_string())])
        );
    }
}

/// An explicit default agent is kept as given; an absent one takes the one the directory is
/// set up for, unless that agent is determined unavailable or the client asked for no
/// detection.
#[tokio::test]
async fn add_project_detects_the_agent_only_when_none_is_given() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-detect");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let add = |name: &str, default_agent: Option<Agent>, detect_default_agent: bool| {
        let repo = dir.join(name);
        std::fs::create_dir(&repo).expect("repo");
        std::fs::write(repo.join("AGENTS.md"), "").expect("marker");
        add_project(
            &state,
            AddProjectRequest {
                console_id: "console-1".to_string(),
                source: ProjectSource::Local,
                path: Some(repo.to_string_lossy().into_owned()),
                remote_url: None,
                name: None,
                default_agent,
                settings: NewProjectSettings::default(),
                detect_default_agent,
                tags: None,
                description: None,
                check_launch: true,
                check_text_limits: true,
            },
        )
    };

    assert_eq!(
        add("auto", None, true).await.unwrap()[0].default_agent,
        Some(Agent::Codex)
    );
    assert_eq!(
        add("explicit", Some(Agent::Grok), true).await.unwrap()[0].default_agent,
        Some(Agent::Grok)
    );
    // A client that has shown the detection and was told to keep "inherit" gets none stored.
    assert_eq!(
        add("kept-auto", None, false).await.unwrap()[0].default_agent,
        None
    );
    state.set_agent_availability(vec![crate::protocol::AgentAvailability {
        agent: Agent::Codex,
        availability: Availability::Unavailable,
        default_account_dir: None,
    }]);
    assert_eq!(
        add("missing", None, true).await.unwrap()[0].default_agent,
        None
    );
}

/// A description is the user's words about one project, so a parent directory that adds
/// several takes none of it, as it takes none of a name; a single project keeps it trimmed.
#[tokio::test]
async fn add_project_applies_a_description_to_a_single_project_only() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-description");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let add = |source: ProjectSource, path: &Path| {
        add_project(
            &state,
            AddProjectRequest {
                console_id: "console-1".to_string(),
                source,
                path: Some(path.to_string_lossy().into_owned()),
                remote_url: None,
                name: None,
                default_agent: None,
                settings: NewProjectSettings::default(),
                detect_default_agent: false,
                tags: None,
                description: Some("  Billing.  ".to_string()),
                check_launch: true,
                check_text_limits: true,
            },
        )
    };

    let single = dir.join("single");
    std::fs::create_dir(&single).unwrap();
    let added = add(ProjectSource::Local, &single).await.unwrap();
    assert_eq!(added[0].description.as_deref(), Some("Billing."));

    let parent = dir.join("parent");
    for name in ["one", "two"] {
        std::fs::create_dir_all(parent.join(name).join(".git")).unwrap();
    }
    let added = add(ProjectSource::Parent, &parent).await.unwrap();
    assert_eq!(added.len(), 2);
    assert!(added.iter().all(|project| project.description.is_none()));
}

/// A directory already associated is refused when written through a link or, where the file
/// system folds case, in another case; another console's project does not count.
#[tokio::test]
async fn add_project_refuses_a_directory_spelled_another_way() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-spellings");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let mut other = console(Agent::Claude, &workdir);
    other.id = "console-2".to_string();
    other.name = "Other".to_string();
    state.store.insert_console(&other).unwrap();
    let add = |console_id: &str, path: &Path| {
        add_project(
            &state,
            AddProjectRequest {
                console_id: console_id.to_string(),
                source: ProjectSource::Local,
                path: Some(path.to_string_lossy().into_owned()),
                remote_url: None,
                name: None,
                default_agent: None,
                settings: NewProjectSettings::default(),
                detect_default_agent: false,
                tags: None,
                description: None,
                check_launch: true,
                check_text_limits: true,
            },
        )
    };
    let is_already_added = |err: anyhow::Error| {
        err.downcast_ref::<CodedError>().map(|coded| coded.code)
            == Some(error_code::ALL_PROJECTS_ALREADY_ADDED)
    };

    let real = dir.join("Real");
    std::fs::create_dir(&real).unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    add("console-1", &link).await.unwrap();
    assert!(is_already_added(add("console-1", &real).await.unwrap_err()));
    // The path is stored as it was given.
    assert_eq!(
        state.store.project_paths("console-1").unwrap(),
        vec![link.to_string_lossy().into_owned()]
    );
    assert_eq!(add("console-2", &real).await.unwrap().len(), 1);
}

/// Two directories of one parent that are the same directory become one project.
#[tokio::test]
async fn add_project_associates_a_linked_repository_of_a_parent_once() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-parent-link");
    state
        .store
        .insert_console(&console(Agent::Claude, &console_workdir_in(&dir)))
        .unwrap();
    let parent = dir.join("parent");
    std::fs::create_dir_all(parent.join("repo").join(".git")).unwrap();
    std::os::unix::fs::symlink(parent.join("repo"), parent.join("repo-link")).unwrap();

    let added = add_project(
        &state,
        AddProjectRequest {
            console_id: "console-1".to_string(),
            source: ProjectSource::Parent,
            path: Some(parent.to_string_lossy().into_owned()),
            remote_url: None,
            name: None,
            default_agent: None,
            settings: NewProjectSettings::default(),
            detect_default_agent: false,
            tags: None,
            description: None,
            check_launch: true,
            check_text_limits: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(added.len(), 1);
}

/// Paths whose look at the file system outlasts the deadline are compared by their text, and
/// while that look is stuck no second thread is started for the console.
#[tokio::test]
async fn a_stalled_look_at_the_file_system_falls_back_to_the_paths_text() {
    let (state, _dir) = crate::test_support::app_state("coordinator-stalled-identities");
    let (release, stuck) = std::sync::mpsc::channel::<()>();
    let paths = vec!["/work/project".to_string()];
    let identities = identities_within(
        &state,
        "console-1",
        paths.clone(),
        Duration::from_millis(50),
        move |_| {
            stuck.recv().ok();
            hostfs::identity::DirectoryIdentities::default()
        },
    )
    .await
    .unwrap();
    assert!(identities.contains(Path::new("/work/./project/")));
    assert!(!identities.contains(Path::new("/work/other")));
    assert!(state.stalled_directory_passes.is_stuck("console-1"));
    assert!(!state.stalled_directory_passes.is_stuck("console-2"));

    // A second call neither starts its build nor waits out a deadline.
    let identities = identities_within(&state, "console-1", paths, Duration::from_secs(60), |_| {
        panic!("a second look was started while one is stuck")
    })
    .await
    .unwrap();
    assert!(identities.contains(Path::new("/work/project")));

    // Once the stuck thread ends the console is looked at again.
    release.send(()).unwrap();
    let ended = tokio::time::timeout(crate::test_support::PATIENCE, async {
        while state.stalled_directory_passes.is_stuck("console-1") {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(ended.is_ok(), "the stalled pass was never cleared");
}

#[tokio::test]
async fn adding_project_text_checks_trimmed_character_limits_before_storing_or_cloning() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-text-limits");
    state
        .store
        .insert_console(&console(Agent::Claude, &console_workdir_in(&dir)))
        .unwrap();
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    let request = |name, description| AddProjectRequest {
        console_id: "console-1".to_string(),
        source: ProjectSource::Local,
        path: Some(repo.to_string_lossy().into_owned()),
        remote_url: None,
        name: Some(name),
        default_agent: None,
        settings: NewProjectSettings::default(),
        detect_default_agent: false,
        tags: None,
        description: Some(description),
        check_launch: true,
        check_text_limits: true,
    };
    for (field, name, description, limit) in [
        ("name", "界".repeat(101), "ok".to_string(), "100"),
        ("description", "ok".to_string(), "😀".repeat(2_001), "2000"),
    ] {
        for source in [ProjectSource::Local, ProjectSource::Git] {
            let mut given = request(name.clone(), description.clone());
            given.source = source;
            given.remote_url = Some("invalid-remote-that-must-not-be-cloned".to_string());
            let err = add_project(&state, given).await.unwrap_err();
            let coded = err.downcast_ref::<CodedError>().expect("coded refusal");
            assert_eq!(coded.code, error_code::FIELD_TOO_LONG);
            assert_eq!(coded.params["field"], field);
            assert_eq!(coded.params["limit"], limit);
            assert!(state.store.list_projects().unwrap().is_empty());
        }
    }
    let name = "界".repeat(100);
    let description = "😀".repeat(2_000);
    let added = add_project(
        &state,
        request(format!("  {name}  "), format!("\n{description}\n")),
    )
    .await
    .unwrap();
    assert_eq!(added[0].name, name);
    assert_eq!(added[0].description.as_deref(), Some(description.as_str()));
}

#[tokio::test]
async fn derived_names_and_ignored_parent_text_do_not_block_association() {
    let (state, dir) = crate::test_support::app_state("coordinator-derived-project-text");
    state
        .store
        .insert_console(&console(Agent::Claude, &console_workdir_in(&dir)))
        .unwrap();
    let long_name = "a".repeat(101);
    let repo = dir.join(&long_name);
    std::fs::create_dir(&repo).unwrap();
    let request = AddProjectRequest {
        console_id: "console-1".to_string(),
        source: ProjectSource::Local,
        path: Some(repo.to_string_lossy().into_owned()),
        remote_url: None,
        name: None,
        default_agent: None,
        settings: NewProjectSettings::default(),
        detect_default_agent: false,
        tags: None,
        description: None,
        check_launch: true,
        check_text_limits: true,
    };
    assert_eq!(
        add_project(&state, request).await.unwrap()[0].name,
        long_name
    );
    let parent = dir.join("parent");
    std::fs::create_dir_all(parent.join("one").join(".git")).unwrap();
    let request = || AddProjectRequest {
        console_id: "console-1".to_string(),
        source: ProjectSource::Parent,
        path: Some(parent.to_string_lossy().into_owned()),
        remote_url: None,
        name: Some(long_name.clone()),
        default_agent: None,
        settings: NewProjectSettings::default(),
        detect_default_agent: false,
        tags: None,
        description: Some("x".repeat(2_001)),
        check_launch: true,
        check_text_limits: true,
    };
    let err = add_project(&state, request()).await.unwrap_err();
    assert_eq!(
        err.downcast_ref::<CodedError>().unwrap().code,
        error_code::FIELD_TOO_LONG
    );
    std::fs::create_dir_all(parent.join("two").join(".git")).unwrap();
    let added = add_project(&state, request()).await.unwrap();
    assert_eq!(added.len(), 2);
    assert!(added
        .iter()
        .all(|project| project.name != long_name && project.description.is_none()));
}

#[tokio::test]
async fn project_text_updates_preserve_legacy_values_and_reject_new_overlong_text() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-project-text-limits");
    state
        .store
        .insert_console(&console(Agent::Claude, &console_workdir_in(&dir)))
        .unwrap();
    let mut legacy = project_of("console-1", "project-1");
    legacy.name = format!("  {}  ", "n".repeat(101));
    legacy.description = Some(format!("\n{}\n", "d".repeat(2_001)));
    state.store.insert_project(&legacy).unwrap();
    let updated = update_project(
        &state,
        &legacy.id,
        ProjectChanges {
            pinned: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.name, legacy.name);
    assert_eq!(updated.description, legacy.description);
    let updated = update_project(
        &state,
        &legacy.id,
        ProjectChanges {
            name: Some(legacy.name.clone()),
            description: Some(legacy.description.clone()),
            pinned: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(!updated.pinned);
    assert_eq!(updated.name, legacy.name);
    assert_eq!(updated.description, legacy.description);
    for (field, changes) in [
        (
            "name",
            ProjectChanges {
                name: Some("m".repeat(101)),
                pinned: Some(true),
                ..Default::default()
            },
        ),
        (
            "description",
            ProjectChanges {
                description: Some(Some("e".repeat(2_001))),
                pinned: Some(true),
                ..Default::default()
            },
        ),
    ] {
        let err = update_project(&state, &legacy.id, changes)
            .await
            .unwrap_err();
        let coded = err.downcast_ref::<CodedError>().expect("coded refusal");
        assert_eq!(coded.code, error_code::FIELD_TOO_LONG);
        assert_eq!(coded.params["field"], field);
        let stored = state.store.get_project(&legacy.id).unwrap().unwrap();
        assert_eq!(stored.name, legacy.name);
        assert_eq!(stored.description, legacy.description);
        assert!(!stored.pinned);
    }
    let name = "😀".repeat(100);
    let description = "界".repeat(2_000);
    let updated = update_project(
        &state,
        &legacy.id,
        ProjectChanges {
            name: Some(format!(" {name} ")),
            description: Some(Some(format!("\n{description}\n"))),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.name, name);
    assert_eq!(updated.description.as_deref(), Some(description.as_str()));
}

/// A project added with its own git workflow values stores them, the items it leaves out
/// following the application-wide value, and a parent directory gives the same values to
/// every project it adds.
#[tokio::test]
async fn add_project_stores_its_own_git_workflow_values_for_every_project_added() {
    use crate::git_workflow::{Delivery, Development, GitWorkflowOverride};
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-git-workflow");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let own = GitWorkflowOverride {
        development: Some(Development::InPlace),
        delivery: Some(Delivery::PullRequest),
        ..GitWorkflowOverride::default()
    };
    let add = |source: &str, path: &Path| {
        let path = serde_json::to_string(&path.to_string_lossy()).unwrap();
        let request = serde_json::from_str::<crate::protocol::Request>(&format!(
            r#"{{"type":"add_project","console_id":"console-1","source":"{source}","path":{path},
                    "git_workflow":{{"development":"in_place","delivery":"pull_request"}}}}"#
        ))
        .unwrap();
        handle(&state, None, request.body)
    };

    let single = dir.join("single");
    std::fs::create_dir(&single).unwrap();
    add("local", &single).await.unwrap();
    let added = state.store.list_projects().unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].git_workflow, own);

    let parent = dir.join("parent");
    for name in ["one", "two"] {
        std::fs::create_dir_all(parent.join(name).join(".git")).unwrap();
    }
    add("parent", &parent).await.unwrap();
    let added = state.store.list_projects().unwrap();
    assert_eq!(added.len(), 3);
    assert!(added.iter().all(|project| project.git_workflow == own));
}

/// `update_project` sets a project's own git workflow items on a value, leaves the override
/// alone when `git_workflow` is absent or empty (an unrelated change included), and goes back
/// to following the application-wide value on `null` for an item; the first change broadcasts
/// the project.
#[tokio::test]
async fn a_project_git_workflow_override_is_left_set_or_cleared_by_an_update() {
    use crate::git_workflow::{Delivery, Development, GitWorkflowOverride};
    let (state, dir) = crate::test_support::app_state("coordinator-update-project-git-workflow");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    state
        .store
        .insert_project(&project_of("console-1", "project-1"))
        .unwrap();
    let stored = || {
        state
            .store
            .get_project("project-1")
            .unwrap()
            .unwrap()
            .git_workflow
    };
    let update = |json: &str| {
        serde_json::from_str::<crate::protocol::Request>(&format!(
            r#"{{"type":"update_project","project":"project-1",{json}}}"#
        ))
        .unwrap()
        .body
    };
    let mut events = state.subscribe();

    handle(
        &state,
        None,
        update(r#""git_workflow":{"development":"in_place","delivery":"pull_request"}"#),
    )
    .await
    .unwrap();
    let expected = GitWorkflowOverride {
        development: Some(Development::InPlace),
        squash_before_delivery: None,
        rebase_before_merge: None,
        delivery: Some(Delivery::PullRequest),
        revert_gate: None,
    };
    assert_eq!(stored(), expected);
    assert!(matches!(
        events.try_recv(),
        Ok(Event::ProjectUpserted { project }) if project.git_workflow == expected
    ));

    handle(&state, None, update(r#""pinned":true"#))
        .await
        .unwrap();
    assert!(
        state
            .store
            .get_project("project-1")
            .unwrap()
            .unwrap()
            .pinned
    );
    assert_eq!(
        stored(),
        expected,
        "an unrelated change leaves the override"
    );
    handle(&state, None, update(r#""git_workflow":{}"#))
        .await
        .unwrap();
    assert_eq!(stored(), expected);

    handle(
        &state,
        None,
        update(r#""git_workflow":{"delivery":null,"revert_gate":true}"#),
    )
    .await
    .unwrap();
    assert_eq!(
        stored(),
        GitWorkflowOverride {
            development: Some(Development::InPlace),
            revert_gate: Some(true),
            ..GitWorkflowOverride::default()
        }
    );

    assert!(serde_json::from_str::<crate::protocol::Request>(
        r#"{"type":"update_project","project":"p","git_workflow":{"delivery":"squash"}}"#
    )
    .is_err());
}

/// A project's description is left alone by an update that omits it, stored trimmed when
/// given, and cleared by `null` or a blank string.
#[tokio::test]
async fn a_project_description_is_left_set_or_cleared_by_an_update() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-project-description");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    state
        .store
        .insert_project(&project_of("console-1", "project-1"))
        .unwrap();
    let stored = || {
        state
            .store
            .get_project("project-1")
            .unwrap()
            .unwrap()
            .description
    };
    let update = |description: Option<Option<&str>>| {
        update_project_request("project-1", move |request| {
            let RequestBody::UpdateProject {
                description: slot, ..
            } = request
            else {
                unreachable!()
            };
            *slot = description.map(|text| text.map(str::to_string));
        })
    };

    handle(&state, None, update(Some(Some("  Billing.  "))))
        .await
        .unwrap();
    assert_eq!(stored().as_deref(), Some("Billing."));

    handle(&state, None, update(None)).await.unwrap();
    assert_eq!(stored().as_deref(), Some("Billing."));

    handle(&state, None, update(Some(None))).await.unwrap();
    assert_eq!(stored(), None);

    handle(&state, None, update(Some(Some("Billing."))))
        .await
        .unwrap();
    handle(&state, None, update(Some(Some(" \n "))))
        .await
        .unwrap();
    assert_eq!(stored(), None);
}

/// An `update_project` request that changes nothing, then `edit` applied to it.
fn update_project_request(project: &str, edit: impl FnOnce(&mut RequestBody)) -> RequestBody {
    let mut request = RequestBody::UpdateProject {
        project: project.to_string(),
        name: None,
        default_agent: None,
        accounts: Default::default(),
        launch: Default::default(),
        pinned: None,
        tags: None,
        description: None,
        git_workflow: None,
    };
    edit(&mut request);
    request
}

/// A project's model and effort are left alone when absent, set (trimmed) by a value, and
/// cleared by `null` or a blank string; each agent's is independent.
#[tokio::test]
async fn a_project_launch_setting_is_left_set_or_cleared_by_an_update() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-project-launch");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let mut seeded = project_of("console-1", "project-1");
    seeded.launch.claude.model = Some("opus".to_string());
    seeded.launch.grok.effort = Some("high".to_string());
    seeded.launch.codex.model = Some("gpt-5".to_string());
    state.store.insert_project(&seeded).unwrap();
    let stored = || state.store.get_project("project-1").unwrap().unwrap();

    handle(&state, None, update_project_request("project-1", |_| {}))
        .await
        .unwrap();
    assert_eq!(stored().launch.claude.model.as_deref(), Some("opus"));

    handle(
        &state,
        None,
        update_project_request("project-1", |request| {
            let RequestBody::UpdateProject { launch, .. } = request else {
                unreachable!()
            };
            launch.claude.model = Some(Some(" sonnet ".to_string()));
            launch.claude.effort = Some(Some("low".to_string()));
            launch.grok.effort = Some(None);
            launch.codex.model = Some(Some("  ".to_string()));
        }),
    )
    .await
    .unwrap();
    let updated = stored();
    assert_eq!(updated.launch.claude.model.as_deref(), Some("sonnet"));
    assert_eq!(updated.launch.claude.effort.as_deref(), Some("low"));
    assert_eq!(updated.launch.grok.effort, None);
    assert_eq!(updated.launch.codex.model, None);
}

/// A project's account reference is checked against the agent it is set for, set or cleared by
/// an update, and left alone when absent; a refusal changes nothing.
#[tokio::test]
async fn a_project_account_reference_is_checked_set_and_cleared_by_an_update() {
    let (state, dir) = crate::test_support::app_state("coordinator-update-project-account");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    state
        .store
        .insert_project(&project_of("console-1", "project-1"))
        .unwrap();
    for (id, agent) in [("claude-acct", Agent::Claude), ("codex-acct", Agent::Codex)] {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent,
                name: id.to_string(),
                config_dir: format!("/home/u/.{id}"),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
    }
    let stored = || state.store.get_project("project-1").unwrap().unwrap();

    handle(
        &state,
        None,
        update_project_request("project-1", |request| {
            let RequestBody::UpdateProject { accounts, .. } = request else {
                unreachable!()
            };
            accounts.claude = Change::Set(pins("claude-acct"));
        }),
    )
    .await
    .unwrap();
    assert_eq!(stored().accounts.claude, pins("claude-acct"));

    // An account of another agent is refused, and the rest of the same request with it.
    let err = handle(
        &state,
        None,
        update_project_request("project-1", |request| {
            let RequestBody::UpdateProject { name, accounts, .. } = request else {
                unreachable!()
            };
            *name = Some("Renamed".to_string());
            accounts.grok = Change::Set(pins("codex-acct"));
        }),
    )
    .await
    .expect_err("refused");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
    assert_eq!(stored().name, "project-1");
    assert_eq!(stored().accounts.grok, ProjectAccount::Inherit);

    handle(&state, None, update_project_request("project-1", |_| {}))
        .await
        .unwrap();
    assert_eq!(stored().accounts.claude, pins("claude-acct"));

    // The agent's default account is stored as such, and needs no account to exist.
    handle(
        &state,
        None,
        update_project_request("project-1", |request| {
            let RequestBody::UpdateProject { accounts, .. } = request else {
                unreachable!()
            };
            accounts.claude = Change::Set(ProjectAccount::Default);
        }),
    )
    .await
    .unwrap();
    assert_eq!(stored().accounts.claude, ProjectAccount::Default);
    // Left alone by a request that does not mention it, then named again from the default.
    handle(&state, None, update_project_request("project-1", |_| {}))
        .await
        .unwrap();
    assert_eq!(stored().accounts.claude, ProjectAccount::Default);
    handle(
        &state,
        None,
        update_project_request("project-1", |request| {
            let RequestBody::UpdateProject { accounts, .. } = request else {
                unreachable!()
            };
            accounts.claude = Change::Set(pins("claude-acct"));
        }),
    )
    .await
    .unwrap();
    assert_eq!(stored().accounts.claude, pins("claude-acct"));

    handle(
        &state,
        None,
        update_project_request("project-1", |request| {
            let RequestBody::UpdateProject { accounts, .. } = request else {
                unreachable!()
            };
            accounts.claude = Change::Clear;
        }),
    )
    .await
    .unwrap();
    assert_eq!(stored().accounts.claude, ProjectAccount::Inherit);

    // An explicit inherit is the same as null, from the default as from an account.
    for before in [ProjectAccount::Default, pins("claude-acct")] {
        let mut project = stored();
        project.accounts.claude = before;
        state.store.update_project(&project).unwrap();
        handle(
            &state,
            None,
            update_project_request("project-1", |request| {
                let RequestBody::UpdateProject { accounts, .. } = request else {
                    unreachable!()
                };
                accounts.claude = Change::Set(ProjectAccount::Inherit);
            }),
        )
        .await
        .unwrap();
        assert_eq!(stored().accounts.claude, ProjectAccount::Inherit);
    }
}

/// A project is added with the settings it was given, a model or effort trimmed and a blank
/// one read as unset, and an account of another agent is refused before anything is stored.
#[tokio::test]
async fn adding_a_project_stores_its_settings_and_refuses_a_mismatched_account() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-settings");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    state
        .store
        .insert_account(&Account {
            id: "codex-acct".to_string(),
            agent: Agent::Codex,
            name: "Codex".to_string(),
            config_dir: "/home/u/.codex-alt".to_string(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        })
        .unwrap();
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).expect("repo");
    let request = |settings: NewProjectSettings| AddProjectRequest {
        console_id: "console-1".to_string(),
        source: ProjectSource::Local,
        path: Some(repo.to_string_lossy().into_owned()),
        remote_url: None,
        name: None,
        default_agent: None,
        settings,
        detect_default_agent: false,
        tags: None,
        description: None,
        check_launch: true,
        check_text_limits: true,
    };
    // What Grok Build offers, so the effort below is checked against this and not against
    // whatever is installed on the machine running the test.
    crate::test_support::offer_models(&state, Agent::Grok, &[("grok-4", &["low", "high"])]);

    let err = add_project(
        &state,
        request(NewProjectSettings {
            accounts: PerAgent {
                claude: pins("codex-acct"),
                ..Default::default()
            },
            ..NewProjectSettings::default()
        }),
    )
    .await
    .expect_err("refused");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::UNKNOWN_ACCOUNT);
    assert!(state.store.list_projects().unwrap().is_empty());

    let added = add_project(
        &state,
        request(NewProjectSettings {
            accounts: PerAgent {
                codex: pins("codex-acct"),
                grok: ProjectAccount::Default,
                ..Default::default()
            },
            launch: PerAgent {
                claude: Launch {
                    model: Some(" opus ".to_string()),
                    effort: Some("  ".to_string()),
                },
                grok: Launch {
                    model: None,
                    effort: Some("high".to_string()),
                },
                ..Default::default()
            },
            ..NewProjectSettings::default()
        }),
    )
    .await
    .expect("added")
    .remove(0);
    let stored = state.store.get_project(&added.id).unwrap().unwrap();
    assert_eq!(stored.accounts.codex, pins("codex-acct"));
    assert_eq!(stored.accounts.grok, ProjectAccount::Default);
    assert_eq!(stored.accounts.claude, ProjectAccount::Inherit);
    assert_eq!(stored.launch.claude.model.as_deref(), Some("opus"));
    assert_eq!(stored.launch.claude.effort, None);
    assert_eq!(stored.launch.grok.effort.as_deref(), Some("high"));
}

/// Removing a project that holds a running lead session and its running sessions stops them
/// all, the lead session's own first, and leaves the console session it is bound to running.
#[tokio::test]
async fn removing_a_project_with_a_lead_session_leaves_its_console_session_alone() {
    use SessionStatus::{Idle, Working};
    let (state, _dir) = console_with_project_and_accounts("coordinator-delete-project-lead");
    for session in [
        console_session("hub", Idle),
        lead_session("lead", Idle, "hub"),
        bound_session("worker", Working, "lead"),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let live: Vec<_> = ["hub", "lead", "worker"]
        .into_iter()
        .map(|id| {
            let stand_in = idle_stand_in(id);
            state.register_live(stand_in.clone());
            stand_in
        })
        .collect();

    handle(
        &state,
        None,
        RequestBody::DeleteProject {
            project: "project-1".to_string(),
            stop_sessions: true,
            trash_directory: false,
            force_remove: Vec::new(),
            inspection: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(session_ids(&state), ["hub"]);
    assert_eq!(status_of(&state, "hub"), Some(Idle));
    assert!(!live[0].poll_exit(), "the console session keeps running");
}

/// A project at a path its console already has is refused by the store as
/// `all_projects_already_added` rather than as an internal error, and `insert_unless_associated`
/// turns that into a skip, while another console's project at the path does not count.
#[tokio::test]
async fn a_project_at_an_associated_path_is_refused_by_the_store_and_skipped() {
    let (state, dir) = crate::test_support::app_state("coordinator-insert-unless-associated");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let mut other = console(Agent::Claude, &workdir);
    other.id = "console-2".to_string();
    other.name = "Other".to_string();
    state.store.insert_console(&other).unwrap();
    let first = project_of("console-1", "first");
    assert!(insert_unless_associated(&state, &first).unwrap());

    let mut again = project_of("console-1", "again");
    again.path = first.path.clone();
    let refused = anyhow::Error::from(state.store.insert_project(&again).unwrap_err());
    assert_eq!(
        refused.downcast_ref::<CodedError>().map(|coded| coded.code),
        Some(error_code::ALL_PROJECTS_ALREADY_ADDED)
    );
    assert!(!insert_unless_associated(&state, &again).unwrap());
    assert_eq!(state.store.project_paths("console-1").unwrap().len(), 1);

    let mut elsewhere = project_of("console-2", "elsewhere");
    elsewhere.path = first.path.clone();
    assert!(insert_unless_associated(&state, &elsewhere).unwrap());
}

/// A second association for a console waits for the first one's claim to drop, while another
/// console's goes ahead.
#[tokio::test]
async fn associating_projects_with_one_console_is_serialised() {
    let (state, _dir) = crate::test_support::app_state("coordinator-project-association-claim");
    let first = state.claim_project_association("console-1").await;

    let waiting = {
        let state = state.clone();
        tokio::spawn(async move { drop(state.claim_project_association("console-1").await) })
    };
    drop(state.claim_project_association("console-2").await);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !waiting.is_finished(),
        "the second claim for the console waits"
    );

    drop(first);
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("the claim is granted once released")
        .unwrap();
}

async fn add_local(state: Arc<AppState>, path: PathBuf) -> Result<Vec<Project>> {
    add_project(
        &state,
        AddProjectRequest {
            console_id: "console-1".to_string(),
            source: ProjectSource::Local,
            path: Some(path.to_string_lossy().into_owned()),
            remote_url: None,
            name: None,
            default_agent: None,
            settings: NewProjectSettings::default(),
            detect_default_agent: false,
            tags: None,
            description: None,
            check_launch: true,
            check_text_limits: true,
        },
    )
    .await
}

/// An association does not store anything while another holds the console's claim, and goes
/// ahead once it is released.
#[tokio::test]
async fn add_project_waits_for_the_consoles_claim_before_storing() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-claim-wait");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let target = dir.join("target");
    std::fs::create_dir(&target).unwrap();

    let claim = state.claim_project_association("console-1").await;
    let adding = tokio::spawn(add_local(state.clone(), target));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!adding.is_finished(), "the association waits for the claim");
    assert!(state.store.project_paths("console-1").unwrap().is_empty());

    drop(claim);
    let added = tokio::time::timeout(Duration::from_secs(5), adding)
        .await
        .expect("the association goes ahead once the claim is released")
        .unwrap()
        .unwrap();
    assert_eq!(added.len(), 1);
}

/// Two overlapping associations of one directory store one project: the second is refused as
/// already added.
#[tokio::test]
async fn two_concurrent_add_projects_for_one_directory_store_one_project() {
    let (state, dir) = crate::test_support::app_state("coordinator-add-project-concurrent");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(Agent::Claude, &workdir))
        .unwrap();
    let target = dir.join("target");
    std::fs::create_dir(&target).unwrap();

    let (first, second) = tokio::join!(
        add_local(state.clone(), target.clone()),
        add_local(state.clone(), target)
    );

    let results = [first, second];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let refused = results
        .into_iter()
        .find_map(Result::err)
        .expect("one is refused");
    assert_eq!(
        refused.downcast_ref::<CodedError>().map(|coded| coded.code),
        Some(error_code::ALL_PROJECTS_ALREADY_ADDED)
    );
    assert_eq!(state.store.project_paths("console-1").unwrap().len(), 1);
}
