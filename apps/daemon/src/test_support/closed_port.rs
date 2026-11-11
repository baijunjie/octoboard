//! A loopback TCP port nothing listens on, and that no parallel test is likely to be given.
//!
//! Binding a listener on port 0 and dropping it hands the port back to the system, and a test
//! running in parallel that binds port 0 next can be given the very number: a connection to it then
//! reaches that test's server instead of being refused. (Holding the port in a socket that never
//! listens does not refuse either: macOS lets the connection time out.) The system hands out port
//! 0's ports from its ephemeral range, which starts at 32768 or above on every host the daemon
//! runs on, so a port below that range is found by trying to bind one, and is left alone by every
//! other test.

use std::net::TcpListener;

/// The first port tried and how many are: well below every system's ephemeral range.
const FIRST: u16 = 10_000;
const TRIED: u16 = 10_000;

/// A port that refuses connections.
pub(crate) struct ClosedPort {
    pub(crate) port: u16,
}

impl ClosedPort {
    pub(crate) fn new() -> Self {
        // Started at a place of this process's own, so that tests of several binaries running at
        // once do not all try the same port first.
        let start = std::process::id() as u16 % TRIED;
        for offset in 0..TRIED {
            let port = FIRST + (start + offset) % TRIED;
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                return Self { port };
            }
        }
        panic!("no free port between {FIRST} and {}", FIRST + TRIED);
    }
}
