//! The browser's way in to the desktop application's daemon on Windows and macOS, and to the server
//! form's daemon once browser access is turned on: a loopback TCP port of its own, open while the
//! setting is on, which serves the UI and accepts the owner key.
//! On Windows it is in place of the owner socket ([`super::socket`]), since Windows' SSH server
//! cannot forward to a socket or a named pipe, so the owner's `ssh -L` forwards to this port; on
//! macOS it is beside the socket, which keeps serving as it did, so that a browser on the same Mac
//! opens the address with no forward run first.
//!
//! This is the one listener on a TCP port that accepts the owner key, and it exists for that reason
//! alone: nothing else may come to rely on it. Its cost is the one a socket avoids: another user of
//! the computer can bind the port whenever the daemon is not holding it (while the application is
//! not running, or while its daemon restarts), and a browser that reconnects through the owner's
//! forward would hand them the key. The confirmation that turns browser access on says so, on macOS
//! as on Windows, and so does `octoboardd browser on`, which a server shared by several users is
//! the likeliest to need to weigh; the server form's port is off until it is turned on. The
//! daemon's own port, which the application and the agents' helpers use, never accepts the owner
//! key, and the application's own way in to a Windows host is never this port.
//!
//! The port is the user's setting, kept by the daemon beside Allow browser access
//! ([`crate::store::Store::remote_access_port`]), the default browser port until they change it.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

/// Whether the desktop application's daemon serves the browser at a loopback port on this host
/// (Windows, macOS).
pub const USED: bool = crate::platform::APPLICATION_BROWSER_PORT;

/// Listens on `port` of the loopback address, for [`super::serve_on_port`] once it is made a Tokio
/// listener. Fails with [`io::ErrorKind::AddrInUse`] when another program, or another user, holds
/// the port.
pub fn bind(port: u16) -> io::Result<TcpListener> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Whether a failure to [`bind`] is the port being held by something else.
pub fn in_use(err: &io::Error) -> bool {
    // Windows answers a port another user's socket holds exclusively with access denied
    // (`WSAEACCES`) rather than in use.
    matches!(
        err.kind(),
        io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied
    )
}

/// Whether the daemon accepts `key` at the loopback `port`, asked as a browser would ask
/// (`GET /key-check`). An error when nothing answers there, or it answers anything else.
pub fn key_accepted(port: u16, key: &str, timeout: Duration) -> io::Result<bool> {
    let stream =
        TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    super::key_check_over(stream, port, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::PATIENCE;

    /// A port something already holds is refused as in use.
    #[test]
    fn a_port_already_held_is_in_use() {
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let err = bind(port).expect_err("the port is held");
        assert!(in_use(&err), "{err:?}");
    }

    /// The key check at the port tells an accepted key from a refused one, through the accept loop
    /// the daemon serves the browser with.
    #[tokio::test]
    async fn the_key_check_goes_over_the_port() {
        let listener = bind(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let _served = crate::browser::serve_on_port(listener, crate::browser::key_check_router());
        let answers = tokio::task::spawn_blocking(move || {
            (
                key_accepted(port, "good", PATIENCE).unwrap(),
                key_accepted(port, "bad", PATIENCE).unwrap(),
            )
        })
        .await
        .unwrap();
        assert_eq!(answers, (true, false));
    }

    /// Dropping the accept loop of a port frees the port at once: this process listens on it no
    /// more before the cancelled task has run. Whether the port can be bound again is not asked:
    /// on Linux and macOS a child process another test is spawning holds a copy of every
    /// descriptor this process has open until its `exec` closes them, so the socket can outlive
    /// the drop for a moment however promptly the daemon let it go.
    #[tokio::test]
    async fn dropping_the_served_port_frees_it_at_once() {
        // No other test of this process is given the port, so a listener found on it is this one.
        let reserved = crate::test_support::ClosedPort::new();
        let port = reserved.port();
        let listening = || {
            crate::platform::proc_table::listening_ports(&[std::process::id() as i32])
                .contains(&port)
        };
        let listener = tokio::net::TcpListener::from_std(bind(port).unwrap()).unwrap();
        let served = crate::browser::serve_on_port(listener, crate::browser::key_check_router());
        assert!(listening(), "the port is served");
        drop(served);
        assert!(!listening(), "the port is still held");
    }
}
