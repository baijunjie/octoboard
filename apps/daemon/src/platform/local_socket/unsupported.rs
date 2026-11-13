//! Windows: no socket of this kind is used, so every call is refused.

use std::io;
use std::path::Path;
use std::time::Duration;

pub(super) const AVAILABLE: bool = false;

/// Never made.
pub(super) enum Listener {}

pub type Stream = tokio::net::TcpStream;
pub type BlockingStream = std::net::TcpStream;

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this host has no owner-only local sockets",
    )
}

pub(super) fn bind_replacing_stale(_path: &Path) -> io::Result<Listener> {
    Err(unsupported())
}

pub(super) async fn accept(listener: &Listener) -> io::Result<Stream> {
    match *listener {}
}

pub(super) fn connect(_path: &Path, _timeout: Duration) -> io::Result<BlockingStream> {
    Err(unsupported())
}

pub(super) fn check_private_tree(_dir: &Path) -> io::Result<()> {
    Err(unsupported())
}
