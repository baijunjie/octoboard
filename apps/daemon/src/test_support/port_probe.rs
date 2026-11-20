//! The probe a running Octoboard daemon sends to the listening ports of a session's processes, as
//! it reaches a test's loopback stand-in.
//!
//! The daemon asks each port a session's process tree listens on for `HEAD /` while a client lists
//! its processes, to find the ones that serve web pages (`crate::processes::web_ports`). When the
//! tests run inside such a session, the test binary is in that tree, so the probe reaches every
//! stand-in server a test starts, at any moment. A stand-in answers it plainly and leaves it out of
//! what it records, and a stand-in's tests send it one to hold that.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};

use super::PATIENCE;

/// Whether `request_line` is the probe, not a request the test made: no request the daemon's
/// helpers or command line send is a `HEAD`.
pub(crate) fn is_port_probe(request_line: &str) -> bool {
    request_line.starts_with("HEAD ")
}

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
