//! The probe a running Octoboard daemon sends to the listening ports of a session's processes, as
//! it reaches a test's loopback stand-in.
//!
//! The daemon asks each port a session's process tree listens on for `HEAD /` while a client lists
//! its processes, to find the ones that serve web pages (`crate::processes::web_ports`). When the
//! tests run inside such a session, the test binary is in that tree, so the probe reaches every
//! stand-in server a test starts, at any moment. A stand-in answers it plainly and leaves it out of
//! what it records, and a stand-in's tests send it one to hold that. One that takes a single
//! connection, or a counted number, takes them past the probe ([`accept_past_probes`]); a test of
//! a listener that is itself under test takes its own client's connection ([`accept_from`]); and
//! one asserting that nothing connected asserts that nothing but the probe did
//! ([`assert_only_probes_connected`]).

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};

use super::PATIENCE;

/// Whether `request_line` is the probe, not a request the test made: no request the daemon's
/// helpers or command line send is a `HEAD`.
pub(crate) fn is_port_probe(request_line: &str) -> bool {
    request_line.as_bytes().starts_with(PROBE_START)
}

/// What the probe's request opens with.
const PROBE_START: &[u8; 5] = b"HEAD ";

/// Sends the probe to the loopback `port`, and gives the status line of its answer.
pub(crate) fn probe_port(port: u16) -> String {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, PATIENCE).expect("the port");
    stream.set_read_timeout(Some(PATIENCE)).unwrap();
    stream.set_write_timeout(Some(PATIENCE)).unwrap();
    stream
        .write_all(b"HEAD / HTTP/1.1\r\nHost: localhost\r\nUser-Agent: Octoboard\r\n\r\n")
        .unwrap();
    let mut status = String::new();
    BufReader::new(stream)
        .read_line(&mut status)
        .expect("an answer to the probe");
    status.trim_end().to_owned()
}

/// The answer a stand-in gives the probe: a plain refusal, after which the connection is closed.
const PROBE_ANSWER: &[u8] =
    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// The next connection on `listener` that is not the probe's, for a stand-in that takes one
/// connection, or a counted number, as the test's own: a probe that connects first is answered
/// and left out. A connection is told by what it opens with, so this is for a client that speaks
/// first (an HTTP request, a WebSocket handshake); one that says nothing is taken for the test's
/// own once [`PATIENCE`] has passed. The connection comes back blocking, with no read timeout.
pub(crate) fn accept_past_probes(listener: &std::net::TcpListener) -> TcpStream {
    loop {
        let (stream, _) = listener.accept().expect("a connection");
        if !answered_as_probe(&stream) {
            return stream;
        }
    }
}

/// Whether `stream`, a connection just accepted, opened with the probe's request, in which case
/// it has been answered; otherwise nothing of it has been read, and the connection is left
/// blocking, with no read timeout.
pub(crate) fn answered_as_probe(stream: &TcpStream) -> bool {
    // A connection accepted from a non-blocking listener can be non-blocking itself.
    stream.set_nonblocking(false).unwrap();
    stream.set_read_timeout(Some(PATIENCE)).unwrap();
    let deadline = std::time::Instant::now() + PATIENCE;
    let mut start = [0u8; PROBE_START.len()];
    let probe = loop {
        match stream.peek(&mut start) {
            // Closed, or silent for the whole of the patience.
            Ok(0) | Err(_) => break false,
            Ok(read) if start[..read] != PROBE_START[..read] => break false,
            Ok(read) if read == start.len() => break true,
            // What has arrived begins as the probe's request does and stops short of telling:
            // its first five bytes came split across segments. Nothing waits for "more than
            // these bytes" on every host, so the peek is repeated until the rest is in, which on
            // loopback is at once.
            Ok(_) if std::time::Instant::now() < deadline => std::thread::yield_now(),
            Ok(_) => break false,
        }
    };
    if probe {
        // The rest of the request is read before the answer: a socket closed with bytes it has
        // not read is reset on Windows, and the client then loses the answer.
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        while reader.read_line(&mut line).is_ok_and(|read| read > 0) && line != "\r\n" {
            line.clear();
        }
        let _ = (&*stream).write_all(PROBE_ANSWER);
    } else {
        // Only here: a probe's prober may have closed by now, and macOS refuses a socket option
        // on a connection its peer has reset.
        stream.set_read_timeout(None).unwrap();
    }
    probe
}

/// [`accept_past_probes`] for a listener on the runtime. Each connection is told on a blocking
/// worker, on the real clock, so this is as good on tokio's paused clock, which a blocking task
/// holds back, as on a running one.
pub(crate) async fn accepted_past_probes(
    listener: &tokio::net::TcpListener,
) -> tokio::net::TcpStream {
    loop {
        let (stream, _) = listener.accept().await.expect("a connection");
        let stream = stream.into_std().expect("the connection, off the runtime");
        let own = tokio::task::spawn_blocking(move || {
            (!answered_as_probe(&stream)).then(|| {
                stream.set_nonblocking(true).unwrap();
                stream
            })
        })
        .await
        .expect("telling a probe from the test's own connection");
        if let Some(own) = own {
            return tokio::net::TcpStream::from_std(own).expect("the connection, on the runtime");
        }
    }
}

/// The connection `client` made (its local address), taken from `listener`, for a test of a
/// listener that is itself under test and whose client says nothing first: any other connection
/// accepted before it, the probe's, is dropped. Panics when the client's connection has not come
/// within [`PATIENCE`]. Whether the listener blocks does not matter, and is left as it was.
#[cfg(unix)]
pub(crate) fn accept_from(listener: &std::net::TcpListener, client: SocketAddr) -> TcpStream {
    use std::os::fd::AsRawFd;

    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let mut waiting = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: polls the one descriptor of the listener this call borrows.
        let ready = unsafe { libc::poll(&mut waiting, 1, left.as_millis() as libc::c_int) };
        if ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        assert!(ready > 0, "the client's connection never came");
        match listener.accept() {
            Ok((stream, peer)) if peer == client => return stream,
            Ok(_) => {}
            // Gone again between the wait and the accept.
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) => panic!("accepting the client's connection failed: {err}"),
        }
    }
}

/// [`accept_from`] for a listener on the runtime. Real clock only: the wait is bounded on tokio's
/// clock, so on a paused one it panics as soon as the runtime is idle, connection or not.
#[cfg(unix)]
pub(crate) async fn accepted_from(
    listener: &tokio::net::TcpListener,
    client: SocketAddr,
) -> tokio::net::TcpStream {
    let own = async {
        loop {
            let (stream, peer) = listener.accept().await.expect("the client's connection");
            if peer == client {
                return stream;
            }
        }
    };
    tokio::time::timeout(PATIENCE, own)
        .await
        .expect("the client's connection never came")
}

/// Asserts that nothing but the probe has connected to the non-blocking `listener` so far: each
/// connection waiting on it is the probe's, and is answered.
pub(crate) fn assert_only_probes_connected(listener: &std::net::TcpListener) {
    loop {
        match listener.accept() {
            Ok((stream, peer)) => assert!(
                answered_as_probe(&stream),
                "something other than a port probe connected, from {peer}"
            ),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(err) => panic!("accepting failed: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe that connects before the test's own client is answered and passed over, by a
    /// blocking stand-in and by one on the runtime, and the test's own connection is handed over
    /// with nothing of it read.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_probe_ahead_of_the_tests_own_connection_is_answered_and_passed_over() {
        use std::io::Read;
        use tokio::io::AsyncReadExt;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let own = std::thread::spawn(move || {
            assert_eq!(probe_port(port), "HTTP/1.1 404 Not Found");
            let mut own = TcpStream::connect(("127.0.0.1", port)).unwrap();
            own.write_all(b"POST / HTTP/1.1\r\n").unwrap();
        });
        let mut taken = accept_past_probes(&listener);
        let mut said = String::new();
        taken.read_to_string(&mut said).unwrap();
        assert_eq!(said, "POST / HTTP/1.1\r\n");
        own.join().unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let own = tokio::task::spawn_blocking(move || {
            assert_eq!(probe_port(port), "HTTP/1.1 404 Not Found");
            let mut own = TcpStream::connect(("127.0.0.1", port)).unwrap();
            own.write_all(b"POST / HTTP/1.1\r\n").unwrap();
        });
        let mut taken = accepted_past_probes(&listener).await;
        let mut said = String::new();
        taken.read_to_string(&mut said).await.unwrap();
        assert_eq!(said, "POST / HTTP/1.1\r\n");
        own.await.unwrap();
    }

    /// The client's own connection is told from a probe ahead of it by its address.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_probe_is_told_from_the_tests_own_connection_by_its_address() {
        use std::io::Read;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let probe = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let taken = accept_from(&listener, client.local_addr().unwrap());
        // The probe's connection was passed over: it is the one closed, and the client's is not.
        drop(taken);
        probe.set_read_timeout(Some(PATIENCE)).unwrap();
        assert_eq!((&probe).read(&mut [0u8; 1]).unwrap(), 0);
        drop(probe);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let probe = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let taken = accepted_from(&listener, client.local_addr().unwrap()).await;
        drop(taken);
        probe.set_read_timeout(Some(PATIENCE)).unwrap();
        assert_eq!((&probe).read(&mut [0u8; 1]).unwrap(), 0);
        drop(probe);
    }

    /// On tokio's paused clock a probe is still told from the test's own connection: telling
    /// them apart waits on no timer of the runtime's.
    #[tokio::test(start_paused = true)]
    async fn a_probe_is_passed_over_on_a_paused_clock_too() {
        use tokio::io::AsyncReadExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let own = tokio::task::spawn_blocking(move || {
            assert_eq!(probe_port(port), "HTTP/1.1 404 Not Found");
            let mut own = TcpStream::connect(("127.0.0.1", port)).unwrap();
            own.write_all(b"POST / HTTP/1.1\r\n").unwrap();
        });
        let mut taken = accepted_past_probes(&listener).await;
        let mut said = String::new();
        taken.read_to_string(&mut said).await.unwrap();
        assert_eq!(said, "POST / HTTP/1.1\r\n");
        own.await.unwrap();
    }

    /// A connection that opens with fewer bytes than the probe's request has, or with other
    /// bytes, is the test's own at once, with nothing of it read.
    #[test]
    fn a_connection_that_does_not_open_as_the_probe_is_the_tests_own_at_once() {
        use std::io::Read;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        for opening in [&b"hi"[..], b"HELLO", b"GET / HTTP/1.1\r\n"] {
            // Kept open, so that being told apart cannot rest on the connection closing.
            let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
            client.write_all(opening).unwrap();
            let mut taken = accept_past_probes(&listener);
            let mut said = vec![0u8; opening.len()];
            taken.read_exact(&mut said).unwrap();
            assert_eq!(said, opening);
        }
    }

    /// A client that never connects fails the wait for it instead of hanging the test: shown
    /// on the runtime's clock, paused, where the patience costs no real time.
    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "the client's connection never came")]
    async fn a_client_that_never_connects_fails_the_wait_for_it() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let nobody = SocketAddr::from((Ipv4Addr::LOCALHOST, 1));
        accepted_from(&listener, nobody).await;
    }

    /// A probe waiting on a listener is not "something connected".
    #[test]
    fn a_probe_waiting_on_a_listener_is_not_something_connected() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        assert_only_probes_connected(&listener);
        let probe = std::thread::spawn(move || probe_port(port));
        // The probe's answer is what says it has connected: until then nothing has.
        while !probe.is_finished() {
            assert_only_probes_connected(&listener);
            std::thread::yield_now();
        }
        assert_eq!(probe.join().unwrap(), "HTTP/1.1 404 Not Found");
    }
}
