//! Starting a session's private Codex app server through the launch's host, and watching it for
//! ending by itself.
//!
//! The server listens where the host opened its endpoint: a Unix socket in a private directory, or
//! a loopback WebSocket port guarded by a capability token, of which the server is given only the
//! SHA-256 and which the terminal reads from its environment, whichever the platform offers
//! (`ServerListen::for_host`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::rpc::Endpoint;
use crate::platform::shell::ResolvedBinary;
use crate::platform::term::Command;
use crate::terminal::server_host::{
    RunningServer, ServerEndpoint, ServerHost, ServerListen, ServerReady,
};

/// How long the server is given to start listening.
#[cfg(not(test))]
const START_TIMEOUT: Duration = Duration::from_secs(15);
/// How often the server is looked at for having exited by itself.
pub(super) const EXIT_POLL: Duration = Duration::from_millis(250);
/// What the server prints right before its port once it listens on loopback,
/// `listening on: ws://127.0.0.1:<port>`.
const PORT_PREFIX: &str = "listening on: ws://127.0.0.1:";

/// [`START_TIMEOUT`]; under test `PATIENCE`, which a test of the timeout itself lowers with
/// `START_TIMEOUT_OVERRIDE`.
fn start_timeout() -> Duration {
    #[cfg(test)]
    {
        START_TIMEOUT_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    START_TIMEOUT
}

#[cfg(test)]
thread_local! {
    pub(super) static START_TIMEOUT_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// The `app-server` subcommand and how it listens on `endpoint`.
pub(super) fn server_args(endpoint: &ServerEndpoint) -> Vec<String> {
    let mut args = vec!["app-server".to_string(), "--listen".to_string()];
    match endpoint {
        ServerEndpoint::Socket(path) => args.push(format!("unix://{}", path.display())),
        ServerEndpoint::Loopback { token, .. } => {
            use sha2::Digest;
            args.extend(
                [
                    "ws://127.0.0.1:0",
                    "--ws-auth",
                    "capability-token",
                    "--ws-token-sha256",
                ]
                .map(str::to_string),
            );
            args.push(hex(&sha2::Sha256::digest(token.as_bytes())));
        }
    }
    args
}

/// Lower-case hexadecimal.
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The port of a line the server prints once it listens, `listening on: ws://127.0.0.1:<port>`.
#[cfg(test)]
pub(super) fn listening_port(line: &str) -> Option<u16> {
    crate::platform::proc::server::port_after(line, PORT_PREFIX)
}

/// Starts the server with `args` through `host`, on an endpoint opened for this start alone, and
/// waits for it to listen: the running server and how a client reaches it. **Blocks.**
pub(super) fn launch_server(
    host: &dyn ServerHost,
    binary: &ResolvedBinary,
    args: &[String],
    environment: &HashMap<String, String>,
    cwd: &Path,
) -> Result<(Box<dyn RunningServer>, Endpoint)> {
    let endpoint = host
        .open_endpoint(ServerListen::for_host("octo-cdx", "rpc"))
        .context("preparing the Codex server's endpoint")?;
    let mut command = Command::new(&binary.program);
    command
        .args(&binary.leading_args)
        .args(args)
        .args(server_args(&endpoint))
        .env_clear();
    for (key, value) in environment {
        command.env(key, value);
    }
    command.cwd(cwd);
    // Codex 0.166 links the socket's path to a socket elsewhere; the link resolves once it listens.
    let ready = match &endpoint {
        ServerEndpoint::Socket(_) => ServerReady::SocketExists,
        ServerEndpoint::Loopback { .. } => ServerReady::PortAfter(PORT_PREFIX),
    };
    let (server, port) = host
        .start(command, ready, start_timeout())
        .context("starting the private Codex app server")?;
    match Endpoint::reached_at(endpoint, port) {
        Some(endpoint) => Ok((server, endpoint)),
        None => {
            server.end();
            Err(anyhow!("the private Codex app server reported no port"))
        }
    }
}

/// Watches the server for exiting by itself, and then calls `exited`, unless `stopping` says by then
/// that it was ended on purpose.
pub(super) fn watch_for_exit(
    server: Arc<Mutex<Option<Box<dyn RunningServer>>>>,
    stopping: impl Fn() -> bool + Send + 'static,
    exited: impl FnOnce() + Send + 'static,
) {
    std::thread::spawn(move || loop {
        std::thread::sleep(EXIT_POLL);
        if stopping() {
            return;
        }
        let gone = match server.lock().expect("Codex server lock poisoned").as_ref() {
            Some(server) => server.exited(),
            None => return,
        };
        if gone {
            if !stopping() {
                exited();
            }
            return;
        }
    });
}
