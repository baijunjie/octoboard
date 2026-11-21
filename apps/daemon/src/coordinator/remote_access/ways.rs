//! The ways in Allow remote access and Allow browser access open: the listeners each setting
//! binds, kept and served, and the opening of the owner socket, the owner pipe and the browser
//! port with the failure each says when it cannot be opened.

use std::path::Path;

use crate::browser::{self, ServeTask};
use crate::protocol::{RemoteAccessFailure, RemoteAccessFailureReason, RemoteAccessState};
use crate::state::AppState;

use super::WayIn;

/// A way in that is bound, to be served once there is a router.
pub(super) enum Bound {
    Socket(crate::platform::local_socket::Listener),
    Pipe(crate::platform::owner_pipe::Listener),
    Port(tokio::net::TcpListener),
}

impl Bound {
    pub(super) fn serve(self, router: axum::Router) -> ServeTask {
        match self {
            Self::Socket(listener) => browser::serve_on(listener, router),
            Self::Pipe(listener) => browser::serve_on(listener, router),
            Self::Port(listener) => browser::serve_on_port(listener, router),
        }
    }
}

/// What one setting opens: its listeners, waiting for the router or served, and why they are not
/// open though the setting is on.
#[derive(Default)]
pub(super) struct Way {
    /// Bound before the daemon serves its clients, and served once it does.
    pending: Vec<Bound>,
    serving: Vec<ServeTask>,
    failure: Option<RemoteAccessFailure>,
}

impl Way {
    pub(super) fn open(&self) -> bool {
        !self.serving.is_empty() || !self.pending.is_empty()
    }

    /// Keeps `bound`, served at once where the daemon has a router.
    pub(super) fn keep(&mut self, bound: Vec<Bound>, router: Option<&axum::Router>) {
        for bound in bound {
            match router {
                Some(router) => self.serving.push(bound.serve(router.clone())),
                None => self.pending.push(bound),
            }
        }
    }

    /// Serves what waits for the router.
    pub(super) fn serve(&mut self, router: &axum::Router) {
        for bound in self.pending.drain(..) {
            self.serving.push(bound.serve(router.clone()));
        }
    }

    /// Closes what is open, the connections made through it with it, and forgets a failure.
    /// Whether anything was open.
    pub(super) fn close(&mut self) -> bool {
        let was_open = self.open();
        self.pending.clear();
        self.serving.clear();
        self.failure = None;
        was_open
    }
}

impl Bound {
    /// Says in the log that the way in is served.
    fn log(&self, data: &Path, what: &str) {
        match self {
            Self::Socket(_) => tracing::info!(
                path = %browser::socket::path_in(data).display(),
                "{what} is on; serving the browser on the owner socket"
            ),
            Self::Pipe(_) => tracing::info!(
                pipe = crate::platform::owner_pipe::name_for(data),
                "{what} is on; serving another computer's application on the owner pipe"
            ),
            Self::Port(listener) => tracing::info!(
                port = listener.local_addr().map(|addr| addr.port()).ok(),
                "{what} is on; serving the browser at its loopback port"
            ),
        }
    }
}

/// What opening a way in gives: the listeners bound, or why not.
pub(super) type Opened = std::result::Result<Vec<Bound>, RemoteAccessFailure>;

/// What a setting stands at as `way` is: serving when it is open, failed when it is on and could
/// not be opened (`failure` says why), else off.
pub(super) fn way_state(
    allowed: bool,
    way: &Way,
) -> (RemoteAccessState, Option<RemoteAccessFailure>) {
    match (&way.failure, way.open()) {
        (_, true) => (RemoteAccessState::Serving, None),
        (Some(failure), false) if allowed => (RemoteAccessState::Failed, Some(failure.clone())),
        _ => (RemoteAccessState::Off, None),
    }
}

/// Keeps what `opened` bound in `way`, served at once where the daemon has a `router`, or the
/// failure it says; `what` is the setting, for the log.
pub(super) fn keep(
    way: &mut Way,
    opened: Opened,
    router: Option<&axum::Router>,
    data: &Path,
    what: &str,
) {
    match opened {
        Ok(bound) => {
            way.failure = None;
            for bound in &bound {
                bound.log(data, what);
            }
            way.keep(bound, router);
        }
        Err(failure) => {
            tracing::warn!(
                reason = ?failure.reason,
                "{what} is on, but it could not be opened: {}",
                failure.detail
            );
            way.failure = Some(failure);
        }
    }
}

/// Closes the owner socket or pipe, with every connection made through it, and removes the
/// socket's file.
pub(super) fn close_remote(data: &Path, way_in: WayIn, remote: &mut Way) {
    if !remote.close() {
        return;
    }
    if way_in.has_socket() {
        // Under the instance lock: the socket is this daemon's, and no other daemon listens on it.
        if let Err(err) = std::fs::remove_file(browser::socket::path_in(data)) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%err, "removing the owner socket failed");
            }
        }
    }
    tracing::info!("remote access is off; its way in is closed");
}

/// Starts to hold the owner key of `data` where the daemon does not hold it yet, or says why it
/// cannot. The key file is trusted here through the data directory's access control list on
/// Windows, which only the user and the system can enter; on macOS, through the file's own mode
/// and owner.
pub(super) fn hold_key(
    state: &AppState,
    data: &Path,
) -> std::result::Result<(), RemoteAccessFailure> {
    if state.owner_key.is_held() {
        return Ok(());
    }
    let path = crate::owner_key::path_in(data);
    state
        .owner_key
        .enable(&path)
        .map_err(|err| RemoteAccessFailure {
            reason: RemoteAccessFailureReason::OwnerKeyFailed,
            detail: format!("reading or writing {}: {err}", path.display()),
        })
}

/// Opens what Allow remote access opens for `data` (the owner socket, or where the host has none,
/// the owner pipe), or says why not.
pub(super) fn open_remote(data: &Path, embeds_ui: bool, way_in: WayIn) -> Opened {
    no_ui(embeds_ui)?;
    if way_in.has_socket() {
        return open_socket(data).map(|socket| vec![Bound::Socket(socket)]);
    }
    let name = crate::platform::owner_pipe::name_for(data);
    crate::platform::owner_pipe::Listener::bind(&name)
        .map(|pipe| vec![Bound::Pipe(pipe)])
        .map_err(|err| RemoteAccessFailure {
            reason: RemoteAccessFailureReason::PipeFailed,
            detail: format!("opening the owner pipe {name}: {err}"),
        })
}

/// Opens what Allow browser access opens, the browser port `port`, or says why not.
pub(super) fn open_browser(state: &AppState, embeds_ui: bool, port: u16) -> Opened {
    no_ui(embeds_ui)?;
    open_port(state, port).map(|port| vec![Bound::Port(port)])
}

/// The refusal of a build that embeds no UI to serve.
fn no_ui(embeds_ui: bool) -> std::result::Result<(), RemoteAccessFailure> {
    if embeds_ui {
        return Ok(());
    }
    Err(RemoteAccessFailure {
        reason: RemoteAccessFailureReason::NoUi,
        detail: "this build of the daemon embeds no UI".to_owned(),
    })
}

/// Opens the owner socket in `data`, or says why not.
fn open_socket(
    data: &Path,
) -> std::result::Result<crate::platform::local_socket::Listener, RemoteAccessFailure> {
    let failed = |reason, detail: String| RemoteAccessFailure { reason, detail };
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
    browser::socket::bind(data)
        .map_err(|err| failed(RemoteAccessFailureReason::SocketFailed, format!("{err:#}")))
}

/// Opens the browser port `port`, or says why not: the daemon's own port is refused, naming both.
fn open_port(
    state: &AppState,
    port: u16,
) -> std::result::Result<tokio::net::TcpListener, RemoteAccessFailure> {
    if port == state.port {
        return Err(RemoteAccessFailure {
            reason: RemoteAccessFailureReason::PortIsDaemons,
            detail: format!(
                "the browser port and the daemon's own port are both {port}; choose another \
                 browser port"
            ),
        });
    }
    let port_failed = |err: std::io::Error| {
        let reason = if browser::port::in_use(&err) {
            RemoteAccessFailureReason::PortInUse
        } else {
            RemoteAccessFailureReason::PortFailed
        };
        RemoteAccessFailure {
            reason,
            detail: format!("listening on 127.0.0.1:{port}: {err}"),
        }
    };
    let listener = browser::port::bind(port).map_err(port_failed)?;
    tokio::net::TcpListener::from_std(listener).map_err(port_failed)
}
