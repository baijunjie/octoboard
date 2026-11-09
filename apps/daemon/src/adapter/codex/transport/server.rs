//! Starting a session's private Codex app server, waiting for it to listen, ending it, and watching
//! it for ending by itself.
//!
//! The server listens on the platform's private channel (`crate::platform::ipc`): a Unix socket in
//! a private directory, or a loopback WebSocket port guarded by a capability token, of which the
//! server is given only the SHA-256 and which the terminal reads from its environment.

use std::collections::HashMap;
use std::io::{self, BufRead, Read};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

use super::rpc::Endpoint;
use crate::platform::ipc::{Address, PrivateListen};
use crate::platform::proc::{self, crash, ChildHandle, Containment, Polite, ProcessId, Reach};
use crate::platform::shell::ResolvedBinary;

/// How long the server is given to start listening.
#[cfg(not(test))]
const START_TIMEOUT: Duration = Duration::from_secs(15);
/// How long the server is given to exit after the polite request, before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(2);
/// How often the server is looked at for having exited by itself.
pub(super) const EXIT_POLL: Duration = Duration::from_millis(250);

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

/// Where a server listens, before it is started. One serves a single start: a server started again
/// is given a new one, so nothing the last left behind is in its way.
pub(super) struct Listen(PrivateListen);

impl Listen {
    /// What the host listens on, as the platform chooses it.
    pub(super) fn for_host() -> io::Result<Self> {
        PrivateListen::for_host("octo-cdx", "rpc").map(Self)
    }

    #[cfg(test)]
    pub(super) fn loopback() -> io::Result<Self> {
        PrivateListen::loopback().map(Self)
    }

    pub(super) fn address(&self) -> Address<'_> {
        self.0.address()
    }

    /// The `app-server` subcommand and how it listens.
    pub(super) fn server_args(&self) -> Vec<String> {
        let mut args = vec!["app-server".to_string(), "--listen".to_string()];
        match self.address() {
            Address::Socket(path) => args.push(format!("unix://{}", path.display())),
            Address::Loopback { token } => {
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

    /// Whether the server's output is read, for the port it was given.
    fn reads_output(&self) -> bool {
        matches!(self.address(), Address::Loopback { .. })
    }
}

/// Lower-case hexadecimal.
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The port of a line the server prints once it listens, `listening on: ws://127.0.0.1:<port>`.
pub(super) fn listening_port(line: &str) -> Option<u16> {
    let (_, rest) = line.split_once("listening on:")?;
    let rest = rest.trim().strip_prefix("ws://127.0.0.1:")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|port| *port != 0)
}

/// The running server, ended when this is dropped.
pub(super) struct ServerProcess {
    pub(super) child: Child,
    pub(super) handle: ChildHandle,
    pub(super) crash_cleanup: Option<crash::Entry>,
}

impl ServerProcess {
    pub(super) fn exited(&self) -> bool {
        self.handle.exited_unreaped() == Some(true)
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let pid = self.handle.pid();
        // The leader is kept unreaped until its session has been swept, so its id cannot be reused.
        // A host with no polite request to send ends it at once.
        if self.handle.exited_unreaped() == Some(false)
            && proc::stop_group(pid, Polite::Terminate).delivered()
        {
            let deadline = Instant::now() + STOP_GRACE;
            while self.handle.exited_unreaped() == Some(false) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        if self.handle.exited_unreaped().is_some() {
            self.handle.kill();
            self.handle.kill_leftovers(&[]).log(pid, "Codex app server");
        }
        self.crash_cleanup.take();
        let _ = self.child.wait();
    }
}

/// Starts the server with `args` and waits for it to listen as `listen` says. The server leads a
/// session of its own (on Windows, is in a job of its own nested in the launch's), so what it runs
/// for the model ends with it, and it is in the launch's containment, so the launch's process
/// ledger accounts for what leaves it.
pub(super) fn launch_server(
    binary: &ResolvedBinary,
    args: &[String],
    listen: &Listen,
    environment: &HashMap<String, String>,
    cwd: &Path,
    containment: &Containment,
) -> Result<(ServerProcess, Endpoint)> {
    let mut command = binary.command();
    command
        .args(args)
        .args(listen.server_args())
        .env_clear()
        .envs(environment)
        .current_dir(cwd)
        .stdin(Stdio::null());
    let output = if listen.reads_output() {
        Stdio::piped
    } else {
        Stdio::null
    };
    command.stdout(output()).stderr(output());
    Reach::Session.lead(&mut command);
    let mut child = command
        .spawn()
        .context("starting the private Codex app server")?;
    let handle =
        match ChildHandle::started(ProcessId(child.id()), Reach::Session, Some(containment)) {
            Ok(handle) => handle,
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err).context("taking on the private Codex app server");
            }
        };
    let crash_cleanup = crash::register(&handle, true);
    let lines = output_lines(&mut child);
    let server = ServerProcess {
        child,
        handle,
        crash_cleanup,
    };
    let endpoint = wait_for_server(&server, listen, lines)?;
    Ok((server, endpoint))
}

/// The lines the server prints on stdout and stderr, where they were piped. The pipes are read to
/// their end, so a server that goes on printing never blocks on them; a line that is not UTF-8 is
/// read as best it can be.
fn output_lines(child: &mut Child) -> Option<mpsc::Receiver<String>> {
    let (sender, lines) = mpsc::channel();
    let readers: Vec<Box<dyn Read + Send>> = [
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    if readers.is_empty() {
        return None;
    }
    for reader in readers {
        let sender = sender.clone();
        std::thread::spawn(move || read_lines(reader, &sender));
    }
    Some(lines)
}

/// Sends each line `reader` gives to `lines` until it ends. Once nobody listens, what follows is
/// only drained.
fn read_lines(reader: impl Read, lines: &mpsc::Sender<String>) {
    let mut reader = io::BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let _ = lines.send(String::from_utf8_lossy(&line).into_owned());
            }
        }
    }
}

fn wait_for_server(
    server: &ServerProcess,
    listen: &Listen,
    lines: Option<mpsc::Receiver<String>>,
) -> Result<Endpoint> {
    let deadline = Instant::now() + start_timeout();
    loop {
        if server.exited() {
            return Err(anyhow!(
                "the private Codex app server exited before becoming ready"
            ));
        }
        match listen.address() {
            // Codex 0.166 links the path to a socket elsewhere; the link resolves once it listens.
            Address::Socket(path) if path.exists() => {
                return Ok(Endpoint::Socket(path.to_path_buf()));
            }
            Address::Socket(_) => {}
            Address::Loopback { token } => {
                let lines = lines
                    .as_ref()
                    .ok_or_else(|| anyhow!("the server's output was not read"))?;
                while let Ok(line) = lines.try_recv() {
                    if let Some(port) = listening_port(&line) {
                        return Ok(Endpoint::Loopback {
                            port,
                            token: token.to_string(),
                        });
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "the private Codex app server did not start listening in time"
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Watches the server for exiting by itself, and then calls `exited`, unless `stopping` says by then
/// that it was ended on purpose.
pub(super) fn watch_for_exit(
    server: Arc<Mutex<Option<ServerProcess>>>,
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

#[cfg(test)]
pub(super) fn read_lines_for_test(reader: impl Read) -> Vec<String> {
    let (sender, lines) = mpsc::channel();
    read_lines(reader, &sender);
    drop(sender);
    lines.into_iter().collect()
}
