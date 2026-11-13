use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

pub(super) const AVAILABLE: bool = true;

pub(super) type Listener = tokio::net::UnixListener;
pub type Stream = tokio::net::UnixStream;
pub type BlockingStream = std::os::unix::net::UnixStream;

pub(super) fn bind_replacing_stale(path: &Path) -> io::Result<Listener> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(path)?,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists and is not a socket", path.display()),
            ))
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let listener = Listener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub(super) async fn accept(listener: &Listener) -> io::Result<Stream> {
    listener.accept().await.map(|(stream, _)| stream)
}

pub(super) fn connect(path: &Path, timeout: Duration) -> io::Result<BlockingStream> {
    // A connect to a listener whose backlog is full waits with no bound on Linux, and `std` has no
    // connect with a timeout for these sockets: the connect runs on a thread of its own, left to
    // finish on its own when it outlasts `timeout` (a stream it makes then is dropped, unsent).
    let (sent, received) = std::sync::mpsc::channel();
    let target = path.to_path_buf();
    std::thread::spawn(move || {
        let _ = sent.send(BlockingStream::connect(&target));
    });
    let stream = match received.recv_timeout(timeout) {
        Ok(connected) => connected?,
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "connecting to {} took longer than {timeout:?}",
                    path.display()
                ),
            ))
        }
    };
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    Ok(stream)
}

fn euid() -> u32 {
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

pub(super) fn check_private_tree(dir: &Path) -> io::Result<()> {
    let me = euid();
    let metadata = std::fs::metadata(dir)?;
    if !metadata.is_dir() || metadata.uid() != me || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} is not a directory this user alone can use (mode {:o}, owner {})",
                dir.display(),
                metadata.mode() & 0o7777,
                metadata.uid()
            ),
        ));
    }
    let resolved = std::fs::canonicalize(dir)?;
    for chain in [dir, resolved.as_path()] {
        for ancestor in chain.ancestors().skip(1) {
            check_ancestor(ancestor, me)?;
        }
    }
    Ok(())
}

/// Fails when someone other than the user `me` could rename an entry of `dir` away.
fn check_ancestor(dir: &Path, me: u32) -> io::Result<()> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    let metadata = std::fs::metadata(dir)?;
    let owner_trusted = metadata.uid() == me || metadata.uid() == 0;
    let sticky = metadata.mode() & 0o1000 != 0;
    let others_write = metadata.mode() & 0o022 != 0;
    if !owner_trusted || (others_write && !sticky) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} lets another user replace what is in it (mode {:o}, owner {})",
                dir.display(),
                metadata.mode() & 0o7777,
                metadata.uid()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_api {
    use std::path::Path;

    /// Sets the mode of `path`, for a test that needs a directory or file opened or closed to
    /// others.
    pub(crate) fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    /// The mode bits of `path`.
    pub(crate) fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }
}

#[cfg(test)]
mod tests {
    use super::test_api::{mode_of, set_mode};
    use super::{bind_replacing_stale, check_ancestor, check_private_tree, connect, euid};

    /// A scratch directory of the test's own, removed when it is dropped, a failed test's included.
    fn scratch(label: &str) -> crate::test_support::ScratchDir {
        crate::test_support::ScratchDir::new(&format!("lsock-{label}"))
    }

    /// A directory above the private one that another user could write into lets them rename it
    /// away and put their own in its place, unless it is sticky; one another user owns is refused
    /// too, unless that user is the superuser.
    #[test]
    fn an_ancestor_writable_by_others_is_refused_unless_sticky() {
        let dir = scratch("ancestor");
        for (mode, allowed) in [
            (0o777, false),
            (0o775, false),
            (0o757, false),
            (0o1777, true),
            (0o755, true),
            (0o700, true),
        ] {
            set_mode(&dir, mode);
            assert_eq!(check_ancestor(&dir, euid()).is_ok(), allowed, "{mode:o}");
        }
        set_mode(&dir, 0o755);
        if euid() != 0 {
            assert!(check_ancestor(&dir, euid() + 1).is_err());
        }
    }

    /// The directory itself has to be the user's and closed to everyone else.
    #[test]
    fn a_directory_others_can_enter_is_refused() {
        let dir = scratch("tree");
        let private = dir.join("data");
        std::fs::create_dir(&private).unwrap();
        for mode in [0o750, 0o705, 0o770, 0o711] {
            set_mode(&private, mode);
            assert!(check_private_tree(&private).is_err(), "{mode:o}");
        }
    }

    /// A socket file whose listener is gone is replaced; anything else at the path is refused and
    /// left as it is; the socket is owner-only.
    #[tokio::test]
    async fn a_stale_socket_is_replaced_and_nothing_else_is() {
        let dir = scratch("stale");
        let path = dir.join("b.sock");
        drop(bind_replacing_stale(&path).expect("a first socket"));
        assert!(
            path.exists(),
            "the socket file stays when its listener goes"
        );
        let second = bind_replacing_stale(&path).expect("a stale socket file is replaced");
        assert_eq!(mode_of(&path) & 0o777, 0o600);
        drop(second);

        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "not a socket").unwrap();
        assert!(bind_replacing_stale(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a socket");
    }

    /// A symlink at the socket's path, even one to a live socket, is not a stale socket: it is
    /// refused, and neither it nor what it leads to is touched.
    #[tokio::test]
    async fn a_symlink_at_the_path_is_refused_and_left_alone() {
        let dir = scratch("link");
        let target = dir.join("real.sock");
        let _listening = bind_replacing_stale(&target).expect("a socket to point at");
        let path = dir.join("b.sock");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(bind_replacing_stale(&path).is_err());
        assert!(std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(target.exists());

        let directory = dir.join("d.sock");
        std::fs::create_dir(&directory).unwrap();
        assert!(bind_replacing_stale(&directory).is_err());
        assert!(directory.is_dir());
    }

    /// Nothing connects to a path with no socket or no listener behind it; a listener is reached.
    #[tokio::test]
    async fn a_connection_reaches_a_listener_and_only_a_listener() {
        let dir = scratch("connect");
        let path = dir.join("b.sock");
        let timeout = std::time::Duration::from_secs(30);
        assert!(connect(&path, timeout).is_err(), "no file");
        let listener = bind_replacing_stale(&path).unwrap();
        let accepting = tokio::spawn(async move { super::accept(&listener).await.map(|_| ()) });
        assert!(connect(&path, timeout).is_ok());
        accepting
            .await
            .unwrap()
            .expect("the connection was accepted");
    }

    /// A socket file nothing listens on is refused as it is, and a path with no socket is not
    /// found: the error the connect itself gave comes back, not a timeout.
    #[test]
    fn a_connect_that_fails_reports_why() {
        let dir = scratch("connect-error");
        let path = dir.join("b.sock");
        let timeout = std::time::Duration::from_secs(30);
        let missing = connect(&path, timeout).expect_err("no file");
        assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);

        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        let refused = connect(&path, timeout).expect_err("no listener");
        assert_eq!(refused.kind(), std::io::ErrorKind::ConnectionRefused);
    }

    /// The stream a connect returns carries the timeout on its reads and its writes, so a peer
    /// that never answers is given up on.
    #[test]
    fn the_connected_stream_carries_the_timeout_on_reads_and_writes() {
        let dir = scratch("connect-timeouts");
        let path = dir.join("b.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let timeout = std::time::Duration::from_secs(7);
        let stream = connect(&path, timeout).expect("connected");
        assert_eq!(stream.read_timeout().unwrap(), Some(timeout));
        assert_eq!(stream.write_timeout().unwrap(), Some(timeout));
    }

    /// A connect to a listener whose backlog is full waits with no bound on Linux; the connect
    /// gives up at the timeout with `TimedOut` instead, so a daemon that stopped accepting does not
    /// hold the caller. The timeout is short here since what is tested is that it fires.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_connect_to_a_full_backlog_gives_up_at_the_timeout() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::ffi::OsStrExt;

        /// Connects to `path` without waiting: the descriptor once connected, `WouldBlock` where
        /// the backlog is full.
        fn connect_without_waiting(path: &std::path::Path) -> std::io::Result<OwnedFd> {
            // SAFETY: a new socket, owned from here on.
            let raw = unsafe {
                libc::socket(
                    libc::AF_UNIX,
                    libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                    0,
                )
            };
            if raw == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: `raw` is open and nothing else owns it.
            let owned = unsafe { OwnedFd::from_raw_fd(raw) };
            // SAFETY: a zeroed `sockaddr_un` is a valid value to fill in.
            let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
            address.sun_family = libc::AF_UNIX as libc::sa_family_t;
            let bytes = path.as_os_str().as_bytes();
            assert!(bytes.len() < address.sun_path.len());
            for (slot, byte) in address.sun_path.iter_mut().zip(bytes) {
                *slot = *byte as libc::c_char;
            }
            // SAFETY: `address` is a filled-in `sockaddr_un` of the size given.
            let connected = unsafe {
                libc::connect(
                    owned.as_raw_fd(),
                    (&address as *const libc::sockaddr_un).cast(),
                    std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
                )
            };
            if connected == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(owned)
        }

        let dir = scratch("connect-full");
        let path = dir.join("b.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        // The smallest backlog, which nothing accepts from.
        // SAFETY: only changes the backlog of a listening socket this test owns.
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
        let mut queued = Vec::new();
        loop {
            match connect_without_waiting(&path) {
                Ok(connection) => queued.push(connection),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(err) => panic!("filling the backlog: {err}"),
            }
            assert!(queued.len() < 64, "the backlog never filled");
        }

        let began = std::time::Instant::now();
        let err = connect(&path, std::time::Duration::from_millis(100))
            .expect_err("a connect nobody accepts");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "{err}");
        // Rules out a wait without bound, not a tight elapsed time.
        assert!(began.elapsed() < crate::test_support::PATIENCE);
        // The thread still connecting ends once the listener is gone.
        drop(listener);
    }

    /// Whether a layout of `data` under `ancestor` is judged by `ancestor` alone: the tree is
    /// private when `ancestor` is closed to others, so where it still is refused something above
    /// the test's own directories decides (a shared volume, say) and there is nothing to learn; and
    /// when `ancestor` is writable by everyone it is refused, naming `ancestor`.
    fn ancestor_decides(data: &std::path::Path, ancestor: &std::path::Path) {
        set_mode(ancestor, 0o755);
        if check_private_tree(data).is_err() {
            return;
        }
        set_mode(ancestor, 0o777);
        let refused = check_private_tree(data).expect_err("an ancestor everyone can write to");
        let name = ancestor.file_name().unwrap().to_string_lossy().into_owned();
        assert!(refused.to_string().contains(&name), "{refused}");
    }

    /// A directory above the data directory that others can write to refuses it, whether it is above
    /// it as the path is spelled or only as it resolves (the data directory reached through a link).
    #[test]
    fn an_open_directory_above_the_data_directory_refuses_it() {
        let dir = scratch("above");
        let open = dir.join("open-above");
        let data = open.join("data");
        std::fs::create_dir_all(&data).unwrap();
        set_mode(&data, 0o700);
        ancestor_decides(&data, &open);

        // The link is in a closed directory; the open one is above where it leads.
        let elsewhere = dir.join("open-resolved");
        let real = elsewhere.join("real");
        std::fs::create_dir_all(real.join("data")).unwrap();
        set_mode(&real.join("data"), 0o700);
        set_mode(&real, 0o755);
        let closed = dir.join("closed");
        std::fs::create_dir(&closed).unwrap();
        set_mode(&closed, 0o755);
        std::os::unix::fs::symlink(&real, closed.join("link")).unwrap();
        ancestor_decides(&closed.join("link").join("data"), &elsewhere);

        // Put the modes back so the directory can be removed.
        set_mode(&open, 0o755);
        set_mode(&elsewhere, 0o755);
    }

    /// A path that is not a directory, or is not there, is no private directory.
    #[test]
    fn a_missing_or_plain_file_is_no_private_directory() {
        let dir = scratch("notdir");
        assert!(check_private_tree(&dir.join("missing")).is_err());
        let file = dir.join("file");
        std::fs::write(&file, "").unwrap();
        set_mode(&file, 0o600);
        assert!(check_private_tree(&file).is_err());
    }
}
