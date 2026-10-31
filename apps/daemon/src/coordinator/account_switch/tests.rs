use super::*;
use crate::coordinator::accounts::set_account_disabled;
use crate::coordinator::launch::stop_process;
use crate::coordinator::launch::LaunchEdit;
use crate::coordinator::restart::keep_what_a_failed_restart_carried;
use crate::coordinator::test_support::{
    archive_during_relaunch, bound_session, code_of, shell_env, status_of, temp_dir,
};
use crate::protocol::{
    error_code, Account, Agent, Event, OrchestrationRole, Role, Session, SessionStatus,
};
use crate::term;
use crate::test_support::{
    bare_session, console_session, console_with_project_and_accounts, fake_live_session,
    idle_stand_in, project_session,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A switch onto a disabled account is refused before anything is recorded or stopped.
#[tokio::test]
async fn a_switch_onto_a_disabled_account_is_refused() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-disabled");
    state
        .store
        .insert_session(&project_session(
            "s",
            Agent::Claude,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    set_account_disabled(&state, Agent::Claude, Some("claude-a"), true).unwrap();

    let err = switch_session_account(
        &state,
        "s",
        Some(Some("claude-a".to_string())),
        None,
        shell_env(&dir),
    )
    .await
    .expect_err("refused");
    assert_eq!(code_of(&err), error_code::ACCOUNT_DISABLED);
    let after = state.store.get_session("s").unwrap().unwrap();
    assert_eq!(after.account_id, None);
    assert_eq!(after.status, SessionStatus::Interrupted);
}

/// Every refusal that can be told before the process is ended names its reason and records
/// nothing: the session keeps its account, its directory and its status, and a Grok home that
/// was never initialized is not given a `sessions/` directory by the refused attempt.
#[tokio::test]
async fn a_refused_switch_records_nothing() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-refused");
    std::fs::create_dir_all(dir.join("grok-a")).unwrap();

    // (what is refused, the session, the request's account, the code it is refused with)
    let archived = project_session("s", Agent::Claude, SessionStatus::Archived);
    let mut with_conversation = project_session("s", Agent::Claude, SessionStatus::Interrupted);
    with_conversation.has_conversation = true;
    with_conversation.agent_session_id = Some("agent-1".to_string());
    let cases = [
        (
            "an account that does not exist",
            project_session("s", Agent::Claude, SessionStatus::Interrupted),
            Some(Some("nope".to_string())),
            error_code::UNKNOWN_ACCOUNT,
        ),
        (
            "another agent's account",
            project_session("s", Agent::Claude, SessionStatus::Interrupted),
            Some(Some("codex-a".to_string())),
            error_code::UNKNOWN_ACCOUNT,
        ),
        (
            "the account it is on",
            project_session("s", Agent::Claude, SessionStatus::Interrupted),
            Some(None),
            error_code::SESSION_ALREADY_ON_ACCOUNT,
        ),
        (
            "no account named at all",
            project_session("s", Agent::Claude, SessionStatus::Interrupted),
            None,
            error_code::FIELD_REQUIRED,
        ),
        (
            "an archived session",
            archived,
            Some(Some("claude-a".to_string())),
            error_code::SESSION_ARCHIVED,
        ),
        (
            "a conversation that is not in its account",
            with_conversation,
            Some(Some("claude-a".to_string())),
            error_code::CONVERSATION_NOT_FOUND,
        ),
        (
            "a Grok home that was never initialized",
            project_session("s", Agent::Grok, SessionStatus::Interrupted),
            Some(Some("grok-a".to_string())),
            error_code::GROK_HOME_NOT_INITIALIZED,
        ),
    ];
    for (why, session, account, code) in cases {
        state.store.delete_session("s").unwrap();
        state.store.insert_session(&session).unwrap();

        let err = switch_session_account(&state, "s", account, None, shell_env(&dir))
            .await
            .expect_err(why);
        assert_eq!(code_of(&err), code, "{why}");

        let after = state.store.get_session("s").unwrap().unwrap();
        assert_eq!(after.account_id, session.account_id, "{why}");
        assert_eq!(after.config_dir, session.config_dir, "{why}");
        assert_eq!(after.status, session.status, "{why}");
    }
    assert!(!dir.join("grok-a/sessions").exists());
    assert!(!dir.join("claude-a").exists());

    // A launch, resume or switch of the session under way refuses another switch of it.
    state.store.delete_session("s").unwrap();
    state
        .store
        .insert_session(&project_session(
            "s",
            Agent::Claude,
            SessionStatus::Interrupted,
        ))
        .unwrap();
    let claim = state.begin_launch("s").unwrap();
    let err = switch_session_account(
        &state,
        "s",
        Some(Some("claude-a".to_string())),
        None,
        shell_env(&dir),
    )
    .await
    .expect_err("launching");
    assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    drop(claim);
}

/// The ownership an owner's tool checked before the switch claimed the session is checked again
/// under the claim: a session handed to another owner meanwhile is refused and left alone.
#[tokio::test]
async fn a_session_handed_to_another_owner_meanwhile_is_not_switched() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-handed-over");
    let mut worker = bound_session("worker", SessionStatus::Idle, "hub");
    worker.has_conversation = true;
    state.store.insert_session(&worker).unwrap();
    let live = idle_stand_in("worker");
    state.register_live(live.clone());
    state.watch_exit(live.clone());

    let err = switch_session_account_with(
        &state,
        "worker",
        Some(Some("claude-a".to_string())),
        Some("other-hub"),
        None,
        LaunchEdit::default(),
        shell_env(&dir),
        |_| panic!("a refused switch launches nothing"),
    )
    .await
    .expect_err("no longer the asker's");

    assert!(err.to_string().contains("no longer yours"), "{err}");
    assert!(!live.poll_exit(), "the process was left running");
    let after = state.store.get_session("worker").unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Idle);
    assert_eq!((after.account_id, after.config_dir), (None, None));
}

/// The default account resolves from one shell snapshot, taken once, and that same snapshot is
/// what the relaunch is handed; an account with a directory of its own needs none. A session
/// with no conversation has nothing to relocate whichever side is the default.
#[test]
fn the_default_account_is_resolved_once_for_the_copy_and_the_relaunch() {
    let dir = temp_dir("switch-prepare");
    let default_dir = dir.join("default-claude");
    let record = default_dir.join("projects/slug/agent-1.jsonl");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "conversation").unwrap();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let snapshot = || {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(HashMap::from([(
            "CLAUDE_CONFIG_DIR".to_string(),
            default_dir.to_string_lossy().into_owned(),
        )]))
    };
    let target = dir.join("claude-a").to_string_lossy().into_owned();

    let plan = prepare_switch(
        Agent::Claude,
        Some("agent-1"),
        None,
        Some(&target),
        snapshot,
    )
    .unwrap();
    let relocation = plan.relocation.expect("a conversation to relocate");
    assert_eq!(
        relocation,
        (
            default_dir.clone(),
            PathBuf::from(&target),
            PathBuf::from("projects/slug/agent-1.jsonl"),
            "agent-1".to_owned()
        )
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        plan.shell_env.expect("kept for the relaunch")["CLAUDE_CONFIG_DIR"],
        default_dir.to_string_lossy()
    );

    // Both sides pinned: no default to resolve, so no snapshot.
    let plan = prepare_switch(Agent::Claude, None, Some(&target), Some(&target), || {
        panic!("no snapshot needed")
    })
    .unwrap();
    assert!(plan.shell_env.is_none() && plan.relocation.is_none());

    // The default account as the target, for a session with no conversation: still one
    // snapshot, for the launch, and nothing to copy.
    let plan = prepare_switch(Agent::Claude, None, Some(&target), None, snapshot).unwrap();
    assert!(plan.shell_env.is_some() && plan.relocation.is_none());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

/// A Codex session whose rollout Codex has compressed is planned for the copy under its
/// `.jsonl.zst` name.
#[test]
fn a_compressed_codex_rollout_is_planned_for_a_switch() {
    const THREAD: &str = "019a0000-0000-7000-8000-000000000001";
    let dir = temp_dir("switch-prepare-compressed");
    let (source, target) = (dir.join("codex-a"), dir.join("codex-b"));
    let record = format!("sessions/2026/10/01/rollout-2026-10-01T01-02-03-{THREAD}.jsonl.zst");
    std::fs::create_dir_all(source.join(&record).parent().unwrap()).unwrap();
    std::fs::write(source.join(&record), "bytes").unwrap();
    let (source_dir, target_dir) = (source.to_string_lossy(), target.to_string_lossy());
    let plan = |id| {
        prepare_switch(
            Agent::Codex,
            Some(id),
            Some(&source_dir),
            Some(&target_dir),
            // Both accounts are pinned, yet a Codex switch snapshots the environment: it says
            // where the target keeps its database.
            || {
                Ok(HashMap::from([(
                    "CODEX_SQLITE_HOME".to_owned(),
                    "db".to_owned(),
                )]))
            },
        )
    };

    let planned = plan(THREAD).unwrap();
    assert_eq!(
        planned.relocation,
        Some((source, target, PathBuf::from(&record), THREAD.to_owned()))
    );
    assert_eq!(planned.shell_env.unwrap()["CODEX_SQLITE_HOME"], "db");
}

/// A relaunch that is refused after the conversation was copied and the account recorded puts
/// the account back, leaves the session interrupted rather than archived, and leaves the
/// conversation in the account it came from as well as the one it was copied to. The
/// switched session here is a live console session, whose process is ended on the way: the
/// session bound to it is not touched, which is what keeps a switch from ever being an
/// archive.
#[tokio::test]
async fn a_switch_whose_relaunch_fails_is_back_on_its_account_and_archives_nothing() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-relaunch-refused");
    let source = dir.join("default-claude");
    let record = "projects/slug/agent-1.jsonl";
    std::fs::create_dir_all(source.join(record).parent().unwrap()).unwrap();
    std::fs::write(source.join(record), "conversation").unwrap();

    let mut hub = bare_session("hub", Role::Console);
    hub.has_conversation = true;
    hub.agent_session_id = Some("agent-1".to_string());
    hub.status = SessionStatus::Working;
    state.store.insert_session(&hub).unwrap();
    let mut bound = project_session("bound", Agent::Claude, SessionStatus::Interrupted);
    bound.bound_to = Some("hub".to_string());
    bound.orchestration_role = OrchestrationRole::Worker;
    state.store.insert_session(&bound).unwrap();
    let live = idle_stand_in("hub");
    state.register_live(live.clone());
    state.watch_exit(live.clone());

    let err = switch_session_account(
        &state,
        "hub",
        Some(Some("claude-a".into())),
        None,
        shell_env(&dir),
    )
    .await
    .expect_err("the console's working directory does not exist");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);

    assert!(live.poll_exit(), "its process was ended");
    let after = state.store.get_session("hub").unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Interrupted);
    assert_eq!((after.account_id, after.config_dir), (None, None));
    assert_eq!(
        state.store.get_session("bound").unwrap().unwrap().status,
        SessionStatus::Interrupted
    );
    assert!(source.join(record).exists(), "copied, not moved");
    assert_eq!(
        std::fs::read_to_string(dir.join("claude-a").join(record)).unwrap(),
        "conversation"
    );
}

/// A switch of a reverted Codex thread carries the whole chain into the target and points the
/// target's database row of the thread, which would otherwise name an older head, at the head;
/// a target whose database cannot be updated fails the switch before any relaunch and leaves
/// the session on its account.
#[tokio::test]
async fn a_codex_switch_carries_the_chain_and_repoints_the_targets_stale_row() {
    use rusqlite::Connection;
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-codex-chain");
    let (source, target) = (dir.join("codex-old"), dir.join("codex-a"));
    let broken = dir.join("codex-b");
    state
        .store
        .insert_account(&Account {
            id: "codex-b".to_string(),
            agent: Agent::Codex,
            name: "codex-b".to_string(),
            config_dir: broken.to_string_lossy().into_owned(),
            share_default_config: false,
            share_default_config_incomplete: false,
            disabled: false,
        })
        .unwrap();
    let id = "019a0000-0000-7000-8000-000000000001";
    let chain = crate::test_support::codex_chain(&source, id, false);
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(
        broken.join("state_5.sqlite"),
        "not a database at all, but long",
    )
    .unwrap();
    let database = target.join("state_5.sqlite");
    Connection::open(&database)
        .unwrap()
        .execute_batch(&format!(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT);
                 INSERT INTO threads VALUES ('{id}', 'older head');"
        ))
        .unwrap();
    let mut codex = project_session("codex", Agent::Codex, SessionStatus::Idle);
    codex.has_conversation = true;
    codex.agent_session_id = Some(id.to_string());
    codex.config_dir = Some(source.to_string_lossy().into_owned());
    state.store.insert_session(&codex).unwrap();
    let live = idle_stand_in("codex");
    state.register_live(live.clone());
    state.watch_exit(live.clone());
    let switch = |account: &str| {
        switch_session_account(
            &state,
            "codex",
            Some(Some(account.into())),
            None,
            shell_env(&dir),
        )
    };

    let err = switch("codex-b")
        .await
        .expect_err("the target's database is no database");
    assert_eq!(code_of(&err), error_code::RELOCATION_FAILED);
    assert!(live.poll_exit(), "its process was ended");
    assert!(
        state.live_session("codex").is_none(),
        "nothing was relaunched"
    );
    let after = state.store.get_session("codex").unwrap().unwrap();
    assert_eq!(after.status, SessionStatus::Interrupted);
    assert_eq!(after.config_dir, codex.config_dir, "left on its account");
    assert!(after.account_id.is_none());

    // The relaunch is refused here for the missing project directory, after the copy.
    let err = switch("codex-a")
        .await
        .expect_err("the project directory does not exist");
    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    let named: String = Connection::open(&database)
        .unwrap()
        .query_row("SELECT rollout_path FROM threads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        Path::new(&named),
        target.join(chain.head.strip_prefix(&source).unwrap())
    );
    for file in chain.all() {
        let relative = file.strip_prefix(&source).unwrap();
        assert!(
            target.join(relative).is_file(),
            "{relative:?} was not carried"
        );
    }
}

/// The other half of the seam: ending a console session's process for a switch is not refused
/// because sessions bound to it are running. That refusal belongs to archiving alone, so a
/// switch of a console session with a working bound session goes on to the relaunch, which
/// fails here only on the missing working directory, and leaves the bound session running.
#[tokio::test]
async fn a_switch_is_not_refused_for_a_running_bound_session() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-running-bound");
    state
        .store
        .insert_session(&console_session("hub", SessionStatus::Working))
        .unwrap();
    state
        .store
        .insert_session(&bound_session("worker", SessionStatus::Working, "hub"))
        .unwrap();
    let hub = idle_stand_in("hub");
    let worker = idle_stand_in("worker");
    state.register_live(hub.clone());
    state.watch_exit(hub.clone());
    state.register_live(worker.clone());

    let err = switch_session_account(
        &state,
        "hub",
        Some(Some("claude-a".into())),
        None,
        shell_env(&dir),
    )
    .await
    .expect_err("the console's working directory does not exist");

    assert_eq!(code_of(&err), error_code::DIRECTORY_UNREACHABLE);
    assert!(hub.poll_exit(), "its process was ended");
    assert!(!worker.poll_exit(), "the bound session was left running");
    assert_eq!(status_of(&state, "worker"), Some(SessionStatus::Working));
}

/// A switched session whose relaunched process ended at once is broadcast interrupted on its
/// old account before the switch returns its error, and nothing the exit watcher publishes
/// afterwards puts the new account back in a client's copy.
#[tokio::test]
async fn a_switch_that_did_not_come_up_is_restored_before_it_returns() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-switch-restored-first");
    let before = project_session("s", Agent::Claude, SessionStatus::Interrupted);
    state.store.insert_session(&before).unwrap();
    state
        .set_session_account("s", Some("claude-a"), Some("/elsewhere"))
        .unwrap();
    state
        .store
        .update_session(&Session {
            status: SessionStatus::Idle,
            ..before.clone()
        })
        .unwrap();
    let mut events = state.subscribe();
    let live = fake_live_session("s", Agent::Claude, 80, 24, "exit 1");
    state.register_live(live.clone());
    state.watch_exit(live.clone());

    put_back_once_down(&state, &before).await;

    let mut last = None;
    while let Ok(event) = events.try_recv() {
        if let Event::SessionUpserted { session } = event {
            last = Some(session);
        }
    }
    let last = last.expect("the session was published");
    assert_eq!(last.status, SessionStatus::Interrupted);
    assert_eq!((last.account_id, last.config_dir), (None, None));
}

/// The claim a switch holds outlives the relaunched process being registered as live, and it
/// outlives that process's exit: while the come-up verdict is pending and the account is put
/// back, a resume (what the terminal's Resume button asks for) and a second switch are both
/// refused.
#[tokio::test]
async fn a_switch_claim_is_held_after_the_relaunched_process_is_live_and_after_it_exits() {
    let (state, _dir) = crate::test_support::app_state("coordinator-switch-claim");
    let claim = state.begin_switch("s").unwrap();
    let live = fake_live_session("s", Agent::Claude, 80, 24, "exit 1");
    state.register_live(live.clone());
    state.watch_exit(live.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.live_session("s").is_some() {
        assert!(Instant::now() < deadline, "the exit was never seen");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    for refused in [state.begin_launch("s"), state.begin_switch("s")] {
        let err = refused.err().expect("still claimed");
        assert_eq!(code_of(&err), error_code::SESSION_ALREADY_STARTING);
    }
    drop(claim);
    assert!(state.begin_launch("s").is_ok());
}

/// Clients are shown a session as switching its account for as long as the switch's claim
/// lasts, and the other claims on a session do not show as one.
#[tokio::test]
async fn a_session_shows_as_switching_its_account_while_the_switch_holds_it() {
    let (state, _dir) = console_with_project_and_accounts("coordinator-switch-shown");
    state
        .store
        .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
        .unwrap();
    let mut events = state.subscribe();
    let shown = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
        let mut last = None;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionUpserted { session } = event {
                last = Some(session.switching_account);
            }
        }
        last
    };

    let claim = state.begin_account_switch("s", false).unwrap();
    assert!(state.is_switching_account("s"));
    assert!(
        state
            .wire_session(state.session_record("s").unwrap())
            .switching_account
    );
    assert_eq!(shown(&mut events), Some(true));
    drop(claim);
    assert!(!state.is_switching_account("s"));
    assert_eq!(shown(&mut events), Some(false));

    let claim = state.begin_switch("s").unwrap();
    assert!(
        !state
            .wire_session(state.session_record("s").unwrap())
            .switching_account
    );
    drop(claim);
}

#[tokio::test]
async fn an_archive_during_account_switch_ends_a_late_process_and_survives_a_late_failure() {
    archive_during_relaunch(true, true).await;
    archive_during_relaunch(true, false).await;
}

/// What the switched session's process started is carried over to its relaunch on the new
/// account and keeps running, listed under the session; the relaunched process stops it
/// when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn a_switch_carries_what_the_process_started_over_to_the_relaunch() {
    use crate::test_support::{stand_in_launch, Launched, StandIn};
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-carry-over");
    std::fs::create_dir_all(dir.join("claude-a")).unwrap();
    state
        .store
        .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
        .unwrap();
    let (first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    let started = Arc::new(std::sync::Mutex::new(Vec::<StandIn>::new()));
    let launches = Arc::new(std::sync::Mutex::new(Vec::<Launched>::new()));

    switch_session_account_with(
        &state,
        "s",
        Some(Some("claude-a".to_string())),
        None,
        None,
        LaunchEdit::default(),
        || Ok(HashMap::new()),
        stand_in_launch(&started, &launches, || {}),
    )
    .await
    .unwrap();

    let relaunched = state.live_session("s").expect("relaunched");
    assert_ne!(relaunched.launch_id, first.launch_id);
    state.process_ledger.tick_now();
    assert!(
        crate::test_support::is_running(child),
        "stopped with the process switched away from"
    );
    let trees = state
        .process_ledger
        .trees_of_sessions(&["s".to_string()], Duration::ZERO);
    assert_eq!(trees.len(), 1, "{trees:?}");
    assert!(trees[0].id.starts_with(&format!("{child}:")));

    stop_process(&state, relaunched).await.unwrap();
    assert!(
        crate::test_support::stops_running(&state, child).await,
        "kept running past the relaunch"
    );
}

/// A switch that fails once it has ended the session's process — its relaunch does not start,
/// or starts and ends at once — leaves the session interrupted, and what its process had
/// started running and kept for the user among its project's processes.
#[tokio::test(flavor = "multi_thread")]
async fn a_switch_that_fails_keeps_what_the_process_started_under_its_project() {
    for comes_up_and_dies in [false, true] {
        let (state, dir) = console_with_project_and_accounts("coordinator-switch-carry-failed");
        std::fs::create_dir_all(dir.join("claude-a")).unwrap();
        state
            .store
            .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
            .unwrap();
        let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
        let _guard = crate::test_support::KillOnDrop(vec![child]);
        let started = Arc::new(std::sync::Mutex::new(
            Vec::<crate::test_support::StandIn>::new(),
        ));
        let kept_alive = started.clone();
        let launch = move |request: term::LaunchRequest| {
            if !comes_up_and_dies {
                anyhow::bail!("the agent could not be started");
            }
            let live = crate::test_support::tagged_live_session(
                &request.session_id,
                request.agent,
                "exit 1",
                &request.process_tag,
            );
            let process = (*live).clone();
            kept_alive.lock().unwrap().push(live);
            Ok(term::Launch {
                session: process,
                agent_session_id: None,
                notice: None,
            })
        };

        switch_session_account_with(
            &state,
            "s",
            Some(Some("claude-a".to_string())),
            None,
            None,
            LaunchEdit::default(),
            || Ok(HashMap::new()),
            launch,
        )
        .await
        .expect_err("the relaunch failed");

        assert_eq!(status_of(&state, "s"), Some(SessionStatus::Interrupted));
        state.process_ledger.tick_now();
        assert!(
            crate::test_support::is_running(child),
            "stopped with a failed switch ({comes_up_and_dies})"
        );
        let listed = state.process_ledger.trees_of_project(&[], "project-1", &[]);
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert!(listed[0].id.starts_with(&format!("{child}:")));
        assert!(listed[0].kept && listed[0].session_id.is_none());
    }
}

/// A process that had exited by itself before the switch is not the switch's doing, so when
/// the switch fails, what it had started is stopped as for any agent that exits on its own,
/// not kept under the project.
#[tokio::test(flavor = "multi_thread")]
async fn a_switch_that_fails_stops_what_a_process_that_had_exited_by_itself_started() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-exited-failed");
    std::fs::create_dir_all(dir.join("claude-a")).unwrap();
    state
        .store
        .insert_session(&project_session("s", Agent::Claude, SessionStatus::Idle))
        .unwrap();
    let (pid_file, go_file) = (dir.join("s-child"), dir.join("go"));
    let live = fake_live_session(
        "s",
        Agent::Claude,
        80,
        24,
        &format!(
            "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} exec \"/bin/sleep\", \"300\";' &\n\
             while [ ! -e {go} ]; do sleep 0.05; done\n\
             exit 0",
            crate::test_support::perl_write(&pid_file, "$$"),
            go = go_file.display()
        ),
    );
    state.register_live(live.clone());
    let child = crate::test_support::wait_for_pid(&pid_file);
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    // The agent's shell is still there for this read, which attributes what it started.
    state.process_ledger.scan_before_ending();
    std::fs::write(&go_file, "").unwrap();
    let exited = live.clone();
    assert!(
        crate::test_support::within(crate::test_support::PATIENCE, move || exited.poll_exit())
            .await,
        "the process exits by itself"
    );
    // Still registered, as in the moment before the exit watcher deregisters it.
    let watched = live.clone();
    let watching = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        watching.watch_exit(watched);
    });

    switch_session_account_with(
        &state,
        "s",
        Some(Some("claude-a".to_string())),
        None,
        None,
        LaunchEdit::default(),
        || Ok(HashMap::new()),
        |_| Err(anyhow::anyhow!("the agent could not be started")),
    )
    .await
    .expect_err("the relaunch failed");

    assert!(
        crate::test_support::stops_running(&state, child).await,
        "kept past a switch that did not end the process"
    );
    assert!(state
        .process_ledger
        .trees_of_project(&[], "project-1", &[])
        .is_empty());
}

/// A session archived while its switch was under way keeps nothing either: what its process
/// had started is stopped, as an archive stops it.
#[tokio::test(flavor = "multi_thread")]
async fn what_a_switch_held_for_a_session_archived_meanwhile_is_stopped() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-archived");
    state
        .store
        .insert_session(&project_session(
            "s",
            Agent::Claude,
            SessionStatus::Archived,
        ))
        .unwrap();
    let (first, child) = crate::test_support::running_leaving_one(&state, &dir, "s");
    let _guard = crate::test_support::KillOnDrop(vec![child]);
    let ledger = &state.process_ledger;
    let held = ledger.hold_for_launch("s");
    ledger.hand_over(&first.launch_id, "s");
    ledger.end_launch(&first.launch_id);
    assert!(crate::test_support::is_running(child));

    keep_what_a_failed_restart_carried(&state, "s", held);

    assert!(
        crate::test_support::stops_running(&state, child).await,
        "kept for an archived session"
    );
    assert!(ledger.trees_of_project(&[], "project-1", &[]).is_empty());
}

/// A console session keeps nothing: what its process had started is stopped when its switch
/// fails, as an interrupted session's is.
#[tokio::test(flavor = "multi_thread")]
async fn a_console_session_whose_switch_fails_stops_what_its_process_started() {
    let (state, dir) = console_with_project_and_accounts("coordinator-switch-console-failed");
    std::fs::create_dir_all(dir.join("claude-a")).unwrap();
    state
        .store
        .insert_session(&console_session("hub", SessionStatus::Idle))
        .unwrap();
    let (_first, child) = crate::test_support::running_leaving_one(&state, &dir, "hub");
    let _guard = crate::test_support::KillOnDrop(vec![child]);

    switch_session_account_with(
        &state,
        "hub",
        Some(Some("claude-a".to_string())),
        None,
        None,
        LaunchEdit::default(),
        || Ok(HashMap::new()),
        |_| Err(anyhow::anyhow!("the agent could not be started")),
    )
    .await
    .expect_err("the relaunch failed");

    assert!(
        crate::test_support::stops_running(&state, child).await,
        "kept running past the failed switch"
    );
}
