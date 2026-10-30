//! The requests over real processes: a stand-in agent whose shell commands each start a
//! terminal session of their own, as an agent CLI's do.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{long_running, ASK_AFTER, ASK_AFTER_OVERRIDE, LIST_FLOOR_OVERRIDE};
use crate::coordinator::handle;
use crate::process_ledger::ProcessTree;
use crate::protocol::{
    error_code, Agent, CodedError, Event, ProcessTarget, RequestBody, SessionStatus,
};
use crate::state::AppState;
use crate::test_support::{
    assert_stops_running, console_with_project_and_accounts, detached_in, fake_live_session,
    is_running, perl_write, project_session, wait_for_pid, KillOnDrop, StandIn, PATIENCE,
};

/// A stand-in agent for session `id` (with a stored record) that runs, in the background, one
/// command per file in `pid_files`: a process in a terminal session of its own that writes its pid
/// to the file and becomes `sleep`.
fn agent_with(state: &Arc<AppState>, id: &str, pid_files: &[&Path]) -> (StandIn, Vec<i32>) {
    state
        .store
        .insert_session(&project_session(id, Agent::Claude, SessionStatus::Idle))
        .unwrap();
    let commands: String = pid_files
        .iter()
        .map(|file| {
            format!(
                "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} exec \"/bin/sleep\", \"300\";' &\n",
                perl_write(file, "$$")
            )
        })
        .collect();
    let live = fake_live_session(
        id,
        Agent::Claude,
        80,
        24,
        &format!("{commands}exec sleep 300"),
    );
    state.register_live(live.clone());
    let pids = pid_files.iter().map(|file| wait_for_pid(file)).collect();
    (live, pids)
}

/// Lists `target` until `count` trees show, since the ledger attributes a process only once a
/// scan has seen it; every list reads the table, whatever the floor.
async fn trees_when(
    state: &Arc<AppState>,
    target: fn() -> ProcessTarget,
    count: usize,
) -> Vec<ProcessTree> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        LIST_FLOOR_OVERRIDE.set(Some(Duration::ZERO));
        let trees = super::list(state, target()).await;
        LIST_FLOOR_OVERRIDE.set(None);
        let trees = trees.unwrap();
        if trees.len() == count {
            return trees;
        }
        assert!(Instant::now() < deadline, "never {count} trees: {trees:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn session_s1() -> ProcessTarget {
    ProcessTarget::Session {
        session_id: "s1".to_string(),
    }
}

fn project_1() -> ProcessTarget {
    ProcessTarget::Project {
        project_id: "project-1".to_string(),
    }
}

fn code_of(err: &anyhow::Error) -> &str {
    err.downcast_ref::<CodedError>()
        .expect("a coded error")
        .code
}

/// A `sleep` working in `cwd`, which a test then removes, detached as an agent's leftover is.
fn sleeper_in(cwd: &Path) -> i32 {
    detached_in(cwd, "/bin/sleep", &["60"], &[])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_lists_its_trees_and_a_session_without_a_process_or_a_record_is_refused() {
    let (state, dir) = console_with_project_and_accounts("process-requests-session");
    let (_live, pids) = agent_with(&state, "s1", &[&dir.join("one")]);
    let _guard = KillOnDrop(pids.clone());
    state
        .store
        .insert_session(&project_session(
            "s2",
            Agent::Claude,
            SessionStatus::Interrupted,
        ))
        .unwrap();

    let trees = trees_when(&state, session_s1, 1).await;
    assert_eq!(trees[0].session_id.as_deref(), Some("s1"));
    assert!(trees[0].id.starts_with(&format!("{}:", pids[0])));
    assert!(trees[0].command.contains("sleep"), "{:?}", trees[0]);

    let not_running = super::list(
        &state,
        ProcessTarget::Session {
            session_id: "s2".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&not_running), error_code::SESSION_NOT_RUNNING);
    let unknown = super::list(
        &state,
        ProcessTarget::Session {
            session_id: "nobody".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&unknown), error_code::UNKNOWN_SESSION);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_lists_its_running_sessions_trees_and_its_leftovers_without_a_session() {
    let (state, dir) = console_with_project_and_accounts("process-requests-project");
    let (_live, pids) = agent_with(&state, "s1", &[&dir.join("one")]);
    let _guard = KillOnDrop(pids);
    let gone = dir.join("no-such-project").join(".worktrees").join("gone");
    std::fs::create_dir_all(&gone).unwrap();
    let left_id = sleeper_in(&gone);
    let _left = KillOnDrop(vec![left_id]);
    std::fs::remove_dir(&gone).unwrap();

    let trees = trees_when(&state, project_1, 2).await;
    let leftover = trees
        .iter()
        .find(|tree| tree.id.starts_with(&format!("{left_id}:")))
        .expect("the leftover");
    assert_eq!(leftover.session_id, None);
    assert!(leftover.cwd_missing);
    assert_eq!(
        trees
            .iter()
            .filter(|tree| tree.session_id.as_deref() == Some("s1"))
            .count(),
        1
    );

    let unknown = super::list(
        &state,
        ProcessTarget::Project {
            project_id: "nowhere".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(&unknown), error_code::UNKNOWN_PROJECT);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_ends_the_named_trees_alone_and_names_only_those_it_stopped() {
    let (state, dir) = console_with_project_and_accounts("process-requests-stop");
    let (_live, pids) = agent_with(&state, "s1", &[&dir.join("one"), &dir.join("two")]);
    let _guard = KillOnDrop(pids.clone());
    let gone = dir.join("no-such-project").join("gone");
    std::fs::create_dir_all(&gone).unwrap();
    let left = sleeper_in(&gone);
    let _left = KillOnDrop(vec![left]);
    std::fs::remove_dir(&gone).unwrap();
    let trees = trees_when(&state, project_1, 3).await;
    let by_pid = |pid: u32| {
        trees
            .iter()
            .find(|tree| tree.id.starts_with(&format!("{pid}:")))
            .expect("tree")
            .id
            .clone()
    };
    let (first, leftover) = (by_pid(pids[0] as u32), by_pid(left as u32));

    let reply = handle(
        &state,
        Some("r".to_string()),
        RequestBody::StopProcesses {
            ids: vec![first.clone(), leftover.clone(), "1:1".to_string()],
        },
    )
    .await
    .unwrap();
    let Some(Event::ProcessesStopped { id, ids }) = reply else {
        panic!("not processes_stopped");
    };
    assert_eq!(id.as_deref(), Some("r"));
    assert_eq!(ids, vec![first, leftover]);
    assert_stops_running(pids[0]);
    assert_stops_running(left);
    assert!(is_running(pids[1]), "a tree not named was stopped");
}

/// A kept tree leaves its session's list for its project's, as one no session owns, and is
/// stopped only when the user asks.
#[tokio::test(flavor = "multi_thread")]
async fn a_kept_tree_is_listed_under_its_project_as_no_sessions() {
    let (state, dir) = console_with_project_and_accounts("process-requests-keep");
    let (_live, pids) = agent_with(&state, "s1", &[&dir.join("one"), &dir.join("two")]);
    let _guard = KillOnDrop(pids.clone());
    let trees = trees_when(&state, session_s1, 2).await;
    let kept = trees
        .iter()
        .find(|tree| tree.id.starts_with(&format!("{}:", pids[0])))
        .unwrap()
        .id
        .clone();

    let reply = handle(
        &state,
        None,
        RequestBody::KeepProcesses {
            ids: vec![kept.clone(), "1:1".to_string()],
        },
    )
    .await
    .unwrap();
    let Some(Event::ProcessesKept { ids, .. }) = reply else {
        panic!("not processes_kept");
    };
    assert_eq!(ids, vec![kept.clone()]);
    let left = trees_when(&state, session_s1, 1).await;
    assert!(left[0].id.starts_with(&format!("{}:", pids[1])));
    assert!(!left[0].kept);
    let in_project = trees_when(&state, project_1, 2).await;
    let unowned: Vec<&ProcessTree> = in_project
        .iter()
        .filter(|tree| tree.session_id.is_none())
        .collect();
    assert_eq!(unowned.len(), 1, "{in_project:?}");
    assert_eq!(unowned[0].id, kept);
    assert!(unowned[0].kept && !unowned[0].cwd_missing);
    assert!(is_running(pids[0]));

    assert_eq!(super::stop(&state, vec![kept]).await.unwrap().len(), 1);
    assert_stops_running(pids[0]);
    assert!(is_running(pids[1]));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preflight_lists_only_the_trees_that_have_run_long_enough_and_only_when_asked() {
    let (state, dir) = console_with_project_and_accounts("process-requests-preflight");
    let (_live, pids) = agent_with(&state, "s1", &[&dir.join("one")]);
    let _guard = KillOnDrop(pids.clone());
    trees_when(&state, session_s1, 1).await;

    let inspect = |processes| {
        handle(
            &state,
            None,
            RequestBody::InspectSessionBranches {
                sessions: vec!["s1".to_string(), "unknown".to_string()],
                processes,
            },
        )
    };
    let Some(Event::SessionBranchesInspected { processes, .. }) = inspect(true).await.unwrap()
    else {
        panic!("not session_branches_inspected");
    };
    assert!(
        processes.is_empty(),
        "a tree younger than {ASK_AFTER:?}: {processes:?}"
    );
    // With the threshold lowered, the same tree is old enough.
    ASK_AFTER_OVERRIDE.set(Some(Duration::ZERO));
    let asked = inspect(true).await;
    ASK_AFTER_OVERRIDE.set(None);
    let Some(Event::SessionBranchesInspected { processes, .. }) = asked.unwrap() else {
        panic!("not session_branches_inspected");
    };
    assert_eq!(processes.len(), 1, "{processes:?}");
    assert!(processes[0].id.starts_with(&format!("{}:", pids[0])));
    assert_eq!(processes[0].session_id.as_deref(), Some("s1"));
    // Raising it past the tree's age hides the tree again.
    assert!(
        long_running(&state, vec!["s1".to_string()], Duration::from_secs(3600))
            .await
            .unwrap()
            .is_empty()
    );
    ASK_AFTER_OVERRIDE.set(Some(Duration::ZERO));
    let unasked = inspect(false).await;
    ASK_AFTER_OVERRIDE.set(None);
    let Some(Event::SessionBranchesInspected { processes, .. }) = unasked.unwrap() else {
        panic!("not session_branches_inspected");
    };
    assert!(processes.is_empty(), "listed without being asked for");
}

/// A tree whose top exited between the listing and the keep — a wrapper that ended, its child
/// reparented and now a top of its own — is still kept by the id the listing gave it, whatever
/// was listed since: what was in it then and still runs becomes the user's, and an archive
/// leaves it running.
#[tokio::test(flavor = "multi_thread")]
async fn a_tree_whose_top_exited_since_it_was_listed_is_kept_by_its_listed_id() {
    let (state, dir) = console_with_project_and_accounts("process-requests-top-exited");
    state
        .store
        .insert_session(&project_session("s1", Agent::Claude, SessionStatus::Idle))
        .unwrap();
    let (top_file, child_file, go) = (dir.join("top"), dir.join("child"), dir.join("go"));
    let wrapper = format!(
        "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} my $c = fork(); if ($c == 0) {{ exec \"/bin/sleep\", \"300\" }} {} \
         select(undef, undef, undef, 0.02) until -e \"{}\"; exit 0;' &",
        perl_write(&top_file, "$$"),
        perl_write(&child_file, "$c"),
        go.display()
    );
    let live = fake_live_session(
        "s1",
        Agent::Claude,
        80,
        24,
        &format!("{wrapper}\nexec sleep 300"),
    );
    state.register_live(live.clone());
    state.watch_exit((*live).clone());
    let (top, child) = (wait_for_pid(&top_file), wait_for_pid(&child_file));
    let _guard = KillOnDrop(vec![top, child]);
    let deadline = Instant::now() + PATIENCE;
    let listed = loop {
        let trees = long_running(&state, vec!["s1".to_string()], Duration::ZERO)
            .await
            .unwrap();
        if trees.first().is_some_and(|tree| tree.process_count == 2) {
            break trees[0].id.clone();
        }
        assert!(Instant::now() < deadline, "never listed whole: {trees:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert!(listed.starts_with(&format!("{top}:")));

    std::fs::write(&go, "").unwrap();
    assert_stops_running(top);
    // Another client's poll lists the session again meanwhile, its tree now topped by the child.
    let deadline = Instant::now() + PATIENCE;
    loop {
        let trees = trees_when(&state, session_s1, 1).await;
        if trees[0].id.starts_with(&format!("{child}:")) {
            break;
        }
        assert!(Instant::now() < deadline, "never relisted: {trees:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    handle(
        &state,
        None,
        RequestBody::ArchiveSession {
            session: "s1".to_string(),
            force_remove: Vec::new(),
            keep: Vec::new(),
            keep_process_ids: vec![listed],
        },
    )
    .await
    .unwrap();
    let deadline = Instant::now() + PATIENCE;
    while state.has_process("s1") {
        assert!(Instant::now() < deadline, "the session never stopped");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(is_running(child), "the kept tree's child was stopped");
}

/// A process left behind is kept like a tree of a running launch, and still listed under the
/// project it was left in, as kept, until the user stops it.
#[tokio::test(flavor = "multi_thread")]
async fn a_leftover_is_kept_and_listed_as_kept() {
    let (state, dir) = console_with_project_and_accounts("process-requests-keep-leftover");
    let gone = dir.join("no-such-project").join(".worktrees").join("gone");
    std::fs::create_dir_all(&gone).unwrap();
    let left = sleeper_in(&gone);
    let _left = KillOnDrop(vec![left]);
    std::fs::remove_dir(&gone).unwrap();
    let trees = trees_when(&state, project_1, 1).await;
    let leftover = trees[0].id.clone();
    assert!(leftover.starts_with(&format!("{left}:")));

    let reply = handle(
        &state,
        None,
        RequestBody::KeepProcesses {
            ids: vec![leftover.clone()],
        },
    )
    .await
    .unwrap();
    let Some(Event::ProcessesKept { ids, .. }) = reply else {
        panic!("not processes_kept");
    };
    assert_eq!(ids, vec![leftover.clone()]);
    let trees = trees_when(&state, project_1, 1).await;
    assert_eq!(trees[0].id, leftover);
    assert!(trees[0].kept && trees[0].cwd_missing);
    assert_eq!(trees[0].session_id, None);
    assert!(is_running(left));
    assert_eq!(
        super::stop(&state, vec![leftover.clone()]).await.unwrap(),
        vec![leftover]
    );
    assert_stops_running(left);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_archive_keeps_the_trees_the_user_chose_and_stops_the_rest() {
    let (state, dir) = console_with_project_and_accounts("process-requests-archive");
    let (live, pids) = agent_with(&state, "s1", &[&dir.join("one"), &dir.join("two")]);
    state.watch_exit((*live).clone());
    let _guard = KillOnDrop(pids.clone());
    let trees = trees_when(&state, session_s1, 2).await;
    let kept = trees
        .iter()
        .find(|tree| tree.id.starts_with(&format!("{}:", pids[0])))
        .unwrap()
        .id
        .clone();

    handle(
        &state,
        None,
        RequestBody::ArchiveSession {
            session: "s1".to_string(),
            force_remove: Vec::new(),
            keep: Vec::new(),
            keep_process_ids: vec![kept],
        },
    )
    .await
    .unwrap();
    assert_stops_running(pids[1]);
    assert!(
        is_running(pids[0]),
        "the kept tree was stopped with the session"
    );
    let in_project = trees_when(&state, project_1, 1).await;
    assert!(in_project[0].id.starts_with(&format!("{}:", pids[0])));
    assert_eq!(in_project[0].session_id, None);
    assert!(in_project[0].kept);
}

/// A list of the same target computed a moment ago answers again, so clients polling it do not
/// each read the table; a keep through the daemon makes the next list read it.
#[tokio::test(flavor = "multi_thread")]
async fn a_list_polled_again_at_once_is_answered_from_the_last_one_until_a_keep() {
    let (state, dir) = console_with_project_and_accounts("process-requests-floor");
    let (_live, pids) = agent_with(&state, "s1", &[&dir.join("one"), &dir.join("two")]);
    let _guard = KillOnDrop(pids.clone());
    let trees = trees_when(&state, session_s1, 2).await;
    let kept = trees
        .iter()
        .find(|tree| tree.id.starts_with(&format!("{}:", pids[0])))
        .unwrap()
        .id
        .clone();
    let listed = |state: &Arc<AppState>| {
        let state = Arc::clone(state);
        async move {
            LIST_FLOOR_OVERRIDE.set(Some(Duration::from_secs(3600)));
            let trees = super::list(&state, session_s1()).await;
            LIST_FLOOR_OVERRIDE.set(None);
            trees.unwrap()
        }
    };
    assert_eq!(listed(&state).await.len(), 2);

    // SAFETY: the test's own fixture, still running.
    unsafe { libc::kill(pids[1], libc::SIGKILL) };
    assert_stops_running(pids[1]);
    assert_eq!(listed(&state).await.len(), 2, "the table was read again");
    assert_eq!(super::keep(&state, vec![kept]).await.unwrap().len(), 1);
    assert!(
        listed(&state).await.is_empty(),
        "a keep left the last list standing"
    );
}

/// A stand-in console session `hub` whose shell command leaves one process running in a terminal
/// session of its own; answers it and that process's pid.
fn console_agent(state: &Arc<AppState>, pid_file: &Path) -> (StandIn, i32) {
    state
        .store
        .insert_session(&crate::test_support::console_session(
            "hub",
            SessionStatus::Idle,
        ))
        .unwrap();
    let live = fake_live_session(
        "hub",
        Agent::Claude,
        80,
        24,
        &format!(
            "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} exec \"/bin/sleep\", \"300\";' &\n\
             exec sleep 300",
            perl_write(pid_file, "$$")
        ),
    );
    state.register_live(live.clone());
    state.watch_exit(live.clone());
    let pid = wait_for_pid(pid_file);
    (live, pid)
}

fn session_hub() -> ProcessTarget {
    ProcessTarget::Session {
        session_id: "hub".to_string(),
    }
}

/// A console session keeps nothing: a keep of its tree keeps nothing, and an archive that names
/// it among the trees to keep stops it all the same.
#[tokio::test(flavor = "multi_thread")]
async fn a_console_sessions_tree_is_not_kept() {
    let (state, dir) = console_with_project_and_accounts("process-requests-console-keep");
    let (_live, pid) = console_agent(&state, &dir.join("child"));
    let _guard = KillOnDrop(vec![pid]);
    let trees = trees_when(&state, session_hub, 1).await;
    let tree = trees[0].id.clone();

    assert!(super::keep(&state, vec![tree.clone()])
        .await
        .unwrap()
        .is_empty());
    assert_eq!(trees_when(&state, session_hub, 1).await[0].id, tree);

    handle(
        &state,
        None,
        RequestBody::ArchiveSession {
            session: "hub".to_string(),
            force_remove: Vec::new(),
            keep: Vec::new(),
            keep_process_ids: vec![tree],
        },
    )
    .await
    .unwrap();
    assert_stops_running(pid);
}

/// The confirmation of a project's removal is told of the project's processes, kept and left
/// behind alike, and the removal stops them, the folder left in place.
#[tokio::test(flavor = "multi_thread")]
async fn removing_a_project_stops_its_processes() {
    let (state, dir) = console_with_project_and_accounts("process-requests-project-removal");
    let (live, pids) = agent_with(&state, "s1", &[&dir.join("one")]);
    state.watch_exit((*live).clone());
    let gone = dir.join("no-such-project").join(".worktrees").join("gone");
    std::fs::create_dir_all(&gone).unwrap();
    let left = sleeper_in(&gone);
    std::fs::remove_dir(&gone).unwrap();
    let _guard = KillOnDrop(vec![pids[0], left]);
    let trees = trees_when(&state, session_s1, 1).await;
    assert_eq!(
        super::keep(&state, vec![trees[0].id.clone()])
            .await
            .unwrap()
            .len(),
        1
    );

    let reply = handle(
        &state,
        None,
        RequestBody::InspectProjectRemoval {
            project: "project-1".to_string(),
        },
    )
    .await
    .unwrap();
    let Some(Event::ProjectRemovalInspected { processes, .. }) = reply else {
        panic!("not project_removal_inspected");
    };
    let mut listed: Vec<(i32, bool)> = processes
        .iter()
        .map(|tree| {
            (
                tree.id.split(':').next().unwrap().parse().unwrap(),
                tree.kept,
            )
        })
        .collect();
    listed.sort();
    let mut expected = vec![(pids[0], true), (left, false)];
    expected.sort();
    assert_eq!(listed, expected);
    assert!(processes.iter().all(|tree| tree.session_id.is_none()));

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
    assert_stops_running(pids[0]);
    assert_stops_running(left);
}
