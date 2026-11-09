use crate::protocol::{
    AgentId, Change, Launch, PerAgent, ProcessTarget, ProjectAccount, ProjectSource, Request,
    RequestBody, Session, SessionBranchState,
};

#[test]
fn an_absent_include_paths_means_true() {
    let json = r#"{"type":"export_configuration"}"#;
    match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::ExportConfiguration { include_paths, .. } => assert!(include_paths),
        _ => panic!("not export_configuration"),
    }
}

#[test]
fn an_absent_apply_consoles_means_true() {
    let json = r#"{"type":"import_configuration","document":{}}"#;
    match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::ImportConfiguration { apply_consoles, .. } => assert!(apply_consoles),
        _ => panic!("not import_configuration"),
    }
}

#[test]
fn an_absent_apply_clone_dir_means_true() {
    let json = r#"{"type":"import_configuration","document":{}}"#;
    match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::ImportConfiguration {
            apply_clone_dir, ..
        } => assert!(apply_clone_dir),
        _ => panic!("not import_configuration"),
    }
}

#[test]
fn an_absent_detect_default_agent_means_true() {
    let json = r#"{"type":"add_project","console_id":"c","source":"local"}"#;
    match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::AddProject {
            detect_default_agent,
            ..
        } => assert!(detect_default_agent),
        _ => panic!("not add_project"),
    }
}

/// `list_processes` names a session or a project by its own field, and the envelope's `id`
/// stays the request's.
#[test]
fn list_processes_reads_a_session_or_a_project() {
    let session =
        serde_json::from_str::<Request>(r#"{"type":"list_processes","id":"r1","session_id":"s"}"#)
            .unwrap();
    assert_eq!(session.id.as_deref(), Some("r1"));
    assert!(matches!(
        session.body,
        RequestBody::ListProcesses {
            target: ProcessTarget::Session { session_id }
        } if session_id == "s"
    ));
    assert!(matches!(
        serde_json::from_str::<Request>(r#"{"type":"list_processes","project_id":"p"}"#)
            .unwrap()
            .body,
        RequestBody::ListProcesses {
            target: ProcessTarget::Project { project_id }
        } if project_id == "p"
    ));
    assert!(serde_json::from_str::<Request>(r#"{"type":"list_processes"}"#).is_err());
}

/// `list_all_processes` takes no target.
#[test]
fn list_all_processes_reads_without_a_target() {
    assert!(matches!(
        serde_json::from_str::<Request>(r#"{"type":"list_all_processes","id":"r1"}"#)
            .unwrap()
            .body,
        RequestBody::ListAllProcesses
    ));
}

/// The bound items a request forces or keeps arrive with the state they were shown in, and
/// leaving both lists out names none.
#[test]
fn forced_and_kept_items_are_read_with_their_state_and_default_to_none() {
    let json = r#"{"type":"archive_session","session":"s","force_remove":[{"kind":"worktree","repository":"/r/.git","branch":null,"path":"/w","state":"locked","uncommitted":true}],"keep":[{"kind":"branch","repository":"/r/.git","branch":"b","state":"unmerged"}]}"#;
    match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::ArchiveSession {
            force_remove, keep, ..
        } => {
            assert_eq!(force_remove[0].state, SessionBranchState::Locked);
            assert!(force_remove[0].uncommitted && force_remove[0].branch.is_none());
            assert_eq!(keep[0].branch.as_deref(), Some("b"));
            assert!(!keep[0].uncommitted);
        }
        _ => panic!("not archive_session"),
    }
    for json in [
        r#"{"type":"delete_session","session":"s"}"#,
        r#"{"type":"delete_archived_sessions","console":"c"}"#,
        r#"{"type":"delete_project","project":"p"}"#,
    ] {
        let empty = match serde_json::from_str::<Request>(json).unwrap().body {
            RequestBody::DeleteSession {
                force_remove, keep, ..
            }
            | RequestBody::DeleteArchivedSessions {
                force_remove, keep, ..
            } => force_remove.is_empty() && keep.is_empty(),
            RequestBody::DeleteProject { force_remove, .. } => force_remove.is_empty(),
            _ => false,
        };
        assert!(empty, "{json}");
    }
    let unshown = r#"{"type":"archive_session","session":"s","force_remove":[{"kind":"branch","repository":"/r","branch":"b"}]}"#;
    assert!(serde_json::from_str::<Request>(unshown).is_err());
}

/// A `git` request may leave out `path`, and the source is emitted as `git`.
#[test]
fn a_git_request_may_omit_its_path_and_the_source_is_written_as_git() {
    let json = r#"{"type":"add_project","console_id":"c","source":"git"}"#;
    match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::AddProject { source, path, .. } => {
            assert_eq!(source, ProjectSource::Git);
            assert!(path.is_none());
        }
        _ => panic!("not add_project"),
    }
    assert_eq!(
        serde_json::to_value(ProjectSource::Git).unwrap(),
        serde_json::json!("git")
    );
}

/// The envelope's request id and the request's own fields arrive in one flat object, so a
/// request field named `id` would be indistinguishable from the envelope's — and the daemon
/// would act on the request id as if it were a record id. These are the requests that address
/// a stored record; each must survive an envelope id sitting next to it.
#[test]
fn an_envelope_id_never_shadows_what_a_request_acts_on() {
    type Target = fn(RequestBody) -> Option<String>;
    let cases: &[(&str, Target)] = &[
        (
            r#"{"type":"delete_console","id":"request-1","console":"console-7"}"#,
            |body| match body {
                RequestBody::DeleteConsole { console } => Some(console),
                _ => None,
            },
        ),
        (
            r#"{"type":"delete_account","id":"request-1","account":"account-7"}"#,
            |body| match body {
                RequestBody::DeleteAccount { account, .. } => Some(account),
                _ => None,
            },
        ),
        (
            r#"{"type":"inspect_account_trash","id":"request-1","account":"account-7"}"#,
            |body| match body {
                RequestBody::InspectAccountTrash { account } => Some(account),
                _ => None,
            },
        ),
        (
            r#"{"type":"delete_project","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::DeleteProject { project, .. } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"inspect_project_removal","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::InspectProjectRemoval { project } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"update_project","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::UpdateProject { project, .. } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"archive_session","id":"request-1","session":"session-7"}"#,
            |body| match body {
                RequestBody::ArchiveSession { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"switch_session_account","id":"request-1","session":"session-7","account":null}"#,
            |body| match body {
                RequestBody::SwitchSessionAccount { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"set_session_model","id":"request-1","session":"session-7","model":null}"#,
            |body| match body {
                RequestBody::SetSessionModel { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"handoff_session","id":"request-1","session":"session-7","agent":"codex"}"#,
            |body| match body {
                RequestBody::HandoffSession { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"interrupt_session","id":"request-1","session":"session-7"}"#,
            |body| match body {
                RequestBody::InterruptSession { session } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"read_session_history","id":"request-1","session":"session-7"}"#,
            |body| match body {
                RequestBody::ReadSessionHistory { session } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"delete_session","id":"request-1","session":"session-7"}"#,
            |body| match body {
                RequestBody::DeleteSession { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"set_session_pinned","id":"request-1","session":"session-7","pinned":true}"#,
            |body| match body {
                RequestBody::SetSessionPinned { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"set_session_locked","id":"request-1","session":"session-7","locked":true}"#,
            |body| match body {
                RequestBody::SetSessionLocked { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"set_session_colour","id":"request-1","session":"session-7","colour":"pink"}"#,
            |body| match body {
                RequestBody::SetSessionColour { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"submit_page","id":"request-1","page":"page-7","data":{}}"#,
            |body| match body {
                RequestBody::SubmitPage { page, .. } => Some(page),
                _ => None,
            },
        ),
        (
            r#"{"type":"confirm_trust","id":"request-1","session":"session-7","remember":true}"#,
            |body| match body {
                RequestBody::ConfirmTrust { session, .. } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"list_pages","id":"request-1","session":"session-7"}"#,
            |body| match body {
                RequestBody::ListPages { session } => Some(session),
                _ => None,
            },
        ),
        (
            r#"{"type":"refresh_git_status","id":"request-1","console":"console-7"}"#,
            |body| match body {
                RequestBody::RefreshGitStatus { console } => Some(console),
                _ => None,
            },
        ),
        (
            r#"{"type":"refresh_account_quotas","id":"request-1","agent":"codex","account":"account-7"}"#,
            |body| match body {
                RequestBody::RefreshAccountQuotas {
                    agent: Some(AgentId::CODEX),
                    account,
                } => account,
                _ => None,
            },
        ),
        (
            r#"{"type":"pull_project_branch","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::PullProjectBranch { project } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"refresh_project_git_status","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::RefreshProjectGitStatus { project } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"open_project_terminal","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::OpenProjectTerminal { project } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"reveal_project_path","id":"request-1","project":"project-7","path":"src/a%25.txt","worktree":"tree-1"}"#,
            |body| match body {
                RequestBody::RevealProjectPath {
                    project,
                    path,
                    worktree,
                } if path == "src/a%25.txt" && worktree.as_deref() == Some("tree-1") => {
                    Some(project)
                }
                _ => None,
            },
        ),
        (
            r#"{"type":"open_project_path","id":"request-1","project":"project-7","path":"src/a%25.txt"}"#,
            |body| match body {
                RequestBody::OpenProjectPath {
                    project,
                    path,
                    worktree,
                } if path == "src/a%25.txt" && worktree.is_none() => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"probe_path_links","id":"request-1","project":"project-7","paths":["a.ts","/tmp/b.png"]}"#,
            |body| match body {
                RequestBody::ProbePathLinks {
                    project: Some(project),
                    paths,
                } if paths == ["a.ts", "/tmp/b.png"] => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"open_project_directory","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::OpenProjectDirectory { project } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"push_project_branch","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::PushProjectBranch { project } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"switch_project_branch","id":"request-1","project":"project-7","branch":"feature/100%25"}"#,
            |body| match body {
                RequestBody::SwitchProjectBranch { project, branch } => {
                    assert_eq!(branch, "feature/100%25");
                    Some(project)
                }
                _ => None,
            },
        ),
        (
            r#"{"type":"create_shell_terminal","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::CreateShellTerminal { project, .. } => Some(project),
                _ => None,
            },
        ),
        (
            r#"{"type":"rename_shell_terminal","id":"request-1","shell_terminal":"terminal-7","title":"t"}"#,
            |body| match body {
                RequestBody::RenameShellTerminal { shell_terminal, .. } => Some(shell_terminal),
                _ => None,
            },
        ),
        (
            r#"{"type":"close_shell_terminal","id":"request-1","shell_terminal":"terminal-7"}"#,
            |body| match body {
                RequestBody::CloseShellTerminal { shell_terminal, .. } => Some(shell_terminal),
                _ => None,
            },
        ),
        (
            r#"{"type":"restore_project_changes","id":"request-1","project":"project-7"}"#,
            |body| match body {
                RequestBody::RestoreProjectChanges { project } => Some(project),
                _ => None,
            },
        ),
    ];

    for (json, target) in cases {
        let request: Request = serde_json::from_str(json).expect(json);
        assert_eq!(request.id.as_deref(), Some("request-1"), "{json}");
        let target = target(request.body).expect(json);
        assert!(target.ends_with("-7"), "{json} acted on `{target}`");
    }
}

#[test]
fn open_path_link_parses_with_and_without_a_project() {
    let request: Request = serde_json::from_str(
        r#"{"type":"open_path_link","id":"request-1","project":"project-7","path":"src/a.ts"}"#,
    )
    .unwrap();
    assert!(matches!(
        request.body,
        RequestBody::OpenPathLink { project: Some(project), path } if project == "project-7" && path == "src/a.ts"
    ));
    let request: Request =
        serde_json::from_str(r#"{"type":"open_path_link","path":"/tmp/b.png"}"#).unwrap();
    assert!(matches!(
        request.body,
        RequestBody::OpenPathLink { project: None, path } if path == "/tmp/b.png"
    ));
}

#[test]
fn reveal_path_link_parses_with_and_without_a_project() {
    let request: Request = serde_json::from_str(
        r#"{"type":"reveal_path_link","id":"request-1","project":"project-7","path":"src/a.ts"}"#,
    )
    .unwrap();
    assert!(matches!(
        request.body,
        RequestBody::RevealPathLink { project: Some(project), path } if project == "project-7" && path == "src/a.ts"
    ));
    let request: Request =
        serde_json::from_str(r#"{"type":"reveal_path_link","path":"/tmp/b.png"}"#).unwrap();
    assert!(matches!(
        request.body,
        RequestBody::RevealPathLink { project: None, path } if path == "/tmp/b.png"
    ));
}

#[test]
fn a_quota_refresh_may_name_neither_agent_nor_account() {
    let request: Request =
        serde_json::from_str(r#"{"type":"refresh_account_quotas","id":"request-1"}"#).unwrap();
    assert!(matches!(
        request.body,
        RequestBody::RefreshAccountQuotas {
            agent: None,
            account: None
        }
    ));
}

/// A switch must name its target: an explicit null is the default account, and a request that
/// names nothing is a different request, which the coordinator refuses rather than reading as
/// the default.
#[test]
fn a_switch_names_the_default_account_with_an_explicit_null() {
    let account = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::SwitchSessionAccount { account, .. } => account,
        _ => panic!("parses as switch_session_account"),
    };
    let kind = r#""type":"switch_session_account","session":"s""#;
    assert_eq!(account(&format!("{{{kind}}}")), None);
    assert_eq!(
        account(&format!(r#"{{{kind},"account":null}}"#)),
        Some(None)
    );
    assert_eq!(
        account(&format!(r#"{{{kind},"account":"a"}}"#)),
        Some(Some("a".to_string()))
    );
}

/// An absent optional field is "leave it alone" and an explicit null is "clear it", which is
/// the only way a project can go back to following its console's project session agent (else
/// its `default_agent`).
#[test]
fn clearing_a_projects_default_agent_is_distinguishable_from_leaving_it() {
    let absent: Request =
        serde_json::from_str(r#"{"type":"update_project","project":"p"}"#).unwrap();
    let cleared: Request =
        serde_json::from_str(r#"{"type":"update_project","project":"p","default_agent":null}"#)
            .unwrap();
    match (absent.body, cleared.body) {
        (
            RequestBody::UpdateProject {
                default_agent: absent,
                ..
            },
            RequestBody::UpdateProject {
                default_agent: cleared,
                ..
            },
        ) => {
            assert_eq!(absent, None);
            assert_eq!(cleared, Some(None));
        }
        _ => panic!("both parse as update_project"),
    }
}

/// An absent `tags` leaves a project's tags alone, and a present array (even an empty one) is
/// what replaces them.
#[test]
fn updating_a_projects_tags_is_distinguishable_from_leaving_them() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateProject { tags, .. } => tags,
        _ => panic!("parses as update_project"),
    };
    assert_eq!(parse(r#"{"type":"update_project","project":"p"}"#), None);
    assert_eq!(
        parse(r#"{"type":"update_project","project":"p","tags":[]}"#),
        Some(vec![])
    );
    assert_eq!(
        parse(r#"{"type":"update_project","project":"p","tags":["a","b"]}"#),
        Some(vec!["a".to_string(), "b".to_string()])
    );
}

/// An absent `description` leaves a project's description alone, and an explicit `null` is
/// what clears it.
#[test]
fn updating_a_projects_description_is_distinguishable_from_leaving_it() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateProject { description, .. } => description,
        _ => panic!("parses as update_project"),
    };
    assert_eq!(parse(r#"{"type":"update_project","project":"p"}"#), None);
    assert_eq!(
        parse(r#"{"type":"update_project","project":"p","description":null}"#),
        Some(None)
    );
    assert_eq!(
        parse(r#"{"type":"update_project","project":"p","description":"A tool"}"#),
        Some(Some("A tool".to_string()))
    );
}

/// A project's accounts, models and efforts are cleared the same way as a console's: an agent
/// left out is left alone, `null` clears an account (back to inheriting) or a launch field,
/// and each agent's is independent; a create request carries a model as a plain optional
/// string and an account as a [`ProjectAccount`], an agent left out or `null` being
/// inheriting there too.
#[test]
fn clearing_a_projects_settings_is_distinguishable_from_leaving_them() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateProject {
            accounts, launch, ..
        } => (accounts, launch),
        _ => panic!("parses as update_project"),
    };
    let (accounts, launch) = parse(r#"{"type":"update_project","project":"p"}"#);
    assert_eq!(accounts, PerAgent::default());
    assert!(
        launch[AgentId::CLAUDE].is_keep()
            && launch[AgentId::CODEX].is_keep()
            && launch[AgentId::GROK].is_keep()
    );

    let (accounts, launch) = parse(
        r#"{"type":"update_project","project":"p","accounts":{"claude":null,
            "grok":{"kind":"account","id":"acct-1"}},
            "launch":{"claude":{"model":"opus"},"codex":{"effort":null}}}"#,
    );
    assert_eq!(accounts[AgentId::CLAUDE], Change::Clear);
    assert_eq!(accounts[AgentId::CODEX], Change::Keep);
    assert_eq!(
        accounts[AgentId::GROK],
        Change::Set(ProjectAccount::Account {
            id: "acct-1".to_string()
        })
    );
    assert_eq!(
        launch[AgentId::CLAUDE].model,
        Some(Some("opus".to_string()))
    );
    assert_eq!(launch[AgentId::CLAUDE].effort, None);
    assert_eq!(launch[AgentId::CODEX].model, None);
    assert_eq!(launch[AgentId::CODEX].effort, Some(None));
    assert!(launch[AgentId::GROK].is_keep());

    let RequestBody::AddProject {
        accounts, launch, ..
    } = serde_json::from_str::<Request>(
        r#"{"type":"add_project","console_id":"c","source":"local","path":"/p",
            "accounts":{"codex":{"kind":"account","id":"acct-2"},"grok":{"kind":"default"}},
            "launch":{"grok":{"model":"grok-4"}}}"#,
    )
    .unwrap()
    .body
    else {
        panic!("parses as add_project");
    };
    assert_eq!(accounts[AgentId::CLAUDE], None);
    assert_eq!(
        accounts[AgentId::CODEX],
        Some(ProjectAccount::Account {
            id: "acct-2".to_string()
        })
    );
    assert_eq!(accounts[AgentId::GROK], Some(ProjectAccount::Default));
    assert_eq!(launch[AgentId::GROK].model.as_deref(), Some("grok-4"));
    assert_eq!(launch[AgentId::CLAUDE], Launch::default());
}

/// `add_project` takes a project's own git workflow values: an item absent or `null` follows
/// the application-wide value, and a `development` or `delivery` outside the three is refused.
#[test]
fn add_project_parses_its_own_git_workflow_values() {
    use crate::protocol::{GitDelivery, GitDevelopment, GitWorkflowOverride};
    let parse = |tail: &str| {
        serde_json::from_str::<Request>(&format!(
            r#"{{"type":"add_project","console_id":"c","source":"local","path":"/p"{tail}}}"#
        ))
    };
    let git_workflow = |tail: &str| match parse(tail).unwrap().body {
        RequestBody::AddProject { git_workflow, .. } => git_workflow.unwrap_or_default(),
        _ => panic!("not add_project"),
    };
    assert_eq!(git_workflow(""), GitWorkflowOverride::default());
    assert_eq!(
        git_workflow(r#","git_workflow":null"#),
        GitWorkflowOverride::default()
    );
    assert_eq!(
        git_workflow(
            r#","git_workflow":{"development":"in_place","delivery":"merge","revert_gate":null}"#
        ),
        GitWorkflowOverride {
            development: Some(GitDevelopment::InPlace),
            delivery: Some(GitDelivery::Merge),
            ..GitWorkflowOverride::default()
        }
    );
    assert!(parse(r#","git_workflow":{"delivery":"rebase"}"#).is_err());
    assert!(parse(r#","git_workflow":{"development":true}"#).is_err());
}

/// A project's account takes all three states on an update: `null` is inheriting, and an
/// object says the agent's default account or a named one; an agent left out is left alone.
#[test]
fn a_projects_account_is_inherit_default_or_named_on_an_update() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateProject { accounts, .. } => accounts[AgentId::CLAUDE].clone(),
        _ => panic!("parses as update_project"),
    };
    let update = |field: &str| format!(r#"{{"type":"update_project","project":"p"{field}}}"#);
    assert_eq!(parse(&update("")), Change::Keep);
    assert_eq!(
        parse(&update(r#","accounts":{"claude":null}"#)),
        Change::Clear
    );
    assert_eq!(
        parse(&update(r#","accounts":{"claude":{"kind":"inherit"}}"#)),
        Change::Set(ProjectAccount::Inherit)
    );
    assert_eq!(
        parse(&update(r#","accounts":{"claude":{"kind":"default"}}"#)),
        Change::Set(ProjectAccount::Default)
    );
    assert_eq!(
        parse(&update(
            r#","accounts":{"claude":{"kind":"account","id":"a"}}"#
        )),
        Change::Set(ProjectAccount::Account {
            id: "a".to_string()
        })
    );
    // A bare string is not an account any more: nothing reads an id without saying what it is.
    assert!(serde_json::from_str::<Request>(&update(r#","accounts":{"claude":"a"}"#)).is_err());
}

/// A key left out versus `null` is the only way a console's account can be cleared as well as
/// set, and each agent's is independent of the others.
#[test]
fn clearing_a_consoles_account_is_distinguishable_from_leaving_it() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateConsole { accounts, .. } => accounts,
        _ => panic!("parses as update_console"),
    };
    assert_eq!(
        parse(r#"{"type":"update_console","console":"c"}"#),
        PerAgent::default()
    );
    assert_eq!(
        parse(
            r#"{"type":"update_console","console":"c","accounts":{"claude":null,
                "codex":"acct-1"}}"#
        ),
        PerAgent::default()
            .with(AgentId::CLAUDE, Change::Clear)
            .with(AgentId::CODEX, Change::Set("acct-1".to_string()))
            .with(AgentId::GROK, Change::Keep)
    );
    assert_eq!(
        parse(r#"{"type":"update_console","console":"c","accounts":{"grok":"acct-2"}}"#),
        PerAgent::default().with(AgentId::GROK, Change::Set("acct-2".to_string()))
    );
}

/// A session's own account is chosen, or deliberately the default one, or left to the console,
/// by the same absent-versus-null distinction.
#[test]
fn choosing_the_default_account_for_a_session_is_distinguishable_from_choosing_none() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::OpenSession { account, .. } => account,
        _ => panic!("parses as open_session"),
    };
    assert_eq!(parse(r#"{"type":"open_session","console_id":"c"}"#), None);
    assert_eq!(
        parse(r#"{"type":"open_session","console_id":"c","account":null}"#),
        Some(None)
    );
    assert_eq!(
        parse(r#"{"type":"open_session","console_id":"c","account":"acct-1"}"#),
        Some(Some("acct-1".to_string()))
    );
}

/// The conversation to resume is optional on the wire: an `open_session` without it still parses.
#[test]
fn the_conversation_to_resume_is_optional() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::OpenSession {
            resume_agent_session_id,
            ..
        } => resume_agent_session_id,
        _ => panic!("parses as open_session"),
    };
    assert_eq!(parse(r#"{"type":"open_session","console_id":"c"}"#), None);
    assert_eq!(
        parse(r#"{"type":"open_session","console_id":"c","resume_agent_session_id":"u"}"#),
        Some("u".to_string())
    );
}

/// Each session launch setting distinguishes inheriting from deliberately using the agent's
/// own default, and a record emitted on the wire keeps that distinction too.
#[test]
fn session_launch_settings_preserve_absent_null_and_values() {
    for (fields, expected) in [
        ("", (None, None)),
        (
            r#","model":null,"effort":"high""#,
            (Some(None), Some(Some("high".to_string()))),
        ),
        (
            r#","model":"opus","effort":null"#,
            (Some(Some("opus".to_string())), Some(None)),
        ),
    ] {
        let json = format!(r#"{{"type":"open_session","console_id":"c"{fields}}}"#);
        let RequestBody::OpenSession { model, effort, .. } =
            serde_json::from_str::<Request>(&json).unwrap().body
        else {
            panic!("parses as open_session")
        };
        assert_eq!((model, effort), expected, "{json}");

        let json = format!(
            r#"{{"id":"s","agent":"claude","agent_session_id":null,
            "console_id":"c","project_id":null,"host_id":"local","kind":"console",
            "origin":"user","title":"Hub","status":"idle","role":"coordinator",
            "has_conversation":false,
            "bound_to":null,"colour":null,"ordinal":null,"account_id":null,"config_dir":null,
            "pinned":false,"started_at":0,"ended_at":null{fields}}}"#
        );
        let session: Session = serde_json::from_str(&json).unwrap();
        assert_eq!((session.model.clone(), session.effort.clone()), expected);
        let emitted = serde_json::to_value(&session).unwrap();
        for (field, value) in [("model", expected.0), ("effort", expected.1)] {
            assert_eq!(emitted.get(field).is_some(), value.is_some());
            if let Some(value) = value {
                assert_eq!(emitted[field], serde_json::to_value(value).unwrap());
            }
        }
    }
}

/// The avatar is cleared the same way as an account: an explicit `null`.
#[test]
fn clearing_a_consoles_icon_is_distinguishable_from_leaving_it() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateConsole { icon, .. } => icon,
        _ => panic!("parses as update_console"),
    };
    assert_eq!(parse(r#"{"type":"update_console","console":"c"}"#), None);
    assert_eq!(
        parse(r#"{"type":"update_console","console":"c","icon":null}"#),
        Some(None)
    );
    assert_eq!(
        parse(r#"{"type":"update_console","console":"c","icon":"data:image/png;base64,AA"}"#),
        Some(Some("data:image/png;base64,AA".to_string()))
    );
}

/// A console's console session and project session launch settings are cleared the same way:
/// within one agent's pair, a field left out is left alone, `null` clears it and a string
/// sets it, and each agent's is independent.
#[test]
fn clearing_a_consoles_launch_setting_is_distinguishable_from_leaving_it() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateConsole {
            launch,
            project_session_launch,
            ..
        } => (launch, project_session_launch),
        _ => panic!("parses as update_console"),
    };
    let (launch, project_session_launch) = parse(r#"{"type":"update_console","console":"c"}"#);
    assert!(
        launch[AgentId::CLAUDE].is_keep()
            && launch[AgentId::CODEX].is_keep()
            && launch[AgentId::GROK].is_keep()
    );
    assert!(project_session_launch[AgentId::CLAUDE].is_keep());

    let (launch, project_session_launch) = parse(
        r#"{"type":"update_console","console":"c",
            "launch":{"claude":{"model":null},"codex":{"model":"gpt-5"}},
            "project_session_launch":{"grok":{"effort":"low"}}}"#,
    );
    assert_eq!(launch[AgentId::CLAUDE].model, Some(None));
    assert_eq!(launch[AgentId::CLAUDE].effort, None);
    assert_eq!(
        launch[AgentId::CODEX].model,
        Some(Some("gpt-5".to_string()))
    );
    assert!(launch[AgentId::GROK].is_keep());
    assert_eq!(
        project_session_launch[AgentId::GROK].effort,
        Some(Some("low".to_string()))
    );
    assert!(project_session_launch[AgentId::CLAUDE].is_keep());
}

/// A model change keeps an absent value, gives an explicit `null` back to the agent and pins
/// a string, for the model and the effort alike.
#[test]
fn a_session_model_change_tells_an_absent_value_from_null() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::SetSessionModel { model, effort, .. } => (model, effort),
        _ => panic!("parses as set_session_model"),
    };
    assert_eq!(
        parse(r#"{"type":"set_session_model","session":"s","model":"opus"}"#),
        (Some(Some("opus".to_string())), None)
    );
    assert_eq!(
        parse(r#"{"type":"set_session_model","session":"s","model":null,"effort":"high"}"#),
        (Some(None), Some(Some("high".to_string())))
    );
    assert_eq!(
        parse(r#"{"type":"set_session_model","session":"s","effort":null}"#),
        (None, Some(None))
    );
}

/// Absent versus null is how a console's project session agent can be cleared as well as set.
#[test]
fn clearing_a_consoles_project_session_agent_is_distinguishable_from_leaving_it() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateConsole {
            project_session_agent,
            ..
        } => project_session_agent,
        _ => panic!("parses as update_console"),
    };
    assert_eq!(parse(r#"{"type":"update_console","console":"c"}"#), None);
    assert_eq!(
        parse(r#"{"type":"update_console","console":"c","project_session_agent":null}"#),
        Some(None)
    );
    assert_eq!(
        parse(r#"{"type":"update_console","console":"c","project_session_agent":"grok"}"#),
        Some(Some(crate::protocol::AgentId::GROK))
    );
}

#[test]
fn creating_a_console_needs_no_account_or_launch_setting() {
    let request: Request =
        serde_json::from_str(r#"{"type":"create_console","name":"n","default_agent":"codex"}"#)
            .unwrap();
    match request.body {
        RequestBody::CreateConsole {
            accounts,
            launch,
            project_session_launch,
            ..
        } => {
            assert_eq!(accounts, PerAgent::default());
            assert_eq!(launch, PerAgent::default());
            assert_eq!(project_session_launch, PerAgent::default());
        }
        _ => panic!("parses as create_console"),
    }
}

/// A create request names only the agents it sets: a key left out, or `null`, is unset, and
/// a value is kept for that agent alone.
#[test]
fn creating_a_console_reads_each_agents_account_and_launch_pair() {
    let request: Request = serde_json::from_str(
        r#"{"type":"create_console","name":"n","default_agent":"codex",
            "accounts":{"claude":"acct-1","codex":null},
            "launch":{"claude":{"model":"opus","effort":"high"}},
            "project_session_launch":{"codex":{"model":"gpt-5"}}}"#,
    )
    .unwrap();
    let RequestBody::CreateConsole {
        accounts,
        launch,
        project_session_launch,
        ..
    } = request.body
    else {
        panic!("parses as create_console");
    };
    assert_eq!(accounts[AgentId::CLAUDE].as_deref(), Some("acct-1"));
    assert_eq!(accounts[AgentId::CODEX], None);
    assert_eq!(accounts[AgentId::GROK], None);
    assert_eq!(
        launch[AgentId::CLAUDE],
        Launch {
            model: Some("opus".to_string()),
            effort: Some("high".to_string())
        }
    );
    assert_eq!(launch[AgentId::CODEX], Launch::default());
    assert_eq!(
        project_session_launch[AgentId::CODEX].model.as_deref(),
        Some("gpt-5")
    );
    assert_eq!(project_session_launch[AgentId::CODEX].effort, None);
}

/// `trust_parent_dir` is newer than `remember`, so a request without it still reads, as "this
/// project only".
#[test]
fn a_trust_confirmation_without_a_scope_means_this_project() {
    let parse = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::ConfirmTrust {
            remember,
            trust_parent_dir,
            ..
        } => (remember, trust_parent_dir),
        _ => panic!("parses as confirm_trust"),
    };
    assert_eq!(
        parse(r#"{"type":"confirm_trust","session":"s","remember":true}"#),
        (true, false)
    );
    assert_eq!(
        parse(r#"{"type":"confirm_trust","session":"s","remember":false,"trust_parent_dir":true}"#),
        (false, true)
    );
    match serde_json::from_str::<Request>(r#"{"type":"remove_trusted_directory","path":"/work"}"#)
        .unwrap()
        .body
    {
        RequestBody::RemoveTrustedDirectory { path } => assert_eq!(path, "/work"),
        _ => panic!("parses as remove_trusted_directory"),
    }
}

/// An answer to a request carries only the values the user changed, and keeps a value left
/// out apart from one set to null, which is the agent's own default.
#[test]
fn an_answer_to_a_session_request_tells_a_value_left_out_from_a_null_one() {
    let parsed = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::AnswerSessionRequest {
            approve,
            agent,
            account,
            model,
            effort,
            ..
        } => (approve, agent, account, model, effort),
        _ => panic!("parses as answer_session_request"),
    };
    assert_eq!(
        parsed(r#"{"type":"answer_session_request","request_id":"r","approve":true}"#),
        (true, None, None, None, None)
    );
    assert_eq!(
        parsed(
            r#"{"type":"answer_session_request","request_id":"r","approve":true,
                "agent":"grok","account":null,"model":"m","effort":null}"#
        ),
        (
            true,
            Some(crate::protocol::AgentId::GROK),
            Some(None),
            Some(Some("m".to_string())),
            Some(None)
        )
    );
}

/// Prevent sleep arrives as a switch and a name of when to hold; a name that is not one is refused.
#[test]
fn an_update_settings_reads_prevent_sleep_and_refuses_an_unknown_when() {
    use crate::protocol::KeepAwakeWhen;
    let body = |json: &str| serde_json::from_str::<Request>(json).map(|request| request.body);
    match body(r#"{"type":"update_settings","keep_awake":true,"keep_awake_when":"always"}"#)
        .unwrap()
    {
        RequestBody::UpdateSettings {
            keep_awake,
            keep_awake_when,
            ..
        } => assert_eq!(
            (keep_awake, keep_awake_when),
            (Some(true), Some(KeepAwakeWhen::Always))
        ),
        _ => panic!("parses as update_settings"),
    }
    assert!(body(r#"{"type":"update_settings","keep_awake_when":"sometimes"}"#).is_err());
}

/// One field is enough: the others stay absent, and a body that names nothing leaves
/// `auto_select_account` and `auto_start_quota_windows` absent too.
#[test]
fn an_update_settings_may_name_only_one_field() {
    let parsed = |json: &str| match serde_json::from_str::<Request>(json).unwrap().body {
        RequestBody::UpdateSettings {
            auto_pull_repositories,
            auto_delete_unresumable_archives,
            auto_approve_session_requests,
            auto_approve_resume_requests,
            auto_approve_account_switches,
            auto_approve_handoffs,
            auto_select_account,
            auto_start_quota_windows,
            default_clone_dir,
            ..
        } => (
            auto_pull_repositories,
            auto_delete_unresumable_archives,
            auto_approve_session_requests,
            auto_approve_resume_requests,
            auto_approve_account_switches,
            auto_approve_handoffs,
            auto_select_account,
            auto_start_quota_windows,
            default_clone_dir,
        ),
        _ => panic!("parses as update_settings"),
    };
    assert_eq!(
        parsed(r#"{"type":"update_settings","auto_select_account":true}"#),
        (None, None, None, None, None, None, Some(true), None, None)
    );
    assert_eq!(
        parsed(r#"{"type":"update_settings","auto_approve_resume_requests":true}"#),
        (None, None, None, Some(true), None, None, None, None, None)
    );
    assert_eq!(
        parsed(r#"{"type":"update_settings","auto_approve_account_switches":true}"#),
        (None, None, None, None, Some(true), None, None, None, None)
    );
    assert_eq!(
        parsed(r#"{"type":"update_settings","auto_approve_handoffs":true}"#),
        (None, None, None, None, None, Some(true), None, None, None)
    );
    assert_eq!(
        parsed(r#"{"type":"update_settings","auto_start_quota_windows":true}"#),
        (None, None, None, None, None, None, None, Some(true), None)
    );
    assert_eq!(
        parsed(r#"{"type":"update_settings"}"#),
        (None, None, None, None, None, None, None, None, None)
    );
}
