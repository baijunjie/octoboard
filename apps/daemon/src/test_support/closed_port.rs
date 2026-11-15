//! A loopback TCP port nothing listens on, and that no parallel test is likely to be given.
//!
//! Binding a listener on port 0 and dropping it hands the port back to the system, and a test
//! running in parallel that binds port 0 next can be given the very number: a connection to it then
//! reaches that test's server instead of being refused. (Holding the port in a socket that never
//! listens does not refuse either: macOS lets the connection time out.) The system hands out port
//! 0's ports from its ephemeral range, which starts at 32768 or above on every host the daemon
//! runs on, so a port below that range is found by trying to bind one, and is left alone by every
//! other test. Within one test process no two `ClosedPort`s alive at once are the same port: finding
//! one binds it for a moment, and a second finder that tried the same port at that moment would
//! hold a port that a test reading this process's listening sockets sees as listening, so the
//! ports handed out are recorded and a port is tried only while no live `ClosedPort` has it. A
//! test that goes on to rely on the port keeps the `ClosedPort` in a local for as long as it does:
//! the port is only handed out by [`ClosedPort::port`], so a number copied out of a temporary
//! `ClosedPort` is a port the next finder in the binary probes first, since every finder starts at
//! the same place.

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::sync::Mutex;

/// The first port tried and how many are: well below every system's ephemeral range.
const FIRST: u16 = 10_000;
const TRIED: u16 = 10_000;

/// The ports the live `ClosedPort`s of this process hold.
static HANDED_OUT: Mutex<BTreeSet<u16>> = Mutex::new(BTreeSet::new());

/// A port that refuses connections.
pub(crate) struct ClosedPort {
    port: u16,
}

impl ClosedPort {
    pub(crate) fn new() -> Self {
        // Started at a place of this process's own, so that tests of several binaries running at
        // once do not all try the same port first.
        let start = std::process::id() as u16 % TRIED;
        // Held across the probes: a second finder waits until the first has bound and dropped its
        // probe, and then skips the port.
        let mut handed_out = HANDED_OUT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for offset in 0..TRIED {
            let port = FIRST + (start + offset) % TRIED;
            if !handed_out.contains(&port) && TcpListener::bind(("127.0.0.1", port)).is_ok() {
                handed_out.insert(port);
                return Self { port };
            }
        }
        panic!("no free port between {FIRST} and {}", FIRST + TRIED);
    }

    /// The port. Valid only while `self` lives: a copy of it that outlives the `ClosedPort` can
    /// be the port the next finder binds, so a test names the port through this and keeps the
    /// `ClosedPort` in a local for as long as it relies on it.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ClosedPort {
    fn drop(&mut self) {
        HANDED_OUT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.port);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_live_closed_ports_are_never_the_same_port() {
        let held: Vec<ClosedPort> = (0..50).map(|_| ClosedPort::new()).collect();
        let distinct: BTreeSet<u16> = held.iter().map(|closed| closed.port()).collect();
        assert_eq!(distinct.len(), held.len());
        assert!(held
            .iter()
            .all(|closed| TcpListener::bind(("127.0.0.1", closed.port())).is_ok()));
    }

    #[test]
    fn a_port_is_held_while_its_closed_port_lives_and_let_go_when_it_is_dropped() {
        let closed = ClosedPort::new();
        let port = closed.port();
        let held = |port: u16| HANDED_OUT.lock().unwrap().contains(&port);
        assert!(held(port));
        drop(closed);
        assert!(!held(port), "a dropped ClosedPort's port stayed claimed");
    }

    #[test]
    fn closed_ports_found_on_several_threads_at_once_are_all_different() {
        let found: Vec<Vec<ClosedPort>> = (0..8)
            .map(|_| std::thread::spawn(|| (0..10).map(|_| ClosedPort::new()).collect()))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        let ports: Vec<u16> = found.iter().flatten().map(|closed| closed.port()).collect();
        let distinct: BTreeSet<u16> = ports.iter().copied().collect();
        assert_eq!(distinct.len(), ports.len());
    }
}
