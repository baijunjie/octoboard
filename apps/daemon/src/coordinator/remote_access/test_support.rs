//! What the tests of remote access share: daemons of each way in over a data directory of their
//! own, the keys and ports asked as a browser asks them, and the control connections the tests
//! close or keep open.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::test_support::{app_state_for_sockets, ClosedPort, ScratchDir, PATIENCE};

pub(super) fn code_of(err: &anyhow::Error) -> crate::protocol::Code {
    CodedError::of(err).code
}

/// A private data directory under the test's scratch directory, and whether the directories
/// above it are private too, which a socket needs: where they are not (a temporary directory
/// on a shared volume), the refusal is what is checked.
pub(super) fn data_dir(dir: &Path) -> (PathBuf, bool) {
    let data = dir.join("data");
    std::fs::create_dir(&data).unwrap();
    #[cfg(unix)]
    crate::platform::local_socket::test_api::set_mode(&data, 0o700);
    let private = crate::platform::local_socket::check_private_tree(&data).is_ok();
    (data, private)
}

/// Whether the owner socket of `data` takes `key`, asked as a browser would.
#[cfg(unix)]
pub(super) async fn socket_takes(data: &Path, key: &str) -> std::io::Result<bool> {
    let (data, key) = (data.to_path_buf(), key.to_owned());
    tokio::task::spawn_blocking(move || {
        browser::socket::key_accepted(&data, DEFAULT_BROWSER_PORT, &key, PATIENCE)
    })
    .await
    .unwrap()
}

/// Whether the daemon takes `key` at `port`, asked as a browser would; an error when nothing
/// answers there.
pub(super) async fn port_takes(port: u16, key: &str) -> std::io::Result<bool> {
    let key = key.to_owned();
    tokio::task::spawn_blocking(move || browser::port::key_accepted(port, &key, PATIENCE))
        .await
        .unwrap()
}

/// A daemon on a host with both ways in (macOS), over a data directory of its own.
#[cfg(unix)]
pub(super) fn both_ways_daemon(label: &str) -> (Arc<AppState>, PathBuf, bool, ScratchDir) {
    let (state, dir) = app_state_for_sockets(label);
    state.set_follows_application();
    let (data, private) = data_dir(&dir);
    state
        .remote_access
        .begin_with(&state, data.clone(), true, WayIn::SocketAndPort);
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    (state, data, private, dir)
}

/// Asserts this process listens on `port` no more, and waits until the port is free again: on
/// Linux and macOS a child process another test is spawning holds a copy of every descriptor
/// this process has open until its `exec` closes them, so a listener closed a moment ago can
/// still have the port.
pub(super) async fn let_go(port: u16) {
    assert!(
        !crate::platform::proc_table::listening_ports(&[std::process::id() as i32]).contains(&port),
        "this process still listens on {port}"
    );
    let gone = tokio::task::spawn_blocking(move || crate::test_support::closed_port::let_go(port))
        .await
        .unwrap();
    assert!(gone, "{port} is still listened on");
}

/// A listener on a port no other test of this process is given.
pub(super) fn held_port() -> (std::net::TcpListener, ClosedPort) {
    let reserved = ClosedPort::new();
    let taken =
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, reserved.port())).unwrap();
    (taken, reserved)
}

/// A daemon that follows the application, offering browser access at its port (and the owner
/// pipe where the host has one), with its data directory, served as the daemon serves it.
pub(super) fn port_daemon(label: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = app_state_for_sockets(label);
    state.set_follows_application();
    state
        .remote_access
        .begin_with(&state, dir.to_path_buf(), true, WayIn::Port);
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    (state, dir)
}

/// The server form's daemon, which holds its owner key from its start, with the browser port
/// offered over a data directory of its own.
pub(super) fn server_daemon(label: &str) -> (Arc<AppState>, PathBuf, ScratchDir) {
    let (state, dir) = app_state_for_sockets(label);
    let data = dir.join("data");
    std::fs::create_dir(&data).unwrap();
    state
        .owner_key
        .enable(&crate::owner_key::path_in(&data))
        .unwrap();
    state.remote_access.begin_server(&state, data.clone(), true);
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    (state, data, dir)
}

/// A control WebSocket over `stream`, a connection to a listener that serves the browser, with
/// `key`, the address's port being `port`, read up to its snapshot, and the timeout every read
/// after it is bounded by ([`PATIENCE`]) set by `bound`.
pub(super) fn control_over<S: std::io::Read + std::io::Write>(
    stream: S,
    port: u16,
    bound: impl FnOnce(&S),
    key: &str,
) -> tokio_tungstenite::tungstenite::WebSocket<S> {
    use tokio_tungstenite::tungstenite::{self, Message};
    let url = format!("ws://127.0.0.1:{port}/ws/control?key={key}");
    let (mut socket, _) = tungstenite::client(url, stream).unwrap();
    bound(socket.get_ref());
    loop {
        if let Message::Text(text) = socket.read().unwrap() {
            if text.contains("\"snapshot\"") {
                break;
            }
        }
    }
    socket
}

/// Whether `socket` is closed within [`PATIENCE`] of `close` being run.
pub(super) fn closes_after<S: std::io::Read + std::io::Write>(
    mut socket: tokio_tungstenite::tungstenite::WebSocket<S>,
    close: impl FnOnce(),
) -> bool {
    use tokio_tungstenite::tungstenite::{self, Message};
    close();
    loop {
        match socket.read() {
            Ok(Message::Close(_)) => return true,
            Ok(_) => {}
            Err(tungstenite::Error::Io(err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return false
            }
            Err(_) => return true,
        }
    }
}

/// Whether `socket` still answers a ping sent once `change` has been run: asked of a connection
/// that is to stay open, so that a close the change wrongly caused would show as the
/// connection ending instead of its answering.
#[cfg(unix)]
pub(super) fn answers_after<S: std::io::Read + std::io::Write>(
    mut socket: tokio_tungstenite::tungstenite::WebSocket<S>,
    change: impl FnOnce(),
) -> bool {
    use tokio_tungstenite::tungstenite::Message;
    change();
    if socket.send(Message::Ping(vec![1])).is_err() {
        return false;
    }
    loop {
        match socket.read() {
            Ok(Message::Pong(_)) => return true,
            Ok(Message::Close(_)) | Err(_) => return false,
            Ok(_) => {}
        }
    }
}

/// A control connection to the browser port `port` with `key`, read up to its snapshot, then
/// whether it is closed within [`PATIENCE`] of `close` being run.
pub(super) async fn closed_by(port: u16, key: &str, close: impl FnOnce() + Send + 'static) -> bool {
    let key = key.to_owned();
    tokio::task::spawn_blocking(move || {
        let stream = crate::loopback::connect(port, PATIENCE).unwrap();
        let socket = control_over(
            stream,
            port,
            |stream| stream.set_read_timeout(Some(PATIENCE)).unwrap(),
            &key,
        );
        closes_after(socket, close)
    })
    .await
    .unwrap()
}

/// A control connection to the browser port `port` with `key`, read up to its snapshot, then
/// whether it still answers once `change` has been run.
#[cfg(unix)]
pub(super) async fn answers_over_port_after(
    port: u16,
    key: &str,
    change: impl FnOnce() + Send + 'static,
) -> bool {
    let key = key.to_owned();
    tokio::task::spawn_blocking(move || {
        let stream = crate::loopback::connect(port, PATIENCE).unwrap();
        let socket = control_over(
            stream,
            port,
            |stream| stream.set_read_timeout(Some(PATIENCE)).unwrap(),
            &key,
        );
        answers_after(socket, change)
    })
    .await
    .unwrap()
}

/// A control connection to the owner socket of `data` with `key`, read up to its snapshot,
/// then whether it is closed within [`PATIENCE`] of `close` being run.
#[cfg(unix)]
pub(super) async fn closed_over_socket(
    data: &Path,
    key: &str,
    close: impl FnOnce() + Send + 'static,
) -> bool {
    let (data, key) = (data.to_path_buf(), key.to_owned());
    tokio::task::spawn_blocking(move || {
        let stream =
            crate::platform::local_socket::connect(&browser::socket::path_in(&data), PATIENCE)
                .unwrap();
        let socket = control_over(
            stream,
            DEFAULT_BROWSER_PORT,
            |stream| stream.set_read_timeout(Some(PATIENCE)).unwrap(),
            &key,
        );
        closes_after(socket, close)
    })
    .await
    .unwrap()
}

/// A control connection to the owner socket of `data` with `key`, read up to its snapshot,
/// then whether it still answers once `change` has been run.
#[cfg(unix)]
pub(super) async fn answers_over_socket_after(
    data: &Path,
    key: &str,
    change: impl FnOnce() + Send + 'static,
) -> bool {
    let (data, key) = (data.to_path_buf(), key.to_owned());
    tokio::task::spawn_blocking(move || {
        let stream =
            crate::platform::local_socket::connect(&browser::socket::path_in(&data), PATIENCE)
                .unwrap();
        let socket = control_over(
            stream,
            DEFAULT_BROWSER_PORT,
            |stream| stream.set_read_timeout(Some(PATIENCE)).unwrap(),
            &key,
        );
        answers_after(socket, change)
    })
    .await
    .unwrap()
}

/// A data directory the owner socket refuses (open to other users), so that the socket
/// fails whatever the directories above it are, and the port can serve alone.
#[cfg(unix)]
pub(super) fn socket_refusing_data(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let data = dir.join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
    data
}

/// A daemon with both ways in whose socket cannot be opened.
#[cfg(unix)]
pub(super) fn socket_refusing_daemon(label: &str) -> (Arc<AppState>, PathBuf, ScratchDir) {
    let (state, dir) = app_state_for_sockets(label);
    state.set_follows_application();
    let data = socket_refusing_data(&dir);
    state
        .remote_access
        .begin_with(&state, data.clone(), true, WayIn::SocketAndPort);
    state
        .remote_access
        .serve_clients(crate::server::browser_router(state.clone()));
    (state, data, dir)
}
