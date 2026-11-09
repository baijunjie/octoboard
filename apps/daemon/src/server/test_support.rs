//! Fixtures the tests of the server's routes share: the real router on a loopback port, and raw
//! HTTP and WebSocket clients that read what it answers.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use super::router;
use crate::test_support::{app_state, PATIENCE, TEST_KEY};

/// Serves the real router on an ephemeral loopback port and returns the port.
pub(super) async fn serve(name: &str) -> u16 {
    serve_with_output_dir(name).await.0
}

/// `serve`, also handing back the directory the daemon keeps saved session output in.
pub(super) async fn serve_with_output_dir(name: &str) -> (u16, std::path::PathBuf) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let port = listener.local_addr().expect("a bound address").port();
    let (state, dir) = app_state(&format!("server-{name}"));
    let output_dir = dir.join("output");
    tokio::spawn(async move {
        let _dir = dir;
        axum::serve(listener, router(state)).await
    });
    (port, output_dir)
}

/// Opens a WebSocket on `path` and reads every frame the daemon sends until its close, as
/// `(opcode, payload)`. Frames from a server are never masked.
pub(super) fn websocket_frames(port: u16, path: &str) -> Vec<(u8, Vec<u8>)> {
    use std::io::Read;

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    write!(
        stream,
        "GET {path}?key={TEST_KEY} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: Upgrade\r\n\
         Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    )
    .expect("write");
    stream
        .set_read_timeout(Some(PATIENCE))
        .expect("a read timeout");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read");
    assert!(line.contains(" 101 "), "{line}");
    while line != "\r\n" {
        line.clear();
        reader.read_line(&mut line).expect("read");
    }
    let mut frames = Vec::new();
    loop {
        let mut head = [0u8; 2];
        reader.read_exact(&mut head).expect("a frame");
        let len = match head[1] & 0x7f {
            126 => {
                let mut len = [0u8; 2];
                reader.read_exact(&mut len).expect("a length");
                u16::from_be_bytes(len) as usize
            }
            127 => {
                let mut len = [0u8; 8];
                reader.read_exact(&mut len).expect("a length");
                u64::from_be_bytes(len) as usize
            }
            len => len as usize,
        };
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).expect("a payload");
        let opcode = head[0] & 0x0f;
        frames.push((opcode, payload));
        if opcode == 0x8 {
            return frames;
        }
    }
}

/// `request_line` with the daemon's key on its path.
pub(super) fn keyed(request_line: &str) -> String {
    format!("{request_line}?key={TEST_KEY}")
}

/// Writes one request with exactly the headers given (plus a body length), with a header telling
/// the server to close the connection after its response when `close`, and returns the connection
/// to read the response from.
fn send_request(port: u16, request_line: &str, headers: &[&str], close: bool) -> TcpStream {
    let mut request = format!("{request_line} HTTP/1.1\r\n");
    if close {
        request.push_str("Connection: close\r\n");
    }
    for header in headers {
        request.push_str(header);
        request.push_str("\r\n");
    }
    request.push_str("Content-Length: 2\r\n\r\n{}");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(PATIENCE))
        .expect("a read timeout");
    stream.write_all(request.as_bytes()).expect("write");
    stream
}

/// The status code of the status line `line`.
fn status_in(line: &str) -> u16 {
    line.split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status line")
}

/// Sends one request with exactly the headers given (plus a body length) and returns the
/// response's status code, read from the status line alone so an upgraded connection does not
/// have to close first.
pub(super) fn status_of(port: u16, request_line: &str, headers: &[&str]) -> u16 {
    let stream = send_request(port, request_line, headers, false);
    let mut status_line = String::new();
    BufReader::new(stream)
        .read_line(&mut status_line)
        .expect("read");
    status_in(&status_line)
}

/// Sends one request as [`status_of`] does and returns the response's status code and body, read
/// to the end of a response that closes its connection or states its length.
pub(super) fn response_of(port: u16, request_line: &str, headers: &[&str]) -> (u16, String) {
    use std::io::Read;

    let mut stream = send_request(port, request_line, headers, true);
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .expect("a response with a head");
    (status_in(head), body.to_string())
}
