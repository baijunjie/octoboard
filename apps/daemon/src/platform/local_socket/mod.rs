//! A listening socket with a path in the file system, for connections from the daemon's own user
//! alone: on macOS and Linux a Unix-domain socket, which only whoever may enter its directory can
//! reach. Windows has none the daemon uses, so there everything here is refused
//! ([`AVAILABLE`] says so).
//!
//! What makes such a socket the user's alone is its directory, which [`check_private_tree`] judges;
//! the socket's own mode only repeats it.

use std::io;
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
use unix as imp;

#[cfg(windows)]
mod unsupported;

#[cfg(windows)]
use unsupported as imp;

/// Whether this host has sockets of this kind.
pub const AVAILABLE: bool = imp::AVAILABLE;

/// A connection a [`Listener`] accepted.
pub type Stream = imp::Stream;

/// A connection [`connect`] made, for blocking reads and writes.
pub type BlockingStream = imp::BlockingStream;

/// A socket listening at a path.
pub struct Listener(imp::Listener);

impl Listener {
    /// Listens at `path`, made readable and writable by its owner alone. A socket file left there
    /// by a listener that is gone is replaced; anything else at the path is left as it is and
    /// refused. Nothing is checked about the directory: that is [`check_private_tree`]'s. Must be
    /// called within a Tokio runtime.
    pub fn bind_replacing_stale(path: &Path) -> io::Result<Self> {
        imp::bind_replacing_stale(path).map(Self)
    }

    /// The next connection.
    pub async fn accept(&self) -> io::Result<Stream> {
        imp::accept(&self.0).await
    }
}

/// Connects to the socket at `path`, with the connect and every read and write bounded by
/// `timeout`.
pub fn connect(path: &Path, timeout: Duration) -> io::Result<BlockingStream> {
    imp::connect(path, timeout)
}

/// Fails unless only the user this process runs as can create, rename or remove entries of the
/// directory `dir`, and nobody else can enter it: `dir` is a directory the user owns with no
/// permission for anyone else, and each directory above it, on the path as given and as resolved,
/// is owned by the user or the superuser and writable by nobody else unless it is sticky (as `/tmp`
/// is, where others cannot rename what they do not own). The error says which directory failed and
/// why.
pub fn check_private_tree(dir: &Path) -> io::Result<()> {
    imp::check_private_tree(dir)
}

#[cfg(test)]
pub(crate) use imp::test_api;
