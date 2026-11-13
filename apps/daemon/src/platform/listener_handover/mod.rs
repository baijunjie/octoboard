//! Handing the daemon's listening TCP socket to the daemon it starts itself at a planned restart, so
//! that the loopback port is never unbound between the two: a port the daemon let go of for a
//! moment could be bound by another user of the host, who would then receive whatever a client sent
//! it in that moment, the launch key the successor keeps among it.
//!
//! On Unix the starting daemon keeps a duplicate of its listener ([`Kept::of`]) for the whole run;
//! the child gets it as descriptor [`HANDED_FD`], which the caller announces on its command line,
//! and adopts it ([`adopt`]) in place of binding. Connections that arrive in between wait in the
//! socket's backlog for the successor to accept them. Windows hands nothing over: [`Kept::of`]
//! keeps nothing there and the successor binds the port as it always has.

use std::io;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
use unix as imp;

#[cfg(windows)]
mod unsupported;

#[cfg(windows)]
use unsupported as imp;

/// The descriptor number the listener is handed over as, the first after the standard three.
pub const HANDED_FD: i32 = 3;

/// A duplicate of the daemon's listening socket, kept to hand to a successor. It holds the port
/// bound for as long as it lives, whether or not the listener itself is still served: dropped, it
/// lets the port go, as the listener's own end does.
pub struct Kept(pub(super) imp::Kept);

impl Kept {
    /// A duplicate of `listener`, not inherited by any process the daemon starts unless handed over
    /// explicitly; `None` where the host hands nothing over or the duplicate cannot be made.
    pub fn of(listener: &tokio::net::TcpListener) -> Option<Self> {
        imp::keep(listener).map(Self)
    }

    /// A listener of its own on the same socket, for the daemon to accept on while it keeps this
    /// one, as it does to turn away what arrives while it stops (see `main.rs`).
    pub fn listener(&self) -> io::Result<std::net::TcpListener> {
        imp::listener_of(&self.0)
    }
}

/// The listener handed over as descriptor `fd` (which must be [`HANDED_FD`]), once it is found to
/// be a TCP socket listening on `127.0.0.1:port`. Only then is the descriptor taken over: one
/// refused is left as it is, as it may not be this process's to close. Made close-on-exec, so that nothing this daemon starts inherits it. Called before anything
/// else in the process opens a descriptor, so that a number left free by a predecessor that handed
/// nothing over is still free.
pub fn adopt(fd: i32, port: u16) -> io::Result<std::net::TcpListener> {
    if fd != HANDED_FD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a listener is handed over as descriptor {HANDED_FD}, not {fd}"),
        ));
    }
    imp::adopt(fd, port)
}

#[cfg(all(unix, test))]
pub(super) use imp::is_listening;
#[cfg(unix)]
pub(super) use imp::{hand_over_in_child, raw};

#[cfg(test)]
mod tests {
    use super::*;

    /// Only descriptor [`HANDED_FD`] is taken over: any other number is refused before the
    /// descriptor is looked at, so one the process owns for another purpose is left open.
    #[test]
    fn only_the_announced_descriptor_is_adopted() {
        for fd in [-1, 0, 1, 2, HANDED_FD + 1, 100] {
            let refused = adopt(fd, 7531).expect_err("another descriptor");
            assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{fd}");
        }
    }

    /// A host that hands nothing over keeps nothing and takes nothing.
    #[cfg(windows)]
    #[tokio::test]
    async fn nothing_is_kept_or_adopted_where_nothing_is_handed_over() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        assert!(Kept::of(&listener).is_none());
        let port = listener.local_addr().unwrap().port();
        let refused = adopt(HANDED_FD, port).expect_err("nothing to take over");
        assert_eq!(refused.kind(), io::ErrorKind::Unsupported);
    }
}
