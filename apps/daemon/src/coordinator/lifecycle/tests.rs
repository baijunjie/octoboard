use super::*;
use crate::coordinator::handle;
use crate::coordinator::test_support::{
    bound_session, code_of, console, console_workdir, lead_session, status_of, wait_until,
};
use crate::hostfs::trash::TrashFailure;
use crate::protocol::{
    error_code, Agent, CodedError, Event, OrchestrationRole, Origin, Project, ProjectSource,
    RequestBody, Role, Session, SessionStatus,
};
use crate::records;
use crate::state::Deletion;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{
    console_session, console_with_project_and_accounts, idle_stand_in, project_session, session_ids,
};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Only an archived session can be deleted, and the bulk delete takes exactly its scope: a
/// project's archived sessions, or with no project the console's own archived console sessions.
#[test]
fn deleting_sessions_is_limited_to_archived_ones_in_scope() {
    let (state, dir) = crate::test_support::app_state("coordinator-delete-sessions");
    let workdir = console_workdir(&dir);
    for id in ["console-1", "console-2"] {
        let mut other = console(Agent::Claude, &workdir);
        other.id = id.to_string();
        state.store.insert_console(&other).unwrap();
    }
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
    let add = |id: &str, console: &str, role: Role, project: Option<&str>, status| {
        state
            .store
            .insert_session(&Session {
                id: id.to_string(),
                agent: Agent::Claude,
                agent_session_id: None,
                console_id: console.to_string(),
                project_id: project.map(str::to_string),
                host_id: LOCAL_HOST_ID.to_string(),
                role,
                origin: Origin::User,
                title: id.to_string(),
                status,
                terminal_title: None,
                record_titles: Default::default(),
                background_tasks: Vec::new(),
                switching_account: false,
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
    };
    let archived = SessionStatus::Archived;
    add(
        "project-archived",
        "console-1",
        Role::Project,
        Some("project-1"),
        archived,
    );
    add(
        "project-idle",
        "console-1",
        Role::Project,
        Some("project-1"),
        SessionStatus::Idle,
    );
    add(
        "console-archived",
        "console-1",
        Role::Console,
        None,
        archived,
    );
    add(
        "other-console-archived",
        "console-2",
        Role::Console,
        None,
        archived,
    );
    let remaining = || -> Vec<String> {
        let mut ids: Vec<String> = state
            .store
            .list_sessions()
            .unwrap()
            .into_iter()
            .map(|session| session.id)
            .collect();
        ids.sort();
        ids
    };

    // A resume in flight holds the session's launch claim, which keeps it from being deleted.
    let claim = state.begin_launch("project-archived").unwrap();
    assert!(!state.delete_if_archived("project-archived").unwrap());
    drop(claim);

    delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
    assert_eq!(
        remaining(),
        ["console-archived", "other-console-archived", "project-idle"]
    );

    delete_archived_sessions(&state, "console-1", None, None).unwrap();
    assert_eq!(remaining(), ["other-console-archived", "project-idle"]);
}

/// The line is the process, not the status: any bound session with one refuses the archive,
/// working, awaiting instructions or waiting for the user alike, and so does one being
/// launched right now. The refusal counts them and names them, and nothing was archived.
#[tokio::test]
async fn archiving_a_console_session_is_refused_while_a_bound_process_runs() {
    use SessionStatus::{Idle, Interrupted, WaitingUser, Working};
    let (state, _dir) = console_with_project_and_accounts("coordinator-archive-refused");
    state
        .store
        .insert_session(&console_session("hub", Idle))
        .unwrap();
    state
        .store
        .insert_session(&bound_session("dormant", Interrupted, "hub"))
        .unwrap();

    // (what runs, the statuses of the bound sessions that have a process)
    let cases: [(&str, &[SessionStatus]); 4] = [
        ("working", &[Working]),
        ("awaiting instructions", &[Idle]),
        ("waiting for the user", &[WaitingUser]),
        ("several", &[Working, Idle]),
    ];
    for (name, statuses) in cases {
        let mut live = Vec::new();
        let ids: Vec<String> = (0..statuses.len()).map(|n| format!("run-{n}")).collect();
        for (id, status) in ids.iter().zip(statuses) {
            state
                .store
                .insert_session(&Session {
                    title: format!("Task “{id}”, notes"),
                    ..bound_session(id, *status, "hub")
                })
                .unwrap();
            let fake = idle_stand_in(id);
            state.register_live(fake.clone());
            live.push(fake);
        }

        let err = archive_session(&state, "hub").expect_err(name);
        assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
        let coded = err.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.params["count"], statuses.len().to_string(), "{name}");
        let mut shown_ids: Vec<String> =
            serde_json::from_str(&coded.params["session_ids"]).unwrap();
        shown_ids.sort();
        assert_eq!(shown_ids, ids, "{name}");
        for id in &ids {
            assert!(
                coded.params["sessions"].contains(&format!("Task “{id}”, notes")),
                "{name}"
            );
            assert!(coded.message.contains(id.as_str()), "{name}");
        }
        assert!(!coded.params["sessions"].contains("dormant"), "{name}");
        assert_eq!(status_of(&state, "hub"), Some(Idle), "{name}");
        assert_eq!(status_of(&state, "dormant"), Some(Interrupted), "{name}");

        drop(live); // ends the stand-in processes
        for id in &ids {
            state.store.delete_session(id).unwrap();
        }
    }

    // A bound session being launched has no registered process yet, and still counts.
    let claim = state.begin_launch("dormant").unwrap();
    let err = archive_session(&state, "hub").expect_err("launching");
    assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
    assert_eq!(status_of(&state, "hub"), Some(Idle));
    drop(claim);
}

/// The user's confirmed control request ends both levels, including manually bound
/// sessions and launches in progress. Agent-driven archives keep their separate safeguard.
#[tokio::test]
async fn user_archive_ends_running_descendants_and_preserves_other_sessions() {
    use SessionStatus::{Archived, Idle, Interrupted, WaitingUser, Working};
    let (state, _dir) = console_with_project_and_accounts("coordinator-user-archive-cascade");
    for session in [
        console_session("hub", Idle),
        lead_session("lead", Working, "hub"),
        Session {
            origin: Origin::User,
            ..bound_session("manual", WaitingUser, "hub")
        },
        bound_session("nested", Idle, "lead"),
        bound_session("launching", Interrupted, "lead"),
        Session {
            ended_at: Some(5),
            ..bound_session("old", Archived, "hub")
        },
        console_session("other-hub", Idle),
        bound_session("elsewhere", Working, "other-hub"),
        project_session("unbound", Agent::Claude, Working),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let processes: Vec<_> = ["hub", "lead", "manual", "nested", "elsewhere", "unbound"]
        .into_iter()
        .map(|id| {
            let live = idle_stand_in(id);
            state.register_live(live.clone());
            live
        })
        .collect();
    let launch = state.begin_launch("launching").unwrap();

    // Preflight includes descendants before ending any process.
    state.store.set_session_locked("nested", true).unwrap();
    let request = || RequestBody::ArchiveSession {
        session: "hub".to_string(),
        force_remove: Vec::new(),
        keep: Vec::new(),
        keep_process_ids: Vec::new(),
    };
    let err = handle(&state, None, request()).await.unwrap_err();
    assert_eq!(code_of(&err), error_code::SESSION_LOCKED);
    for id in ["hub", "lead", "manual", "nested", "launching"] {
        assert_ne!(status_of(&state, id), Some(Archived), "{id}");
    }
    state.store.set_session_locked("nested", false).unwrap();
    let mut events = state.subscribe();
    handle(&state, None, request()).await.unwrap();
    let mut archived = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::SessionUpserted { session } = event {
            archived.push(session.id);
        }
    }
    assert_eq!(archived.last().map(String::as_str), Some("hub"));
    let position = |id: &str| archived.iter().position(|session| session == id).unwrap();
    assert!(position("nested") < position("lead"));
    assert!(position("launching") < position("lead"));
    for process in &processes[..4] {
        wait_until(|| process.poll_exit()).await;
    }
    for process in &processes[4..] {
        assert!(!process.poll_exit(), "unrelated process was ended");
    }

    for id in ["hub", "lead", "manual", "nested", "launching"] {
        assert_eq!(status_of(&state, id), Some(Archived), "{id}");
    }
    for id in ["elsewhere", "unbound"] {
        assert_eq!(status_of(&state, id), Some(Working), "{id}");
    }
    assert_eq!(status_of(&state, "other-hub"), Some(Idle));
    assert_eq!(state.session_record("old").unwrap().ended_at, Some(5));
    drop(launch);
    drop(processes);
}

/// With no bound process running, archiving a console session archives every bound session
/// that is not archived yet, and only those: not an unbound session, not one bound to another
/// console session, and an archived one keeps its own ended time.
#[tokio::test]
async fn archiving_a_console_session_archives_its_interrupted_bound_sessions() {
    use SessionStatus::{Archived, Idle, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-archive-cascade");
    for session in [
        console_session("hub", Idle),
        console_session("other-hub", Idle),
        bound_session("a", Interrupted, "hub"),
        bound_session("b", Interrupted, "hub"),
        Session {
            ended_at: Some(5),
            ..bound_session("old", Archived, "hub")
        },
        bound_session("elsewhere", Interrupted, "other-hub"),
        project_session("unbound", Agent::Claude, Interrupted),
    ] {
        state.store.insert_session(&session).unwrap();
    }

    archive_session(&state, "hub").unwrap();

    let statuses: Vec<(String, SessionStatus)> = session_ids(&state)
        .into_iter()
        .map(|id| {
            let status = status_of(&state, &id).unwrap();
            (id, status)
        })
        .collect();
    assert_eq!(
        statuses,
        [
            ("a".to_string(), Archived),
            ("b".to_string(), Archived),
            ("elsewhere".to_string(), Interrupted),
            ("hub".to_string(), Archived),
            ("old".to_string(), Archived),
            ("other-hub".to_string(), Idle),
            ("unbound".to_string(), Interrupted),
        ]
    );
    assert_eq!(
        state.store.get_session("old").unwrap().unwrap().ended_at,
        Some(5)
    );
}

/// A project session that started sessions is an owner like a console session: archiving it is
/// refused while one of them has a process, and otherwise takes the interrupted ones along.
#[tokio::test]
async fn archiving_a_project_session_follows_the_rule_for_the_sessions_bound_to_it() {
    use SessionStatus::{Archived, Idle, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-archive-project-owner");
    for session in [
        project_session("starter", Agent::Claude, Idle),
        bound_session("running", Idle, "starter"),
        bound_session("dormant", Interrupted, "starter"),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let running = idle_stand_in("running");
    state.register_live(running.clone());

    let err = archive_session(&state, "starter").expect_err("a bound session is running");
    assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
    assert_eq!(status_of(&state, "starter"), Some(Idle));
    assert_eq!(status_of(&state, "dormant"), Some(Interrupted));

    drop(running);
    state.store.delete_session("running").unwrap();
    archive_session(&state, "starter").unwrap();
    assert_eq!(status_of(&state, "starter"), Some(Archived));
    assert_eq!(status_of(&state, "dormant"), Some(Archived));
}

/// Archiving a project session reaches no other session: the automatic archive of a `done` report
/// (see `messaging/reporting.rs`) and a `delete_project` that stops sessions both end here, and
/// neither takes the console session the session is bound to.
#[tokio::test]
async fn archiving_a_bound_project_session_leaves_its_console_session_alone() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-archive-bound-alone");
    state
        .store
        .insert_session(&console_session("hub", SessionStatus::Idle))
        .unwrap();
    state
        .store
        .insert_session(&bound_session("worker", SessionStatus::Idle, "hub"))
        .unwrap();

    archive_session(&state, "worker").unwrap();

    assert_eq!(status_of(&state, "worker"), Some(SessionStatus::Archived));
    assert_eq!(status_of(&state, "hub"), Some(SessionStatus::Idle));
}

#[tokio::test]
async fn session_lock_changes_cannot_cross_a_removal_claim() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-lock-removal-race");
    state
        .store
        .insert_session(&console_session("session", SessionStatus::Interrupted))
        .unwrap();
    set_session_locked(&state, "session", true).unwrap();
    let removal = state.claim_session_removal();
    let nested = state.claim_session_removal();
    let task_state = state.clone();
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        for locked in [true, false] {
            assert_eq!(
                code_of(&set_session_locked(&task_state, "session", locked).unwrap_err()),
                error_code::SESSION_LOCK_BUSY
            );
        }
    })
    .await
    .unwrap();
    drop(nested);
    assert_eq!(
        code_of(&set_session_locked(&state, "session", false).unwrap_err()),
        error_code::SESSION_LOCK_BUSY
    );
    assert_eq!(
        code_of(&archive_session(&state, "session").unwrap_err()),
        error_code::SESSION_LOCKED
    );
    drop(removal);
    assert!(state.session_record("session").unwrap().locked);
    set_session_locked(&state, "session", false).unwrap();
    archive_session(&state, "session").unwrap();
    assert_eq!(status_of(&state, "session"), Some(SessionStatus::Archived));
}

#[test]
fn locking_an_archived_session_is_refused_but_unlocking_one_is_not() {
    use SessionStatus::{Archived, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-lock-archived");
    for (id, status) in [("live", Interrupted), ("old", Archived)] {
        state
            .store
            .insert_session(&console_session(id, status))
            .unwrap();
    }

    assert_eq!(
        code_of(&set_session_locked(&state, "old", true).unwrap_err()),
        error_code::SESSION_ARCHIVED
    );
    assert!(!state.session_record("old").unwrap().locked);

    state.store.set_session_locked("old", true).unwrap();
    assert!(!set_session_locked(&state, "old", false).unwrap().locked);

    assert!(set_session_locked(&state, "live", true).unwrap().locked);
}

#[test]
fn locked_sessions_refuse_archive_and_delete_cascades_without_partial_changes() {
    use SessionStatus::{Archived, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-locked-cascades");
    for session in [
        console_session("hub", Interrupted),
        lead_session("lead", Interrupted, "hub"),
        bound_session("worker", Interrupted, "lead"),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    set_session_locked(&state, "worker", true).unwrap();
    for id in ["worker", "lead", "hub"] {
        assert_eq!(
            code_of(&archive_session(&state, id).unwrap_err()),
            error_code::SESSION_LOCKED
        );
        assert_eq!(status_of(&state, id), Some(Interrupted));
    }
    set_session_locked(&state, "worker", false).unwrap();
    archive_session(&state, "hub").unwrap();
    // Straight to the store: the lock a session archived before locking was refused still carries.
    state.store.set_session_locked("worker", true).unwrap();
    assert_eq!(
        code_of(&delete_session(&state, "hub").unwrap_err()),
        error_code::SESSION_LOCKED
    );
    assert_eq!(
        code_of(&delete_archived_sessions(&state, "console-1", None, None).unwrap_err()),
        error_code::SESSION_LOCKED
    );
    for id in ["hub", "lead", "worker"] {
        assert_eq!(status_of(&state, id), Some(Archived));
    }
    set_session_locked(&state, "worker", false).unwrap();
    assert_eq!(delete_session(&state, "hub").unwrap().len(), 3);
}

#[test]
fn a_locked_unused_session_cannot_be_deleted() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-locked-unused");
    state
        .store
        .insert_session(&console_session("unused", SessionStatus::Idle))
        .unwrap();
    set_session_locked(&state, "unused", true).unwrap();
    assert_eq!(
        code_of(&delete_session(&state, "unused").unwrap_err()),
        error_code::SESSION_LOCKED
    );
    assert_eq!(
        code_of(&delete_unused_session(&state, "unused").unwrap_err()),
        error_code::SESSION_LOCKED
    );
}

/// Deleting an archived console session takes the archived sessions bound to it, at both
/// levels, and no others; an archived lead session takes its own archived sessions and leaves
/// its console session; one bound session on its own leaves the console session; a console
/// session that is not archived is refused and takes nothing with it.
#[tokio::test]
async fn deleting_an_archived_console_session_deletes_its_archived_bound_sessions() {
    use SessionStatus::{Archived, Idle, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-delete-cascade");
    for session in [
        console_session("hub", Archived),
        bound_session("a", Archived, "hub"),
        lead_session("b", Archived, "hub"),
        bound_session("of-b", Archived, "b"),
        bound_session("still-here", Interrupted, "hub"),
        console_session("other-hub", Archived),
        bound_session("c", Archived, "other-hub"),
        project_session("unbound", Agent::Claude, Archived),
        console_session("live-hub", Idle),
        bound_session("d", Archived, "live-hub"),
        lead_session("lead", Archived, "live-hub"),
        bound_session("of-lead", Archived, "lead"),
    ] {
        state.store.insert_session(&session).unwrap();
    }

    delete_session(&state, "hub").unwrap();
    assert_eq!(
        session_ids(&state),
        [
            "c",
            "d",
            "lead",
            "live-hub",
            "of-lead",
            "other-hub",
            "still-here",
            "unbound"
        ]
    );

    delete_session(&state, "lead").unwrap();
    assert_eq!(
        session_ids(&state),
        ["c", "d", "live-hub", "other-hub", "still-here", "unbound"],
        "the lead session takes its own and leaves its console session"
    );

    delete_session(&state, "c").unwrap();
    assert_eq!(
        session_ids(&state),
        ["d", "live-hub", "other-hub", "still-here", "unbound"],
        "the bound session alone leaves its console session"
    );

    let err = delete_session(&state, "live-hub").expect_err("sessions bound to it");
    assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE);
    assert_eq!(
        session_ids(&state),
        ["d", "live-hub", "other-hub", "still-here", "unbound"]
    );
}

/// The agent's records of the sessions a delete removed go through the trash function, Claude's
/// subagent directory with the transcript when there is one, unless a remaining session refers
/// to the same agent session; a missing record never reaches it and a failing move is not an
/// error.
#[test]
fn the_agent_records_of_deleted_sessions_are_trashed_unless_shared() {
    use SessionStatus::{Archived, Interrupted};
    let (state, dir) = console_with_project_and_accounts("coordinator-delete-trash-records");
    let account = dir.join("claude-account");
    let uuid = |name: &str| {
        let n = ["solo", "tail", "shared", "never-written"]
            .iter()
            .position(|each| *each == name)
            .unwrap();
        format!("00000000-0000-0000-0000-00000000000{n}")
    };
    let project_dir = account.join("projects").join("-work");
    std::fs::create_dir_all(project_dir.join(uuid("solo")).join("subagents")).unwrap();
    for name in ["solo", "tail", "shared"] {
        std::fs::write(project_dir.join(format!("{}.jsonl", uuid(name))), "{}\n").unwrap();
    }
    let native = |id: &str, native: &str, status| Session {
        agent_session_id: Some(uuid(native)),
        config_dir: Some(account.to_string_lossy().into_owned()),
        ..project_session(id, Agent::Claude, status)
    };
    for session in [
        native("solo", "solo", Archived),
        native("tail", "tail", Archived),
        native("copy-a", "shared", Archived),
        native("copy-b", "shared", Interrupted),
        native("unrecorded", "never-written", Archived),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let trashed = std::sync::Mutex::new(Vec::new());
    let trash = |path: &Path| {
        trashed.lock().unwrap().push(path.to_path_buf());
        Err(TrashFailure::new(path, "the Trash refused it"))
    };
    let locator = || {
        records::Locator::with_shell(HashMap::from([(
            "HOME".to_string(),
            dir.join("home").to_string_lossy().into_owned(),
        )]))
    };
    let deleted = delete_session(&state, "solo").unwrap();
    trash_agent_records_of(&state, &deleted, locator(), trash);
    assert_eq!(
        *trashed.lock().unwrap(),
        [
            project_dir.join(format!("{}.jsonl", uuid("solo"))),
            project_dir.join(uuid("solo"))
        ]
    );
    trashed.lock().unwrap().clear();
    let deleted = delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
    assert_eq!(deleted.len(), 3);
    trash_agent_records_of(&state, &deleted, locator(), trash);
    assert_eq!(
        *trashed.lock().unwrap(),
        [project_dir.join(format!("{}.jsonl", uuid("tail")))],
        "the shared copy, the unwritten record and the missing subagent directory are left"
    );
}

/// A Codex thread that had a turn reverted goes to the Trash whole: its first rollout and every
/// later segment, plain or compressed.
#[test]
fn a_reverted_codex_threads_whole_chain_is_trashed() {
    let (state, dir) = console_with_project_and_accounts("coordinator-delete-trash-chain");
    let account = dir.join("codex-account");
    let thread = "019a0000-0000-7000-8000-000000000001";
    let mut expected = Vec::new();
    for compressed in [false, true] {
        expected.extend(crate::test_support::codex_chain(&account, thread, compressed).all());
    }
    state
        .store
        .insert_session(&Session {
            agent_session_id: Some(thread.to_owned()),
            config_dir: Some(account.to_string_lossy().into_owned()),
            ..project_session("reverted", Agent::Codex, SessionStatus::Archived)
        })
        .unwrap();
    let deleted = delete_session(&state, "reverted").unwrap();
    let trashed = std::sync::Mutex::new(Vec::new());
    trash_agent_records_of(
        &state,
        &deleted,
        records::Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            dir.join("home").to_string_lossy().into_owned(),
        )])),
        |path| {
            trashed.lock().unwrap().push(path.to_path_buf());
            Ok(())
        },
    );
    let mut trashed = trashed.into_inner().unwrap();
    trashed.sort();
    expected.sort();
    assert_eq!(trashed, expected);
}

#[tokio::test]
async fn guarded_deletion_accepts_an_unused_codex_session_without_a_native_id() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-delete-unused-codex");
    state
        .store
        .insert_session(&project_session(
            "unused",
            Agent::Codex,
            SessionStatus::Idle,
        ))
        .unwrap();
    let mut events = state.subscribe();
    handle(
        &state,
        None,
        RequestBody::DeleteSession {
            session: "unused".to_owned(),
            only_if_unused: true,
            trash_agent_records: false,
            force_remove: Vec::new(),
            keep: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        events.try_recv().unwrap(),
        Event::SessionDeleted { session } if session == "unused"
    ));
    assert!(state.store.get_session("unused").unwrap().is_none());
}

#[tokio::test]
async fn guarded_deletion_accepts_and_trashes_unused_grok_native_records() {
    let (state, dir) = console_with_project_and_accounts("coordinator-delete-unused-grok");
    let account = dir.join("account");
    let native_id = "11111111-2222-3333-4444-555555555555";
    let records = account.join("sessions/work").join(native_id);
    std::fs::create_dir_all(&records).unwrap();
    std::fs::write(records.join("usage.json"), "{}").unwrap();
    std::fs::write(records.join("messages.json"), "[]").unwrap();
    state
        .store
        .insert_session(&Session {
            agent_session_id: Some(native_id.to_owned()),
            config_dir: Some(account.to_string_lossy().into_owned()),
            ..project_session("unused", Agent::Grok, SessionStatus::Idle)
        })
        .unwrap();
    let deleted = delete_unused_session(&state, "unused").unwrap();
    assert!(state.store.get_session("unused").unwrap().is_none());
    let trashed = dir.join("trash");
    trash_agent_records_of(
        &state,
        &deleted,
        records::Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            dir.join("home").to_string_lossy().into_owned(),
        )])),
        |path| {
            assert_eq!(path, records);
            std::fs::rename(path, &trashed).map_err(|err| TrashFailure::new(path, err))
        },
    );
    assert!(!records.exists());
    assert!(trashed.join("messages.json").exists());
}

#[tokio::test]
async fn unused_deletion_returns_the_native_identity_at_the_atomic_delete() {
    let (state, dir) = console_with_project_and_accounts("coordinator-delete-unused-identity");
    let initial = project_session("unused", Agent::Codex, SessionStatus::Idle);
    state.store.insert_session(&initial).unwrap();
    let stale = state.session_record("unused").unwrap();
    assert!(stale.agent_session_id.is_none());
    let native_id = "11111111-2222-3333-4444-555555555555";
    let account = dir.join("codex-account");
    let rollout = account
        .join("sessions/2026/10/17")
        .join(format!("rollout-2026-10-17T01-02-03-{native_id}.jsonl"));
    std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
    std::fs::write(&rollout, "{}\n").unwrap();
    let discovered = Session {
        agent_session_id: Some(native_id.to_owned()),
        config_dir: Some(account.to_string_lossy().into_owned()),
        ..initial
    };
    state.save_session(&discovered).unwrap();
    state
        .set_session_account("unused", None, discovered.config_dir.as_deref())
        .unwrap();
    let Deletion::Deleted { session, .. } = state.delete_if_unused("unused").unwrap() else {
        panic!("native-id discovery must leave an unused session deletable");
    };
    assert_eq!(session.agent_session_id, discovered.agent_session_id);
    assert_eq!(session.config_dir, discovered.config_dir);
    let trashed = dir.join("trashed-rollout.jsonl");
    trash_agent_records_of(
        &state,
        &[*session],
        records::Locator::with_shell(HashMap::from([(
            "HOME".to_owned(),
            dir.join("home").to_string_lossy().into_owned(),
        )])),
        |path| {
            assert_eq!(path, rollout);
            std::fs::rename(path, &trashed).map_err(|err| TrashFailure::new(path, err))
        },
    );
    assert!(!rollout.exists());
    assert!(trashed.exists());
}

#[tokio::test]
async fn guarded_deletion_refuses_a_first_turn_or_archived_owner_instead_of_cascading() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-delete-unused-race");
    let session = project_session("first-turn", Agent::Codex, SessionStatus::Idle);
    state.store.insert_session(&session).unwrap();
    let mut started = session.clone();
    started.has_conversation = true;
    started.status = SessionStatus::Archived;
    state.save_session(&started).unwrap();
    state
        .store
        .insert_session(&console_session("owner", SessionStatus::Archived))
        .unwrap();
    state
        .store
        .insert_session(&Session {
            origin: Origin::Console,
            ..bound_session("child", SessionStatus::Archived, "owner")
        })
        .unwrap();
    for id in ["first-turn", "owner", "child"] {
        let err = handle(
            &state,
            None,
            RequestBody::DeleteSession {
                session: id.to_owned(),
                only_if_unused: true,
                trash_agent_records: false,
                force_remove: Vec::new(),
                keep: Vec::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE, "{id}");
    }
    assert_eq!(session_ids(&state), ["child", "first-turn", "owner"]);
}

/// A session that is not archived is deleted only while it is unused: no turn yet, with none
/// bound to it, and bound to no owner or opened by the user. Deleting one stops its process;
/// any other is refused and keeps both its record and its process.
#[tokio::test]
async fn deleting_a_session_that_is_not_archived_needs_it_to_be_unused() {
    use SessionStatus::{Idle, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-delete-unused");
    for session in [
        project_session("fresh", Agent::Claude, Idle),
        project_session("never-used", Agent::Claude, Interrupted),
        Session {
            has_conversation: true,
            ..project_session("used", Agent::Claude, Idle)
        },
        console_session("hub", Idle),
        bound_session("opened-by-user", Idle, "hub"),
        Session {
            origin: Origin::Console,
            ..bound_session("started-by-agent", Idle, "hub")
        },
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let fresh = idle_stand_in("fresh");
    state.register_live(fresh.clone());
    let used = idle_stand_in("used");
    state.register_live(used.clone());
    let mut events = state.subscribe();

    delete_session(&state, "fresh").unwrap();
    assert!(matches!(
        events.try_recv().unwrap(),
        Event::SessionDeleted { session } if session == "fresh"
    ));
    let deadline = Instant::now() + crate::test_support::PATIENCE;
    while !fresh.poll_exit() {
        assert!(Instant::now() < deadline, "the process was never stopped");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let claim = state.begin_launch("never-used").unwrap();
    let err = delete_session(&state, "never-used").expect_err("starting");
    assert_eq!(code_of(&err), error_code::SESSION_NOT_DELETABLE);
    drop(claim);
    delete_session(&state, "never-used").unwrap();
    delete_unused_session(&state, "opened-by-user").unwrap();

    for refused in ["used", "hub", "started-by-agent"] {
        let err = delete_session(&state, refused).expect_err(refused);
        assert_eq!(
            code_of(&err),
            error_code::SESSION_NOT_DELETABLE,
            "{refused}"
        );
    }
    assert_eq!(session_ids(&state), ["hub", "started-by-agent", "used"]);
    assert!(!used.poll_exit());
}

/// Deleting every archived console session of a console takes their archived bound sessions
/// too, at both levels; deleting a project's archived sessions takes the bound ones among them
/// and leaves the console sessions; deleting a console session's archive takes its archived
/// bound sessions, a lead session's own included, and nothing else, and a project together
/// with a console session is refused.
#[tokio::test]
async fn deleting_archived_sessions_in_bulk_follows_the_binding_for_console_sessions_only() {
    use SessionStatus::Archived;
    let seed = |name: &str| {
        let (state, dir) = console_with_project_and_accounts(&format!("coordinator-{name}"));
        for session in [
            console_session("hub", Archived),
            bound_session("a", Archived, "hub"),
            lead_session("lead", Archived, "hub"),
            bound_session("of-lead", Archived, "lead"),
            project_session("unbound", Agent::Claude, Archived),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        (state, dir)
    };

    let (state, _dir) = seed("bulk-console-scope");
    delete_archived_sessions(&state, "console-1", None, None).unwrap();
    assert_eq!(session_ids(&state), ["unbound"]);

    let (state, _dir) = seed("bulk-project-scope");
    delete_archived_sessions(&state, "console-1", Some("project-1"), None).unwrap();
    assert_eq!(session_ids(&state), ["hub"]);

    let (state, _dir) = seed("bulk-bound-scope");
    delete_archived_sessions(&state, "console-1", None, Some("hub")).unwrap();
    assert_eq!(session_ids(&state), ["hub", "unbound"]);
    for (project, owner, code) in [
        (
            Some("project-1"),
            Some("hub"),
            error_code::CONFLICTING_FIELDS,
        ),
        (None, Some("unbound"), error_code::UNKNOWN_SESSION),
        (None, Some("nobody"), error_code::UNKNOWN_SESSION),
    ] {
        let refused =
            delete_archived_sessions(&state, "console-1", project, owner).expect_err("not a scope");
        assert_eq!(code_of(&refused), code);
    }
    assert_eq!(session_ids(&state), ["hub", "unbound"]);
}

/// Archiving a console session reaches through its lead session to the lead session's own
/// sessions: refused, naming it, while one of those has a process although the lead session
/// itself has none, and otherwise archiving both levels.
#[tokio::test]
async fn archiving_a_console_session_covers_its_lead_sessions_own_sessions() {
    use SessionStatus::{Archived, Idle, Interrupted, Working};
    let (state, _dir) = console_with_project_and_accounts("coordinator-archive-two-levels");
    for session in [
        console_session("hub", Idle),
        lead_session("lead", Interrupted, "hub"),
        bound_session("running", Working, "lead"),
        bound_session("dormant", Interrupted, "lead"),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let running = idle_stand_in("running");
    state.register_live(running.clone());

    let err = archive_session(&state, "hub").expect_err("a session two levels down runs");
    assert_eq!(code_of(&err), error_code::SESSION_HAS_RUNNING_SESSIONS);
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(coded.params["count"], "1");
    assert!(coded.params["sessions"].contains("running"));
    for (id, status) in [
        ("hub", Idle),
        ("lead", Interrupted),
        ("dormant", Interrupted),
    ] {
        assert_eq!(status_of(&state, id), Some(status), "{id}");
    }

    drop(running);
    state.store.delete_session("running").unwrap();
    archive_session(&state, "hub").unwrap();
    for id in ["hub", "lead", "dormant"] {
        assert_eq!(status_of(&state, id), Some(Archived), "{id}");
    }
}
