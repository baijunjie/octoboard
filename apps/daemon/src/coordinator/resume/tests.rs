use super::*;
use crate::coordinator::handle;
use crate::coordinator::launch::LaunchEdit;
use crate::coordinator::open::{open_session, OpenRequest};
use crate::coordinator::test_support::{
    archive_during_relaunch, bound_coordinator, bound_session, shell_env, status_of, stub_shell,
    temp_dir,
};
use crate::protocol::{
    error_code, Account, AgentId, CodedError, ConversationLocation, Event, Origin, RequestBody,
    Role, Session, SessionKind, SessionStatus,
};
use crate::session::term;
use crate::state::AppState;
use crate::test_support::code_of;
use crate::test_support::{
    console_session, console_with_project_and_accounts, idle_stand_in, project_session,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Reopening a session of an agent that has since been removed is not refused for that: its
/// settings and its place stay, so it fails here only for its own reason.
#[tokio::test]
async fn a_reopen_is_not_refused_for_an_agent_that_is_not_added() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-agent-removed");
    state
        .store
        .set_added_agents_if_undecided(&[AgentId::CODEX])
        .unwrap();
    state
        .store
        .insert_session(&Session {
            ended_at: Some(5),
            ..project_session("s", AgentId::CLAUDE, SessionStatus::Archived)
        })
        .unwrap();

    let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
        .await
        .expect_err("the project's directory does not exist");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
}

/// A conversation to resume is refused before anything is launched or written, each for its
/// own code; blank counts as none and the id is kept trimmed.
#[tokio::test]
async fn a_conversation_to_resume_is_checked_before_a_session_is_opened() {
    const ID: &str = "11111111-2222-3333-4444-555555555555";
    let (state, _dir) = console_with_project_and_accounts("coordinator-resume-id-checked");
    let mut holder = project_session("holder", AgentId::CLAUDE, SessionStatus::Archived);
    holder.agent_session_id = Some(ID.to_string());
    state.store.insert_session(&holder).unwrap();
    let check = |kind, project: Option<&str>, bound_to: Option<&str>, id: &str| {
        checked_resume_id(&state, kind, project, bound_to, Some(id.to_string()))
    };

    // (what is refused, the result, the code it is refused with)
    let cases = [
        (
            "a console session",
            check(SessionKind::Console, None, None, ID),
            error_code::RESUME_FOR_CONSOLE_SESSION,
        ),
        (
            "a session that reports to another",
            check(SessionKind::Project, Some("project-1"), Some("hub"), ID),
            error_code::CONFLICTING_FIELDS,
        ),
        (
            "an id that is not a UUID",
            check(SessionKind::Project, Some("project-1"), None, "my-title"),
            error_code::RESUME_ID_INVALID,
        ),
        (
            "an id a session of the project holds",
            check(SessionKind::Project, Some("project-1"), None, ID),
            error_code::CONVERSATION_IN_SESSION,
        ),
        (
            "an id a session of another project holds",
            check(SessionKind::Project, Some("project-2"), None, ID),
            error_code::CONVERSATION_OUTSIDE_PROJECT,
        ),
    ];
    for (what, result, code) in cases {
        assert_eq!(code_of(&result.expect_err(what)), code, "{what}");
    }
    // The session already holding it is named by id, which the UI words as its current title.
    let err = check(SessionKind::Project, Some("project-1"), None, ID).unwrap_err();
    assert_eq!(
        err.downcast_ref::<CodedError>().unwrap().params["session"],
        "holder"
    );
    let err = check(SessionKind::Project, Some("project-2"), None, ID).unwrap_err();
    assert_eq!(
        err.downcast_ref::<CodedError>().unwrap().params["session"],
        "holder"
    );
    let free = "99999999-2222-3333-4444-555555555555";
    assert_eq!(
        check(
            SessionKind::Project,
            Some("project-1"),
            None,
            &format!(" {} \n", free.to_uppercase())
        )
        .unwrap(),
        Some(free.to_string())
    );
    assert_eq!(check(SessionKind::Console, None, None, "  ").unwrap(), None);

    // Through `open_session`, where the refusal leaves no record of the one to open.
    let err = open_session(
        &state,
        OpenRequest {
            compatibility: Default::default(),
            console_id: "console-1".to_string(),
            project_id: Some("project-1".to_string()),
            agent: None,
            account: None,
            model: None,
            effort: None,
            task: None,
            title: None,
            origin: Origin::User,
            bound_to: None,
            resume_agent_session_id: Some("my-title".to_string()),
            session_id: None,
            terminal_appearance: None,
        },
    )
    .await
    .expect_err("refused");
    assert_eq!(code_of(&err), error_code::RESUME_ID_INVALID);
    assert_eq!(state.store.list_sessions().unwrap().len(), 1);
}

/// The conversation to resume is looked for in the directory the session will launch with: the
/// pinned one, else the default account's from the shell snapshot.
#[test]
fn a_conversation_to_resume_must_be_in_the_accounts_directory() {
    const ID: &str = "11111111-2222-3333-4444-555555555555";
    let dir = temp_dir("resume-conversation-located");
    let (pinned, default) = (dir.join("pinned"), dir.join("default-claude"));
    let no_snapshot = || panic!("a pinned account needs no snapshot");
    let snapshot = shell_env(&dir);

    let err = locate_conversation_to_resume(
        AgentId::CLAUDE,
        Some(&pinned.to_string_lossy()),
        ID,
        no_snapshot,
    )
    .expect_err("not there");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::CONVERSATION_TO_RESUME_NOT_FOUND);
    assert_eq!(coded.params["id"], ID);

    let record = default.join("projects/slug").join(format!("{ID}.jsonl"));
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "").unwrap();
    locate_conversation_to_resume(AgentId::CLAUDE, None, ID, snapshot).unwrap();
}

/// The conversation to resume is placed in the account whose record was written to last, in the
/// default account and the pinned ones of every agent offered, and nowhere when none holds it.
#[test]
fn a_conversation_to_resume_is_located_in_the_account_written_to_last() {
    const ID: &str = "11111111-2222-3333-4444-555555555555";
    let dir = temp_dir("resume-conversation-found");
    let account = |id: &str, agent, dir: PathBuf| Account {
        id: id.to_string(),
        agent,
        name: id.to_string(),
        config_dir: dir.to_string_lossy().into_owned(),
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled: false,
        versions: Default::default(),
    };
    let (work, other) = (dir.join("work"), dir.join("other"));
    let accounts = [
        account("work", AgentId::CLAUDE, work.clone()),
        account("other", AgentId::CLAUDE, other.clone()),
    ];
    let write_record = |config_dir: &Path, seconds: u64| {
        let record = config_dir.join("projects/slug").join(format!("{ID}.jsonl"));
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, "").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&record)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    };
    let find = |agents: &[AgentId]| {
        find_conversation_to_resume(agents, &accounts, ID, shell_env(&dir)).unwrap()
    };
    let in_account = |agent, account: &str| {
        Some(ConversationLocation::Record {
            agent,
            account: Some(account.to_string()),
        })
    };

    assert_eq!(find(&[AgentId::CLAUDE, AgentId::GROK]), None);

    write_record(&other, 100);
    assert_eq!(
        find(&[AgentId::CLAUDE]),
        in_account(AgentId::CLAUDE, "other")
    );
    assert_eq!(
        find(&[AgentId::CODEX, AgentId::GROK]),
        None,
        "only the agents offered"
    );

    write_record(&work, 200);
    assert_eq!(
        find(&[AgentId::CLAUDE]),
        in_account(AgentId::CLAUDE, "work")
    );

    // The default account's directory comes from the snapshot, which only a lookup that has a
    // default account to look in takes.
    write_record(&dir.join("default-claude"), 300);
    assert_eq!(
        find(&[AgentId::CLAUDE]),
        Some(ConversationLocation::Record {
            agent: AgentId::CLAUDE,
            account: None,
        })
    );

    // A shell that cannot be snapshotted leaves the default accounts out, not the stored ones;
    // it is the failure only when nothing was found.
    let no_shell = || -> Result<HashMap<String, String>> { anyhow::bail!("no login shell") };
    assert_eq!(
        find_conversation_to_resume(&[AgentId::CLAUDE], &accounts, ID, no_shell).unwrap(),
        in_account(AgentId::CLAUDE, "work"),
    );
    let other_id = "99999999-2222-3333-4444-555555555555";
    let err = find_conversation_to_resume(&[AgentId::CLAUDE], &accounts, other_id, no_shell)
        .expect_err("nothing found, and the default account unknown");
    assert!(format!("{err:#}").contains("no login shell"));

    // A Grok record in another agent's directory layout, written later still.
    let grok = dir.join("grok-home");
    let grok_account = account("grok", AgentId::GROK, grok.clone());
    std::fs::create_dir_all(grok.join("sessions/cwd").join(ID)).unwrap();
    let both = [accounts[0].clone(), grok_account];
    assert_eq!(
        find_conversation_to_resume(
            &[AgentId::CLAUDE, AgentId::GROK],
            &both,
            ID,
            shell_env(&dir)
        )
        .unwrap(),
        in_account(AgentId::GROK, "grok"),
    );
}

/// `locate_conversation_to_resume` names the session that records the conversation, by whose
/// it is and what state it is in, before any record on disk is looked at; an id that is no UUID
/// and a project that does not exist are refused, and a conversation nobody holds is not found.
#[tokio::test]
async fn locating_a_conversation_to_resume_names_the_session_that_holds_it() {
    const ID: &str = "11111111-2222-3333-4444-555555555555";
    let (state, _dir) = console_with_project_and_accounts("coordinator-locate-held");
    let locate = |project: &str, id: &str| {
        handle(
            &state,
            Some("req-1".to_string()),
            RequestBody::LocateConversationToResume {
                project: project.to_string(),
                agent_session_id: id.to_string(),
            },
        )
    };
    let located = |reply: Option<Event>| match reply {
        Some(Event::ConversationLocated { id, location }) => {
            assert_eq!(id.as_deref(), Some("req-1"));
            location
        }
        other => panic!("expected conversation_located, got {other:?}"),
    };
    let mut holder = project_session("holder", AgentId::CLAUDE, SessionStatus::Archived);
    holder.agent_session_id = Some(ID.to_string());
    state.store.insert_session(&holder).unwrap();
    let project = holder.project_id.clone().expect("a project session");
    let mut other = state.store.get_project(&project).unwrap().unwrap();
    other.id = "project-2".to_string();
    other.path.push_str("-2");
    state.store.insert_project(&other).unwrap();

    // Upper case, as the user may type it: held all the same.
    let archived = located(locate(&project, &ID.to_uppercase()).await.unwrap());
    assert_eq!(
        archived,
        ConversationLocation::ArchivedSession {
            session: "holder".to_string()
        }
    );

    for (status, project_id, expected) in [
        (
            SessionStatus::Idle,
            Some(project.as_str()),
            ConversationLocation::LiveSession {
                session: "holder".to_string(),
            },
        ),
        (
            SessionStatus::Archived,
            Some("project-2"),
            ConversationLocation::OutsideProject {
                session: "holder".to_string(),
            },
        ),
        (
            SessionStatus::Idle,
            None,
            ConversationLocation::OutsideProject {
                session: "holder".to_string(),
            },
        ),
    ] {
        let mut moved = holder.clone();
        moved.status = status;
        moved.project_id = project_id.map(str::to_string);
        state.store.delete_session("holder").unwrap();
        state.store.insert_session(&moved).unwrap();
        assert_eq!(located(locate(&project, ID).await.unwrap()), expected);
    }

    let err = locate(&project, "my-title").await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::RESUME_ID_INVALID);
    let err = locate("no-such-project", ID).await.expect_err("refused");
    assert_eq!(code_of(&err), error_code::UNKNOWN_PROJECT);
}

#[tokio::test]
async fn an_archive_during_resume_ends_a_late_process_and_survives_a_late_failure() {
    archive_during_relaunch(false, true).await;
    archive_during_relaunch(false, false).await;
}

/// A reopen whose launch fails leaves the session exactly as it was archived: the rollback
/// puts its status back, and the time it was archived at is not re-stamped, so its place in
/// the archive and the time its row reports both stay where they were.
#[tokio::test]
async fn a_reopen_that_fails_to_launch_keeps_the_time_it_was_archived_at() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-launch-refused");
    state
        .store
        .insert_session(&Session {
            ended_at: Some(5),
            ..project_session("s", AgentId::CLAUDE, SessionStatus::Archived)
        })
        .unwrap();

    let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
        .await
        .expect_err("the project's directory does not exist");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);

    let after = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Archived);
    assert_eq!(after.ended_at, Some(5));
}

/// A reopen that changes the model and effort refuses a value the agent does not offer before
/// anything is launched or recorded, and puts the session's own back when the launch fails.
#[tokio::test]
async fn a_reopen_with_another_model_is_checked_and_put_back_when_it_fails() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-launch-edit");
    state
        .store
        .insert_session(&Session {
            ended_at: Some(5),
            model: Some(Some("sonnet".to_string())),
            ..project_session("s", AgentId::CLAUDE, SessionStatus::Archived)
        })
        .unwrap();
    let edit = |model: &str, effort: Option<&str>| LaunchEdit {
        compatibility: Default::default(),
        model: Some(Some(model.to_string())),
        effort: effort.map(|effort| Some(effort.to_string())),
    };

    let err = resume_session_with_launch(
        &state,
        "s",
        None,
        None,
        edit("gpt-9", None),
        stub_shell(&dir),
        term::launch,
    )
    .await
    .expect_err("model not offered");
    assert_eq!(code_of(&err), error_code::INVALID_LAUNCH_SETTING);
    let err = resume_session_with_launch(
        &state,
        "s",
        None,
        None,
        edit("opus", Some("extreme")),
        stub_shell(&dir),
        term::launch,
    )
    .await
    .expect_err("effort not offered");
    assert_eq!(code_of(&err), error_code::INVALID_LAUNCH_SETTING);
    let after = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(after.model, Some(Some("sonnet".to_string())));

    // Offered, so recorded for the launch, which fails: the project's directory does not exist.
    let err = resume_session_with_launch(
        &state,
        "s",
        None,
        None,
        edit("opus", Some("high")),
        stub_shell(&dir),
        term::launch,
    )
    .await
    .expect_err("the project's directory does not exist");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    let after = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Archived);
    assert_eq!(
        (after.model, after.effort),
        (Some(Some("sonnet".to_string())), None)
    );
}

const AGENT_ID: &str = "11111111-2222-3333-4444-555555555555";

/// An archived session of `agent` with a conversation, pinned to `account`, archived at 5.
fn archived_with_conversation(id: &str, agent: AgentId, account: &Path) -> Session {
    Session {
        ended_at: Some(5),
        has_conversation: true,
        agent_session_id: Some(AGENT_ID.to_string()),
        config_dir: Some(account.to_string_lossy().into_owned()),
        ..project_session(id, agent, SessionStatus::Archived)
    }
}

fn assert_still_archived_at_5(state: &Arc<AppState>, id: &str) {
    let after = state.store.get_session(id).unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Archived);
    assert_eq!(after.ended_at, Some(5));
}

/// A reopen of a session whose conversation the agent no longer has is refused before any
/// launch, leaving the session archived with its archive time.
#[tokio::test]
async fn a_reopen_is_refused_when_the_conversation_record_is_gone() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-unresumable");
    let account = dir.join("claude-a");
    std::fs::create_dir_all(account.join("projects")).unwrap();
    state
        .store
        .insert_session(&archived_with_conversation("s", AgentId::CLAUDE, &account))
        .unwrap();

    let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
        .await
        .expect_err("no transcript");
    assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
    assert_still_archived_at_5(&state, "s");

    // The record's presence lets the reopen go as far as the launch, which this fixture's
    // missing project directory then refuses.
    let transcript = account.join(format!("projects/work/{AGENT_ID}.jsonl"));
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(&transcript, "{}\n").unwrap();
    let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
        .await
        .expect_err("no cwd");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
}

/// The check made before a request to resume is put to the user raises what the resume would
/// raise for a conversation the agent no longer has, changes nothing, and otherwise names the
/// archived owners that would come back with the session.
#[tokio::test]
async fn the_check_before_asking_to_resume_refuses_a_conversation_that_is_gone() {
    let (state, dir) = console_with_project_and_accounts("coordinator-check-can-resume");
    let account = dir.join("claude-a");
    std::fs::create_dir_all(account.join("projects")).unwrap();
    let mut owner = archived_with_conversation("owner", AgentId::CLAUDE, &account);
    owner.agent_session_id = Some("owner-conversation".to_string());
    state.store.insert_session(&owner).unwrap();
    let mut held = archived_with_conversation("s", AgentId::CLAUDE, &account);
    held.bound_to = Some("owner".to_string());
    held.role = Role::Worker;
    state.store.insert_session(&held).unwrap();

    let err = check_can_resume_with(&state, "s", stub_shell(&dir))
        .await
        .expect_err("no transcript");
    assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
    assert_still_archived_at_5(&state, "s");

    let transcript = account.join(format!("projects/work/{AGENT_ID}.jsonl"));
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(&transcript, "{}\n").unwrap();
    let owners = check_can_resume_with(&state, "s", stub_shell(&dir))
        .await
        .unwrap();
    assert_eq!(owners, ["owner"]);
}

/// Only proven absence refuses: a Grok session folder missing locally may be restored by Grok.
#[tokio::test]
async fn a_reopen_of_a_record_that_cannot_be_proven_gone_still_launches() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-unknown");
    let account = dir.join("grok-a");
    std::fs::create_dir_all(account.join("sessions")).unwrap();
    state
        .store
        .insert_session(&archived_with_conversation("s", AgentId::GROK, &account))
        .unwrap();

    let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
        .await
        .expect_err("no cwd");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
}

/// An archived console session `hub` with a bound `worker`. `hub`'s workdir is never created
/// (the fixture's console points at one that does not exist), so a reopen that reaches `hub`'s
/// launch stops at the missing directory instead of starting the real `claude`.
fn insert_hub_and_worker(state: &Arc<AppState>, hub_account: &Path, worker_account: &Path) {
    for session in [
        Session {
            ended_at: Some(5),
            has_conversation: true,
            agent_session_id: Some("22222222-2222-3333-4444-555555555555".to_string()),
            config_dir: Some(hub_account.to_string_lossy().into_owned()),
            ..console_session("hub", SessionStatus::Archived)
        },
        Session {
            bound_to: Some("hub".to_string()),
            role: Role::Worker,
            ..archived_with_conversation("worker", AgentId::CLAUDE, worker_account)
        },
    ] {
        state.store.insert_session(&session).unwrap();
    }
}

/// The session to reopen is checked before any owner is brought back, so a session whose
/// record is gone leaves the whole group archived.
#[tokio::test]
async fn a_reopen_checks_the_session_before_reopening_any_owner() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-target-unresumable");
    let hub_account = dir.join("claude-hub");
    let record = hub_account.join("projects/work/22222222-2222-3333-4444-555555555555.jsonl");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "{}\n").unwrap();
    let worker_account = dir.join("claude-worker");
    std::fs::create_dir_all(worker_account.join("projects")).unwrap();
    insert_hub_and_worker(&state, &hub_account, &worker_account);

    let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
        .await
        .expect_err("the session's transcript is gone");
    assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
    assert_still_archived_at_5(&state, "hub");
    assert_still_archived_at_5(&state, "worker");
}

/// An owner whose record is gone is forgotten only when the reopen goes on: with the session
/// itself refused too, the owner keeps its conversation.
#[tokio::test]
async fn a_refused_reopen_leaves_an_unresumable_owner_with_its_conversation() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-both-unresumable");
    let (hub_account, worker_account) = (dir.join("claude-hub"), dir.join("claude-worker"));
    std::fs::create_dir_all(hub_account.join("projects")).unwrap();
    std::fs::create_dir_all(worker_account.join("projects")).unwrap();
    insert_hub_and_worker(&state, &hub_account, &worker_account);

    let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
        .await
        .expect_err("neither transcript exists");
    assert_eq!(code_of(&err), error_code::CONVERSATION_UNRESUMABLE);
    let hub = state.store.get_session("hub").unwrap().unwrap();
    assert!(hub.has_conversation);
    assert_eq!(
        hub.agent_session_id.as_deref(),
        Some("22222222-2222-3333-4444-555555555555")
    );
    assert_still_archived_at_5(&state, "hub");
    assert_still_archived_at_5(&state, "worker");
}

/// An owner whose record is gone is relaunched into a fresh conversation, and the reopen goes
/// on instead of being refused. The launch here stops at the owner's missing workdir, after
/// the conversation was forgotten.
#[tokio::test]
async fn a_reopen_relaunches_an_owner_whose_record_is_gone_into_a_fresh_conversation() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-owner-unresumable");
    let hub_account = dir.join("claude-hub");
    std::fs::create_dir_all(hub_account.join("projects")).unwrap();
    let worker_account = dir.join("claude-worker");
    let record = worker_account.join(format!("projects/work/{AGENT_ID}.jsonl"));
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "{}\n").unwrap();
    insert_hub_and_worker(&state, &hub_account, &worker_account);

    let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
        .await
        .expect_err("the owner's workdir does not exist");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    assert_still_archived_at_5(&state, "hub");
    let hub = state.store.get_session("hub").unwrap().unwrap();
    assert!(!hub.has_conversation);
    assert_eq!(hub.agent_session_id, None);
    assert_still_archived_at_5(&state, "worker");
}

/// A target whose `codex unarchive` fails is refused before any owner is relaunched, so the
/// archived owner above it stays archived too.
#[tokio::test]
async fn a_reopen_whose_target_cannot_be_unarchived_leaves_the_owner_archived() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-target-unarchive");
    let hub_account = dir.join("claude-hub");
    let hub_record = hub_account.join("projects/work/22222222-2222-3333-4444-555555555555.jsonl");
    std::fs::create_dir_all(hub_record.parent().unwrap()).unwrap();
    std::fs::write(&hub_record, "{}\n").unwrap();
    let account = dir.join("codex-a");
    let archived = account.join(format!(
        "archived_sessions/rollout-2026-10-01T00-00-00-{AGENT_ID}.jsonl.zst"
    ));
    std::fs::create_dir_all(archived.parent().unwrap()).unwrap();
    std::fs::write(&archived, "x").unwrap();
    insert_hub_and_worker(&state, &hub_account, &dir.join("claude-worker"));
    state.store.delete_session_if_archived("worker").unwrap();
    state
        .store
        .insert_session(&Session {
            bound_to: Some("hub".to_string()),
            role: Role::Worker,
            ..archived_with_conversation("worker", AgentId::CODEX, &account)
        })
        .unwrap();

    let err = resume_session_with(&state, "worker", None, None, fake_codex(&dir, false))
        .await
        .expect_err("unarchive fails");
    assert_eq!(
        code_of(&err),
        crate::protocol::Code::of_provider(crate::providers::codex::UNARCHIVE_FAILED)
    );
    assert_still_archived_at_5(&state, "hub");
    assert_still_archived_at_5(&state, "worker");
    assert!(archived.exists());
}

/// The leftovers of the conversation an owner gives up go to the Trash, wherever the
/// accounts hold a copy of it; a copy the owner's own account no longer has does not make the
/// owner resumable.
#[tokio::test]
async fn a_reopen_trashes_the_leftovers_of_the_conversation_an_owner_gives_up() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-owner-leftover");
    let hub_account = dir.join("claude-hub");
    std::fs::create_dir_all(hub_account.join("projects")).unwrap();
    let leftover = dir
        .join("claude-a")
        .join("projects/work/22222222-2222-3333-4444-555555555555.jsonl");
    std::fs::create_dir_all(leftover.parent().unwrap()).unwrap();
    std::fs::write(&leftover, "{}\n").unwrap();
    let worker_account = dir.join("claude-worker");
    let record = worker_account.join(format!("projects/work/{AGENT_ID}.jsonl"));
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "{}\n").unwrap();
    insert_hub_and_worker(&state, &hub_account, &worker_account);

    let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
        .await
        .expect_err("the owner's workdir does not exist");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    assert!(!leftover.exists(), "the Trash stand-in removed it");
    assert!(record.exists(), "the session's own record is untouched");
}

/// A pinned account directory that cannot be read proves nothing, so the reopen goes on to the
/// launch, which refuses it for its own reason.
#[tokio::test]
async fn a_reopen_with_an_unreadable_account_directory_is_left_to_the_launch() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-account-gone");
    state
        .store
        .insert_session(&archived_with_conversation(
            "s",
            AgentId::CLAUDE,
            &dir.join("gone"),
        ))
        .unwrap();

    let err = resume_session_with(&state, "s", None, None, stub_shell(&dir))
        .await
        .expect_err("no cwd");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    assert_still_archived_at_5(&state, "s");
}

/// A `codex` that reports the home it ran under and moves the archived rollout back, or fails.
fn fake_codex(dir: &Path, succeeds: bool) -> impl Fn() -> Result<HashMap<String, String>> {
    let body = if succeeds {
        format!(
            "echo \"$CODEX_HOME $*\" > '{log}'\nmkdir -p \"$CODEX_HOME/sessions/2026/10/01\"\nmv \"$CODEX_HOME\"/archived_sessions/* \"$CODEX_HOME/sessions/2026/10/01/\"\n",
            log = dir.join("unarchive.log").display()
        )
    } else {
        "echo 'thread is not archived' >&2\nexit 1\n".to_string()
    };
    let env = crate::test_support::fake_agent(dir, "codex", &body);
    move || Ok(env.clone())
}

/// A Codex rollout only in `archived_sessions/` is brought back under the session's own
/// account before the resume; a failed `codex unarchive` is its own coded error and the
/// session stays as it was.
#[tokio::test]
async fn a_reopen_unarchives_a_codex_rollout_under_the_sessions_own_account() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-unarchive");
    let account = dir.join("codex-a");
    let rollout = format!("rollout-2026-10-01T00-00-00-{AGENT_ID}.jsonl.zst");
    let archived = account.join("archived_sessions").join(&rollout);
    std::fs::create_dir_all(archived.parent().unwrap()).unwrap();
    std::fs::write(&archived, "x").unwrap();
    state
        .store
        .insert_session(&archived_with_conversation("s", AgentId::CODEX, &account))
        .unwrap();

    let err = resume_session_with(&state, "s", None, None, fake_codex(&dir, false))
        .await
        .expect_err("unarchive fails");
    assert_eq!(
        code_of(&err),
        crate::protocol::Code::of_provider(crate::providers::codex::UNARCHIVE_FAILED)
    );
    let coded = err.downcast_ref::<CodedError>().unwrap();
    assert_eq!(
        coded.params.get("detail").map(String::as_str),
        Some("thread is not archived")
    );
    assert_still_archived_at_5(&state, "s");
    assert!(archived.exists());

    // The unarchive done, the reopen goes on to the launch, which this fixture refuses.
    let err = resume_session_with(&state, "s", None, None, fake_codex(&dir, true))
        .await
        .expect_err("no cwd");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    assert!(!archived.exists());
    assert!(account.join("sessions/2026/10/01").join(&rollout).exists());
    let log = std::fs::read_to_string(dir.join("unarchive.log")).unwrap();
    assert_eq!(
        log.trim(),
        format!("{} unarchive {AGENT_ID}", account.display())
    );
    assert_still_archived_at_5(&state, "s");
}

/// Which owners a relaunch brings back first, topmost first: the archived owner a session without a
/// process is bound to and, when that is an archived bound coordinator, its archived console
/// session above it. The walk stops at the first owner that is not archived — only interrupted,
/// running or gone — and nothing is brought back for an unbound session, for one with a process, or
/// for a console session, so reopening an owner reopens nothing bound to it.
#[test]
fn a_reopen_brings_back_the_archived_owners_above_it_topmost_first() {
    use SessionStatus::{Archived, Idle, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-archived-owners");
    for session in [
        console_session("archived-hub", Archived),
        console_session("interrupted-hub", Interrupted),
        console_session("running-hub", Idle),
        bound_coordinator("archived-coordinator", Archived, "archived-hub"),
        bound_coordinator("interrupted-coordinator", Interrupted, "archived-hub"),
        bound_coordinator("coordinator-under-running-hub", Archived, "running-hub"),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    // (the session's status, what it is bound to, the owners it brings back)
    let cases: [(&str, SessionStatus, Option<&str>, &[&str]); 10] = [
        (
            "archived",
            Archived,
            Some("archived-hub"),
            &["archived-hub"],
        ),
        (
            "interrupted",
            Interrupted,
            Some("archived-hub"),
            &["archived-hub"],
        ),
        ("owner interrupted", Archived, Some("interrupted-hub"), &[]),
        ("owner running", Archived, Some("running-hub"), &[]),
        ("owner gone", Archived, Some("no-such-hub"), &[]),
        ("unbound", Archived, None, &[]),
        ("with a process", Idle, Some("archived-hub"), &[]),
        (
            "both levels archived",
            Archived,
            Some("archived-coordinator"),
            &["archived-hub", "archived-coordinator"],
        ),
        (
            "coordinator interrupted",
            Archived,
            Some("interrupted-coordinator"),
            &[],
        ),
        (
            "coordinator's owner running",
            Archived,
            Some("coordinator-under-running-hub"),
            &["coordinator-under-running-hub"],
        ),
    ];
    for (name, status, owner, expected) in cases {
        let session = Session {
            bound_to: owner.map(str::to_string),
            ..project_session("s", AgentId::CLAUDE, status)
        };
        state.store.insert_session(&session).unwrap();
        assert_eq!(archived_owners(&state, "s").unwrap(), expected, "{name}");
        state.store.delete_session("s").unwrap();
    }
    assert!(archived_owners(&state, "archived-hub").unwrap().is_empty());
}

/// An owner is left to the reopen while it is archived or a launch of it is in flight (the
/// claim then refuses the reopen); one that is idle, running or interrupted and not in flight
/// is skipped.
#[test]
fn a_reopen_skips_an_owner_that_is_neither_archived_nor_being_launched() {
    use SessionStatus::{Archived, Idle, Interrupted};
    let (state, _dir) = console_with_project_and_accounts("coordinator-needs-reopening");
    for session in [
        console_session("archived", Archived),
        console_session("idle", Idle),
        console_session("interrupted", Interrupted),
        console_session("launching", Idle),
        console_session("running", Idle),
    ] {
        state.store.insert_session(&session).unwrap();
    }
    let _claim = state.begin_launch("launching").unwrap();
    // Brought back by a sibling reopen, whose claim is already released.
    let running = idle_stand_in("running");
    state.register_live(running.clone());

    let needs = |id: &str| needs_reopening(&state, id).unwrap();
    assert!(needs("archived"));
    assert!(needs("launching"));
    assert!(!needs("idle"));
    assert!(!needs("running"));
    assert!(!needs("interrupted"));
}

/// The topmost owner is relaunched first, and the reopen fails as a whole when it cannot be:
/// nothing below it is attempted (the ended times are untouched) and all of them stay
/// archived. The console's working directory is missing here and the project's is not, so the
/// failure can only be the console session's.
#[tokio::test]
async fn a_reopen_fails_as_a_whole_when_its_topmost_owner_cannot_be_reopened() {
    let (state, dir) = console_with_project_and_accounts("coordinator-reopen-owner-fails");
    std::fs::create_dir_all(dir.join("no-such-project")).unwrap();
    for session in [
        console_session("hub", SessionStatus::Archived),
        Session {
            ended_at: Some(5),
            ..bound_coordinator("coordinator", SessionStatus::Archived, "hub")
        },
        Session {
            ended_at: Some(5),
            ..bound_session("worker", SessionStatus::Archived, "coordinator")
        },
    ] {
        state.store.insert_session(&session).unwrap();
    }

    let err = resume_session_with(&state, "worker", None, None, stub_shell(&dir))
        .await
        .expect_err("the console session cannot launch");

    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    assert_eq!(status_of(&state, "hub"), Some(SessionStatus::Archived));
    for id in ["coordinator", "worker"] {
        let session = state.store.get_session(id).unwrap().unwrap();
        assert_eq!(
            (session.status, session.ended_at),
            (SessionStatus::Archived, Some(5)),
            "{id}"
        );
    }
    assert!(["hub", "coordinator", "worker"]
        .iter()
        .all(|id| !state.has_process(id)));
}
