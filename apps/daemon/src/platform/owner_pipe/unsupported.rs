//! Linux and macOS: the owner endpoint is a socket there, so every call is refused.

use std::io;
use std::time::Duration;

pub(super) const AVAILABLE: bool = false;

/// Never made.
pub(super) enum Listener {}

pub type Stream = tokio::io::DuplexStream;
pub type BlockingStream = std::fs::File;
pub type AsyncStream = tokio::io::DuplexStream;

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this host has no owner pipes; its owner endpoint is a socket",
    )
}

pub(super) fn bind(_name: &str) -> io::Result<Listener> {
    Err(unsupported())
}

pub(super) async fn accept(listener: &Listener) -> io::Result<Stream> {
    match *listener {}
}

pub(super) fn connect(_name: &str, _timeout: Duration) -> io::Result<BlockingStream> {
    Err(unsupported())
}

pub(super) async fn connect_async(_name: &str, _timeout: Duration) -> io::Result<AsyncStream> {
    Err(unsupported())
}
