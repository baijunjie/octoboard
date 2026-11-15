//! Remote access to the desktop application's daemon (Allow remote access): while the setting is
//! on, a daemon that follows the application opens the browser's way in, serves the UI there and
//! holds and accepts the owner key ([`crate::owner_key`]), as the server form's daemon always does;
//! while it is off, it does none of that. The way in is the owner socket on Linux and macOS
//! ([`browser::socket`]) and, on Windows, whose SSH server cannot forward to a socket, a loopback
//! port of its own ([`browser::port`]) whose number is a setting beside the switch. On Windows the
//! owner endpoint another device's application reaches through `octoboardd proxy` is a named pipe
//! only the daemon's user can open ([`crate::platform::owner_pipe`]), opened with the browser's
//! port, serving the same and closed when remote access is turned off; a change of port leaves it
//! open, since the application never uses the port. Both take effect at once, at the daemon's start
//! and while it runs: turning the switch off closes the way in, refuses the owner key and closes
//! every connection the key opened, as a rotation does; changing the port does the same and opens
//! the new one.
//!
//! The settings are the user's alone: no agent's settings tool reads or changes them, no
//! configuration export carries them, and a connection the owner key opened (a browser's) cannot
//! change them or rotate the key. A daemon that runs on its own (the server form) serves its
//! browser as it always has and offers no setting.
//!
//! The owner record says whether the daemon takes the owner key, and at which port on Windows, so
//! `octoboardd address` answers for the application's daemon: the record is written again as the
//! settings are applied.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Result;

use crate::browser::{self, ServeTask};
use crate::owner_key::RotateError;
use crate::owner_record::Owner;
use crate::protocol::{
    error_code, CodedError, ConnectionDetails, Event, RemoteAccess as RemoteAccessInfo,
    RemoteAccessFailure, RemoteAccessFailureReason, RemoteAccessState,
};
use crate::state::AppState;

/// The port a browser's machine forwards the owner socket to, in the address and the forward
/// command: the server form's default port, the same at every launch, so that an address the
/// owner bookmarked keeps opening. The desktop daemon's own TCP port is one the system assigns. On
/// Windows it is also the browser port's default, until the user sets another.
pub const FORWARD_PORT: u16 = match u16::from_str_radix(env!("OCTOBOARD_DEFAULT_SERVER_PORT"), 10) {
    Ok(port) => port,
    Err(_) => panic!("OCTOBOARD_DEFAULT_SERVER_PORT is not a port number"),
};

/// Which way in a browser has to the daemon while remote access is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WayIn {
    /// The owner socket in the data directory (Linux, macOS).
    Socket,
    /// A loopback port of its own, at the port the user set (Windows).
    Port,
}

impl WayIn {
    /// This host's, or `None` where it has neither.
    fn of_host() -> Option<Self> {
        if browser::port::USED {
            Some(Self::Port)
        } else if browser::socket::AVAILABLE {
            Some(Self::Socket)
        } else {
            None
        }
    }
}

/// What a daemon that follows the application holds of remote access. Empty, and offering
/// nothing, in a daemon that does not ([`RemoteAccess::begin`] is never called there).
#[derive(Default)]
pub struct RemoteAccess {
    /// The data directory, set by [`RemoteAccess::begin`]: the daemon offers remote access only
    /// once it is.
    data: OnceLock<PathBuf>,
    /// Whether the daemon's build embeds a UI to serve, set with `data`.
    embeds_ui: OnceLock<bool>,
    /// The browser's way in on this host, set with `data`.
    way_in: OnceLock<WayIn>,
    inner: Mutex<Inner>,
    /// Held across each change of the settings and its application, from the store's write to the
    /// way in opened or closed: requests run on tasks of their own, and a change applied after a
    /// later one would leave the way in open with the switch stored off.
    changing: Mutex<()>,
}

/// The way in, open: the owner socket's listener, or the browser port's with the owner pipe's
/// (none where the host has no owner pipes, as in a test of the port on another system).
enum Opened {
    Socket(crate::platform::local_socket::Listener),
    Port(
        tokio::net::TcpListener,
        Option<crate::platform::owner_pipe::Listener>,
    ),
}

#[derive(Default)]
struct Inner {
    /// The router the way in serves, given once the daemon serves its clients; until then an open
    /// way in waits in `pending`.
    router: Option<axum::Router>,
    pending: Option<Opened>,
    serving: Option<Vec<ServeTask>>,
    /// The owner pipe on Windows, kept apart from the browser port: a change of port leaves it
    /// open, since another device's application never goes through the port. A pipe closed and
    /// made again at once could be reached in between through the closing listener's last
    /// instance, which ends the connection unanswered. The pipe a quick off and on of remote access
    /// replaces is only aborted, not awaited, so for a moment the new one may not be accepting yet
    /// and a connection made then fails: another device's application retries it.
    pipe: Option<Pipe>,
    failure: Option<RemoteAccessFailure>,
    /// The daemon's owner record, written again whenever whether it takes the owner key, or at
    /// which port, changes.
    record: Option<Owner>,
    /// Set as the daemon stops: nothing is opened or recorded any more.
    closed: bool,
}

/// The owner pipe's listener, waiting for the router, or served.
enum Pipe {
    Pending(crate::platform::owner_pipe::Listener),
    /// Held for as long as the pipe is served: dropping it ends the accepting.
    Serving {
        _task: ServeTask,
    },
}

impl Inner {
    fn open(&self) -> bool {
        self.serving.is_some() || self.pending.is_some()
    }
}

impl RemoteAccess {
    /// Offers remote access in a daemon that follows the application, whose data directory is
    /// `data` and whose build embeds a UI where `embeds_ui`, on a host with a way in for the
    /// browser, and applies the stored setting: the way in is opened and the owner key held now,
    /// before the owner record is written, and served once [`RemoteAccess::serve_clients`] is
    /// called.
    pub fn begin(&self, state: &Arc<AppState>, data: PathBuf, embeds_ui: bool) {
        if let Some(way_in) = WayIn::of_host() {
            self.begin_with(state, data, embeds_ui, way_in);
        }
    }

    /// [`RemoteAccess::begin`], with the way in named rather than the host's.
    pub(crate) fn begin_with(
        &self,
        state: &Arc<AppState>,
        data: PathBuf,
        embeds_ui: bool,
        way_in: WayIn,
    ) {
        let _changing = self.changing.lock().unwrap();
        if self.embeds_ui.set(embeds_ui).is_err() {
            return;
        }
        let _ = self.way_in.set(way_in);
        let _ = self.data.set(data);
        let allowed = state.store.allow_remote_access().unwrap_or_else(|err| {
            tracing::warn!(%err, "reading Allow remote access failed; remote access stays off");
            false
        });
        self.apply(state, allowed);
    }

    /// Whether this daemon offers remote access.
    pub fn offered(&self) -> bool {
        self.data.get().is_some()
    }

    /// The port the browser is served at on Windows, as the user set it: `None` where the browser
    /// goes through the owner socket, or remote access is not offered.
    fn browser_port(&self, state: &AppState) -> Option<u16> {
        (self.way_in.get() == Some(&WayIn::Port)).then(|| {
            state
                .store
                .remote_access_port()
                .unwrap_or_else(|err| {
                    tracing::warn!(%err, "reading the browser port failed; the default is used");
                    None
                })
                .unwrap_or(FORWARD_PORT)
        })
    }

    /// The browser port for the owner record: set while the owner key is taken at it.
    pub fn recorded_port(&self, state: &AppState) -> Option<u16> {
        self.browser_port(state)
            .filter(|_| state.owner_key.is_held())
    }

    /// Starts serving the way in with `router`, now and whenever remote access is turned on from
    /// here on: called once the daemon serves its other clients.
    pub fn serve_clients(&self, router: axum::Router) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(opened) = inner.pending.take() {
            inner.serving = Some(serving(opened, router.clone()));
        }
        if let Some(Pipe::Pending(pipe)) = inner.pipe.take() {
            inner.pipe = Some(Pipe::Serving {
                _task: browser::serve_on(pipe, router.clone()),
            });
        }
        inner.router = Some(router);
    }

    /// Keeps `owner`, the daemon's owner record as written, to write again as remote access is
    /// turned on and off.
    pub fn keep_record(&self, owner: Owner) {
        self.inner.lock().unwrap().record = Some(owner);
    }

    /// Stops serving as the daemon stops, and hands back its owner record as it stands, for the
    /// daemon to remove. The socket file is left for the next daemon, which replaces it.
    pub fn close(&self) -> Option<Owner> {
        let mut inner = self.inner.lock().unwrap();
        inner.closed = true;
        inner.serving = None;
        inner.pending = None;
        inner.pipe = None;
        // The router holds the state, which holds this: let it go.
        inner.router = None;
        inner.record.clone()
    }

    /// Where remote access stands, for the snapshot; `None` where it is not offered.
    pub fn info(&self, state: &AppState) -> Option<RemoteAccessInfo> {
        let data = self.data.get()?;
        let inner = self.inner.lock().unwrap();
        Some(self.info_of(state, data, &inner))
    }

    fn info_of(&self, state: &AppState, data: &Path, inner: &Inner) -> RemoteAccessInfo {
        let allowed = state.store.allow_remote_access().unwrap_or(false);
        let octoboardd = Path::new(&state.install_exe);
        let browser_port = self.browser_port(state);
        let key = state.owner_key.current();
        let serving = key.is_some() && inner.open();
        let (state, failure) = match (&inner.failure, serving) {
            (_, true) => (RemoteAccessState::Serving, None),
            (Some(failure), false) if allowed => (RemoteAccessState::Failed, Some(failure.clone())),
            _ => (RemoteAccessState::Off, None),
        };
        let serving = state == RemoteAccessState::Serving;
        let port = browser_port.unwrap_or(FORWARD_PORT);
        let connection = serving.then(|| connection_details(octoboardd)).flatten();
        RemoteAccessInfo {
            allowed,
            state,
            browser_port,
            address: key
                .filter(|_| serving)
                .map(|key| browser::browser_address(port, &key)),
            forward_command: serving.then(|| match browser_port {
                Some(port) => browser::forward::to_port_for_account(port),
                None => browser::forward::to_socket_in(port, data),
            }),
            failure,
            connection,
        }
    }

    /// Turns the setting on or off, as the user asked over a connection the owner key did not
    /// open, applies it and broadcasts where remote access stands.
    pub fn set(&self, state: &Arc<AppState>, allowed: bool) -> Result<()> {
        if !self.offered() {
            return Err(unavailable("this daemon offers no remote access"));
        }
        let _changing = self.changing.lock().unwrap();
        state.store.set_allow_remote_access(allowed)?;
        self.apply(state, allowed);
        Ok(())
    }

    /// Sets the port the browser is served at on Windows, as the user asked over a connection the
    /// owner key did not open, and applies it at once: while remote access is on, the way in at
    /// the old port is closed with every connection the owner key opened there, and the new port
    /// opened (or why not said). The same port again while it is served changes nothing; while it
    /// could not be opened, it is tried again.
    pub fn set_port(&self, state: &Arc<AppState>, port: u16) -> Result<()> {
        let _changing = self.changing.lock().unwrap();
        let Some(current) = self.browser_port(state) else {
            return Err(unavailable(
                "this daemon serves the browser at no port of its own",
            ));
        };
        state.store.set_remote_access_port(Some(port))?;
        let allowed = state.store.allow_remote_access().unwrap_or(false);
        let served = self.inner.lock().unwrap().open();
        if allowed && !(served && current == port) {
            self.close_way_in(state);
            self.apply(state, true);
        } else {
            self.broadcast(state);
        }
        Ok(())
    }

    /// Replaces the owner key, which cuts off every page opened with the old one, and broadcasts
    /// the new address.
    pub fn rotate(&self, state: &Arc<AppState>) -> Result<()> {
        if !self.offered() {
            return Err(unavailable("remote access is not on"));
        }
        self.rotate_key(state).map_err(|err| match err {
            RotateError::NotHeld => unavailable("remote access is not on"),
            RotateError::Write(_) => {
                anyhow::anyhow!("rotating the owner key failed: {}", err.message())
            }
        })
    }

    /// Replaces the owner key and broadcasts the new address: every rotation of the daemon's, the
    /// one `octoboardd address --rotate` asks for too. Where remote access is offered, it runs
    /// under the lock a change of the settings holds, so a rotation that races remote access being
    /// turned off either finds the key no longer held or is broadcast before the switch-off, never
    /// after.
    pub fn rotate_key(&self, state: &AppState) -> std::result::Result<(), RotateError> {
        let _changing = self.offered().then(|| self.changing.lock().unwrap());
        state.owner_key.rotate()?;
        tracing::info!("the owner key was rotated");
        // A desktop daemon's Settings show the address, which the rotation replaced.
        self.broadcast(state);
        Ok(())
    }

    /// Opens or closes the way in and the owner key to match `allowed`, records whether the daemon
    /// takes the key, and broadcasts where remote access stands. The caller holds `changing`.
    fn apply(&self, state: &Arc<AppState>, allowed: bool) {
        let (Some(data), Some(&way_in)) = (self.data.get(), self.way_in.get()) else {
            return;
        };
        let port = self.browser_port(state);
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.closed {
                return;
            }
            if allowed {
                if !inner.open() {
                    let embeds_ui = self.embeds_ui.get() == Some(&true);
                    let need_pipe = inner.pipe.is_none();
                    match open(
                        state,
                        data,
                        embeds_ui,
                        way_in,
                        port.unwrap_or(FORWARD_PORT),
                        need_pipe,
                    ) {
                        Ok(mut opened) => {
                            inner.failure = None;
                            if let Opened::Port(_, pipe) = &mut opened {
                                if let Some(pipe) = pipe.take() {
                                    inner.pipe = Some(match inner.router.clone() {
                                        Some(router) => Pipe::Serving {
                                            _task: browser::serve_on(pipe, router),
                                        },
                                        None => Pipe::Pending(pipe),
                                    });
                                }
                            }
                            match inner.router.clone() {
                                Some(router) => inner.serving = Some(serving(opened, router)),
                                None => inner.pending = Some(opened),
                            }
                            log_serving(way_in, data, port);
                        }
                        Err(failure) => {
                            tracing::warn!(
                                reason = ?failure.reason,
                                "remote access is on, but no browser is served: {}",
                                failure.detail
                            );
                            inner.failure = Some(failure);
                        }
                    }
                }
            } else {
                inner.failure = None;
                close_open(state, data, way_in, &mut inner, false);
            }
            let takes = state.owner_key.is_held();
            let recorded_port = port.filter(|_| takes);
            if let Some(record) = inner.record.as_mut() {
                if record.takes_owner_key != takes || record.browser_port != recorded_port {
                    record.takes_owner_key = takes;
                    record.browser_port = recorded_port;
                    if let Err(err) = record.record() {
                        tracing::warn!(%err, "writing the owner record failed");
                    }
                }
            }
        }
        self.broadcast(state);
    }

    /// Closes the way in and the owner key, without changing the setting, ahead of opening them
    /// again at another port; the owner pipe stays open.
    fn close_way_in(&self, state: &AppState) {
        let (Some(data), Some(&way_in)) = (self.data.get(), self.way_in.get()) else {
            return;
        };
        let mut inner = self.inner.lock().unwrap();
        inner.failure = None;
        close_open(state, data, way_in, &mut inner, true);
    }

    /// Broadcasts where remote access stands; nothing where it is not offered.
    pub(crate) fn broadcast(&self, state: &AppState) {
        if let Some(remote_access) = self.info(state) {
            state.broadcast(Event::RemoteAccessUpdated { remote_access });
        }
    }
}

/// Closes what is open of the way in, the owner pipe too unless `keep_pipe`, refuses the owner key
/// and has every connection it opened closed.
fn close_open(state: &AppState, data: &Path, way_in: WayIn, inner: &mut Inner, keep_pipe: bool) {
    let was_open = inner.serving.take().is_some() | inner.pending.take().is_some();
    if !keep_pipe {
        inner.pipe = None;
    }
    // Every connection the key opened is told to close.
    state.owner_key.disable();
    if !was_open {
        return;
    }
    if way_in == WayIn::Socket {
        // Under the instance lock: the socket is this daemon's, and no other daemon listens on it.
        if let Err(err) = std::fs::remove_file(browser::socket::path_in(data)) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%err, "removing the owner socket failed");
            }
        }
    }
    tracing::info!("the browser's way in is closed");
}

/// Opens the way in for `data` (on Windows, at `port`, with the owner pipe where `need_pipe`) and
/// holds the owner key, or says why not.
/// The way in is not kept without the key, which is what it is for.
fn open(
    state: &AppState,
    data: &Path,
    embeds_ui: bool,
    way_in: WayIn,
    port: u16,
    need_pipe: bool,
) -> std::result::Result<Opened, RemoteAccessFailure> {
    let failed = |reason, detail: String| RemoteAccessFailure { reason, detail };
    if !embeds_ui {
        return Err(failed(
            RemoteAccessFailureReason::NoUi,
            "this build of the daemon embeds no UI".to_owned(),
        ));
    }
    let opened = match way_in {
        WayIn::Socket => {
            if let Err(err) = crate::platform::local_socket::check_private_tree(data) {
                return Err(failed(
                    RemoteAccessFailureReason::NotPrivate,
                    format!(
                        "{} is not private to this user ({err}); `chmod 700` it, and keep the \
                         directories above it writable by nobody else",
                        data.display()
                    ),
                ));
            }
            let listener = browser::socket::bind(data).map_err(|err| {
                failed(RemoteAccessFailureReason::SocketFailed, format!("{err:#}"))
            })?;
            Opened::Socket(listener)
        }
        // The owner key file is trusted here through the data directory's access control list,
        // which only the user and the system can enter.
        WayIn::Port => {
            let port_failed = |err: std::io::Error| {
                let reason = if browser::port::in_use(&err) {
                    RemoteAccessFailureReason::PortInUse
                } else {
                    RemoteAccessFailureReason::PortFailed
                };
                failed(reason, format!("listening on 127.0.0.1:{port}: {err}"))
            };
            let listener = browser::port::bind(port).map_err(port_failed)?;
            let listener = tokio::net::TcpListener::from_std(listener).map_err(port_failed)?;
            let pipe = if need_pipe && crate::platform::owner_pipe::AVAILABLE {
                let name = crate::platform::owner_pipe::name_for(data);
                let pipe = crate::platform::owner_pipe::Listener::bind(&name).map_err(|err| {
                    failed(
                        RemoteAccessFailureReason::PipeFailed,
                        format!("opening the owner pipe {name}: {err}"),
                    )
                })?;
                Some(pipe)
            } else {
                None
            };
            Opened::Port(listener, pipe)
        }
    };
    state
        .owner_key
        .enable(&crate::owner_key::path_in(data))
        .map_err(|err| {
            failed(
                RemoteAccessFailureReason::OwnerKeyFailed,
                format!(
                    "reading or writing {}: {err}",
                    crate::owner_key::path_in(data).display()
                ),
            )
        })?;
    Ok(opened)
}

fn serving(opened: Opened, router: axum::Router) -> Vec<ServeTask> {
    match opened {
        Opened::Socket(listener) => vec![browser::serve_on(listener, router)],
        Opened::Port(listener, pipe) => {
            let mut tasks = vec![browser::serve_on_port(listener, router.clone())];
            tasks.extend(pipe.map(|pipe| browser::serve_on(pipe, router)));
            tasks
        }
    }
}

/// The connection details another device's application reaches this daemon with over the
/// computer's SSH server, the path being `octoboardd`, this daemon's installed executable; `None`
/// when they cannot be told (the user's or the machine's name unreadable), or the QR code cannot be
/// made of them.
fn connection_details(octoboardd: &Path) -> Option<ConnectionDetails> {
    let text = crate::connection_details::of_this_machine(Some(octoboardd))?.to_text();
    let modules = crate::connection_details::qr_modules(&text)
        .map_err(|err| tracing::warn!(%err, "no QR code of the connection details"))
        .ok()?;
    let qr = modules
        .iter()
        .map(|row| {
            row.iter()
                .map(|&dark| if dark { '1' } else { '0' })
                .collect()
        })
        .collect();
    Some(ConnectionDetails { text, qr })
}

fn log_serving(way_in: WayIn, data: &Path, port: Option<u16>) {
    match (way_in, port) {
        (WayIn::Port, Some(port)) => tracing::info!(
            port,
            "remote access is on; serving the browser at its loopback port"
        ),
        _ => tracing::info!(
            path = %browser::socket::path_in(data).display(),
            "remote access is on; serving the browser on the owner socket"
        ),
    }
}

fn unavailable(message: &str) -> anyhow::Error {
    CodedError::raised(error_code::REMOTE_ACCESS_UNAVAILABLE, message, &[])
}

/// The refusal of a request that changes remote access over a connection the owner key opened.
pub fn refused_to_browser() -> anyhow::Error {
    CodedError::raised(
        error_code::REMOTE_ACCESS_REFUSED_TO_BROWSER,
        "only the application's own window changes remote access",
        &[],
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_support::{app_state, PATIENCE};

    /// A private data directory under the test's scratch directory, and whether the directories
    /// above it are private too, which a socket needs: where they are not (a temporary directory
    /// on a shared volume), the refusal is what is checked.
    fn data_dir(dir: &Path) -> (PathBuf, bool) {
        use crate::platform::local_socket::test_api::set_mode;
        let data = dir.join("data");
        std::fs::create_dir(&data).unwrap();
        set_mode(&data, 0o700);
        let private = crate::platform::local_socket::check_private_tree(&data).is_ok();
        (data, private)
    }

    fn code_of(err: &anyhow::Error) -> crate::protocol::Code {
        CodedError::of(err).code
    }

    /// Whether the owner socket of `data` takes `key`, asked as a browser would.
    async fn socket_takes(data: &Path, key: &str) -> std::io::Result<bool> {
        let (data, key) = (data.to_path_buf(), key.to_owned());
        tokio::task::spawn_blocking(move || {
            browser::socket::key_accepted(&data, FORWARD_PORT, &key, PATIENCE)
        })
        .await
        .unwrap()
    }

    /// The setting turns the owner socket and the owner key on and off while the daemon runs:
    /// on, the socket answers with the owner key and the address carries it; off, the socket is
    /// gone, the key is refused and what it admitted is cut off, and the setting is stored; on
    /// again, the same key opens it, so a bookmarked address keeps working.
    #[tokio::test]
    async fn the_setting_opens_and_closes_the_owner_socket_while_the_daemon_runs() {
        let (state, dir) = app_state("remote-access-toggle");
        state.set_follows_application();
        let (data, private) = data_dir(&dir);
        state.remote_access.begin(&state, data.clone(), true);
        state
            .remote_access
            .serve_clients(crate::server::browser_router(state.clone()));
        let mut events = state.subscribe();

        let off = state.remote_access.info(&state).expect("offered");
        assert_eq!((off.allowed, off.state), (false, RemoteAccessState::Off));
        assert_eq!(
            (off.address, off.forward_command, off.failure),
            (None, None, None)
        );
        assert!(!browser::socket::path_in(&data).exists());
        assert!(!state.owner_key.is_held());

        state.remote_access.set(&state, true).unwrap();
        assert!(state.store.allow_remote_access().unwrap());
        let on = state.remote_access.info(&state).unwrap();
        let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
            panic!("the change was not broadcast");
        };
        assert_eq!(remote_access, on);
        if !private {
            assert_eq!(on.state, RemoteAccessState::Failed);
            let failure = on.failure.expect("why");
            assert!(matches!(
                failure.reason,
                RemoteAccessFailureReason::NotPrivate
            ));
            assert!(failure.detail.contains("chmod 700"), "{}", failure.detail);
            assert!(!state.owner_key.is_held());
            assert!(!browser::socket::path_in(&data).exists());
            return;
        }
        assert_eq!(on.state, RemoteAccessState::Serving);
        let key = state.owner_key.current().expect("the owner key is held");
        assert_eq!(
            crate::owner_key::read_in(&data).as_deref(),
            Some(key.as_str())
        );
        assert_eq!(
            on.address.as_deref(),
            Some(format!("http://127.0.0.1:{FORWARD_PORT}/?key={key}").as_str())
        );
        let forward = on.forward_command.expect("the forward");
        assert!(
            forward.contains(&format!("127.0.0.1:{FORWARD_PORT}:")),
            "{forward}"
        );
        assert!(forward.contains("browser.sock"), "{forward}");
        assert!(socket_takes(&data, &key).await.unwrap());
        assert!(!socket_takes(&data, &"0".repeat(64)).await.unwrap());
        let mut admitted = state
            .owner_key
            .admit(Some(&format!("key={key}")))
            .expect("admitted");

        state.remote_access.set(&state, false).unwrap();
        assert!(!state.store.allow_remote_access().unwrap());
        let off = state.remote_access.info(&state).unwrap();
        assert_eq!(off.state, RemoteAccessState::Off);
        assert_eq!(off.address, None);
        assert!(!state.owner_key.is_held());
        assert!(!browser::socket::path_in(&data).exists());
        assert!(
            socket_takes(&data, &key).await.is_err(),
            "the socket still answers"
        );
        tokio::time::timeout(PATIENCE, admitted.revoked())
            .await
            .expect("turning it off cuts off what the key opened");

        state.remote_access.set(&state, true).unwrap();
        assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
        assert!(socket_takes(&data, &key).await.unwrap());
        drop(state.remote_access.close());
    }

    /// Rotating the key replaces the address and refuses the old key on the socket.
    #[tokio::test]
    async fn a_rotation_replaces_the_address() {
        let (state, dir) = app_state("remote-access-rotate");
        state.set_follows_application();
        let (data, private) = data_dir(&dir);
        state.remote_access.begin(&state, data.clone(), true);
        state
            .remote_access
            .serve_clients(crate::server::browser_router(state.clone()));
        let refused = state.remote_access.rotate(&state).unwrap_err();
        assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
        if !private {
            return;
        }
        state.remote_access.set(&state, true).unwrap();
        let old = state.owner_key.current().unwrap();
        state.remote_access.rotate(&state).unwrap();
        let new = state.owner_key.current().unwrap();
        assert_ne!(old, new);
        let info = state.remote_access.info(&state).unwrap();
        assert!(info.address.unwrap().ends_with(&new));
        assert!(!socket_takes(&data, &old).await.unwrap());
        assert!(socket_takes(&data, &new).await.unwrap());
        drop(state.remote_access.close());
    }

    /// A setting left on is applied as the daemon starts: the socket is open and the key held
    /// before the daemon serves its clients, which begins the serving.
    #[tokio::test]
    async fn a_setting_left_on_is_applied_at_the_start() {
        let (state, dir) = app_state("remote-access-start");
        state.set_follows_application();
        state.store.set_allow_remote_access(true).unwrap();
        let (data, private) = data_dir(&dir);
        state.remote_access.begin(&state, data.clone(), true);
        let info = state.remote_access.info(&state).unwrap();
        if !private {
            assert_eq!(info.state, RemoteAccessState::Failed);
            return;
        }
        assert_eq!(info.state, RemoteAccessState::Serving);
        assert!(state.owner_key.is_held());
        let key = state.owner_key.current().unwrap();
        state
            .remote_access
            .serve_clients(crate::server::browser_router(state.clone()));
        assert!(socket_takes(&data, &key).await.unwrap());
        drop(state.remote_access.close());
    }

    /// A build that embeds no UI has nothing to serve: turned on, remote access says so, and
    /// neither the socket nor the key is opened.
    #[tokio::test]
    async fn a_build_without_a_ui_says_so() {
        let (state, dir) = app_state("remote-access-no-ui");
        state.set_follows_application();
        let (data, _) = data_dir(&dir);
        state.remote_access.begin(&state, data.clone(), false);
        state.remote_access.set(&state, true).unwrap();
        let info = state.remote_access.info(&state).unwrap();
        assert_eq!(info.state, RemoteAccessState::Failed);
        assert!(matches!(
            info.failure.unwrap().reason,
            RemoteAccessFailureReason::NoUi
        ));
        assert!(!state.owner_key.is_held());
        assert!(!browser::socket::path_in(&data).exists());
    }

    /// A daemon that does not follow the application offers no remote access: no state in the
    /// snapshot, and both requests refused.
    #[tokio::test]
    async fn a_daemon_on_its_own_offers_no_remote_access() {
        let (state, _dir) = app_state("remote-access-alone");
        assert_eq!(state.remote_access.info(&state), None);
        let set = state.remote_access.set(&state, true).unwrap_err();
        assert_eq!(code_of(&set), error_code::REMOTE_ACCESS_UNAVAILABLE);
        assert!(
            !state.store.allow_remote_access().unwrap(),
            "nothing stored"
        );
        let rotate = state.remote_access.rotate(&state).unwrap_err();
        assert_eq!(code_of(&rotate), error_code::REMOTE_ACCESS_UNAVAILABLE);
    }
}

/// The browser port of Windows, exercised on every host through [`RemoteAccess::begin_with`].
#[cfg(test)]
mod port_tests {
    use super::*;
    use crate::test_support::{app_state, ClosedPort, PATIENCE};

    /// Asserts this process listens on `port` no more, and waits until the port is free again: on
    /// Linux and macOS a child process another test is spawning holds a copy of every descriptor
    /// this process has open until its `exec` closes them, so a listener closed a moment ago can
    /// still have the port.
    async fn let_go(port: u16) {
        assert!(
            !crate::platform::proc_table::listening_ports(&[std::process::id() as i32])
                .contains(&port),
            "this process still listens on {port}"
        );
        let gone =
            tokio::task::spawn_blocking(move || crate::test_support::closed_port::let_go(port))
                .await
                .unwrap();
        assert!(gone, "{port} is still listened on");
    }

    /// A listener on a port no other test of this process is given.
    fn held_port() -> (std::net::TcpListener, ClosedPort) {
        let reserved = ClosedPort::new();
        let taken =
            std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, reserved.port())).unwrap();
        (taken, reserved)
    }

    /// Whether the daemon takes `key` at `port`, asked as a browser would; an error when nothing
    /// answers there.
    async fn port_takes(port: u16, key: &str) -> std::io::Result<bool> {
        let key = key.to_owned();
        tokio::task::spawn_blocking(move || browser::port::key_accepted(port, &key, PATIENCE))
            .await
            .unwrap()
    }

    /// A daemon that follows the application, offering remote access through the browser port,
    /// with its data directory, served as the daemon serves it.
    fn port_daemon(label: &str) -> (Arc<AppState>, crate::test_support::ScratchDir) {
        let (state, dir) = app_state(label);
        state.set_follows_application();
        state
            .remote_access
            .begin_with(&state, dir.to_path_buf(), true, WayIn::Port);
        state
            .remote_access
            .serve_clients(crate::server::browser_router(state.clone()));
        (state, dir)
    }

    fn code_of(err: &anyhow::Error) -> crate::protocol::Code {
        CodedError::of(err).code
    }

    /// Whether the daemon whose data directory is `data` takes `key` on its owner pipe, asked as
    /// `octoboardd address --json` reaches it; an error when no pipe of its own answers.
    async fn pipe_takes(data: &Path, key: &str) -> std::io::Result<bool> {
        let (name, key) = (crate::platform::owner_pipe::name_for(data), key.to_owned());
        tokio::task::spawn_blocking(move || {
            let pipe = crate::platform::owner_pipe::connect(&name, PATIENCE)?;
            browser::key_check_over(pipe, FORWARD_PORT, &key)
        })
        .await
        .unwrap()
    }

    /// Where the host has owner pipes (Windows), the switch opens the owner pipe with the browser
    /// port, serving the daemon's router there with the owner key, keeps it through a change of
    /// port, and closes it when turned off.
    #[tokio::test]
    async fn the_owner_pipe_follows_the_switch() {
        if !crate::platform::owner_pipe::AVAILABLE {
            return;
        }
        let (state, dir) = port_daemon("remote-access-pipe");
        assert!(pipe_takes(&dir, "x").await.is_err(), "no pipe while off");
        let (first, second) = (ClosedPort::new(), ClosedPort::new());
        state.remote_access.set_port(&state, first.port()).unwrap();
        state.remote_access.set(&state, true).unwrap();
        let key = state.owner_key.current().expect("serving");
        assert!(pipe_takes(&dir, &key).await.unwrap());
        assert!(!pipe_takes(&dir, &"0".repeat(64)).await.unwrap());

        state.remote_access.set_port(&state, second.port()).unwrap();
        let info = state.remote_access.info(&state).unwrap();
        assert_eq!(info.state, RemoteAccessState::Serving, "{:?}", info.failure);
        assert!(
            info.connection.is_some(),
            "the details are given while serving"
        );
        assert!(pipe_takes(&dir, &key).await.unwrap());

        state.remote_access.set(&state, false).unwrap();
        let gone = crate::test_support::within(PATIENCE, {
            let dir = dir.to_path_buf();
            move || {
                crate::platform::owner_pipe::connect(
                    &crate::platform::owner_pipe::name_for(&dir),
                    std::time::Duration::ZERO,
                )
                .is_err()
            }
        })
        .await;
        assert!(gone, "the pipe still answers once remote access is off");
        drop(state.remote_access.close());
    }

    /// The port is the default until the user sets one, and is said whether remote access is on
    /// or not; the switch opens and closes the port while the daemon runs, with the owner key, and
    /// the address and forward name the port; the daemon's own port refuses the owner key all the
    /// while.
    #[tokio::test]
    async fn the_switch_opens_and_closes_the_browser_port_while_the_daemon_runs() {
        let (state, dir) = port_daemon("remote-access-port-toggle");
        let off = state.remote_access.info(&state).expect("offered");
        assert_eq!(off.browser_port, Some(FORWARD_PORT));
        assert_eq!((off.allowed, off.state), (false, RemoteAccessState::Off));

        let reserved = ClosedPort::new();
        let port = reserved.port();
        state.remote_access.set_port(&state, port).unwrap();
        assert_eq!(state.store.remote_access_port().unwrap(), Some(port));
        let off = state.remote_access.info(&state).unwrap();
        assert_eq!(
            (off.browser_port, off.state),
            (Some(port), RemoteAccessState::Off)
        );
        assert!(!state.owner_key.is_held(), "setting the port opens nothing");
        assert!(port_takes(port, &"0".repeat(64)).await.is_err());

        let mut events = state.subscribe();
        state.remote_access.set(&state, true).unwrap();
        let on = state.remote_access.info(&state).unwrap();
        let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
            panic!("the change was not broadcast");
        };
        assert_eq!(remote_access, on);
        assert_eq!(on.state, RemoteAccessState::Serving, "{:?}", on.failure);
        let key = state.owner_key.current().expect("the owner key is held");
        assert_eq!(
            crate::owner_key::read_in(&dir).as_deref(),
            Some(key.as_str())
        );
        assert_eq!(
            on.address.as_deref(),
            Some(format!("http://127.0.0.1:{port}/?key={key}").as_str())
        );
        let forward = on.forward_command.expect("the forward");
        assert!(
            forward.contains(&format!("-L 127.0.0.1:{port}:127.0.0.1:{port} ")),
            "{forward}"
        );
        assert!(port_takes(port, &key).await.unwrap());
        assert!(!port_takes(port, &"0".repeat(64)).await.unwrap());
        assert!(
            !browser::socket::path_in(&dir).exists(),
            "no socket is opened on this way in"
        );

        // The daemon's own port, which the application and the helpers use, refuses the key.
        let own = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let own_port = own.local_addr().unwrap().port();
        let router = crate::server::router(state.clone());
        let own_served = tokio::spawn(async move { axum::serve(own, router).await });
        assert!(!port_takes(own_port, &key).await.unwrap());

        let mut admitted = state
            .owner_key
            .admit(Some(&format!("key={key}")))
            .expect("admitted");
        state.remote_access.set(&state, false).unwrap();
        let off = state.remote_access.info(&state).unwrap();
        assert_eq!(off.state, RemoteAccessState::Off);
        assert_eq!((off.address, off.forward_command), (None, None));
        assert_eq!(off.browser_port, Some(port));
        assert!(!state.owner_key.is_held());
        assert!(
            port_takes(port, &key).await.is_err(),
            "the port still answers"
        );
        tokio::time::timeout(PATIENCE, admitted.revoked())
            .await
            .expect("turning it off cuts off what the key opened");

        own_served.abort();
        drop(state.remote_access.close());
    }

    /// A change of port while serving lets the old port go, cuts off what the key opened there,
    /// and serves the same key at the new one; the same port again changes nothing.
    #[tokio::test]
    async fn a_change_of_port_moves_the_browser_at_once() {
        let (state, _dir) = port_daemon("remote-access-port-change");
        let (old_port, new_port) = (ClosedPort::new(), ClosedPort::new());
        let old = old_port.port();
        state.remote_access.set_port(&state, old).unwrap();
        state.remote_access.set(&state, true).unwrap();
        let key = state.owner_key.current().expect("serving");
        assert!(port_takes(old, &key).await.unwrap());

        let mut same = state.owner_key.admit(Some(&format!("key={key}"))).unwrap();
        state.remote_access.set_port(&state, old).unwrap();
        // Cutting off is announced by the time the call returns, so a connection not told by now
        // is not going to be.
        assert!(
            futures_util::FutureExt::now_or_never(same.revoked()).is_none(),
            "the same port again cuts nothing off"
        );
        assert!(port_takes(old, &key).await.unwrap());

        let new = new_port.port();
        state.remote_access.set_port(&state, new).unwrap();
        let info = state.remote_access.info(&state).unwrap();
        assert_eq!(info.state, RemoteAccessState::Serving);
        assert_eq!(info.browser_port, Some(new));
        assert_eq!(
            info.address.as_deref(),
            Some(format!("http://127.0.0.1:{new}/?key={key}").as_str())
        );
        assert!(port_takes(new, &key).await.unwrap());
        assert!(
            port_takes(old, &key).await.is_err(),
            "the old port is let go"
        );
        tokio::time::timeout(PATIENCE, same.revoked())
            .await
            .expect("what the key opened at the old port is cut off");
        drop(state.remote_access.close());
    }

    /// A port already in use is said, with nothing served and no owner key held, until the port is
    /// changed; the daemon runs on.
    #[tokio::test]
    async fn a_port_in_use_is_said_until_it_is_changed() {
        let (state, _dir) = port_daemon("remote-access-port-in-use");
        let taken = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let taken_port = taken.local_addr().unwrap().port();
        state.remote_access.set_port(&state, taken_port).unwrap();
        state.remote_access.set(&state, true).unwrap();
        let info = state.remote_access.info(&state).unwrap();
        assert_eq!(info.state, RemoteAccessState::Failed);
        let failure = info.failure.expect("why");
        assert!(
            matches!(failure.reason, RemoteAccessFailureReason::PortInUse),
            "{failure:?}"
        );
        assert!(
            failure.detail.contains(&taken_port.to_string()),
            "{}",
            failure.detail
        );
        assert_eq!((info.address, info.forward_command), (None, None));
        assert!(!state.owner_key.is_held());

        let reserved = ClosedPort::new();
        let free = reserved.port();
        state.remote_access.set_port(&state, free).unwrap();
        let info = state.remote_access.info(&state).unwrap();
        assert_eq!(info.state, RemoteAccessState::Serving);
        assert_eq!(info.failure, None);
        let key = state.owner_key.current().unwrap();
        assert!(port_takes(free, &key).await.unwrap());
        drop(taken);
        drop(state.remote_access.close());
    }

    /// Settings left on are applied at the start, at the stored port; the owner record names the
    /// port while the key is taken there.
    #[tokio::test]
    async fn settings_left_on_are_applied_at_the_start() {
        let (state, dir) = app_state("remote-access-port-start");
        state.set_follows_application();
        let reserved = ClosedPort::new();
        let port = reserved.port();
        state.store.set_allow_remote_access(true).unwrap();
        state.store.set_remote_access_port(Some(port)).unwrap();
        state
            .remote_access
            .begin_with(&state, dir.to_path_buf(), true, WayIn::Port);
        assert_eq!(state.remote_access.recorded_port(&state), Some(port));
        let key = state.owner_key.current().expect("held before serving");
        state
            .remote_access
            .serve_clients(crate::server::browser_router(state.clone()));
        assert!(port_takes(port, &key).await.unwrap());
        state.remote_access.set(&state, false).unwrap();
        assert_eq!(state.remote_access.recorded_port(&state), None);
        drop(state.remote_access.close());
    }

    /// Where the browser goes through the owner socket, there is no port to set.
    #[tokio::test]
    async fn the_socket_way_in_has_no_port() {
        let (state, dir) = app_state("remote-access-port-socket");
        state.set_follows_application();
        state
            .remote_access
            .begin_with(&state, dir.to_path_buf(), true, WayIn::Socket);
        assert_eq!(state.remote_access.info(&state).unwrap().browser_port, None);
        let refused = state.remote_access.set_port(&state, 4000).unwrap_err();
        assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
        assert_eq!(
            state.store.remote_access_port().unwrap(),
            None,
            "nothing stored"
        );
        let (alone, _dir) = app_state("remote-access-port-alone");
        let refused = alone.remote_access.set_port(&alone, 4000).unwrap_err();
        assert_eq!(code_of(&refused), error_code::REMOTE_ACCESS_UNAVAILABLE);
    }

    /// Another device's application is set up from the connection details while the way in is
    /// served: the text is the line `octoboardd connection` prints, the QR code is the code of
    /// exactly that text, and there are none while remote access is off or has failed.
    #[tokio::test]
    async fn the_connection_details_are_given_while_serving_and_not_otherwise() {
        if crate::connection_details::of_this_machine(None).is_none() {
            // Neither the user's nor the machine's name can be read here: no details are told.
            return;
        }
        let (state, _dir) = port_daemon("remote-access-connection");
        let reserved = ClosedPort::new();
        state
            .remote_access
            .set_port(&state, reserved.port())
            .unwrap();
        assert_eq!(state.remote_access.info(&state).unwrap().connection, None);

        state.remote_access.set(&state, true).unwrap();
        let info = state.remote_access.info(&state).unwrap();
        assert_eq!(info.state, RemoteAccessState::Serving, "{:?}", info.failure);
        let connection = info.connection.expect("the details while serving");
        assert!(
            connection.text.starts_with("octoboard+ssh://"),
            "{}",
            connection.text
        );
        assert!(!connection.text.contains(char::is_whitespace));
        let key = state.owner_key.current().unwrap();
        assert!(
            !connection.text.contains(&key),
            "the details carry no secret"
        );
        let modules = crate::connection_details::qr_modules(&connection.text).unwrap();
        assert_eq!(connection.qr.len(), modules.len());
        for (row, modules) in connection.qr.iter().zip(&modules) {
            let drawn: Vec<bool> = row.chars().map(|module| module == '1').collect();
            assert_eq!(&drawn, modules);
            assert!(row.chars().all(|module| module == '0' || module == '1'));
        }

        state.remote_access.set(&state, false).unwrap();
        assert_eq!(state.remote_access.info(&state).unwrap().connection, None);
        let taken = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let taken_port = taken.local_addr().unwrap().port();
        state.remote_access.set_port(&state, taken_port).unwrap();
        state.remote_access.set(&state, true).unwrap();
        let failed = state.remote_access.info(&state).unwrap();
        assert_eq!(failed.state, RemoteAccessState::Failed);
        assert_eq!(failed.connection, None);
        drop(taken);
        drop(state.remote_access.close());
    }

    /// Turning remote access off after it failed leaves nothing of the failure: the state is off
    /// with no reason, the setting is off, and turning it on again tries again.
    #[tokio::test]
    async fn turning_it_off_after_a_failure_clears_the_failure() {
        let (state, _dir) = port_daemon("remote-access-failure-cleared");
        let (taken, _reserved) = held_port();
        let taken_port = taken.local_addr().unwrap().port();
        state.remote_access.set_port(&state, taken_port).unwrap();
        state.remote_access.set(&state, true).unwrap();
        let failed = state.remote_access.info(&state).unwrap();
        assert_eq!(
            (failed.allowed, failed.state),
            (true, RemoteAccessState::Failed)
        );

        state.remote_access.set(&state, false).unwrap();
        let off = state.remote_access.info(&state).unwrap();
        assert_eq!((off.allowed, off.state), (false, RemoteAccessState::Off));
        assert_eq!(off.failure, None);
        assert_eq!(off.browser_port, Some(taken_port), "the port setting stays");

        drop(taken);
        let_go(taken_port).await;
        state.remote_access.set(&state, true).unwrap();
        let on = state.remote_access.info(&state).unwrap();
        assert_eq!(on.state, RemoteAccessState::Serving, "{:?}", on.failure);
        drop(state.remote_access.close());
    }

    /// A rotation that waits on a switch-off in progress finds the key no longer held, and
    /// broadcasts nothing after it: the last word is that remote access is off.
    #[tokio::test]
    async fn a_rotation_racing_a_switch_off_does_not_outlive_it() {
        let (state, _dir) = port_daemon("remote-access-rotate-race");
        let reserved = ClosedPort::new();
        state
            .remote_access
            .set_port(&state, reserved.port())
            .unwrap();
        state.remote_access.set(&state, true).unwrap();
        let mut events = state.subscribe();

        let changing = state.remote_access.changing.lock().unwrap();
        let rotation = std::thread::spawn({
            let state = state.clone();
            move || state.remote_access.rotate_key(&state)
        });
        state.store.set_allow_remote_access(false).unwrap();
        state.remote_access.apply(&state, false);
        drop(changing);

        assert!(matches!(
            rotation.join().unwrap(),
            Err(RotateError::NotHeld)
        ));
        let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
            panic!("the switch-off was not broadcast");
        };
        assert_eq!(remote_access.state, RemoteAccessState::Off);
        assert!(events.try_recv().is_err(), "nothing is broadcast after it");
        drop(state.remote_access.close());
    }

    /// Remote access turned off and straight back on opens the same browser port again: turning
    /// it off frees the port at once, not once the accept loop's cancelled task next runs.
    #[tokio::test]
    async fn off_and_straight_back_on_reopens_the_same_port() {
        let (state, _dir) = port_daemon("remote-access-port-reopen");
        let reserved = ClosedPort::new();
        let port = reserved.port();
        state.remote_access.set_port(&state, port).unwrap();
        state.remote_access.set(&state, true).unwrap();
        assert_eq!(
            state.remote_access.info(&state).unwrap().state,
            RemoteAccessState::Serving
        );

        state.remote_access.set(&state, false).unwrap();
        // `let_go` checks the listener is closed before it awaits anything: before the cancelled
        // task can have run.
        let_go(port).await;
        state.remote_access.set(&state, true).unwrap();
        let on = state.remote_access.info(&state).unwrap();
        assert_eq!(on.state, RemoteAccessState::Serving, "{:?}", on.failure);
        let key = state.owner_key.current().unwrap();
        assert!(port_takes(port, &key).await.unwrap());
        drop(state.remote_access.close());
    }

    /// Rotating the key is refused while nothing is served, as for a daemon that offers none,
    /// whether remote access is off or failed; while serving it cuts off what the old key opened
    /// and the browser port takes the new key alone.
    #[tokio::test]
    async fn a_rotation_is_refused_unless_serving_and_moves_the_browser_port_to_the_new_key() {
        let (state, _dir) = port_daemon("remote-access-port-rotate");
        let off = state.remote_access.rotate(&state).unwrap_err();
        assert_eq!(code_of(&off), error_code::REMOTE_ACCESS_UNAVAILABLE);

        let taken = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let taken_port = taken.local_addr().unwrap().port();
        state.remote_access.set_port(&state, taken_port).unwrap();
        state.remote_access.set(&state, true).unwrap();
        let failed = state.remote_access.rotate(&state).unwrap_err();
        assert_eq!(code_of(&failed), error_code::REMOTE_ACCESS_UNAVAILABLE);

        let reserved = ClosedPort::new();
        let port = reserved.port();
        state.remote_access.set_port(&state, port).unwrap();
        let old = state.owner_key.current().expect("serving at the new port");
        let mut admitted = state
            .owner_key
            .admit(Some(&format!("key={old}")))
            .expect("admitted");
        let mut events = state.subscribe();
        state.remote_access.rotate(&state).unwrap();
        let new = state.owner_key.current().unwrap();
        assert_ne!(old, new);
        let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
            panic!("the rotation was not broadcast");
        };
        assert_eq!(
            remote_access.address.as_deref(),
            Some(format!("http://127.0.0.1:{port}/?key={new}").as_str())
        );
        assert!(!port_takes(port, &old).await.unwrap());
        assert!(port_takes(port, &new).await.unwrap());
        tokio::time::timeout(PATIENCE, admitted.revoked())
            .await
            .expect("a rotation cuts off what the old key opened");
        drop(taken);
        drop(state.remote_access.close());
    }

    /// A port that could not be listened on is tried again when the same port is set again, and
    /// setting a port while remote access is off is broadcast with the port and opens nothing.
    #[tokio::test]
    async fn the_same_port_again_retries_it_and_a_port_set_while_off_is_broadcast() {
        let (state, _dir) = port_daemon("remote-access-port-retry");
        let mut events = state.subscribe();
        let reserved = ClosedPort::new();
        let port = reserved.port();
        state.remote_access.set_port(&state, port).unwrap();
        let Event::RemoteAccessUpdated { remote_access } = events.recv().await.unwrap() else {
            panic!("the port was not broadcast");
        };
        assert_eq!(
            (remote_access.browser_port, remote_access.state),
            (Some(port), RemoteAccessState::Off)
        );
        assert!(port_takes(port, &"0".repeat(64)).await.is_err());

        let (taken, _reserved) = held_port();
        let taken_port = taken.local_addr().unwrap().port();
        state.remote_access.set_port(&state, taken_port).unwrap();
        state.remote_access.set(&state, true).unwrap();
        assert_eq!(
            state.remote_access.info(&state).unwrap().state,
            RemoteAccessState::Failed
        );
        drop(taken);
        let_go(taken_port).await;
        state.remote_access.set_port(&state, taken_port).unwrap();
        let info = state.remote_access.info(&state).unwrap();
        assert_eq!(info.state, RemoteAccessState::Serving, "{:?}", info.failure);
        let key = state.owner_key.current().unwrap();
        assert!(port_takes(taken_port, &key).await.unwrap());
        drop(state.remote_access.close());
    }

    /// Changes asked at once, from requests on tasks of their own, are applied in the order they
    /// are stored: however on and off and a change of port interleave, the way in ends open
    /// exactly when the stored switch is on, and the owner key is held exactly then.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn changes_asked_at_once_leave_the_way_in_as_the_switch_is_stored() {
        const ROUNDS: usize = 20;
        const AT_ONCE: usize = 6;
        let (state, _dir) = port_daemon("remote-access-race");
        // A port of its own for every change of port, held to the end: a port the daemon has let
        // go is never given to it again.
        let reserved: Vec<ClosedPort> = (0..=ROUNDS * AT_ONCE).map(|_| ClosedPort::new()).collect();
        state
            .remote_access
            .set_port(&state, reserved[0].port())
            .unwrap();
        for round in 0..ROUNDS {
            let tasks: Vec<_> = (0..AT_ONCE)
                .map(|i| {
                    let state = state.clone();
                    let port = reserved[1 + round * AT_ONCE + i].port();
                    tokio::task::spawn_blocking(move || {
                        let remote_access = &state.remote_access;
                        match (round + i) % 3 {
                            0 => remote_access.set(&state, true),
                            1 => remote_access.set(&state, false),
                            _ => remote_access.set_port(&state, port),
                        }
                    })
                })
                .collect();
            for task in tasks {
                task.await.unwrap().unwrap();
            }
            let allowed = state.store.allow_remote_access().unwrap();
            let mut info = state.remote_access.info(&state).unwrap();
            // A turn on straight after a turn off can meet the copy a child process holds of the
            // listener just closed (see `let_go`) and find the port in use; once the copy is gone,
            // the same port set again tries it again. `let_go` refuses a listener of the daemon's
            // own.
            if info.state == RemoteAccessState::Failed
                && info
                    .failure
                    .as_ref()
                    .is_some_and(|failure| failure.reason == RemoteAccessFailureReason::PortInUse)
            {
                let port = info.browser_port.expect("the port");
                let_go(port).await;
                state.remote_access.set_port(&state, port).unwrap();
                info = state.remote_access.info(&state).unwrap();
            }
            assert_eq!(state.owner_key.is_held(), allowed, "round {round}");
            let open = state.remote_access.inner.lock().unwrap().open();
            assert_eq!(open, allowed, "round {round}: {info:?}");
        }
        drop(state.remote_access.close());
    }

    /// Turning it on while it is on changes nothing: the key stays, and what it opened is not cut
    /// off.
    #[tokio::test]
    async fn turning_it_on_again_while_serving_cuts_nothing_off() {
        let (state, _dir) = port_daemon("remote-access-on-twice");
        let reserved = ClosedPort::new();
        state
            .remote_access
            .set_port(&state, reserved.port())
            .unwrap();
        state.remote_access.set(&state, true).unwrap();
        let key = state.owner_key.current().expect("serving");
        let mut admitted = state
            .owner_key
            .admit(Some(&format!("key={key}")))
            .expect("admitted");
        state.remote_access.set(&state, true).unwrap();
        assert_eq!(state.owner_key.current().as_deref(), Some(key.as_str()));
        assert!(futures_util::FutureExt::now_or_never(admitted.revoked()).is_none());
        assert_eq!(
            state.remote_access.info(&state).unwrap().state,
            RemoteAccessState::Serving
        );
        drop(state.remote_access.close());
    }

    /// Once the daemon is stopping, nothing is opened any more: the setting is stored but no way
    /// in or key follows.
    #[tokio::test]
    async fn nothing_is_opened_once_the_daemon_has_closed() {
        let (state, _dir) = port_daemon("remote-access-closed");
        let reserved = ClosedPort::new();
        state
            .remote_access
            .set_port(&state, reserved.port())
            .unwrap();
        drop(state.remote_access.close());
        state.remote_access.set(&state, true).unwrap();
        assert!(!state.owner_key.is_held());
        assert_eq!(
            state.remote_access.info(&state).unwrap().state,
            RemoteAccessState::Off
        );
    }
}
