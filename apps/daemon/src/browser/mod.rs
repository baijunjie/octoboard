//! The browser's way in: the listeners that serve the embedded UI and accept the owner key
//! ([`crate::owner_key`]), which a browser on another machine reaches through an SSH forward, and
//! another device's application through `octoboardd proxy`. The server form's daemon always serves
//! its socket, and a loopback port besides while browser access is on; the desktop application's
//! daemon serves its socket (on Windows its owner pipe) while Allow remote access is on and its
//! loopback port while Allow browser access is on, each apart from the other
//! ([`crate::coordinator::remote_access`]).
//!
//! The way in is the owner socket in the data directory on Linux and macOS ([`socket`]), a
//! loopback port of its own ([`port`]) beside it on macOS and in the server form, and on
//! Windows, whose SSH server cannot forward to a socket, the port with the owner pipe
//! ([`crate::platform::owner_pipe`]) in place of the socket, for `octoboardd proxy`. They are all
//! served by the one accept loop here; [`forward`] makes the command a browser's machine forwards
//! them with.
//!
//! See `apps/daemon/PROTOCOL.md`, "Who may connect", for what each listener accepts.

pub mod forward;
pub mod port;
pub mod socket;

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::platform::local_socket;

/// A listener [`serve_on`] accepts connections from: the browser socket ([`socket`]) or, on
/// Windows, the owner pipe ([`crate::platform::owner_pipe`]). The loopback port ([`port`]) is
/// served through [`serve_on_port`] alone.
pub trait Accept: Send + Sync + 'static {
    type Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static;
    fn accept(&self) -> impl std::future::Future<Output = std::io::Result<Self::Stream>> + Send;
}

impl Accept for local_socket::Listener {
    type Stream = local_socket::Stream;
    async fn accept(&self) -> std::io::Result<Self::Stream> {
        local_socket::Listener::accept(self).await
    }
}

impl Accept for crate::platform::owner_pipe::Listener {
    type Stream = crate::platform::owner_pipe::Stream;
    async fn accept(&self) -> std::io::Result<Self::Stream> {
        crate::platform::owner_pipe::Listener::accept(self).await
    }
}

/// A port's listener that [`ServeTask`] can close at once ([`serve_on_port`]): the accept loop
/// borrows it only while it polls, never across an await, so the listener is not held by a task
/// that is still to be cancelled.
struct Releasable(Arc<Mutex<Option<tokio::net::TcpListener>>>);

impl Accept for Releasable {
    type Stream = tokio::net::TcpStream;
    async fn accept(&self) -> std::io::Result<Self::Stream> {
        std::future::poll_fn(|cx| {
            match &*self
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
            {
                Some(listener) => listener.poll_accept(cx).map_ok(|(stream, _)| stream),
                // Released: the task is being cancelled, and nothing will be accepted any more.
                None => std::task::Poll::Pending,
            }
        })
        .await
    }
}

/// Serves `router` on `listener` until the returned task is dropped: each connection on a task of
/// its own, WebSocket upgrades included, as `axum::serve` serves the TCP port. The connections
/// made through it are told when the task is dropped ([`WayClosed`]) and end as well.
pub fn serve_on(listener: impl Accept, router: axum::Router) -> ServeTask {
    let (closed, watching) = tokio::sync::watch::channel(());
    ServeTask {
        task: accept_loop(listener, router.layer(axum::Extension(WayClosed(watching)))),
        port: None,
        _closed: closed,
    }
}

/// What a connection made through a listener served by [`serve_on`] or [`serve_on_port`] watches,
/// as a request extension: it resolves once that listener is closed, which the connection then
/// ends at. A page's connection upgraded to a WebSocket outlives the accept loop and its own task,
/// so closing the listener cannot end it but by telling it. Each way in has its own, which is what
/// lets Allow remote access and Allow browser access be turned off apart: the owner key admits
/// the connections of both, and is let go only once both are closed.
#[derive(Clone)]
pub struct WayClosed(tokio::sync::watch::Receiver<()>);

impl WayClosed {
    /// Resolves when the listener the request came through has been closed.
    pub async fn closed(&mut self) {
        // Nothing is ever sent: the sender going away is the close.
        let _ = self.0.changed().await;
    }
}

/// [`serve_on`] for a loopback port the browser is served at ([`port`]), whose listener is closed
/// as the returned task is dropped, not once the cancelled task is next run: the port is free
/// again at once, for browser access turned off and on, or a change of port and back.
pub fn serve_on_port(listener: tokio::net::TcpListener, router: axum::Router) -> ServeTask {
    let slot = Arc::new(Mutex::new(Some(listener)));
    let (closed, watching) = tokio::sync::watch::channel(());
    ServeTask {
        task: accept_loop(
            Releasable(slot.clone()),
            router.layer(axum::Extension(WayClosed(watching))),
        ),
        port: Some(slot),
        _closed: closed,
    }
}

fn accept_loop(listener: impl Accept, router: axum::Router) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let stream = match listener.accept().await {
                Ok(stream) => stream,
                Err(err) => {
                    tracing::warn!(%err, "accepting a browser way-in connection failed");
                    // A failure such as running out of descriptors passes; a busy loop does not
                    // help it pass.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let service = hyper_util::service::TowerToHyperService::new(router.clone());
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                // A client may shut down its sending half once it has sent its request, as
                // `octoboardd proxy` does when its input ends: the request is still answered.
                if let Err(err) = hyper::server::conn::http1::Builder::new()
                    .half_close(true)
                    .serve_connection(io, service)
                    .with_upgrades()
                    .await
                {
                    tracing::debug!(%err, "a browser way-in connection ended with an error");
                }
            });
        }
    })
}

/// The accept loop [`serve_on`] or [`serve_on_port`] runs; ended when dropped.
pub struct ServeTask {
    task: tokio::task::JoinHandle<()>,
    /// The listener of a port served by [`serve_on_port`], closed on the drop.
    port: Option<Arc<Mutex<Option<tokio::net::TcpListener>>>>,
    /// The sender the connections' [`WayClosed`] watch, which the drop ends.
    _closed: tokio::sync::watch::Sender<()>,
}

impl Drop for ServeTask {
    fn drop(&mut self) {
        if let Some(listener) = &self.port {
            drop(
                listener
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take(),
            );
        }
        self.task.abort();
    }
}

/// Asks the key check of `key` over `stream`, a connection to a listener that serves the browser,
/// naming `port` as the address's port: whether it was accepted, or an error when the answer is
/// neither.
pub fn key_check_over(
    mut stream: impl Read + Write,
    port: u16,
    key: &str,
) -> std::io::Result<bool> {
    let request = format!(
        "GET /key-check?key={key} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let status = std::str::from_utf8(&response)
        .ok()
        .and_then(|text| text.split(' ').nth(1))
        .and_then(|status| status.parse::<u16>().ok());
    match status {
        Some(204) => Ok(true),
        Some(401) => Ok(false),
        other => Err(std::io::Error::other(format!(
            "the daemon answered {}",
            other.map_or_else(
                || "nothing readable".to_owned(),
                |status| status.to_string()
            )
        ))),
    }
}

/// The address a browser opens a daemon at through the forward of its way in to `port` on the
/// browser's machine, with `key`: a loopback address, which a browser treats as a secure context.
pub fn browser_address(port: u16, key: &str) -> String {
    format!("http://127.0.0.1:{port}/?key={key}")
}

/// A router that answers a key check as the daemon's does: 204 to the key `good`, 401 to any
/// other.
#[cfg(test)]
pub(crate) fn key_check_router() -> axum::Router {
    use axum::http::StatusCode;
    use axum::routing::get;

    axum::Router::new().route(
        "/key-check",
        get(
            |axum::extract::RawQuery(query): axum::extract::RawQuery| async move {
                if query.as_deref() == Some("key=good") {
                    StatusCode::NO_CONTENT
                } else {
                    StatusCode::UNAUTHORIZED
                }
            },
        ),
    )
}
