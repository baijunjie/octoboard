use super::*;

/// Each of the user's requests that archives or deletes `s1`, with what it forces, and what it
/// leaves for the test to check.
async fn request_for(fixture: &Fixture, kind: &str, forced: Vec<SessionBranchTarget>) {
    let request = match kind {
        "archive" => RequestBody::ArchiveSession {
            session: "s1".to_string(),
            force_remove: forced,
            keep: Vec::new(),
            keep_process_ids: Vec::new(),
        },
        "delete" => RequestBody::DeleteSession {
            session: "s1".to_string(),
            only_if_unused: false,
            trash_agent_records: false,
            force_remove: forced,
            keep: Vec::new(),
        },
        _ => RequestBody::DeleteArchivedSessions {
            console: "console-1".to_string(),
            project: Some("p1".to_string()),
            console_session: None,
            trash_agent_records: false,
            force_remove: forced,
            keep: Vec::new(),
        },
    };
    crate::coordinator::handle(&fixture.state, None, request)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_users_requests_force_what_they_name_and_report_what_is_left() {
    for kind in ["archive", "delete", "delete-archived"] {
        let fixture = Fixture::new(&format!("cleanup-request-{kind}"));
        let status = if kind == "archive" {
            SessionStatus::Idle
        } else {
            SessionStatus::Archived
        };
        fixture.store("s1", status);
        let forced = fixture.worktree("forced", "s1");
        std::fs::write(forced.join("new.txt"), "untracked\n").unwrap();
        let left = fixture.worktree("left", "s1");
        std::fs::write(left.join("new.txt"), "untracked\n").unwrap();
        let shown = fixture.inspect(&[session("s1", status)]).await;
        let named: Vec<SessionBranchTarget> = shown
            .iter()
            .filter(|item| {
                item.path.as_deref().is_some_and(|path| {
                    path.ends_with(&format!("{}forced", std::path::MAIN_SEPARATOR))
                })
            })
            .map(target)
            .collect();

        let mut events = fixture.state.subscribe();
        request_for(&fixture, kind, named).await;
        let told = reported(&mut events).await;
        assert!(!forced.exists(), "{kind}");
        assert!(left.exists(), "{kind}");
        let names: Vec<&str> = told
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), 1, "{kind}: {told}");
        assert!(
            names[0].ends_with(&format!("{}left", std::path::MAIN_SEPARATOR)),
            "{kind}: {told}"
        );
        assert_eq!(told[0]["state"], "uncommitted_changes");
        assert_eq!(told[0]["uncommitted"], true);
    }
}

#[tokio::test]
async fn deleting_an_unused_session_removes_what_can_go() {
    let fixture = Fixture::new("cleanup-unused");
    let unused = Session {
        has_conversation: false,
        ..session("s1", SessionStatus::Idle)
    };
    fixture.state.store.insert_session(&unused).unwrap();
    branch_made_by(&fixture.state, &fixture.repo, "early", "s1");
    crate::coordinator::handle(
        &fixture.state,
        None,
        RequestBody::DeleteSession {
            session: "s1".to_string(),
            only_if_unused: true,
            trash_agent_records: false,
            force_remove: Vec::new(),
            keep: Vec::new(),
        },
    )
    .await
    .unwrap();
    until(|| !fixture.has_branch("early")).await;
}

#[tokio::test]
async fn removing_a_project_takes_its_outside_worktrees_only_with_its_folder() {
    for trash in [false, true] {
        let fixture = Fixture::new(&format!("cleanup-remove-project-{trash}"));
        fixture.store("s1", SessionStatus::Archived);
        let outside = fixture.worktree("outside", "s1");
        crate::coordinator::handle(
            &fixture.state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: trash,
                force_remove: Vec::new(),
                inspection: None,
            },
        )
        .await
        .unwrap();
        assert!(fixture.state.store.get_project("p1").unwrap().is_none());
        if trash {
            until(|| !outside.exists()).await;
            assert!(!fixture.repo.exists());
        } else {
            // Nothing runs in the background to wait on: the request touches no worktree.
            assert!(outside.exists() && fixture.has_branch("outside"));
        }
    }
}

#[tokio::test]
async fn a_project_removal_that_cannot_find_out_what_goes_along_says_so_and_leaves_it() {
    let fixture = Fixture::new("cleanup-remove-project-unknown");
    fixture.store("s1", SessionStatus::Archived);
    let outside = fixture.worktree("outside", "s1");
    fixture.state.host_tools.replace(crate::host_tools::Tools {
        git: || anyhow::bail!("git is not on the PATH"),
        ..host_tools()
    });
    let mut events = fixture.state.subscribe();
    crate::coordinator::handle(
        &fixture.state,
        None,
        RequestBody::DeleteProject {
            project: "p1".to_string(),
            stop_sessions: true,
            trash_directory: true,
            force_remove: Vec::new(),
            inspection: None,
        },
    )
    .await
    .unwrap();
    assert!(fixture.state.store.get_project("p1").unwrap().is_none());
    let told = loop {
        match events.try_recv() {
            Ok(Event::Error { code, params, .. }) => break (code, params),
            Ok(_) => continue,
            Err(err) => panic!("not told: {err:?}"),
        }
    };
    assert_eq!(told.0, error_code::PROJECT_BRANCHES_NOT_DETERMINED);
    assert!(
        told.1["detail"].contains("git is not on the PATH"),
        "{:?}",
        told.1
    );
    assert!(outside.exists());
}

#[tokio::test]
async fn a_removal_that_would_be_refused_lists_nothing() {
    let fixture = Fixture::new("cleanup-refused");
    let mut inner = fixture.state.store.get_project("p1").unwrap().unwrap();
    inner.id = "p2".to_string();
    inner.path = fixture.repo.join("inner").to_string_lossy().into_owned();
    std::fs::create_dir(fixture.repo.join("inner")).unwrap();
    fixture.state.store.insert_project(&inner).unwrap();
    let err = crate::coordinator::handle(
        &fixture.state,
        None,
        RequestBody::InspectProjectRemoval {
            project: "p1".to_string(),
        },
    )
    .await
    .unwrap_err();
    let coded = err.downcast_ref::<crate::protocol::CodedError>().unwrap();
    assert_eq!(coded.code, error_code::PROJECT_DIRECTORY_SHARED);
}

/// The removal confirmed after an inspection refused over another project's folder that did not
/// respond, given that inspection's token, is refused the same way once the folder responds; one
/// confirmed after an inspection that resolved, which carries no token, goes ahead. Removing the
/// project, or its console, drops what was kept for it.
#[tokio::test]
async fn a_removal_after_an_unresolved_inspection_repeats_its_refusal() {
    let fixture = Fixture::new("cleanup-refused-kept");
    let state = &fixture.state;
    let elsewhere = fixture.worktrees.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let mut other = state.store.get_project("p1").unwrap().unwrap();
    other.id = "p2".to_string();
    other.path = elsewhere.to_string_lossy().into_owned();
    state.store.insert_project(&other).unwrap();
    let elsewhere = &elsewhere;
    let inspect = |stalled: bool| async move {
        let stuck = Arc::new(());
        if stalled {
            state.directory_looks.record(
                crate::hostfs::lexical_key(elsewhere),
                Arc::downgrade(&stuck),
            );
        }
        crate::coordinator::handle(
            state,
            None,
            RequestBody::InspectProjectRemoval {
                project: "p1".to_string(),
            },
        )
        .await
    };
    let delete = |inspection: Option<String>| {
        crate::coordinator::handle(
            state,
            None,
            RequestBody::DeleteProject {
                project: "p1".to_string(),
                stop_sessions: true,
                trash_directory: true,
                force_remove: Vec::new(),
                inspection,
            },
        )
    };
    let coded = |err: anyhow::Error| err.downcast::<crate::protocol::CodedError>().unwrap();

    let inspection = coded(inspect(true).await.unwrap_err());
    assert_eq!(inspection.code, error_code::PROJECT_DIRECTORY_SHARED);
    let token = inspection.params["inspection"].clone();
    let repeated = coded(delete(Some(token)).await.unwrap_err());
    assert_eq!(
        (repeated.code, &repeated.message),
        (inspection.code, &inspection.message)
    );
    assert!(state.store.get_project("p1").unwrap().is_some());
    assert_eq!(state.unresolved_removals.len(), 0, "the token is used up");

    coded(inspect(true).await.unwrap_err());
    assert!(matches!(
        inspect(false).await.unwrap(),
        Some(Event::ProjectRemovalInspected { .. })
    ));
    delete(None).await.unwrap();
    assert!(state.store.get_project("p1").unwrap().is_none());
    assert_eq!(
        state.unresolved_removals.len(),
        0,
        "dropped with the project"
    );

    let again = fixture.worktrees.join("again");
    std::fs::create_dir_all(&again).unwrap();
    insert_project(state, &again);
    coded(inspect(true).await.unwrap_err());
    assert_eq!(state.unresolved_removals.len(), 1);
    crate::coordinator::handle(
        state,
        None,
        RequestBody::DeleteConsole {
            console: "console-1".to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        state.unresolved_removals.len(),
        0,
        "dropped with the console"
    );
}
