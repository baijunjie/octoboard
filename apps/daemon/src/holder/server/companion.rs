//! A terminal's companion: a process the holder starts for a terminal before the terminal, under
//! the terminal's id, on an endpoint the holder opened for it. The terminal binds it at its spawn
//! and ends it once it has exited; the companion's own death ends the terminal. Until a terminal
//! binds it, the companion is the request's: the holder keeps it in its map of unbound companions
//! and ends it with the connection it was asked over, on a revoke or an explicit end, and after a
//! bind deadline. The holder knows nothing of what the companion serves: the daemon sends the
//! command, the endpoint's kind and how readiness shows, and the holder reports the endpoint and
//! the port.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::time::Duration;

use super::terminal::Terminal;
use crate::holder::protocol::{
    CompanionEndpoint, CompanionInfo, ContainmentSpec, EndpointKind, Ready,
};
use crate::platform::ipc::{Address, PrivateListen};
use crate::platform::proc::server::{Readiness, ServerChild};
use crate::platform::proc::{Containment, ProcessId, ProcessIdentity};
use crate::platform::proc_table;
use crate::platform::term::Command;

/// How long a companion is given to exit after the polite request, before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// How often a running companion is looked at for having exited by itself.
const EXIT_POLL: Duration = Duration::from_millis(100);

/// What the holder does once a companion has exited by itself, given the terminal that bound it,
/// if one did and it is still kept.
pub(super) type OnExit = Box<dyn FnOnce(Option<Arc<Terminal>>) + Send>;

/// Tells one companion from every other the holder ever opened, so that a deadline set for one is
/// not applied to a later one under the same terminal id.
static SERIALS: AtomicU64 = AtomicU64::new(1);

pub(super) struct Companion {
    pub(super) id: String,
    pub(super) serial: u64,
    /// Where the companion listens, completed with the port once it reports it.
    endpoint: Mutex<CompanionEndpoint>,
    /// Keeps the endpoint's directory (or token) until the companion ends; dropped, it removes
    /// the directory and whatever the companion left in it.
    listen: Mutex<Option<PrivateListen>>,
    /// The running process, until it is ended.
    process: Mutex<Option<ServerChild>>,
    /// The process's pid and identity, once started.
    started: OnceLock<(ProcessId, Option<ProcessIdentity>)>,
    state: Mutex<State>,
    /// Held for the whole of [`Self::end`], so that a later call waits for the end under way.
    ending: Mutex<()>,
}

#[derive(Default)]
struct State {
    /// The terminal that bound the companion.
    terminal: Option<Weak<Terminal>>,
    /// The companion exited by itself.
    exited: bool,
    /// The companion is being ended, or was, on purpose: its exit is no news.
    ending: bool,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

/// Whether `name` is a plain name, with no directory in it.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && Path::new(name).file_name().is_some_and(|file| file == name)
}

impl Companion {
    /// Opens the endpoint `kind` asks for, for the companion of the terminal `id`: the companion
    /// is not started yet.
    pub(super) fn open(id: &str, kind: &EndpointKind) -> Result<Arc<Self>, String> {
        let listen = match kind {
            EndpointKind::Socket { prefix, name } => {
                if !plain_name(prefix) || !plain_name(name) {
                    return Err("the socket's name and prefix have to be plain names".to_string());
                }
                PrivateListen::socket(prefix, name)
            }
            EndpointKind::Loopback => PrivateListen::loopback(),
            EndpointKind::Unknown => {
                return Err("an endpoint of a kind this holder does not know".to_string());
            }
        }
        .map_err(|err| format!("opening the companion's endpoint: {err}"))?;
        let endpoint = match listen.address() {
            Address::Socket(path) => CompanionEndpoint::Socket {
                path: path.to_string_lossy().into_owned(),
            },
            Address::Loopback { token } => CompanionEndpoint::Loopback {
                token: token.to_string(),
                port: None,
            },
        };
        Ok(Arc::new(Self {
            id: id.to_string(),
            serial: SERIALS.fetch_add(1, Ordering::Relaxed),
            endpoint: Mutex::new(endpoint),
            listen: Mutex::new(Some(listen)),
            process: Mutex::new(None),
            started: OnceLock::new(),
            state: Mutex::new(State::default()),
            ending: Mutex::new(()),
        }))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        locked(&self.state)
    }

    pub(super) fn endpoint(&self) -> CompanionEndpoint {
        locked(&self.endpoint).clone()
    }

    /// Whether the companion was started and runs: neither exited by itself nor ended.
    pub(super) fn running(&self) -> bool {
        let state = self.state();
        self.started.get().is_some() && !state.exited && !state.ending
    }

    /// Whether the companion was started, whatever came of it after.
    pub(super) fn started(&self) -> bool {
        self.started.get().is_some()
    }

    /// Starts `command` as the companion and waits up to `within` for it to be ready as `ready`
    /// says; a companion that exits first or is not ready in time is ended. On success it is
    /// watched for exiting by itself, which `on_exit` is told. Returns the pid, the identity and
    /// the port reported. A companion ended while it started is ended as it comes up, and its start
    /// fails. **Blocks.**
    pub(super) fn start(
        self: &Arc<Self>,
        command: &Command,
        ready: &Ready,
        within: Duration,
        containment: Option<&ContainmentSpec>,
        on_exit: OnExit,
    ) -> Result<(u32, Option<String>, Option<u16>), String> {
        let socket = match self.endpoint() {
            CompanionEndpoint::Socket { path } => Some(PathBuf::from(path)),
            _ => None,
        };
        // How readiness shows, and so whether the output is read for it, settled before anything
        // is started.
        let (readiness, read_output) = match (ready, &socket) {
            (Ready::SocketExists, Some(path)) => (Readiness::PathExists(path), false),
            (Ready::SocketExists, None) => {
                return Err("the companion's endpoint has no socket to wait for".to_string());
            }
            (Ready::PortAfter { prefix }, _) => (Readiness::PortAfter(prefix), true),
            (Ready::Unknown, _) => {
                return Err("a readiness this holder does not know".to_string());
            }
        };
        // The daemon made the launch's containment, which the companion joins here as the
        // launch's terminal does.
        let containment = containment
            .map(|launch| Containment::join_launch(&launch.instance, &launch.launch))
            .transpose()
            .map_err(|err| format!("opening the launch's containment: {err}"))?;
        let mut plain = command.clone();
        if let Some(containment) = &containment {
            containment.prepare(&mut plain);
        }
        let label = format!("companion of terminal {}", self.id);
        let mut process = ServerChild::start(plain, read_output, containment.as_ref(), &label)
            .map_err(|err| format!("starting the companion: {err}"))?;
        let port = match process.wait_ready(readiness, within) {
            Ok(port) => port,
            Err(err) => {
                process.end(STOP_GRACE);
                return Err(format!("the companion did not become ready: {err}"));
            }
        };
        let pid = process.pid();
        let identity = proc_table::start_of(pid.0 as i32)
            .map(|start_micros| ProcessIdentity { pid, start_micros });
        {
            let state = self.state();
            if state.ending {
                drop(state);
                process.end(STOP_GRACE);
                return Err("the companion was ended while it started".to_string());
            }
            *locked(&self.process) = Some(process);
            if let (Some(reported), CompanionEndpoint::Loopback { port: told, .. }) =
                (port, &mut *locked(&self.endpoint))
            {
                *told = Some(reported);
            }
            self.started.set((pid, identity)).ok();
        }
        self.watch(on_exit);
        Ok((pid.0, identity.map(|identity| identity.key()), port))
    }

    /// Watches the running companion for exiting by itself, and then tells `on_exit`, with the
    /// terminal that bound it, unless it is being ended on purpose by then.
    fn watch(self: &Arc<Self>, on_exit: OnExit) {
        let companion = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(EXIT_POLL);
            let exited = match locked(&companion.process).as_ref() {
                Some(process) => process.exited(),
                // Ended.
                None => return,
            };
            if !exited {
                continue;
            }
            let terminal = {
                let mut state = companion.state();
                if state.ending {
                    return;
                }
                state.exited = true;
                state.terminal.as_ref().and_then(Weak::upgrade)
            };
            on_exit(terminal);
            return;
        });
    }

    /// Binds the companion to `terminal`, which ends it once it has exited. `false` when the
    /// companion is no longer running (it exited by itself, or is being ended), and nothing is
    /// bound: a terminal must not run attached to a dead server.
    pub(super) fn bind(&self, terminal: &Arc<Terminal>) -> bool {
        let mut state = self.state();
        if state.exited || state.ending {
            return false;
        }
        state.terminal = Some(Arc::downgrade(terminal));
        true
    }

    /// Ends the companion: a polite request to its process group, a short grace, then a kill and a
    /// sweep of what it left, and its endpoint removed. Its exit is no news from now on. A call
    /// made while another is ending it waits for that end to finish, so that whoever returns can
    /// count on the companion being gone; once ended, a later call does nothing. **Blocks** for
    /// up to the grace.
    pub(super) fn end(&self) {
        let _one_at_a_time = locked(&self.ending);
        self.state().ending = true;
        // Taken out first, so that the lock is not held through the grace.
        let process = locked(&self.process).take();
        if let Some(mut process) = process {
            process.end(STOP_GRACE);
        }
        locked(&self.listen).take();
    }

    /// What a welcome lists of a started companion with its terminal; `None` for one never
    /// started.
    pub(super) fn info(&self) -> Option<CompanionInfo> {
        let &(pid, identity) = self.started.get()?;
        let exited = {
            let state = self.state();
            state.exited || state.ending
        };
        Some(CompanionInfo {
            endpoint: self.endpoint(),
            pid: pid.0,
            identity: identity.map(|identity| identity.key()),
            exited,
        })
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn a_socket_endpoint_is_refused_where_the_host_has_no_unix_sockets() {
        let kind = EndpointKind::Socket {
            prefix: "octo-test".to_string(),
            name: "rpc".to_string(),
        };
        let refused = Companion::open("t1", &kind);
        assert!(refused.is_err());
    }
}
