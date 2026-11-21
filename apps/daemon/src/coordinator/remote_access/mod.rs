//! Remote access and browser access to the desktop application's daemon: two settings of their
//! own, each turned on and off alone and applied at once, at the daemon's start and while it runs.
//!
//! Allow remote access is for another computer reaching this one over SSH: it opens the owner
//! socket on macOS and Linux ([`browser::socket`]), which a browser forwarding to it and another
//! computer's application through `octoboardd proxy` both use, and on Windows, whose SSH server
//! cannot forward to a socket, the owner pipe a named pipe only the daemon's user can open
//! ([`crate::platform::owner_pipe`]), which another computer's application reaches through
//! `octoboardd proxy`. Allow browser access opens the loopback browser port ([`browser::port`])
//! whose number is a setting beside the switch, on Windows and macOS, so that a browser on this
//! computer opens its address with no forward run first. Either can be on without the other.
//!
//! The daemon holds and accepts the owner key ([`crate::owner_key`]), as the server form's daemon
//! always does, while either way in is open, and refuses it once both are closed. Each way in
//! closing takes the connections made through it with it ([`browser::WayClosed`]): turning remote
//! access off leaves what runs through the browser port as it is, and turning browser access off
//! leaves what runs through the socket; only both off lets the key go and closes every connection
//! it opened, as a rotation does. Changing the port closes the way in at the old port and opens
//! the new one; the owner socket or pipe is not touched. One way in failing to open does not stop
//! the other from serving; each says its own failure.
//!
//! The server form's daemon (`RemoteAccess::begin_server`) keeps browser access alone: its owner
//! socket and the owner key are its own from the start, and serve whatever the setting says; it
//! has no Allow remote access. Turning browser access off there closes the port and every
//! connection made through it, and leaves the socket, the key and the connections through the
//! socket as they are; only a rotation changes the key.
//!
//! The settings are the user's alone: no agent's settings tool reads or changes them, no
//! configuration export carries them, and a connection the owner key opened (a browser's) cannot
//! change them or rotate the key.
//!
//! The owner record says whether the daemon takes the owner key at its owner endpoint (the socket or
//! pipe Allow remote access opens), and at which port where it serves the browser at one, so
//! `octoboardd address` and `octoboardd proxy` answer for the application's daemon: the record is
//! written again as the settings are applied.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Result;

use crate::browser;
use crate::owner_key::RotateError;
use crate::owner_record::Owner;
use crate::protocol::{
    error_code, CodedError, ConnectionDetails, Event, RemoteAccess as RemoteAccessInfo,
    RemoteAccessState,
};
use crate::state::AppState;

mod ways;
use ways::{close_remote, hold_key, keep, open_browser, open_remote, way_state, Opened, Way};
#[cfg(all(test, unix))]
mod both_ways_tests;
#[cfg(test)]
mod port_tests;
#[cfg(test)]
mod server_tests;
#[cfg(all(test, unix))]
mod socket_tests;
#[cfg(test)]
mod test_support;

/// The port a browser's machine forwards the owner socket to, in the address and the forward
/// command, and the browser port's default until the user sets another: the same at every launch,
/// so that an address the owner bookmarked keeps opening. The daemon's own TCP port, on both forms,
/// is one the system assigns, and never this one.
pub const DEFAULT_BROWSER_PORT: u16 =
    match u16::from_str_radix(env!("OCTOBOARD_DEFAULT_BROWSER_PORT"), 10) {
        Ok(port) => port,
        Err(_) => panic!("OCTOBOARD_DEFAULT_BROWSER_PORT is not a port number"),
    };

/// Which ways in a browser has to the daemon on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WayIn {
    /// The owner socket in the data directory, which Allow remote access opens (Linux).
    Socket,
    /// A loopback port of its own, which Allow browser access opens, at the port the user set,
    /// and, where the host has owner pipes, the owner pipe Allow remote access opens (Windows;
    /// the server form's, whose socket is served apart from either setting).
    Port,
    /// Both the socket and the port, served independently (macOS).
    SocketAndPort,
}

impl WayIn {
    /// This host's, or `None` where it has neither.
    fn of_host() -> Option<Self> {
        match (browser::socket::AVAILABLE, browser::port::USED) {
            (true, true) => Some(Self::SocketAndPort),
            (false, true) => Some(Self::Port),
            (true, false) => Some(Self::Socket),
            (false, false) => None,
        }
    }

    fn has_socket(self) -> bool {
        matches!(self, Self::Socket | Self::SocketAndPort)
    }

    fn has_port(self) -> bool {
        matches!(self, Self::Port | Self::SocketAndPort)
    }
}

/// What a daemon that follows the application holds of remote access and browser access. Empty,
/// and offering nothing, in a daemon that does not ([`RemoteAccess::begin`] is never called there).
#[derive(Default)]
pub struct RemoteAccess {
    /// The data directory, set by [`RemoteAccess::begin`]: the daemon offers its settings only once
    /// it is.
    data: OnceLock<PathBuf>,
    /// Whether the daemon's build embeds a UI to serve, set with `data`.
    embeds_ui: OnceLock<bool>,
    /// Whether this is the server form's daemon, whose socket and owner key are not the settings'
    /// to open and close, set with `data`.
    server: OnceLock<bool>,
    /// The browser's ways in on this host, set with `data`.
    way_in: OnceLock<WayIn>,
    inner: Mutex<Inner>,
    /// Held across each change of the settings and its application, from the store's write to the
    /// way in opened or closed: requests run on tasks of their own, and a change applied after a
    /// later one would leave a way in open with its switch stored off.
    changing: Mutex<()>,
}

#[derive(Default)]
struct Inner {
    /// The router the ways in serve, given once the daemon serves its clients; until then an open
    /// way in waits in its `pending`.
    router: Option<axum::Router>,
    /// What Allow remote access opens: the owner socket, or on Windows the owner pipe. A pipe
    /// closed and made again at once could be reached in between through the closing listener's
    /// last instance, which ends the connection unanswered: the pipe a quick off and on replaces
    /// is only aborted, not awaited, so for a moment the new one may not be accepting yet and a
    /// connection made then fails, which another device's application retries.
    remote: Way,
    /// What Allow browser access opens: the browser port.
    browser: Way,
    /// The daemon's owner record, written again whenever whether it takes the owner key, or at
    /// which port, changes.
    record: Option<Owner>,
    /// Set as the daemon stops: nothing is opened or recorded any more.
    closed: bool,
}

impl RemoteAccess {
    /// Offers the two settings in a daemon that follows the application, whose data directory is
    /// `data` and whose build embeds a UI where `embeds_ui`, on a host with a way in for the
    /// browser, and applies the stored settings: the ways in are opened and the owner key held
    /// now, before the owner record is written, and served once [`RemoteAccess::serve_clients`] is
    /// called.
    pub fn begin(&self, state: &Arc<AppState>, data: PathBuf, embeds_ui: bool) {
        if let Some(way_in) = WayIn::of_host() {
            self.begin_with(state, data, embeds_ui, way_in);
        }
    }

    /// Offers the browser port in the server form's daemon, which already serves its owner socket
    /// and holds the owner key, whose data directory is `data` and whose build embeds a UI where
    /// `embeds_ui`, on a host with a socket: the stored setting, off where none is stored, is
    /// applied now, and the port served once [`RemoteAccess::serve_clients`] is called.
    pub fn begin_server(&self, state: &Arc<AppState>, data: PathBuf, embeds_ui: bool) {
        let _ = self.server.set(true);
        self.begin_with(state, data, embeds_ui, WayIn::Port);
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
        self.apply(state);
    }

    /// Whether this daemon offers either setting.
    pub fn offered(&self) -> bool {
        self.data.get().is_some()
    }

    /// Whether this is the server form's daemon.
    fn is_server(&self) -> bool {
        self.server.get() == Some(&true)
    }

    /// Whether Allow remote access is a setting of this daemon: where the host has an owner socket
    /// or an owner pipe, and not in the server form, whose socket serves always.
    fn remote_offered(&self) -> bool {
        !self.is_server()
            && self.way_in.get().is_some_and(|way_in| {
                way_in.has_socket() || (way_in.has_port() && crate::platform::owner_pipe::AVAILABLE)
            })
    }

    /// Whether Allow remote access is on, where it is a setting.
    fn remote_allowed(&self, state: &AppState) -> bool {
        self.remote_offered() && state.store.allow_remote_access().unwrap_or(false)
    }

    /// Whether Allow browser access is on, where the daemon has a browser port.
    fn browser_allowed(&self, state: &AppState) -> bool {
        self.browser_port(state).is_some() && state.store.allow_browser_access().unwrap_or(false)
    }

    /// The port the browser is served at (Windows, macOS, and the server form), as the user set
    /// it: `None` where the browser goes through the owner socket alone, or the daemon offers
    /// nothing.
    fn browser_port(&self, state: &AppState) -> Option<u16> {
        self.way_in
            .get()
            .is_some_and(|way_in| way_in.has_port())
            .then(|| {
                state
                    .store
                    .remote_access_port()
                    .unwrap_or_else(|err| {
                        tracing::warn!(%err, "reading the browser port failed; the default is used");
                        None
                    })
                    .unwrap_or(DEFAULT_BROWSER_PORT)
            })
    }

    /// The browser port for the owner record: set while the owner key is taken at it.
    pub fn recorded_port(&self, state: &AppState) -> Option<u16> {
        self.browser_port(state)
            .filter(|_| state.owner_key.is_held() && self.inner.lock().unwrap().browser.open())
    }

    /// Whether the owner record says the daemon takes the owner key at its owner endpoint, the
    /// socket (on Windows the owner pipe) that Allow remote access opens: the server form's always
    /// does while it holds the key; the other daemons' only while that way in is open. The browser
    /// port is recorded apart ([`RemoteAccess::recorded_port`]), so that `address` and `proxy` can
    /// tell which way in serves.
    pub fn recorded_endpoint(&self, state: &AppState) -> bool {
        if !self.offered() || self.is_server() {
            return state.owner_key.is_held();
        }
        self.inner.lock().unwrap().remote.open()
    }

    /// Starts serving the ways in with `router`, now and whenever a setting is turned on from here
    /// on: called once the daemon serves its other clients.
    pub fn serve_clients(&self, router: axum::Router) {
        let mut inner = self.inner.lock().unwrap();
        inner.remote.serve(&router);
        inner.browser.serve(&router);
        inner.router = Some(router);
    }

    /// Keeps `owner`, the daemon's owner record as written, to write again as the settings are
    /// turned on and off.
    pub fn keep_record(&self, owner: Owner) {
        self.inner.lock().unwrap().record = Some(owner);
    }

    /// Stops serving as the daemon stops, and hands back its owner record as it stands, for the
    /// daemon to remove. The socket file is left for the next daemon, which replaces it.
    pub fn close(&self) -> Option<Owner> {
        let mut inner = self.inner.lock().unwrap();
        inner.closed = true;
        inner.remote.close();
        inner.browser.close();
        // The router holds the state, which holds this: let it go.
        inner.router = None;
        inner.record.clone()
    }

    /// Where the two settings stand, for the snapshot; `None` where they are not offered.
    pub fn info(&self, state: &AppState) -> Option<RemoteAccessInfo> {
        let data = self.data.get()?;
        // Whether a way in can be serving: the owner key is held (the server form's always is, so
        // its setting stands for it). Only then are the server addresses read, and before the lock
        // is taken: reading them asks the system (a process on macOS), which nothing holding a lock
        // waits on.
        let wanted = if self.is_server() {
            self.browser_allowed(state)
        } else {
            state.owner_key.current().is_some()
        };
        let addresses = if wanted {
            crate::platform::net::server_addresses()
        } else {
            Vec::new()
        };
        let inner = self.inner.lock().unwrap();
        Some(self.info_of(state, data, &inner, addresses))
    }

    /// The state as `inner` stands, with `server_addresses` as read for it (given only while a way
    /// in serves).
    fn info_of(
        &self,
        state: &AppState,
        data: &Path,
        inner: &Inner,
        server_addresses: Vec<String>,
    ) -> RemoteAccessInfo {
        let allowed = self.remote_allowed(state);
        let browser_allowed = self.browser_allowed(state);
        let octoboardd = Path::new(&state.install_exe);
        let browser_port = self.browser_port(state);
        let key = state.owner_key.current();
        let (remote_state, failure) = way_state(allowed, &inner.remote);
        let (browser_state, browser_failure) = way_state(browser_allowed, &inner.browser);
        let remote_serving = remote_state == RemoteAccessState::Serving;
        let serving = remote_serving || browser_state == RemoteAccessState::Serving;
        // The browser port's address and forward while it is open; where only the socket serves,
        // the socket's forward stands. Windows has no socket, so remote access alone gives a
        // browser nothing there.
        let served_port = browser_port.filter(|_| browser_state == RemoteAccessState::Serving);
        let socket_served =
            remote_serving && self.way_in.get().is_some_and(|way_in| way_in.has_socket());
        let reachable = served_port.is_some() || socket_served;
        let port = served_port.unwrap_or(DEFAULT_BROWSER_PORT);
        RemoteAccessInfo {
            allowed,
            state: remote_state,
            browser_allowed,
            browser_state,
            browser_port,
            address: key
                .filter(|_| reachable)
                .map(|key| browser::browser_address(port, &key)),
            forward_command: match served_port {
                Some(port) => Some(browser::forward::to_port_for_account(port)),
                None => socket_served.then(|| browser::forward::to_socket_in(port, data)),
            },
            server_addresses: if serving {
                server_addresses
            } else {
                Vec::new()
            },
            failure,
            browser_failure,
            connection: remote_serving
                .then(|| connection_details(octoboardd))
                .flatten(),
        }
    }

    /// Turns Allow remote access on or off, as the user asked over a connection the owner key did
    /// not open, applies it and broadcasts where remote access stands.
    pub fn set(&self, state: &Arc<AppState>, allowed: bool) -> Result<()> {
        if !self.remote_offered() {
            return Err(unavailable("this daemon offers no remote access"));
        }
        let _changing = self.changing.lock().unwrap();
        state.store.set_allow_remote_access(allowed)?;
        self.apply(state);
        Ok(())
    }

    /// Turns Allow browser access on or off, as the user asked over a connection the owner key did
    /// not open, applies it and broadcasts where browser access stands.
    pub fn set_browser(&self, state: &Arc<AppState>, allowed: bool) -> Result<()> {
        if self.browser_port(state).is_none() {
            return Err(unavailable(
                "this daemon serves the browser at no port of its own",
            ));
        }
        let _changing = self.changing.lock().unwrap();
        state.store.set_allow_browser_access(allowed)?;
        self.apply(state);
        Ok(())
    }

    /// Sets the port the browser is served at, as the user asked over a connection the owner key
    /// did not open, and applies it at once: while browser access is on, the way in at the old
    /// port is closed with every connection made through it, and the new port opened (or why not
    /// said). The same port again while it is served changes nothing; while it could not be
    /// opened, it is tried again.
    pub fn set_port(&self, state: &Arc<AppState>, port: u16) -> Result<()> {
        let _changing = self.changing.lock().unwrap();
        let Some(current) = self.browser_port(state) else {
            return Err(unavailable(
                "this daemon serves the browser at no port of its own",
            ));
        };
        state.store.set_remote_access_port(Some(port))?;
        let served = {
            let inner = self.inner.lock().unwrap();
            inner.browser.open()
        };
        if self.browser_allowed(state) && !(served && current == port) {
            self.close_browser();
            self.apply(state);
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
            RotateError::NotHeld => unavailable("neither remote access nor browser access is on"),
            RotateError::Write(_) => {
                anyhow::anyhow!("rotating the owner key failed: {}", err.message())
            }
        })
    }

    /// Replaces the owner key and broadcasts the new address: every rotation of the daemon's, the
    /// one `octoboardd address --rotate` asks for too. Where remote access is offered, it runs
    /// under the lock a change of the settings holds, so a rotation that races the last way in
    /// being turned off either finds the key no longer held or is broadcast before the switch-off,
    /// never after.
    pub fn rotate_key(&self, state: &AppState) -> std::result::Result<(), RotateError> {
        let _changing = self.offered().then(|| self.changing.lock().unwrap());
        state.owner_key.rotate()?;
        tracing::info!("the owner key was rotated");
        // A desktop daemon's Settings show the address, which the rotation replaced.
        self.broadcast(state);
        Ok(())
    }

    /// Opens or closes each way in to match its setting, holds the owner key while either is open
    /// and lets it go once neither is, records whether the daemon takes the key, and broadcasts
    /// where the settings stand. A way in whose setting is on and that is not open is tried again.
    /// The caller holds `changing`.
    fn apply(&self, state: &Arc<AppState>) {
        let (Some(data), Some(&way_in)) = (self.data.get(), self.way_in.get()) else {
            return;
        };
        let remote = self.remote_allowed(state);
        let browser = self.browser_allowed(state);
        let port = self.browser_port(state);
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.closed {
                return;
            }
            let embeds_ui = self.embeds_ui.get() == Some(&true);
            let router = inner.router.clone();
            if !remote {
                close_remote(data, way_in, &mut inner.remote);
            }
            if !browser && inner.browser.close() {
                tracing::info!("browser access is off; the browser port is closed");
            }
            let mut opened_remote =
                (remote && !inner.remote.open()).then(|| open_remote(data, embeds_ui, way_in));
            let mut opened_browser = (browser && !inner.browser.open())
                .then(|| open_browser(state, embeds_ui, port.unwrap_or(DEFAULT_BROWSER_PORT)));
            if !self.is_server() {
                // Held before anything is served, while either way in is open or opened, and let go
                // (with every connection it opened closed) once neither is.
                let opened = |opened: &Option<Opened>| matches!(opened, Some(Ok(_)));
                if inner.remote.open()
                    || inner.browser.open()
                    || opened(&opened_remote)
                    || opened(&opened_browser)
                {
                    if let Err(failure) = hold_key(state, data) {
                        // The ways in are for the key: not kept without it.
                        for opening in [&mut opened_remote, &mut opened_browser] {
                            if opened(opening) {
                                *opening = Some(Err(failure.clone()));
                            }
                        }
                    }
                } else {
                    state.owner_key.disable();
                }
            }
            if let Some(opened) = opened_remote {
                keep(
                    &mut inner.remote,
                    opened,
                    router.as_ref(),
                    data,
                    "remote access",
                );
            }
            if let Some(opened) = opened_browser {
                keep(
                    &mut inner.browser,
                    opened,
                    router.as_ref(),
                    data,
                    "browser access",
                );
            }
            let takes = if self.is_server() {
                state.owner_key.is_held()
            } else {
                inner.remote.open()
            };
            let recorded_port = port.filter(|_| state.owner_key.is_held() && inner.browser.open());
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

    /// Closes the browser port, without changing the setting, ahead of opening it again at another
    /// port; the owner socket or pipe stays open, and so does the owner key.
    fn close_browser(&self) {
        self.inner.lock().unwrap().browser.close();
    }

    /// Broadcasts where the settings stand; nothing where they are not offered.
    pub(crate) fn broadcast(&self, state: &AppState) {
        if let Some(remote_access) = self.info(state) {
            state.broadcast(Event::RemoteAccessUpdated { remote_access });
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

fn unavailable(message: &str) -> anyhow::Error {
    CodedError::raised(error_code::REMOTE_ACCESS_UNAVAILABLE, message, &[])
}

/// The refusal of a request that changes remote access over a connection the owner key opened.
pub fn refused_to_browser() -> anyhow::Error {
    CodedError::raised(
        error_code::REMOTE_ACCESS_REFUSED_TO_BROWSER,
        "remote access and browser access are changed only with the launch key, not over a connection the owner key opened",
        &[],
    )
}
