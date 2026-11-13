//! Windows: nothing is handed over.

use std::io;

/// Never made.
pub(crate) enum Kept {}

pub(super) fn keep(_listener: &tokio::net::TcpListener) -> Option<Kept> {
    None
}

pub(super) fn listener_of(kept: &Kept) -> io::Result<std::net::TcpListener> {
    match *kept {}
}

pub(super) fn adopt(_fd: i32, _port: u16) -> io::Result<std::net::TcpListener> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this host hands no listener over",
    ))
}
