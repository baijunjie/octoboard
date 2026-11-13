use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd, RawFd};

pub(crate) struct Kept(OwnedFd);

pub(super) fn keep(listener: &tokio::net::TcpListener) -> Option<Kept> {
    // `try_clone_to_owned` duplicates with `F_DUPFD_CLOEXEC`: no child inherits it by accident.
    match listener.as_fd().try_clone_to_owned() {
        Ok(fd) => Some(Kept(fd)),
        Err(err) => {
            tracing::warn!(%err, "keeping the listener to hand to a successor failed; a planned restart will bind the port again");
            None
        }
    }
}

/// The descriptor `kept` holds, for [`hand_over_in_child`]; valid while `kept` lives.
pub(in crate::platform) fn raw(kept: &super::Kept) -> RawFd {
    kept.0 .0.as_raw_fd()
}

/// Puts the descriptor `fd` at `target` in a child between `fork` and `exec`, open across the
/// `exec`. Async-signal-safe: system calls only.
pub(in crate::platform) fn hand_over_in_child(fd: RawFd, target: RawFd) -> io::Result<()> {
    if fd == target {
        // `dup2` onto itself changes nothing, so the close-on-exec flag is cleared by hand.
        // SAFETY: `fd` is open in this child, and only its descriptor flags change.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, 0) } == -1 {
            return Err(io::Error::last_os_error());
        }
        return Ok(());
    }
    // SAFETY: `fd` is open in this child; `target` is replaced, and the new descriptor `dup2` makes
    // is not close-on-exec.
    if unsafe { libc::dup2(fd, target) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn adopt(fd: RawFd, port: u16) -> io::Result<std::net::TcpListener> {
    // Checked before the descriptor is taken over, so that one refused is left as it is: it may be
    // something of this process's own when nothing was handed over at that number.
    if !is_listening(fd)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the descriptor handed over is not a listening socket",
        ));
    }
    let expected = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    // SAFETY: `fd` is an open socket; wrapped without being owned, so nothing is closed here.
    let local = std::mem::ManuallyDrop::new(unsafe { std::net::TcpListener::from_raw_fd(fd) })
        .local_addr()?;
    if local != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the listener handed over is on {local}, not {expected}"),
        ));
    }
    // SAFETY: the descriptor is a listener on the announced address, which the predecessor handed
    // over for this process to take; from here it is owned.
    let listener = std::net::TcpListener::from(unsafe { OwnedFd::from_raw_fd(fd) });
    // SAFETY: only the descriptor's own flags change.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// A listener on the socket `kept` holds, served by its own descriptor, which the caller may
/// accept on while it keeps `kept`.
pub(super) fn listener_of(kept: &Kept) -> io::Result<std::net::TcpListener> {
    Ok(std::net::TcpListener::from(kept.0.try_clone()?))
}

/// Whether `fd` is a socket that listens (`SO_ACCEPTCONN`); an error for a descriptor that is not
/// open or not a socket. macOS does not answer `SO_ACCEPTCONN`, so there the socket's TCP state is
/// read instead.
#[cfg(target_os = "macos")]
pub(in crate::platform) fn is_listening(fd: RawFd) -> io::Result<bool> {
    crate::platform::proc_table::own_socket_listens(fd)
}

/// Whether `fd` is a socket that listens (`SO_ACCEPTCONN`); an error for a descriptor that is not
/// open or not a socket.
#[cfg(not(target_os = "macos"))]
pub(in crate::platform) fn is_listening(fd: RawFd) -> io::Result<bool> {
    let mut value: libc::c_int = 0;
    let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `value` and `length` outlive the call, which writes at most `length` bytes.
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            (&mut value as *mut libc::c_int).cast(),
            &mut length,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(value != 0)
}

#[cfg(test)]
mod tests {
    use std::os::fd::{AsRawFd, IntoRawFd};

    use super::*;

    /// A listener on loopback, its duplicate taken as the daemon takes it.
    async fn listener_and_kept() -> (tokio::net::TcpListener, super::super::Kept) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let kept = super::super::Kept::of(&listener).expect("a duplicate");
        (listener, kept)
    }

    /// Whether `port`, let go by this test, can be bound again within `PATIENCE`: a child another
    /// test spawns shares this process's sockets until it execs, so a listener closed here can
    /// hold the port for a moment longer under the suite's load.
    fn freed(port: u16) -> bool {
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while std::net::TcpListener::bind(("127.0.0.1", port)).is_err() {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        true
    }

    /// The duplicate holds the port bound and listening once the listener itself is gone, so a
    /// connection made in between waits in the backlog and is accepted from the duplicate.
    #[tokio::test]
    async fn the_kept_duplicate_holds_the_port_after_the_listener_is_gone() {
        let (listener, kept) = listener_and_kept().await;
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
            "nobody else can bind the port"
        );
        let client =
            std::net::TcpStream::connect(("127.0.0.1", port)).expect("queued, not refused");
        let raw = kept.0 .0.try_clone().unwrap().into_raw_fd();
        // SAFETY: `raw` is a fresh duplicate this test owns.
        let adopted = std::net::TcpListener::from(unsafe { OwnedFd::from_raw_fd(raw) });
        // The duplicate shares the runtime's non-blocking mode; waiting here is what shows the
        // connection was queued rather than refused, however late the handshake completes.
        adopted.set_nonblocking(false).unwrap();
        let (accepted, _) = adopted.accept().expect("the queued connection");
        assert_eq!(accepted.peer_addr().unwrap(), client.local_addr().unwrap());
        drop((adopted, kept));
        assert!(freed(port), "the port is free again");
    }

    /// The duplicate is close-on-exec, so nothing the daemon starts inherits it by accident.
    #[tokio::test]
    async fn the_kept_duplicate_is_close_on_exec() {
        let (_listener, kept) = listener_and_kept().await;
        // SAFETY: only reads the flags of a descriptor this test holds.
        let flags = unsafe { libc::fcntl(kept.0 .0.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
    }

    /// A listener on the announced port is adopted, made close-on-exec and non-blocking; one on
    /// another port, a socket that does not listen, or a closed descriptor is refused, and the
    /// descriptor is closed.
    #[tokio::test]
    async fn only_a_listener_on_the_port_is_adopted() {
        let (listener, kept) = listener_and_kept().await;
        let port = listener.local_addr().unwrap().port();
        let raw = kept.0 .0.try_clone().unwrap().into_raw_fd();
        let adopted = adopt(raw, port).expect("adopted");
        // SAFETY: only reads the flags of the descriptor the listener holds.
        let flags = unsafe { libc::fcntl(adopted.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        assert_eq!(
            adopted.accept().err().map(|err| err.kind()),
            Some(io::ErrorKind::WouldBlock),
            "non-blocking"
        );

        // A refused descriptor is left open, as it is not taken over: each is checked open, then
        // closed by the test.
        let refused_and_left_open = |raw: RawFd| {
            // SAFETY: only reads the flags of a descriptor this test holds, then closes it.
            unsafe {
                assert_ne!(libc::fcntl(raw, libc::F_GETFD), -1, "left open");
                libc::close(raw);
            }
        };
        let raw = kept.0 .0.try_clone().unwrap().into_raw_fd();
        assert!(adopt(raw, port.wrapping_add(1).max(1)).is_err());
        refused_and_left_open(raw);

        let stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let raw = stream.into_raw_fd();
        assert!(
            adopt(raw, port).is_err(),
            "a connected socket does not listen"
        );
        refused_and_left_open(raw);

        let file = std::fs::File::open("/dev/null").unwrap().into_raw_fd();
        assert!(adopt(file, port).is_err(), "not a socket");
        refused_and_left_open(file);
    }

    /// Only the loopback address the daemon binds is adopted: a listener on every interface, or on
    /// the IPv6 loopback, is not the one the port was announced for even when its port matches.
    #[test]
    fn a_listener_on_another_address_is_not_adopted() {
        let everywhere = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = everywhere.local_addr().unwrap().port();
        let raw = everywhere.try_clone().unwrap().into_raw_fd();
        let refused = adopt(raw, port).expect_err("a listener on every interface");
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
        // SAFETY: a refused descriptor is left open and is this test's own; closed once.
        unsafe { libc::close(raw) };

        // A host without IPv6 loopback has nothing more to check.
        if let Ok(v6) = std::net::TcpListener::bind("[::1]:0") {
            let port = v6.local_addr().unwrap().port();
            let raw = v6.try_clone().unwrap().into_raw_fd();
            assert!(adopt(raw, port).is_err(), "a listener on ::1");
            // SAFETY: as above.
            unsafe { libc::close(raw) };
        }
    }

    /// The adopted listener serves through the runtime as the original did: it accepts the
    /// connection a client makes once the original is gone.
    #[tokio::test]
    async fn the_adopted_listener_accepts_what_the_original_would_have() {
        let (listener, kept) = listener_and_kept().await;
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let raw = kept.0 .0.try_clone().unwrap().into_raw_fd();
        let adopted = tokio::net::TcpListener::from_std(adopt(raw, port).expect("adopted"))
            .expect("registered with the runtime");
        let client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connected");
        let (accepted, _) = adopted.accept().await.expect("accepted");
        assert_eq!(accepted.peer_addr().unwrap(), client.local_addr().unwrap());
    }

    /// A listener of its own on the kept socket accepts on the kept port, and is close-on-exec
    /// like the kept duplicate.
    #[tokio::test]
    async fn a_listener_of_its_own_accepts_on_the_kept_port() {
        let (listener, kept) = listener_and_kept().await;
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let own = kept.listener().expect("a listener of its own");
        assert_eq!(own.local_addr().unwrap().port(), port);
        // SAFETY: only reads the flags of a descriptor this test holds.
        let flags = unsafe { libc::fcntl(own.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        let client = std::net::TcpStream::connect(("127.0.0.1", port)).expect("queued");
        // Shares the runtime listener's non-blocking mode; wait for the handshake to complete.
        own.set_nonblocking(false).unwrap();
        let (accepted, _) = own.accept().expect("accepted on the listener of its own");
        assert_eq!(accepted.peer_addr().unwrap(), client.local_addr().unwrap());
    }

    /// Dropping the listener of its own does not let the port go: the kept duplicate holds it,
    /// and a connection is still queued for whoever takes the kept socket next; only dropping the
    /// kept duplicate lets it go.
    #[tokio::test]
    async fn dropping_the_listener_of_its_own_leaves_the_port_held_by_the_kept() {
        let (listener, kept) = listener_and_kept().await;
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        drop(kept.listener().expect("a listener of its own"));
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
            "the port is still held"
        );
        std::net::TcpStream::connect(("127.0.0.1", port)).expect("still queued, not refused");
        drop(kept);
        assert!(
            freed(port),
            "the port is free once the kept duplicate is gone"
        );
    }

    /// What the daemon does to a connection that arrives while it holds the port for its
    /// successor: accepted on the listener of its own and closed with a zero linger, which the
    /// client reads as a reset, not as an orderly end or a wait.
    #[tokio::test]
    async fn a_connection_closed_with_zero_linger_reads_as_a_reset() {
        use tokio::io::AsyncReadExt;

        let (listener, kept) = listener_and_kept().await;
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let own = kept.listener().expect("a listener of its own");
        own.set_nonblocking(true).unwrap();
        let own = tokio::net::TcpListener::from_std(own).unwrap();
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connected");
        let (connection, _) = own.accept().await.expect("accepted");
        connection.set_zero_linger().expect("zero linger");
        drop(connection);
        let mut buffer = [0u8; 16];
        let read = tokio::time::timeout(crate::test_support::PATIENCE, client.read(&mut buffer))
            .await
            .expect("the client was told within the patience");
        assert_eq!(
            read.expect_err("a reset, not an orderly end").kind(),
            io::ErrorKind::ConnectionReset
        );
    }
}
