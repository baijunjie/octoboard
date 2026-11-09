//! The client's requests for a terminal's companion, and what it learns of one, over holders that
//! run in this process; the companions are shells that make the socket they are waited for on and
//! stay up. A first `Holders` stands for the daemon that started the launch and is gone (its
//! terminals are leaked, since dropping one releases it in the holder), a second for the daemon
//! that starts next.

use std::time::{Duration, Instant};

use super::*;
use crate::holder::companion_test_support::{directory_of, socket_kind, touching};
use crate::holder::protocol::{CompanionEndpoint, Ready, SpawnSpec};
use crate::platform::term::Size;
use crate::test_support::{is_running, kill, programs, KillOnDrop, ScratchDir, PATIENCE};

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn spec(companion: bool) -> SpawnSpec {
    SpawnSpec {
        id: "replaced-by-the-launch".to_string(),
        command: programs::sleep(60).term_command(),
        size: Size { rows: 24, cols: 80 },
        containment: None,
        kill_session_leftovers: false,
        meta: serde_json::Value::Null,
        companion,
    }
}

/// A launch in `holders` whose companion runs on a socket, the terminal not started yet.
fn launched(holders: &Holders) -> (Issued, CompanionEndpoint, HeldCompanion) {
    let issued = holders
        .issue_launch("s1", None)
        .expect("the holder issues the launch's credentials");
    assert!(issued.companions, "a holder of this build runs companions");
    let endpoint = issued
        .open_endpoint(socket_kind())
        .expect("the endpoint opens");
    let CompanionEndpoint::Socket { path } = &endpoint else {
        panic!("{endpoint:?}");
    };
    let (companion, port) = issued
        .start_companion(touching(path), Ready::SocketExists, PATIENCE, None)
        .expect("the companion starts");
    assert_eq!(port, None);
    (issued, endpoint, companion)
}

/// The launch of [`launched`] with its terminal started and bound to the companion, as a daemon
/// that is gone left it in the holder of `dir`: the endpoint, and the pids of the companion and of
/// the terminal, which the guard ends.
fn left_behind(dir: &ScratchDir) -> (CompanionEndpoint, i32, i32, KillOnDrop) {
    let previous = in_process(dir.join("holders")).expect("holders");
    let (issued, endpoint, companion) = launched(&previous);
    let companion_pid = companion.pid().0 as i32;
    let held = issued
        .spawn(spec(true))
        .expect("the terminal starts bound to its companion");
    let terminal_pid = held.pid().0 as i32;
    let guard = KillOnDrop(vec![terminal_pid, companion_pid]);
    std::mem::forget((held, companion));
    drop(previous);
    (endpoint, companion_pid, terminal_pid, guard)
}

#[test]
fn a_terminal_the_next_daemon_finds_carries_its_companion_which_reads_as_running() {
    let dir = ScratchDir::new("client-companion-found");
    let (endpoint, companion_pid, _, _guard) = left_behind(&dir);

    let next = in_process(dir.join("holders")).expect("holders");
    let found = next.adopt_existing(unresolved()).found;
    assert_eq!(found.len(), 1);
    let terminal = &found[0].terminals[0];
    let listed = terminal
        .companion
        .as_ref()
        .expect("the companion is listed with its terminal");
    assert_eq!(listed.endpoint, endpoint);
    assert!(!listed.server.exited());
    assert!(is_running(companion_pid));
}

#[test]
fn a_terminal_started_without_a_companion_carries_none() {
    let dir = ScratchDir::new("client-companion-none");
    let previous = in_process(dir.join("holders")).expect("holders");
    let held = previous.spawn(spec(false)).expect("the terminal starts");
    let _guard = KillOnDrop(vec![held.pid().0 as i32]);
    std::mem::forget(held);
    drop(previous);

    let next = in_process(dir.join("holders")).expect("holders");
    let found = next.adopt_existing(unresolved()).found;
    assert_eq!(found.len(), 1);
    assert!(found[0].terminals[0].companion.is_none());
}

/// The holder's report that a companion exited by itself reaches the handle of a companion found
/// with an adopted terminal.
#[test]
fn a_found_companion_reads_as_exited_once_the_holder_reports_its_end() {
    let dir = ScratchDir::new("client-companion-ended-found");
    let (_, companion_pid, _, _guard) = left_behind(&dir);
    let next = in_process(dir.join("holders")).expect("holders");
    let found = next.adopt_existing(unresolved()).found;
    let listed = found[0].terminals[0]
        .companion
        .as_ref()
        .expect("the companion is listed with its terminal");
    assert!(!listed.server.exited());

    kill(companion_pid);
    wait_until("the companion's end was not reported to its handle", || {
        listed.server.exited()
    });
}

/// The same report reaches the handle of a companion the daemon started itself.
#[test]
fn a_started_companion_reads_as_exited_once_the_holder_reports_its_end() {
    let dir = ScratchDir::new("client-companion-ended-started");
    let holders = in_process(dir.join("holders")).expect("holders");
    let (issued, _, companion) = launched(&holders);
    let companion_pid = companion.pid().0 as i32;
    let held = issued
        .spawn(spec(true))
        .expect("the terminal starts bound to its companion");
    let _guard = KillOnDrop(vec![held.pid().0 as i32, companion_pid]);
    assert!(!companion.exited());

    kill(companion_pid);
    wait_until("the companion's end was not reported to its handle", || {
        companion.exited()
    });
    drop((held, companion, holders));
}

/// A companion the welcome lists exited is a server that is gone, however it came to be.
#[test]
fn a_companion_that_exited_before_the_next_daemon_linked_reads_as_exited() {
    let dir = ScratchDir::new("client-companion-ended-before");
    let (_, companion_pid, _, _guard) = left_behind(&dir);
    kill(companion_pid);

    // Linked again until the holder has seen the companion go, which it does on its own time.
    let deadline = Instant::now() + PATIENCE;
    loop {
        let next = in_process(dir.join("holders")).expect("holders");
        let found = next.adopt_existing(unresolved()).found;
        let exited = found[0].terminals[0]
            .companion
            .as_ref()
            .expect("the companion is listed with its terminal")
            .server
            .exited();
        if exited {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the companion was never listed as exited"
        );
        // Parted from, and the terminals leaked as the previous daemon's are: dropping a found
        // terminal releases it in its holder.
        next.shutdown();
        std::mem::forget(found);
        drop(next);
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A companion no terminal has bound is ended through the link, which removes its endpoint.
#[test]
fn a_companion_can_be_ended_over_the_link_before_its_terminal_binds_it() {
    let dir = ScratchDir::new("client-companion-end");
    let holders = in_process(dir.join("holders")).expect("holders");
    let (issued, endpoint, companion) = launched(&holders);
    let pid = companion.pid().0 as i32;
    let _guard = KillOnDrop(vec![pid]);
    assert!(directory_of(&endpoint).is_dir());

    companion
        .end()
        .expect("the holder ends a companion no terminal has bound");
    assert!(companion.exited());
    wait_until("the companion kept running", || !is_running(pid));
    wait_until("the endpoint was not removed", || {
        !directory_of(&endpoint).exists()
    });
    drop((issued, holders));
}

/// A launch that falls back ends what it opened through its credentials, and its terminal then
/// starts without a companion.
#[test]
fn a_launch_that_falls_back_ends_its_companion_and_starts_its_terminal_unbound() {
    let dir = ScratchDir::new("client-companion-fallback");
    let holders = in_process(dir.join("holders")).expect("holders");
    let (issued, endpoint, companion) = launched(&holders);
    let pid = companion.pid().0 as i32;
    let _guard = KillOnDrop(vec![pid]);

    issued
        .end_companion()
        .expect("the holder ends the launch's companion");
    assert!(companion.exited());
    wait_until("the companion kept running", || !is_running(pid));
    wait_until("the endpoint was not removed", || {
        !directory_of(&endpoint).exists()
    });

    let held = issued
        .spawn(spec(false))
        .expect("the terminal starts without a companion");
    let _terminal = KillOnDrop(vec![held.pid().0 as i32]);
    drop((held, holders));
}

/// A terminal that bound its companion ends it by itself: the holder refuses the end, and the
/// companion goes on running.
#[test]
fn a_companion_its_terminal_has_bound_cannot_be_ended_over_the_link() {
    let dir = ScratchDir::new("client-companion-bound");
    let holders = in_process(dir.join("holders")).expect("holders");
    let (issued, _, companion) = launched(&holders);
    let pid = companion.pid().0 as i32;
    let held = issued
        .spawn(spec(true))
        .expect("the terminal starts bound to its companion");
    let _guard = KillOnDrop(vec![held.pid().0 as i32, pid]);

    companion
        .end()
        .expect_err("the companion is its terminal's now");
    assert!(!companion.exited());
    assert!(is_running(pid));
    drop((held, companion, holders));
}
