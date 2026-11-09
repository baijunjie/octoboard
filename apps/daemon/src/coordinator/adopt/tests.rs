//! Adoption over holders that run in this process: a first `Holders` stands for the daemon that
//! started the terminals and is gone (its link is parted from, and its terminals are never
//! dropped, so nothing is released), and a second one, over the same registry directory, for the
//! daemon that starts next.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::*;
use crate::coordinator::test_support;
#[cfg(unix)]
use crate::coordinator::test_support::left_running_script;
use crate::holder::client::{in_process, unresolved, FoundTerminal, Holders};
use crate::holder::protocol::SpawnSpec;
use crate::platform::term::Size;
use crate::protocol::SessionStatus;
use crate::test_support::{
    console_with_project_and_accounts, is_running, programs, project_session, KillOnDrop,
    ScratchDir, PATIENCE,
};

/// The terminals a previous daemon left in the holder of `dir`, each started with its meta, and the
/// pids of their programs. The previous daemon's terminals are leaked on purpose, since dropping
/// one releases it in the holder; its link is parted from.
fn left_behind(dir: &ScratchDir, terminals: Vec<(&str, Value, programs::Program)>) -> Vec<i32> {
    let previous = in_process(dir.join("holders")).expect("holders");
    let mut pids = Vec::new();
    for (id, meta, program) in terminals {
        let held = previous
            .spawn(SpawnSpec {
                id: id.to_string(),
                command: program.term_command(),
                size: Size { rows: 24, cols: 80 },
                containment: None,
                kill_session_leftovers: false,
                meta,
                companion: false,
            })
            .expect("the program starts in the holder");
        pids.push(held.pid().0 as i32);
        std::mem::forget(held);
    }
    drop(previous);
    pids
}

/// What the next daemon finds in the holders of `dir`. Its `Holders` is returned too, to be kept
/// alive for as long as the terminals are.
fn found_in(dir: &ScratchDir) -> (Holders, Vec<FoundTerminal>) {
    let next = in_process(dir.join("holders")).expect("holders");
    let terminals = test_support::found_in(&next);
    (next, terminals)
}

/// [`found_in`] again and again until `ready` holds of what it finds, for up to [`PATIENCE`]: what
/// a program printed while no daemon was linked reaches the tail the holder keeps only when the
/// holder's reader has read it.
async fn found_when(
    dir: &ScratchDir,
    ready: impl Fn(&[FoundTerminal]) -> bool,
) -> (Holders, Vec<FoundTerminal>) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let (next, found) = found_in(dir);
        if ready(&found) {
            return (next, found);
        }
        assert!(Instant::now() < deadline, "the holder never showed it");
        // Parted from, and the terminals leaked as the previous daemon's are: dropping a found
        // terminal releases it in its holder.
        next.shutdown();
        std::mem::forget(found);
        drop(next);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn tail_has(found: &[FoundTerminal], text: &str) -> bool {
    found
        .first()
        .is_some_and(|terminal| String::from_utf8_lossy(&terminal.tail).contains(text))
}

fn session_meta(session: &str, scratch: &std::path::Path) -> Value {
    json!({
        "kind": "session",
        "session_id": session,
        "launch_id": format!("launch-{session}"),
        "agent": AgentId::TEST,
        "scratch": scratch.to_string_lossy(),
        "mcp_token": format!("token-{session}"),
        "resolves_approvals_itself": true,
        "native_stash": false,
        "launch_prompt": null,
        "adoption": null,
    })
}

fn record(state: &AppState, id: &str, status: SessionStatus) {
    state
        .store
        .insert_session(&project_session(id, AgentId::TEST, status))
        .expect("the session is stored");
}

/// Waits until exactly `count` of `pids` still run: a terminal that was ended is reaped by the time
/// its end returns, but its program is seen gone only then.
fn wait_until_running(pids: &[i32], count: usize) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let running = pids.iter().filter(|pid| is_running(**pid)).count();
        if running == count {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{running} programs run, not {count}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

async fn adopt_in_blocking(
    state: &Arc<AppState>,
    terminals: Vec<FoundTerminal>,
    run_dir: PathBuf,
) -> Adopted {
    let state = state.clone();
    tokio::task::spawn_blocking(move || adopt_found(&state, terminals, &run_dir))
        .await
        .expect("the adoption ran")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_sessions_terminal_is_adopted_with_its_token_scratch_directory_and_status() {
    let (state, dir) = console_with_project_and_accounts("adopt-session");
    record(&state, "s1", SessionStatus::Working);
    let run_dir = dir.join("run");
    let scratch = run_dir.join("s1");
    std::fs::create_dir_all(&scratch).unwrap();
    let pids = left_behind(
        &dir,
        vec![("t1", session_meta("s1", &scratch), programs::sleep(60))],
    );
    let _guard = KillOnDrop(pids.clone());

    let (_next, found) = found_in(&dir);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].meta["session_id"], "s1");
    let adopted = adopt_in_blocking(&state, found, run_dir).await;

    assert_eq!(adopted.sessions, ["s1"]);
    assert_eq!(adopted.scratch_dirs, std::slice::from_ref(&scratch));
    assert_eq!(adopted.ended, 0);
    let live = state.live_session("s1").expect("the session is live");
    assert_eq!(live.launch_id, "launch-s1");
    assert!(live.resolves_approvals_itself);
    assert_eq!(live.process_id().0 as i32, pids[0]);
    assert!(is_running(pids[0]));
    // Its token works again, and it was not interrupted.
    assert_eq!(
        state.session_for_mcp_token("token-s1").as_deref(),
        Some("s1")
    );
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Working
    );
    assert!(scratch.is_dir());
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_scratch_directory_the_daemon_made_for_the_session_is_kept() {
    let (state, dir) = console_with_project_and_accounts("adopt-scratch");
    record(&state, "s1", SessionStatus::Idle);
    let run_dir = dir.join("run");
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let pids = left_behind(
        &dir,
        vec![("t1", session_meta("s1", &elsewhere), programs::sleep(60))],
    );
    let _guard = KillOnDrop(pids);

    let (_next, found) = found_in(&dir);
    let adopted = adopt_in_blocking(&state, found, run_dir).await;
    assert_eq!(adopted.sessions, ["s1"]);
    assert!(adopted.scratch_dirs.is_empty());
    assert!(state.live_session("s1").is_some());
    assert!(elsewhere.is_dir(), "a directory the meta named was removed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_the_daemon_has_no_use_for_is_ended() {
    let (state, dir) = console_with_project_and_accounts("adopt-ended");
    record(&state, "archived", SessionStatus::Archived);
    record(&state, "stopped", SessionStatus::Interrupted);
    record(&state, "twice", SessionStatus::Working);
    let run_dir = dir.join("run");
    let sleeping = || programs::sleep(60);
    let pids = left_behind(
        &dir,
        vec![
            ("no-record", session_meta("ghost", &run_dir), sleeping()),
            ("archived", session_meta("archived", &run_dir), sleeping()),
            ("stopped", session_meta("stopped", &run_dir), sleeping()),
            ("twice-a", session_meta("twice", &run_dir), sleeping()),
            ("twice-b", session_meta("twice", &run_dir), sleeping()),
            ("mystery", json!({"kind": "mystery"}), sleeping()),
            ("nothing", Value::Null, sleeping()),
            (
                "broken",
                json!({"kind": "session", "session_id": 3}),
                sleeping(),
            ),
        ],
    );
    let _guard = KillOnDrop(pids.clone());

    let (_next, found) = found_in(&dir);
    assert_eq!(found.len(), 8);
    let adopted = adopt_in_blocking(&state, found, run_dir).await;

    // One of the two terminals of a session is adopted; every other is ended.
    assert_eq!(adopted.sessions, ["twice"]);
    assert_eq!(adopted.ended, 7);
    wait_until_running(&pids, 1);
    assert!(state.live_session("ghost").is_none());
    assert!(state.live_session("archived").is_none());
    assert!(state.live_session("stopped").is_none());
    assert!(state.live_session("twice").is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_terminal_is_put_back_with_its_project_and_ended_without_one() {
    let (state, dir) = console_with_project_and_accounts("adopt-shell");
    let meta = |id: &str, project: &str| {
        ShellMeta {
            id: id.to_string(),
            project_id: project.to_string(),
            worktree: None,
            cwd: dir.to_string_lossy().into_owned(),
            created_at: 5,
            shell_name: Some("sh".to_string()),
        }
        .to_value()
    };
    let pids = left_behind(
        &dir,
        vec![
            ("kept", meta("kept", "project-1"), programs::sleep(60)),
            (
                "orphan",
                meta("orphan", "no-such-project"),
                programs::sleep(60),
            ),
        ],
    );
    let _guard = KillOnDrop(pids.clone());

    let (_next, found) = found_in(&dir);
    let adopted = adopt_in_blocking(&state, found, dir.join("run")).await;

    assert_eq!(adopted.shells, ["kept"]);
    assert_eq!(adopted.ended, 1);
    let records = state.shell_terminals.list();
    assert_eq!(records.len(), 1);
    assert_eq!(
        (records[0].id.as_str(), records[0].project_id.as_str()),
        ("kept", "project-1")
    );
    assert_eq!(records[0].created_at, 5);
    assert_eq!(records[0].title, "sh");
    assert!(
        records[0].screen_inexact,
        "a put back terminal's screen is rebuilt"
    );
    wait_until_running(&pids, 1);
    crate::terminal::shell::close_all(&state).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_that_had_exited_is_adopted_with_its_output_and_goes_through_the_exit_path() {
    let (state, dir) = console_with_project_and_accounts("adopt-exited");
    record(&state, "s1", SessionStatus::Working);
    let run_dir = dir.join("run");
    left_behind(
        &dir,
        vec![(
            "t1",
            session_meta("s1", &run_dir.join("s1")),
            programs::echo("said before the daemon came"),
        )],
    );
    // The program prints and exits while no daemon is linked.
    let (_next, found) = found_when(&dir, |found| {
        found.first().is_some_and(|terminal| terminal.exited)
            && tail_has(found, "said before the daemon came")
    })
    .await;
    let adopted = adopt_in_blocking(&state, found, run_dir).await;
    assert_eq!(adopted.sessions, ["s1"]);

    let deadline = Instant::now() + PATIENCE;
    while state.live_session("s1").is_some() {
        assert!(Instant::now() < deadline, "the exit was never observed");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Interrupted
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_no_holder_runs_are_marked_interrupted_and_the_adopted_are_not() {
    let (state, _dir) = console_with_project_and_accounts("adopt-interrupt");
    record(&state, "adopted", SessionStatus::Working);
    record(&state, "gone", SessionStatus::Working);
    record(&state, "idle", SessionStatus::Idle);
    record(&state, "archived", SessionStatus::Archived);
    let changed = state
        .store
        .mark_unheld_sessions_interrupted(&["adopted".to_string()])
        .unwrap();
    assert_eq!(changed, 2);
    let status = |id: &str| state.session_record(id).unwrap().status;
    assert_eq!(status("adopted"), SessionStatus::Working);
    assert_eq!(status("gone"), SessionStatus::Interrupted);
    assert_eq!(status("idle"), SessionStatus::Interrupted);
    assert_eq!(status("archived"), SessionStatus::Archived);
    assert_eq!(state.store.mark_live_sessions_interrupted().unwrap(), 1);
    assert_eq!(status("adopted"), SessionStatus::Interrupted);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_holder_gone_from_the_registry_listing_is_removed_and_adopts_nothing() {
    let dir = ScratchDir::new("adopt-stale");
    let holders = dir.join("holders");
    let stale = crate::holder::registry::RegistryEntry {
        holder_id: "stale".to_string(),
        pid: 4_000_000,
        started: "4000000:1".to_string(),
        port: 1,
        secret: "x".to_string(),
        version: "v".to_string(),
        build_id: "b".to_string(),
        exe: "/x".to_string(),
    };
    crate::holder::registry::write(&holders, &stale).unwrap();
    let (_next, found) = found_in(&dir);
    assert!(found.is_empty());
    assert!(crate::holder::registry::read(&holders, "stale").is_none());
}

/// What an adopted terminal is, over a program that draws two lines, then reports its size each
/// time it is resized: its screen is rebuilt from the tail the holder kept, marked inexact on the
/// session and on what a client receives, and the program is resized one row and back, which it
/// reports, leaving the terminal and the parsed screen at the size it had.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_sessions_screen_is_rebuilt_from_the_tail_marked_inexact_and_redrawn() {
    use crate::test_support::within;
    let (state, dir) = console_with_project_and_accounts("adopt-inexact");
    record(&state, "s1", SessionStatus::Working);
    let run_dir = dir.join("run");
    let script = "printf 'first line\\r\\nsecond line\\r\\n'; \
                  trap 'echo \"size $(stty size)\"' WINCH; echo ready; while :; do sleep 0.05; done";
    let pid = left_running_script(
        &dir,
        "t1",
        session_meta("s1", &run_dir.join("s1")),
        script,
        Size { rows: 24, cols: 80 },
    );
    let _guard = KillOnDrop(vec![pid]);
    // The program draws and sets its trap while no daemon is linked; the adoption's resize must
    // come after the trap, or the program never reports it.
    let (_next, found) = found_when(&dir, |found| tail_has(found, "ready")).await;
    assert!(tail_has(&found, "second line"));
    adopt_in_blocking(&state, found, run_dir).await;

    let live = state.live_session("s1").expect("the session is live");
    assert!(live.screen_inexact());
    assert!(
        state
            .wire_session(state.session_record("s1").unwrap())
            .screen_inexact
    );
    let screen = live.screen_text();
    assert!(screen.contains("first line"), "{screen:?}");
    assert!(screen.contains("second line"), "{screen:?}");

    // The last size the program saw is the one it had, so the resize back came last.
    let reported = {
        let live = live.clone();
        within(PATIENCE, move || live.screen_text().contains("size 24 80")).await
    };
    assert!(
        reported,
        "the program never saw the resize: {:?}",
        live.screen_text()
    );
    assert_eq!(live.screen_text().lines().count(), 24);
}

/// A terminal whose holder reports the handover a screen was saved at gets that screen back, with
/// what the program wrote since replayed over it: marked inexact only when the saved screen was.
/// What the tail held before the handover's mark is not replayed over it. A screen saved at
/// another handover is not used, and the screen is rebuilt from the whole tail. Either way the
/// adoption takes the screen, so that nothing is left for a later start to replay the tail over
/// again. A screen that is marked, restored inexact or not restored, is redrawn: the program is
/// resized and reports it. A screen restored exact is not redrawn.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_screen_saved_at_the_handover_is_restored_with_the_tail_replayed_over_it() {
    use crate::holder::client::HeldAt;
    use crate::holder::protocol::HandoverTail;
    use crate::terminal::saved_screen::SavedScreen;
    use crate::test_support::within;

    // The token the holder reports, whether the saved screen was inexact, and whether it is used.
    for (reported, inexact, restored) in [
        ("token", false, true),
        ("token", true, true),
        ("another", false, false),
    ] {
        let (state, dir) =
            console_with_project_and_accounts(&format!("adopt-saved-{reported}-{inexact}"));
        record(&state, "s1", SessionStatus::Working);
        let run_dir = dir.join("run");
        let pid = left_running_script(
            &dir,
            "t1",
            session_meta("s1", &run_dir.join("s1")),
            "trap 'echo \"size $(stty size)\"' WINCH; \
             printf 'history before\\r\\n'; printf 'written since\\r\\n'; \
             while :; do sleep 0.05; done",
            Size { rows: 24, cols: 80 },
        );
        let _guard = KillOnDrop(vec![pid]);
        let (_next, mut found) = found_when(&dir, |found| tail_has(found, "written since")).await;
        // As a holder that carried the handover out reports it.
        let since = String::from_utf8_lossy(&found[0].tail)
            .find("written since")
            .expect("the tail holds the output since the mark") as u64;
        found[0].handover = Some(HandoverTail {
            token: reported.to_string(),
            since,
            truncated: false,
            ages: Vec::new(),
        });
        let at = HeldAt {
            holder_id: found[0].holder_id.clone(),
            terminal_id: found[0].id.clone(),
        };
        state.keep_restart_screen(
            "token",
            at.clone(),
            SavedScreen {
                inexact,
                drawn: false,
                ..SavedScreen::of_bytes(24, 80, b"\x1b[H\x1b[2Jsaved at the handover\r\n")
            },
        );
        adopt_in_blocking(&state, found, run_dir).await;
        assert!(
            !state.has_restart_screen(&at),
            "{reported} {inexact}: the adoption left the saved screen"
        );

        let live = state.live_session("s1").expect("the session is live");
        let marked = inexact || !restored;
        assert_eq!(live.screen_inexact(), marked, "{reported} {inexact}");
        assert_eq!(
            state
                .wire_session(state.session_record("s1").unwrap())
                .screen_inexact,
            marked
        );
        let shown = {
            let live = live.clone();
            within(PATIENCE, move || {
                let screen = live.screen_text();
                screen.contains("written since")
                    && screen.contains("saved at the handover") == restored
                    && screen.contains("history before") == !restored
                    && (!marked || screen.contains("size 24 80"))
            })
            .await
        };
        assert!(shown, "{reported} {inexact}: {:?}", live.screen_text());
        if !marked {
            // Nothing redraws an exact screen; give a resize the time it would have taken.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            assert!(
                !live.screen_text().contains("size 24 80"),
                "{reported} {inexact}: an exact screen was redrawn: {:?}",
                live.screen_text()
            );
        }
    }
}

/// A terminal started by this daemon is not marked.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_the_daemon_started_has_no_inexact_screen() {
    let (state, _dir) = console_with_project_and_accounts("adopt-exact");
    record(&state, "s1", SessionStatus::Working);
    let live = crate::test_support::running(&state, "s1");
    assert!(!live.screen_inexact());
    assert!(
        !state
            .wire_session(state.session_record("s1").unwrap())
            .screen_inexact
    );
}

/// A holder outside the compatible range is not adopted: its program is ended, and the session it
/// ran is one no holder runs, which the start marks interrupted like any other.
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_out_of_the_range_is_not_adopted_and_its_session_is_interrupted() {
    use crate::holder::client::in_process_with;
    use crate::holder::compat::Range;

    let (state, dir) = console_with_project_and_accounts("adopt-out-of-range");
    record(&state, "s1", SessionStatus::Working);
    let run_dir = dir.join("run");
    let scratch = run_dir.join("s1");
    std::fs::create_dir_all(&scratch).unwrap();
    let pids = left_behind(
        &dir,
        vec![("t1", session_meta("s1", &scratch), programs::sleep(60))],
    );
    let _guard = KillOnDrop(pids.clone());

    // A daemon whose floor is above the version the holder reports.
    let next = in_process_with(
        dir.join("holders"),
        std::sync::Arc::new(crate::holder::client::Unavailable),
        "99.1.0",
        Range {
            floor: "99.0.0".to_string(),
            own: "99.1.0".to_string(),
        },
    )
    .expect("holders");
    let found: Vec<FoundTerminal> = next
        .adopt_existing(unresolved())
        .found
        .into_iter()
        .flat_map(|holder| holder.terminals)
        .collect();
    assert!(found.is_empty());
    let adopted = adopt_in_blocking(&state, found, run_dir).await;

    assert!(adopted.sessions.is_empty());
    assert!(state.live_session("s1").is_none());
    wait_until_running(&pids, 0);
    let interrupted = state
        .store
        .mark_unheld_sessions_interrupted(&adopted.sessions)
        .expect("marked");
    assert_eq!(interrupted, 1);
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Interrupted
    );
}

/// The id of the one holder the registry of `dir` lists.
fn only_holder(dir: &ScratchDir) -> String {
    let listed = crate::holder::registry::list(&dir.join("holders"));
    assert_eq!(listed.len(), 1, "one holder runs");
    listed[0].holder_id.clone()
}

fn launched_in(holder: &str, session: &str, launch: &str) -> crate::store::HeldTerminalRecord {
    crate::store::HeldTerminalRecord {
        kind: crate::store::HeldKind::Session,
        owner_id: session.to_string(),
        holder_id: holder.to_string(),
        terminal_id: "t1".to_string(),
        launch_id: launch.to_string(),
    }
}

async fn claim_in_blocking(
    state: &Arc<AppState>,
    pending: Vec<String>,
    run_dir: PathBuf,
) -> Adopted {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let mut adopted = Adopted::default();
        unlinked::claim_at_start(&state, pending, Vec::new(), &run_dir, &mut adopted);
        adopted
    })
    .await
    .expect("the claims ran")
}

/// A session whose launch a holder that cannot be linked to runs stays claimed at start: not
/// interrupted, not launched again, its scratch directory kept, its launch followed by the ledger
/// and its holder shown pending. Once the holder is linked to, its terminal is taken over as at
/// start, and the token and the ledger's launch are still the original launch's.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_a_holder_that_cannot_be_linked_to_runs_stays_claimed_and_is_taken_over_later() {
    let (state, dir) = console_with_project_and_accounts("adopt-unlinked");
    record(&state, "s1", SessionStatus::Working);
    let run_dir = dir.join("run");
    let scratch = run_dir.join("s1");
    std::fs::create_dir_all(&scratch).unwrap();
    let pids = left_behind(
        &dir,
        vec![("t1", session_meta("s1", &scratch), programs::sleep(60))],
    );
    let _guard = KillOnDrop(pids.clone());
    let holder = only_holder(&dir);
    state
        .store
        .record_held_terminal(&launched_in(&holder, "s1", "launch-s1"))
        .unwrap();

    let adopted = claim_in_blocking(&state, vec![holder.clone()], run_dir.clone()).await;
    assert_eq!(adopted.claimed, ["s1"]);
    assert_eq!(adopted.scratch_dirs, std::slice::from_ref(&scratch));
    let interrupted = state
        .store
        .mark_unheld_sessions_interrupted(&adopted.claimed)
        .expect("marked");
    assert_eq!(interrupted, 0);
    let shown = state.wire_session(state.session_record("s1").unwrap());
    assert_eq!(shown.status, SessionStatus::Working);
    assert_eq!(
        shown.holder_link,
        Some(crate::protocol::HolderLink::Pending)
    );
    assert!(
        state.begin_launch("s1").is_err(),
        "a second copy could start"
    );
    assert_eq!(
        state
            .process_ledger
            .followed_session("launch-s1")
            .as_deref(),
        Some("s1")
    );

    // The holder answers now: what the holders' client resolves it to is done here by hand.
    let (_next, found) = found_in(&dir);
    let taken = adopt_in_blocking(&state, found, run_dir.clone()).await;
    assert_eq!(taken.sessions, ["s1"]);
    {
        let (state, run_dir, holder) = (state.clone(), run_dir.clone(), holder.clone());
        tokio::task::spawn_blocking(move || {
            unlinked::settle_holder(&state, &holder, Some(taken.sessions.as_slice()), &run_dir);
            unlinked::finish(&state, &run_dir);
        })
        .await
        .unwrap();
    }

    let live = state.live_session("s1").expect("the session is live");
    assert_eq!(live.launch_id, "launch-s1");
    assert_eq!(live.process_id().0 as i32, pids[0]);
    assert_eq!(
        state.session_for_mcp_token("token-s1").as_deref(),
        Some("s1")
    );
    assert_eq!(
        state
            .process_ledger
            .followed_session("launch-s1")
            .as_deref(),
        Some("s1")
    );
    assert_eq!(
        state.store.held_terminals().unwrap(),
        vec![launched_in(&holder, "s1", "launch-s1")]
    );
    let shown = state.wire_session(state.session_record("s1").unwrap());
    assert_eq!(shown.status, SessionStatus::Working);
    assert_eq!(shown.holder_link, None);
    assert!(!state.is_launching("s1"), "the claim was kept");
    assert!(scratch.is_dir());
}

/// A session claimed for a holder that turns out to be gone is interrupted then, its launch ended
/// in the ledger and its claim released.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_claimed_for_a_holder_that_is_gone_is_interrupted_once_it_is_known() {
    let (state, dir) = console_with_project_and_accounts("adopt-unlinked-gone");
    record(&state, "s1", SessionStatus::Idle);
    let run_dir = dir.join("run");
    state
        .store
        .record_held_terminal(&launched_in("gone", "s1", "launch-s1"))
        .unwrap();
    let adopted = claim_in_blocking(&state, vec!["gone".to_string()], run_dir.clone()).await;
    assert_eq!(adopted.claimed, ["s1"]);

    tokio::task::spawn_blocking({
        let state = state.clone();
        move || {
            unlinked::settle_holder(&state, "gone", None, &run_dir);
            unlinked::finish(&state, &run_dir);
        }
    })
    .await
    .unwrap();
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Interrupted
    );
    assert_eq!(state.process_ledger.followed_session("launch-s1"), None);
    assert!(!state.is_launching("s1"));
}

/// A session in a running status that no record places stays claimed while a holder that cannot
/// be linked to may run it, and resuming the interrupted sessions waits; once no such holder is
/// left it is handed back, and the resume, of the sessions the start listed, is the caller's to
/// run.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_no_record_places_waits_for_every_holder_that_cannot_be_linked_to() {
    let (state, dir) = console_with_project_and_accounts("adopt-unlinked-unplaced");
    record(&state, "s1", SessionStatus::Working);
    let adopted = claim_in_blocking(&state, vec!["h".to_string()], dir.join("run")).await;
    assert_eq!(adopted.claimed, ["s1"]);
    let among: std::collections::HashSet<String> = ["s1".to_string()].into();
    assert!(
        state.defer_start_work(Some(among.clone())),
        "the resume did not wait"
    );
    assert!(state.finish_unlinked().is_none(), "a holder is still tried");

    let run_dir = dir.join("run");
    tokio::task::spawn_blocking({
        let state = state.clone();
        move || unlinked::settle_holder(&state, "h", None, &run_dir)
    })
    .await
    .unwrap();
    let finished = state.finish_unlinked().expect("no holder is tried");
    assert_eq!(finished.unmatched, ["s1"]);
    assert!(finished.run_start_work);
    assert_eq!(finished.resume_among, Some(among));
    assert!(state.finish_unlinked().is_none(), "handed over twice");
}

/// A shell terminal's name the user gave it is kept with the terminal's identity: put back from
/// its holder, it shows the name, and neither the title the shell set before nor one it sets after
/// takes its place; a rename is stored, and closing the terminal forgets it.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_shell_terminals_user_name_is_kept_across_adoption_and_its_own_titles() {
    use crate::test_support::within;
    let (state, dir) = console_with_project_and_accounts("adopt-shell-name");
    let meta = ShellMeta {
        id: "t1".to_string(),
        project_id: "project-1".to_string(),
        worktree: None,
        cwd: dir.to_string_lossy().into_owned(),
        created_at: 5,
        shell_name: Some("sh".to_string()),
    }
    .to_value();
    let script = "printf '\\033]0;from the tail\\007'; echo ready; read line; \
                  printf '\\033]0;later\\007'; echo done; while :; do sleep 0.05; done";
    let pid = left_running_script(&dir, "t1", meta, script, Size { rows: 24, cols: 80 });
    let _guard = KillOnDrop(vec![pid]);
    // Named by the user under the daemon that is gone.
    state
        .store
        .set_shell_terminal_title("t1", "my server")
        .unwrap();

    let (_next, found) = found_when(&dir, |found| tail_has(found, "ready")).await;
    adopt_in_blocking(&state, found, dir.join("run")).await;
    let shown = || state.shell_terminals.list()[0].clone();
    assert_eq!(shown().title, "my server");
    assert!(shown().renamed);

    let process = state.shell_terminals.process("t1").expect("put back");
    assert!(process.write(b"\n").is_ok(), "the write failed");
    let drawn = {
        let process = process.clone();
        within(PATIENCE, move || process.screen_text().contains("done")).await
    };
    assert!(drawn, "{:?}", process.screen_text());
    assert_eq!(shown().title, "my server");

    crate::terminal::shell::rename(&state, "t1", "renamed".to_string()).unwrap();
    assert_eq!(
        state.store.shell_terminal_title("t1").unwrap().as_deref(),
        Some("renamed")
    );
    crate::terminal::shell::close(&state, "t1").await.unwrap();
    assert_eq!(state.store.shell_terminal_title("t1").unwrap(), None);
}

/// A terminal of a session a launch claims (a resume under way, say, when a holder that could not
/// be linked to at start is linked to) is not adopted beside the launch: it is ended.
#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_of_a_session_being_launched_is_ended_not_adopted() {
    let (state, dir) = console_with_project_and_accounts("adopt-being-launched");
    record(&state, "s1", SessionStatus::Working);
    let run_dir = dir.join("run");
    let pids = left_behind(
        &dir,
        vec![(
            "t1",
            session_meta("s1", &run_dir.join("s1")),
            programs::sleep(60),
        )],
    );
    let _guard = KillOnDrop(pids.clone());
    let launching = state
        .begin_launch("s1")
        .expect("the launch claims the session");

    let (_next, found) = found_in(&dir);
    let adopted = adopt_in_blocking(&state, found, run_dir).await;
    assert!(adopted.sessions.is_empty());
    assert_eq!(adopted.ended, 1);
    assert!(state.live_session("s1").is_none());
    wait_until_running(&pids, 0);
    assert!(state.is_launching("s1"), "the launch lost its claim");
    drop(launching);
}

/// A session of a holder being ended off the start (here for a welcome that cannot be read) stays
/// claimed until the holder is gone, and the resume waits for it and includes it: it is resumed
/// once it has no process, never beside it.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_of_a_holder_being_ended_waits_for_its_end_and_is_resumed_after() {
    let (state, dir) = console_with_project_and_accounts("adopt-ending");
    record(&state, "s1", SessionStatus::Working);
    state
        .store
        .record_held_terminal(&launched_in("ending", "s1", "launch-s1"))
        .unwrap();
    let run_dir = dir.join("run");
    let adopted = {
        let (state, run_dir) = (state.clone(), run_dir.clone());
        tokio::task::spawn_blocking(move || {
            let mut adopted = Adopted::default();
            unlinked::claim_at_start(
                &state,
                Vec::new(),
                vec!["ending".to_string()],
                &run_dir,
                &mut adopted,
            );
            adopted
        })
        .await
        .unwrap()
    };
    assert_eq!(adopted.claimed, ["s1"]);
    assert_eq!(state.unlinked_resumable(), ["s1"]);
    let among: std::collections::HashSet<String> = ["s1".to_string()].into();
    assert!(state.defer_start_work(Some(among.clone())));
    assert!(
        state.begin_launch("s1").is_err(),
        "a launch could start beside it"
    );

    tokio::task::spawn_blocking({
        let state = state.clone();
        move || unlinked::settle_holder(&state, "ending", None, &run_dir)
    })
    .await
    .unwrap();
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Interrupted
    );
    let finished = state.finish_unlinked().expect("no holder is tried");
    assert!(finished.run_start_work);
    assert_eq!(finished.resume_among, Some(among));
}

fn saved_screen(text: &str) -> crate::terminal::saved_screen::SavedScreen {
    crate::terminal::saved_screen::SavedScreen::of_bytes(24, 80, text.as_bytes())
}

/// The preview kept for `session`, as text.
fn preview_of(state: &AppState, session: &str) -> Option<String> {
    state
        .saved_output(session)
        .map(|preview| String::from_utf8_lossy(&preview).into_owned())
}

fn held_at(holder: &str, terminal: &str) -> crate::holder::client::HeldAt {
    crate::holder::client::HeldAt {
        holder_id: holder.to_string(),
        terminal_id: terminal.to_string(),
    }
}

/// The screen a planned stop saved for a session whose holder is gone at the next start becomes
/// the preview the session is interrupted with; a session already interrupted keeps its own, and
/// a session of a holder still tried keeps none (it is not interrupted). Once the previews are
/// written the gone holder's screens are consumed, its shell terminal's, one no record names and
/// one whose preview could not be written too, which nothing could use any more; the pending
/// holder's stays. A screen of a size no screen has, from an altered file, is previewed at
/// another.
#[tokio::test(flavor = "multi_thread")]
async fn a_saved_screen_of_a_gone_holder_becomes_the_sessions_preview() {
    let (state, dir) = console_with_project_and_accounts("adopt-gone-preview");
    for (id, status) in [
        ("gone", SessionStatus::Working),
        ("done", SessionStatus::Interrupted),
        ("pending", SessionStatus::Working),
        // No preview can be written under an id that is not a file name.
        ("not.a.file", SessionStatus::Working),
        ("odd", SessionStatus::Working),
    ] {
        record(&state, id, status);
    }
    state.keep_saved_output("done", b"its own preview");
    for (session, holder, terminal) in [
        ("gone", "h-gone", "t1"),
        ("done", "h-gone", "t2"),
        ("not.a.file", "h-gone", "t3"),
        ("pending", "h-pending", "t1"),
    ] {
        state
            .store
            .record_held_terminal(&crate::store::HeldTerminalRecord {
                terminal_id: terminal.to_string(),
                ..launched_in(holder, session, &format!("launch-{session}"))
            })
            .unwrap();
        state.keep_restart_screen(
            "token",
            held_at(holder, terminal),
            saved_screen(&format!("screen of {session}")),
        );
    }

    state
        .store
        .record_held_terminal(&crate::store::HeldTerminalRecord {
            kind: crate::store::HeldKind::ShellTerminal,
            terminal_id: "t4".to_string(),
            ..launched_in("h-gone", "shell-1", "shell-1")
        })
        .unwrap();
    state
        .store
        .record_held_terminal(&crate::store::HeldTerminalRecord {
            terminal_id: "t6".to_string(),
            ..launched_in("h-gone", "odd", "launch-odd")
        })
        .unwrap();
    state.keep_restart_screen(
        "token",
        held_at("h-gone", "t6"),
        crate::terminal::saved_screen::SavedScreen::of_bytes(0, u16::MAX, b"screen of odd"),
    );
    for terminal in ["t4", "t5"] {
        state.keep_restart_screen(
            "token",
            held_at("h-gone", terminal),
            saved_screen("a shell"),
        );
    }

    claim_in_blocking(&state, vec!["h-pending".to_string()], dir.join("run")).await;

    let preview = preview_of(&state, "gone").expect("a preview");
    assert!(preview.contains("screen of gone"), "{preview:?}");
    assert_eq!(
        state.saved_output("done"),
        Some(b"its own preview".to_vec())
    );
    assert_eq!(state.saved_output("pending"), None);
    assert_eq!(state.saved_output("not.a.file"), None);
    let odd = preview_of(&state, "odd").expect("a preview");
    assert!(odd.contains("screen of odd"), "{odd:?}");
    for (terminal, left) in [
        (held_at("h-gone", "t1"), false),
        (held_at("h-gone", "t2"), false),
        (held_at("h-gone", "t3"), false),
        (held_at("h-gone", "t4"), false),
        (held_at("h-gone", "t5"), false),
        (held_at("h-gone", "t6"), false),
        (held_at("h-pending", "t1"), true),
    ] {
        assert_eq!(state.has_restart_screen(&terminal), left, "{terminal:?}");
    }
}

/// A holder that could not be linked to at start and turns out to be gone has the screen saved for
/// its session kept as the preview the session is interrupted with.
#[tokio::test(flavor = "multi_thread")]
async fn a_saved_screen_of_a_holder_found_gone_later_becomes_the_sessions_preview() {
    let (state, dir) = console_with_project_and_accounts("adopt-resolved-gone-preview");
    record(&state, "s1", SessionStatus::Working);
    state
        .store
        .record_held_terminal(&launched_in("h-late", "s1", "launch-s1"))
        .unwrap();
    state.keep_restart_screen(
        "token",
        held_at("h-late", "t1"),
        saved_screen("screen of s1"),
    );
    let run_dir = dir.join("run");
    let adopted = claim_in_blocking(&state, vec!["h-late".to_string()], run_dir.clone()).await;
    assert_eq!(adopted.claimed, ["s1"]);
    assert_eq!(state.saved_output("s1"), None, "the holder is still tried");

    let on_resolved = unlinked::on_resolved(&state, &run_dir);
    on_resolved(crate::holder::client::Resolved::Gone {
        holder_id: "h-late".to_string(),
        tails: Vec::new(),
    });

    let deadline = Instant::now() + PATIENCE;
    while state.session_record("s1").unwrap().status != SessionStatus::Interrupted {
        assert!(Instant::now() < deadline, "the session was not interrupted");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let preview = preview_of(&state, "s1").expect("a preview");
    assert!(preview.contains("screen of s1"), "{preview:?}");
    let deadline = Instant::now() + PATIENCE;
    while state.has_restart_screen(&held_at("h-late", "t1")) {
        assert!(
            Instant::now() < deadline,
            "the saved screen was not consumed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A holder ended by a start that follows an ordinary stop gives the tail of each terminal: the
/// session's preview is the screen rebuilt from it, preferred to an older saved screen, and the
/// saved screen is the preview where the tail has nothing to show.
#[tokio::test(flavor = "multi_thread")]
async fn a_tail_of_an_ended_holder_becomes_the_sessions_preview_over_a_saved_screen() {
    use crate::holder::client::{EndedTail, Resolved};

    let (state, dir) = console_with_project_and_accounts("adopt-ended-tail-preview");
    for (session, launch, terminal) in [("s1", "launch-s1", "t1"), ("s2", "launch-s2", "t2")] {
        record(&state, session, SessionStatus::Working);
        state
            .store
            .record_held_terminal(&crate::store::HeldTerminalRecord {
                terminal_id: terminal.to_string(),
                ..launched_in("h-ended", session, launch)
            })
            .unwrap();
        state.keep_restart_screen(
            "token",
            held_at("h-ended", terminal),
            saved_screen(&format!("saved screen of {session}")),
        );
    }
    let run_dir = dir.join("run");
    claim_in_blocking(&state, vec!["h-ended".to_string()], run_dir.clone()).await;

    let on_resolved = unlinked::on_resolved(&state, &run_dir);
    on_resolved(Resolved::Gone {
        holder_id: "h-ended".to_string(),
        tails: vec![
            EndedTail {
                terminal_id: "t1".to_string(),
                size: None,
                tail: b"the last words".to_vec(),
                handover: None,
            },
            EndedTail {
                terminal_id: "t2".to_string(),
                size: None,
                tail: Vec::new(),
                handover: None,
            },
        ],
    });

    let deadline = Instant::now() + PATIENCE;
    while state.session_record("s2").unwrap().status != SessionStatus::Interrupted
        || state.session_record("s1").unwrap().status != SessionStatus::Interrupted
    {
        assert!(
            Instant::now() < deadline,
            "the sessions were not interrupted"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let rebuilt = preview_of(&state, "s1").expect("a preview");
    assert!(rebuilt.contains("the last words"), "{rebuilt:?}");
    assert!(!rebuilt.contains("saved screen of s1"), "{rebuilt:?}");
    let saved = preview_of(&state, "s2").expect("a preview");
    assert!(saved.contains("saved screen of s2"), "{saved:?}");
}

/// A tail that still holds the mark of the handover a saved screen was saved at makes the preview
/// the saved screen with the output since the mark on it; a tail that does not (another handover)
/// is rebuilt raw, as the saved screen cannot be told to be the one it follows.
#[tokio::test(flavor = "multi_thread")]
async fn a_tail_that_holds_the_handover_mark_is_previewed_on_the_saved_screen() {
    use crate::holder::client::{EndedTail, Resolved};
    use crate::holder::protocol::HandoverTail;

    let (state, dir) = console_with_project_and_accounts("adopt-ended-tail-exact-preview");
    let tail = b"stale words\r\nfresh words".to_vec();
    let since = "stale words\r\n".len() as u64;
    let mut tails = Vec::new();
    for (session, launch, terminal, token) in [
        ("s1", "launch-s1", "t1", "token"),
        ("s2", "launch-s2", "t2", "another"),
    ] {
        record(&state, session, SessionStatus::Working);
        state
            .store
            .record_held_terminal(&crate::store::HeldTerminalRecord {
                terminal_id: terminal.to_string(),
                ..launched_in("h-ended", session, launch)
            })
            .unwrap();
        state.keep_restart_screen(
            "token",
            held_at("h-ended", terminal),
            saved_screen(&format!("saved screen of {session}\r\n")),
        );
        tails.push(EndedTail {
            terminal_id: terminal.to_string(),
            size: Some(Size { rows: 24, cols: 80 }),
            tail: tail.clone(),
            handover: Some(HandoverTail {
                token: token.to_string(),
                since,
                truncated: false,
                ages: Vec::new(),
            }),
        });
    }
    let run_dir = dir.join("run");
    claim_in_blocking(&state, vec!["h-ended".to_string()], run_dir.clone()).await;

    let on_resolved = unlinked::on_resolved(&state, &run_dir);
    on_resolved(Resolved::Gone {
        holder_id: "h-ended".to_string(),
        tails,
    });

    let deadline = Instant::now() + PATIENCE;
    while state.session_record("s2").unwrap().status != SessionStatus::Interrupted
        || state.session_record("s1").unwrap().status != SessionStatus::Interrupted
    {
        assert!(
            Instant::now() < deadline,
            "the sessions were not interrupted"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let exact = String::from_utf8_lossy(&state.saved_output("s1").expect("a preview")).to_string();
    assert!(exact.contains("saved screen of s1"), "{exact}");
    assert!(exact.contains("fresh words"), "{exact}");
    assert!(!exact.contains("stale words"), "{exact}");
    let raw = String::from_utf8_lossy(&state.saved_output("s2").expect("a preview")).to_string();
    assert!(raw.contains("stale words"), "{raw}");
    assert!(!raw.contains("saved screen of s2"), "{raw}");
}

/// The meta of a Codex session's terminal, which is [`session_meta`] for the agent Codex.
fn codex_meta(session: &str, scratch: &std::path::Path) -> Value {
    let mut meta = session_meta(session, scratch);
    meta["agent"] = json!(AgentId::CODEX);
    meta
}

/// A Codex session `id` on record, working, whose conversation the agent calls `thread-1`.
fn record_codex(state: &AppState, id: &str) {
    let mut session = project_session(id, AgentId::CODEX, SessionStatus::Working);
    session.agent_session_id = Some("thread-1".to_string());
    state
        .store
        .insert_session(&session)
        .expect("the session is stored");
}

/// A title the same terminal set before adoption still proves Codex passed its startup screens,
/// whether it comes from the holder's tail or the exact screen saved at a planned handover.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_codex_session_accepts_messages_after_its_startup_title_is_restored() {
    use crate::holder::protocol::HandoverTail;

    for exact in [false, true] {
        let (state, dir) = console_with_project_and_accounts(&format!("adopt-codex-ready-{exact}"));
        record_codex(&state, "s1");
        let run_dir = dir.join("run");
        let left = left_with_server(&dir, codex_meta("s1", &run_dir.join("s1")));
        let _guard = KillOnDrop(vec![left.terminal, left.companion]);
        let (_next, mut found) = found_in(&dir);
        let screen = b"\x1b]2;Codex session\x07ready before the daemon stopped\r\n";
        if exact {
            found[0].tail.clear();
            found[0].handover = Some(HandoverTail {
                token: "ready-handover".to_string(),
                since: 0,
                truncated: false,
                ages: Vec::new(),
            });
            state.keep_restart_screen(
                "ready-handover",
                held_at(&found[0].holder_id, &found[0].id),
                crate::terminal::saved_screen::SavedScreen::of_bytes(24, 80, screen),
            );
        } else {
            found[0].tail = screen.to_vec();
        }

        let adopted = adopt_in_blocking(&state, found, run_dir).await;
        assert_eq!(adopted.sessions, ["s1"]);
        let live = state.live_session("s1").expect("the session is live");
        assert_eq!(live.screen_inexact(), !exact);
        assert!(live
            .screen_text()
            .contains("ready before the daemon stopped"));
        let interrupted = crate::session::interrupt::interrupt_session(&state, "s1")
            .await
            .expect("the adopted session is no longer rejected as still starting");
        assert!(
            !interrupted.interrupted,
            "the stand-in shows no active turn"
        );
        crate::coordinator::messages::send_message(&state, "s1", "hello again".to_string())
            .await
            .expect("the adopted session is past its startup screens");

        let requests = left.requests.lock().unwrap().clone();
        let (_, added) = requests
            .iter()
            .find(|(method, _)| method == "thread/queue/add")
            .expect("the message went through the adopted session into Codex's queue");
        assert_eq!(added["threadId"], "thread-1");
        assert_eq!(added["input"][0]["text"], "hello again");
        assert!(
            added["clientUserMessageId"]
                .as_str()
                .is_some_and(|id| id.starts_with("launch-s1:")),
            "{added:?}"
        );
        assert!(is_running(left.terminal));
        assert!(is_running(left.companion));
    }
}

/// Stored status, title and conversation identity may belong to an earlier launch. None lets a
/// message answer the current launch's startup or folder-trust screen.
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_codex_session_without_its_own_startup_title_still_refuses_messages() {
    let trust_screen = include_bytes!("../../../testdata/codex_trust_screen.bin");
    for (label, tail) in [
        ("starting", b"starting Codex".as_slice()),
        ("trust", trust_screen.as_slice()),
        ("cleared", b"\x1b]2;old title\x07\x1b]2;\x07".as_slice()),
    ] {
        let (state, dir) = console_with_project_and_accounts(&format!("adopt-codex-held-{label}"));
        record_codex(&state, "s1");
        state
            .store
            .set_terminal_title("s1", Some("title from an earlier launch"))
            .unwrap();
        let run_dir = dir.join("run");
        let pids = left_behind(
            &dir,
            vec![(
                "t1",
                codex_meta("s1", &run_dir.join("s1")),
                programs::sleep(60),
            )],
        );
        let _guard = KillOnDrop(pids);
        let (_next, mut found) = found_in(&dir);
        found[0].tail = tail.to_vec();
        let adopted = adopt_in_blocking(&state, found, run_dir).await;
        assert_eq!(adopted.sessions, ["s1"]);

        let refused = crate::coordinator::messages::send_message(
            &state,
            "s1",
            "do not type this".to_string(),
        )
        .await
        .expect_err("the current terminal has not shown Codex's startup signal");
        assert_eq!(
            refused
                .downcast_ref::<crate::protocol::CodedError>()
                .expect("a coded refusal")
                .code,
            crate::protocol::error_code::SESSION_TRUST_PENDING,
            "{label}: {refused:#}"
        );
    }
}

/// A console host can set a title before Codex is ready; adoption preserves the rule that its
/// parsed composer, rather than the host's title, establishes readiness.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_host_rendered_codex_terminal_does_not_start_at_its_title() {
    let (state, dir) = console_with_project_and_accounts("adopt-codex-host-title");
    record_codex(&state, "s1");
    let run_dir = dir.join("run");
    let pids = left_behind(
        &dir,
        vec![(
            "t1",
            codex_meta("s1", &run_dir.join("s1")),
            programs::sleep(60),
        )],
    );
    let _guard = KillOnDrop(pids);
    let (_next, mut found) = found_in(&dir);
    found[0].tail = b"loading\x1b]2;codex.exe\x07".to_vec();
    adopt_in_blocking(&state, found, run_dir).await;

    let refused =
        crate::coordinator::messages::send_message(&state, "s1", "do not type this".to_string())
            .await
            .expect_err("the host title does not say Codex is ready");
    assert_eq!(
        refused
            .downcast_ref::<crate::protocol::CodedError>()
            .expect("a coded refusal")
            .code,
        crate::protocol::error_code::SESSION_TRUST_PENDING
    );
}

/// A stand-in for Codex's app server listening on the socket `path`, answering what a message
/// written to the session's queue asks of it; gives every request it was sent, by method and
/// parameters.
#[cfg(unix)]
fn fake_codex_on(path: &std::path::Path) -> Arc<std::sync::Mutex<Vec<(String, Value)>>> {
    use tokio_tungstenite::tungstenite::{self, Message};
    let listener = std::os::unix::net::UnixListener::bind(path).expect("the socket binds");
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let log = log.clone();
            std::thread::spawn(move || {
                let Ok(mut socket) = tungstenite::accept(stream) else {
                    return;
                };
                while let Ok(message) = socket.read() {
                    let Ok(request) =
                        serde_json::from_str::<Value>(message.to_text().unwrap_or(""))
                    else {
                        continue;
                    };
                    // A notification, which is answered by nothing.
                    let Some(id) = request.get("id").cloned() else {
                        continue;
                    };
                    let method = request["method"].as_str().unwrap_or_default().to_string();
                    log.lock()
                        .unwrap()
                        .push((method.clone(), request["params"].clone()));
                    let result = match method.as_str() {
                        "thread/loaded/list" => json!({"data":["thread-1"],"nextCursor":null}),
                        "thread/queue/list" => json!({"data":[],"nextCursor":null}),
                        "thread/queue/add" => json!({"queuedSubmission":{"id":"q1"}}),
                        _ => json!({}),
                    };
                    let reply = json!({"id":id,"result":result}).to_string();
                    let _ = socket.send(Message::Text(reply));
                }
            });
        }
    });
    requests
}

/// What a previous daemon left in a holder: a terminal bound to a companion, the stand-in for
/// Codex's server listening at the companion's socket.
#[cfg(unix)]
struct LeftWithServer {
    terminal: i32,
    companion: i32,
    requests: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
}

/// Leaves a terminal with `meta` in the holder of `dir`, bound to its companion as a launch in a
/// holder that runs them starts it, as a daemon that is gone would: its terminal and its
/// companion's handle are leaked, since dropping one releases it in the holder, and its link is
/// parted from.
#[cfg(unix)]
fn left_with_server(dir: &ScratchDir, meta: Value) -> LeftWithServer {
    use crate::holder::protocol::{CompanionEndpoint, EndpointKind, Ready};
    let previous = in_process(dir.join("holders")).expect("holders");
    let issued = previous
        .issue_launch("s1", None)
        .expect("the holder issues the launch's credentials");
    let endpoint = issued
        .open_endpoint(EndpointKind::Socket {
            prefix: "octo-test".to_string(),
            name: "rpc".to_string(),
        })
        .expect("the endpoint opens");
    let CompanionEndpoint::Socket { path } = &endpoint else {
        panic!("{endpoint:?}");
    };
    let requests = fake_codex_on(std::path::Path::new(path));
    let mut server = crate::platform::term::Command::new("/bin/sh");
    server.args(["-c", "exec sleep 60"]);
    let (companion, _) = issued
        .start_companion(server, Ready::SocketExists, PATIENCE, None)
        .expect("the companion starts");
    let held = issued
        .spawn(SpawnSpec {
            id: "t1".to_string(),
            command: programs::sleep(60).term_command(),
            size: Size { rows: 24, cols: 80 },
            containment: None,
            kill_session_leftovers: false,
            meta,
            companion: true,
        })
        .expect("the terminal starts bound to its companion");
    let left = LeftWithServer {
        terminal: held.pid().0 as i32,
        companion: companion.pid().0 as i32,
        requests,
    };
    std::mem::forget((held, companion));
    drop(previous);
    left
}

/// The server the holder runs with an adopted Codex session's terminal is linked to again, so a
/// message to the session still goes into Codex's queue, and for the conversation the store knows
/// by the agent's id of it: the successor learned nothing of it from the server.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_codex_sessions_message_goes_through_its_servers_queue_with_the_thread_from_the_store(
) {
    let (state, dir) = console_with_project_and_accounts("adopt-codex-server");
    record_codex(&state, "s1");
    let run_dir = dir.join("run");
    let left = left_with_server(&dir, codex_meta("s1", &run_dir.join("s1")));
    let _guard = KillOnDrop(vec![left.terminal, left.companion]);

    let (_next, found) = found_in(&dir);
    assert_eq!(found.len(), 1);
    assert!(found[0].companion.is_some());
    let adopted = adopt_in_blocking(&state, found, run_dir).await;
    assert_eq!(adopted.sessions, ["s1"]);
    let transport = state
        .live_session("s1")
        .expect("the session is live")
        .transport
        .clone()
        .expect("the adopted session reaches its server");
    let sent = tokio::task::spawn_blocking(move || transport.send("hello again", "launch-s1:5"))
        .await
        .unwrap();
    assert_eq!(sent.unwrap(), crate::session::term::Sent::Queued);

    let requests = left.requests.lock().unwrap().clone();
    let (_, added) = requests
        .iter()
        .find(|(method, _)| method == "thread/queue/add")
        .expect("the message went into the server's queue");
    assert_eq!(added["threadId"], "thread-1");
    assert_eq!(added["clientUserMessageId"], "launch-s1:5");
    assert!(is_running(left.terminal), "the terminal was not ended");
}

/// A server the welcome lists exited is a server that is gone: the terminal is ended for it, and
/// the session goes through its exit path as one whose server died.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_terminal_whose_companion_exited_goes_through_the_exit_path() {
    let (state, dir) = console_with_project_and_accounts("adopt-codex-server-gone");
    record_codex(&state, "s1");
    let run_dir = dir.join("run");
    let left = left_with_server(&dir, codex_meta("s1", &run_dir.join("s1")));
    let _guard = KillOnDrop(vec![left.terminal, left.companion]);
    crate::test_support::kill(left.companion);

    // The holder ends the terminal with its companion, whether or not a daemon is linked.
    let (_next, found) = found_when(&dir, |found| {
        found.first().is_some_and(|terminal| {
            terminal.exited
                && terminal
                    .companion
                    .as_ref()
                    .is_some_and(|companion| companion.server.exited())
        })
    })
    .await;
    let adopted = adopt_in_blocking(&state, found, run_dir).await;
    assert_eq!(adopted.sessions, ["s1"]);

    let deadline = Instant::now() + PATIENCE;
    while state.live_session("s1").is_some() {
        assert!(Instant::now() < deadline, "the exit was never observed");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        state.session_record("s1").unwrap().status,
        SessionStatus::Interrupted
    );
}

/// A terminal the welcome lists no companion with, as every one an older holder runs, is adopted
/// without a transport: the session's messages go to its terminal.
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_codex_terminal_with_no_companion_has_no_transport() {
    let (state, dir) = console_with_project_and_accounts("adopt-codex-no-server");
    record_codex(&state, "s1");
    let run_dir = dir.join("run");
    let pids = left_behind(
        &dir,
        vec![(
            "t1",
            codex_meta("s1", &run_dir.join("s1")),
            programs::sleep(60),
        )],
    );
    let _guard = KillOnDrop(pids);

    let (_next, found) = found_in(&dir);
    assert!(found[0].companion.is_none());
    let adopted = adopt_in_blocking(&state, found, run_dir).await;
    assert_eq!(adopted.sessions, ["s1"]);
    let live = state.live_session("s1").expect("the session is live");
    assert!(live.transport.is_none());
}
