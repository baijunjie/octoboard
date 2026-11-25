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
/// daemon answered with an error status (see [`error_status`]). The key is added to `path`'s query,
/// which `path` may carry already, its values percent-encoded (`crate::text::percent_encode`).
pub fn exchange(
    mut stream: &TcpStream,
    port: u16,
    key: &str,
    path: &str,
    body: &[u8],
) -> std::io::Result<Vec<u8>> {
    let target = match path.split_once('?') {
        Some((path, query)) => format!("{path}?key={key}&{query}"),
        None => format!("{path}?key={key}"),
    };
    let request = format!(
        "POST {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
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
/// is no answer to parse; a caller that knows what the daemon says in refusing a call reads it from
/// [`error_body`].
#[derive(Debug)]
struct ErrorStatus {
    status: u16,
    body: Vec<u8>,
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

/// The body of the daemon's error answer when `err` is one (see [`error_status`]).
pub fn error_body(err: &std::io::Error) -> Option<&[u8]> {
    err.get_ref()?
        .downcast_ref::<ErrorStatus>()
        .map(|e| e.body.as_slice())
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
    let body = response
        .windows(SEPARATOR.len())
        .position(|window| window == SEPARATOR)
        .map(|at| response[at + SEPARATOR.len()..].to_vec())
        .unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(std::io::Error::other(ErrorStatus { status, body }));
    }
    Ok(body)
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

    /// An error answer is told apart from a success by its status, with its body kept apart; a
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
            assert_eq!(super::error_body(&err), Some(&b"{}"[..]));
            assert!(err.to_string().contains(said), "{err}");
        }
    }

    #[test]
    fn a_response_without_a_status_line_has_no_status() {
        let err = super::split_body(b"garbage".to_vec()).unwrap_err();
        assert_eq!(super::error_status(&err), None);
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    /// What request line an [`super::exchange`] sends for `path`, read from a listener standing in
    /// for the daemon, which answers with an empty `200`.
    fn request_line_for(path: &str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let daemon = std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Read, Write};
            let mut stream = crate::test_support::accept_past_probes(&listener);
            let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
            let mut request_line = String::new();
            reader.read_line(&mut request_line).expect("a request line");
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("a header");
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().expect("a length");
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).expect("the body");
            assert_eq!(body, b"{}");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .expect("an answer");
            request_line
        });
        // A running daemon may probe the port at any point: the probe is not the request.
        assert_eq!(
            crate::test_support::probe_port(port),
            "HTTP/1.1 404 Not Found"
        );
        let answered = super::post(port, "the-key", path, b"{}", crate::test_support::PATIENCE);
        assert!(answered.expect("answered").is_empty());
        daemon.join().expect("the stand-in daemon ran")
    }

    /// The key goes first in the path's query, whether the path has one of its own or not, and the
    /// path's own query follows it untouched.
    #[test]
    fn the_key_is_put_first_in_a_path_that_has_a_query_already() {
        assert_eq!(
            request_line_for("/hook/s"),
            "POST /hook/s?key=the-key HTTP/1.1\r\n"
        );
        assert_eq!(
            request_line_for("/hook/s?event=Pre%20Tool"),
            "POST /hook/s?key=the-key&event=Pre%20Tool HTTP/1.1\r\n"
        );
    }
}
