//! The owner endpoint on Windows: a named pipe that only the daemon's user can open and that
//! refuses clients on other computers, in place of the socket in the data directory that Linux and
//! macOS use ([`super::local_socket`]). Elsewhere there is none ([`AVAILABLE`] says so) and every
//! call is refused.
//!
//! A pipe's name is global: any user of the computer can create a pipe of any name not yet taken,
//! with an access control list of their own. So the daemon creates its pipe as the name's first
//! instance, with a list that lets only its own user open it and that user as the owner; where the
//! name is taken, it joins the pipe only when that pipe is its user's own (one whose connections
//! outlive an earlier listener), and fails otherwise; and a client checks, once connected, that the
//! pipe it reached is owned by the user the client runs as, so a pipe someone else made under the
//! name while the daemon was not holding it is refused before a byte is sent. A client opens the
//! pipe at the identification level of impersonation only, so whoever made the pipe cannot act as
//! the client either.
//!
//! The name follows from the data directory ([`name_for`]), so the daemon and a command run as its
//! user find the same pipe without anything written down, and two daemons with data directories of
//! their own (a development build beside the installed one) have pipes of their own.

use std::io;
use std::path::Path;
use std::time::Duration;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as imp;

#[cfg(not(windows))]
mod unsupported;

#[cfg(not(windows))]
use unsupported as imp;

/// Whether this host has owner pipes.
pub const AVAILABLE: bool = imp::AVAILABLE;

/// A connection a [`Listener`] accepted.
pub type Stream = imp::Stream;

/// A connection [`connect`] made, for blocking reads and writes.
pub type BlockingStream = imp::BlockingStream;

/// A connection [`connect_async`] made.
pub type AsyncStream = imp::AsyncStream;

/// The prefix of every owner pipe's name, after the pipe namespace.
const PREFIX: &str = "octoboard-owner-";

/// The name of the owner pipe of the daemon whose data directory is `data`:
/// `\\.\pipe\octoboard-owner-<hash>`, the hash being of the directory's path as Windows compares
/// paths, without regard to case or to the separator used.
pub fn name_for(data: &Path) -> String {
    use sha2::{Digest, Sha256};

    let path = data
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    let digest = Sha256::digest(path.as_bytes());
    let hash: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(r"\\.\pipe\{PREFIX}{hash}")
}

/// The owner pipe, listening.
pub struct Listener(imp::Listener);

impl Listener {
    /// Creates the pipe `name`, which only the user this process runs as can open, owned by that
    /// user and refusing remote clients, or joins this user's own pipe of that name. Fails when
    /// a pipe of anyone else holds the name. Must be called within a Tokio runtime.
    pub fn bind(name: &str) -> io::Result<Self> {
        imp::bind(name).map(Self)
    }

    /// The next connection.
    pub async fn accept(&self) -> io::Result<Stream> {
        imp::accept(&self.0).await
    }
}

/// Connects to the pipe `name`, waiting up to `timeout` while every instance is busy, and checks
/// that the pipe is owned by the user this process runs as: one owned by anyone else is refused
/// with [`io::ErrorKind::PermissionDenied`]. Reads and writes on it are not bounded.
pub fn connect(name: &str, timeout: Duration) -> io::Result<BlockingStream> {
    imp::connect(name, timeout)
}

/// [`connect`], for asynchronous reads and writes. Must be called within a Tokio runtime.
pub async fn connect_async(name: &str, timeout: Duration) -> io::Result<AsyncStream> {
    imp::connect_async(name, timeout).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_follows_the_data_directory_as_windows_compares_paths() {
        let name = name_for(Path::new(
            r"C:\Users\Bob\AppData\Roaming\dev.octoboard.desktop",
        ));
        assert!(name.starts_with(r"\\.\pipe\octoboard-owner-"), "{name}");
        assert_eq!(name.len(), r"\\.\pipe\octoboard-owner-".len() + 32);
        assert_eq!(
            name,
            name_for(Path::new(
                r"c:\users\bob\appdata\roaming\DEV.OCTOBOARD.DESKTOP\"
            ))
        );
        assert_eq!(
            name,
            name_for(Path::new(
                "C:/Users/Bob/AppData/Roaming/dev.octoboard.desktop"
            ))
        );
        assert_ne!(
            name,
            name_for(Path::new(
                r"C:\Users\Bob\AppData\Roaming\dev.octoboard.desktop.dev"
            ))
        );
    }

    #[test]
    fn nothing_is_offered_where_there_are_no_owner_pipes() {
        if AVAILABLE {
            return;
        }
        assert!(connect(&name_for(Path::new("/tmp/x")), Duration::ZERO).is_err());
    }
}
