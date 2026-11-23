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

/// Accepts the next connection on `listener` that is not the probe: one that begins with it
/// ([`is_port_probe`]), or closes before saying anything, is answered with a plain `404` where it
/// can be and left out. For a stand-in whose client speaks first and that takes only so many
/// connections, which a probe arriving first would otherwise use up. What a connection begins with
/// is read without taking it, so the one returned is whole.
pub(crate) async fn accept_unprobed(
    listener: &tokio::net::TcpListener,
) -> std::io::Result<tokio::net::TcpStream> {
    use tokio::io::AsyncWriteExt;

    loop {
        let (mut stream, _) = listener.accept().await?;
        if !probe_or_silent(&stream).await {
            return Ok(stream);
        }
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .ok();
    }
}

/// Whether the connection `stream` begins with the probe, or closes, or fails, before it says
/// enough to tell.
async fn probe_or_silent(stream: &tokio::net::TcpStream) -> bool {
    // As long as the probe's method and the space after it.
    let mut start = [0u8; 5];
    loop {
        match stream.peek(&mut start).await {
            Ok(0) | Err(_) => return true,
            Ok(read) if read == start.len() => {
                return is_port_probe(std::str::from_utf8(&start).unwrap_or(""))
            }
            // The rest is on its way: what has come is read again with it.
            Ok(_) => tokio::task::yield_now().await,
        }
    }
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
