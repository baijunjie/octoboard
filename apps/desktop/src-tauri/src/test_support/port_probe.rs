//! The probe a running Octoboard daemon sends to the listening ports of a session's processes, as
//! it reaches a loopback listener of a test. The daemon crate has the same helper; it is repeated
//! here because the two crates share no test-support crate.
//!
//! The daemon asks each port a session's process tree listens on for `HEAD /` while a client lists
//! its processes, to find the ones that serve web pages. When the tests run inside such a session,
//! the test binary is in that tree, so the probe reaches every listener a test opens, at any
//! moment: a stand-in's, and the ports the code under test listens on itself (a remote window's, a
//! forwarded port's). A stand-in answers it plainly and leaves it out of what it records; a test of
//! a port the code under test listens on asserts only what its own connections got, since that
//! port carries a stranger's connection like any other. Either sends the probe itself to hold that.
//!
//! Unlike the daemon's stand-ins, not every listener here answers: a port whose connection is
//! carried elsewhere may close it with nothing sent back, so no answer is an empty line here.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};

use super::PATIENCE;

/// What the probe sends.
const PROBE: &[u8] = b"HEAD / HTTP/1.1\r\nHost: localhost\r\nUser-Agent: Octoboard\r\n\r\n";

/// Whether `request_line` is the probe, not a request the test made: nothing the shell sends is a
/// `HEAD`.
pub(crate) fn is_port_probe(request_line: &str) -> bool {
    request_line.starts_with("HEAD ")
}

/// Sends the probe to the loopback `port`, and gives the first line of what comes back: the status
/// line of an answer, and nothing where the connection is closed unanswered. Blocks, so a test on
/// an async runtime that also runs the listener uses [`port_probed`].
pub(crate) fn probe_port(port: u16) -> String {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, PATIENCE).expect("the port");
    stream.set_read_timeout(Some(PATIENCE)).unwrap();
    stream.set_write_timeout(Some(PATIENCE)).unwrap();
    stream.write_all(PROBE).unwrap();
    let mut status = String::new();
    // A listener that closes the connection without reading the probe resets it.
    let _ = BufReader::new(stream).read_line(&mut status);
    status.trim_end().to_owned()
}

/// [`probe_port`] from a test on an async runtime, which goes on running the listener meanwhile.
pub(crate) async fn port_probed(port: u16) -> String {
    tokio::task::spawn_blocking(move || probe_port(port))
        .await
        .expect("the probe ran")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn the_probe_is_a_head_request_and_gives_the_status_line_of_its_answer() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (told, requests) = std::sync::mpsc::channel();
        // Every connection is answered alike, the real probe's too, should one come.
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while !request.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(read) if read > 0 => request.extend_from_slice(&buffer[..read]),
                        _ => break,
                    }
                }
                if told.send(request).is_err() {
                    return;
                }
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
            }
        });
        assert_eq!(probe_port(port), "HTTP/1.1 204 No Content");
        assert!(requests.try_iter().any(|request| request == PROBE));
        assert!(is_port_probe(&String::from_utf8_lossy(PROBE)));
        assert!(!is_port_probe("POST /shutdown?key=ab12 HTTP/1.1"));
    }

    #[test]
    fn a_connection_closed_unanswered_is_an_empty_line() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                drop(stream);
            }
        });
        assert_eq!(probe_port(port), "");
    }
}
