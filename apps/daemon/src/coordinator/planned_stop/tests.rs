//! The planned stop over holders that run in this process: a first `Holders` stands for the daemon
//! that started the terminals and is gone, a second for the daemon that adopts them and then makes
//! the planned stop, and a third for the daemon that comes next. The programs are bash scripts.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::coordinator::adopt::adopt_found;
use crate::coordinator::test_support::{found_in, left_running_script};
use crate::holder::client::in_process;
use crate::platform::term::Size;
use crate::protocol::{AgentId, SessionStatus, ShellTerminalState};
use crate::terminal::shell::ShellMeta;
use crate::test_support::{
    app_state, console_with_project_and_accounts, is_running, project_session, within, KillOnDrop,
    ScratchDir, PATIENCE,
};

/// A daemon that adopted a session `s1` (terminal `t1`) and a shell terminal `t2` from holders
/// that run `session_script` and `shell_script`, and is about to make a planned stop.
struct Adopting {
    state: Arc<AppState>,
    dir: ScratchDir,
    holders: Arc<Holders>,
    scratch: PathBuf,
    pids: Vec<i32>,
    _guard: KillOnDrop,
}

async fn adopting(name: &str, session_script: &str, shell_script: &str) -> Adopting {
    let (state, dir) = console_with_project_and_accounts(name);
    state
        .store
        .insert_session(&project_session(
            "s1",
            AgentId::TEST,
            SessionStatus::Working,
        ))
        .expect("the session is stored");
    let run_dir = dir.join("run");
    let scratch = run_dir.join("s1");
    std::fs::create_dir_all(&scratch).unwrap();
    let session_meta = json!({
        "kind": "session",
        "session_id": "s1",
        "launch_id": "launch-s1",
        "agent": AgentId::TEST,
        "scratch": scratch.to_string_lossy(),
    });
    let shell_meta = ShellMeta {
        id: "t2".to_string(),
        project_id: "project-1".to_string(),
        worktree: None,
        cwd: dir.to_string_lossy().into_owned(),
        created_at: 5,
        shell_name: Some("sh".to_string()),
    }
    .to_value();
    let size = Size { rows: 24, cols: 80 };
    let pids = vec![
        left_running_script(&dir, "t1", session_meta, session_script, size),
        left_running_script(&dir, "t2", shell_meta, shell_script, size),
    ];
    let guard = KillOnDrop(pids.clone());

    let holders = Arc::new(in_process(dir.join("holders")).expect("holders"));
    let found = found_in(&holders);
    let adopted = {
        let (state, run_dir) = (state.clone(), run_dir.clone());
        tokio::task::spawn_blocking(move || adopt_found(&state, found, &run_dir))
            .await
            .expect("the adoption ran")
    };
    assert_eq!(adopted.sessions, ["s1"]);
    assert_eq!(adopted.shells, ["t2"]);
    let live = state.live_session("s1").expect("the session is live");
    let shell = state
        .shell_terminals
        .process("t2")
        .expect("the shell is held");
    let drawn = within(PATIENCE, move || {
        live.screen_text().contains("the session drew this")
            && shell.screen_text().contains("the shell drew this")
    })
    .await;
    assert!(drawn, "the programs' output never reached their screens");
    Adopting {
        state,
        dir,
        holders,
        scratch,
        pids,
        _guard: guard,
    }
}

/// A planned stop hands every terminal over and ends nothing: the session keeps its status, its
/// process, its scratch directory and its place among the live sessions, the shell terminal its
/// record, and neither is recorded as exited afterwards. The screens saved are those the next
/// daemon's holders verify, by the handover they report; a screen already inexact stays so.
#[tokio::test(flavor = "multi_thread")]
async fn a_planned_stop_leaves_every_terminal_running_and_saves_its_screen_for_the_next_daemon() {
    let Adopting {
        state,
        dir,
        holders,
        scratch,
        pids,
        _guard,
    } = adopting(
        "planned-stop",
        "printf 'the session drew this\\r\\n'; while :; do sleep 0.05; done",
        "printf 'the shell drew this\\r\\n'; while :; do sleep 0.05; done",
    )
    .await;

    let path = dir.join("restart-screens.json");
    let handed = hand_over(&state, holders.clone(), path.clone()).await;
    assert_eq!(
        handed,
        Handover::Parted(HandedOver {
            terminals: 2,
            screens: 2,
            not_handed_over: 0,
        })
    );
    assert!(state.is_handing_over());

    // Past several polls of the exit watchers.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Working
    );
    assert!(state.live_session("s1").is_some());
    assert!(
        scratch.is_dir(),
        "the session's scratch directory was removed"
    );
    assert_eq!(state.shell_terminals.list().len(), 1);
    assert!(
        pids.iter().all(|pid| is_running(*pid)),
        "a program was ended"
    );
    assert!(path.exists(), "no screen was saved");

    // The daemon that comes next.
    let (next_state, _next_dir) = app_state("planned-stop-next");
    next_state.load_restart_screens(&path);
    assert!(path.exists(), "loading consumed the saved screens");
    let next = in_process(dir.join("holders")).expect("holders");
    let found = found_in(&next);
    assert_eq!(found.len(), 2);
    for terminal in &found {
        let exact = next_state
            .select_exact_screen(terminal)
            .unwrap_or_else(|why| panic!("{}: {why}", terminal.id));
        // Both were adopted as after a crash, which no handover clears.
        assert!(exact.saved.inexact, "{}", terminal.id);
        assert_eq!((exact.saved.rows, exact.saved.cols), (24, 80));
    }
}

/// A program that ends after the handover has begun is the next daemon's to find: nothing of the
/// old daemon records the exit, ends the session, removes its scratch directory or closes the
/// shell terminal's record.
#[tokio::test(flavor = "multi_thread")]
async fn a_program_that_ends_while_the_terminals_are_handed_over_is_not_recorded_as_exited() {
    let go = ScratchDir::new("planned-stop-exit-go");
    let go_file = go.join("go");
    let wait_for_go = format!(
        "while [ ! -e '{}' ]; do sleep 0.05; done",
        go_file.display()
    );
    let adopting = adopting(
        "planned-stop-exit",
        &format!("printf 'the session drew this\\r\\n'; {wait_for_go}"),
        &format!("printf 'the shell drew this\\r\\n'; {wait_for_go}"),
    )
    .await;
    let (state, scratch) = (&adopting.state, &adopting.scratch);

    let path = adopting.dir.join("restart-screens.json");
    hand_over(state, adopting.holders.clone(), path).await;
    assert!(state.is_handing_over());

    // Both programs end now, with the daemon past its handover.
    std::fs::write(&go_file, "").unwrap();
    for pid in adopting.pids.clone() {
        assert!(
            within(PATIENCE, move || !is_running(pid)).await,
            "a program never ended"
        );
    }
    // Past several polls of the exit watchers.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Working
    );
    assert!(state.live_session("s1").is_some());
    assert!(
        scratch.is_dir(),
        "the session's scratch directory was removed"
    );
    let shells = state.shell_terminals.list();
    assert_eq!(shells.len(), 1);
    assert_eq!(shells[0].state, ShellTerminalState::Running);
}

/// An ordinary stop asked for before a planned stop begins its handover escalates it at once:
/// nothing is handed over, and no screen is written.
#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_stop_asked_before_the_handover_hands_nothing_over() {
    let (state, dir) = app_state("planned-stop-escalated-before");
    let holders = Arc::new(in_process(dir.join("holders")).expect("holders"));
    state.request_planned_stop();
    state.request_shutdown();
    let path = dir.join("restart-screens.json");
    assert_eq!(
        hand_over(&state, holders, path.clone()).await,
        Handover::Escalated
    );
    assert!(!state.is_handing_over());
    assert!(!path.exists(), "a screen was written");
}

/// An ordinary stop asked for while the planned stop waits for an operation admitted before its
/// barrier escalates it without waiting that out: the handover never begins.
#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_stop_cuts_the_wait_for_admitted_operations_short() {
    let (state, dir) = app_state("planned-stop-escalated-waiting");
    let holders = Arc::new(in_process(dir.join("holders")).expect("holders"));
    // Held throughout: a stop that waited it out would go on to hand over.
    let admitted = state.admit_change().expect("admitted");
    state.request_planned_stop();
    let path = dir.join("restart-screens.json");
    let (handover, ()) = tokio::join!(hand_over(&state, holders, path.clone()), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        state.request_shutdown();
    });
    assert_eq!(handover, Handover::Escalated);
    assert!(state.handover_barrier_raised());
    assert!(
        !state.is_handing_over(),
        "the handover began after the wait"
    );
    assert!(!path.exists(), "a screen was written");
    drop(admitted);
}

/// An ordinary stop asked for once the terminals are marked stops the handover there: no screen
/// is saved from then on, the screens file is replaced all the same, and the holders are not
/// parted from but named among those left to the next daemon, which the ordinary stop then ends
/// and forgets.
#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_stop_asked_after_the_marks_leaves_the_holders_linked_for_the_stop_to_end() {
    let adopting = adopting(
        "planned-stop-escalated-marked",
        "printf 'the session drew this\\r\\n'; while :; do sleep 0.05; done",
        "printf 'the shell drew this\\r\\n'; while :; do sleep 0.05; done",
    )
    .await;
    let live = adopting
        .state
        .live_session("s1")
        .expect("the session is live");
    let holder = live.held_at().expect("held").holder_id;
    let path = adopting.dir.join("restart-screens.json");
    // A file of an earlier handover, which the stop replaces.
    std::fs::write(&path, "earlier").unwrap();
    let shells = adopting.state.shell_terminals.processes();
    let (holders, blocking_path) = (adopting.holders.clone(), path.clone());
    let handover = tokio::task::spawn_blocking(move || {
        // No ordinary stop at the look before the marks, one at every look after.
        let looked = std::sync::atomic::AtomicBool::new(false);
        let escalated = || looked.swap(true, std::sync::atomic::Ordering::AcqRel);
        hand_over_blocking(&holders, &[live], &shells, &blocking_path, &[], &escalated)
    })
    .await
    .expect("the handover ran");
    assert_eq!(handover, Handover::Escalated);
    assert!(!path.exists(), "the earlier file was not replaced");
    assert!(adopting.holders.left_to_successor().contains(&holder));
    // Still linked to: a parting takes the links.
    assert!(adopting.holders.versions().contains_key(&holder));
    assert!(
        adopting.pids.iter().all(|pid| is_running(*pid)),
        "a program was ended"
    );

    // A holder in this process is never signalled; the stop still takes it out of use.
    let holders = adopting.holders.clone();
    tokio::task::spawn_blocking(move || holders.end_handed_over())
        .await
        .unwrap();
    assert!(adopting.holders.left_to_successor().is_empty());
    assert!(!adopting.holders.versions().contains_key(&holder));
}

/// An ordinary stop that escalates a planned one tells a terminal of a holder left to the next
/// daemon nothing: its program is left to end with its holder, which the stop has the caller end,
/// while the session's preview is kept and the session left interrupted.
#[tokio::test(flavor = "multi_thread")]
async fn an_escalated_stop_leaves_the_program_of_a_left_holder_to_its_holder() {
    let adopting = adopting(
        "planned-stop-escalated-sessions",
        "printf 'the session drew this\\r\\n'; while :; do sleep 0.05; done",
        "printf 'the shell drew this\\r\\n'; while :; do sleep 0.05; done",
    )
    .await;
    let state = &adopting.state;
    let live = state.live_session("s1").expect("the session is live");
    let left = std::collections::HashSet::from([live.held_at().expect("held").holder_id]);
    let ended_left = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ending = ended_left.clone();
    state
        .stop_all_sessions_after_handover(&left, move || {
            ending.store(true, std::sync::atomic::Ordering::Release);
        })
        .await;
    assert!(ended_left.load(std::sync::atomic::Ordering::Acquire));
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Interrupted
    );
    let saved = state.saved_output("s1").expect("the preview was kept");
    assert!(String::from_utf8_lossy(&saved).contains("the session drew this"));
    assert!(
        is_running(adopting.pids[0]),
        "the session's program was ended terminal by terminal"
    );
}
