//! A minimal HTTP client for the daemon's own loopback address, used by the CLI modes that have to
//! call the running daemon from a separate process.
//!
//! Hand-rolled rather than taken from a client crate. The traffic only ever goes to `127.0.0.1`,
//! so a TLS stack and URL normalisation would be weight with no purpose.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Posts a body and returns the response body, or an error when the daemon answered with an error
/// status (see [`error_status`]). Each step of the round trip is bounded by `timeout`, because
/// every caller runs on a path where a wedged daemon would otherwise hold something up: the
/// agent's turn, a git command, or the status line. `key` is the daemon's key (see
/// `crate::server::access`), sent as the path's `key` query parameter.
pub fn post(
    port: u16,
    key: &str,
    path: &str,
    body: &[u8],
    timeout: Duration,
) -> std::io::Result<Vec<u8>> {
    exchange(&connect(port, timeout)?, port, key, path, body)
}

/// Opens the connection an [`exchange`] goes over, with every read and write on it bounded by
/// `timeout`. Kept apart from the exchange so a caller can share the stream with it and shut it
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

/// Posts a body over a stream from [`connect`] and returns the response body, or an error when the
/// daemon answered with an error status (see [`error_status`]). `path` has no query of its own: the
/// key is appended as one.
pub fn exchange(
    mut stream: &TcpStream,
    port: u16,
    key: &str,
    path: &str,
    body: &[u8],
) -> std::io::Result<Vec<u8>> {
    let request = format!(
        "POST {path}?key={key} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
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
    split_body(response)
}

/// The status a holder answers a helper's question with when no daemon linked within its deadline.
/// What a helper does then is what it does when it finds no daemon to connect to.
pub const NO_DAEMON: u16 = 503;

/// The status a holder answers with when the daemon it had put the question to was lost before it
/// answered, so that the call may or may not have run.
pub const DAEMON_LOST: u16 = 502;

/// Whether `err` is a holder saying there is no daemon (see [`NO_DAEMON`]).
pub fn is_no_daemon(err: &std::io::Error) -> bool {
    error_status(err) == Some(NO_DAEMON)
}

/// Whether `err` is a holder saying it lost the daemon with the call in flight (see
/// [`DAEMON_LOST`]).
pub fn is_daemon_lost(err: &std::io::Error) -> bool {
    error_status(err) == Some(DAEMON_LOST)
}

/// A response that was not a success: the daemon turned the call away (a wrong key or a foreign
/// origin, 401 or 403), could not read it (400) or failed while handling it (5xx), and what follows
/// is no answer to parse.
#[derive(Debug)]
struct ErrorStatus {
    status: u16,
}

impl std::fmt::Display for ErrorStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the daemon {} the call (HTTP {})",
            status_verb(self.status),
            self.status
        )
    }
}

impl std::error::Error for ErrorStatus {}

/// What the daemon did with a call it answered with the error `status`: a 4xx turned the call
/// away, anything else means the daemon failed while handling it.
pub fn status_verb(status: u16) -> &'static str {
    if (400..500).contains(&status) {
        "refused"
    } else {
        "failed"
    }
}

/// The HTTP status of the daemon's error answer when `err` is one, as opposed to a failure to
/// reach the daemon or to read its answer.
pub fn error_status(err: &std::io::Error) -> Option<u16> {
    err.get_ref()?
        .downcast_ref::<ErrorStatus>()
        .map(|e| e.status)
}

/// Drops the status line and headers, leaving the body of a 2xx response; any other status is an
/// [`ErrorStatus`] error. A response the daemon did not shape (an axum rejection, say) still
/// parses: anything before the blank line goes.
fn split_body(response: Vec<u8>) -> std::io::Result<Vec<u8>> {
    const SEPARATOR: &[u8] = b"\r\n\r\n";
    let status = response
        .split(|&byte| byte == b'\r')
        .next()
        .and_then(|line| std::str::from_utf8(line).ok())
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the daemon's response has no status line",
            )
        })?;
    if !(200..300).contains(&status) {
        return Err(std::io::Error::other(ErrorStatus { status }));
    }
    Ok(response
        .windows(SEPARATOR.len())
        .position(|window| window == SEPARATOR)
        .map(|at| response[at + SEPARATOR.len()..].to_vec())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_body_is_what_follows_the_headers() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec();
        assert_eq!(super::split_body(response).unwrap(), b"{}".to_vec());
    }

    /// A response with no header terminator is not a response; reading the whole thing as a body
    /// would hand the caller a status line to parse as JSON.
    #[test]
    fn a_truncated_response_yields_nothing() {
        assert!(super::split_body(b"HTTP/1.1 200 OK".to_vec())
            .unwrap()
            .is_empty());
    }

    /// An error answer is told apart from a success by its status, with the body left unread; a
    /// 4xx is a refusal and a 5xx a failure.
    #[test]
    fn an_error_status_is_an_error_carrying_the_status() {
        for (status, said) in [
            (400u16, "refused the call (HTTP 400)"),
            (401, "refused the call (HTTP 401)"),
            (403, "refused the call (HTTP 403)"),
            (500, "failed the call (HTTP 500)"),
        ] {
            let response =
                format!("HTTP/1.1 {status} X\r\nContent-Length: 2\r\n\r\n{{}}").into_bytes();
            let err = super::split_body(response).unwrap_err();
            assert_eq!(super::error_status(&err), Some(status));
            assert!(err.to_string().contains(said), "{err}");
        }
    }

    #[test]
    fn a_response_without_a_status_line_has_no_status() {
        let err = super::split_body(b"garbage".to_vec()).unwrap_err();
        assert_eq!(super::error_status(&err), None);
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
