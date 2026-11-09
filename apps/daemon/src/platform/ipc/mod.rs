//! A private channel a child process of the daemon listens on, for the daemon's connections alone:
//! on macOS and Linux a Unix socket in a directory only the user can enter; on Windows a loopback
//! port, which any local process can reach, guarded by a secret token the caller hands the child
//! and presents on each connection. What travels over it is the caller's.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
mod unix;

/// Where a child listens, made before it is started; dropped, it removes what the child left there.
pub struct PrivateListen {
    kind: Kind,
}

enum Kind {
    #[cfg(unix)]
    Socket(unix::SocketDirectory),
    #[cfg_attr(
        all(unix, not(test)),
        allow(dead_code, reason = "a Unix host listens on a socket")
    )]
    Loopback { token: String },
}

/// How a [`PrivateListen`] is reached.
pub enum Address<'a> {
    /// A Unix socket at this path, which the child creates.
    #[cfg_attr(windows, allow(dead_code, reason = "Windows listens on loopback"))]
    Socket(&'a Path),
    /// A loopback port the child chooses and reports, guarded by this token.
    #[cfg_attr(
        all(unix, not(test)),
        allow(dead_code, reason = "a Unix host listens on a socket")
    )]
    Loopback { token: &'a str },
}

impl PrivateListen {
    /// The channel the host offers: a socket named `name` in a new private directory whose name
    /// starts with `prefix` on macOS and Linux; a loopback port with a new token on Windows.
    /// Windows has Unix sockets too, but a child such as Codex takes one only in a directory whose
    /// security descriptor admits the user alone, and neither the standard library nor tokio
    /// connects to a Unix socket there.
    pub fn for_host(prefix: &str, name: &str) -> io::Result<Self> {
        #[cfg(unix)]
        {
            unix::SocketDirectory::create(prefix, name).map(|directory| Self {
                kind: Kind::Socket(directory),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = (prefix, name);
            Self::loopback()
        }
    }

    /// A loopback port with a new token, as Windows uses.
    #[cfg_attr(
        all(unix, not(test)),
        allow(dead_code, reason = "a Unix host listens on a socket")
    )]
    pub fn loopback() -> io::Result<Self> {
        let mut secret = [0u8; 32];
        super::random::fill(&mut secret)?;
        let token = secret.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self {
            kind: Kind::Loopback { token },
        })
    }

    pub fn address(&self) -> Address<'_> {
        match &self.kind {
            #[cfg(unix)]
            Kind::Socket(directory) => Address::Socket(directory.socket()),
            Kind::Loopback { token } => Address::Loopback { token },
        }
    }

    /// The private directory a socket is in, for a test of its cleanup.
    #[cfg(all(test, unix))]
    pub fn directory(&self) -> Option<std::path::PathBuf> {
        match &self.kind {
            #[cfg(unix)]
            Kind::Socket(directory) => Some(directory.path().to_path_buf()),
            Kind::Loopback { .. } => None,
        }
    }
}

/// A connection to a child's private channel.
pub enum Connection {
    #[cfg(unix)]
    Socket(std::os::unix::net::UnixStream),
    Loopback(TcpStream),
}

impl Connection {
    /// Connects to the socket at `path`; refused where the host has no Unix sockets.
    pub fn socket(path: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        {
            std::os::unix::net::UnixStream::connect(path).map(Self::Socket)
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this host has no Unix sockets",
            ))
        }
    }

    /// Connects to the loopback `port`, waiting at most `timeout`.
    pub fn loopback(port: u16, timeout: Duration) -> io::Result<Self> {
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&address, timeout).map(Self::Loopback)
    }

    /// Bounds each later read and write by `timeout`.
    pub fn set_timeouts(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Socket(stream) => {
                stream.set_read_timeout(timeout)?;
                stream.set_write_timeout(timeout)
            }
            Self::Loopback(stream) => {
                stream.set_read_timeout(timeout)?;
                stream.set_write_timeout(timeout)
            }
        }
    }
}

impl Read for Connection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Socket(stream) => stream.read(buffer),
            Self::Loopback(stream) => stream.read(buffer),
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Socket(stream) => stream.write(buffer),
            Self::Loopback(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Socket(stream) => stream.flush(),
            Self::Loopback(stream) => stream.flush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loopback_channel_has_a_fresh_token_each_time() {
        let (first, second) = (
            PrivateListen::loopback().unwrap(),
            PrivateListen::loopback().unwrap(),
        );
        let (Address::Loopback { token: a }, Address::Loopback { token: b }) =
            (first.address(), second.address())
        else {
            unreachable!();
        };
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[cfg(windows)]
    #[test]
    fn windows_listens_on_a_loopback_port() {
        let listen = PrivateListen::for_host("octo-test", "rpc").unwrap();
        assert!(matches!(listen.address(), Address::Loopback { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn sockets_are_private_short_and_removed_with_the_owner() {
        use std::os::unix::fs::PermissionsExt;
        let listen = PrivateListen::for_host("octo-test", "rpc").unwrap();
        let Address::Socket(socket) = listen.address() else {
            unreachable!();
        };
        let socket = socket.to_path_buf();
        let path = listen.directory().unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(socket.as_os_str().len() < 104);
        // A child may link the path to its real socket and leave a lock file beside it.
        std::os::unix::fs::symlink("/nonexistent/socket", &socket).unwrap();
        std::fs::write(path.join("rpc.lock"), "").unwrap();
        drop(listen);
        assert!(!path.exists());
    }
}
