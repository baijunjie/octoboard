//! Which holders a daemon adopts at start, over holders that run in this process: a first
//! `Holders` of one version stands for the daemon that started the terminals and is gone (it is
//! parted from, and its terminals are never dropped, which would release them), and a second one,
//! whose range says what it adopts, for the daemon that starts next.

use std::time::{Duration, Instant};

use super::*;
use crate::holder::protocol::SpawnSpec;
use crate::test_support::{is_running, programs, KillOnDrop, ScratchDir, PATIENCE};

/// The holders found gone, each with the terminals it ran.
type GoneHolders = Arc<std::sync::Mutex<Vec<(String, Vec<String>)>>>;

fn range(floor: &str, own: &str) -> Range {
    Range {
        floor: floor.to_string(),
        own: own.to_string(),
    }
}

fn spec(id: &str) -> SpawnSpec {
    SpawnSpec {
        id: id.to_string(),
        command: programs::sleep(60).term_command(),
        size: Size { rows: 24, cols: 80 },
        containment: None,
        kill_session_leftovers: false,
        meta: serde_json::Value::Null,
        companion: false,
    }
}

/// Starts a program in a holder that reports `version`, as a daemon that is gone left it, and
/// returns its pid. The gone daemon's link is parted from; its terminal is leaked, since
/// dropping it releases it in the holder.
fn left_behind(dir: &ScratchDir, version: &str) -> i32 {
    let previous = in_process_with(
        dir.join("holders"),
        Arc::new(Unavailable),
        version,
        range("0.0.0", version),
    )
    .expect("holders");
    let held = previous.spawn(spec("old")).expect("the program starts");
    let pid = held.pid().0 as i32;
    std::mem::forget(held);
    drop(previous);
    pid
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn next_daemon(dir: &ScratchDir, floor: &str, own: &str) -> Holders {
    in_process_with(
        dir.join("holders"),
        Arc::new(Unavailable),
        own,
        range(floor, own),
    )
    .expect("holders")
}

/// A link another daemon took the holder over from is finished, not opened again: that would take
/// the holder back from the daemon that has it.
#[test]
fn a_link_another_daemon_took_the_holder_over_from_is_lost_without_being_opened_again() {
    let dir = ScratchDir::new("range-superseded");
    let previous = next_daemon(&dir, "0.2.0", "0.4.0");
    let held = previous.spawn(spec("old")).expect("the program starts");
    let pid = held.pid().0 as i32;
    let _guard = KillOnDrop(vec![pid]);
    let link = locked(&previous.current).clone().expect("the current link");

    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    let found = next.adopt_existing(unresolved()).found;
    assert_eq!(found.len(), 1);
    wait_until("the first daemon's link was not finished", || !link.alive());
    link.settled();
    assert_eq!(link.connections(), 1, "the first link was opened again");
    assert_eq!(held.exited_unreaped(), Some(true));
    assert!(is_running(pid), "the holder's program was ended");
    assert_eq!(found[0].terminals.len(), 1);
    assert!(!found[0].terminals[0].exited);
    drop((held, previous));
}

#[test]
fn a_holder_in_the_range_is_adopted_with_its_terminals() {
    let dir = ScratchDir::new("range-adopted");
    let pid = left_behind(&dir, "0.3.0");
    let _guard = KillOnDrop(vec![pid]);
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    let found = next.adopt_existing(unresolved()).found;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "0.3.0");
    assert_eq!(found[0].terminals.len(), 1);
    assert!(is_running(pid));
}

fn out_of_range_is_ended(version: &str, label: &str) {
    let dir = ScratchDir::new(label);
    let pid = left_behind(&dir, version);
    let _guard = KillOnDrop(vec![pid]);
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    let found = next.adopt_existing(unresolved()).found;
    assert!(found.is_empty(), "no holder outside the range is adopted");
    assert!(
        locked(&next.shared.endings).is_empty(),
        "an ending is waited for only where every holder is ended"
    );
    wait_until("the program was not ended", || !is_running(pid));
    // Drained and parted from, the holder has nothing left to run and exits.
    let registry_dir = dir.join("holders");
    wait_until("the holder did not exit", || {
        registry::list(&registry_dir).is_empty()
    });
}

#[test]
fn a_holder_older_than_the_floor_has_its_terminals_ended_and_exits() {
    out_of_range_is_ended("0.1.0", "range-too-old");
}

#[test]
fn a_holder_newer_than_the_daemon_has_its_terminals_ended_and_exits() {
    out_of_range_is_ended("0.5.0", "range-too-new");
}

#[test]
fn a_holder_of_another_build_in_the_range_is_adopted_drained_and_given_no_new_terminals() {
    let dir = ScratchDir::new("range-other-build");
    let pid = left_behind(&dir, "0.3.0");
    let _guard = KillOnDrop(vec![pid]);
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    let mut found = next.adopt_existing(unresolved()).found;
    assert_eq!(found.len(), 1);
    let old = found.remove(0);
    // A terminal started now goes to a holder of its own.
    let fresh = next.spawn(spec("new")).expect("the program starts");
    let registry_dir = dir.join("holders");
    assert_eq!(registry::list(&registry_dir).len(), 2);
    assert_ne!(fresh.pid().0 as i32, pid);
    assert!(is_running(pid), "the adopted holder keeps its terminal");

    // Its last terminal ends, and it exits; the holder of the new terminal stays.
    for terminal in old.terminals {
        terminal.held.end();
    }
    wait_until("the drained holder did not exit", || {
        registry::list(&registry_dir).len() == 1
    });
    assert!(registry::list(&registry_dir)
        .iter()
        .all(|entry| entry.holder_id != old.holder_id));
    fresh.end();
}

/// A holder that answers every hello with a message this daemon cannot read as a welcome: a
/// connection whose handshake the daemon gave up on is followed by another, as for any holder.
#[cfg(unix)]
async fn garbled_holder() -> u16 {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                socket.next().await;
                socket
                    .send(Message::Text(r#"{"type":"welcome_v99"}"#.to_string()))
                    .await
                    .ok();
                tokio::time::timeout(PATIENCE, async {
                    while let Some(Ok(_)) = socket.next().await {}
                })
                .await
                .ok();
            });
        }
    });
    port
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_holder_whose_welcome_cannot_be_read_is_ended_by_its_process() {
    let dir = ScratchDir::new("range-unreadable");
    let port = garbled_holder().await;
    // A process that stands for the holder, which the registry names by its identity.
    let mut stand_in = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("a process");
    let start = crate::platform::proc_table::start_of(stand_in.id() as i32).expect("it runs");
    let pid = stand_in.id();
    // Reaped as soon as it ends, as a holder is by the daemon that started it: the daemon waits
    // for it to be gone, which a process that is not reaped does not look like.
    let reaper = std::thread::spawn(move || stand_in.wait());
    let registry_dir = dir.join("holders");
    registry::write(
        &registry_dir,
        &RegistryEntry {
            holder_id: "garbled".to_string(),
            pid,
            started: format!("{pid}:{start}"),
            port,
            secret: "secret".to_string(),
            version: "9.9.9".to_string(),
            build_id: "x".to_string(),
            exe: "/x".to_string(),
        },
    )
    .unwrap();

    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    // Kept until the end: a daemon that stops ends a holder it is still trying to link to by its
    // process, which leaves the file the ending under test removes.
    let (next, found) = tokio::task::spawn_blocking(move || {
        let found = next.adopt_existing(unresolved()).found;
        (next, found)
    })
    .await
    .unwrap();
    assert!(found.is_empty());
    let status = reaper.join().unwrap().expect("it ended");
    assert!(!status.success(), "the process was killed");
    // Ended off the start, which removes the file once the process is gone.
    wait_until("the registry file was not removed", || {
        registry::read(&registry_dir, "garbled").is_none()
    });
    tokio::task::spawn_blocking(move || drop(next))
        .await
        .unwrap();
}

/// A quit's record, scoped to the holders the registry lists, handed to `next`.
fn end_what_is_listed(next: &Holders, dir: &ScratchDir) -> Arc<QuitTargets> {
    let record = dir.join("application-quit");
    std::fs::write(&record, b"").unwrap();
    let targets = crate::holder::quit_targets::reconcile(&record, &dir.join("holders"))
        .expect("the holders listed are targets");
    next.end_quit_targets(targets.clone());
    targets
}

/// A quit target in the compatible range has its terminals ended like a holder outside it, and
/// exits; the record is gone once its ending is confirmed.
#[test]
fn a_quit_target_in_the_range_is_ended_and_leaves_the_record() {
    let dir = ScratchDir::new("range-end-all");
    let pid = left_behind(&dir, "0.3.0");
    let _guard = KillOnDrop(vec![pid]);
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    let targets = end_what_is_listed(&next, &dir);
    let found = next.adopt_existing(unresolved()).found;
    assert!(found.is_empty(), "no holder is adopted");
    assert_eq!(locked(&next.shared.endings).len(), 1);
    assert!(next.wait_endings(PATIENCE), "the ending was not confirmed");
    assert!(locked(&next.shared.endings).is_empty(), "the handles stay");
    assert!(targets.remaining().is_empty());
    assert!(!dir.join("application-quit").exists());
    wait_until("the program was not ended", || !is_running(pid));
    let registry_dir = dir.join("holders");
    wait_until("the holder did not exit", || {
        registry::list(&registry_dir).is_empty()
    });
}

/// A quit target ended resolves as gone with the tail of each terminal it ran, for the screens no
/// daemon will show again.
#[test]
fn a_quit_target_resolves_as_gone_with_its_terminals_tails() {
    let dir = ScratchDir::new("range-end-all-tails");
    let pid = left_behind(&dir, "0.3.0");
    let _guard = KillOnDrop(vec![pid]);
    let registry_dir = dir.join("holders");
    let holder_id = registry::list(&registry_dir)[0].holder_id.clone();
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    end_what_is_listed(&next, &dir);
    let gone: GoneHolders = Arc::default();
    let told = gone.clone();
    next.adopt_existing(Arc::new(move |resolved| {
        if let Resolved::Gone { holder_id, tails } = resolved {
            let terminals = tails.into_iter().map(|ended| ended.terminal_id).collect();
            told.lock().unwrap().push((holder_id, terminals));
        }
    }));
    assert!(next.wait_endings(PATIENCE));
    assert_eq!(
        *gone.lock().unwrap(),
        vec![(holder_id, vec!["old".to_string()])]
    );
}

/// A holder the record does not name is adopted as usual, and the record is left as it is; a target
/// with no registry file is resolved at once.
#[test]
fn a_holder_that_is_no_quit_target_is_adopted() {
    let dir = ScratchDir::new("range-quit-other");
    let pid = left_behind(&dir, "0.3.0");
    let _guard = KillOnDrop(vec![pid]);
    let record = dir.join("application-quit");
    let ghost = crate::holder::quit_targets::Target {
        holder_id: "ghost".to_string(),
        started: "1:1".to_string(),
    };
    let named = crate::holder::quit_targets::QuitTargets::new(record.clone(), vec![ghost]);
    std::fs::write(
        &record,
        br#"{"version":1,"targets":[{"holder_id":"ghost","started":"1:1"}]}"#,
    )
    .unwrap();
    let named = Arc::new(named);
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    next.end_quit_targets(named.clone());
    assert!(named.remaining().is_empty(), "no registry file, so gone");
    assert!(!record.exists());
    let found = next.adopt_existing(unresolved()).found;
    assert_eq!(
        found.len(),
        1,
        "the holder the record does not name is adopted"
    );
    assert!(is_running(pid));
}

/// An ending that did not confirm, or that is still running once the wait's bound passes, is
/// reported as unconfirmed; the handles are taken either way, the unfinished one left to carry on.
#[test]
fn waiting_for_endings_reports_one_that_failed_or_outlasts_the_bound() {
    let dir = ScratchDir::new("range-wait-endings");
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    let push = |holder: &str, ending: std::thread::JoinHandle<bool>| {
        locked(&next.shared.endings).push((holder.to_string(), ending));
    };
    push("done", std::thread::spawn(|| true));
    assert!(next.wait_endings(PATIENCE));
    push("done", std::thread::spawn(|| true));
    push("stuck", std::thread::spawn(|| false));
    assert!(
        !next.wait_endings(PATIENCE),
        "a holder that cannot be ended"
    );
    assert!(locked(&next.shared.endings).is_empty());
    push(
        "slow",
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(2));
            true
        }),
    );
    let started = Instant::now();
    assert!(!next.wait_endings(Duration::from_millis(100)));
    assert!(started.elapsed() < Duration::from_secs(1), "the bound held");
    assert!(locked(&next.shared.endings).is_empty());
}

/// The versions of the holders linked to come from their welcomes, those still tried again from
/// their registry entries, and the link being opened does not hold the reading up.
#[test]
fn the_versions_of_the_holders_are_those_linked_to_and_those_still_tried() {
    let dir = ScratchDir::new("range-versions");
    let pid = left_behind(&dir, "0.3.0");
    let _guard = KillOnDrop(vec![pid]);
    let next = next_daemon(&dir, "0.2.0", "0.4.0");
    let found = next.adopt_existing(unresolved()).found;
    let linked = found[0].holder_id.clone();
    let pending = RegistryEntry {
        holder_id: "pending".to_string(),
        pid: 1,
        started: "1:1".to_string(),
        port: 1,
        secret: "secret".to_string(),
        version: "0.2.5".to_string(),
        build_id: "x".to_string(),
        exe: "/x".to_string(),
    };
    locked(&next.shared.pending).insert("pending".to_string(), pending);

    // As `link` holds it while a holder starts.
    let opening = locked(&next.current);
    let versions = next.versions();
    drop(opening);

    assert_eq!(versions.len(), 2);
    assert_eq!(versions.get(&linked).map(String::as_str), Some("0.3.0"));
    assert_eq!(versions.get("pending").map(String::as_str), Some("0.2.5"));
}
