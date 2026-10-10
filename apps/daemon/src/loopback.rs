//! A minimal HTTP client for the daemon's own loopback address, used by the two CLI modes that
//! have to call the running daemon from a separate process: the hook forwarder and the MCP server.
//!
//! Hand-rolled rather than taken from a client crate. The traffic only ever goes to `127.0.0.1`,
//! so a TLS stack and URL normalisation would be weight with no purpose, and the IDNA chain a
//! general client pulls in raises the Rust toolchain floor past this project's (see "Rust
//! toolchain floor" under "Known pitfalls of the Tauri / Rust approach" in `docs/architecture.md`).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Posts a body and returns the response body. The whole round trip is bounded by `timeout`,
/// because both callers sit on a path where a wedged daemon would otherwise be charged to the
/// agent's turn.
pub fn post(port: u16, path: &str, body: &[u8], timeout: Duration) -> std::io::Result<Vec<u8>> {
    exchange(connect(port, timeout)?, port, path, body)
}

/// Opens the connection an [`exchange`] goes over, with every read and write on it bounded by
/// `timeout`. Kept apart from the exchange so a caller can keep a clone of the stream and shut it
/// down from elsewhere, which ends a blocked exchange and tells the daemon the call is gone.
pub fn connect(port: u16, timeout: Duration) -> std::io::Result<TcpStream> {
    let address = format!("127.0.0.1:{port}").parse().map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "unusable daemon address")
    })?;
    let stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_write_timeout(Some(timeout))?;
    stream.set_read_timeout(Some(timeout))?;
    Ok(stream)
}

/// Posts a body over a stream from [`connect`] and returns the response body.
pub fn exchange(
    mut stream: TcpStream,
    port: u16,
    path: &str,
    body: &[u8],
) -> std::io::Result<Vec<u8>> {
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;

    // `Connection: close` means end-of-stream is end-of-body, so the response needs no chunked
    // or content-length handling of its own.
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    Ok(split_body(response))
}

/// Drops the status line and headers, leaving the body. A response the daemon did not shape (an
/// axum rejection, say) still parses: anything before the blank line goes.
fn split_body(response: Vec<u8>) -> Vec<u8> {
    const SEPARATOR: &[u8] = b"\r\n\r\n";
    response
        .windows(SEPARATOR.len())
        .position(|window| window == SEPARATOR)
        .map(|at| response[at + SEPARATOR.len()..].to_vec())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_body_is_what_follows_the_headers() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec();
        assert_eq!(super::split_body(response), b"{}".to_vec());
    }

    /// A response with no header terminator is not a response; reading the whole thing as a body
    /// would hand the caller a status line to parse as JSON.
    #[test]
    fn a_truncated_response_yields_nothing() {
        assert!(super::split_body(b"HTTP/1.1 200 OK".to_vec()).is_empty());
    }
}
