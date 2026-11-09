//! Where a launch's private server runs ([`crate::session::term::PrivateServer`]): the host that opens its
//! endpoint, starts it and ends it before its terminal binds it. The provider says what to run and
//! how it shows it is ready; the host knows nothing of what the server serves.
//!
//! The [`LocalHost`] runs the server in the daemon, as a child it owns, for terminals that run in
//! the daemon too. The [`HeldHost`] has the holder that is to run the terminal start the server as
//! the terminal's companion, under the launch's credentials, so that it outlives the daemon with
//! the terminal (see `apps/daemon/src/holder/README.md`'s "Companions" bullet).

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::holder::client::{HeldCompanion, Issued};
use crate::holder::protocol::{CompanionEndpoint, ContainmentSpec, EndpointKind, Ready};
use crate::platform::ipc::{Address, PrivateListen};
use crate::platform::proc::server::{Readiness, ServerChild};
use crate::platform::proc::Containment;
use crate::platform::term::Command;

/// How long a server the daemon runs is given to exit after the polite request, before it is
/// killed.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// What a server the daemon runs is called in the log.
const LOCAL_LABEL: &str = "private server";

/// Where a private server is to listen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerListen {
    /// A socket named `name` in a new owner-only directory whose name starts with `prefix`.
    Socket {
        prefix: &'static str,
        name: &'static str,
    },
    /// A loopback port the server chooses and reports, guarded by a new random token.
    Loopback,
}

impl ServerListen {
    /// What the host offers: a socket named `name` in a new private directory whose name starts
    /// with `prefix` on macOS and Linux; a loopback port with a new token on Windows. Windows has
    /// Unix sockets too, but a server such as Codex's takes one only in a directory whose security
    /// descriptor admits the user alone, and neither the standard library nor tokio connects to a
    /// Unix socket there.
    pub fn for_host(prefix: &'static str, name: &'static str) -> Self {
        if PrivateListen::offers_socket() {
            Self::Socket { prefix, name }
        } else {
            Self::Loopback
        }
    }
}

/// Where a private server listens, as its host opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEndpoint {
    /// The socket at this path, which the server makes once it listens.
    Socket(PathBuf),
    /// A loopback port guarded by `token`; `port` is absent until the server reports it.
    Loopback { token: String, port: Option<u16> },
}

impl ServerEndpoint {
    /// The endpoint a holder lists for a companion, or `None` for a kind this build does not know.
    pub fn of_companion(endpoint: &CompanionEndpoint) -> Option<Self> {
        match endpoint {
            CompanionEndpoint::Socket { path } => Some(Self::Socket(PathBuf::from(path))),
            CompanionEndpoint::Loopback { token, port } => Some(Self::Loopback {
                token: token.clone(),
                port: *port,
            }),
            CompanionEndpoint::Unknown => None,
        }
    }

    fn of_listen(listen: &PrivateListen) -> Self {
        match listen.address() {
            Address::Socket(path) => Self::Socket(path.to_path_buf()),
            Address::Loopback { token } => Self::Loopback {
                token: token.to_string(),
                port: None,
            },
        }
    }
}

/// A private server a holder lists with a terminal the daemon adopted, running or exited, for the
/// provider to link to again ([`crate::adapter::AgentProvider::rebuild_launch`]).
pub struct FoundServer {
    /// Where it listens, as the holder's welcome listed it.
    pub endpoint: ServerEndpoint,
    pub server: Box<dyn RunningServer>,
}

/// How a started private server shows it is ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerReady<'a> {
    /// The socket path of its endpoint appears (it may be a link to a socket elsewhere).
    SocketExists,
    /// A line of its output holds a non-zero port number right after this text.
    PortAfter(&'a str),
}

/// Where a launch's private server runs. A host serves one launch: it opens one endpoint at a time,
/// and a server started again is given a new one, so nothing the last left behind is in its way.
pub trait ServerHost {
    /// Opens the endpoint `listen` asks for, for the next [`Self::start`]. An endpoint opened before
    /// and not started on is ended first. **Blocks.**
    fn open_endpoint(&self, listen: ServerListen) -> Result<ServerEndpoint>;

    /// Starts `command` as the server on the endpoint opened last, and waits up to `within` for it
    /// to be ready as `ready` says; a server that exits first or is not ready in time is ended, with
    /// its endpoint. Returns the running server and the port it reported, for
    /// [`ServerReady::PortAfter`]. **Blocks.**
    fn start(
        &self,
        command: Command,
        ready: ServerReady<'_>,
        within: Duration,
    ) -> Result<(Box<dyn RunningServer>, Option<u16>)>;

    /// Ends what was opened or started under this host and not yet bound to a terminal: for a
    /// launch that falls back to an ordinary terminal. Failures are logged. **Blocks.**
    fn end(&self);

    /// Whether the server runs as the companion of the terminal the launch spawns next, which
    /// the spawn binds ([`crate::holder::protocol::SpawnSpec::companion`]).
    fn companion(&self) -> bool;
}

/// A private server a [`ServerHost`] started, or one a holder was found running.
pub trait RunningServer: Send + Sync {
    /// Whether the server is known to no longer run. Never waits.
    fn exited(&self) -> bool;

    /// Ends the server, where whoever holds this may still end it: one the daemon runs at any
    /// time, a holder's companion only before its terminal binds it. Failures are logged.
    /// **Blocks**, for up to the server's grace.
    fn end(&self);

    /// Whether the server ends once its terminal has exited, by the holder that runs both, so the
    /// daemon never ends it once the terminal runs: a session's stop leaves it alone.
    fn ends_with_its_terminal(&self) -> bool;
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

/// The host of a launch whose terminal runs in the daemon: the server is the daemon's child,
/// leading a session of its own in the launch's containment and in the daemon's crash table.
pub struct LocalHost<'a> {
    containment: &'a Containment,
    /// The endpoint opened last and not yet started on.
    listen: Mutex<Option<PrivateListen>>,
}

impl<'a> LocalHost<'a> {
    pub fn new(containment: &'a Containment) -> Self {
        Self {
            containment,
            listen: Mutex::new(None),
        }
    }
}

impl ServerHost for LocalHost<'_> {
    fn open_endpoint(&self, listen: ServerListen) -> Result<ServerEndpoint> {
        let mut slot = locked(&self.listen);
        slot.take();
        let opened = match listen {
            ServerListen::Socket { prefix, name } => PrivateListen::socket(prefix, name),
            ServerListen::Loopback => PrivateListen::loopback(),
        }
        .context("opening the private server's endpoint")?;
        let endpoint = ServerEndpoint::of_listen(&opened);
        *slot = Some(opened);
        Ok(endpoint)
    }

    fn start(
        &self,
        command: Command,
        ready: ServerReady<'_>,
        within: Duration,
    ) -> Result<(Box<dyn RunningServer>, Option<u16>)> {
        let listen = locked(&self.listen)
            .take()
            .ok_or_else(|| anyhow!("no endpoint was opened for the private server"))?;
        let socket = match listen.address() {
            Address::Socket(path) => Some(path.to_path_buf()),
            Address::Loopback { .. } => None,
        };
        let readiness = match (ready, socket.as_deref()) {
            (ServerReady::PortAfter(prefix), _) => Readiness::PortAfter(prefix),
            (ServerReady::SocketExists, Some(path)) => Readiness::PathExists(path),
            (ServerReady::SocketExists, None) => {
                bail!("the private server's endpoint has no socket to wait for")
            }
        };
        let mut plain = command.plain();
        self.containment.prepare(&mut plain);
        let mut child = ServerChild::start(
            plain,
            matches!(readiness, Readiness::PortAfter(_)),
            Some(self.containment),
            LOCAL_LABEL,
        )
        .context("starting the private server")?;
        let port = match child.wait_ready(readiness, within) {
            Ok(port) => port,
            Err(err) => {
                // The endpoint goes with the server, and what it left there with it.
                child.end(STOP_GRACE);
                return Err(anyhow!(err).context("the private server did not become ready"));
            }
        };
        Ok((Box::new(LocalServer::new(child, Some(listen))), port))
    }

    fn end(&self) {
        locked(&self.listen).take();
    }

    fn companion(&self) -> bool {
        false
    }
}

/// A private server the daemon runs, with the endpoint it listens on: ended, at the latest when
/// dropped, with a polite request and a grace, and its endpoint removed after it.
pub struct LocalServer {
    child: Mutex<ServerChild>,
    listen: Mutex<Option<PrivateListen>>,
}

impl LocalServer {
    /// The running `child`, listening on `listen` when the daemon opened its endpoint.
    pub fn new(child: ServerChild, listen: Option<PrivateListen>) -> Self {
        Self {
            child: Mutex::new(child),
            listen: Mutex::new(listen),
        }
    }

    /// The server's process.
    #[cfg(all(test, unix))]
    pub fn pid(&self) -> crate::platform::proc::ProcessId {
        locked(&self.child).pid()
    }
}

impl RunningServer for LocalServer {
    fn exited(&self) -> bool {
        locked(&self.child).exited()
    }

    fn end(&self) {
        locked(&self.child).end(STOP_GRACE);
        locked(&self.listen).take();
    }

    fn ends_with_its_terminal(&self) -> bool {
        false
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        RunningServer::end(self);
    }
}

/// The host of a launch whose terminal runs in a holder that runs companions: the server is
/// started by that holder, as the companion of the terminal the launch's credentials were issued
/// for, in the launch's containment. The daemon's crash table never holds it.
pub struct HeldHost<'a> {
    issued: &'a Issued,
    containment: ContainmentSpec,
}

impl<'a> HeldHost<'a> {
    /// The host of the launch `issued` is for, in the launch's `containment`. `issued` has to come
    /// from a holder that runs companions.
    pub fn new(issued: &'a Issued, containment: ContainmentSpec) -> Self {
        Self {
            issued,
            containment,
        }
    }
}

impl ServerHost for HeldHost<'_> {
    fn open_endpoint(&self, listen: ServerListen) -> Result<ServerEndpoint> {
        let kind = match listen {
            ServerListen::Socket { prefix, name } => EndpointKind::Socket {
                prefix: prefix.to_string(),
                name: name.to_string(),
            },
            ServerListen::Loopback => EndpointKind::Loopback,
        };
        let opened = self
            .issued
            .open_endpoint(kind)
            .context("having the holder open the private server's endpoint")?;
        ServerEndpoint::of_companion(&opened).ok_or_else(|| {
            anyhow!("the holder opened an endpoint of a kind this build does not know")
        })
    }

    fn start(
        &self,
        command: Command,
        ready: ServerReady<'_>,
        within: Duration,
    ) -> Result<(Box<dyn RunningServer>, Option<u16>)> {
        let ready = match ready {
            ServerReady::SocketExists => Ready::SocketExists,
            ServerReady::PortAfter(prefix) => Ready::PortAfter {
                prefix: prefix.to_string(),
            },
        };
        let (companion, port) = self
            .issued
            .start_companion(command, ready, within, Some(self.containment.clone()))
            .context("having the holder start the private server")?;
        Ok((Box::new(HeldServer::new(companion)), port))
    }

    fn end(&self) {
        if let Err(err) = self.issued.end_companion() {
            tracing::warn!(
                error = %format!("{err:#}"),
                "the holder did not end the private server of a launch that falls back; it ends \
                 it with the launch's spawn or connection"
            );
        }
    }

    fn companion(&self) -> bool {
        true
    }
}

/// A private server a holder runs as a terminal's companion: started through a [`HeldHost`], or
/// found running with a terminal the daemon adopted
/// ([`crate::holder::client::FoundCompanion::server`]). Whether it runs is the link's to say (see
/// [`HeldCompanion`]); the holder ends it once its terminal has exited.
pub struct HeldServer(HeldCompanion);

impl HeldServer {
    pub fn new(companion: HeldCompanion) -> Self {
        Self(companion)
    }
}

impl RunningServer for HeldServer {
    fn exited(&self) -> bool {
        self.0.exited()
    }

    fn end(&self) {
        if let Err(err) = self.0.end() {
            tracing::warn!(
                error = %format!("{err:#}"),
                "the holder did not end the private server; it ends it with its terminal or the \
                 launch's connection"
            );
        }
    }

    fn ends_with_its_terminal(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_listens_on_a_loopback_port() {
        assert_eq!(
            ServerListen::for_host("octo-test", "rpc"),
            ServerListen::Loopback
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_listens_on_a_socket_of_the_name_asked_for() {
        assert_eq!(
            ServerListen::for_host("octo-test", "rpc"),
            ServerListen::Socket {
                prefix: "octo-test",
                name: "rpc"
            }
        );
    }

    #[test]
    fn the_endpoint_a_holder_lists_is_read_as_the_servers_and_one_it_does_not_know_is_not() {
        assert_eq!(
            ServerEndpoint::of_companion(&CompanionEndpoint::Socket {
                path: "/tmp/octo-cdx-1/rpc".to_string()
            }),
            Some(ServerEndpoint::Socket(PathBuf::from("/tmp/octo-cdx-1/rpc")))
        );
        assert_eq!(
            ServerEndpoint::of_companion(&CompanionEndpoint::Loopback {
                token: "tok".to_string(),
                port: Some(4321)
            }),
            Some(ServerEndpoint::Loopback {
                token: "tok".to_string(),
                port: Some(4321)
            })
        );
        // A port still to be reported stays absent.
        assert_eq!(
            ServerEndpoint::of_companion(&CompanionEndpoint::Loopback {
                token: "tok".to_string(),
                port: None
            }),
            Some(ServerEndpoint::Loopback {
                token: "tok".to_string(),
                port: None
            })
        );
        assert_eq!(
            ServerEndpoint::of_companion(&CompanionEndpoint::Unknown),
            None
        );
    }

    /// The hosts run real servers: shells that make the socket they are waited for on and stay up.
    #[cfg(unix)]
    mod hosts {
        use std::path::Path;

        use super::*;
        use crate::holder::protocol::SpawnSpec;
        use crate::platform::term::Size;
        use crate::test_support::{
            is_running, programs, wait_for_pid, KillOnDrop, ScratchDir, PATIENCE,
        };

        fn sh(script: &str) -> Command {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            command
        }

        /// A server that records its pid in `pid_file`, makes the file `socket`, and stays up.
        fn recording(socket: &Path, pid_file: &Path) -> Command {
            sh(&format!(
                "echo $$ > '{pid}.tmp'; mv '{pid}.tmp' '{pid}'; touch '{socket}'; exec sleep 60",
                pid = pid_file.display(),
                socket = socket.display()
            ))
        }

        fn socket_listen() -> ServerListen {
            ServerListen::for_host("octo-test", "rpc")
        }

        fn directory_of(endpoint: &ServerEndpoint) -> PathBuf {
            match endpoint {
                ServerEndpoint::Socket(path) => path
                    .parent()
                    .expect("a socket in a directory")
                    .to_path_buf(),
                other => panic!("not a socket: {other:?}"),
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

        #[test]
        fn the_local_host_starts_the_server_on_a_private_socket_and_ends_it_with_its_endpoint() {
            let dir = ScratchDir::new("server-host-local");
            let containment = Containment::create("test", "server-host-local").unwrap();
            let host = LocalHost::new(&containment);
            assert!(!host.companion());
            let endpoint = host.open_endpoint(socket_listen()).unwrap();
            let ServerEndpoint::Socket(socket) = &endpoint else {
                panic!("{endpoint:?}");
            };
            let directory = directory_of(&endpoint);
            assert!(directory.is_dir());

            let pid_file = dir.join("pid");
            let (server, port) = host
                .start(
                    recording(socket, &pid_file),
                    ServerReady::SocketExists,
                    PATIENCE,
                )
                .unwrap();
            let pid = wait_for_pid(&pid_file);
            let _guard = KillOnDrop(vec![pid]);
            assert_eq!(port, None);
            assert!(!server.exited());
            assert!(
                !server.ends_with_its_terminal(),
                "the daemon ends a server it runs"
            );

            server.end();
            assert!(server.exited());
            assert!(!is_running(pid));
            assert!(!directory.exists());
        }

        #[test]
        fn the_local_host_reports_the_port_the_server_prints_on_a_loopback_endpoint() {
            const PREFIX: &str = "listening on: ws://127.0.0.1:";
            let containment = Containment::create("test", "server-host-loopback").unwrap();
            let host = LocalHost::new(&containment);
            let endpoint = host.open_endpoint(ServerListen::Loopback).unwrap();
            let ServerEndpoint::Loopback { token, port } = &endpoint else {
                panic!("{endpoint:?}");
            };
            assert_eq!((token.len(), *port), (64, None));
            let (server, port) = host
                .start(
                    sh(&format!("echo '{PREFIX}4321'; exec sleep 60")),
                    ServerReady::PortAfter(PREFIX),
                    PATIENCE,
                )
                .unwrap();
            assert_eq!(port, Some(4321));
            assert!(!server.exited());
            server.end();
            assert!(server.exited());
        }

        #[test]
        fn a_local_start_that_fails_or_has_no_endpoint_leaves_nothing() {
            let containment = Containment::create("test", "server-host-failed").unwrap();
            let host = LocalHost::new(&containment);
            let error = host
                .start(sh("exec sleep 60"), ServerReady::SocketExists, PATIENCE)
                .err()
                .expect("a start with no endpoint opened fails");
            assert!(
                format!("{error:#}").contains("no endpoint was opened"),
                "{error:#}"
            );

            let endpoint = host.open_endpoint(socket_listen()).unwrap();
            let error = host
                .start(sh("exit 3"), ServerReady::SocketExists, PATIENCE)
                .err()
                .expect("a server that exits is not ready");
            assert!(
                format!("{error:#}").contains("did not become ready"),
                "{error:#}"
            );
            assert!(!directory_of(&endpoint).exists());
        }

        #[test]
        fn opening_an_endpoint_again_or_ending_the_host_removes_the_one_not_started_on() {
            let containment = Containment::create("test", "server-host-reopen").unwrap();
            let host = LocalHost::new(&containment);
            let first = host.open_endpoint(socket_listen()).unwrap();
            let second = host.open_endpoint(socket_listen()).unwrap();
            assert_ne!(directory_of(&first), directory_of(&second));
            assert!(!directory_of(&first).exists());
            assert!(directory_of(&second).is_dir());
            host.end();
            assert!(!directory_of(&second).exists());
        }

        #[test]
        fn the_held_host_has_the_holder_start_the_server_as_the_terminals_companion() {
            let dir = ScratchDir::new("server-host-held");
            let holders = crate::holder::client::in_process(dir.join("holders")).unwrap();
            let issued = holders.issue_launch("s1", None).unwrap();
            let host = HeldHost::new(
                &issued,
                ContainmentSpec {
                    instance: "test".to_string(),
                    launch: uuid::Uuid::new_v4().to_string(),
                },
            );
            assert!(host.companion());
            let endpoint = host.open_endpoint(socket_listen()).unwrap();
            let ServerEndpoint::Socket(socket) = &endpoint else {
                panic!("{endpoint:?}");
            };
            let directory = directory_of(&endpoint);
            let pid_file = dir.join("pid");
            let (server, port) = host
                .start(
                    recording(socket, &pid_file),
                    ServerReady::SocketExists,
                    PATIENCE,
                )
                .unwrap();
            let pid = wait_for_pid(&pid_file);
            let _guard = KillOnDrop(vec![pid]);
            assert_eq!(port, None);
            assert!(!server.exited());
            assert!(
                server.ends_with_its_terminal(),
                "the holder ends a server it runs, with its terminal"
            );

            // A launch that falls back ends what it started, before any terminal binds it.
            host.end();
            assert!(server.exited());
            crate::test_support::assert_stops_running(pid);
            let deadline = std::time::Instant::now() + PATIENCE;
            while directory.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the endpoint was not removed"
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            drop(host);
            drop((issued, holders));
        }

        #[test]
        fn a_held_start_that_fails_leaves_no_companion_so_only_an_unbound_terminal_starts() {
            let dir = ScratchDir::new("server-host-held-failed");
            let holders = crate::holder::client::in_process(dir.join("holders")).unwrap();
            let expecting = holders.issue_launch("s1", None).unwrap();
            let host = HeldHost::new(
                &expecting,
                ContainmentSpec {
                    instance: "test".to_string(),
                    launch: uuid::Uuid::new_v4().to_string(),
                },
            );
            host.open_endpoint(socket_listen()).unwrap();
            let error = host
                .start(sh("exit 3"), ServerReady::SocketExists, PATIENCE)
                .err()
                .expect("a server that exits is not ready");
            assert!(
                format!("{error:#}").contains("did not become ready"),
                "{error:#}"
            );
            // Nothing is left to end, and the launch falls back.
            host.end();
            drop(host);
            let refused = expecting
                .spawn(spec(true))
                .err()
                .expect("a terminal expecting the companion that failed is refused");
            assert!(format!("{refused:#}").contains("companion"), "{refused:#}");

            let unbound = holders.issue_launch("s2", None).unwrap();
            let held = unbound
                .spawn(spec(false))
                .expect("a terminal that expects no companion starts");
            let _guard = KillOnDrop(vec![held.pid().0 as i32]);
            drop((held, holders));
        }
    }
}
